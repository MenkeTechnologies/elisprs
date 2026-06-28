//! An elisp-correct S-expression reader producing rust_lisp `Value`s.
//!
//! We keep rust_lisp's `Value`/`List` data model but not its parser: rust_lisp's
//! reader mis-tokenizes core elisp syntax — `1+`/`1-` (it splits the sign off
//! the digit), `#'foo` (function-quote), and real dotted pairs. Those are far
//! too common in `.el` to live without, so the reader is ours; the value model,
//! evaluator infrastructure, and builtins still build on rust_lisp.
//!
//! Milestone-1 scope: integers, floats, strings, symbols (including `1+`, `<=`,
//! `:keywords`), `nil`/`t`, `'quote`, `#'function`, `?c` char literals, and
//! `;` comments. Not yet: backquote/unquote and true dotted pairs (both error
//! explicitly rather than silently misread).

use crate::error::{ElError, ElResult};
use rust_lisp::model::{Symbol, Value};

pub fn read_all(src: &str) -> ElResult<Vec<Value>> {
    let mut r = Reader { chars: src.chars().collect(), pos: 0 };
    let mut out = Vec::new();
    loop {
        r.skip_ws();
        if r.pos >= r.chars.len() {
            break;
        }
        out.push(r.read_form()?);
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

fn quoted(head: &str, form: Value) -> Value {
    Value::List(vec![Value::Symbol(Symbol(head.to_string())), form].into_iter().collect())
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

    fn read_form(&mut self) -> ElResult<Value> {
        self.skip_ws();
        let c = self.peek().ok_or_else(|| ElError::err("unexpected end of input"))?;
        match c {
            '(' => self.read_list(),
            ')' => Err(ElError::err("unexpected )")),
            '`' | ',' => Err(ElError::err("backquote/unquote not supported in milestone 1")),
            '"' => self.read_string(),
            '?' => self.read_char_literal(),
            '\'' => {
                self.pos += 1;
                Ok(quoted("quote", self.read_form()?))
            }
            '#' if self.peek_at(1) == Some('\'') => {
                self.pos += 2;
                Ok(quoted("function", self.read_form()?))
            }
            _ => self.read_atom(),
        }
    }

    fn read_list(&mut self) -> ElResult<Value> {
        self.pos += 1; // consume '('
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(ElError::err("unclosed list")),
                Some(')') => {
                    self.pos += 1;
                    break;
                }
                Some('.') if self.is_lone_dot() => {
                    return Err(ElError::err(
                        "dotted pairs are not supported in milestone 1 (rust_lisp's list model is proper-only)",
                    ));
                }
                _ => items.push(self.read_form()?),
            }
        }
        Ok(Value::List(items.into_iter().collect()))
    }

    /// A `.` that is its own token (dotted-pair marker), not `.5` or `...`.
    fn is_lone_dot(&self) -> bool {
        self.peek() == Some('.') && self.peek_at(1).map(is_delim).unwrap_or(true)
    }

    fn read_string(&mut self) -> ElResult<Value> {
        self.pos += 1; // consume opening quote
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(ElError::err("unterminated string")),
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.pos += 1;
                    let e = self.peek().ok_or_else(|| ElError::err("unterminated string"))?;
                    self.pos += 1;
                    s.push(unescape(e));
                }
                Some(c) => {
                    self.pos += 1;
                    s.push(c);
                }
            }
        }
        Ok(Value::String(s))
    }

    fn read_char_literal(&mut self) -> ElResult<Value> {
        self.pos += 1; // consume '?'
        let c = self.peek().ok_or_else(|| ElError::err("unterminated char literal"))?;
        self.pos += 1;
        let ch = if c == '\\' {
            let e = self.peek().ok_or_else(|| ElError::err("unterminated char literal"))?;
            self.pos += 1;
            unescape(e)
        } else {
            c
        };
        Ok(Value::Int(ch as i64))
    }

    fn read_atom(&mut self) -> ElResult<Value> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_delim(c) {
                break;
            }
            self.pos += 1;
        }
        let tok: String = self.chars[start..self.pos].iter().collect();
        Ok(classify(&tok))
    }
}

fn unescape(e: char) -> char {
    match e {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        'e' => '\u{1b}',
        other => other, // \\ \" \? etc.
    }
}

fn classify(tok: &str) -> Value {
    match tok {
        "nil" => return Value::NIL,
        "t" => return Value::True,
        _ => {}
    }
    if tok.starts_with(':') {
        return Value::Symbol(Symbol(tok.to_string()));
    }
    // Try integer, then float. Crucially, `1+`/`1-`/`+`/`<=` fail both parses
    // and fall through to Symbol — the whole reason we don't use rust_lisp's
    // number-greedy tokenizer.
    if let Ok(i) = tok.parse::<i64>() {
        return Value::Int(i);
    }
    if looks_numeric(tok) {
        if let Ok(f) = tok.parse::<f64>() {
            return Value::Float(f);
        }
    }
    Value::Symbol(Symbol(tok.to_string()))
}

/// Guard float parsing so symbols like `+`/`.` aren't misread, while still
/// accepting `1.5`, `-3.0`, `.5`, `1e9`.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn read1(s: &str) -> Value {
        read_all(s).unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn reads_symbols_that_look_like_numbers() {
        assert!(matches!(read1("1+"), Value::Symbol(s) if s.0 == "1+"));
        assert!(matches!(read1("1-"), Value::Symbol(s) if s.0 == "1-"));
        assert!(matches!(read1("<="), Value::Symbol(s) if s.0 == "<="));
        assert!(matches!(read1("42"), Value::Int(42)));
        assert!(matches!(read1("-3"), Value::Int(-3)));
        assert!(matches!(read1("1.5"), Value::Float(_)));
    }

    #[test]
    fn function_quote_desugars() {
        // #'foo => (function foo)
        let v = read1("#'foo");
        let parts = crate::interp::to_vec(&v).unwrap();
        assert!(matches!(&parts[0], Value::Symbol(s) if s.0 == "function"));
    }

    #[test]
    fn char_literal_is_code_point() {
        assert!(matches!(read1("?A"), Value::Int(65)));
        assert!(matches!(read1("?\\n"), Value::Int(10)));
    }

    #[test]
    fn dotted_pairs_error_loudly() {
        assert!(read_all("(1 . 2)").is_err());
    }
}
