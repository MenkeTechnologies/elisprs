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

/// What a modification must run, `None` when `inhibit-modification-hooks`
/// suppresses all of it: whether `signal_before_change` has anything to run
/// (`before-change-functions`, or `first-change-hook` in an unmodified
/// buffer). The after side is decided after the change, from the hook's value
/// then.
fn hook_state(h: &mut ElispHost) -> Option<bool> {
    if el_truthy(&var(h, "inhibit-modification-hooks")) {
        return None;
    }
    let unmodified = {
        let m = &h.cur_buf_ref().mods;
        m.save_modiff >= m.modiff
    };
    let before = el_truthy(&var(h, "before-change-functions"))
        || (unmodified && el_truthy(&var(h, "first-change-hook")));
    Some(before)
}

/// insdel.c's frame around one change of the text `[beg, end)`:
/// `signal_before_change`, `deactivate-mark` set to t, the change itself
/// (BODY), then `signal_after_change` with the changed span's new end and old
/// length. BUFFER, when given, is the buffer the change happens in (a text
/// property change can name another buffer); the hooks run there.
fn modify(beg: usize, end: usize, buffer: Option<Value>, body: impl FnOnce() -> R) -> R {
    let in_buffer = |f: &mut dyn FnMut(&mut ElispHost) -> Option<bool>| {
        with_host(|h| match &buffer {
            Some(b) => {
                let saved = h.current;
                if let Some(bi) = h.buffer_slot(b) {
                    h.current = bi;
                }
                let r = f(h);
                h.current = saved;
                r
            }
            None => f(h),
        })
    };
    let Some(before) = in_buffer(&mut hook_state) else {
        return body();
    };
    let buf_arg = buffer.clone().unwrap_or(Value::Undef);
    if before {
        let fname = with_host(|h| h.intern("elisprs--signal-before-change"));
        call_function(
            &fname,
            &[
                Value::Int(beg as i64),
                Value::Int(end as i64),
                buf_arg.clone(),
            ],
        )?;
    }
    in_buffer(&mut |h| {
        let sym = h.intern("deactivate-mark");
        let _ = h.set_value(&sym, Value::Bool(true));
        None
    });
    let size = |h: &mut ElispHost| h.cur_buf_ref().text.len() as i64;
    let size_before = with_host(|h| match &buffer {
        Some(b) => h
            .buffer_slot(b)
            .map(|bi| h.buffers[bi].text.len() as i64)
            .unwrap_or(0),
        None => size(h),
    });
    let r = body()?;
    let (size_after, after) = with_host(|h| {
        let saved = h.current;
        if let Some(bi) = buffer.as_ref().and_then(|b| h.buffer_slot(b)) {
            h.current = bi;
        }
        let out = (size(h), !el_nil(&var(h, "after-change-functions")));
        h.current = saved;
        out
    });
    if after {
        let new_end = end as i64 + (size_after - size_before);
        let fname = with_host(|h| h.intern("elisprs--signal-after-change"));
        call_function(
            &fname,
            &[
                Value::Int(beg as i64),
                Value::Int(new_end),
                Value::Int((end - beg) as i64),
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
