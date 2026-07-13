//! Emacs-Lisp regexp → `regex` crate translation.
//!
//! Emacs regexps invert the convention of POSIX-ERE / PCRE engines: grouping,
//! alternation and bounded repetition are spelled with a leading backslash
//! (`\(`, `\|`, `\{`), while the bare characters `(` `)` `|` `{` `}` are
//! literals. The `regex` crate is the opposite. This module walks an elisp
//! pattern and emits an equivalent pattern in the crate's dialect so the engine
//! can be reused wholesale instead of writing a matcher by hand.
//!
//! Coverage is the common, portable subset of elisp regexp syntax: grouping
//! (incl. shy `\(?:`), alternation, bounded repeats, anchors (`\``, `\'`, `\<`,
//! `\>`, `\_<`, `\_>`), word/symbol/whitespace escapes (`\w \W \b \B \s- \sw`),
//! and character alternatives `[...]` (passed through, since both dialects share
//! `[a-z]`, `[^...]`, and POSIX `[:class:\]`). Backreferences in the *pattern*
//! (`\1`..`\9`) pass through to fancy-regex's backtracking engine, which spells
//! them the same way.

/// Translate an Emacs regexp string into the `regex` crate's syntax.
///
/// The translator also *diagnoses* — with Emacs's own wording (`regex-emacs.c`),
/// because the error string is part of the `invalid-regexp` error data that elisp
/// code catches and prints — and *tolerates* what Emacs tolerates. A repetition
/// operator with nothing to repeat is a literal in Emacs (`(string-match "*x" "x")`
/// is 0, not an error), and a reversed range like `[z-a]` simply never matches
/// rather than failing to compile. Both would otherwise surface as a
/// `fancy-regex` parse failure whose message names byte offsets in the
/// *translated* pattern — meaningless to the elisp caller.
pub fn translate(pat: &str) -> Result<String, String> {
    let mut out = String::with_capacity(pat.len() + 8);
    let mut it = pat.chars().peekable();
    // Depth of open `\(` groups, so a stray `\)` is diagnosed like Emacs's.
    let mut depth: i32 = 0;
    // Whether a repetition operator here has something to repeat. False at the
    // start of the pattern and just after `\(` or `\|`, where Emacs reads
    // `*`/`+`/`?` as ordinary characters.
    let mut can_repeat = false;
    while let Some(c) = it.next() {
        match c {
            '\\' => translate_escape(&mut it, &mut out, &mut depth, &mut can_repeat)?,
            // Literal in elisp, special in the crate → escape.
            '(' | ')' | '{' | '}' | '|' => {
                out.push('\\');
                out.push(c);
                can_repeat = true;
            }
            // Character alternative: copy through verbatim. Both dialects agree
            // on `[a-z]`, `[^...]`, a leading/`^`-leading `]`, and `[:class:]`.
            '[' => {
                copy_class(&mut it, &mut out)?;
                can_repeat = true;
            }
            // A repetition operator with no preceding expression is a literal
            // character in Emacs, not an error.
            '*' | '+' | '?' => {
                if can_repeat {
                    out.push(c);
                } else {
                    out.push('\\');
                    out.push(c);
                    can_repeat = true;
                }
            }
            '^' | '$' => out.push(c),
            // `.` and ordinary chars share meaning across dialects.
            _ => {
                out.push(c);
                can_repeat = true;
            }
        }
    }
    if depth > 0 {
        return Err(UNMATCHED_OPEN.into());
    }
    Ok(out)
}

/// Emacs's own `invalid-regexp` messages (`regex-emacs.c`), reproduced verbatim:
/// elisp code catches these and prints the string.
const UNMATCHED_OPEN: &str = "Unmatched ( or \\(";
const UNMATCHED_CLOSE: &str = "Unmatched ) or \\)";
const UNMATCHED_BRACKET: &str = "Unmatched [ or [^";
const UNMATCHED_BRACE: &str = "Unmatched \\{";
const INVALID_BRACE_CONTENT: &str = "Invalid content of \\{\\}";
const TRAILING_BACKSLASH: &str = "Trailing backslash";

fn translate_escape(
    it: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
    depth: &mut i32,
    can_repeat: &mut bool,
) -> Result<(), String> {
    let Some(e) = it.next() else {
        return Err(TRAILING_BACKSLASH.into());
    };
    // Only a group open / alternation leaves nothing to repeat after it.
    *can_repeat = !matches!(e, '(' | '|');
    match e {
        // Grouping / alternation / bounds: drop the backslash.
        '(' => {
            // `\(?…` is either a shy group `\(?:` or an explicitly-numbered group
            // `\(?N:RE\)`. fancy-regex has no explicit-numbering syntax, but it
            // numbers capture groups positionally — so an explicit group becomes a
            // plain capture `(`, which gives the right match-data index whenever the
            // explicit numbers are sequential (the common case, e.g. font-lock's
            // `\(?1:…\)\(?2:…\)`). Non-sequential numbering isn't preserved.
            if it.peek() == Some(&'?') {
                it.next(); // consume '?'
                if matches!(it.peek(), Some(d) if d.is_ascii_digit()) {
                    // `\(?N:` — drop the digits and the ':' , emit a plain capture.
                    while matches!(it.peek(), Some(d) if d.is_ascii_digit()) {
                        it.next();
                    }
                    if it.peek() == Some(&':') {
                        it.next();
                    }
                    out.push('(');
                } else {
                    // Shy group `\(?:` (or any other `?`-modifier run up to ':').
                    out.push('(');
                    out.push('?');
                    while let Some(&n) = it.peek() {
                        out.push(n);
                        it.next();
                        if n == ':' {
                            break;
                        }
                    }
                }
            } else {
                out.push('(');
            }
            *depth += 1;
        }
        ')' => {
            if *depth == 0 {
                return Err(UNMATCHED_CLOSE.into());
            }
            *depth -= 1;
            out.push(')');
        }
        '|' => out.push('|'),
        // `\{m,n\}` — Emacs validates the bounds itself, and its diagnostics are
        // what elisp code sees.
        '{' => {
            let mut body = String::new();
            let mut closed = false;
            while let Some(c) = it.next() {
                if c == '\\' {
                    match it.next() {
                        Some('}') => {
                            closed = true;
                            break;
                        }
                        Some(o) => {
                            body.push('\\');
                            body.push(o);
                        }
                        None => return Err(TRAILING_BACKSLASH.into()),
                    }
                } else {
                    body.push(c);
                }
            }
            if !closed {
                return Err(UNMATCHED_BRACE.into());
            }
            if !valid_brace_body(&body) {
                return Err(INVALID_BRACE_CONTENT.into());
            }
            out.push('{');
            out.push_str(&body);
            out.push('}');
        }
        '}' => out.push('}'),
        // Anchors.
        '`' => out.push_str(r"\A"),
        '\'' => out.push_str(r"\z"),
        '<' | '>' => out.push_str(r"\b"),
        '_' => {
            // Symbol boundaries `\_<` / `\_>` — approximate with a word boundary.
            match it.next() {
                Some('<') | Some('>') => out.push_str(r"\b"),
                Some(o) => {
                    out.push('_');
                    out.push(o);
                }
                None => out.push('_'),
            }
        }
        '=' => {} // point — no analogue; matches empty.
        // Word / boundary escapes shared with the crate.
        'w' => out.push_str(r"\w"),
        'W' => out.push_str(r"\W"),
        'b' => out.push_str(r"\b"),
        'B' => out.push_str(r"\B"),
        // Syntax classes `\sC` / `\SC`: map the common whitespace/word codes,
        // otherwise fall back to "anything" so the pattern still compiles.
        's' | 'S' => {
            let neg = e == 'S';
            match it.next() {
                Some('-') | Some(' ') => out.push_str(if neg { r"\S" } else { r"\s" }),
                Some('w') => out.push_str(if neg { r"\W" } else { r"\w" }),
                Some(_) | None => out.push_str(if neg { r"\S" } else { r"\s" }),
            }
        }
        // Backreferences `\1`..`\9` — fancy-regex's backtracking engine handles
        // these; both dialects spell them the same way.
        '1'..='9' => {
            out.push('\\');
            out.push(e);
        }
        // Anything else: keep the escape (covers `\.`, `\*`, `\\`, `\+`, …).
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    Ok(())
}

/// Whether `body` is a valid `\\{…\\}` repetition count: `m`, `m,`, `,n` or
/// `m,n` with `m <= n`. Emacs signals `Invalid content of \\{\\}` otherwise —
/// notably for a reversed bound like `a\\{2,1\\}`.
fn valid_brace_body(body: &str) -> bool {
    let parse = |s: &str| -> Option<u64> { s.parse().ok() };
    match body.split_once(',') {
        None => !body.is_empty() && body.chars().all(|c| c.is_ascii_digit()),
        Some((lo, hi)) => {
            let lo_v = if lo.is_empty() { Some(0) } else { parse(lo) };
            match (lo_v, hi.is_empty()) {
                (Some(_), true) => true,
                (Some(l), false) => matches!(parse(hi), Some(h) if l <= h),
                _ => false,
            }
        }
    }
}

/// Copy a `[...]` character alternative from `it` into `out`, leading `[`
/// already consumed. Handles a `^` negation and a `]` that appears first (or
/// first-after-`^`) as a literal, matching elisp/POSIX rules.
fn copy_class(
    it: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
) -> Result<(), String> {
    // Collect the members first: a reversed range (`[z-a]`) has to be rewritten,
    // because Emacs matches nothing for it where the `regex` crate refuses to
    // compile at all.
    let mut buf = String::new();
    let mut closed = false;
    let out_start = out.len();
    out.push('[');
    if it.peek() == Some(&'^') {
        out.push('^');
        it.next();
    }
    // A `]` in the first position is a literal member, not the terminator.
    if it.peek() == Some(&']') {
        out.push(']');
        it.next();
    }
    while let Some(c) = it.next() {
        match c {
            // In an elisp char class a backslash is an ordinary character (no
            // escapes), so escape it for the `regex` crate: `[\"]` matches `\`/`"`.
            '\\' => {
                out.push_str("\\\\");
                buf.push('\\');
            }
            // POSIX class `[:alpha:]` — copy through its closing `:]`.
            '[' if it.peek() == Some(&':') => {
                out.push('[');
                for n in it.by_ref() {
                    out.push(n);
                    if n == ']' {
                        break;
                    }
                }
            }
            // A bare `[` is an ordinary member in elisp/POSIX bracket expressions
            // (e.g. `[{[]` matches `{` or `[`), but the `regex` crate rejects an
            // unescaped `[` inside a class — escape it.
            '[' => {
                out.push_str("\\[");
                buf.push('[');
            }
            ']' => {
                out.push(']');
                closed = true;
                break;
            }
            _ => {
                out.push(c);
                buf.push(c);
            }
        }
    }
    if !closed {
        return Err(UNMATCHED_BRACKET.into());
    }
    // A reversed range matches nothing in Emacs; emit a class that can never
    // match rather than letting the engine reject the pattern.
    if has_reversed_range(&buf) {
        out.truncate(out_start);
        out.push_str("[^\\s\\S]");
    }
    Ok(())
}

/// Whether a class body contains a range whose end sorts before its start.
fn has_reversed_range(body: &str) -> bool {
    let cs: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i + 2 < cs.len() {
        if cs[i + 1] == '-' && cs[i + 2] != ']' && cs[i] > cs[i + 2] {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::translate;

    fn t(p: &str) -> String {
        translate(p).unwrap()
    }

    #[test]
    fn grouping_and_alternation_invert() {
        assert_eq!(t(r"\(ab\|cd\)+"), "(ab|cd)+");
        assert_eq!(t(r"a(b)c"), r"a\(b\)c");
        assert_eq!(t(r"\(?:foo\)"), "(?:foo)");
        // Explicitly-numbered groups `\(?N:…\)` become plain captures (fancy-regex
        // numbers positionally, which is correct for sequential explicit numbers).
        assert_eq!(t(r"\(?1:foo\)"), "(foo)");
        assert_eq!(t(r"\(?1:a\)-\(?2:b\)"), "(a)-(b)");
        assert_eq!(t(r"\(?:x\)\(?1:y\)"), "(?:x)(y)");
    }

    #[test]
    fn bounds_and_anchors() {
        assert_eq!(t(r"a\{2,3\}"), "a{2,3}");
        assert_eq!(t(r"\`foo\'"), r"\Afoo\z");
        assert_eq!(t(r"\<word\>"), r"\bword\b");
    }

    #[test]
    fn classes_pass_through() {
        assert_eq!(t(r"[a-z]+"), "[a-z]+");
        assert_eq!(t(r"[]ab]"), "[]ab]");
        assert_eq!(t(r"[[:alpha:]]"), "[[:alpha:]]");
        assert_eq!(t(r"[^()]"), "[^()]");
        // A bare `[` is a literal class member in elisp (`\{[` keymap check in
        // derived.el's `derived-mode-make-docstring`); the crate needs it escaped.
        assert_eq!(t(r"[{[]"), r"[{\[]");
        assert_eq!(t(r"\\[{[]"), r"\\[{\[]");
    }

    #[test]
    fn syntax_and_word_escapes() {
        assert_eq!(t(r"\w+\s-\sw"), r"\w+\s\w");
    }

    #[test]
    fn backreference_passes_through() {
        assert_eq!(t(r"\(a\)\1"), r"(a)\1");
    }
}
