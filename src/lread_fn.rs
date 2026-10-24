//! `read` from a function stream: lread.c's `read0` driven through
//! `source_function_get` / `source_function_unget`.
//!
//! A function source sees every `readchar` and `unreadchar` the reader makes,
//! so the call sequence is part of the contract: `(a b)` is read as `(`, `a`,
//! SPC, *unread* SPC, SPC, `b`, `)`, *unread* `)`, `)`. This module ports the
//! character consumption of `read0` and its helpers (`read_char_literal`,
//! `read_string_literal`, `read_char_escape`, `read_integer`,
//! `read_bool_vector`, `skip_lazy_string`, `skip_space_and_comments`) with
//! the same reads and unreads in the same order, collecting the characters
//! that make up the object. The object itself is then built by the string
//! reader from those characters, so its value is the one `read-from-string`
//! gives for the same text.
//!
//! A syntax error found while reading carries `read0`'s message; one found
//! while building the object (a record, char-table or bool-vector of the
//! wrong shape, an unknown character name) is the string reader's.

use crate::host::{call_function, with_host};
use fusevm::Value;

type R = Result<Value, String>;

const NO_BREAK_SPACE: i64 = 0xA0;

/// `read_entry_type`: the syntactic context an object is read in.
#[derive(Clone, Copy, PartialEq)]
enum Entry {
    ListStart,
    List,
    ListDot,
    Vector,
    Record,
    CharTable,
    SubCharTable,
    ByteCode,
    StringProps,
    Special,
    Numbered,
}

/// Why reading stopped short of an object.
enum Stop {
    /// `end_of_file_error`: a function source signals `(end-of-file)`.
    Eof,
    /// `invalid_syntax` or `error`, with `read0`'s message.
    Syntax(String),
    /// An error signalled by the stream function itself.
    Lisp(String),
}

impl From<String> for Stop {
    fn from(e: String) -> Stop {
        Stop::Lisp(e)
    }
}

type S<T> = Result<T, Stop>;

/// A function source and the text it has delivered.
struct Source {
    fun: Value,
    /// Characters read and not given back.
    text: Vec<char>,
}

fn to_char(c: i64) -> char {
    char::from_u32(c as u32).unwrap_or('\u{fffd}')
}

fn is_space(c: i64) -> bool {
    c <= 32 || c == NO_BREAK_SPACE
}

impl Source {
    /// `readchar` → `source_function_get`: `(FUNCTION)`, -1 unless it
    /// answers a character.
    fn get(&mut self) -> S<i64> {
        let x = call_function(&self.fun, &[])?;
        let c = match x {
            Value::Int(n) if (0..=0x3F_FFFF).contains(&n) => n,
            _ => -1,
        };
        if c >= 0 {
            self.text.push(to_char(c));
        }
        Ok(c)
    }

    /// `unreadchar` → `source_function_unget`: `(FUNCTION C)`, except for
    /// the end-of-input mark, which `readchar` did not advance past.
    fn unget(&mut self, c: i64) -> S<()> {
        if c == -1 {
            return Ok(());
        }
        self.text.pop();
        call_function(&self.fun, &[Value::Int(c)])?;
        Ok(())
    }

    fn syntax<T>(&self, msg: &str) -> S<T> {
        Err(Stop::Syntax(format!("invalid-read-syntax: {msg}")))
    }

    fn error<T>(&self, msg: String) -> S<T> {
        Err(Stop::Syntax(format!("error: {msg}")))
    }

    /// `skip_space_and_comments`.
    fn skip_space_and_comments(&mut self) -> S<()> {
        loop {
            let mut c = self.get()?;
            if c == ';' as i64 {
                loop {
                    c = self.get()?;
                    if c < 0 || c == '\n' as i64 {
                        break;
                    }
                }
            }
            if c < 0 {
                return Err(Stop::Eof);
            }
            if !is_space(c) {
                return self.unget(c);
            }
        }
    }

    /// `read_and_buffer`: end of input is `invalid_syntax_with_buffer`.
    fn read_and_buffer(&mut self, buf: &mut String) -> S<i64> {
        let c = self.get()?;
        if c < 0 {
            return self.syntax(buf);
        }
        buf.push(to_char(c));
        Ok(c)
    }

    /// `read_char_escape`, after the backslash and NEXT.
    fn char_escape(&mut self, next: i64) -> S<()> {
        let mut c = next;
        loop {
            match u8::try_from(c).map(char::from).unwrap_or('\0') {
                _ if c == -1 => return Err(Stop::Eof),
                '\n' => {
                    return self.error("Invalid escape char syntax: \\<newline>".into());
                }
                m @ ('M' | 'S' | 'H' | 'A' | 's') => {
                    let c1 = self.get()?;
                    if c1 != '-' as i64 {
                        if m == 's' {
                            return self.unget(c1);
                        }
                        return self.error(format!(
                            "Invalid escape char syntax: \\{m} not followed by -"
                        ));
                    }
                    let c1 = self.get()?;
                    if c1 == '\\' as i64 {
                        c = self.get()?;
                        continue;
                    }
                    return if c1 < 0 { Err(Stop::Eof) } else { Ok(()) };
                }
                m @ ('C' | '^') => {
                    if m == 'C' {
                        let c1 = self.get()?;
                        if c1 != '-' as i64 {
                            return self
                                .error("Invalid escape char syntax: \\C not followed by -".into());
                        }
                    }
                    let c1 = self.get()?;
                    if c1 == '\\' as i64 {
                        c = self.get()?;
                        continue;
                    }
                    return if c1 < 0 { Err(Stop::Eof) } else { Ok(()) };
                }
                '0'..='7' => {
                    for _ in 0..2 {
                        let d = self.get()?;
                        if !('0' as i64..='7' as i64).contains(&d) {
                            return self.unget(d);
                        }
                    }
                    return Ok(());
                }
                'x' => {
                    let mut i: u64 = 0;
                    let mut count = 0;
                    loop {
                        let d = self.get()?;
                        let Some(digit) = u32::try_from(d)
                            .ok()
                            .and_then(char::from_u32)
                            .and_then(|ch| ch.to_digit(16))
                        else {
                            self.unget(d)?;
                            break;
                        };
                        i = (i << 4) + u64::from(digit);
                        if i > 0x7F_FFFF {
                            return self.error(format!("Hex character out of range: \\x{i:x}..."));
                        }
                        count += 1;
                    }
                    if count == 0 {
                        return self.error(
                            "Invalid escape char syntax: \\x not followed by hex digit".into(),
                        );
                    }
                    return Ok(());
                }
                u @ ('u' | 'U') => {
                    let n = if u == 'u' { 4 } else { 8 };
                    for _ in 0..n {
                        let d = self.get()?;
                        if d < 0 {
                            return self.error(format!("Malformed Unicode escape: \\{u}"));
                        }
                        if !to_char(d).is_ascii_hexdigit() {
                            return self.error(format!(
                                "Non-hex character used for Unicode escape: {} ({d})",
                                to_char(d)
                            ));
                        }
                    }
                    return Ok(());
                }
                'N' => {
                    if self.get()? != '{' as i64 {
                        return self.syntax("Expected opening brace after \\N");
                    }
                    loop {
                        let d = self.get()?;
                        if d < 0 {
                            return Err(Stop::Eof);
                        }
                        if d == '}' as i64 {
                            return Ok(());
                        }
                        if d >= 0x80 {
                            return self
                                .syntax(&format!("Invalid character U+{d:04X} in character name"));
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// `read_char_literal`, after the `?`.
    fn char_literal(&mut self) -> S<()> {
        let ch = self.get()?;
        if ch < 0 {
            return Err(Stop::Eof);
        }
        if ch == ' ' as i64 || ch == '\t' as i64 {
            return Ok(());
        }
        if ch == '\\' as i64 {
            let next = self.get()?;
            self.char_escape(next)?;
        }
        let nch = self.get()?;
        self.unget(nch)?;
        if nch <= 32 || "\"';()[]#?`,.".contains(to_char(nch)) {
            return Ok(());
        }
        self.syntax("?")
    }

    /// `read_string_literal`, after the opening `"`.
    fn string_literal(&mut self) -> S<()> {
        loop {
            let ch = self.get()?;
            if ch < 0 {
                return Err(Stop::Eof);
            }
            if ch == '"' as i64 {
                return Ok(());
            }
            if ch == '\\' as i64 {
                let e = self.get()?;
                if !(e == 's' as i64 || e == ' ' as i64 || e == '\n' as i64) {
                    self.char_escape(e)?;
                }
            }
        }
    }

    /// `read_integer` in RADIX, after the prefix.
    fn integer(&mut self, radix: u32) -> S<()> {
        let mut valid: i8 = -1;
        let mut c = self.get()?;
        if c == '-' as i64 || c == '+' as i64 {
            c = self.get()?;
        }
        if c == '0' as i64 {
            valid = 1;
            while c == '0' as i64 {
                c = self.get()?;
            }
        }
        while let Some(digit) = u8::try_from(c)
            .ok()
            .filter(u8::is_ascii_alphanumeric)
            .and_then(|d| char::from(d).to_digit(36))
        {
            if digit >= radix {
                valid = 0;
            }
            if valid < 0 {
                valid = 1;
            }
            c = self.get()?;
        }
        self.unget(c)?;
        if valid != 1 {
            return self.syntax(&format!("integer, radix {radix}"));
        }
        Ok(())
    }

    /// `read_bool_vector`, after `#&`.
    fn bool_vector(&mut self) -> S<()> {
        loop {
            let c = self.get()?;
            if !('0' as i64..='9' as i64).contains(&c) {
                if c != '"' as i64 {
                    return self.syntax("#&");
                }
                return self.string_literal();
            }
        }
    }

    /// `skip_lazy_string`, after `#@`: false for `#@00`, which reads
    /// to the end of the input.
    fn lazy_string(&mut self) -> S<bool> {
        let mut nskip: u64 = 0;
        let mut digits = 0;
        loop {
            let c = self.get()?;
            if !('0' as i64..='9' as i64).contains(&c) {
                if nskip == 0 {
                    self.unget(c)?;
                }
                break;
            }
            nskip = nskip * 10 + (c - '0' as i64) as u64;
            digits += 1;
            if digits == 2 && nskip == 0 {
                while self.get()? >= 0 {}
                return Ok(false);
            }
        }
        // skip_dyn_bytes on a source other than a file: up to `\037`.
        loop {
            let c = self.get()?;
            if c < 0 || c == 0o37 {
                return Ok(true);
            }
        }
    }

    /// `read0`.
    fn read0(&mut self, read_circle: bool) -> S<()> {
        let mut stack: Vec<Entry> = Vec::new();
        'read_obj: loop {
            let c = self.get()?;
            if c < 0 {
                return Err(Stop::Eof);
            }
            let ch = to_char(c);
            match ch {
                '(' => {
                    stack.push(Entry::ListStart);
                    continue 'read_obj;
                }
                ')' => match stack.last() {
                    Some(Entry::ListStart | Entry::List | Entry::Record | Entry::StringProps) => {
                        stack.pop();
                    }
                    _ => return self.syntax(")"),
                },
                '[' => {
                    stack.push(Entry::Vector);
                    continue 'read_obj;
                }
                ']' => match stack.last() {
                    Some(
                        Entry::Vector | Entry::ByteCode | Entry::CharTable | Entry::SubCharTable,
                    ) => {
                        stack.pop();
                    }
                    _ => return self.syntax("]"),
                },
                '#' => {
                    let hash_at = self.text.len() - 1;
                    let mut buf = String::from("#");
                    let ch = self.read_and_buffer(&mut buf)?;
                    match to_char(ch) {
                        '\'' => {
                            stack.push(Entry::Special);
                            continue 'read_obj;
                        }
                        '#' => {}
                        's' => {
                            let ch = self.read_and_buffer(&mut buf)?;
                            if ch != '(' as i64 {
                                self.unget(ch)?;
                                return self.syntax(&buf);
                            }
                            stack.push(Entry::Record);
                            continue 'read_obj;
                        }
                        '^' => {
                            let mut ch = self.read_and_buffer(&mut buf)?;
                            let mut entry = Entry::CharTable;
                            if ch == '^' as i64 {
                                ch = self.read_and_buffer(&mut buf)?;
                                entry = Entry::SubCharTable;
                            }
                            if ch != '[' as i64 {
                                self.unget(ch)?;
                                return self.syntax(&buf);
                            }
                            stack.push(entry);
                            continue 'read_obj;
                        }
                        '(' => {
                            stack.push(Entry::StringProps);
                            continue 'read_obj;
                        }
                        '[' => {
                            stack.push(Entry::ByteCode);
                            continue 'read_obj;
                        }
                        '&' => self.bool_vector()?,
                        '!' => {
                            loop {
                                let c = self.get()?;
                                if c < 0 || c == '\n' as i64 {
                                    break;
                                }
                            }
                            self.text.truncate(hash_at);
                            continue 'read_obj;
                        }
                        'x' | 'X' => self.integer(16)?,
                        'o' | 'O' => self.integer(8)?,
                        'b' | 'B' => self.integer(2)?,
                        '@' => {
                            let more = self.lazy_string()?;
                            self.text.truncate(hash_at);
                            if more {
                                continue 'read_obj;
                            }
                            self.text.extend("nil ".chars());
                        }
                        '$' => {}
                        ':' | '_' => {
                            let c = self.get()?;
                            if is_space(c) || "\"';#()[]`,".contains(to_char(c)) {
                                self.unget(c)?;
                            } else {
                                self.symbol(c)?;
                            }
                        }
                        d @ '0'..='9' => {
                            let mut n = i64::from(d as u8 - b'0');
                            let c = loop {
                                let c = self.read_and_buffer(&mut buf)?;
                                if !('0' as i64..='9' as i64).contains(&c) {
                                    break c;
                                }
                                match n
                                    .checked_mul(10)
                                    .and_then(|n| n.checked_add(c - '0' as i64))
                                {
                                    Some(v) => n = v,
                                    None => return self.syntax(&buf),
                                }
                            };
                            if c == 'r' as i64 || c == 'R' as i64 {
                                if !(2..=36).contains(&n) {
                                    return self.syntax(&format!("integer, radix {n}"));
                                }
                                self.integer(n as u32)?;
                            } else if n < (1 << 61) && read_circle && c == '=' as i64 {
                                stack.push(Entry::Numbered);
                                continue 'read_obj;
                            } else if !(n < (1 << 61) && read_circle && c == '#' as i64) {
                                return self.syntax(&buf);
                            }
                        }
                        _ => return self.syntax(&buf),
                    }
                }
                '?' => self.char_literal()?,
                '"' => self.string_literal()?,
                '\'' | '`' => {
                    stack.push(Entry::Special);
                    continue 'read_obj;
                }
                ',' => {
                    let ch = self.get()?;
                    if ch != '@' as i64 && ch >= 0 {
                        self.unget(ch)?;
                    }
                    stack.push(Entry::Special);
                    continue 'read_obj;
                }
                ';' => {
                    loop {
                        let c = self.get()?;
                        if c < 0 || c == '\n' as i64 {
                            break;
                        }
                    }
                    continue 'read_obj;
                }
                _ => {
                    if ch == '.' {
                        let nch = self.get()?;
                        self.unget(nch)?;
                        if is_space(nch) || "\"';([#?`,".contains(to_char(nch)) {
                            if stack.last() == Some(&Entry::List) {
                                *stack.last_mut().unwrap() = Entry::ListDot;
                                continue 'read_obj;
                            }
                            return self.syntax(".");
                        }
                    }
                    if is_space(c) {
                        continue 'read_obj;
                    }
                    self.symbol(c)?;
                }
            }
            // An object has been read: the stack decides what follows it.
            while let Some(top) = stack.last_mut() {
                match *top {
                    Entry::ListStart => {
                        *top = Entry::List;
                        continue 'read_obj;
                    }
                    Entry::List
                    | Entry::Vector
                    | Entry::Record
                    | Entry::CharTable
                    | Entry::SubCharTable
                    | Entry::ByteCode
                    | Entry::StringProps => continue 'read_obj,
                    Entry::ListDot => {
                        self.skip_space_and_comments()?;
                        if self.get()? != ')' as i64 {
                            return self.syntax("expected )");
                        }
                        stack.pop();
                    }
                    Entry::Special | Entry::Numbered => {
                        stack.pop();
                    }
                }
            }
            return Ok(());
        }
    }

    /// `read_symbol`: C is the first character; the token ends at a
    /// delimiter, which is given back.
    fn symbol(&mut self, first: i64) -> S<()> {
        let mut c = first;
        loop {
            if c == '\\' as i64 {
                c = self.get()?;
                if c < 0 {
                    return Err(Stop::Eof);
                }
            }
            c = self.get()?;
            if !(c > 32 && c != NO_BREAK_SPACE && (c >= 128 || !"\"';#()[]`,".contains(to_char(c))))
            {
                break;
            }
        }
        self.unget(c)
    }
}

/// `(read FUNCTION)`.
pub fn read_from_function(fun: &Value) -> R {
    let read_circle = with_host(|h| {
        let s = h.intern("read-circle");
        h.get_value(&s)
            .map(|v| !crate::host::el_nil(&v))
            .unwrap_or(true)
    });
    let mut src = Source {
        fun: fun.clone(),
        text: Vec::new(),
    };
    match src.read0(read_circle) {
        Ok(()) => {
            let text: String = src.text.iter().collect();
            with_host(|h| crate::reader::read_one(h, &text, 0).map(|(form, _)| form))
        }
        Err(Stop::Eof) => Err("end-of-file".into()),
        Err(Stop::Lisp(e)) => Err(e),
        Err(Stop::Syntax(msg)) => Err(msg),
    }
}
