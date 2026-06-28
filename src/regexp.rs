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
//! `[a-z]`, `[^...]`, and POSIX `[:class:]`). Backreferences in the *pattern*
//! (`\1`..`\9`) are unsupported — the `regex` crate has no backtracking — and
//! produce an explicit error rather than a silent mismatch.

/// Translate an Emacs regexp string into the `regex` crate's syntax.
pub fn translate(pat: &str) -> Result<String, String> {
    let mut out = String::with_capacity(pat.len() + 8);
    let mut it = pat.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => translate_escape(&mut it, &mut out)?,
            // Literal in elisp, special in the crate → escape.
            '(' | ')' | '{' | '}' | '|' => {
                out.push('\\');
                out.push(c);
            }
            // Character alternative: copy through verbatim. Both dialects agree
            // on `[a-z]`, `[^...]`, a leading/`^`-leading `]`, and `[:class:]`.
            '[' => copy_class(&mut it, &mut out),
            // `+ * ? . ^ $` and ordinary chars share meaning across dialects.
            _ => out.push(c),
        }
    }
    Ok(out)
}

fn translate_escape(
    it: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
) -> Result<(), String> {
    let Some(e) = it.next() else {
        return Err("trailing backslash in regexp".into());
    };
    match e {
        // Grouping / alternation / bounds: drop the backslash.
        '(' => {
            // Preserve a shy group `\(?:` as the crate's `(?:`.
            if it.peek() == Some(&'?') {
                out.push('(');
                out.push('?');
                it.next();
                // Copy the modifier run up to and including ':' (e.g. `?:`).
                while let Some(&n) = it.peek() {
                    out.push(n);
                    it.next();
                    if n == ':' {
                        break;
                    }
                }
            } else {
                out.push('(');
            }
        }
        ')' => out.push(')'),
        '|' => out.push('|'),
        '{' => out.push('{'),
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
        // Backreferences are unrepresentable in a non-backtracking engine.
        '1'..='9' => {
            return Err(format!(
                "backreference \\{e} in regexp is unsupported (no backtracking)"
            ));
        }
        // Anything else: keep the escape (covers `\.`, `\*`, `\\`, `\+`, …).
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    Ok(())
}

/// Copy a `[...]` character alternative from `it` into `out`, leading `[`
/// already consumed. Handles a `^` negation and a `]` that appears first (or
/// first-after-`^`) as a literal, matching elisp/POSIX rules.
fn copy_class(it: &mut std::iter::Peekable<std::str::Chars>, out: &mut String) {
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
        out.push(c);
        if c == '[' && it.peek() == Some(&':') {
            // POSIX class `[:alpha:]` — copy through its closing `:]`.
            for n in it.by_ref() {
                out.push(n);
                if n == ']' {
                    break;
                }
            }
        } else if c == ']' {
            break;
        }
    }
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
    }

    #[test]
    fn syntax_and_word_escapes() {
        assert_eq!(t(r"\w+\s-\sw"), r"\w+\s\w");
    }

    #[test]
    fn backreference_rejected() {
        assert!(translate(r"\(a\)\1").is_err());
    }
}
