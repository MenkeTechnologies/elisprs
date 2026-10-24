//! The window and frame objects of a batch Emacs (window.c, frame.c).
//!
//! `emacs --batch` has one terminal frame, `F1`, holding the root window
//! (number 1, showing `*scratch*`, 80x24 with a mode line) and the minibuffer
//! window (number 2, showing ` *Minibuf-0*`, 80x1). Nothing is ever drawn,
//! but the objects, their buffers, points and starts, the selected window and
//! frame, window configurations and the frame's buffer list behave as they do
//! there; this module is the C side of them.
//!
//! The windows of a frame form window.c's tree: a live window shows a
//! buffer, an internal window holds a vertical or horizontal combination of
//! child windows, and siblings are chained through `next` / `prev` (the root
//! window's `next` is the minibuffer window). Splitting, deleting and
//! resizing are window.el's, over the primitives here. A terminal frame's
//! character cell is one pixel, so pixel and character sizes coincide.
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
    /// The buffer a live window shows (`contents` when a buffer).
    pub buffer: Option<usize>,
    /// The first child of an internal window (`contents` when a window).
    pub child: Option<usize>,
    /// `horizontal`: an internal window's children are side by side.
    pub horizontal: bool,
    pub parent: Option<usize>,
    pub next: Option<usize>,
    pub prev: Option<usize>,
    /// `old_buffer`: the buffer a deleted window last showed.
    pub old_buffer: Option<usize>,
    /// The buffer `delete_all_child_windows` keeps for a later
    /// `set-window-configuration` (window.c keeps it in `combination_limit`).
    pub saved_buffer: Option<usize>,
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
    /// `left_margin_cols` / `right_margin_cols`.
    pub left_margin: i64,
    pub right_margin: i64,
    pub pixel_left: i64,
    pub pixel_top: i64,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub left_col: i64,
    pub top_line: i64,
    pub total_cols: i64,
    pub total_lines: i64,
    pub normal_lines: Value,
    pub normal_cols: Value,
    pub new_total: Value,
    pub new_normal: Value,
    pub new_pixel: Value,
    pub combination_limit: Value,
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

/// window.c `struct saved_window`: one window of a configuration, in
/// `save_window_save`'s order (each window, then its children).
#[derive(Clone, Debug)]
pub struct SavedWindow {
    pub window: usize,
    pub buffer: Option<usize>,
    pub parent: Option<usize>,
    pub prev: Option<usize>,
    pub pixel_left: i64,
    pub pixel_top: i64,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub left_col: i64,
    pub top_line: i64,
    pub total_cols: i64,
    pub total_lines: i64,
    pub normal_cols: Value,
    pub normal_lines: Value,
    pub hscroll: i64,
    pub left_margin: i64,
    pub right_margin: i64,
    pub dedicated: Value,
    pub combination_limit: Value,
    pub parameters: Value,
    /// Point, old point and start of a live window.
    pub marks: Option<(usize, usize, usize)>,
}

/// A saved window configuration (`current-window-configuration`).
#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub frame: usize,
    pub windows: Vec<SavedWindow>,
    pub root: usize,
    pub selected: usize,
    pub current_buffer: usize,
}

/// The canonical character size of the terminal frame, in pixels.
const UNIT: i64 = 1;
/// `FRAME_TOP_MARGIN`: the terminal frame's menu bar line.
const TOP_MARGIN: i64 = 1;
/// The terminal frame's size in pixels (`FRAME_PIXEL_WIDTH` / `_HEIGHT`).
const FRAME_PIXEL_WIDTH: i64 = 80;
const FRAME_PIXEL_HEIGHT: i64 = 25;

/// `make_window`: a fresh window (no buffer yet) on FRAME.
fn new_window_data(obj: Value, number: i64, frame: usize) -> WindowData {
    WindowData {
        self_obj: obj,
        number,
        frame,
        buffer: None,
        child: None,
        horizontal: false,
        parent: None,
        next: None,
        prev: None,
        old_buffer: None,
        saved_buffer: None,
        pointm: 1,
        old_pointm: 1,
        start: 1,
        mini: false,
        dedicated: Value::Undef,
        parameters: Value::Undef,
        prev_buffers: Value::Undef,
        next_buffers: Value::Undef,
        hscroll: 0,
        left_margin: 0,
        right_margin: 0,
        pixel_left: 0,
        pixel_top: 0,
        pixel_width: 0,
        pixel_height: 0,
        left_col: 0,
        top_line: 0,
        total_cols: 0,
        total_lines: 0,
        normal_lines: Value::Float(1.0),
        normal_cols: Value::Float(1.0),
        new_total: Value::Int(0),
        new_normal: Value::Int(0),
        new_pixel: Value::Int(0),
        combination_limit: Value::Undef,
        use_time: 0,
    }
}

/// `make_window` / `make_parent_window`: allocate a window on FRAME with the
/// next sequence number.
fn make_window(h: &mut ElispHost, frame: usize) -> usize {
    let w = h.windows.len();
    let obj = h.alloc(Obj::Window(w));
    h.window_count += 1;
    let number = h.window_count;
    h.windows.push(new_window_data(obj, number, frame));
    w
}

/// The initial frame and its two windows, made with the host.
pub fn init(h: &mut ElispHost, scratch: usize, minibuf: usize) {
    h.windows.clear();
    h.window_count = 0;
    let root = make_window(h, 0);
    let mini = make_window(h, 0);
    let f1 = h.alloc(Obj::Frame(0));
    for (w, buffer, top, lines) in [(root, scratch, 0, 24), (mini, minibuf, 24, 1)] {
        let wd = &mut h.windows[w];
        wd.buffer = Some(buffer);
        wd.pixel_top = top;
        wd.top_line = top;
        wd.pixel_width = 80;
        wd.total_cols = 80;
        wd.pixel_height = lines;
        wd.total_lines = lines;
    }
    h.windows[root].next = Some(mini);
    h.windows[mini].prev = Some(root);
    h.windows[mini].mini = true;
    h.frames = vec![FrameData {
        self_obj: f1,
        name: "F1".to_string(),
        root,
        mini,
        selected: root,
        buffer_list: vec![scratch],
        parameters: Value::Undef,
    }];
    h.selected_window = root;
}

fn is_nil(v: &Value) -> bool {
    el_nil(v)
}

fn is_live(h: &ElispHost, w: usize) -> bool {
    h.windows[w].buffer.is_some()
}

/// `WINDOW_VALID_P`: live, or an internal window.
fn is_valid(h: &ElispHost, w: usize) -> bool {
    h.windows[w].buffer.is_some() || h.windows[w].child.is_some()
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

/// `decode_valid_window`: as `live_window`, but an internal window passes too.
fn valid_window(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.selected_window),
        Some(v) if is_nil(v) => Ok(h.selected_window),
        Some(v) => match h.obj(v) {
            Some(Obj::Window(i)) if is_valid(h, *i) => Ok(*i),
            _ => Err(h.signal_wrong_type("window-valid-p", v)),
        },
    }
}

/// `decode_any_window`: any window object, deleted ones included.
fn any_window(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.selected_window),
        Some(v) if is_nil(v) => Ok(h.selected_window),
        Some(v) => match h.obj(v) {
            Some(Obj::Window(i)) => Ok(*i),
            _ => Err(h.signal_wrong_type("windowp", v)),
        },
    }
}

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
    all_windows(h)
        .into_iter()
        .any(|w| h.windows[w].buffer == Some(b))
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
    for w in all_windows(h) {
        if h.windows[w].buffer == Some(b) && !h.windows[w].mini {
            h.windows[w].dedicated = Value::Undef;
            set_window_buffer_internal(h, w, replacement, false);
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
fn set_window_buffer_internal(h: &mut ElispHost, w: usize, b: usize, keep_margins: bool) {
    let samebuf = h.windows[w].buffer == Some(b);
    h.windows[w].buffer = Some(b);
    window_discard_buffer(h, w, b);
    if keep_margins && samebuf {
        return;
    }
    let pt = h.buffers[b].point;
    let start = h.last_window_start.get(&b).copied().unwrap_or(1);
    let (begv, zv) = (h.buffers[b].begv, h.buffers[b].zv);
    let wd = &mut h.windows[w];
    wd.pointm = pt;
    wd.old_pointm = pt;
    wd.start = start.clamp(begv, zv);
    wd.hscroll = 0;
    if !keep_margins {
        // `set_window_margins` from the buffer's margin widths.
        let margin = |h: &mut ElispHost, var: &str| {
            let sym = h.intern(var);
            match h.buffer_local_or_default(&sym, b) {
                Ok(Value::Int(n)) if n >= 0 => n,
                _ => 0,
            }
        };
        let (left, right) = (
            margin(h, "left-margin-width"),
            margin(h, "right-margin-width"),
        );
        set_margins(h, w, left, right);
    }
}

/// `set_window_margins`: the new widths, unless they leave no room for
/// two columns of text; true when they changed.
fn set_margins(h: &mut ElispHost, w: usize, left: i64, right: i64) -> bool {
    let wd = &mut h.windows[w];
    if (wd.left_margin, wd.right_margin) == (left, right)
        || wd.pixel_width - (left + right) * UNIT < 2 * UNIT
    {
        return false;
    }
    wd.left_margin = left;
    wd.right_margin = right;
    true
}

/// `select_window`: the old selected window keeps its buffer's point, W
/// becomes selected, its buffer current with point at W's point; unless
/// NORECORD, W's use time advances and its buffer is recorded.
fn select_window_internal(h: &mut ElispHost, w: usize, norecord: bool) {
    select_window_swap(h, w, norecord, false)
}

/// [`select_window_internal`]; with INHIBIT_POINT_SWAP the old selected
/// window's point is left as it is, and reselecting the selected window
/// still sets its buffer's point from the window.
fn select_window_swap(h: &mut ElispHost, w: usize, norecord: bool, inhibit_point_swap: bool) {
    let old = h.selected_window;
    if let Some(b) = h.windows[w].buffer {
        h.current = b;
    }
    if !(old == w && !inhibit_point_swap) {
        if !inhibit_point_swap {
            if let Some(b) = h.windows[old].buffer {
                h.windows[old].pointm = h.buffers[b].point;
            }
        }
        h.selected_window = w;
        let f = h.windows[w].frame;
        h.frames[f].selected = w;
        if let Some(b) = h.windows[w].buffer {
            let pm = h.windows[w].pointm;
            let buf = &mut h.buffers[b];
            buf.point = pm.clamp(buf.begv, buf.zv);
        }
    }
    if let Some(b) = h.windows[w].buffer {
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
    set_window_buffer_internal(h, w, b, false);
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
        matches!(h.obj(&a[0]), Some(Obj::Window(w)) if is_live(h, *w)),
    ))
}
fn window_valid_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(h.obj(&a[0]), Some(Obj::Window(w)) if is_valid(h, *w)),
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
fn window_use_time(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].use_time))
}
fn frame_first_window(h: &mut ElispHost, a: &[Value]) -> R {
    let f = frame_or_window_frame(h, arg(a, 0))?;
    Ok(window_obj(h, first_window(h, f)))
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

// ── the window tree ─────────────────────────────────────────────────────────

/// `wset_combination`: W's children start at CHILD (none makes W deleted).
fn set_combination(h: &mut ElispHost, w: usize, horizontal: bool, child: Option<usize>) {
    let wd = &mut h.windows[w];
    wd.child = child;
    wd.horizontal = child.is_some() && horizontal;
    if child.is_some() {
        wd.buffer = None;
    }
}

/// The children of internal window W, in order.
fn children(h: &ElispHost, w: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut c = h.windows[w].child;
    while let Some(x) = c {
        out.push(x);
        c = h.windows[x].next;
    }
    out
}

fn vertical_combination(h: &ElispHost, w: usize) -> bool {
    h.windows[w].child.is_some() && !h.windows[w].horizontal
}

fn horizontal_combination(h: &ElispHost, w: usize) -> bool {
    h.windows[w].child.is_some() && h.windows[w].horizontal
}

/// `foreach_window`: the live windows of FRAME, depth first from the root
/// window and on to its `next`, the minibuffer window.
fn frame_windows(h: &ElispHost, f: usize) -> Vec<usize> {
    fn walk(h: &ElispHost, mut w: Option<usize>, out: &mut Vec<usize>) {
        while let Some(x) = w {
            match h.windows[x].child {
                Some(c) => walk(h, Some(c), out),
                None => out.push(x),
            }
            w = h.windows[x].next;
        }
    }
    let mut out = Vec::new();
    walk(h, Some(h.frames[f].root), &mut out);
    out
}

/// `window_list`: the live windows of every frame.
fn all_windows(h: &ElispHost) -> Vec<usize> {
    (0..h.frames.len())
        .flat_map(|f| frame_windows(h, f))
        .collect()
}

/// `replace_window`: NEW takes OLD's place in the tree (and, with SETFLAG,
/// its geometry and normal sizes).
fn replace_window(h: &mut ElispHost, old: usize, new: usize, setflag: bool) {
    let f = h.windows[old].frame;
    if h.frames[f].root == old {
        h.frames[f].root = new;
    }
    if setflag {
        let o = h.windows[old].clone();
        let n = &mut h.windows[new];
        n.pixel_left = o.pixel_left;
        n.pixel_top = o.pixel_top;
        n.pixel_width = o.pixel_width;
        n.pixel_height = o.pixel_height;
        n.left_col = o.left_col;
        n.top_line = o.top_line;
        n.total_cols = o.total_cols;
        n.total_lines = o.total_lines;
        n.normal_cols = o.normal_cols;
        n.normal_lines = o.normal_lines;
        h.windows[old].normal_cols = Value::Float(1.0);
        h.windows[old].normal_lines = Value::Float(1.0);
    }
    let (next, prev, parent) = {
        let o = &h.windows[old];
        (o.next, o.prev, o.parent)
    };
    h.windows[new].next = next;
    if let Some(t) = next {
        h.windows[t].prev = Some(new);
    }
    h.windows[new].prev = prev;
    if let Some(t) = prev {
        h.windows[t].next = Some(new);
    }
    h.windows[new].parent = parent;
    if let Some(p) = parent {
        if h.windows[p].child == Some(old) {
            let hz = h.windows[p].horizontal;
            set_combination(h, p, hz, Some(new));
        }
    }
}

/// `recombine_windows`: when WINDOW's children combine in the direction of
/// WINDOW's own combination, they become children of WINDOW's parent.
fn recombine_windows(h: &mut ElispHost, w: usize) {
    let Some(p) = h.windows[w].parent else {
        return;
    };
    if !is_nil(&h.windows[w].combination_limit) {
        return;
    }
    let (Some(_), Some(first)) = (h.windows[p].child, h.windows[w].child) else {
        return;
    };
    if h.windows[p].horizontal != h.windows[w].horizontal {
        return;
    }
    let horflag = horizontal_combination(h, w);
    match h.windows[w].prev {
        None => set_combination(h, p, horflag, Some(first)),
        Some(prev) => {
            h.windows[first].prev = Some(prev);
            h.windows[prev].next = Some(first);
        }
    }
    let mut c = Some(first);
    while let Some(x) = c {
        h.windows[x].parent = Some(p);
        if horflag {
            h.windows[x].normal_cols =
                Value::Float(h.windows[x].pixel_width as f64 / h.windows[p].pixel_width as f64);
        } else {
            h.windows[x].normal_lines =
                Value::Float(h.windows[x].pixel_height as f64 / h.windows[p].pixel_height as f64);
        }
        match h.windows[x].next {
            None => {
                if let Some(n) = h.windows[w].next {
                    h.windows[x].next = Some(n);
                    h.windows[n].prev = Some(x);
                }
                c = None;
            }
            next => c = next,
        }
    }
    set_combination(h, w, false, None);
}

/// `delete_all_child_windows`: W, its later siblings and all their
/// descendants are deleted; a live one keeps its buffer for
/// `set-window-configuration`.
fn delete_all_child_windows(h: &mut ElispHost, w: usize) {
    if let Some(n) = h.windows[w].next {
        delete_all_child_windows(h, n);
    }
    if let Some(c) = h.windows[w].child {
        delete_all_child_windows(h, c);
        set_combination(h, w, false, None);
    } else if let Some(b) = h.windows[w].buffer {
        h.windows[w].old_buffer = Some(b);
        unshow_buffer(h, w);
        h.windows[w].saved_buffer = Some(b);
        h.windows[w].buffer = None;
    }
}

/// `window_resize_check`: the new pixel sizes of W's subtree add up.
fn window_resize_check(h: &ElispHost, w: usize, horflag: bool) -> bool {
    let new_pixel = int_of(&h.windows[w].new_pixel);
    let kids = children(h, w);
    if kids.is_empty() {
        return new_pixel >= if horflag { 2 * UNIT } else { UNIT };
    }
    let along = horizontal_combination(h, w) == horflag;
    if along {
        let mut remaining = new_pixel;
        for c in kids {
            if !window_resize_check(h, c, horflag) {
                return false;
            }
            remaining -= int_of(&h.windows[c].new_pixel);
            if remaining < 0 {
                return false;
            }
        }
        remaining == 0
    } else {
        kids.into_iter().all(|c| {
            int_of(&h.windows[c].new_pixel) == new_pixel && window_resize_check(h, c, horflag)
        })
    }
}

/// `window_resize_apply`: the new pixel sizes (and new normal sizes) of W's
/// subtree are installed and the children's positions recomputed.
fn window_resize_apply(h: &mut ElispHost, w: usize, horflag: bool) {
    let new_pixel = int_of(&h.windows[w].new_pixel);
    let new_normal = h.windows[w].new_normal.clone();
    let numberp = matches!(new_normal, Value::Int(_) | Value::Float(_));
    let mut edge = {
        let wd = &mut h.windows[w];
        if horflag {
            wd.pixel_width = new_pixel;
            wd.total_cols = new_pixel / UNIT;
            if numberp {
                wd.normal_cols = new_normal;
            }
            wd.pixel_left
        } else {
            wd.pixel_height = new_pixel;
            wd.total_lines = new_pixel / UNIT;
            if numberp {
                wd.normal_lines = new_normal;
            }
            wd.pixel_top
        }
    };
    let along = horizontal_combination(h, w) == horflag;
    for c in children(h, w) {
        if horflag {
            h.windows[c].pixel_left = edge;
            h.windows[c].left_col = edge / UNIT;
        } else {
            h.windows[c].pixel_top = edge;
            h.windows[c].top_line = edge / UNIT;
        }
        window_resize_apply(h, c, horflag);
        if along {
            edge += if horflag {
                h.windows[c].pixel_width
            } else {
                h.windows[c].pixel_height
            };
        }
    }
}

/// `window_resize_apply_total`: the new total sizes of W's subtree are
/// installed and the children's line or column positions recomputed.
fn window_resize_apply_total(h: &mut ElispHost, w: usize, horflag: bool) {
    let new_total = int_of(&h.windows[w].new_total);
    let mut edge = {
        let wd = &mut h.windows[w];
        if horflag {
            wd.total_cols = new_total;
            wd.left_col
        } else {
            wd.total_lines = new_total;
            wd.top_line
        }
    };
    let along = horizontal_combination(h, w) == horflag;
    for c in children(h, w) {
        if horflag {
            h.windows[c].left_col = edge;
        } else {
            h.windows[c].top_line = edge;
        }
        window_resize_apply_total(h, c, horflag);
        if along {
            edge += if horflag {
                h.windows[c].total_cols
            } else {
                h.windows[c].total_lines
            };
        }
    }
}

fn int_of(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        _ => 0,
    }
}

/// The value of window parameter PARAM of W.
fn param(h: &mut ElispHost, w: usize, param: &str) -> Value {
    let key = h.intern(param);
    let params = h.windows[w].parameters.clone();
    let cell = assq(h, &key, &params);
    match h.obj(&cell) {
        Some(Obj::Cons(_, v)) => v.clone(),
        _ => Value::Undef,
    }
}

/// The value of buffer B's variable VAR; true when it is void here.
fn buffer_var_set(h: &mut ElispHost, b: usize, var: &str) -> bool {
    let sym = h.intern(var);
    h.buffer_local_or_default(&sym, b)
        .map(|v| el_truthy(&v))
        .unwrap_or(var == "mode-line-format")
}

/// `window_wants_mode_line`.
fn wants_mode_line(h: &mut ElispHost, w: usize) -> bool {
    let Some(b) = h.windows[w].buffer else {
        return false;
    };
    if h.windows[w].mini {
        return false;
    }
    let format = param(h, w, "mode-line-format");
    let none = h.sym_name(&format).as_deref() == Some("none");
    !none
        && (el_truthy(&format) || buffer_var_set(h, b, "mode-line-format"))
        && h.windows[w].pixel_height > UNIT
}

/// `window_wants_header_line`.
fn wants_header_line(h: &mut ElispHost, w: usize) -> bool {
    let Some(b) = h.windows[w].buffer else {
        return false;
    };
    if h.windows[w].mini {
        return false;
    }
    let format = param(h, w, "header-line-format");
    let none = h.sym_name(&format).as_deref() == Some("none");
    let room = if wants_mode_line(h, w) {
        2 * UNIT
    } else {
        UNIT
    };
    !none
        && (el_truthy(&format) || buffer_var_set(h, b, "header-line-format"))
        && h.windows[w].pixel_height > room
}

/// `window_wants_tab_line`.
fn wants_tab_line(h: &mut ElispHost, w: usize) -> bool {
    let Some(b) = h.windows[w].buffer else {
        return false;
    };
    if h.windows[w].mini {
        return false;
    }
    let format = param(h, w, "tab-line-format");
    let none = h.sym_name(&format).as_deref() == Some("none");
    let lines = i64::from(wants_mode_line(h, w)) + i64::from(wants_header_line(h, w)) + 1;
    !none
        && (el_truthy(&format) || buffer_var_set(h, b, "tab-line-format"))
        && h.windows[w].pixel_height > lines * UNIT
}

/// `WINDOW_RIGHTMOST_P`: no window to the right of W in its frame.
fn rightmost(h: &ElispHost, w: usize) -> bool {
    let mut x = w;
    while let Some(p) = h.windows[x].parent {
        if horizontal_combination(h, p) && h.windows[x].next.is_some() {
            return false;
        }
        x = p;
    }
    true
}

/// `window_body_height` in pixels (lines, the unit being one pixel).
fn body_height(h: &mut ElispHost, w: usize) -> i64 {
    let lines = i64::from(wants_tab_line(h, w))
        + i64::from(wants_header_line(h, w))
        + i64::from(wants_mode_line(h, w));
    (h.windows[w].pixel_height - lines * UNIT).max(0)
}

/// `window_body_width`: a terminal window that is not rightmost gives a
/// column to the vertical border.
fn body_width(h: &ElispHost, w: usize) -> i64 {
    let wd = &h.windows[w];
    let margins = (wd.left_margin + wd.right_margin) * UNIT;
    (wd.pixel_width - i64::from(!rightmost(h, w)) - margins).max(0)
}

// ── tree and size primitives ────────────────────────────────────────────────

fn window_or_nil(h: &ElispHost, w: Option<usize>) -> Value {
    w.map_or(Value::Undef, |w| window_obj(h, w))
}
fn window_parent(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(window_or_nil(h, h.windows[w].parent))
}
fn window_top_child(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let c = h.windows[w].child.filter(|_| vertical_combination(h, w));
    Ok(window_or_nil(h, c))
}
fn window_left_child(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let c = h.windows[w].child.filter(|_| horizontal_combination(h, w));
    Ok(window_or_nil(h, c))
}
fn window_next_sibling(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(window_or_nil(h, h.windows[w].next))
}
fn window_prev_sibling(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(window_or_nil(h, h.windows[w].prev))
}
/// `CHECK_VALID_WINDOW` and the leaf check of `window-combination-limit`.
fn internal_window(h: &mut ElispHost, v: &Value) -> Result<usize, String> {
    let w = match h.obj(v) {
        Some(Obj::Window(w)) if is_valid(h, *w) => *w,
        _ => return Err(h.signal_wrong_type("window-valid-p", v)),
    };
    if h.windows[w].child.is_none() {
        return Err("error: Combination limit is meaningful for internal windows only".into());
    }
    Ok(w)
}
fn window_combination_limit(h: &mut ElispHost, a: &[Value]) -> R {
    let w = internal_window(h, &a[0])?;
    Ok(h.windows[w].combination_limit.clone())
}
fn set_window_combination_limit(h: &mut ElispHost, a: &[Value]) -> R {
    let w = internal_window(h, &a[0])?;
    h.windows[w].combination_limit = a[1].clone();
    Ok(a[1].clone())
}
fn window_pixel_width(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].pixel_width))
}
fn window_pixel_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].pixel_height))
}
fn window_pixel_left(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].pixel_left))
}
fn window_pixel_top(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].pixel_top))
}
/// `window-total-width` / `-height`: ROUND `ceiling` or `floor` divides the
/// pixel size by the character size; anything else is the internal total.
fn window_total_size(h: &mut ElispHost, a: &[Value], horizontal: bool) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let round = arg(a, 1).and_then(|v| h.sym_name(v));
    let wd = &h.windows[w];
    let (total, pixel) = if horizontal {
        (wd.total_cols, wd.pixel_width)
    } else {
        (wd.total_lines, wd.pixel_height)
    };
    Ok(Value::Int(match round.as_deref() {
        Some("ceiling") => (pixel + UNIT - 1) / UNIT,
        Some("floor") => pixel / UNIT,
        _ => total,
    }))
}
fn window_total_width(h: &mut ElispHost, a: &[Value]) -> R {
    window_total_size(h, a, true)
}
fn window_total_height(h: &mut ElispHost, a: &[Value]) -> R {
    window_total_size(h, a, false)
}
fn window_new_total(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(h.windows[w].new_total.clone())
}
fn window_new_normal(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(h.windows[w].new_normal.clone())
}
fn window_new_pixel(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(h.windows[w].new_pixel.clone())
}
fn window_normal_size(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let wd = &h.windows[w];
    Ok(if arg(a, 1).is_some_and(el_truthy) {
        wd.normal_cols.clone()
    } else {
        wd.normal_lines.clone()
    })
}
/// `set-window-new-pixel`: SIZE, or with ADD the current new size plus SIZE;
/// the result must stay a non-negative int.
fn set_window_new_pixel(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let add = arg(a, 2).is_some_and(el_truthy);
    let cur = int_of(&h.windows[w].new_pixel);
    let min = if add { -cur } else { 0 };
    let size = match &a[1] {
        Value::Int(n) if *n >= min && *n <= min + i64::from(i32::MAX) => *n,
        Value::Int(n) => {
            let hi = min + i64::from(i32::MAX);
            return Err(format!("args-out-of-range: {n} {min} {hi}"));
        }
        v => return Err(h.signal_wrong_type("integerp", v)),
    };
    h.windows[w].new_pixel = Value::Int(if add { cur + size } else { size });
    Ok(h.windows[w].new_pixel.clone())
}
fn set_window_new_total(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let size = match &a[1] {
        Value::Int(n) => *n,
        v => return Err(h.signal_wrong_type("fixnump", v)),
    };
    let add = arg(a, 2).is_some_and(el_truthy);
    let cur = int_of(&h.windows[w].new_total);
    h.windows[w].new_total = Value::Int(if add { cur + size } else { size });
    Ok(h.windows[w].new_total.clone())
}
fn set_window_new_normal(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    let size = arg(a, 1).cloned().unwrap_or(Value::Undef);
    h.windows[w].new_normal = size.clone();
    Ok(size)
}
fn window_left_column(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].left_col))
}
fn window_top_line(h: &mut ElispHost, a: &[Value]) -> R {
    let w = valid_window(h, arg(a, 0))?;
    Ok(Value::Int(h.windows[w].top_line))
}
/// `window-body-height`: the window less its mode, header and tab lines.
fn window_body_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(body_height(h, w)))
}
/// `window-body-width`: the window less the border column of a window
/// that is not rightmost (batch Emacs has no fringes, margins or scroll bars).
fn window_body_width(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(body_width(h, w)))
}
fn window_mode_line_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(i64::from(wants_mode_line(h, w)) * UNIT))
}
fn window_header_line_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(i64::from(wants_header_line(h, w)) * UNIT))
}
fn window_tab_line_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(i64::from(wants_tab_line(h, w)) * UNIT))
}
/// The divider widths and scroll bar areas of a terminal window: none.
fn window_zero_width(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
/// `window-resize-apply`: install the new pixel sizes when they add up to
/// the root window's size.
fn window_resize_apply_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let horflag = arg(a, 1).is_some_and(el_truthy);
    let r = h.frames[f].root;
    let size = if horflag {
        h.windows[r].pixel_width
    } else {
        h.windows[r].pixel_height
    };
    if !window_resize_check(h, r, horflag) || int_of(&h.windows[r].new_pixel) != size {
        return Ok(Value::Undef);
    }
    window_resize_apply(h, r, horflag);
    Ok(Value::Bool(true))
}
/// `window-resize-apply-total`: the new total sizes, the root window placed
/// below the frame's menu bar line and the minibuffer window below it.
fn window_resize_apply_total_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let horflag = arg(a, 1).is_some_and(el_truthy);
    let r = h.frames[f].root;
    h.windows[r].left_col = 0;
    h.windows[r].top_line = TOP_MARGIN;
    window_resize_apply_total(h, r, horflag);
    let m = h.frames[f].mini;
    let new_total = int_of(&h.windows[m].new_total);
    if horflag {
        h.windows[m].total_cols = new_total;
    } else {
        h.windows[m].top_line = h.windows[r].top_line + h.windows[r].total_lines;
        h.windows[m].total_lines = new_total;
    }
    Ok(Value::Bool(true))
}

/// `split-window-internal`: OLD shares its space with a new window of
/// PIXEL-SIZE on SIDE, whose sizes window.el's `split-window` has set up.
fn split_window_internal(h: &mut ElispHost, a: &[Value]) -> R {
    let o = valid_window(h, arg(a, 0))?;
    let f = h.windows[o].frame;
    let side = arg(a, 2).cloned().unwrap_or(Value::Undef);
    let side_name = h.sym_name(&side);
    let horflag =
        matches!(side, Value::Bool(true)) || matches!(side_name.as_deref(), Some("left" | "right"));
    let pixel_size = match &a[1] {
        Value::Int(n) => *n,
        v => return Err(h.signal_wrong_type("fixnump", v)),
    };
    let normal_size = arg(a, 3).cloned().unwrap_or(Value::Undef);
    let total_size = pixel_size / UNIT;
    let limit_var = {
        let sym = h.intern("window-combination-limit");
        h.get_value(&sym).unwrap_or(Value::Undef)
    };
    let resize_var = {
        let sym = h.intern("window-combination-resize");
        h.get_value(&sym).unwrap_or(Value::Undef)
    };
    let combination_limit = matches!(limit_var, Value::Bool(true))
        || match h.windows[o].parent {
            None => true,
            Some(p) => {
                if horflag {
                    vertical_combination(h, p)
                } else {
                    horizontal_combination(h, p)
                }
            }
        };
    // REFER: a live window to copy, or a deleted one to bring back.
    let mut parent: Option<usize> = None;
    let refer = arg(a, 4).cloned().unwrap_or(Value::Undef);
    let (r, dead) = if is_nil(&refer) {
        let r = if is_live(h, o) {
            o
        } else {
            h.frames[f].selected
        };
        (r, false)
    } else if let Some(Obj::Cons(car, cdr)) = h.obj(&refer) {
        let (car, cdr) = (car.clone(), cdr.clone());
        let r = any_window(h, Some(&car))?;
        if is_valid(h, r) || h.windows[r].old_buffer.is_none() {
            return Err("error: REFER's car must specify a deleted, former live window".into());
        }
        let ob = h.windows[r].old_buffer.expect("old buffer");
        if h.buffers[ob].name.is_none() {
            return Err("error: The buffer formerly shown by REFER's car has been killed".into());
        }
        if h.windows[r].frame != f {
            return Err("error: REFER's car must specify a window on same frame as WINDOW".into());
        }
        let p = if matches!(cdr, Value::Bool(true)) {
            h.windows[r].parent
        } else {
            Some(any_window(h, Some(&cdr))?)
        };
        let Some(p) = p else {
            return Err(h.signal_wrong_type("windowp", &Value::Undef));
        };
        if is_valid(h, p) || h.windows[p].old_buffer.is_some() {
            return Err("error: REFER's cdr must specify a deleted, former parent window".into());
        }
        if h.windows[p].frame != f {
            return Err("error: REFER's cdr must specify window on same frame as WINDOW".into());
        }
        parent = Some(p);
        (r, true)
    } else {
        let r = any_window(h, Some(&refer))?;
        if !is_valid(h, r) {
            match h.windows[r].old_buffer {
                None => {
                    return Err(
                        "error: REFER must specify a former live window (must have shown a buffer)"
                            .into(),
                    )
                }
                Some(ob) if h.buffers[ob].name.is_none() => {
                    return Err("error: The buffer formerly shown by REFER has been killed".into())
                }
                _ if h.windows[r].frame != f => {
                    return Err("error: REFER must specify a window on same frame as WINDOW".into())
                }
                _ => (r, true),
            }
        } else if !is_live(h, r) {
            return Err("error: REFER is not a live window (does not show a buffer)".into());
        } else {
            (r, false)
        }
    };
    let size_of = |h: &ElispHost, w: usize| {
        if horflag {
            h.windows[w].pixel_width
        } else {
            h.windows[w].pixel_height
        }
    };
    if h.windows[o].mini {
        return Err("error: Attempt to split minibuffer window".into());
    } else if total_size < if horflag { 2 } else { 1 } {
        return Err("error: Size of new window too small (after split)".into());
    } else if !combination_limit && el_truthy(&resize_var) {
        let op = h.windows[o].parent.expect("combined window");
        h.windows[op].new_pixel = Value::Int(size_of(h, op) - pixel_size);
        if !window_resize_check(h, op, horflag) {
            return Err("error: Window sizes don't fit".into());
        }
        h.windows[op].new_pixel = Value::Int(size_of(h, op));
    } else if !window_resize_check(h, o, horflag) {
        return Err("error: Resizing old window failed".into());
    } else if pixel_size + int_of(&h.windows[o].new_pixel) != size_of(h, o) {
        return Err("error: Sum of sizes of old and new window don't fit".into());
    }
    // The point of no return.
    let p = if combination_limit {
        let new_normal = if horflag {
            h.windows[o].normal_cols.clone()
        } else {
            h.windows[o].normal_lines.clone()
        };
        let p = match parent {
            Some(p) => p,
            None => make_window(h, f),
        };
        replace_window(h, o, p, true);
        h.windows[o].next = None;
        h.windows[o].prev = None;
        h.windows[o].parent = Some(p);
        set_combination(h, p, horflag, Some(o));
        if matches!(limit_var, Value::Bool(true)) {
            h.windows[p].combination_limit = Value::Bool(true);
        }
        let (pixel, total) = if horflag {
            (h.windows[o].pixel_width, h.windows[o].total_cols)
        } else {
            (h.windows[o].pixel_height, h.windows[o].total_lines)
        };
        h.windows[p].new_pixel = Value::Int(pixel);
        h.windows[p].new_total = Value::Int(total);
        h.windows[p].new_normal = new_normal;
        p
    } else {
        h.windows[o].parent.expect("combined window")
    };
    let n = if dead { r } else { make_window(h, f) };
    if !dead {
        h.windows[n].left_margin = h.windows[r].left_margin;
        h.windows[n].right_margin = h.windows[r].right_margin;
    }
    h.windows[n].frame = f;
    h.windows[n].parent = h.windows[o].parent;
    if matches!(side_name.as_deref(), Some("above" | "left")) {
        let prev = h.windows[o].prev;
        h.windows[n].prev = prev;
        match prev {
            None => set_combination(h, p, horflag, Some(n)),
            Some(pv) => h.windows[pv].next = Some(n),
        }
        h.windows[n].next = Some(o);
        h.windows[o].prev = Some(n);
    } else {
        let next = h.windows[o].next;
        h.windows[n].next = next;
        if let Some(nx) = next {
            h.windows[nx].prev = Some(n);
        }
        h.windows[n].prev = Some(o);
        h.windows[o].next = Some(n);
    }
    // Orthogonal coordinates and sizes come from OLD directly.
    if horflag {
        h.windows[n].pixel_top = h.windows[o].pixel_top;
        h.windows[n].top_line = h.windows[o].top_line;
        h.windows[n].pixel_height = h.windows[o].pixel_height;
        h.windows[n].total_lines = h.windows[o].total_lines;
    } else {
        h.windows[n].pixel_left = h.windows[o].pixel_left;
        h.windows[n].left_col = h.windows[o].left_col;
        h.windows[n].pixel_width = h.windows[o].pixel_width;
        h.windows[n].total_cols = h.windows[o].total_cols;
    }
    h.windows[n].new_pixel = Value::Int(pixel_size);
    let sum: i64 = children(h, p)
        .into_iter()
        .filter(|&c| c != n)
        .map(|c| int_of(&h.windows[c].new_total))
        .sum();
    let p_total = if horflag {
        h.windows[p].total_cols
    } else {
        h.windows[p].total_lines
    };
    h.windows[n].new_total = Value::Int(p_total - sum);
    h.windows[n].new_normal = normal_size;
    if dead {
        // The dead window gets its old buffer and positions back.
        let ob = h.windows[n].old_buffer.expect("old buffer");
        let (begv, zv) = (h.buffers[ob].begv, h.buffers[ob].zv);
        let wd = &mut h.windows[n];
        wd.buffer = Some(ob);
        wd.start = wd.start.clamp(begv, zv);
        wd.pointm = wd.pointm.clamp(begv, zv);
        wd.old_pointm = wd.old_pointm.clamp(begv, zv);
    }
    window_resize_apply(h, p, horflag);
    let rb = h.windows[r]
        .buffer
        .expect("reference window shows a buffer");
    set_window_buffer_internal(h, n, rb, true);
    Ok(window_obj(h, n))
}

/// `delete-window-internal`: WINDOW leaves the tree and its siblings get
/// its space as window.el's `delete-window` has set up.
fn delete_window_internal(h: &mut ElispHost, a: &[Value]) -> R {
    let w = any_window(h, arg(a, 0))?;
    if !is_valid(h, w) {
        return Ok(Value::Undef);
    }
    let Some(p) = h.windows[w].parent else {
        return Err("error: Attempt to delete minibuffer or sole ordinary window".into());
    };
    if h.windows[w].prev.is_none() && h.windows[w].next.is_none() {
        return Err("error: Attempt to delete sole window of parent".into());
    }
    let horflag = horizontal_combination(h, p);
    let f = h.windows[w].frame;
    let r = h.frames[f].root;
    // Unlink WINDOW.
    let s = match h.windows[w].prev {
        None => {
            let s = h.windows[w].next.expect("sibling");
            h.windows[s].prev = None;
            set_combination(h, p, horflag, Some(s));
            s
        }
        Some(s) => {
            let next = h.windows[w].next;
            h.windows[s].next = next;
            if let Some(nx) = next {
                h.windows[nx].prev = Some(s);
            }
            s
        }
    };
    let r_size = if horflag {
        h.windows[r].pixel_width
    } else {
        h.windows[r].pixel_height
    };
    if window_resize_check(h, r, horflag) && int_of(&h.windows[r].new_pixel) == r_size {
        window_resize_apply(h, p, horflag);
        h.windows[w].next = None;
        if let Some(c) = h.windows[w].child {
            delete_all_child_windows(h, c);
            set_combination(h, w, false, None);
        } else {
            h.windows[w].old_buffer = h.windows[w].buffer;
            unshow_buffer(h, w);
            h.windows[w].buffer = None;
        }
        if h.windows[s].prev.is_none() && h.windows[s].next.is_none() {
            // SIBLING is PARENT's only child now: it takes PARENT's place.
            replace_window(h, p, s, false);
            h.windows[s].normal_cols = h.windows[p].normal_cols.clone();
            h.windows[s].normal_lines = h.windows[p].normal_lines.clone();
            set_combination(h, p, false, None);
            recombine_windows(h, s);
        }
        let sel = h.frames[f].selected;
        if !is_live(h, sel) {
            let first = first_window(h, f);
            if sel == h.selected_window {
                select_window_internal(h, first, true);
            } else {
                h.frames[f].selected = first;
            }
        }
        Ok(Value::Undef)
    } else {
        // Undo the unlinking.
        match h.windows[w].prev {
            None => {
                h.windows[s].prev = Some(w);
                set_combination(h, p, horflag, Some(w));
            }
            Some(pv) => {
                h.windows[pv].next = Some(w);
                if let Some(nx) = h.windows[w].next {
                    h.windows[nx].prev = Some(w);
                }
            }
        }
        Err("error: Deletion failed".into())
    }
}

/// `Fframe_first_window`: the leftmost, topmost live window of frame F.
fn first_window(h: &ElispHost, f: usize) -> usize {
    let mut w = h.frames[f].root;
    while let Some(c) = h.windows[w].child {
        w = c;
    }
    w
}

// ── window lists ────────────────────────────────────────────────────────────

/// `candidate_window_p` for the arguments `decode_next_window_args` leaves:
/// MINIBUF is `t` (the minibuffer window counts) or lambda (it does not; an
/// active minibuffer is never the case in batch), ALL_FRAMES a frame or nil
/// (the frame of OWINDOW) or t.
fn candidate_window_p(
    h: &ElispHost,
    w: usize,
    owindow: usize,
    minibuf_t: bool,
    all_frames: &FrameScope,
) -> bool {
    if !is_live(h, w) {
        return false;
    }
    if h.windows[w].mini && !minibuf_t {
        return false;
    }
    match all_frames {
        FrameScope::All => true,
        FrameScope::OwnFrame => h.windows[w].frame == h.windows[owindow].frame,
        FrameScope::Frame(f) => h.windows[w].frame == *f,
    }
}

/// The frames a window list ranges over.
enum FrameScope {
    /// nil: the frame of the starting window.
    OwnFrame,
    /// t, `visible`, 0: every frame (batch has one, always visible).
    All,
    Frame(usize),
}

/// `decode_next_window_args`: the starting window and whether the
/// minibuffer window is a candidate (batch never has an active one, so
/// only MINIBUF t makes it one).
fn next_window_args(
    h: &mut ElispHost,
    a: &[Value],
    window_at: usize,
    minibuf_at: usize,
    frames_at: usize,
) -> Result<(usize, bool, FrameScope), String> {
    let w = live_window(h, arg(a, window_at))?;
    let minibuf_t = arg(a, minibuf_at).is_some_and(|v| matches!(v, Value::Bool(true)));
    let scope = match arg(a, frames_at) {
        None => FrameScope::OwnFrame,
        Some(v) if is_nil(v) => FrameScope::OwnFrame,
        Some(v) => match h.obj(v) {
            Some(Obj::Frame(f)) => FrameScope::Frame(*f),
            _ => {
                let visible = h.sym_name(v).as_deref() == Some("visible");
                if matches!(v, Value::Bool(true) | Value::Int(0)) || visible {
                    FrameScope::All
                } else {
                    FrameScope::OwnFrame
                }
            }
        },
    };
    Ok((w, minibuf_t, scope))
}

/// `next_window`.
fn next_window_impl(h: &mut ElispHost, a: &[Value], next_p: bool) -> R {
    let (w, minibuf_t, scope) = next_window_args(h, a, 0, 1, 2)?;
    if let FrameScope::Frame(f) = scope {
        if f != h.windows[w].frame {
            return Ok(window_obj(h, first_window(h, f)));
        }
    }
    let list = all_windows(h);
    let pos = list.iter().position(|&x| x == w);
    let cand = |x: usize| candidate_window_p(h, x, w, minibuf_t, &scope);
    let found = if next_p {
        let after = pos.map_or(&list[..0], |p| &list[p + 1..]);
        after.iter().copied().find(|&x| cand(x)).or_else(|| {
            list.iter()
                .copied()
                .take_while(|&x| x != w)
                .find(|&x| cand(x))
        })
    } else {
        let mut candidate = None;
        for &x in &list {
            if x == w {
                if candidate.is_some() {
                    break;
                }
            } else if cand(x) {
                candidate = Some(x);
            }
        }
        candidate
    };
    Ok(window_obj(h, found.unwrap_or(w)))
}
fn next_window(h: &mut ElispHost, a: &[Value]) -> R {
    next_window_impl(h, a, true)
}
fn previous_window(h: &mut ElispHost, a: &[Value]) -> R {
    next_window_impl(h, a, false)
}

/// `window_list_1`: the candidate windows, starting with WINDOW.
fn window_list_order(h: &ElispHost, w: usize, minibuf_t: bool, scope: &FrameScope) -> Vec<usize> {
    let mut list: Vec<usize> = all_windows(h)
        .into_iter()
        .filter(|&x| candidate_window_p(h, x, w, minibuf_t, scope))
        .collect();
    if let Some(p) = list.iter().position(|&x| x == w) {
        list.rotate_left(p);
    }
    list
}

/// `window-list`: FRAME's live windows in cyclic order from WINDOW (the
/// frame's selected window by default); the minibuffer window only when
/// MINIBUF is t (it is never active in batch).
fn window_list(h: &mut ElispHost, a: &[Value]) -> R {
    let f = match arg(a, 0) {
        Some(v) if !is_nil(v) => Some(live_frame(h, Some(v))?),
        _ => None,
    };
    let w = match arg(a, 2) {
        Some(v) if !is_nil(v) => match h.obj(v) {
            Some(Obj::Window(w)) => *w,
            _ => return Err(h.signal_wrong_type("windowp", v)),
        },
        _ => f.map_or(h.selected_window, |f| h.frames[f].selected),
    };
    let f = f.unwrap_or(h.selected_frame);
    if h.windows[w].frame != f {
        return Err("error: Window is on a different frame".into());
    }
    if !is_live(h, w) {
        return Err(h.signal_wrong_type("window-live-p", &window_obj(h, w)));
    }
    let minibuf_t = arg(a, 1).is_some_and(|v| matches!(v, Value::Bool(true)));
    let items = window_list_order(h, w, minibuf_t, &FrameScope::Frame(f))
        .into_iter()
        .map(|w| window_obj(h, w))
        .collect();
    Ok(h.list_from(items))
}
/// `window-list-1`: WINDOW first, then the others in cyclic order.
fn window_list_1(h: &mut ElispHost, a: &[Value]) -> R {
    let (w, minibuf_t, scope) = next_window_args(h, a, 0, 1, 2)?;
    let items = window_list_order(h, w, minibuf_t, &scope)
        .into_iter()
        .map(|w| window_obj(h, w))
        .collect();
    Ok(h.list_from(items))
}
/// `get-buffer-window` (`window_loop` GET_BUFFER_WINDOW): the selected
/// window when it shows BUFFER-OR-NAME, else the first such window in
/// cyclic order; nil when none (or the buffer is not live).
fn get_buffer_window(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(b) = arg(a, 0).map_or(Some(h.current), |v| {
        if is_nil(v) {
            Some(h.current)
        } else {
            h.resolve_buffer(v).filter(|&b| h.buffers[b].name.is_some())
        }
    }) else {
        return Ok(Value::Undef);
    };
    let sel = h.selected_window;
    let scope = match arg(a, 1) {
        None => FrameScope::Frame(h.selected_frame),
        Some(v) if is_nil(v) => FrameScope::Frame(h.selected_frame),
        Some(v) => match h.obj(v) {
            Some(Obj::Frame(f)) => FrameScope::Frame(*f),
            _ => FrameScope::All,
        },
    };
    let start = match scope {
        FrameScope::Frame(f) => h.frames[f].selected,
        _ => h.frames[h.selected_frame].selected,
    };
    let list = window_list_order(h, start, true, &scope);
    let mut best = None;
    for w in list {
        if h.windows[w].mini || h.windows[w].buffer != Some(b) {
            continue;
        }
        if w == sel {
            return Ok(window_obj(h, w));
        }
        best.get_or_insert(w);
    }
    Ok(window_or_nil(h, best))
}

// ── display-related window state of a frame that is never displayed ────────

/// `frame-char-height` / `frame-char-width`: a terminal frame's cell.
fn frame_char_size(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(UNIT))
}
/// `window-fringes` of a terminal window: no fringes.
fn window_fringes(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    let items = vec![Value::Int(0), Value::Int(0), Value::Undef, Value::Undef];
    Ok(h.list_from(items))
}
/// `window-margins`: `(LEFT . RIGHT)`, nil for no margin.
fn window_margins(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let cols = |n: i64| if n == 0 { Value::Undef } else { Value::Int(n) };
    let (l, r) = (h.windows[w].left_margin, h.windows[w].right_margin);
    Ok(h.cons(cols(l), cols(r)))
}
/// `set-window-margins`: t when the margins changed.
fn set_window_margins(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let mut dims = [0i64; 2];
    for (i, d) in dims.iter_mut().enumerate() {
        *d = match arg(a, i + 1) {
            None => 0,
            Some(v) if is_nil(v) => 0,
            Some(Value::Int(n)) if *n >= 0 && *n <= i64::from(i32::MAX) => *n,
            Some(Value::Int(n)) => {
                return Err(format!("args-out-of-range: {n} 0 {}", i32::MAX));
            }
            Some(v) => return Err(h.signal_wrong_type("integerp", v)),
        };
    }
    Ok(nil_or(set_margins(h, w, dims[0], dims[1])))
}
/// `set-window-fringes` and `set-window-scroll-bars` on a terminal window:
/// there are none to change.
fn set_window_no_change(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Undef)
}
fn set_window_vscroll(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
fn set_window_display_table(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(arg(a, 1).cloned().unwrap_or(Value::Undef))
}
/// `window-discard-buffer-from-window`: BUFFER leaves WINDOW's previous
/// and next buffers.
fn window_discard_buffer_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(b) = h.resolve_buffer(&a[0]) else {
        return Err(h.signal_wrong_type("bufferp", &a[0]));
    };
    let w = any_window(h, arg(a, 1))?;
    window_discard_buffer(h, w, b);
    Ok(Value::Undef)
}
/// frame.c `last-nonminibuffer-frame`: the one frame.
fn last_nonminibuffer_frame(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.frames[h.selected_frame].self_obj.clone())
}
/// keyboard.c `posn-at-point`: nothing is displayed in batch.
fn posn_at_point(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Undef)
}
/// `window-scroll-bars` of a terminal window: none, of the frame's type.
fn window_scroll_bars(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    let items = vec![
        Value::Undef,
        Value::Int(0),
        Value::Bool(true),
        Value::Undef,
        Value::Int(0),
        Value::Bool(true),
        Value::Undef,
    ];
    Ok(h.list_from(items))
}
/// `window-text-width`: the text area, which is the body on a terminal.
fn window_text_width(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(body_width(h, w)))
}
fn window_text_height(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    Ok(Value::Int(body_height(h, w)))
}
fn window_old_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let w = any_window(h, arg(a, 0))?;
    Ok(h.windows[w]
        .old_buffer
        .map_or(Value::Undef, |b| buffer_obj(h, b)))
}
/// `window-bump-use-time`.
fn window_bump_use_time(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 0))?;
    let sw = h.selected_window;
    if w != sw && h.windows[sw].use_time == h.window_select_count {
        h.windows[w].use_time = h.window_select_count;
        h.window_select_count += 1;
        h.windows[sw].use_time = h.window_select_count;
        Ok(Value::Int(h.windows[w].use_time))
    } else {
        Ok(Value::Undef)
    }
}
/// The state window change functions record at redisplay, which batch
/// Emacs never runs: no old selected window, old sizes 0.
fn frame_old_selected_window(h: &mut ElispHost, a: &[Value]) -> R {
    frame_or_window_frame(h, arg(a, 0))?;
    Ok(Value::Undef)
}
fn window_old_size(h: &mut ElispHost, a: &[Value]) -> R {
    valid_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
fn window_old_body_size(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
fn window_vscroll(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
fn window_display_table(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Undef)
}
fn minibuffer_selected_window(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Undef)
}
/// `window-at`: the window whose pixel area holds column X, line Y of
/// FRAME (its mode line included), else nil.
fn window_at(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 2))?;
    let coord = |h: &mut ElispHost, v: &Value| match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(x) => Ok(*x),
        v => Err(h.signal_wrong_type("numberp", v)),
    };
    let x = coord(h, &a[0])? * UNIT as f64;
    let y = coord(h, &a[1])? * UNIT as f64;
    let found = frame_windows(h, f).into_iter().find(|&w| {
        let wd = &h.windows[w];
        x >= wd.pixel_left as f64
            && x < (wd.pixel_left + wd.pixel_width) as f64
            && y >= wd.pixel_top as f64
            && y < (wd.pixel_top + wd.pixel_height) as f64
    });
    Ok(window_or_nil(h, found))
}

/// `vmotion (FROM, -N)` on a window that is never displayed: the start of
/// the line N lines above the one holding FROM in buffer B.
fn line_start_back(h: &ElispHost, b: usize, from: usize, n: i64) -> usize {
    let buf = &h.buffers[b];
    let bol = |mut pos: usize| {
        while pos > buf.begv && buf.text[pos - 2] != '\n' {
            pos -= 1;
        }
        pos
    };
    let mut pos = bol(from);
    for _ in 0..n {
        if pos <= buf.begv {
            break;
        }
        pos = bol(pos - 1);
    }
    pos
}

/// `recenter` on the initial terminal frame (window.c's `FRAME_INITIAL_P`
/// branch): the selected window starts ARG lines above point's line (half
/// the window with ARG nil, counted from the bottom when negative).
fn recenter(h: &mut ElispHost, a: &[Value]) -> R {
    let w = h.selected_window;
    let b = h.windows[w].buffer.expect("selected window is live");
    if b != h.current {
        return Err(
            "error: \u{2018}recenter\u{2019}ing a window that does not display current-buffer"
                .into(),
        );
    }
    let ht = body_height(h, w);
    let raw = arg(a, 0).cloned().unwrap_or(Value::Undef);
    let iarg = match &raw {
        v if is_nil(v) => ht / 2,
        Value::Int(n) if *n < 0 => n + ht,
        Value::Int(n) => *n,
        v if matches!(h.obj(v), Some(Obj::Cons(_, _))) => ht / 2,
        v if h.sym_name(v).as_deref() == Some("-") => ht - 1,
        v if h.sym_name(v).is_some() => 1,
        v => return Err(h.signal_wrong_type("fixnump", v)),
    };
    let margin = 0;
    let iarg = iarg.clamp(margin, (ht - margin - 1).max(margin));
    let pt = h.buffers[b].point;
    h.windows[w].start = line_start_back(h, b, pt, iarg);
    Ok(Value::Undef)
}

// ── window configurations ───────────────────────────────────────────────────

/// `save_window_save`: W and its later siblings, each followed by its
/// children, into OUT; INDEX maps a window to its slot there.
fn save_window_save(h: &mut ElispHost, w: Option<usize>, out: &mut Vec<SavedWindow>) {
    let mut cur = w;
    while let Some(w) = cur {
        let persistent = {
            let sym = h.intern("window-persistent-parameters");
            h.get_value(&sym).unwrap_or(Value::Undef)
        };
        let mut params = Vec::new();
        for pers in h.list_vec(&persistent).unwrap_or_default() {
            let Some(Obj::Cons(key, flag)) = h.obj(&pers) else {
                continue;
            };
            if is_nil(flag) {
                continue;
            }
            let key = key.clone();
            let own = h.windows[w].parameters.clone();
            let cell = assq(h, &key, &own);
            let value = match h.obj(&cell) {
                Some(Obj::Cons(_, v)) => v.clone(),
                _ => Value::Undef,
            };
            params.insert(0, h.cons(key, value));
        }
        let parameters = h.list_from(params);
        let wd = &h.windows[w];
        let marks = wd.buffer.map(|b| {
            let point = if w == h.selected_window {
                h.buffers[b].point
            } else {
                wd.pointm
            };
            (point, wd.old_pointm, wd.start)
        });
        let index_of = |x: Option<usize>| x.and_then(|x| out.iter().position(|s| s.window == x));
        let saved = SavedWindow {
            window: w,
            buffer: wd.buffer,
            parent: index_of(wd.parent),
            prev: index_of(wd.prev),
            pixel_left: wd.pixel_left,
            pixel_top: wd.pixel_top,
            pixel_width: wd.pixel_width,
            pixel_height: wd.pixel_height,
            left_col: wd.left_col,
            top_line: wd.top_line,
            total_cols: wd.total_cols,
            total_lines: wd.total_lines,
            normal_cols: wd.normal_cols.clone(),
            normal_lines: wd.normal_lines.clone(),
            hscroll: wd.hscroll,
            left_margin: wd.left_margin,
            right_margin: wd.right_margin,
            dedicated: wd.dedicated.clone(),
            combination_limit: wd.combination_limit.clone(),
            parameters,
            marks,
        };
        let child = wd.child;
        let next = wd.next;
        out.push(saved);
        if child.is_some() {
            save_window_save(h, child, out);
        }
        cur = next;
    }
}

/// `current-window-configuration`: FRAME's window tree with every window's
/// geometry, buffer, point and start, the selected window and the current
/// buffer.
fn current_window_configuration(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let root = h.frames[f].root;
    let mut windows = Vec::new();
    save_window_save(h, Some(root), &mut windows);
    h.window_configs.push(WindowConfig {
        frame: f,
        windows,
        root,
        selected: h.frames[f].selected,
        current_buffer: h.current,
    });
    let id = h.window_configs.len() - 1;
    Ok(h.alloc(Obj::WindowConfig(id)))
}

fn buffer_live(h: &ElispHost, b: usize) -> bool {
    h.buffers[b].name.is_some()
}

fn minibuffer_p(h: &ElispHost, b: usize) -> bool {
    h.buffers[b]
        .name
        .as_ref()
        .is_some_and(|n| n.starts_with(" *Minibuf-"))
}

/// The windows of CONFIGURATION that `set-window-configuration` makes show
/// another buffer: window.c calls `record-window-buffer` for each first.
pub fn windows_changing_buffer(h: &ElispHost, config: &Value) -> Vec<Value> {
    let Some(Obj::WindowConfig(id)) = h.obj(config) else {
        return Vec::new();
    };
    h.window_configs[*id]
        .windows
        .iter()
        .filter(|p| {
            let wd = &h.windows[p.window];
            wd.buffer.is_some()
                && wd.buffer != p.buffer
                && p.buffer
                    .is_some_and(|b| buffer_live(h, b) && !minibuffer_p(h, b))
        })
        .map(|p| window_obj(h, p.window))
        .collect()
}

/// `restrict`: POS clipped to buffer B's accessible portion.
fn restricted(h: &ElispHost, b: usize, pos: usize) -> usize {
    pos.clamp(h.buffers[b].begv, h.buffers[b].zv)
}

/// `set-window-configuration` (window.c), up to the Lisp it leaves to
/// [`after_set_window_configuration`]: every window of the frame is deleted
/// and the saved tree revived with its geometry, buffers and positions; the
/// frame is then resized to its proper layout (`adjust_frame_size` with
/// INHIBIT 4), which puts the root window below the menu bar line.
fn set_window_configuration(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(Obj::WindowConfig(id)) = h.obj(&a[0]) else {
        return Err(h.signal_wrong_type("window-configuration-p", &a[0]));
    };
    let data = h.window_configs[*id].clone();
    let dont_set_miniwindow = arg(a, 2).is_some_and(el_truthy);
    let new_current = Some(data.current_buffer).filter(|&b| buffer_live(h, b));
    let sel = h.selected_window;
    let cw = data.selected;
    let cw_shows = |h: &ElispHost, b: usize| h.windows[cw].buffer == Some(b);
    let old_point = new_current.map(|ncb| {
        if ncb == h.current {
            if cw_shows(h, ncb) && h.windows[sel].buffer == Some(ncb) && sel != cw {
                h.windows[cw].pointm
            } else {
                h.buffers[ncb].point
            }
        } else if cw_shows(h, ncb) && sel != cw {
            h.windows[cw].pointm
        } else {
            h.buffers[ncb].point
        }
    });
    let f = data.frame;
    // Swap out point from the selected window's buffer.
    if let Some(b) = h.windows[sel].buffer {
        h.windows[sel].pointm = h.buffers[b].point;
    }
    let root = h.frames[f].root;
    delete_all_child_windows(h, root);
    let mut dead_windows = Vec::new();
    for k in 0..data.windows.len() {
        let p = data.windows[k].clone();
        let w = p.window;
        h.windows[w].next = None;
        let parent = p.parent.map(|i| data.windows[i].window);
        h.windows[w].parent = parent;
        match p.prev.map(|i| data.windows[i].window) {
            Some(prev) => {
                h.windows[w].prev = Some(prev);
                h.windows[prev].next = Some(w);
            }
            None => {
                h.windows[w].prev = None;
                if let Some(par) = parent {
                    let horizontal = p.total_cols != h.windows[par].total_cols;
                    set_combination(h, par, horizontal, Some(w));
                }
            }
        }
        if let Some(b) = h.windows[w].saved_buffer.take() {
            h.windows[w].buffer = Some(b);
        }
        {
            let wd = &mut h.windows[w];
            wd.pixel_left = p.pixel_left;
            wd.pixel_top = p.pixel_top;
            wd.pixel_width = p.pixel_width;
            wd.pixel_height = p.pixel_height;
            wd.left_col = p.left_col;
            wd.top_line = p.top_line;
            wd.total_cols = p.total_cols;
            wd.total_lines = p.total_lines;
            wd.normal_cols = p.normal_cols.clone();
            wd.normal_lines = p.normal_lines.clone();
            wd.hscroll = p.hscroll;
            wd.left_margin = p.left_margin;
            wd.right_margin = p.right_margin;
            wd.dedicated = p.dedicated.clone();
            wd.combination_limit = p.combination_limit.clone();
        }
        // Saved parameters: a non-nil value is restored, a nil one resets
        // an existing non-nil association only.
        for pers in h.list_vec(&p.parameters).unwrap_or_default() {
            let Some(Obj::Cons(key, value)) = h.obj(&pers) else {
                continue;
            };
            let (key, value) = (key.clone(), value.clone());
            let params = h.windows[w].parameters.clone();
            let cell = assq(h, &key, &params);
            if is_nil(&value) {
                if let Value::Obj(cid) = cell {
                    if let Some(Obj::Cons(_, v)) = h.arena.get_mut(cid as usize) {
                        *v = Value::Undef;
                    }
                }
            } else if let Value::Obj(cid) = cell {
                if let Some(Obj::Cons(_, v)) = h.arena.get_mut(cid as usize) {
                    *v = value;
                }
            } else {
                let pair = h.cons(key, value);
                h.windows[w].parameters = h.cons(pair, params);
            }
        }
        let saved_live = p
            .buffer
            .filter(|&b| buffer_live(h, b))
            .filter(|_| !dont_set_miniwindow || !h.windows[w].mini);
        if let Some(b) = saved_live {
            if h.windows[w].buffer != Some(b) {
                h.windows[w].buffer = Some(b);
                window_discard_buffer(h, w, b);
            }
            let (pointm, old_pointm, start) = p.marks.unwrap_or((1, 1, 1));
            h.windows[w].start = restricted(h, b, start);
            h.windows[w].pointm = restricted(h, b, pointm);
            h.windows[w].old_pointm = restricted(h, b, old_pointm);
            if Some(b) != new_current && b == h.current {
                let pm = h.windows[w].pointm;
                h.buffers[b].point = pm;
            }
        } else if h.windows[w].buffer.is_some_and(|b| buffer_live(h, b)) {
            // The window keeps the buffer it had.
        } else if p.marks.is_some() {
            // A leaf whose buffer is gone gets another one.
            let b = match other_buffer(h, Some(h.current), false) {
                Some(b) => b,
                None => {
                    let handle = h.get_buffer_create("*scratch*");
                    h.resolve_buffer(&handle).expect("fresh *scratch*")
                }
            };
            h.windows[w].buffer = Some(b);
            window_discard_buffer(h, w, b);
            let begv = h.buffers[b].begv;
            h.windows[w].start = begv;
            h.windows[w].pointm = begv;
            h.windows[w].old_pointm = begv;
            if !h.windows[w].mini {
                if !is_nil(&p.dedicated) {
                    dead_windows.push(w);
                }
                h.windows[w].dedicated = Value::Undef;
            }
        }
    }
    h.frames[f].root = data.root;
    if let (Some(ncb), Some(op)) = (new_current, old_point) {
        if cw_shows(h, ncb) {
            h.windows[cw].pointm = restricted(h, ncb, op);
        }
    }
    select_window_swap(h, cw, true, true);
    // `adjust_frame_size (f, -1, -1, 4, …)`.
    let pending_resize = adjust_frame_size(h, f);
    PENDING.with(|pending| {
        *pending.borrow_mut() = Some(Pending {
            frame: f,
            resize: pending_resize,
            dead_windows,
            current_window: cw,
            new_current,
            old_point,
        })
    });
    if !pending_resize.0 && !pending_resize.1 {
        finish_set_window_configuration(h);
    }
    Ok(Value::Bool(true))
}

/// What `set-window-configuration` still has to do in Lisp.
struct Pending {
    frame: usize,
    /// The horizontal and vertical `resize_frame_windows` of a root window
    /// with children: window.el's `window--resize-root-window`.
    resize: (bool, bool),
    dead_windows: Vec<usize>,
    current_window: usize,
    new_current: Option<usize>,
    old_point: Option<usize>,
}

thread_local! {
    static PENDING: std::cell::RefCell<Option<Pending>> = const { std::cell::RefCell::new(None) };
}

/// `adjust_frame_size` with INHIBIT 4 on the terminal frame: the frame
/// keeps its size, and its windows are resized when they do not fill its
/// inner area below the menu bar line. A root window with children is
/// resized in Lisp; the result says which directions are left for that.
fn adjust_frame_size(h: &mut ElispHost, f: usize) -> (bool, bool) {
    let r = h.frames[f].root;
    let m = h.frames[f].mini;
    let old_inner_width = h.windows[r].pixel_width;
    let old_inner_height = h.windows[r].pixel_height + h.windows[m].pixel_height;
    let new_inner_width = FRAME_PIXEL_WIDTH;
    let new_inner_height = FRAME_PIXEL_HEIGHT - TOP_MARGIN;
    if h.windows[r].pixel_top == TOP_MARGIN
        && new_inner_width == old_inner_width
        && new_inner_height == old_inner_height
    {
        return (false, false);
    }
    let mut lisp = (false, false);
    if new_inner_width != old_inner_width {
        lisp.0 = resize_frame_windows(h, f, new_inner_width, true);
    }
    if new_inner_height != old_inner_height || h.windows[r].pixel_top != TOP_MARGIN {
        lisp.1 = resize_frame_windows(h, f, new_inner_height, false);
    }
    lisp
}

/// `resize_frame_windows` up to its Lisp: a leaf root window takes SIZE
/// directly; for one with children the result is true and
/// [`resize_root_window_lisp`] does the rest.
fn resize_frame_windows(h: &mut ElispHost, f: usize, size: i64, horflag: bool) -> bool {
    let r = h.frames[f].root;
    let m = h.frames[f].mini;
    let old = if horflag {
        h.windows[r].pixel_width
    } else {
        h.windows[r].pixel_height
    };
    let mini_height = UNIT + h.windows[m].pixel_height - body_height(h, m);
    let new_pixel = (if horflag { size } else { size - mini_height }).max(UNIT);
    let unchanged = new_pixel == old && (horflag || h.windows[r].pixel_top == TOP_MARGIN);
    if !unchanged {
        if h.windows[r].child.is_none() {
            let wd = &mut h.windows[r];
            if horflag {
                wd.total_cols = new_pixel / UNIT;
                wd.pixel_width = new_pixel;
            } else {
                wd.top_line = TOP_MARGIN;
                wd.pixel_top = TOP_MARGIN;
                wd.total_lines = new_pixel / UNIT;
                wd.pixel_height = new_pixel;
            }
        } else {
            if !horflag {
                h.windows[r].top_line = TOP_MARGIN;
                h.windows[r].pixel_top = TOP_MARGIN;
            }
            return true;
        }
    }
    place_minibuffer_window(h, f, new_pixel, mini_height, horflag);
    false
}

/// The minibuffer window's part of `resize_frame_windows`.
fn place_minibuffer_window(
    h: &mut ElispHost,
    f: usize,
    size: i64,
    mini_height: i64,
    horflag: bool,
) {
    let r = h.frames[f].root;
    let m = h.frames[f].mini;
    if horflag {
        h.windows[m].total_cols = size / UNIT;
        h.windows[m].pixel_width = size;
    } else {
        h.windows[m].total_lines = mini_height / UNIT;
        h.windows[m].pixel_height = mini_height;
        h.windows[m].top_line = h.windows[r].top_line + h.windows[r].total_lines;
        h.windows[m].pixel_top = h.windows[r].pixel_top + h.windows[r].pixel_height;
    }
}

/// The end of `set-window-configuration`: the selected window is selected
/// again recording its buffer, and the buffer current at save time is
/// current again.
fn finish_set_window_configuration(h: &mut ElispHost) {
    let Some(p) = PENDING.with(|pending| pending.borrow_mut().take()) else {
        return;
    };
    if is_live(h, p.current_window) {
        select_window_swap(h, p.current_window, false, false);
    }
    if let Some(ncb) = p.new_current {
        h.current = ncb;
        if h.windows[h.selected_window].buffer != Some(ncb) {
            if let Some(op) = p.old_point {
                h.buffers[ncb].point = restricted(h, ncb, op);
            }
        }
    }
}

/// The Lisp of `set-window-configuration`, run by `crate::modhooks` after
/// the subr: `resize_frame_windows` of a root window with children
/// (`window--resize-root-window`, then `window--pixel-to-total`), the
/// deletion of dedicated windows whose buffer was killed, and the end of
/// the restore.
pub fn after_set_window_configuration() -> R {
    use crate::host::{call_function, with_host};
    let Some((f, resize, dead)) = PENDING.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .map(|p| (p.frame, p.resize, p.dead_windows.clone()))
    }) else {
        return Ok(Value::Undef);
    };
    for (horflag, needed) in [(true, resize.0), (false, resize.1)] {
        if !needed {
            continue;
        }
        let size = if horflag {
            FRAME_PIXEL_WIDTH
        } else {
            FRAME_PIXEL_HEIGHT - TOP_MARGIN
        };
        resize_root_window_lisp(f, size, horflag)?;
    }
    for w in dead {
        let (window, root) = with_host(|h| (window_obj(h, w), h.frames[f].root));
        if with_host(|h| is_live(h, w)) && w != root {
            let deletable = with_host(|h| h.intern("window-deletable-p"));
            if el_truthy(&call_function(&deletable, std::slice::from_ref(&window))?) {
                let delete = with_host(|h| h.intern("delete-window"));
                call_function(&delete, &[window])?;
            }
        }
    }
    with_host(finish_set_window_configuration);
    Ok(Value::Undef)
}

/// `resize_frame_windows` for a root window with children: window.el sizes
/// the tree for the new SIZE (without, then with, minimum-size checks) and
/// the minibuffer window follows.
fn resize_root_window_lisp(f: usize, size: i64, horflag: bool) -> R {
    use crate::host::{call_function, with_host};
    let (root, old, mini_height) = with_host(|h| {
        let r = h.frames[f].root;
        let m = h.frames[f].mini;
        let old = if horflag {
            h.windows[r].pixel_width
        } else {
            h.windows[r].pixel_height
        };
        (r, old, UNIT + h.windows[m].pixel_height - body_height(h, m))
    });
    let new_pixel = (if horflag { size } else { size - mini_height }).max(UNIT);
    let (window, frame, resize, to_total) = with_host(|h| {
        (
            window_obj(h, root),
            h.frames[f].self_obj.clone(),
            h.intern("window--resize-root-window"),
            h.intern("window--pixel-to-total"),
        )
    });
    let hz = if horflag {
        Value::Bool(true)
    } else {
        Value::Undef
    };
    for ignore in [Value::Undef, Value::Bool(true)] {
        call_function(
            &resize,
            &[
                window.clone(),
                Value::Int(new_pixel - old),
                hz.clone(),
                ignore,
                Value::Bool(true),
            ],
        )?;
        let ok = with_host(|h| {
            let ok = window_resize_check(h, root, horflag)
                && int_of(&h.windows[root].new_pixel) == new_pixel;
            if ok {
                window_resize_apply(h, root, horflag);
            }
            ok
        });
        if ok {
            call_function(&to_total, &[frame.clone(), hz.clone()])?;
            break;
        }
    }
    with_host(|h| place_minibuffer_window(h, f, new_pixel, mini_height, horflag));
    Ok(Value::Undef)
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

/// `delete-other-windows-internal` (window.c): WINDOW takes the place of
/// ROOT (the frame's root window by default), resized there by window.el's
/// `window--resize-root-window` when it has children. It calls Lisp, so
/// `crate::modhooks` runs it outside the host borrow; the subr itself only
/// checks its arguments.
pub fn delete_other_windows_internal(args: &[Value]) -> R {
    use crate::host::{call_function, with_host};
    let (w, r) = with_host(|h| -> Result<(usize, usize), String> {
        let w = valid_window(h, arg(args, 0))?;
        let f = h.windows[w].frame;
        let r = match arg(args, 1) {
            Some(v) if !is_nil(v) => {
                let r = valid_window(h, Some(v))?;
                let mut p = h.windows[w].parent;
                while let Some(x) = p {
                    if x == r {
                        break;
                    }
                    p = h.windows[x].parent;
                }
                if p != Some(r) {
                    return Err(
                        "error: Specified root is not an ancestor of specified window".into(),
                    );
                }
                r
            }
            _ => h.frames[f].root,
        };
        Ok((w, r))
    })?;
    if w == r {
        return Ok(Value::Undef);
    }
    if with_host(|h| h.windows[w].mini) {
        return Err("error: Can't expand minibuffer to full frame".into());
    }
    let f = with_host(|h| h.windows[w].frame);
    let mut window = w;
    let mut swindow = w;
    let mut start_top = None;
    if with_host(|h| is_live(h, w)) {
        let (start, top, selected) = with_host(|h| {
            (
                h.windows[w].start,
                h.windows[w].top_line - TOP_MARGIN,
                h.frames[f].selected,
            )
        });
        start_top = Some((start, top));
        if w != selected {
            let obj = with_host(|h| window_obj(h, w));
            let select = with_host(|h| h.intern("select-window"));
            call_function(&select, &[obj])?;
        }
    } else {
        // The selected window, or the last window before it that lies
        // within WINDOW, stays selected.
        swindow = with_host(|h| {
            let mut s = h.frames[f].selected;
            loop {
                let mut p = Some(s);
                while let Some(x) = p {
                    if x == w {
                        return s;
                    }
                    p = h.windows[x].parent;
                }
                let prev = previous_window(h, &[window_obj(h, s), Value::Undef, Value::Undef]);
                match prev.ok().and_then(|v| match h.obj(&v) {
                    Some(Obj::Window(x)) => Some(*x),
                    _ => None,
                }) {
                    Some(x) if x != s => s = x,
                    _ => return s,
                }
            }
        });
        if swindow != with_host(|h| h.frames[f].selected) {
            let obj = with_host(|h| window_obj(h, swindow));
            let select = with_host(|h| h.intern("select-window"));
            call_function(&select, &[obj])?;
        }
    }
    let resize = with_host(|h| h.intern("window--resize-root-window"));
    let to_total = with_host(|h| h.intern("window--pixel-to-total"));
    let mut resize_failed = false;
    if with_host(|h| h.windows[w].child.is_some()) {
        for horflag in [false, true] {
            if resize_failed {
                break;
            }
            let (obj, delta) = with_host(|h| {
                let (rw, ww) = (&h.windows[r], &h.windows[w]);
                let delta = if horflag {
                    rw.pixel_width - ww.pixel_width
                } else {
                    rw.pixel_height - ww.pixel_height
                };
                let (rl, rpl, rt, rpt) = (rw.left_col, rw.pixel_left, rw.top_line, rw.pixel_top);
                let wd = &mut h.windows[w];
                if horflag {
                    wd.left_col = rl;
                    wd.pixel_left = rpl;
                } else {
                    wd.pixel_top = rpt;
                    wd.top_line = rt;
                }
                (window_obj(h, w), delta)
            });
            let hz = if horflag {
                Value::Bool(true)
            } else {
                Value::Undef
            };
            let mut done = false;
            for ignore in [Value::Undef, Value::Bool(true)] {
                call_function(
                    &resize,
                    &[
                        obj.clone(),
                        Value::Int(delta),
                        hz.clone(),
                        ignore,
                        Value::Bool(true),
                    ],
                )?;
                done = with_host(|h| {
                    let ok = window_resize_check(h, w, horflag);
                    if ok {
                        window_resize_apply(h, w, horflag);
                    }
                    ok
                });
                if done {
                    break;
                }
            }
            resize_failed = !done;
        }
        if resize_failed {
            window = swindow;
        }
    }
    with_host(|h| {
        // Unlink WINDOW from its siblings and put it in ROOT's place.
        match h.windows[window].prev {
            Some(s) => {
                let next = h.windows[window].next;
                h.windows[s].next = next;
                if let Some(n) = next {
                    h.windows[n].prev = Some(s);
                }
            }
            None => {
                let s = h.windows[window].next;
                if let Some(s) = s {
                    h.windows[s].prev = None;
                }
                if let Some(p) = h.windows[window].parent {
                    let hz = h.windows[p].horizontal;
                    set_combination(h, p, hz, s);
                }
            }
        }
        if let Some(c) = h.windows[r].child {
            delete_all_child_windows(h, c);
            set_combination(h, r, false, None);
        }
        replace_window(h, r, window, true);
    });
    let frame = with_host(|h| h.frames[f].self_obj.clone());
    call_function(&to_total, &[frame.clone(), Value::Undef])?;
    call_function(&to_total, &[frame, Value::Bool(true)])?;
    if let (Some((start, top)), false) = (start_top, resize_failed) {
        // `vmotion (startpos, -top)`: the window's start moves back as many
        // lines as the window was below the top of the frame.
        with_host(|h| {
            let new_top = h.windows[window].top_line - TOP_MARGIN;
            let Some(b) = h.windows[window].buffer else {
                return;
            };
            let buf = &h.buffers[b];
            if new_top != top && start >= buf.begv && start <= buf.zv {
                h.windows[window].start = line_start_back(h, b, start, top);
            }
        });
    }
    Ok(Value::Undef)
}

/// The subr body of `delete-other-windows-internal`; the work is
/// [`delete_other_windows_internal`]'s.
fn delete_other_windows_internal_subr(h: &mut ElispHost, a: &[Value]) -> R {
    valid_window(h, arg(a, 0))?;
    Err("error: delete-other-windows-internal runs through the change-hook layer".into())
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
    s("previous-window", 0, Some(3), previous_window);
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
    s("window-parent", 0, Some(1), window_parent);
    s("window-top-child", 0, Some(1), window_top_child);
    s("window-left-child", 0, Some(1), window_left_child);
    s("window-next-sibling", 0, Some(1), window_next_sibling);
    s("window-prev-sibling", 0, Some(1), window_prev_sibling);
    s(
        "window-combination-limit",
        1,
        Some(1),
        window_combination_limit,
    );
    s(
        "set-window-combination-limit",
        2,
        Some(2),
        set_window_combination_limit,
    );
    s("window-pixel-width", 0, Some(1), window_pixel_width);
    s("window-pixel-height", 0, Some(1), window_pixel_height);
    s("window-pixel-left", 0, Some(1), window_pixel_left);
    s("window-pixel-top", 0, Some(1), window_pixel_top);
    s("window-new-total", 0, Some(1), window_new_total);
    s("window-new-normal", 0, Some(1), window_new_normal);
    s("window-new-pixel", 0, Some(1), window_new_pixel);
    s("window-normal-size", 0, Some(2), window_normal_size);
    s("set-window-new-pixel", 2, Some(3), set_window_new_pixel);
    s("set-window-new-total", 2, Some(3), set_window_new_total);
    s("set-window-new-normal", 1, Some(2), set_window_new_normal);
    s(
        "window-mode-line-height",
        0,
        Some(1),
        window_mode_line_height,
    );
    s(
        "window-header-line-height",
        0,
        Some(1),
        window_header_line_height,
    );
    s("window-tab-line-height", 0, Some(1), window_tab_line_height);
    s("window-right-divider-width", 0, Some(1), window_zero_width);
    s("window-bottom-divider-width", 0, Some(1), window_zero_width);
    s("window-scroll-bar-width", 0, Some(1), window_zero_width);
    s("window-scroll-bar-height", 0, Some(1), window_zero_width);
    s("frame-char-height", 0, Some(1), frame_char_size);
    s("frame-char-width", 0, Some(1), frame_char_size);
    s("window-fringes", 0, Some(1), window_fringes);
    s("window-margins", 0, Some(1), window_margins);
    s("set-window-margins", 2, Some(3), set_window_margins);
    s("set-window-fringes", 2, Some(5), set_window_no_change);
    s("set-window-scroll-bars", 1, Some(6), set_window_no_change);
    s("set-window-vscroll", 2, Some(4), set_window_vscroll);
    s(
        "set-window-display-table",
        2,
        Some(2),
        set_window_display_table,
    );
    s(
        "window-discard-buffer-from-window",
        2,
        Some(3),
        window_discard_buffer_fn,
    );
    s(
        "last-nonminibuffer-frame",
        0,
        Some(0),
        last_nonminibuffer_frame,
    );
    s("posn-at-point", 0, Some(2), posn_at_point);
    s("window-scroll-bars", 0, Some(1), window_scroll_bars);
    s("window-text-width", 0, Some(2), window_text_width);
    s("window-text-height", 0, Some(2), window_text_height);
    s("window-old-buffer", 0, Some(1), window_old_buffer);
    s("window-bump-use-time", 0, Some(1), window_bump_use_time);
    s(
        "frame-old-selected-window",
        0,
        Some(1),
        frame_old_selected_window,
    );
    s("window-old-pixel-width", 0, Some(1), window_old_size);
    s("window-old-pixel-height", 0, Some(1), window_old_size);
    s(
        "window-old-body-pixel-width",
        0,
        Some(1),
        window_old_body_size,
    );
    s(
        "window-old-body-pixel-height",
        0,
        Some(1),
        window_old_body_size,
    );
    s("window-vscroll", 0, Some(2), window_vscroll);
    s("window-display-table", 0, Some(1), window_display_table);
    s(
        "minibuffer-selected-window",
        0,
        Some(0),
        minibuffer_selected_window,
    );
    s("window-at", 2, Some(3), window_at);
    s("recenter", 0, Some(2), recenter);
    s("window-resize-apply", 0, Some(2), window_resize_apply_fn);
    s(
        "window-resize-apply-total",
        0,
        Some(2),
        window_resize_apply_total_fn,
    );
    s("split-window-internal", 4, Some(5), split_window_internal);
    s("delete-window-internal", 1, Some(1), delete_window_internal);
    s(
        "delete-other-windows-internal",
        0,
        Some(2),
        delete_other_windows_internal_subr,
    );
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

// ── frame.c's geometry of the terminal frame ────────────────────────────────

/// `frame-pixel-width`, `frame-text-width`, `frame-native-width`: 80.
fn frame_pixel_width(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(FRAME_PIXEL_WIDTH))
}
/// `frame-pixel-height`, `frame-text-height`, `frame-native-height`: 25.
fn frame_pixel_height(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(FRAME_PIXEL_HEIGHT))
}
/// Borders, dividers, scroll bars and fringes of a terminal frame: none.
fn frame_zero(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Int(0))
}
/// `frame-parent`: a terminal's root frame has none. term.c's
/// `tty-frame-edges` answers nil for the initial terminal too.
fn frame_parent(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Undef)
}
/// `frame-ancestor-p`: with no child frames, only a frame and itself.
fn frame_ancestor_p(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    let g = live_frame(h, arg(a, 1))?;
    Ok(nil_or(f == g))
}
fn frame_position(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(h.cons(Value::Int(0), Value::Int(0)))
}
/// `frame-current-scroll-bars`: `(VERTICAL . HORIZONTAL)`, none.
fn frame_current_scroll_bars(h: &mut ElispHost, a: &[Value]) -> R {
    frame_or_window_frame(h, arg(a, 0))?;
    Ok(h.cons(Value::Undef, Value::Undef))
}
/// `make-frame-visible`: the terminal frame is always visible.
fn make_frame_visible(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, arg(a, 0))?;
    Ok(h.frames[f].self_obj.clone())
}
fn frame_visible_p(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::Frame(_)) => Ok(Value::Bool(true)),
        _ => Err(h.signal_wrong_type("framep", &a[0])),
    }
}
/// `raise-frame`, `redirect-frame-focus`, `frame-focus`: no window system.
fn frame_nil(h: &mut ElispHost, a: &[Value]) -> R {
    live_frame(h, arg(a, 0))?;
    Ok(Value::Undef)
}
/// `select-frame` (`do_switch_frame`): FRAME's selected window is
/// selected, recorded unless NORECORD.
fn select_frame(h: &mut ElispHost, a: &[Value]) -> R {
    let f = live_frame(h, Some(&a[0]))?;
    let norecord = arg(a, 1).is_some_and(el_truthy);
    h.selected_frame = f;
    let w = h.frames[f].selected;
    select_window_internal(h, w, norecord);
    Ok(h.frames[f].self_obj.clone())
}

/// minibuf.c `active-minibuffer-window`: batch never reads from one.
fn active_minibuffer_window(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Undef)
}

/// Register frame.c's geometry subrs.
pub fn install_frame_geometry(h: &mut ElispHost) {
    let mut s = |n: &str, min: usize, max: Option<usize>, f: crate::host::SubrFn| {
        h.defsubr(n, min, max, f);
    };
    for name in [
        "frame-pixel-width",
        "frame-text-width",
        "frame-native-width",
    ] {
        s(name, 0, Some(1), frame_pixel_width);
    }
    for name in [
        "frame-pixel-height",
        "frame-text-height",
        "frame-native-height",
    ] {
        s(name, 0, Some(1), frame_pixel_height);
    }
    for name in [
        "frame-internal-border-width",
        "frame-right-divider-width",
        "frame-bottom-divider-width",
        "frame-scroll-bar-width",
        "frame-scroll-bar-height",
        "frame-fringe-width",
        "frame-border-width",
    ] {
        s(name, 0, Some(1), frame_zero);
    }
    s("frame-parent", 0, Some(1), frame_parent);
    s("frame-ancestor-p", 2, Some(2), frame_ancestor_p);
    s("frame-position", 0, Some(1), frame_position);
    s(
        "frame-current-scroll-bars",
        0,
        Some(1),
        frame_current_scroll_bars,
    );
    s(
        "active-minibuffer-window",
        0,
        Some(0),
        active_minibuffer_window,
    );
    s("tty-frame-edges", 0, Some(2), frame_parent);
    s("make-frame-visible", 0, Some(1), make_frame_visible);
    s("frame-visible-p", 1, Some(1), frame_visible_p);
    s("raise-frame", 0, Some(1), frame_nil);
    s("frame-focus", 0, Some(1), frame_nil);
    s("redirect-frame-focus", 1, Some(2), frame_nil);
    s("select-frame", 1, Some(2), select_frame);
}

// ── coordinates and visibility on a frame that is never displayed ──────────

/// `coordinates-in-window-p` (`coordinates_in_window` on a terminal frame,
/// which has no dividers, scroll bars, fringes or margins).
fn coordinates_in_window_p(h: &mut ElispHost, a: &[Value]) -> R {
    let w = live_window(h, arg(a, 1))?;
    let Some(Obj::Cons(lx, ly)) = h.obj(&a[0]) else {
        return Err(h.signal_wrong_type("consp", &a[0]));
    };
    let (lx, ly) = (lx.clone(), ly.clone());
    let num = |h: &mut ElispHost, v: &Value| match v {
        Value::Int(n) => Ok(*n),
        Value::Float(x) => Ok(*x as i64),
        v => Err(h.signal_wrong_type("numberp", v)),
    };
    let (x, y) = (num(h, &lx)? * UNIT, num(h, &ly)? * UNIT);
    let wd = &h.windows[w];
    let (left, top) = (wd.pixel_left, wd.pixel_top);
    let (right, bottom) = (left + wd.pixel_width, top + wd.pixel_height);
    if y < top || y >= bottom || x < left || x >= right {
        return Ok(Value::Undef);
    }
    let rightmost = rightmost(h, w);
    let on_border = !rightmost && (x - right).abs() < UNIT;
    let part = if wants_mode_line(h, w) && y >= bottom - UNIT {
        Some("mode-line")
    } else if wants_tab_line(h, w) && y < top + UNIT {
        Some("tab-line")
    } else if wants_header_line(h, w) && y < top + UNIT + i64::from(wants_tab_line(h, w)) * UNIT {
        Some("header-line")
    } else {
        None
    };
    if let Some(part) = part {
        let name = if on_border { "vertical-line" } else { part };
        return Ok(h.intern(name));
    }
    if !rightmost && x > right - 1 - UNIT {
        return Ok(h.intern("vertical-line"));
    }
    Ok(h.cons(Value::Int((x - left) / UNIT), Value::Int((y - top) / UNIT)))
}
/// `pos-visible-in-window-p` and `window-line-height` need a display:
/// batch Emacs answers nil.
fn window_never_displayed(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 1))?;
    Ok(Value::Undef)
}
fn window_line_height(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 1))?;
    Ok(Value::Undef)
}
/// `window-cursor-type`: the window's own cursor type, t by default.
fn window_cursor_type(h: &mut ElispHost, a: &[Value]) -> R {
    live_window(h, arg(a, 0))?;
    Ok(Value::Bool(true))
}
/// terminal.c `terminal-live-p`: the initial terminal (type t) is live.
fn terminal_live_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(match a.first() {
        None => Value::Bool(true),
        Some(v) if is_nil(v) => Value::Bool(true),
        Some(v) => nil_or(matches!(h.obj(v), Some(Obj::Frame(_)))),
    })
}

/// xdisp.c `line-number-display-width`: no line numbers are displayed.
fn line_number_display_width(h: &mut ElispHost, a: &[Value]) -> R {
    let columns = a.first().and_then(|v| h.sym_name(v)).as_deref() == Some("columns");
    Ok(if columns {
        Value::Float(0.0)
    } else {
        Value::Int(0)
    })
}

/// Register the coordinate and visibility subrs.
pub fn install_window_display(h: &mut ElispHost) {
    let mut s = |n: &str, min: usize, max: Option<usize>, f: crate::host::SubrFn| {
        h.defsubr(n, min, max, f);
    };
    s(
        "coordinates-in-window-p",
        2,
        Some(2),
        coordinates_in_window_p,
    );
    s(
        "pos-visible-in-window-p",
        0,
        Some(3),
        window_never_displayed,
    );
    s("window-line-height", 0, Some(2), window_line_height);
    s("window-cursor-type", 0, Some(1), window_cursor_type);
    s("terminal-live-p", 1, Some(1), terminal_live_p);
    s(
        "line-number-display-width",
        0,
        Some(1),
        line_number_display_width,
    );
}
