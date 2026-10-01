//! The native JSON primitives — `json-parse-string`, `json-parse-buffer`,
//! `json-serialize`, `json-insert`, `json-available-p` — written against the
//! contract of Emacs 31.1's `src/json.c` (its own parser and serializer, not
//! json.el and not libjansson).
//!
//! Parse errors signal `(SYMBOL LINE nil POSITION)`: LINE counts from 1 and
//! goes up at every newline the parser consumed, and POSITION is the number of
//! characters consumed when the error was detected — the offending character
//! included, or the input length at end of input. Every position quoted in the
//! comments below was measured against `emacs -Q --batch`.

use crate::builtins::{el_eq, ht_ref, make_hash_table_with, puthash};
use crate::host::{ElispHost, Obj};
use fusevm::Value;
use num_bigint::BigInt;
use std::collections::HashSet;

type R = Result<Value, String>;

fn is_nil(v: &Value) -> bool {
    matches!(v, Value::Undef | Value::Bool(false))
}

/// Signal `(SYM . DATA)` through the host's error channel.
fn signal(h: &mut ElispHost, sym: &str, data: Vec<Value>) -> String {
    let symv = h.intern(sym);
    let data = h.list_from(data);
    let msg = format!("{sym}: {}", h.print(&data, true));
    let obj = h.cons(symv, data);
    h.set_pending_error(&msg, obj);
    msg
}

/// `(error MESSAGE VALUE)`, the shape json.c's argument checks use.
fn error_with(h: &mut ElispHost, message: &str, value: &Value) -> String {
    let m = h.new_string(message);
    signal(h, "error", vec![m, value.clone()])
}

#[derive(Clone, Copy, PartialEq)]
enum ObjectType {
    HashTable,
    Alist,
    Plist,
}

struct Conf {
    object_type: ObjectType,
    array_list: bool,
    null_object: Value,
    false_object: Value,
}

/// json.c `json_parse_args`. ARGS must be a plist (`plistp`); an unknown key,
/// or an unknown `:object-type` / `:array-type`, is an `error` whose datum is
/// the offending VALUE. The serializer accepts only the two object keys.
fn parse_args(h: &mut ElispHost, args: &[Value], parse: bool) -> Result<Conf, String> {
    if args.len() % 2 == 1 {
        let l = h.list_from(args.to_vec());
        let p = h.intern("plistp");
        return Err(signal(h, "wrong-type-argument", vec![p, l]));
    }
    let mut conf = Conf {
        object_type: ObjectType::HashTable,
        array_list: false,
        null_object: h.intern(":null"),
        false_object: h.intern(":false"),
    };
    for pair in args.chunks(2) {
        let (key, value) = (&pair[0], &pair[1]);
        let key = h.sym_name(key).unwrap_or_default();
        match key.as_str() {
            ":null-object" => conf.null_object = value.clone(),
            ":false-object" => conf.false_object = value.clone(),
            ":object-type" if parse => {
                conf.object_type = match h.sym_name(value).as_deref() {
                    Some("hash-table") => ObjectType::HashTable,
                    Some("alist") => ObjectType::Alist,
                    Some("plist") => ObjectType::Plist,
                    _ => {
                        return Err(error_with(
                            h,
                            "One of hash-table, alist or plist should be specified",
                            value,
                        ))
                    }
                }
            }
            ":array-type" if parse => {
                conf.array_list = match h.sym_name(value).as_deref() {
                    Some("array") => false,
                    Some("list") => true,
                    _ => {
                        return Err(error_with(
                            h,
                            "One of array or list should be specified",
                            value,
                        ))
                    }
                }
            }
            _ if parse => {
                return Err(error_with(
                    h,
                    "One of :object-type, :array-type, :null-object or :false-object should be specified",
                    value,
                ))
            }
            _ => {
                return Err(error_with(
                    h,
                    "One of :null-object or :false-object should be specified",
                    value,
                ))
            }
        }
    }
    Ok(conf)
}

// ── parsing ──────────────────────────────────────────────────────────────

struct Parser<'a> {
    text: &'a [char],
    /// Characters consumed so far — the POSITION an error reports.
    pos: usize,
    line: i64,
    conf: &'a Conf,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.next();
        }
    }

    fn fail(&self, h: &mut ElispHost, sym: &str) -> String {
        signal(
            h,
            sym,
            vec![
                Value::Int(self.line),
                Value::Undef,
                Value::Int(self.pos as i64),
            ],
        )
    }

    /// The next character, or `json-end-of-file` when there is none.
    fn need(&mut self, h: &mut ElispHost) -> Result<char, String> {
        self.next().ok_or_else(|| self.fail(h, "json-end-of-file"))
    }

    fn value(&mut self, h: &mut ElispHost) -> R {
        self.skip_ws();
        let c = self.need(h)?;
        match c {
            '{' => self.object(h),
            '[' => self.array(h),
            '"' => {
                let s = self.string(h)?;
                Ok(h.new_string(s))
            }
            '-' | '0'..='9' => self.number(h, c),
            't' => self.literal(h, "rue", Value::Bool(true)),
            'f' => {
                let v = self.conf.false_object.clone();
                self.literal(h, "alse", v)
            }
            'n' => {
                let v = self.conf.null_object.clone();
                self.literal(h, "ull", v)
            }
            _ => Err(self.fail(h, "json-parse-error")),
        }
    }

    /// The rest of `true`/`false`/`null`. A literal cut short — by a wrong
    /// character or by the end of input — is a parse error, not end-of-file
    /// (`"tru"` → `(json-parse-error 1 nil 3)`), and one glued to a following
    /// letter, digit or `-` is one too (`"true1"`, `"true-"`), while any other
    /// character is left for the caller (`"true_"` is trailing content).
    fn literal(&mut self, h: &mut ElispHost, rest: &str, v: Value) -> R {
        for expected in rest.chars() {
            if self.next() != Some(expected) {
                return Err(self.fail(h, "json-parse-error"));
            }
        }
        if matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '-') {
            self.next();
            return Err(self.fail(h, "json-parse-error"));
        }
        Ok(v)
    }

    fn digit(&mut self, h: &mut ElispHost) -> Result<char, String> {
        let c = self.need(h)?;
        if c.is_ascii_digit() {
            Ok(c)
        } else {
            Err(self.fail(h, "json-parse-error"))
        }
    }

    /// RFC 8259 number. Without a fraction or exponent it is an integer of any
    /// size (a bignum when it must be); otherwise a float, and a float that
    /// overflows is `json-number-out-of-range-error` at its end (`"1.8e308"`).
    fn number(&mut self, h: &mut ElispHost, first: char) -> R {
        let start = self.pos - 1;
        let lead = if first == '-' { self.digit(h)? } else { first };
        if lead != '0' {
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.next();
            }
        }
        let mut float = false;
        if self.peek() == Some('.') {
            float = true;
            self.next();
            self.digit(h)?;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.next();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            float = true;
            self.next();
            if matches!(self.peek(), Some('+' | '-')) {
                self.next();
            }
            self.digit(h)?;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.next();
            }
        }
        let text: String = self.text[start..self.pos].iter().collect();
        if float {
            let f: f64 = text.parse().unwrap_or(f64::INFINITY);
            if f.is_infinite() {
                return Err(self.fail(h, "json-number-out-of-range-error"));
            }
            Ok(Value::Float(f))
        } else {
            let n: BigInt = text.parse().unwrap_or_default();
            Ok(h.make_integer(n))
        }
    }

    fn hex4(&mut self, h: &mut ElispHost) -> Result<u32, String> {
        let mut n = 0;
        for _ in 0..4 {
            let c = self.need(h)?;
            match c.to_digit(16) {
                Some(d) => n = n * 16 + d,
                None => return Err(self.fail(h, "json-escape-sequence-error")),
            }
        }
        Ok(n)
    }

    /// A string body, after its opening quote. A raw control character is a
    /// parse error; `\uXXXX` must pair a high surrogate with a low one.
    fn string(&mut self, h: &mut ElispHost) -> Result<String, String> {
        let mut out = String::new();
        loop {
            let c = self.need(h)?;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = self.need(h)?;
                    let ch = match e {
                        '"' | '\\' | '/' => e,
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => self.unicode_escape(h)?,
                        _ => return Err(self.fail(h, "json-escape-sequence-error")),
                    };
                    out.push(ch);
                }
                c if (c as u32) < 0x20 => return Err(self.fail(h, "json-parse-error")),
                c => out.push(c),
            }
        }
    }

    fn unicode_escape(&mut self, h: &mut ElispHost) -> Result<char, String> {
        let hi = self.hex4(h)?;
        let code = match hi {
            0xD800..=0xDBFF => {
                if self.need(h)? != '\\' || self.need(h)? != 'u' {
                    return Err(self.fail(h, "json-invalid-surrogate-error"));
                }
                let lo = self.hex4(h)?;
                if !(0xDC00..=0xDFFF).contains(&lo) {
                    return Err(self.fail(h, "json-invalid-surrogate-error"));
                }
                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.fail(h, "json-invalid-surrogate-error")),
            n => n,
        };
        Ok(char::from_u32(code).unwrap_or('\u{FFFD}'))
    }

    fn array(&mut self, h: &mut ElispHost) -> R {
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.next();
        } else {
            loop {
                items.push(self.value(h)?);
                self.skip_ws();
                match self.need(h)? {
                    ',' => continue,
                    ']' => break,
                    _ => return Err(self.fail(h, "json-parse-error")),
                }
            }
        }
        Ok(if self.conf.array_list {
            h.list_from(items)
        } else {
            h.alloc(Obj::Vector(items))
        })
    }

    fn object(&mut self, h: &mut ElispHost) -> R {
        let mut pairs: Vec<(String, Value)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.next();
        } else {
            loop {
                self.skip_ws();
                if self.need(h)? != '"' {
                    return Err(self.fail(h, "json-parse-error"));
                }
                let key = self.string(h)?;
                self.skip_ws();
                if self.need(h)? != ':' {
                    return Err(self.fail(h, "json-parse-error"));
                }
                let v = self.value(h)?;
                pairs.push((key, v));
                self.skip_ws();
                match self.need(h)? {
                    ',' => continue,
                    '}' => break,
                    _ => return Err(self.fail(h, "json-parse-error")),
                }
            }
        }
        self.build_object(h, pairs)
    }

    /// Hash table (`equal`, a repeated key keeps its last value), alist with
    /// interned symbol keys, or plist with keyword keys — the last two keep
    /// every pair, repeats included, in input order.
    fn build_object(&self, h: &mut ElispHost, pairs: Vec<(String, Value)>) -> R {
        match self.conf.object_type {
            ObjectType::HashTable => {
                let test = h.intern(":test");
                let equal = h.intern("equal");
                let table = make_hash_table_with(h, &[test, equal], None)?;
                for (k, v) in pairs {
                    let k = h.new_string(k);
                    puthash(h, &[k, v, table.clone()])?;
                }
                Ok(table)
            }
            ObjectType::Alist => {
                let mut cells = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    let k = h.intern(&k);
                    cells.push(h.cons(k, v));
                }
                Ok(h.list_from(cells))
            }
            ObjectType::Plist => {
                let mut items = Vec::with_capacity(pairs.len() * 2);
                for (k, v) in pairs {
                    items.push(h.intern(&format!(":{k}")));
                    items.push(v);
                }
                Ok(h.list_from(items))
            }
        }
    }
}

fn check_string(h: &mut ElispHost, v: &Value) -> Result<String, String> {
    match h.str_text(v) {
        Some(s) => Ok(s.to_string()),
        None => {
            let p = h.intern("stringp");
            Err(signal(h, "wrong-type-argument", vec![p, v.clone()]))
        }
    }
}

/// `(json-parse-string STRING &rest ARGS)` — the whole of STRING must be one
/// value, optionally surrounded by whitespace; anything after it is
/// `json-trailing-content`.
pub(crate) fn json_parse_string(h: &mut ElispHost, a: &[Value]) -> R {
    let text: Vec<char> = check_string(h, &a[0])?.chars().collect();
    let conf = parse_args(h, &a[1..], true)?;
    let mut p = Parser {
        text: &text,
        pos: 0,
        line: 1,
        conf: &conf,
    };
    let v = p.value(h)?;
    p.skip_ws();
    if p.next().is_some() {
        return Err(p.fail(h, "json-trailing-content"));
    }
    Ok(v)
}

/// `(json-parse-buffer &rest ARGS)` — read one value from point, within the
/// accessible portion, and leave point right after it (trailing whitespace is
/// not skipped). On an error point does not move, and the error's LINE and
/// POSITION count from where the read started.
pub(crate) fn json_parse_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let conf = parse_args(h, a, true)?;
    let (start, text) = {
        let b = h.cur_buf_ref();
        (b.point, b.text[b.point - 1..b.zv - 1].to_vec())
    };
    let mut p = Parser {
        text: &text,
        pos: 0,
        line: 1,
        conf: &conf,
    };
    let v = p.value(h)?;
    h.cur_buf().point = start + p.pos;
    Ok(v)
}

// ── serializing ──────────────────────────────────────────────────────────

/// A JSON string literal: `"` and `\` escaped, the control characters with a
/// short form as `\b \f \n \r \t`, every other one below 0x20 as `\u00XX` with
/// upper-case hex (`"\e"` → `"\u001B"`). DEL and non-ASCII pass through.
fn quote_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn wrong_type(h: &mut ElispHost, pred: &str, v: &Value) -> String {
    let p = h.intern(pred);
    signal(h, "wrong-type-argument", vec![p, v.clone()])
}

struct Writer<'a> {
    conf: &'a Conf,
    out: String,
}

impl Writer<'_> {
    fn value(&mut self, h: &mut ElispHost, v: &Value) -> Result<(), String> {
        if el_eq(h, v, &self.conf.null_object) {
            self.out.push_str("null");
        } else if el_eq(h, v, &self.conf.false_object) {
            self.out.push_str("false");
        } else if matches!(v, Value::Bool(true)) {
            self.out.push_str("true");
        } else if is_nil(v) {
            self.out.push_str("{}");
        } else if let Value::Float(f) = v {
            if !f.is_finite() {
                return Err(error_with(h, "JSON does not allow Inf or NaN", v));
            }
            let s = h.print(v, true);
            self.out.push_str(&s);
        } else if matches!(v, Value::Int(_)) || h.is_bignum(v) {
            let s = h.print(v, true);
            self.out.push_str(&s);
        } else if let Some(s) = h.str_text(v) {
            let s = s.to_string();
            quote_into(&mut self.out, &s);
        } else {
            match h.obj(v).cloned() {
                Some(Obj::Vector(items)) => {
                    self.out.push('[');
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            self.out.push(',');
                        }
                        self.value(h, item)?;
                    }
                    self.out.push(']');
                }
                Some(Obj::HashTable(_)) => self.hash_table(h, v)?,
                Some(Obj::Cons(car, _)) => {
                    if matches!(h.obj(&car), Some(Obj::Cons(..))) {
                        self.alist(h, v)?;
                    } else {
                        self.plist(h, v)?;
                    }
                }
                _ => return Err(wrong_type(h, "json-value-p", v)),
            }
        }
        Ok(())
    }

    fn member(
        &mut self,
        h: &mut ElispHost,
        first: &mut bool,
        key: &str,
        v: &Value,
    ) -> Result<(), String> {
        if !*first {
            self.out.push(',');
        }
        *first = false;
        quote_into(&mut self.out, key);
        self.out.push(':');
        self.value(h, v)
    }

    /// Keys must be strings; every entry is written, in table order.
    fn hash_table(&mut self, h: &mut ElispHost, table: &Value) -> Result<(), String> {
        let pairs: Vec<(Value, Value)> = ht_ref(h, table)?.pairs().cloned().collect();
        self.out.push('{');
        let mut first = true;
        for (k, v) in pairs {
            let Some(key) = h.str_text(&k).map(str::to_string) else {
                return Err(wrong_type(h, "stringp", &k));
            };
            self.member(h, &mut first, &key, &v)?;
        }
        self.out.push('}');
        Ok(())
    }

    /// `((KEY . VALUE) ...)`: KEY a symbol, written by name; a repeated key
    /// keeps its first value. A dotted tail is `listp` of the whole list.
    fn alist(&mut self, h: &mut ElispHost, list: &Value) -> Result<(), String> {
        self.out.push('{');
        let mut first = true;
        let mut seen = HashSet::new();
        let mut tail = list.clone();
        while let Some(Obj::Cons(elt, rest)) = h.obj(&tail).cloned() {
            let Some(Obj::Cons(key, value)) = h.obj(&elt).cloned() else {
                return Err(wrong_type(h, "consp", &elt));
            };
            let Some(name) = symbol_name(h, &key) else {
                return Err(wrong_type(h, "symbolp", &key));
            };
            if seen.insert(name.clone()) {
                self.member(h, &mut first, &name, &value)?;
            }
            tail = rest;
        }
        if !is_nil(&tail) {
            return Err(wrong_type(h, "listp", list));
        }
        self.out.push('}');
        Ok(())
    }

    /// `(KEY VALUE ...)`: a keyword KEY loses its colon. A KEY with no VALUE is
    /// `consp` of the missing tail, checked before KEY must be a symbol.
    fn plist(&mut self, h: &mut ElispHost, list: &Value) -> Result<(), String> {
        self.out.push('{');
        let mut first = true;
        let mut seen = HashSet::new();
        let mut tail = list.clone();
        while let Some(Obj::Cons(key, rest)) = h.obj(&tail).cloned() {
            let Some(Obj::Cons(value, rest)) = h.obj(&rest).cloned() else {
                return Err(wrong_type(h, "consp", &rest));
            };
            let Some(name) = symbol_name(h, &key) else {
                return Err(wrong_type(h, "symbolp", &key));
            };
            let name = name.strip_prefix(':').map(str::to_string).unwrap_or(name);
            if seen.insert(name.clone()) {
                self.member(h, &mut first, &name, &value)?;
            }
            tail = rest;
        }
        if !is_nil(&tail) {
            return Err(wrong_type(h, "listp", list));
        }
        self.out.push('}');
        Ok(())
    }
}

/// The name of a symbol (`nil` and `t` included), or None for a non-symbol.
fn symbol_name(h: &ElispHost, v: &Value) -> Option<String> {
    match v {
        Value::Undef | Value::Bool(_) => h.sym_name(v),
        _ if matches!(h.obj(v), Some(Obj::Symbol(_))) => h.sym_name(v),
        _ => None,
    }
}

fn serialize(h: &mut ElispHost, a: &[Value]) -> Result<String, String> {
    let conf = parse_args(h, &a[1..], false)?;
    let mut w = Writer {
        conf: &conf,
        out: String::new(),
    };
    w.value(h, &a[0])?;
    Ok(w.out)
}

/// `(json-serialize OBJECT &rest ARGS)`.
pub(crate) fn json_serialize(h: &mut ElispHost, a: &[Value]) -> R {
    let s = serialize(h, a)?;
    Ok(h.new_string(s))
}

/// `(json-insert OBJECT &rest ARGS)` — insert the serialization at point.
pub(crate) fn json_insert(h: &mut ElispHost, a: &[Value]) -> R {
    let s = serialize(h, a)?;
    h.cur_insert(s.chars().collect(), true);
    Ok(Value::Undef)
}

/// `(json-available-p)` — the parser is built in, so always t.
pub(crate) fn json_available_p(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Bool(true))
}
