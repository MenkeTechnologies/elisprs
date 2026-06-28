//! Elisp reader. Builds forms directly as ElispHost heap objects (cons cells,
//! symbols) so the compiler can walk them and quoted literals can be emitted as
//! `Value::Obj` constants.
//!
//! nil → fusevm `Undef`, t → fusevm `T`. Milestone scope: ints/floats/strings,
//! symbols (incl. `1+`, `<=`, `:keywords`), `'quote`, `#'function`, `?c` char
//! literals, `;` comments. (Backquote and true dotted-pair reading land next.)

use crate::host::ElispHost;
use fusevm::Value;

pub fn read_all(h: &mut ElispHost, src: &str) -> Result<Vec<Value>, String> {
    let mut r = Reader { chars: src.chars().collect(), pos: 0 };
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
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\'' | '`' | ',' | ';')
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
            '`' | ',' => Err("backquote/unquote not supported yet".to_string()),
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
        self.pos += 1;
        let c = self.peek().ok_or("unterminated char literal")?;
        self.pos += 1;
        let ch = if c == '\\' {
            let e = self.peek().ok_or("unterminated char literal")?;
            self.pos += 1;
            unescape(e)
        } else {
            c
        };
        Ok(Value::Int(ch as i64))
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
        "t" => return Value::T,
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
