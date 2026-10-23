//! The window and frame objects of a batch Emacs (window.c, frame.c).
//!
//! `emacs --batch` has one terminal frame, `F1`, holding the root window
//! (number 1, showing `*scratch*`, 80x24 with a mode line) and the minibuffer
//! window (number 2, showing ` *Minibuf-0*`, 80x1). Nothing is ever drawn,
//! but the objects, their buffers, points and starts, the selected window and
//! frame, window configurations and the frame's buffer list behave as they do
//! there; this module is the C side of them. Splitting and deleting windows is
//! not modelled: the window tree is always that one leaf plus the minibuffer.
//!
//! A window's point and start are marker-like positions adjusted with every
//! edit of the buffer it shows (`ElispHost::adjust_for_insert` / `_delete`).

use crate::host::{el_nil, el_truthy, ElispHost, Obj};
use fusevm::Value;

type R = Result<Value, String>;

/// One window (window.c `struct window`, the parts batch Emacs uses).
#[derive(Clone, Debug)]
pub struct WindowData {
    /// The window object itself.
    pub self_obj: Value,
    /// `sequence_number`, shown in the printed form.
    pub number: i64,
    pub frame: usize,
    /// The buffer slot shown, `None` once the window is deleted.
    pub buffer: Option<usize>,
    /// `pointm`: the window's own point, used when it is not selected.
    pub pointm: usize,
    /// `old_pointm`.
    pub old_pointm: usize,
    /// `start`.
    pub start: usize,
    pub mini: bool,
    pub dedicated: Value,
    pub parameters: Value,
    pub prev_buffers: Value,
    pub next_buffers: Value,
    pub hscroll: i64,
    pub total_cols: i64,
    pub total_lines: i64,
    pub top_line: i64,
    /// `use_time`, bumped by every `select-window` that records.
    pub use_time: i64,
}

/// One frame.
#[derive(Clone, Debug)]
pub struct FrameData {
    pub self_obj: Value,
    pub name: String,
    pub root: usize,
    pub mini: usize,
    pub selected: usize,
    /// `f->buffer_list`: the buffers selected in this frame, most recent first.
    pub buffer_list: Vec<usize>,
    pub parameters: Value,
}

/// A saved window configuration (`current-window-configuration`).
#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub frame: usize,
    /// Per window: (buffer, point, start).
    pub windows: Vec<(Option<usize>, usize, usize)>,
    pub selected: usize,
    pub current_buffer: usize,
}

/// The initial frame and its two windows, made with the host.
pub fn init(h: &mut ElispHost, scratch: usize, minibuf: usize) {
    let w1 = h.alloc(Obj::Window(0));
    let w2 = h.alloc(Obj::Window(1));
    let f1 = h.alloc(Obj::Frame(0));
    let window = |obj: Value, number, buffer, mini, total_lines, top_line| WindowData {
        self_obj: obj,
        number,
        frame: 0,
        buffer: Some(buffer),
        pointm: 1,
        old_pointm: 1,
        start: 1,
        mini,
        dedicated: Value::Undef,
        parameters: Value::Undef,
        prev_buffers: Value::Undef,
        next_buffers: Value::Undef,
        hscroll: 0,
        total_cols: 80,
        total_lines,
        top_line,
        use_time: 0,
    };
    h.windows = vec![
        window(w1, 1, scratch, false, 24, 0),
        window(w2, 2, minibuf, true, 1, 24),
    ];
    h.frames = vec![FrameData {
        self_obj: f1,
        name: "F1".to_string(),
        root: 0,
        mini: 1,
        selected: 0,
        buffer_list: vec![scratch],
        parameters: Value::Undef,
    }];
    h.selected_window = 0;
    h.window_count = 2;
}

fn is_nil(v: &Value) -> bool {
    el_nil(v)
}

/// `decode_live_window`: nil is the selected window; anything else must be a
/// live window.
fn live_window(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.selected_window),
        Some(v) if is_nil(v) => Ok(h.selected_window),
        Some(v) => match h.obj(v) {
            Some(Obj::Window(i)) if h.windows[*i].buffer.is_some() => Ok(*i),
            _ => Err(h.signal_wrong_type("window-live-p", v)),
        },
    }
}

/// `decode_valid_window`: as `live_window`, but a deleted window passes too.
fn valid_window(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.selected_window),
        Some(v) if is_nil(v) => Ok(h.selected_window),
        Some(v) => match h.obj(v) {
            Some(Obj::Window(i)) => Ok(*i),
            _ => Err(h.signal_wrong_type("window-valid-p", v)),
        },
    }
}

/// `decode_live_frame`.
fn live_frame(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.selected_frame),
        Some(v) if is_nil(v) => Ok(h.selected_frame),
        Some(v) => match h.obj(v) {
            Some(Obj::Frame(i)) => Ok(*i),
            _ => Err(h.signal_wrong_type("frame-live-p", v)),
        },
    }
}

fn window_obj(h: &ElispHost, w: usize) -> Value {
    h.windows[w].self_obj.clone()
}

fn buffer_obj(h: &ElispHost, b: usize) -> Value {
    h.buffers[b].self_obj.clone()
}

/// The printed form of window W (`print_vectorlike`).
pub fn print_window(h: &ElispHost, w: usize) -> String {
    let wd = &h.windows[w];
    match wd.buffer.and_then(|b| h.buffers[b].name.as_ref()) {
        Some(name) => format!("#<window {} on {name}>", wd.number),
        None => format!("#<window {}>", wd.number),
    }
}

/// The printed form of frame F. Emacs shows the frame's address; this shows
/// its heap handle in the same place.
pub fn print_frame(h: &ElispHost, f: usize) -> String {
    let id = match h.frames[f].self_obj {
        Value::Obj(id) => id as u64,
        _ => 0,
    };
    format!(
        "#<frame {} 0x{:x}>",
        h.frames[f].name,
        0xb00000000u64 + id * 0x10
    )
}

/// The window's point: the buffer's point for the selected window
/// (`Fwindow_point`), its own `pointm` otherwise.
fn window_point_of(h: &ElispHost, w: usize) -> usize {
    let wd = &h.windows[w];
    match wd.buffer {
        Some(b) if w == h.selected_window => h.buffers[b].point,
        _ => wd.pointm,
    }
}

/// `record_buffer`: BUFFER moves to the front of the buffer list and of the
/// selected frame's buffer list.
pub fn record_buffer(h: &mut ElispHost, b: usize) {
    h.buffer_order_seq += 1;
    h.buffers[b].list_order = -h.buffer_order_seq;
    let f = h.selected_frame;
    let list = &mut h.frames[f].buffer_list;
    list.retain(|&x| x != b);
    list.insert(0, b);
}

/// Whether buffer B is shown in a live window.
fn buffer_visible(h: &ElispHost, b: usize) -> bool {
    h.windows.iter().any(|w| w.buffer == Some(b))
}

/// buffer.c `Fother_buffer`'s choice: the most recent live, unhidden buffer
/// other than EXCLUDE (the frame's buffer list first, then all buffers),
/// preferring one no window shows unless VISIBLE_OK; `None` when there is no
/// such buffer at all (the caller then makes `*scratch*`).
pub fn other_buffer(h: &ElispHost, exclude: Option<usize>, visible_ok: bool) -> Option<usize> {
    let candidate = |b: usize| {
        Some(b) != exclude
            && h.buffers[b]
                .name
                .as_ref()
                .is_some_and(|n| !n.starts_with(' '))
    };
    let mut notsogood = None;
    let frame_list = h.frames[h.selected_frame].buffer_list.clone();
    for b in frame_list.into_iter().chain(h.buffer_order()) {
        if candidate(b) {
            if visible_ok || !buffer_visible(h, b) {
                return Some(b);
            }
            notsogood.get_or_insert(b);
        }
    }
    notsogood
}

/// `replace_buffer_in_windows`: every window showing the killed buffer B
/// switches to another buffer (window.el's `replace-buffer-in-windows`
/// ends in `switch-to-prev-buffer`, which takes `other-buffer` here: windows
/// keep no previous buffers).
pub fn replace_buffer_in_windows(h: &mut ElispHost, b: usize, replacement: usize) {
    for w in 0..h.windows.len() {
        if h.windows[w].buffer == Some(b) && !h.windows[w].mini {
            h.windows[w].dedicated = Value::Undef;
            set_window_buffer_internal(h, w, replacement);
        }
    }
    let f = h.selected_frame;
    h.frames[f].buffer_list.retain(|&x| x != b);
}

/// `window_discard_buffer_from_window`: B leaves W's previous and next
/// buffer lists.
fn window_discard_buffer(h: &mut ElispHost, w: usize, b: usize) {
    for which in [0, 1] {
        let list = if which == 0 {
            h.windows[w].prev_buffers.clone()
        } else {
            h.windows[w].next_buffers.clone()
        };
        let items = h.list_vec(&list).unwrap_or_default();
        let kept: Vec<Value> = items
            .into_iter()
            .filter(|e| {
                let head = match h.obj(e) {
                    Some(Obj::Cons(car, _)) => car.clone(),
                    _ => e.clone(),
                };
                !matches!(h.obj(&head), Some(Obj::Buffer(x)) if *x == b)
            })
            .collect();
        let new = h.list_from(kept);
        if which == 0 {
            h.windows[w].prev_buffers = new;
        } else {
            h.windows[w].next_buffers = new;
        }
    }
}

/// `unshow_buffer`: the buffer W stops showing remembers W's start, and its
/// point becomes W's point unless the selected window shows it.
fn unshow_buffer(h: &mut ElispHost, w: usize) {
    let Some(old) = h.windows[w].buffer else {
        return;
    };
    h.last_window_start.insert(old, h.windows[w].start);
    let sel_buf = h.windows[h.selected_window].buffer;
    if sel_buf != Some(old) {
        let pm = h.windows[w].pointm;
        let buf = &mut h.buffers[old];
        buf.point = pm.clamp(buf.begv, buf.zv);
    }
}

/// `set_window_buffer`: W shows buffer B from B's point, starting where B's
/// last window started.
fn set_window_buffer_internal(h: &mut ElispHost, w: usize, b: usize) {
    window_discard_buffer(h, w, b);
    let pt = h.buffers[b].point;
    let start = h.last_window_start.get(&b).copied().unwrap_or(1);
    let (begv, zv) = (h.buffers[b].begv, h.buffers[b].zv);
    let wd = &mut h.windows[w];
    wd.buffer = Some(b);
    wd.pointm = pt;
    wd.old_pointm = pt;
    wd.start = start.clamp(begv, zv);
    wd.hscroll = 0;
}

/// `select_window`: the old selected window keeps its buffer's point, W
/// becomes selected, its buffer current with point at W's point; unless
/// NORECORD, W's use time advances and its buffer is recorded.
fn select_window_internal(h: &mut ElispHost, w: usize, norecord: bool) {
    let old = h.selected_window;
    if let Some(b) = h.windows[old].buffer {
        h.windows[old].pointm = h.buffers[b].point;
    }
    h.selected_window = w;
    let f = h.windows[w].frame;
    h.frames[f].selected = w;
    if let Some(b) = h.windows[w].buffer {
        h.current = b;
        let pm = h.windows[w].pointm;
        let buf = &mut h.buffers[b];
        buf.point = pm.clamp(buf.begv, buf.zv);
        if !norecord {
            h.window_select_count += 1;
            h.windows[w].use_time = h.window_select_count;
            record_buffer(h, b);
        }
    }
}

fn arg(a: &[Value], i: usize) -> Option<&Value> {
    a.get(i)
}

fn int_arg(h: &mut ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => match h.obj(v) {
            Some(Obj::Marker(m)) => Ok(m.borrow().pos as i64),
            _ => Err(h.signal_wrong_type("integer-or-marker-p", v)),
        },
    }
}

fn selected_window(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(window_obj(h, h.selected_window))
}
fn minibuffer_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    Ok(window_obj(h, h.frames[f].mini))
}
fn frame_root_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = frame_or_window_frame(h, arg(a, 0))?;
    Ok(window_obj(h, h.frames[f].root))
}
fn frame_selected_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = frame_or_window_frame(h, arg(a, 0))?;
    Ok(window_obj(h, h.frames[f].selected))
}
/// FRAME-OR-WINDOW: a frame, a window's frame, or nil for the selected frame.
fn frame_or_window_frame(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v.map(|v| h.obj(v)) {
        Some(Some(Obj::Window(w))) => Ok(h.windows[*w].frame),
        _ => live_frame(h, v),
    }
}
fn window_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(h.windows[w]
        .buffer
        .map_or(Value::Undef, |b| buffer_obj(h, b)))
}
fn window_point(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(window_point_of(h, w) as i64))
}
fn window_old_point(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].old_pointm as i64))
}
fn window_start(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].start as i64))
}
/// `window-end`: batch Emacs never displays, so `window_end_pos` stays 0 and
/// the end is the buffer's `Z`, with or without UPDATE.
fn window_end(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let b = h.windows[w].buffer.expect("live window");
    Ok(Value::Int(h.buffers[b].text.len() as i64 + 1))
}
/// `set-window-point`: the selected window moves its buffer's point;
/// any other window's own point is set, clipped to its buffer.
fn set_window_point(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let pos = int_arg(h, &a[1])?;
    let b = h.windows[w].buffer.expect("live window");
    let buf = &h.buffers[b];
    let clipped = (pos.max(buf.begv as i64) as usize).min(buf.zv);
    // The selected window's point is its buffer's (`Fgoto_char` there,
    // whether or not that buffer is current).
    if w == h.selected_window {
        h.buffers[b].point = clipped;
    } else {
        h.windows[w].pointm = clipped;
    }
    Ok(Value::Int(pos))
}
fn set_window_start(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let pos = int_arg(h, &a[1])?;
    let b = h.windows[w].buffer.expect("live window");
    let buf = &h.buffers[b];
    h.windows[w].start = (pos.max(buf.begv as i64) as usize).min(buf.zv);
    Ok(Value::Int(pos))
}
/// `set-window-buffer` — window.c. A window strongly dedicated (t) to another
/// buffer refuses; a weakly dedicated one loses its dedication. Its
/// `record-window-buffer` call is `crate::modhooks`'s.
fn set_window_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let b = match h.obj(&a[1]) {
        Some(Obj::Buffer(b)) => *b,
        _ => match h.str_text(&a[1]).and_then(|n| h.find_buffer_by_name(n)) {
            Some(b) => b,
            None if h.is_string(&a[1]) => return Err(h.signal_wrong_type("bufferp", &Value::Undef)),
            None => return Err(h.signal_wrong_type("bufferp", &a[1])),
        },
    };
    if h.buffers[b].name.is_none() {
        return Err("error: Attempt to display deleted buffer".to_string());
    }
    let current = h.windows[w].buffer;
    if current != Some(b) {
        if matches!(h.windows[w].dedicated, Value::Bool(true)) {
            let name = current
                .and_then(|c| h.buffers[c].name.clone())
                .unwrap_or_default();
            return Err(format!(
                "error: Window is dedicated to \u{2018}{name}\u{2019}"
            ));
        }
        h.windows[w].dedicated = Value::Undef;
    }
    unshow_buffer(h, w);
    set_window_buffer_internal(h, w, b);
    Ok(Value::Undef)
}
fn select_window(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, Some(&a[0]))?;
    let norecord = arg(a, 1).is_some_and(el_truthy);
    select_window_internal(h, w, norecord);
    Ok(window_obj(h, w))
}
fn set_frame_selected_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, Some(&a[0]))?;
    let w = live_window(h, Some(&a[1]))?;
    if f == h.selected_frame {
        let norecord = arg(a, 2).is_some_and(el_truthy);
        select_window_internal(h, w, norecord);
    } else {
        h.frames[f].selected = w;
    }
    Ok(window_obj(h, w))
}
fn windowp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Window(_)))))
}
fn window_live_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(h.obj(&a[0]), Some(Obj::Window(w)) if h.windows[*w].buffer.is_some()),
    ))
}
fn window_valid_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(h.obj(&a[0]), Some(Obj::Window(w)) if h.windows[*w].buffer.is_some()),
    ))
}
fn window_frame(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(h.frames[h.windows[w].frame].self_obj.clone())
}
fn window_minibuffer_p(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(nil_or(h.windows[w].mini))
}
/// `window-list`: the frame's live windows in cyclic order from WINDOW (the
/// frame's selected window by default); the minibuffer window only when
/// MINIBUF is t or it is active (never, in batch).
fn window_list(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let with_mini = arg(a, 1).is_some_and(|v| matches!(v, Value::Bool(true)));
    let mut items = vec![window_obj(h, h.frames[f].root)];
    if with_mini {
        items.push(window_obj(h, h.frames[f].mini));
    }
    Ok(h.list_from(items))
}
/// `window-list-1`: WINDOW first, then the others in cyclic order.
fn window_list_1(h: &mut ElispHost, a: &[Value]) -> R {
    let start = valid_window(h, arg(a, 0))?;
    let with_mini = arg(a, 1).is_some_and(|v| matches!(v, Value::Bool(true)));
    let f = h.windows[start].frame;
    let (root, mini) = (h.frames[f].root, h.frames[f].mini);
    let mut order = vec![root];
    if with_mini || start == mini {
        order.push(mini);
    }
    let pos = order.iter().position(|&w| w == start).unwrap_or(0);
    order.rotate_left(pos);
    let items = order.into_iter().map(|w| window_obj(h, w)).collect();
    Ok(h.list_from(items))
}
/// `next-window` / `previous-window`: with one ordinary window the cycle is
/// that window (and the minibuffer window when MINIBUF is t).
fn next_window(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let with_mini = arg(a, 1).is_some_and(|v| matches!(v, Value::Bool(true)));
    let f = h.windows[w].frame;
    let (root, mini) = (h.frames[f].root, h.frames[f].mini);
    let next = if with_mini && w == root { mini } else { root };
    Ok(window_obj(h, next))
}
/// `get-buffer-window`: the first live window showing BUFFER-OR-NAME, the
/// selected window preferred; nil when none (or the buffer is not live).
fn get_buffer_window(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(b) = arg(a, 0).map_or(Some(h.current), |v| {
        if is_nil(v) {
            Some(h.current)
        } else {
            h.resolve_buffer(v)
        }
    }) else {
        return Ok(Value::Undef);
    };
    let sel = h.selected_window;
    let found = std::iter::once(sel)
        .chain(0..h.windows.len())
        .find(|&w| h.windows[w].buffer == Some(b) && !h.windows[w].mini);
    Ok(found.map_or(Value::Undef, |w| window_obj(h, w)))
}
fn window_dedicated_p(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(h.windows[w].dedicated.clone())
}
fn set_window_dedicated_p(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    h.windows[w].dedicated = a[1].clone();
    Ok(a[1].clone())
}
fn window_parameters(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let items = h.list_vec(&h.windows[w].parameters).unwrap_or_default();
    let copies: Vec<Value> = items
        .into_iter()
        .map(|cell| match h.obj(&cell) {
            Some(Obj::Cons(k, v)) => {
                let (k, v) = (k.clone(), v.clone());
                h.cons(k, v)
            }
            _ => cell,
        })
        .collect();
    Ok(h.list_from(copies))
}
fn window_parameter(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let params = h.windows[w].parameters.clone();
    let cell = assq(h, &a[1], &params);
    Ok(match h.obj(&cell) {
        Some(Obj::Cons(_, v)) => v.clone(),
        _ => Value::Undef,
    })
}
fn set_window_parameter(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let params = h.windows[w].parameters.clone();
    let cell = assq(h, &a[1], &params);
    if let Value::Obj(id) = cell {
        if let Some(Obj::Cons(_, v)) = h.arena.get_mut(id as usize) {
            *v = a[2].clone();
            return Ok(a[2].clone());
        }
    }
    let pair = h.cons(a[1].clone(), a[2].clone());
    h.windows[w].parameters = h.cons(pair, params);
    Ok(a[2].clone())
}
fn window_prev_buffers(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(h.windows[w].prev_buffers.clone())
}
fn window_next_buffers(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(h.windows[w].next_buffers.clone())
}
fn set_window_prev_buffers(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    h.windows[w].prev_buffers = a[1].clone();
    Ok(a[1].clone())
}
fn set_window_next_buffers(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    h.windows[w].next_buffers = a[1].clone();
    Ok(a[1].clone())
}
fn window_hscroll(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].hscroll))
}
fn set_window_hscroll(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let n = match &a[1] {
        Value::Int(n) => *n,
        v => return Err(h.signal_wrong_type("fixnump", v)),
    };
    h.windows[w].hscroll = n.max(0);
    Ok(Value::Int(n.max(0)))
}
fn window_total_width(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].total_cols))
}
fn window_total_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].total_lines))
}
/// The body excludes the mode line (ordinary windows) — batch Emacs has no
/// fringes, margins or scroll bars.
fn window_body_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let wd = &h.windows[w];
    Ok(Value::Int(wd.total_lines - i64::from(!wd.mini)))
}
fn window_body_width(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].total_cols))
}
fn window_left_column(h: &mut ElispHost, a: &[Value]) -> R {
    valid_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
fn window_top_line(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].top_line))
}
/// The window tree accessors: the root window is a leaf with no parent or
/// siblings; the minibuffer window has none either.
fn window_no_relative(h: &mut ElispHost, a: &[Value]) -> R {
    valid_window(h, arg(a, 0))?;
    Ok(Value::Undef)
}
fn window_use_time(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].use_time))
}
fn frame_first_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = frame_or_window_frame(h, arg(a, 0))?;
    Ok(window_obj(h, h.frames[f].root))
}
fn selected_frame(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.frames[h.selected_frame].self_obj.clone())
}
fn frame_list(h: &mut ElispHost, _a: &[Value]) -> R {
    let items = h.frames.iter().map(|f| f.self_obj.clone()).collect();
    Ok(h.list_from(items))
}
/// `framep`: t for a terminal frame, which every frame here is.
fn framep(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Frame(_)))))
}
fn frame_live_p(h: &mut ElispHost, a: &[Value]) -> R {
    framep(h, a)
}
/// `(other-buffer &optional BUFFER VISIBLE-OK FRAME)` — buffer.c.
fn other_buffer_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let exclude = arg(a, 0).and_then(|v| match h.obj(v) {
        Some(Obj::Buffer(b)) => Some(*b),
        _ => None,
    });
    let visible_ok = arg(a, 1).is_some_and(el_truthy);
    let b = match other_buffer(h, exclude, visible_ok) {
        Some(b) => b,
        None => {
            let handle = h.get_buffer_create("*scratch*");
            h.resolve_buffer(&handle).expect("fresh *scratch*")
        }
    };
    Ok(buffer_obj(h, b))
}
/// `(bury-buffer-internal BUFFER)` — buffer.c: BUFFER goes to the end of the
/// buffer list and of the frame's.
fn bury_buffer_internal(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(b) = h.resolve_buffer(&a[0]) else {
        return Err(h.signal_wrong_type("bufferp", &a[0]));
    };
    h.buffer_order_seq += 1;
    h.buffers[b].list_order = h.buffer_order_seq + (1 << 40);
    let f = h.selected_frame;
    let list = &mut h.frames[f].buffer_list;
    if list.contains(&b) {
        list.retain(|&x| x != b);
        list.push(b);
    }
    Ok(Value::Undef)
}
/// `current-window-configuration`: every window's buffer, point and start,
/// the selected window and the current buffer.
fn current_window_configuration(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let sel = h.selected_window;
    let windows = (0..h.windows.len())
        .map(|w| {
            let wd = &h.windows[w];
            (wd.buffer, window_point_of(h, w), wd.start)
        })
        .collect();
    h.window_configs.push(WindowConfig {
        frame: f,
        windows,
        selected: sel,
        current_buffer: h.current,
    });
    let id = h.window_configs.len() - 1;
    Ok(h.alloc(Obj::WindowConfig(id)))
}
/// `set-window-configuration`: windows get their buffers, points and starts
/// back (a buffer killed since is not restored), the selected window is
/// selected again, and the buffer current at save time is current again
/// when it is live.
fn set_window_configuration(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(Obj::WindowConfig(id)) = h.obj(&a[0]) else {
        return Err(h.signal_wrong_type("window-configuration-p", &a[0]));
    };
    let config = h.window_configs[*id].clone();
    for (w, (buffer, point, start)) in config.windows.iter().enumerate() {
        if let Some(b) = buffer.filter(|&b| h.buffers[b].name.is_some()) {
            let wd = &mut h.windows[w];
            wd.buffer = Some(b);
            wd.pointm = *point;
            wd.start = *start;
        }
    }
    // Batch Emacs restores the frame's size from the configuration's text
    // lines, one fewer than the terminal frame's: the root window comes back
    // 23 lines tall, and stays so (`(save-window-excursion nil)` then
    // `(window-total-height)` is 23 on GNU Emacs 31.1).
    let root = h.frames[config.frame].root;
    h.windows[root].total_lines = 23;
    // The selected window is selected again with its buffer recorded.
    select_window_internal(h, config.selected, false);
    if let Some(Some(b)) = config.windows.get(config.selected).map(|(b, _, _)| *b) {
        if h.buffers[b].name.is_some() {
            let p = config.windows[config.selected].1;
            let buf = &mut h.buffers[b];
            buf.point = p.clamp(buf.begv, buf.zv);
        }
    }
    if h.buffers[config.current_buffer].name.is_some() {
        h.current = config.current_buffer;
    }
    Ok(Value::Bool(true))
}
fn window_configuration_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::WindowConfig(_)))))
}
fn window_configuration_frame(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(Obj::WindowConfig(id)) = h.obj(&a[0]) else {
        return Err(h.signal_wrong_type("window-configuration-p", &a[0]));
    };
    let f = h.window_configs[*id].frame;
    Ok(h.frames[f].self_obj.clone())
}

/// `assq` of KEY in ALIST: the first cons whose car is `eq` to KEY, else nil.
fn assq(h: &ElispHost, key: &Value, alist: &Value) -> Value {
    let mut cur = alist.clone();
    while let Some(Obj::Cons(cell, rest)) = h.obj(&cur) {
        if let Some(Obj::Cons(k, _)) = h.obj(cell) {
            if h.values_eq(k, key) {
                return cell.clone();
            }
        }
        cur = rest.clone();
    }
    Value::Undef
}

/// The frame's parameters (`f->param_alist`), as batch Emacs's terminal frame
/// starts with them: `(frame-parameters)` on GNU Emacs 31.1.
fn initial_frame_parameters(h: &mut ElispHost) -> Value {
    let sym = |h: &mut ElispHost, s: &str| h.intern(s);
    let mut items = Vec::new();
    let entries: Vec<(&str, Value)> = vec![
        ("no-accept-focus", Value::Undef),
        ("visibility", Value::Bool(true)),
        ("tab-bar-lines", Value::Int(0)),
        ("menu-bar-lines", Value::Int(1)),
        ("buried-buffer-list", Value::Undef),
        ("buffer-list", Value::Undef),
        ("unsplittable", Value::Undef),
        ("modeline", Value::Bool(true)),
        ("width", Value::Int(80)),
        ("height", Value::Int(25)),
        ("name", Value::Undef),
        ("font", Value::Undef),
        ("background-color", Value::Undef),
        ("foreground-color", Value::Undef),
        ("background-mode", Value::Undef),
        ("display-type", Value::Undef),
        ("minibuffer", Value::Bool(true)),
    ];
    for (k, v) in entries {
        let key = sym(h, k);
        let v = match k {
            "name" => h.new_string("F1".to_string()),
            "font" => h.new_string("tty".to_string()),
            "background-color" => h.new_string("unspecified-bg".to_string()),
            "foreground-color" => h.new_string("unspecified-fg".to_string()),
            "background-mode" => sym(h, "dark"),
            "display-type" => sym(h, "mono"),
            _ => v,
        };
        items.push(h.cons(key, v));
    }
    h.list_from(items)
}

/// `frame-parameters`: a copy of the parameter alist, with `buffer-list` the
/// frame's buffer list as it stands.
fn frame_parameters(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    if el_nil(&h.frames[f].parameters) {
        h.frames[f].parameters = initial_frame_parameters(h);
    }
    let params = h.frames[f].parameters.clone();
    let items = h.list_vec(&params).unwrap_or_default();
    let bl_key = h.intern("buffer-list");
    let mut out = Vec::new();
    for cell in items {
        let Some(Obj::Cons(k, v)) = h.obj(&cell) else {
            continue;
        };
        let (k, v) = (k.clone(), v.clone());
        let v = if h.values_eq(&k, &bl_key) {
            let bufs: Vec<Value> = h.frames[f]
                .buffer_list
                .clone()
                .into_iter()
                .filter(|&b| h.buffers[b].name.is_some())
                .map(|b| buffer_obj(h, b))
                .collect();
            h.list_from(bufs)
        } else {
            v
        };
        out.push(h.cons(k, v));
    }
    Ok(h.list_from(out))
}
fn frame_parameter(h: &mut ElispHost, a: &[Value]) -> R {
    let all = frame_parameters(h, &a[..1.min(a.len())])?;
    let cell = assq(h, &a[1], &all);
    Ok(match h.obj(&cell) {
        Some(Obj::Cons(_, v)) => v.clone(),
        _ => Value::Undef,
    })
}
/// `modify-frame-parameters`: `store_frame_param` for each (PARAM . VALUE):
/// an existing parameter changes in place, a new one goes first.
fn modify_frame_parameters(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    if el_nil(&h.frames[f].parameters) {
        h.frames[f].parameters = initial_frame_parameters(h);
    }
    for cell in h.list_vec(&a[1]).unwrap_or_default() {
        let Some(Obj::Cons(k, v)) = h.obj(&cell) else {
            continue;
        };
        let (k, v) = (k.clone(), v.clone());
        let params = h.frames[f].parameters.clone();
        match assq(h, &k, &params) {
            Value::Obj(id) => {
                if let Some(Obj::Cons(_, slot)) = h.arena.get_mut(id as usize) {
                    *slot = v;
                }
            }
            _ => {
                let pair = h.cons(k, v);
                h.frames[f].parameters = h.cons(pair, params);
            }
        }
    }
    Ok(Value::Undef)
}
/// `window-system`: nil, the terminal frame of batch Emacs.
fn window_system(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Undef)
}
/// frame.c's size accessors on the 80x25 terminal frame.
fn frame_cols(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(80))
}
fn frame_lines(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(25))
}

fn nil_or(b: bool) -> Value {
    if b {
        Value::Bool(true)
    } else {
        Value::Undef
    }
}

/// Register the subrs.
pub fn install(h: &mut ElispHost) {
    let mut s = |n: &str, min: usize, max: Option<usize>, f: crate::host::SubrFn| {
        h.defsubr(n, min, max, f);
    };
    s("selected-window", 0, Some(0), selected_window);
    s("minibuffer-window", 0, Some(1), minibuffer_window);
    s("frame-root-window", 0, Some(1), frame_root_window);
    s("frame-selected-window", 0, Some(1), frame_selected_window);
    s("frame-first-window", 0, Some(1), frame_first_window);
    s("window-buffer", 0, Some(1), window_buffer);
    s("window-point", 0, Some(1), window_point);
    s("window-old-point", 0, Some(1), window_old_point);
    s("window-start", 0, Some(1), window_start);
    s("window-end", 0, Some(2), window_end);
    s("set-window-point", 2, Some(2), set_window_point);
    s("set-window-start", 2, Some(3), set_window_start);
    s("set-window-buffer", 2, Some(3), set_window_buffer);
    s("select-window", 1, Some(2), select_window);
    s(
        "set-frame-selected-window",
        2,
        Some(3),
        set_frame_selected_window,
    );
    s("windowp", 1, Some(1), windowp);
    s("window-live-p", 1, Some(1), window_live_p);
    s("window-valid-p", 1, Some(1), window_valid_p);
    s("window-frame", 0, Some(1), window_frame);
    s("window-minibuffer-p", 0, Some(1), window_minibuffer_p);
    s("window-list", 0, Some(3), window_list);
    s("window-list-1", 0, Some(3), window_list_1);
    s("next-window", 0, Some(3), next_window);
    s("previous-window", 0, Some(3), next_window);
    s("get-buffer-window", 0, Some(2), get_buffer_window);
    s("window-dedicated-p", 0, Some(1), window_dedicated_p);
    s("set-window-dedicated-p", 2, Some(2), set_window_dedicated_p);
    s("window-parameters", 0, Some(1), window_parameters);
    s("window-parameter", 2, Some(2), window_parameter);
    s("set-window-parameter", 3, Some(3), set_window_parameter);
    s("window-prev-buffers", 0, Some(1), window_prev_buffers);
    s("window-next-buffers", 0, Some(1), window_next_buffers);
    s(
        "set-window-prev-buffers",
        2,
        Some(2),
        set_window_prev_buffers,
    );
    s(
        "set-window-next-buffers",
        2,
        Some(2),
        set_window_next_buffers,
    );
    s("window-hscroll", 0, Some(1), window_hscroll);
    s("set-window-hscroll", 2, Some(2), set_window_hscroll);
    s("window-total-width", 0, Some(2), window_total_width);
    s("window-total-height", 0, Some(2), window_total_height);
    s("window-body-height", 0, Some(2), window_body_height);
    s("window-body-width", 0, Some(2), window_body_width);
    s("window-left-column", 0, Some(1), window_left_column);
    s("window-top-line", 0, Some(1), window_top_line);
    s("window-parent", 0, Some(1), window_no_relative);
    s("window-top-child", 0, Some(1), window_no_relative);
    s("window-left-child", 0, Some(1), window_no_relative);
    s("window-next-sibling", 0, Some(1), window_no_relative);
    s("window-prev-sibling", 0, Some(1), window_no_relative);
    s("window-use-time", 0, Some(1), window_use_time);
    s("selected-frame", 0, Some(0), selected_frame);
    s("frame-list", 0, Some(0), frame_list);
    s("framep", 1, Some(1), framep);
    s("frame-live-p", 1, Some(1), frame_live_p);
    s("other-buffer", 0, Some(3), other_buffer_fn);
    s("bury-buffer-internal", 1, Some(1), bury_buffer_internal);
    s(
        "current-window-configuration",
        0,
        Some(1),
        current_window_configuration,
    );
    s(
        "set-window-configuration",
        1,
        Some(3),
        set_window_configuration,
    );
    s("window-configuration-p", 1, Some(1), window_configuration_p);
    s(
        "window-configuration-frame",
        1,
        Some(1),
        window_configuration_frame,
    );
    s("frame-parameters", 0, Some(1), frame_parameters);
    s("frame-parameter", 2, Some(2), frame_parameter);
    s(
        "modify-frame-parameters",
        2,
        Some(2),
        modify_frame_parameters,
    );
    s("window-system", 0, Some(1), window_system);
    s("frame-text-cols", 0, Some(1), frame_cols);
    s("frame-total-cols", 0, Some(1), frame_cols);
    s("frame-native-width", 0, Some(1), frame_cols);
    s("frame-text-lines", 0, Some(1), frame_lines);
    s("frame-total-lines", 0, Some(1), frame_lines);
}
