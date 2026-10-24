//! Lisp that runs around a buffer-modifying or field-aware primitive.
//!
//! A subr body holds the host borrow, so it cannot call Lisp. The C primitives
//! these port do call Lisp in the middle of their work: insdel.c's
//! `prepare_to_modify_buffer` runs `before-change-functions` (and
//! `first-change-hook`) before the text changes and `signal_after_change` runs
//! `after-change-functions` after it; editfns.c's `line-beginning-position`
//! and friends call `constrain-to-field`; `insert-char` with INHERIT and
//! `indent-to` inherit text properties. [`around`] is called by
//! `host::call_function` for every subr, after its arity check and outside the
//! borrow; for the primitives below it does the Lisp part itself and runs the
//! subr body for the rest. A user's redefinition of one of these names is a
//! closure, not a subr, so it never comes here.
//!
//! Each modifying primitive's region is the one its C computes for
//! `prepare_to_modify_buffer`, taken after argument validation: an invalid
//! call runs the body alone, which signals as before and runs no hook.

use crate::builtins;
use crate::host::{call_function, el_nil, el_truthy, with_host, ElispHost, SubrFn};
use fusevm::Value;

type R = Result<Value, String>;

/// The Lisp-side part of NAME, when NAME is one of the primitives this module
/// wraps; `None` for every other subr, which then runs as usual.
pub fn around(name: &str, f: SubrFn, args: &[Value]) -> Option<R> {
    let r = match name {
        "insert" | "--insert-before-markers--" => insert(f, args),
        "insert-char" => insert_char(f, args),
        "indent-to" => indent_to(args),
        "delete-region" | "delete-and-extract-region" => delete_region(f, args),
        "delete-char" => delete_char(f, args),
        "erase-buffer" => erase_buffer(f, args),
        "elisprs--replace-chars-in-place" => replace_chars(f, args),
        "transpose-regions" => transpose_regions(f, args),
        "replace-match" => replace_match(f, args),
        "insert-file-contents" => {
            let pt = point();
            modify(pt, pt, None, || run(f, args))
        }
        "put-text-property"
        | "add-text-properties"
        | "remove-text-properties"
        | "set-text-properties"
        | "add-face-text-property" => text_props(name, f, args),
        "line-beginning-position" => line_position(f, args, true),
        "line-end-position" => line_position(f, args, false),
        "beginning-of-line" => line_motion(f, args, true),
        "end-of-line" => line_motion(f, args, false),
        "set-window-buffer" => set_window_buffer(f, args),
        "princ" | "prin1" | "print" | "terpri" | "write-char" => print_to_buffer(name, f, args),
        "move-to-column" => move_to_column(f, args),
        "call-process" => call_process(f, args),
        "subst-char-in-region" => subst_char(f, args),
        "read" => read_stream(f, args),
        _ => return None,
    };
    Some(r)
}

/// Run the subr body itself.
fn run(f: SubrFn, args: &[Value]) -> R {
    with_host(|h| f(h, args))
}

fn point() -> usize {
    with_host(|h| h.cur_buf_ref().point)
}

/// The value of variable NAME in the current buffer, nil when void.
fn var(h: &mut ElispHost, name: &str) -> Value {
    let sym = h.intern(name);
    h.get_value(&sym).unwrap_or(Value::Undef)
}

/// What a change to the current buffer has Lisp to run for
/// (`prepare_to_modify_buffer_1` / `signal_after_change`).
struct ModState {
    inhibit: bool,
    /// `run_undoable_change`, `verify_interval_modification`, or
    /// `signal_before_change` has something to do.
    prepare: bool,
}

fn mod_state(h: &mut ElispHost) -> ModState {
    let inhibit = el_truthy(&var(h, "inhibit-modification-hooks"));
    let b = h.cur_buf_ref();
    let unmodified = b.mods.save_modiff >= b.mods.modiff;
    let (intervals, overlays) = (b.has_intervals, !b.overlays.is_empty());
    let undo = !matches!(var(h, "buffer-undo-list"), Value::Bool(true));
    let before = !inhibit
        && (el_truthy(&var(h, "before-change-functions"))
            || (unmodified && el_truthy(&var(h, "first-change-hook")))
            || overlays);
    ModState {
        inhibit,
        prepare: undo || intervals || before,
    }
}

/// insdel.c's frame around one change of the text `[beg, end)`.
fn modify(beg: usize, end: usize, buffer: Option<Value>, body: impl FnOnce() -> R) -> R {
    modify_with(beg, end, buffer, None, body)
}

/// `modify`, with the after-change span AFTER `(BEG END OLD-LEN)` given
/// rather than taken from the size change (`subst-char-in-region` reports
/// only the stretch it changed).
fn modify_with(
    beg: usize,
    end: usize,
    buffer: Option<Value>,
    after: Option<(usize, usize, usize)>,
    body: impl FnOnce() -> R,
) -> R {
    change(beg, end, buffer, after, true, body)
}

/// [`modify_with`]; with TEXT_CHANGES false nothing is inserted or deleted
/// and only the after-change half runs, as print.c's `print_finish` calls
/// `signal_after_change` after an `insert_1_both` of no characters (which
/// returns before `prepare_to_modify_buffer`).
fn change(
    beg: usize,
    end: usize,
    buffer: Option<Value>,
    after: Option<(usize, usize, usize)>,
    text_changes: bool,
    body: impl FnOnce() -> R,
) -> R {
    let in_buffer = |f: &mut dyn FnMut(&mut ElispHost) -> ModState| {
        with_host(|h| {
            let saved = h.current;
            if let Some(bi) = buffer.as_ref().and_then(|b| h.buffer_slot(b)) {
                h.current = bi;
            }
            let r = f(h);
            h.current = saved;
            r
        })
    };
    // No text changes: no `prepare_to_modify_buffer` and no `deactivate-mark`.
    let state = if text_changes {
        in_buffer(&mut mod_state)
    } else {
        ModState {
            inhibit: true,
            prepare: false,
        }
    };
    let buf_arg = buffer.clone().unwrap_or(Value::Undef);
    if state.prepare {
        let fname = with_host(|h| h.intern("elisprs--prepare-to-modify"));
        call_function(
            &fname,
            &[
                Value::Int(beg as i64),
                Value::Int(end as i64),
                buf_arg.clone(),
            ],
        )?;
    }
    if !state.inhibit {
        in_buffer(&mut |h| {
            let sym = h.intern("deactivate-mark");
            let _ = h.set_value(&sym, Value::Bool(true));
            ModState {
                inhibit: false,
                prepare: false,
            }
        });
    }
    let size = |h: &mut ElispHost| -> i64 {
        let saved = h.current;
        if let Some(bi) = buffer.as_ref().and_then(|b| h.buffer_slot(b)) {
            h.current = bi;
        }
        let n = h.cur_buf_ref().text.len() as i64;
        h.current = saved;
        n
    };
    let size_before = with_host(size);
    let r = body()?;
    let size_after = with_host(size);
    let run_after = with_host(|h| {
        let saved = h.current;
        if let Some(bi) = buffer.as_ref().and_then(|b| h.buffer_slot(b)) {
            h.current = bi;
        }
        let b = h.cur_buf_ref();
        let (intervals, overlays) = (b.has_intervals, !b.overlays.is_empty());
        let out = !el_truthy(&var(h, "inhibit-modification-hooks"))
            && (!el_nil(&var(h, "after-change-functions")) || intervals || overlays);
        h.current = saved;
        out
    });
    if run_after {
        let (a_beg, a_end, a_len) = after.map_or(
            (
                beg as i64,
                end as i64 + (size_after - size_before),
                (end - beg) as i64,
            ),
            |(b, e, l)| (b as i64, e as i64, l as i64),
        );
        let fname = with_host(|h| h.intern("elisprs--signal-after-change"));
        call_function(
            &fname,
            &[
                Value::Int(a_beg),
                Value::Int(a_end),
                Value::Int(a_len),
                buf_arg,
            ],
        )?;
    }
    Ok(r)
}

/// The number of characters inserting V adds, `None` when V is not a string
/// or character (the body then signals).
fn insert_len(h: &ElispHost, v: &Value) -> Option<usize> {
    if let Some(s) = h.str_text(v) {
        return Some(s.chars().count());
    }
    matches!(v, Value::Int(_)).then_some(1)
}

/// `insert`: each argument is its own insertion (`general_insert_function`),
/// with its own pair of hook calls; an empty one runs none.
fn insert(f: SubrFn, args: &[Value]) -> R {
    for arg in args {
        let one = std::slice::from_ref(arg);
        match with_host(|h| insert_len(h, arg)) {
            Some(n) if n > 0 => {
                let pt = point();
                modify(pt, pt, None, || run(f, one))?;
            }
            _ => {
                run(f, one)?;
            }
        }
    }
    Ok(Value::Undef)
}

/// `insert-char`: COUNT copies of CHARACTER, inheriting the surrounding text
/// properties when INHERIT (as `insert-and-inherit` does), no hooks when COUNT
/// is not positive.
fn insert_char(f: SubrFn, args: &[Value]) -> R {
    let (Some(Value::Int(c)), count) = (args.first(), args.get(1)) else {
        return run(f, args);
    };
    let count = match count {
        None | Some(Value::Undef) | Some(Value::Bool(false)) => 1,
        Some(Value::Int(n)) => *n,
        Some(_) => return run(f, args),
    };
    if count <= 0 || char::from_u32(*c as u32).is_none() {
        return run(f, args);
    }
    let inherit = args.get(2).is_some_and(el_truthy);
    if inherit && with_host(|h| h.cur_buf_ref().has_intervals) {
        let ch = char::from_u32(*c as u32).unwrap_or('\u{fffd}');
        let (fname, text) = with_host(|h| {
            let text: String = std::iter::repeat_n(ch, count as usize).collect();
            (h.intern("insert-and-inherit"), h.new_string(text))
        });
        call_function(&fname, &[text])?;
        return Ok(Value::Undef);
    }
    let pt = point();
    modify(pt, pt, None, || run(f, args))
}

/// `indent-to`: `Findent_to` inserts its tabs, then its spaces, each with
/// `Finsert_char (…, Qt)`.
fn indent_to(args: &[Value]) -> R {
    let (mincol, tabs, spaces) = with_host(|h| builtins::indent_to_plan(h, args))?;
    for (c, n) in [('\t', tabs), (' ', spaces)] {
        if n > 0 {
            let ins = [
                Value::Int(c as i64),
                Value::Int(n as i64),
                Value::Bool(true),
            ];
            insert_char(builtins::insert_char, &ins)?;
        }
    }
    Ok(Value::Int(mincol))
}

/// `delete-region` / `delete-and-extract-region`: `validate_region`, then a
/// change over `[START, END)` unless it is empty.
fn delete_region(f: SubrFn, args: &[Value]) -> R {
    let region = with_host(|h| builtins::validate_region(h, &args[0], &args[1]));
    match region {
        Ok((lo, hi)) if lo < hi => modify(lo, hi, None, || run(f, args)),
        _ => run(f, args),
    }
}

/// `delete-char`: N characters from point; KILLFLAG kills them instead
/// (`kill-forward-chars`, cmds.c).
fn delete_char(f: SubrFn, args: &[Value]) -> R {
    let Some(Value::Int(n)) = args.first() else {
        return run(f, args);
    };
    let n = *n;
    let (pt, begv, zv) = with_host(|h| {
        let b = h.cur_buf_ref();
        (b.point as i64, b.begv as i64, b.zv as i64)
    });
    let pos = pt + n;
    if pos < begv || pos > zv {
        return run(f, args);
    }
    if args.get(1).is_some_and(el_truthy) {
        let fname = with_host(|h| h.intern("kill-forward-chars"));
        return call_function(&fname, &[Value::Int(n)]);
    }
    let (lo, hi) = if n < 0 { (pos, pt) } else { (pt, pos) };
    if lo == hi {
        return run(f, args);
    }
    modify(lo as usize, hi as usize, None, || run(f, args))
}

/// `erase-buffer`: `Fwiden`, then a change over the whole text.
fn erase_buffer(f: SubrFn, args: &[Value]) -> R {
    let len = with_host(|h| {
        h.widen();
        h.cur_buf_ref().text.len()
    });
    if len == 0 {
        return run(f, args);
    }
    modify(1, len + 1, None, || run(f, args))
}

/// `elisprs--replace-chars-in-place`: `modify_text` over `[START, END)` when
/// the replacement has as many characters.
fn replace_chars(f: SubrFn, args: &[Value]) -> R {
    let region = with_host(|h| {
        let (lo, hi) = builtins::validate_region(h, &args[0], &args[1]).ok()?;
        let n = h.str_text(&args[2])?.chars().count();
        (lo < hi && n == hi - lo).then_some((lo, hi))
    });
    match region {
        Some((lo, hi)) => modify(lo, hi, None, || run(f, args)),
        None => run(f, args),
    }
}

/// `transpose-regions`: `modify_text (start1, end2)` over the span from the
/// earlier region's start to the later one's end.
fn transpose_regions(f: SubrFn, args: &[Value]) -> R {
    if args.len() < 4 {
        return run(f, args);
    }
    let span = with_host(|h| {
        let (mut s1, mut e1) = builtins::validate_region(h, &args[0], &args[1]).ok()?;
        let (mut s2, mut e2) = builtins::validate_region(h, &args[2], &args[3]).ok()?;
        if s2 < e1 {
            std::mem::swap(&mut s1, &mut s2);
            std::mem::swap(&mut e1, &mut e2);
        }
        let noop = (s1 == e1 || s2 == e2) && e1 == s2;
        (s2 >= e1 && !noop).then_some((s1, e2))
    });
    match span {
        Some((lo, hi)) => modify(lo, hi, None, || run(f, args)),
        None => run(f, args),
    }
}

/// `replace-match` in the buffer: `replace_range` over the matched SUBEXP.
fn replace_match(f: SubrFn, args: &[Value]) -> R {
    if args.get(3).is_some_and(el_truthy) {
        return run(f, args);
    }
    let span = with_host(|h| {
        let subexp = match args.get(4) {
            None | Some(Value::Undef) | Some(Value::Bool(false)) => 0,
            Some(Value::Int(n)) if *n >= 0 => *n as usize,
            Some(_) => return None,
        };
        let (b, e) = h
            .match_data
            .as_ref()?
            .spans
            .get(subexp)
            .copied()
            .flatten()?;
        let buf = h.cur_buf_ref();
        (buf.begv <= b && b <= e && e <= buf.zv).then_some((b, e))
    });
    match span {
        Some((b, e)) => modify(b, e, None, || run(f, args)),
        None => run(f, args),
    }
}

/// The text-property primitives on a buffer: `modify_text_properties` over
/// `[START, END)` when the call changes something (textprop.c returns early,
/// running no hook, when it would not).
fn text_props(name: &str, f: SubrFn, args: &[Value]) -> R {
    let object_index = match name {
        "put-text-property" => 4,
        "set-text-properties" => 3,
        "add-face-text-property" => 4,
        _ => 3,
    };
    let span = with_host(|h| builtins::text_prop_change_span(h, name, args, object_index));
    match span {
        Some((bi, lo, hi)) => {
            let buffer = with_host(|h| h.buffers[bi].self_obj.clone());
            modify(lo, hi, Some(buffer), || run(f, args))
        }
        None => run(f, args),
    }
}

/// Whether the current buffer can have a `field` property anywhere: without
/// text properties or overlays, `constrain-to-field` cannot move a position.
fn may_have_fields(h: &ElispHost) -> bool {
    let b = h.cur_buf_ref();
    b.has_intervals || !b.overlays.is_empty()
}

/// `constrain-to-field` of NEW-POS against point, as editfns.c's line
/// functions call it (ONLY-IN-LINE t, no capture property).
fn constrain(new_pos: Value, escape: bool) -> R {
    let (fname, pt) = with_host(|h| (h.intern("constrain-to-field"), h.cur_buf_ref().point));
    call_function(
        &fname,
        &[
            new_pos,
            Value::Int(pt as i64),
            Value::Bool(escape),
            Value::Bool(true),
            Value::Undef,
        ],
    )
}

/// ESCAPE-FROM-EDGE for `line-beginning-position`: `bol`'s count is non-zero,
/// i.e. N is given and is not 1.
fn bol_escapes(args: &[Value]) -> bool {
    match args.first() {
        None | Some(Value::Undef) | Some(Value::Bool(false)) => false,
        Some(Value::Int(n)) => *n != 1,
        Some(_) => true,
    }
}

/// `line-beginning-position` / `line-end-position`: the `pos-bol` / `pos-eol`
/// position constrained to point's field.
fn line_position(f: SubrFn, args: &[Value], bol: bool) -> R {
    let pos = run(f, args)?;
    if !with_host(|h| may_have_fields(h)) {
        return Ok(pos);
    }
    constrain(pos, bol && bol_escapes(args))
}

/// `beginning-of-line` / `end-of-line` (cmds.c): N nil is 1, anything else
/// must be a fixnum; point goes to the field-constrained line position.
fn line_motion(f: SubrFn, args: &[Value], bol: bool) -> R {
    let (orig, may) = with_host(|h| (h.cur_buf_ref().point, may_have_fields(h)));
    run(f, args)?;
    if !may {
        return Ok(Value::Undef);
    }
    let target = with_host(|h| {
        let p = h.cur_buf_ref().point;
        h.cur_buf().point = orig;
        p
    });
    let pos = constrain(Value::Int(target as i64), bol && bol_escapes(args))?;
    with_host(|h| {
        if let Value::Int(p) = pos {
            let b = h.cur_buf();
            b.point = (p as usize).clamp(b.begv, b.zv);
        }
    });
    Ok(Value::Undef)
}

/// `set-window-buffer`: before WINDOW shows a different buffer, window.c
/// calls `record-window-buffer` (unless the window is strongly dedicated,
/// which the body then refuses).
fn set_window_buffer(f: SubrFn, args: &[Value]) -> R {
    let changes = with_host(|h| {
        let w = match args.first().map(|v| h.obj(v)) {
            None | Some(None) => Some(h.selected_window),
            Some(Some(crate::host::Obj::Window(w))) => Some(*w),
            _ => None,
        }?;
        let wd = &h.windows[w];
        let current = wd.buffer?;
        let target = h.resolve_buffer(args.get(1)?)?;
        (current != target && !matches!(wd.dedicated, Value::Bool(true)))
            .then(|| wd.self_obj.clone())
    });
    if let Some(window) = changes {
        let fname = with_host(|h| h.intern("record-window-buffer"));
        call_function(&fname, &[window])?;
    }
    run(f, args)
}

/// The printing primitives with a buffer or marker destination: print.c
/// collects the text and inserts it once (`PRINTFINISH`), one change at the
/// destination's insertion position in its buffer; printing nothing runs only
/// the after-change half.
fn print_to_buffer(name: &str, f: SubrFn, args: &[Value]) -> R {
    let pcf = usize::from(name != "terpri");
    let target = with_host(|h| match builtins::print_dest(h, args.get(pcf)) {
        Ok(builtins::PrintDest::Buffer(bi)) => Some((bi, h.buffers[bi].point)),
        Ok(builtins::PrintDest::Marker(m)) => match h.obj(&m) {
            Some(crate::host::Obj::Marker(md)) => {
                let md = md.borrow();
                md.buffer.map(|bi| (bi, md.pos))
            }
            _ => None,
        },
        _ => None,
    });
    match target {
        Some((bi, pos)) => {
            // `print_finish` inserts what was printed and then always runs
            // `signal_after_change`; an empty output inserts nothing, so it
            // runs no before-change half.
            let (buffer, inserts) = with_host(|h| {
                let b = &h.buffers[bi];
                let inserts = match name {
                    "terpri" if args.get(1).is_some_and(el_truthy) => {
                        // Fterpri's ENSURE: `Fbolp` at the print position.
                        !(pos <= b.begv || b.text.get(pos - 2) == Some(&'\n'))
                    }
                    "princ" => match args.first() {
                        Some(v) => h
                            .str_text(v)
                            .map(str::to_string)
                            .or_else(|| h.sym_name(v))
                            .is_none_or(|s| !s.is_empty()),
                        None => true,
                    },
                    _ => true,
                };
                (b.self_obj.clone(), inserts)
            });
            change(pos, pos, Some(buffer), None, inserts, || run(f, args))
        }
        None => run(f, args),
    }
}

/// `move-to-column` with FORCE (indent.c): a tab straddling COLUMN becomes
/// spaces — `insert-char` of spaces before it, the tab deleted, `indent-to`
/// its old end column — and with FORCE t a short line is padded with
/// `indent-to`. Each step is a change of its own.
fn move_to_column(f: SubrFn, args: &[Value]) -> R {
    let (Some(Value::Int(goal)), Some(force)) = (args.first(), args.get(1)) else {
        return run(f, args);
    };
    if *goal < 0 || el_nil(force) {
        return run(f, args);
    }
    let goal = *goal;
    let force_t = matches!(force, Value::Bool(true));
    let (pos, mut col, prev_pos, prev_col) = with_host(|h| {
        let zv = h.cur_buf_ref().zv;
        let r = builtins::scan_for_column(h, zv, goal);
        h.cur_buf().point = r.0;
        r
    });
    if col > goal {
        let tab = with_host(|h| h.cur_buf_ref().text[prev_pos - 1] == '\t');
        if tab && prev_col < goal && prev_pos < pos {
            with_host(|h| h.cur_buf().point = prev_pos);
            let spaces = [
                Value::Int(' ' as i64),
                Value::Int(goal - prev_col),
                Value::Bool(true),
            ];
            insert_char(builtins::insert_char, &spaces)?;
            let at = point();
            let del = [Value::Int(at as i64), Value::Int(at as i64 + 1)];
            delete_region(builtins::delete_region, &del)?;
            indent_to(&[Value::Int(col)])?;
            with_host(|h| h.cur_buf().point = at);
            col = goal;
        }
    }
    if col < goal && force_t {
        col = goal;
        indent_to(&[Value::Int(col)])?;
    }
    Ok(Value::Int(col))
}

/// `call-process` inserting its output: one change at point of the buffer
/// the output goes to (DESTINATION t is the current buffer).
fn call_process(f: SubrFn, args: &[Value]) -> R {
    let inserts = args
        .get(2)
        .is_some_and(|d| el_truthy(d) && !matches!(d, Value::Int(0)));
    if !inserts {
        return run(f, args);
    }
    let pt = point();
    modify(pt, pt, None, || run(f, args))
}

/// `subst-char-in-region`: `modify_text` from the first changed character to
/// END before, `signal_after_change` over the changed stretch after. With
/// NOUNDO the C sets `buffer-undo-list` to t for the duration, so nothing is
/// recorded (the first-change record and `undo-auto--undoable-change` neither).
fn subst_char(f: SubrFn, args: &[Value]) -> R {
    let Some((_, hi, first, last)) = with_host(|h| builtins::subst_char_span(h, args)) else {
        return run(f, args);
    };
    let noundo = args.get(4).is_some_and(el_truthy);
    let saved = noundo.then(|| {
        with_host(|h| {
            let sym = h.intern("buffer-undo-list");
            let old = h.get_value(&sym).unwrap_or(Value::Undef);
            let _ = h.set_value(&sym, Value::Bool(true));
            old
        })
    });
    let r = modify_with(first, hi, None, Some((first, last, last - first)), || {
        run(f, args)
    });
    if let Some(old) = saved {
        with_host(|h| {
            let sym = h.intern("buffer-undo-list");
            let _ = h.set_value(&sym, old);
        });
    }
    r
}

/// `read` from a function: `crate::lread_fn` drives lread.c's `read0`
/// through the function's `readchar` / `unreadchar` calls. A nil STREAM is
/// `standard-input`, as in `Fread`.
fn read_stream(f: SubrFn, args: &[Value]) -> R {
    let stream = match args.first() {
        Some(s) if !el_nil(s) => s.clone(),
        _ => with_host(|h| {
            let sym = h.intern("standard-input");
            h.get_value(&sym).unwrap_or(Value::Bool(true))
        }),
    };
    let is_function = with_host(|h| {
        !el_nil(&stream)
            && !matches!(stream, Value::Bool(true))
            && !h.is_string(&stream)
            && !matches!(
                h.obj(&stream),
                Some(crate::host::Obj::Buffer(_)) | Some(crate::host::Obj::Marker(_))
            )
            && h.sym_name(&stream).as_deref() != Some("read-char")
    });
    if !is_function {
        return run(f, args);
    }
    crate::lread_fn::read_from_function(&stream)
}
