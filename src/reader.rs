//! Elisp reader. Builds forms directly as ElispHost heap objects (cons cells,
//! symbols) so the compiler can walk them and quoted literals can be emitted as
//! `Value::Obj` constants.
//!
//! nil → fusevm `Undef`, t → fusevm `T`. Reads: ints/floats/strings, symbols
//! (incl. `1+`, `<=`, `:keywords`), `'quote`, `#'function`, `?c` char literals,
//! `;` comments, backquote/unquote (`` ` `` `,` `,@`), and dotted pairs (`a . b`).

use crate::host::{ElispHost, Obj};
use fusevm::Value;
use num_bigint::BigInt;

pub fn read_all(h: &mut ElispHost, src: &str) -> Result<Vec<Value>, String> {
    let mut r = Reader {
        chars: src.chars().collect(),
        pos: 0,
    };
    let mut out = Vec::new();
    loop {
        r.skip_ws();
        if r.pos >= r.chars.len() {
            break;
        }
        out.push(r.read_form(h)?);
    }
    Ok(out)
}

/// Read a single form starting at char index `start`, returning it together
/// with the char index just past it (for `read-from-string`).
pub fn read_one(h: &mut ElispHost, src: &str, start: usize) -> Result<(Value, usize), String> {
    let mut r = Reader {
        chars: src.chars().collect(),
        pos: start.min(src.chars().count()),
    };
    r.skip_ws();
    if r.pos >= r.chars.len() {
        return Err("end-of-file".into());
    }
    let form = r.read_form(h)?;
    Ok((form, r.pos))
}

struct Reader {
    chars: Vec<char>,
    pos: usize,
}

fn is_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '"' | '\'' | '`' | ',' | ';')
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }
    /// With the current char a `.`, is it the dotted-pair separator rather than
    /// the start of a symbol/number? Emacs (`lread.c` `read1`) treats `.` as the
    /// separator only when the following char is end-of-input, whitespace, or one
    /// of `"';([#?` `` ` `` `,` — notably NOT `)`/`]`, so `(a .)` and `(.)` read
    /// the `.` as the symbol `\.`, while `.5`/`...`/`a.b` stay atoms.
    fn dot_is_separator(&self) -> bool {
        match self.peek_at(1) {
            None => true,
            Some(c) => {
                c.is_whitespace()
                    || matches!(c, '"' | '\'' | ';' | '(' | '[' | '#' | '?' | '`' | ',')
            }
        }
    }
    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.pos += 1,
                Some(';') => {
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// Read one form. Recursive descent nests one Rust frame per `(`, `[`, `'`,
    /// etc., so deeply-nested input (`((((…))))` thousands deep) would overflow
    /// the fixed OS thread stack. `stacker::maybe_grow` extends the stack on the
    /// same thread when it runs low, so the reader stays unbounded like Emacs's
    /// (which reads 500k-deep nesting without error) while keeping the
    /// thread-local host reachable.
    fn read_form(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        stacker::maybe_grow(128 * 1024, 16 * 1024 * 1024, || self.read_form_inner(h))
    }

    fn read_form_inner(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        self.skip_ws();
        let c = self.peek().ok_or("unexpected end of input")?;
        match c {
            '(' => self.read_list(h),
            ')' => Err("unexpected )".to_string()),
            // A separator `.` reaching `read_form` is misplaced: the valid dotted
            // position is handled inside `read_list`. Top-level `(read ".")`, a
            // vector element `[a . b]`, or a quoted `'.` all signal invalid syntax.
            '.' if self.dot_is_separator() => Err("invalid-read-syntax: .".to_string()),
            '[' => self.read_vector(h),
            ']' => Err("unexpected ]".to_string()),
            '`' => {
                self.pos += 1;
                let f = self.read_form(h)?;
                Ok(bq_expand(h, &f))
            }
            ',' => {
                self.pos += 1;
                if self.peek() == Some('@') {
                    self.pos += 1;
                    let f = self.read_form(h)?;
                    Ok(marker(h, "unquote-splicing", f))
                } else {
                    let f = self.read_form(h)?;
                    Ok(marker(h, "unquote", f))
                }
            }
            '"' => self.read_string(),
            '?' => self.read_char_literal(),
            '\'' => {
                self.pos += 1;
                let f = self.read_form(h)?;
                Ok(quoted(h, "quote", f))
            }
            '#' if self.peek_at(1) == Some('\'') => {
                self.pos += 2;
                let f = self.read_form(h)?;
                Ok(quoted(h, "function", f))
            }
            // #s(hash-table …) / #s(RECORD …) literal.
            '#' if self.peek_at(1) == Some('s') && self.peek_at(2) == Some('(') => {
                self.pos += 2;
                self.read_record(h)
            }
            // #("string" …intervals) — read the string, dropping text properties.
            '#' if self.peek_at(1) == Some('(') => {
                self.pos += 1;
                let lst = self.read_list(h)?;
                Ok(h.list_vec(&lst)
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or(Value::Undef))
            }
            '#' if matches!(
                self.peek_at(1),
                Some('x' | 'X' | 'o' | 'O' | 'b' | 'B' | '0'..='9')
            ) =>
            {
                self.read_radix(h)
            }
            _ => self.read_atom(h),
        }
    }

    fn read_list(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        self.pos += 1; // consume '('
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err("unclosed list".to_string()),
                Some(')') => {
                    self.pos += 1;
                    break;
                }
                Some('.') if self.dot_is_separator() => {
                    // dotted tail: (a b . c). A separator `.` with no preceding car
                    // (`(. a)`) is invalid read syntax; so is a missing cdr (`(a . )`)
                    // or a second `.`/extra form before the close (`(a . b . c)`).
                    if items.is_empty() {
                        return Err("invalid-read-syntax: .".to_string());
                    }
                    self.pos += 1; // consume '.'
                    self.skip_ws();
                    if self.peek() == Some(')') {
                        return Err("invalid-read-syntax: )".to_string());
                    }
                    let tail = self.read_form(h)?;
                    self.skip_ws();
                    if self.peek() != Some(')') {
                        return Err("invalid-read-syntax: expected )".to_string());
                    }
                    self.pos += 1;
                    let mut acc = tail;
                    for x in items.into_iter().rev() {
                        acc = h.cons(x, acc);
                    }
                    return Ok(acc);
                }
                _ => items.push(self.read_form(h)?),
            }
        }
        Ok(h.list_from(items))
    }

    fn read_vector(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err("unclosed vector".to_string()),
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                // A vector literal is self-evaluating; its elements are read
                // verbatim (not evaluated), matching elisp `[a b c]` semantics.
                _ => items.push(self.read_form(h)?),
            }
        }
        Ok(h.alloc(Obj::Vector(items)))
    }

    fn read_string(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".to_string()),
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.pos += 1;
                    let e = self.peek().ok_or("unterminated string")?;
                    let dash = self.peek_at(1) == Some('-');
                    let scalar = match e {
                        // Control: `\^X` and `\C-X` fold like char literals. The
                        // target is read as a plain char (or nested escape), not
                        // re-interpreted as an escape letter (so `\^a` is C-a, not
                        // control of the bell `\a`).
                        '^' => {
                            self.pos += 1;
                            apply_control(self.read_control_target()?)
                        }
                        'C' if dash => {
                            self.pos += 2;
                            apply_control(self.read_control_target()?)
                        }
                        // `\s` (not `\s-`) is the space character.
                        's' if !dash => {
                            self.pos += 1;
                            32
                        }
                        // A literal newline after `\` is elided (line continuation).
                        '\n' => {
                            self.pos += 1;
                            continue;
                        }
                        _ => self.read_escape_scalar()?,
                    };
                    match char::from_u32(scalar as u32) {
                        Some(c) => s.push(c),
                        None => return Err(format!("invalid character code {scalar} in string")),
                    }
                }
                Some(c) => {
                    self.pos += 1;
                    s.push(c);
                }
            }
        }
        Ok(Value::str(s))
    }

    fn read_char_literal(&mut self) -> Result<Value, String> {
        self.pos += 1; // consume '?'
        Ok(Value::Int(self.read_char_spec()?))
    }

    /// Read one character specification (after `?`), honoring the modifier
    /// prefixes `\C-` / `\^` (control), `\M-` (meta), `\S-` (shift), `\H-`,
    /// `\s-`, `\A-`, which may nest: `?\C-\M-a` => control+meta a.
    fn read_char_spec(&mut self) -> Result<i64, String> {
        let c = self.peek().ok_or("unterminated char literal")?;
        self.pos += 1;
        if c != '\\' {
            return Ok(c as i64);
        }
        let e = self.peek().ok_or("unterminated char literal")?;
        let dash = self.peek_at(1) == Some('-');
        match e {
            'C' if dash => {
                self.pos += 2;
                Ok(apply_control(self.read_char_spec()?))
            }
            '^' => {
                self.pos += 1;
                Ok(apply_control(self.read_char_spec()?))
            }
            'M' if dash => {
                self.pos += 2;
                Ok(self.read_char_spec()? | C_META)
            }
            'S' if dash => {
                self.pos += 2;
                Ok(self.read_char_spec()? | C_SHIFT)
            }
            'H' if dash => {
                self.pos += 2;
                Ok(self.read_char_spec()? | C_HYPER)
            }
            's' if dash => {
                self.pos += 2;
                Ok(self.read_char_spec()? | C_SUPER)
            }
            'A' if dash => {
                self.pos += 2;
                Ok(self.read_char_spec()? | C_ALT)
            }
            // `?\s` not followed by `-` is the space character.
            's' => {
                self.pos += 1;
                Ok(32)
            }
            // `\xHH…` / `\uHHHH` / `\U00HHHHHH` / octal `\OOO` / named escapes.
            _ => self.read_escape_scalar(),
        }
    }

    /// Read a numeric/named escape, `self.pos` pointing at the char right after
    /// the backslash: hex `xHH…`, unicode `uHHHH` / `U00HHHHHH`, octal `OOO`
    /// (1–3 digits), or a single-letter escape via `unescape`.
    fn read_escape_scalar(&mut self) -> Result<i64, String> {
        let e = self.peek().ok_or("unterminated escape")?;
        match e {
            'x' => {
                self.pos += 1;
                let mut n: i64 = 0;
                let mut any = false;
                while let Some(d) = self.peek().and_then(|c| c.to_digit(16)) {
                    n = n * 16 + d as i64;
                    self.pos += 1;
                    any = true;
                }
                if !any {
                    return Err("missing hex digits after \\x".to_string());
                }
                Ok(n)
            }
            'u' => {
                self.pos += 1;
                self.read_fixed_hex(4)
            }
            'U' => {
                self.pos += 1;
                self.read_fixed_hex(8)
            }
            // \N{U+HHHH} codepoint escape (the named-char form needs a name table).
            'N' if self.peek_at(1) == Some('{') => {
                self.pos += 2;
                let start = self.pos;
                while self.peek().is_some() && self.peek() != Some('}') {
                    self.pos += 1;
                }
                let name: String = self.chars[start..self.pos].iter().collect();
                if self.peek() == Some('}') {
                    self.pos += 1;
                }
                match name.trim().strip_prefix("U+") {
                    Some(hex) => i64::from_str_radix(hex.trim(), 16)
                        .map_err(|_| format!("invalid \\N{{{name}}}")),
                    None => Err(format!("unsupported character name: {name}")),
                }
            }
            '0'..='7' => {
                let mut n: i64 = 0;
                let mut count = 0;
                while count < 3 {
                    match self.peek().and_then(|c| c.to_digit(8)) {
                        Some(d) => {
                            n = n * 8 + d as i64;
                            self.pos += 1;
                            count += 1;
                        }
                        None => break,
                    }
                }
                Ok(n)
            }
            other => {
                self.pos += 1;
                Ok(unescape(other) as i64)
            }
        }
    }

    /// Read the target of a control prefix (`\^`/`\C-`): a plain character, or a
    /// nested `\…` escape if the next char is a backslash.
    fn read_control_target(&mut self) -> Result<i64, String> {
        let nc = self.peek().ok_or("unterminated escape")?;
        if nc == '\\' {
            self.pos += 1;
            self.read_escape_scalar()
        } else {
            self.pos += 1;
            Ok(nc as i64)
        }
    }

    /// Read exactly `n` hex digits (for `\u` / `\U`), erroring if any is missing.
    fn read_fixed_hex(&mut self, n: usize) -> Result<i64, String> {
        let mut val: i64 = 0;
        for _ in 0..n {
            let d = self
                .peek()
                .and_then(|c| c.to_digit(16))
                .ok_or("missing hex digits in unicode escape")?;
            val = val * 16 + d as i64;
            self.pos += 1;
        }
        Ok(val)
    }

    /// Read a radix-prefixed integer: `#x1f` / `#b101` / `#o17` (and uppercase),
    /// or the general `#NNr…` form (e.g. `#16rFF`). An optional sign may follow
    /// the prefix.
    fn read_radix(&mut self, _h: &mut ElispHost) -> Result<Value, String> {
        self.pos += 1; // consume '#'
        let c = self.peek().ok_or("unterminated radix literal")?;
        let base: u32 = match c {
            'x' | 'X' => {
                self.pos += 1;
                16
            }
            'o' | 'O' => {
                self.pos += 1;
                8
            }
            'b' | 'B' => {
                self.pos += 1;
                2
            }
            '0'..='9' => {
                let mut n = 0u32;
                while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
                    n = n * 10 + d;
                    self.pos += 1;
                }
                match self.peek() {
                    Some('r') | Some('R') => self.pos += 1,
                    _ => return Err("malformed radix literal (expected `r`)".to_string()),
                }
                if !(2..=36).contains(&n) {
                    return Err(format!("invalid radix {n}"));
                }
                n
            }
            _ => return Err(format!("unsupported reader macro #{c}")),
        };
        let mut sign = 1i64;
        match self.peek() {
            Some('+') => self.pos += 1,
            Some('-') => {
                sign = -1;
                self.pos += 1;
            }
            _ => {}
        }
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_delim(c) {
                break;
            }
            self.pos += 1;
        }
        let tok: String = self.chars[start..self.pos].iter().collect();
        let n = i64::from_str_radix(&tok, base)
            .map_err(|_| format!("invalid digits for base {base}: {tok}"))?;
        Ok(Value::Int(sign * n))
    }

    /// Read a `#s(…)` literal: a hash-table (`#s(hash-table test … data (k v …))`)
    /// or a record (`#s(NAME slot…)`, stored as a `cl-struct-NAME`-tagged vector).
    /// `self.pos` is at the `(`.
    fn read_record(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        let lst = self.read_list(h)?;
        let items = h.list_vec(&lst).ok_or("malformed #s(...) literal")?;
        if items.is_empty() {
            return Err("empty #s(...) literal".to_string());
        }
        if h.sym_name(&items[0]).as_deref() == Some("hash-table") {
            let mut test = 1u8; // eql
            let mut data: Vec<Value> = Vec::new();
            let mut i = 1;
            while i + 1 < items.len() {
                match h.sym_name(&items[i]).as_deref() {
                    Some("test") => {
                        test = match h.sym_name(&items[i + 1]).as_deref() {
                            Some("eq") => 0,
                            Some("equal") => 2,
                            _ => 1,
                        };
                    }
                    Some("data") => data = h.list_vec(&items[i + 1]).unwrap_or_default(),
                    _ => {}
                }
                i += 2;
            }
            let mut entries = Vec::new();
            let mut j = 0;
            while j + 1 < data.len() {
                entries.push((data[j].clone(), data[j + 1].clone()));
                j += 2;
            }
            Ok(h.alloc(Obj::HashTable { test, entries }))
        } else {
            let name = h
                .sym_name(&items[0])
                .ok_or("#s record type must be a symbol")?;
            let tag = h.intern(&format!("cl-struct-{name}"));
            let mut v = vec![tag];
            v.extend_from_slice(&items[1..]);
            Ok(h.alloc(Obj::Vector(v)))
        }
    }

    fn read_atom(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        let mut tok = String::new();
        // A `\` escapes the next char into the symbol name (so `foo\ bar`, `\,`,
        // `\#x` are single symbols). Any escape also forces the token to be a
        // symbol — never a number or `nil`/`t`.
        let mut had_escape = false;
        while let Some(c) = self.peek() {
            if c == '\\' {
                self.pos += 1;
                match self.peek() {
                    Some(e) => {
                        tok.push(e);
                        self.pos += 1;
                        had_escape = true;
                    }
                    None => return Err("trailing backslash in symbol".to_string()),
                }
            } else if is_delim(c) {
                break;
            } else {
                tok.push(c);
                self.pos += 1;
            }
        }
        if had_escape {
            Ok(h.intern(&tok))
        } else {
            Ok(classify(h, &tok))
        }
    }
}

fn quoted(h: &mut ElispHost, head: &str, form: Value) -> Value {
    let q = h.intern(head);
    h.list_from(vec![q, form])
}

/// Build `(NAME FORM)` — the internal unquote / unquote-splicing markers.
fn marker(h: &mut ElispHost, name: &str, form: Value) -> Value {
    let s = h.intern(name);
    h.list_from(vec![s, form])
}

/// Recognize an unquote marker: returns ("unquote"|"unquote-splicing", payload).
fn unquote_kind(h: &ElispHost, e: &Value) -> Option<(String, Value)> {
    let v = h.list_vec(e)?;
    if v.len() == 2 {
        if let Some(name) = h.sym_name(&v[0]) {
            if name == "unquote" || name == "unquote-splicing" {
                return Some((name, v[1].clone()));
            }
        }
    }
    None
}

fn call_form(h: &mut ElispHost, fname: &str, a: Value, b: Value) -> Value {
    let f = h.intern(fname);
    h.list_from(vec![f, a, b])
}

/// Expand `` `FORM `` at read time into `cons`/`append`/`quote` calls (the
/// standard backquote decomposition from the manual). The result is ordinary
/// elisp that builds the templated structure at run time.
fn bq_expand(h: &mut ElispHost, form: &Value) -> Value {
    // `,x  →  x   (a top-level unquote)
    if let Some((kind, payload)) = unquote_kind(h, form) {
        if kind == "unquote" {
            return payload;
        }
        // `,@x at top level is ill-formed; fall through to quoting.
    }
    // A (possibly dotted) list: walk the cons spine collecting elements, then
    // fold right. The tail may be nil (proper list), a `,x` unquote in the dotted
    // position (`(a . ,x) -> the final cdr is x), or another atom.
    if matches!(h.obj(form), Some(Obj::Cons(..))) {
        let mut elems: Vec<Value> = Vec::new();
        let mut cur = form.clone();
        let tail;
        loop {
            match h.obj(&cur) {
                Some(Obj::Cons(car, cdr)) => {
                    let (car, cdr) = (car.clone(), cdr.clone());
                    // A `,x in the dotted-cdr position becomes the final cdr.
                    if let Some((kind, payload)) = unquote_kind(h, &cdr) {
                        if kind == "unquote" {
                            elems.push(car);
                            tail = payload;
                            break;
                        }
                    }
                    elems.push(car);
                    cur = cdr;
                }
                _ => {
                    tail = if matches!(cur, Value::Undef) {
                        Value::Undef
                    } else {
                        bq_expand(h, &cur)
                    };
                    break;
                }
            }
        }
        let mut rest = tail;
        for e in elems.iter().rev() {
            match unquote_kind(h, e) {
                Some((kind, payload)) if kind == "unquote-splicing" => {
                    rest = call_form(h, "append", payload, rest);
                }
                Some((_unquote, payload)) => {
                    rest = call_form(h, "cons", payload, rest);
                }
                None => {
                    let sub = bq_expand(h, e);
                    rest = call_form(h, "cons", sub, rest);
                }
            }
        }
        return rest;
    }
    // A vector template `[…]: fold the elements like a list, then `vconcat' the
    // resulting list back into a vector. (`pcase--compile' recognises the
    // `vconcat' head as a vector pattern.)
    if let Some(Obj::Vector(items)) = h.obj(form) {
        let items = items.clone();
        let mut rest = Value::Undef;
        for e in items.iter().rev() {
            match unquote_kind(h, e) {
                Some((kind, payload)) if kind == "unquote-splicing" => {
                    rest = call_form(h, "append", payload, rest);
                }
                Some((_unquote, payload)) => {
                    rest = call_form(h, "cons", payload, rest);
                }
                None => {
                    let sub = bq_expand(h, e);
                    rest = call_form(h, "cons", sub, rest);
                }
            }
        }
        let f = h.intern("vconcat");
        return h.list_from(vec![f, rest]);
    }
    // Atom: symbols must be quoted; self-evaluating atoms can stand as-is.
    match form {
        Value::Obj(_) => quoted(h, "quote", form.clone()),
        _ => form.clone(),
    }
}

// Emacs character modifier bits (see `Character Type` in the manual).
const C_META: i64 = 1 << 27;
const C_CTL: i64 = 1 << 26;
const C_SHIFT: i64 = 1 << 25;
const C_HYPER: i64 = 1 << 24;
const C_SUPER: i64 = 1 << 23;
const C_ALT: i64 = 1 << 22;
const C_MODMASK: i64 = C_META | C_CTL | C_SHIFT | C_HYPER | C_SUPER | C_ALT;

/// Apply the control modifier to a (possibly already modified) character: fold
/// ASCII letters / `@A-Z[\]^_` into 0–31, `?` into 127, and otherwise set the
/// control bit. Any non-control modifier bits already present are preserved.
fn apply_control(c: i64) -> i64 {
    let mods = c & C_MODMASK;
    let base = c & !C_MODMASK;
    let ctrl = if base == '?' as i64 {
        127
    } else if (b'a' as i64..=b'z' as i64).contains(&base) {
        base - b'a' as i64 + 1
    } else if (64..=95).contains(&base) {
        base ^ 64
    } else {
        return c | C_CTL;
    };
    ctrl | mods
}

fn unescape(e: char) -> char {
    match e {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        'e' => '\u{1b}', // escape
        'a' => '\u{7}',  // bell
        'b' => '\u{8}',  // backspace
        'v' => '\u{b}',  // vertical tab
        'f' => '\u{c}',  // formfeed
        'd' => '\u{7f}', // delete
        other => other,
    }
}

fn classify(h: &mut ElispHost, tok: &str) -> Value {
    match tok {
        "nil" => return Value::Undef,
        "t" => return Value::Bool(true),
        _ => {}
    }
    if !tok.starts_with(':') {
        if let Ok(i) = tok.parse::<i64>() {
            return h.make_integer(BigInt::from(i));
        }
        // Too big for an i64 but still all digits: a bignum, exactly as in Emacs.
        // Reading it as an f64 (the `looks_numeric` fallback below) would both
        // lose precision and change the type from integer to float.
        if let Ok(b) = tok.parse::<BigInt>() {
            return h.make_integer(b);
        }
        // A trailing decimal point with no fractional digits is an integer in
        // elisp: `1.` => 1, `-3.` => -3 (but `1.5`/`1.e3` are floats).
        if let Some(intpart) = tok.strip_suffix('.') {
            if let Ok(i) = intpart.parse::<i64>() {
                return Value::Int(i);
            }
            if let Ok(b) = intpart.parse::<BigInt>() {
                return h.make_integer(b);
            }
        }
        if looks_numeric(tok) {
            if let Ok(f) = tok.parse::<f64>() {
                return Value::Float(f);
            }
        }
        // Emacs read syntax for the non-finite floats: a float mantissa followed
        // by `e+INF` or `e+NaN` (e.g. `1.0e+INF`, `-0.0e+NaN`).
        if let Some(mant) = tok.strip_suffix("e+INF") {
            if mant.parse::<f64>().is_ok() {
                let inf = if mant.starts_with('-') {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
                return Value::Float(inf);
            }
        }
        if let Some(mant) = tok.strip_suffix("e+NaN") {
            if mant.parse::<f64>().is_ok() {
                return Value::Float(if mant.starts_with('-') {
                    -f64::NAN
                } else {
                    f64::NAN
                });
            }
        }
    }
    h.intern(tok)
}

/// Does this token read back as an elisp number (integer or float)? Used by the
/// printer to decide whether a symbol name needs a leading escape.
pub(crate) fn token_is_number(tok: &str) -> bool {
    if tok.parse::<i64>().is_ok() {
        return true;
    }
    looks_numeric(tok) && tok.parse::<f64>().is_ok()
}

fn looks_numeric(tok: &str) -> bool {
    let mut seen_digit = false;
    for c in tok.chars() {
        match c {
            '0'..='9' => seen_digit = true,
            '+' | '-' | '.' | 'e' | 'E' => {}
            _ => return false,
        }
    }
    seen_digit
}
