//! The paragraph direction of bidi.c (`bidi_paragraph_init`), behind
//! xdisp.c's `current-bidi-paragraph-direction`, over the `bidi-class` data
//! of [`crate::bidi_data`].
//!
//! The paragraph holding point is found backwards with
//! `bidi_find_paragraph_start` (`paragraph-start` regexp `^\(\f\|[ \t]*\)$`
//! unless the buffer's `bidi-paragraph-start-re` is a string), and its
//! direction is that of its first strong character (`L` left to right, `R`
//! or `AL` right to left), isolates skipped (UAX#9 P2, P3). A paragraph with
//! no strong character takes the direction of the paragraph before it, the
//! first one in the buffer left to right. Display properties and overlays,
//! which `bidi_fetch_char` consults while displaying, are not looked at.

use crate::bidi_data::{CLASSES, RUNS};
use crate::host::{el_nil, ElispHost, Obj};
use fusevm::Value;

type R = Result<Value, String>;

const L: u8 = 0;
const R_: u8 = 1;
const AL: u8 = 2;
const B: u8 = 10;
const LRI: u8 = 19;
const RLI: u8 = 20;
const FSI: u8 = 21;
const PDI: u8 = 22;

/// `MAX_PARAGRAPH_SEARCH` and `MAX_STRONG_CHAR_SEARCH`.
const MAX_PARAGRAPH_SEARCH: usize = 7500;
const MAX_STRONG_CHAR_SEARCH: usize = 100_000;

/// The `bidi-class` of character C, as an index into [`CLASSES`].
pub fn class_of(c: u32) -> u8 {
    let i = RUNS.partition_point(|&(_, last, _)| last < c);
    RUNS.get(i).map_or(L, |&(_, _, cls)| cls)
}

/// The `bidi-class` symbol name of character C.
pub fn class_name(c: u32) -> &'static str {
    CLASSES[class_of(c) as usize]
}

fn strong(cls: u8) -> bool {
    matches!(cls, L | R_ | AL)
}

/// A buffer's text and the regexps the paragraph search uses.
struct Scan<'a> {
    h: &'a ElispHost,
    text: String,
    chars: Vec<char>,
    /// BEGV and ZV, 1-based.
    begv: usize,
    zv: usize,
    start_re: String,
    separate_re: String,
}

impl Scan<'_> {
    /// The character at POS, `None` at ZV (`BIDI_EOB`).
    fn char_at(&self, pos: usize) -> Option<char> {
        (pos < self.zv).then(|| self.chars[pos - self.begv])
    }

    fn class_at(&self, pos: usize) -> u8 {
        self.char_at(pos).map_or(B, |c| class_of(c as u32))
    }

    /// `fast_looking_at (RE, POS)`: RE matches at POS, without case folding.
    fn looking_at(&self, re: &str, pos: usize) -> bool {
        let Ok(re) = crate::builtins::compile_cf_buf(self.h, re, false) else {
            return false;
        };
        let s = pos - self.begv;
        crate::builtins::match_at(&re, &self.text, s, self.chars.len()).is_some()
    }

    /// `bidi_at_paragraph_end`: >= -1 when POS starts a paragraph
    /// separator or a paragraph.
    fn at_paragraph_end(&self, pos: usize) -> bool {
        self.looking_at(&self.separate_re, pos) || self.looking_at(&self.start_re, pos)
    }

    /// `find_newline_no_quit (POS, -1)`: the position just after the newline
    /// before POS (which search.c calls past it, going backwards), BEGV when
    /// there is none.
    fn newline_before(&self, pos: usize) -> usize {
        let mut p = pos;
        while p > self.begv {
            p -= 1;
            if self.chars[p - self.begv] == '\n' {
                return p + 1;
            }
        }
        self.begv
    }

    /// `bidi_find_paragraph_start`.
    fn paragraph_start(&self, mut pos: usize) -> usize {
        let mut n = 0;
        while pos > self.begv && n < MAX_PARAGRAPH_SEARCH && !self.looking_at(&self.start_re, pos) {
            n += 1;
            pos = self.newline_before(pos - 1);
        }
        if n >= MAX_PARAGRAPH_SEARCH {
            pos = self.begv;
        }
        pos.clamp(self.begv, self.zv)
    }

    /// `bidi_fetch_char_skip_isolates`: the class of the character at POS,
    /// or, for an isolate initiator, of the PDI (or paragraph separator)
    /// that ends it; and the position after it.
    fn fetch_skip_isolates(&self, pos: usize) -> (u8, usize) {
        let mut cls = self.class_at(pos);
        let mut next = pos + 1;
        if matches!(cls, LRI | RLI | FSI) {
            let mut level = 1;
            while level > 0 && cls != B {
                cls = self.class_at(next);
                next += 1;
                if matches!(cls, LRI | RLI | FSI) {
                    level += 1;
                } else if cls == PDI {
                    level -= 1;
                }
            }
        }
        (cls, next)
    }

    /// `find_first_strong_char` from POS.
    fn first_strong(&self, pos: usize) -> u8 {
        let (mut cls, mut next) = self.fetch_skip_isolates(pos);
        let pos1 = pos;
        while !strong(cls) && next - pos1 < MAX_STRONG_CHAR_SEARCH {
            if next >= self.zv {
                return B;
            }
            if cls == B && self.at_paragraph_end(next) {
                break;
            }
            let (c, n) = self.fetch_skip_isolates(next);
            cls = c;
            next = n;
        }
        cls
    }

    /// `bidi_paragraph_init (NEUTRAL_DIR, …, true)` at POS: true for right
    /// to left.
    fn paragraph_r2l(&self, mut pos: usize) -> bool {
        if pos == self.begv && pos == self.zv {
            return false;
        }
        if pos > self.begv && self.char_at(pos) == Some('\n') {
            pos += 1;
        }
        let mut pstart = self.paragraph_start(pos);
        loop {
            match self.first_strong(pstart) {
                R_ | AL => return true,
                L => return false,
                _ if pstart == self.begv => return false,
                _ => {
                    let mut p = pstart;
                    let mut prev = pstart;
                    while p > self.begv && prev >= pstart {
                        p -= 1;
                        prev = self.paragraph_start(p);
                    }
                    pstart = prev;
                }
            }
        }
    }
}

/// `(current-bidi-paragraph-direction &optional BUFFER)` — xdisp.c.
pub fn current_bidi_paragraph_direction(h: &mut ElispHost, a: &[Value]) -> R {
    let saved = h.current;
    let b = match a.first() {
        Some(v) if !el_nil(v) => match h.obj(v) {
            Some(Obj::Buffer(b)) => *b,
            _ => return Err(h.signal_wrong_type("bufferp", v)),
        },
        _ => h.current,
    };
    // A per-buffer variable of buffer B; one elisprs leaves void counts as
    // non-nil (`enable-multibyte-characters' is t in every buffer here).
    let var = |h: &mut ElispHost, name: &str| {
        let sym = h.intern(name);
        h.buffer_local_or_default(&sym, b)
            .unwrap_or(Value::Bool(name == "enable-multibyte-characters"))
    };
    let l2r = h.intern("left-to-right");
    if el_nil(&var(h, "bidi-display-reordering")) || el_nil(&var(h, "enable-multibyte-characters"))
    {
        return Ok(l2r);
    }
    let fixed = var(h, "bidi-paragraph-direction");
    if !el_nil(&fixed) {
        return Ok(fixed);
    }
    let re_var = |h: &mut ElispHost, name: &str, default: &str| match var(h, name) {
        v if h.is_string(&v) => h.str_text(&v).unwrap_or_default().to_string(),
        _ => default.to_string(),
    };
    let start_re = re_var(h, "bidi-paragraph-start-re", "^\\(\u{c}\\|[ \t]*\\)$");
    let separate_re = re_var(h, "bidi-paragraph-separate-re", "^[ \t\u{c}]*$");
    h.current = b;
    let r2l = {
        let buf = h.cur_buf_ref();
        let (begv, zv, point) = (buf.begv, buf.zv, buf.point);
        let chars: Vec<char> = buf.text[begv - 1..zv - 1].to_vec();
        let scan = Scan {
            h,
            text: chars.iter().collect(),
            chars,
            begv,
            zv,
            start_re,
            separate_re,
        };
        let mut pos = point;
        if pos >= zv && pos > begv {
            pos -= 1;
        }
        if scan.looking_at("[\u{c}\t ]*\n", pos) {
            while matches!(scan.char_at(pos), Some('\n' | ' ' | '\t' | '\u{c}')) {
                if pos <= begv {
                    break;
                }
                pos -= 1;
            }
        }
        scan.paragraph_r2l(pos)
    };
    h.current = saved;
    Ok(if r2l { h.intern("right-to-left") } else { l2r })
}
