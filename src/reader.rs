//! Elisp reader. Builds forms directly as ElispHost heap objects (cons cells,
//! symbols) so the compiler can walk them and quoted literals can be emitted as
//! `Value::Obj` constants.
//!
//! nil → fusevm `Undef`, t → fusevm `T`. Reads: ints/floats/strings, symbols
//! (incl. `1+`, `<=`, `:keywords`), `'quote`, `#'function`, `?c` char literals,
//! `;` comments, backquote/unquote (`` ` `` `,` `,@`), and dotted pairs (`a . b`).

use crate::host::{ElispHost, Obj};
use fusevm::Value;

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

    fn read_form(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        self.skip_ws();
        let c = self.peek().ok_or("unexpected end of input")?;
        match c {
            '(' => self.read_list(h),
            ')' => Err("unexpected )".to_string()),
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
                Some('.') if self.peek_at(1).map(is_delim).unwrap_or(true) => {
                    // dotted tail: (a b . c)
                    self.pos += 1;
                    let tail = self.read_form(h)?;
                    self.skip_ws();
                    if self.peek() != Some(')') {
                        return Err("malformed dotted list".to_string());
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
                    self.pos += 1;
                    s.push(unescape(e));
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
            _ => {
                self.pos += 1;
                Ok(unescape(e) as i64)
            }
        }
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

    fn read_atom(&mut self, h: &mut ElispHost) -> Result<Value, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_delim(c) {
                break;
            }
            self.pos += 1;
        }
        let tok: String = self.chars[start..self.pos].iter().collect();
        Ok(classify(h, &tok))
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
    // A proper list: fold right, splicing where `,@` appears.
    if let Some(elems) = h.list_vec(form) {
        // (unquote x) was handled above; here elems are template elements.
        let mut rest = Value::Undef; // nil
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
        'e' => '\u{1b}',
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
            return Value::Int(i);
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
