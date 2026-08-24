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
/// Resolves a syntax-class designator (`w`, `_`, `<`, …) to the character
/// ranges that carry it, so `\sC` can be compiled against the syntax table in
/// force where the regexp is compiled.
///
/// A trait object rather than a direct host call because this module is a pure
/// translator with no arena access; `crate::builtins::compile_cf` supplies the
/// live table and [`translate`] keeps a no-table path for the unit tests.
pub trait SyntaxLookup {
    /// Ascending, non-overlapping `[lo, hi]` character ranges in class `class`.
    /// An empty result means the class is genuinely empty in this table — `\s!`
    /// with no generic-comment-fence character, say — and `\sC` then matches
    /// nothing, which is not the same as having no table at all.
    fn ranges(&self, class: char) -> Vec<(u32, u32)>;
    /// Whether a syntax table is reachable. False only for [`NoSyntax`].
    fn available(&self) -> bool {
        true
    }
}

/// The syntax lookup used when no table is available (the module's own tests).
/// `translate` then falls back to the hardcoded whitespace/word sets it used
/// before syntax tables were consulted.
pub struct NoSyntax;
impl SyntaxLookup for NoSyntax {
    fn ranges(&self, _class: char) -> Vec<(u32, u32)> {
        Vec::new()
    }
    fn available(&self) -> bool {
        false
    }
}

pub fn translate(pat: &str) -> Result<String, String> {
    translate_with(pat, &NoSyntax)
}

/// [`translate`], resolving `\sC` / `\SC` against `syn`.
pub fn translate_with(pat: &str, syn: &dyn SyntaxLookup) -> Result<String, String> {
    translate_groups(pat, syn).map(|(re, _)| re)
}

/// [`translate_with`], additionally reporting how the emitted capture groups map
/// back to the *Emacs* group numbers.
///
/// The two numberings only coincide when the pattern uses nothing but plain
/// `\(`. Emacs's `\(?N:RE\)` names a group explicitly and then continues
/// counting from `N + 1` (`regex-emacs.c` sets `regnum = N`), so
/// `\(?5:a\)\(b\)` has groups 5 and 6 and `(match-data)` reports
/// `(0 2 nil nil nil nil nil nil nil nil 0 1 1 2)`. `fancy_regex` has no
/// explicit-numbering syntax and numbers positionally, so the same pattern
/// compiles to two groups numbered 1 and 2 there.
///
/// The returned vector is that correspondence: element `i` is the Emacs group
/// number of the compiled pattern's group `i + 1`. It is the identity vector
/// `[1, 2, 3, …]` whenever the pattern uses no explicit numbering, which is the
/// case callers can skip the remap for.
pub fn translate_groups(pat: &str, syn: &dyn SyntaxLookup) -> Result<(String, Vec<u32>), String> {
    let mut out = String::with_capacity(pat.len() + 8);
    let mut it = pat.chars().peekable();
    // Depth of open `\(` groups, so a stray `\)` is diagnosed like Emacs's.
    let mut depth: i32 = 0;
    // Emacs group number the next `\(` will take, and the number each emitted
    // capture group carries, in emission order.
    let mut next_group: u32 = 1;
    let mut groups: Vec<u32> = Vec::new();
    // Whether a repetition operator here has something to repeat. False at the
    // start of the pattern and just after `\(` or `\|`, where Emacs reads
    // `*`/`+`/`?` as ordinary characters.
    let mut can_repeat = false;
    while let Some(c) = it.next() {
        match c {
            '\\' => translate_escape(
                &mut it,
                &mut out,
                &mut depth,
                &mut can_repeat,
                syn,
                &mut next_group,
                &mut groups,
            )?,
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
    Ok((out, groups))
}

/// The largest scalar value a Rust `char` can hold. Emacs's character space runs
/// to `#x3FFFFF` (raw bytes and unassigned codepoints live above Unicode), but a
/// `fancy_regex` class can only name real scalar values, so a syntax-class range
/// is clipped here and the surrogate block is stepped over.
const MAX_SCALAR: u32 = 0x10FFFF;

/// A `fancy_regex` character class covering `ranges` (or its complement when
/// `neg`). Empty input yields a class that cannot match — `[^\x{0}-\x{10FFFF}]`
/// for the positive form, `.`-equivalent for the negated one — because a syntax
/// class with no characters must match nothing, not everything.
fn char_class(ranges: &[(u32, u32)], neg: bool) -> String {
    let mut body = String::new();
    for &(lo, hi) in ranges {
        if lo > MAX_SCALAR {
            continue;
        }
        let hi = hi.min(MAX_SCALAR);
        // Split around the surrogate block: D800–DFFF are not scalar values and
        // the regex parser rejects them inside a class.
        for (a, b) in [(lo, hi.min(0xD7FF)), (lo.max(0xE000), hi)] {
            if a > b || a > MAX_SCALAR {
                continue;
            }
            if a == b {
                body.push_str(&format!("\\x{{{a:X}}}"));
            } else {
                body.push_str(&format!("\\x{{{a:X}}}-\\x{{{b:X}}}"));
            }
        }
    }
    if body.is_empty() {
        // No character is in the class.
        body = "\\x{0}-\\x{10FFFF}".to_string();
        return format!("(?-i:[{}{body}])", if neg { "" } else { "^" });
    }
    // `(?-i:…)` because a syntax class is never case-folded. Emacs's matcher asks
    // `SYNTAX (c)` of the character as it stands; `case-fold-search` only folds
    // literal characters. Without the override, `case-fold-search`'s `(?i)`
    // leaked in and `(progn (modify-syntax-entry ?a "_") (string-match "\\sw"
    // "a"))` answered 0 — "a" matching the class's `A-Z` run — where Emacs 30.2
    // answers nil.
    format!("(?-i:[{}{body}])", if neg { "^" } else { "" })
}

/// Emacs's own `invalid-regexp` messages (`regex-emacs.c`), reproduced verbatim:
/// elisp code catches these and prints the string.
const UNMATCHED_OPEN: &str = "Unmatched ( or \\(";
const UNMATCHED_CLOSE: &str = "Unmatched ) or \\)";
const UNMATCHED_BRACKET: &str = "Unmatched [ or [^";
const UNMATCHED_BRACE: &str = "Unmatched \\{";
const INVALID_BRACE_CONTENT: &str = "Invalid content of \\{\\}";
const TRAILING_BACKSLASH: &str = "Trailing backslash";
/// Emacs's catch-all for a malformed construct it has no specific message for —
/// what `\(?0:…\)` and `\(?a:…\)` report.
const INVALID_REGEXP: &str = "Invalid regular expression";

fn translate_escape(
    it: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
    depth: &mut i32,
    can_repeat: &mut bool,
    syn: &dyn SyntaxLookup,
    next_group: &mut u32,
    groups: &mut Vec<u32>,
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
            // `\(?N:RE\)`. fancy-regex has no explicit-numbering syntax and numbers
            // capture groups positionally, so an explicit group still emits a plain
            // capture — the Emacs number it stands for is recorded in `groups`
            // instead, and the caller remaps the spans after the match.
            if it.peek() == Some(&'?') {
                it.next(); // consume '?'
                if matches!(it.peek(), Some(d) if d.is_ascii_digit()) {
                    // `\(?N:` — read N, drop the digits and the ':', emit a plain
                    // capture, and continue Emacs's counter from N + 1 (regnum = N).
                    let mut n: u32 = 0;
                    while let Some(&d) = it.peek() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        it.next();
                        // Emacs's `regnum` is a plain int; a number this large
                        // cannot name a real group, and its own reader rejects the
                        // whole construct rather than overflowing.
                        n = n
                            .saturating_mul(10)
                            .saturating_add(d as u32 - '0' as u32)
                            .min(u32::MAX / 16);
                    }
                    // Group 0 is the whole match and cannot be named, and anything
                    // that is not `\(?DIGITS:` is not an explicit group at all —
                    // Emacs rejects both with its generic message.
                    if n == 0 || it.peek() != Some(&':') {
                        return Err(INVALID_REGEXP.into());
                    }
                    it.next(); // consume ':'
                    *next_group = n + 1;
                    groups.push(n);
                    out.push('(');
                } else {
                    // Shy group `\(?:`. Emacs's reader accepts exactly two things
                    // after `\(?` — a digit run ending in `:`, handled above, and a
                    // bare `:` — so a `\(?i:…`-style inline flag is not elisp
                    // syntax and must report Emacs's message, not the regex
                    // crate's parse error.
                    if it.peek() != Some(&':') {
                        return Err(INVALID_REGEXP.into());
                    }
                    it.next();
                    out.push_str("(?:");
                }
            } else {
                out.push('(');
                groups.push(*next_group);
                *next_group += 1;
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
            // GNU regex reads the interval strictly as digits[,digits] and
            // diagnoses the FIRST wrong thing it sees: a character that can
            // never appear in an interval (including `\X` for X ≠ `}`) is
            // "Invalid content of \{\}" even when the pattern then ends, while
            // running out of pattern with only valid interval content so far is
            // "Unmatched \{" — so `a\{2,` is Unmatched but `a\{x` is Invalid.
            let mut body = String::new();
            let mut closed = false;
            while let Some(c) = it.next() {
                if c == '\\' {
                    match it.next() {
                        Some('}') => {
                            closed = true;
                            break;
                        }
                        Some(_) => return Err(INVALID_BRACE_CONTENT.into()),
                        None => return Err(TRAILING_BACKSLASH.into()),
                    }
                } else if c.is_ascii_digit() || (c == ',' && !body.contains(',')) {
                    body.push(c);
                } else {
                    return Err(INVALID_BRACE_CONTENT.into());
                }
            }
            if !closed {
                return Err(UNMATCHED_BRACE.into());
            }
            if !valid_brace_body(&body) {
                return Err(INVALID_BRACE_CONTENT.into());
            }
            // Emacs allows an empty lower bound (`\{,3\}`, even `\{\}`) meaning
            // 0; the `regex` dialect does not, so make the 0 explicit.
            out.push('{');
            if body.is_empty() || body.starts_with(',') {
                out.push('0');
            }
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
        // `\w` / `\W` are the word-constituent class of the *syntax table*
        // (`regex-emacs.c` tests `SYNTAX (c) == Sword`), not the crate's
        // `[0-9A-Za-z_]`. The two disagree on `_` under any lisp-mode table,
        // where `_` is a symbol constituent: `(string-match "\\w" "_")` is nil in
        // Emacs 30.2 and was 0 here.
        'w' | 'W' => {
            if syn.available() {
                out.push_str(&char_class(&syn.ranges('w'), e == 'W'));
            } else {
                out.push_str(if e == 'W' { r"\W" } else { r"\w" });
            }
        }
        'b' => out.push_str(r"\b"),
        'B' => out.push_str(r"\B"),
        // Syntax classes `\sC` / `\SC`, resolved against the syntax table in
        // force where this regexp is compiled.
        //
        // Emacs asks `SYNTAX (c) == class` per character while matching; this
        // translator has to answer for the whole character space up front, which
        // `SyntaxLookup` does by reading the table's runs. Getting the table
        // involved is the whole point: `\s_` matched nothing before (the arm
        // below fell through to the whitespace set), and the classes that *were*
        // hardcoded ignored `with-syntax-table` entirely.
        's' | 'S' => {
            let neg = e == 'S';
            let class = it.peek().copied();
            // `-` is `modify-syntax-entry`'s alias for the whitespace class.
            let class = class.map(|c| if c == '-' { ' ' } else { c });
            if let (true, Some(class)) = (syn.available(), class) {
                it.next();
                out.push_str(&char_class(&syn.ranges(class), neg));
                return Ok(());
            }
            match it.next() {
                // Whitespace syntax is the SYNTAX TABLE's whitespace class, not
                // the regex crate's `\s`. They differ on the line terminators:
                // `\s` matches `\n`, `\r` and `\v`, but the standard syntax table
                // classes newline as comment-end (`>`) and CR as a symbol
                // constituent, so Emacs does not. Verified against GNU Emacs 30.2:
                //
                //   (string-match "\\s-" "\n") => nil    "\r" => nil   "\v" => nil
                //   (string-match "\\s-" "\t") => 0      "\f" => 0     " "  => 0
                //   (string-match "\\s-" " ") => 0
                //
                // which is exactly `?\t ?\f ?\s 160` — the set the standard table
                // marks whitespace. Emitting `\s` made every `\s-` silently match
                // across a line boundary.
                Some('-') | Some(' ') => out.push_str(if neg { WS_SYNTAX_NEG } else { WS_SYNTAX }),
                Some('w') => out.push_str(if neg { r"\W" } else { r"\w" }),
                Some(_) | None => out.push_str(if neg { WS_SYNTAX_NEG } else { WS_SYNTAX }),
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
        // An empty count is a valid interval in Emacs — `a\{\}` means `a\{0\}`.
        None => body.chars().all(|c| c.is_ascii_digit()),
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

/// The standard syntax table's whitespace class, as a `regex` character class:
/// tab, formfeed, space, U+00A0. This is what `\s-` means in Emacs — see the
/// `'s' | 'S'` arm of [`translate_escape`] for the ground-truth transcript.
/// Newline, CR and vertical tab are deliberately absent; the regex crate's `\s`
/// includes them.
const WS_SYNTAX: &str = "[\\t\\x0C\\x{A0} ]";
/// Complement of [`WS_SYNTAX`] — the translation of `\S-`.
const WS_SYNTAX_NEG: &str = "[^\\t\\x0C\\x{A0} ]";

/// Emacs's `[:class:]` names, as class BODY text for the `regex` crate.
///
/// The `regex` crate's own POSIX classes are ASCII-only, so copying `[:alpha:]`
/// through left `(string-match "[[:alpha:]]" "Ü")` at nil where Emacs answers 0.
/// Emacs's classes are defined over the whole character set (Elisp manual,
/// "Char Classes"), so each one is re-expressed as the Unicode property the
/// manual names. `None` keeps the `regex` crate's own definition, which for
/// `digit`/`xdigit` already agrees with Emacs char for char.
///
/// `upper`/`lower` need no special handling for `case-fold-search`: the manual
/// says a non-nil `case-fold-search` makes `[:upper:]` match lower case too, and
/// `compile_cf` already compiles under `(?i)` in exactly that case, which applies
/// Unicode case folding to these classes and produces the same set.
///
/// NOT closed here, and left at the crate's ASCII-only definition on purpose:
/// `graph` and `print`. The manual defines both by COMPLEMENT ("any character
/// except whitespace, control characters, surrogates, and unassigned
/// codepoints"), and a complement is not expressible as class body text —
/// `[[^…]…]` needs nested classes, which fancy-regex's parser rejects
/// ("error parsing pattern"). Reaching them means either translating the whole
/// alternative rather than one member, or a different engine.
/// `space`/`punct`/`word` are improved but still approximations: in Emacs those
/// three read the SYNTAX TABLE, not a Unicode property, so their answer depends
/// on the major mode — `(string-match "[[:space:]]" "\n")` is nil in
/// fundamental-mode and 0 in text-mode. `translate` is a pure string→string
/// function with no host access, so it cannot read the live table here; the
/// `\s-` escape is handled separately via [`WS_SYNTAX`].
fn posix_class(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => r"\p{Alphabetic}\p{M}",
        "alnum" => r"\p{Alphabetic}\p{M}\p{Nd}",
        "space" => r"\p{White_Space}",
        "upper" => r"\p{Uppercase}",
        "lower" => r"\p{Lowercase}",
        "punct" => r"\p{P}\p{S}",
        "word" => r"\p{Alphabetic}\p{Nd}\p{M}",
        "blank" => "\t\\p{Zs}",
        // "any ASCII control character (ASCII codes 0 to 31)" — DEL is NOT one,
        // and the `regex` crate's own `[:cntrl:]` includes it.
        "cntrl" => r"\x00-\x1F",
        // "matches any ASCII character (codes 0-127)" / its complement. These two
        // and the byte-width pair below were not classes the `regex` crate knows
        // at all.
        "ascii" | "unibyte" => "[:ascii:]",
        "nonascii" | "multibyte" => "[:^ascii:]",
        _ => return None,
    })
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
            // POSIX class `[:alpha:]`.
            '[' if it.peek() == Some(&':') => {
                let mut name = String::new();
                let mut raw = String::from("[");
                for n in it.by_ref() {
                    raw.push(n);
                    if n == ']' {
                        break;
                    }
                    if n != ':' {
                        name.push(n);
                    }
                }
                match posix_class(&name) {
                    Some(repl) => out.push_str(repl),
                    None => out.push_str(&raw),
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
        // A POSIX class does NOT pass through: the regex crate's own
        // `[:alpha:]` is ASCII-only, so `posix_class` rewrites it to the
        // Unicode properties. Without that rewrite
        // `(string-match "[[:alpha:]]" "Ü")` was nil where Emacs answers 0.
        assert_eq!(t(r"[[:alpha:]]"), r"[\p{Alphabetic}\p{M}]");
        assert_eq!(t(r"[^()]"), "[^()]");
        // A bare `[` is a literal class member in elisp (`\{[` keymap check in
        // derived.el's `derived-mode-make-docstring`); the crate needs it escaped.
        assert_eq!(t(r"[{[]"), r"[{\[]");
        assert_eq!(t(r"\\[{[]"), r"\\[{\[]");
    }

    /// `\s-` is the syntax table's whitespace class, NOT the regex crate's `\s`.
    /// This expectation was `\w+\s\w` — that spelling is what made `\s-` match a
    /// newline, because the crate's `\s` includes `\n`, `\r` and `\v` while the
    /// standard syntax table classes newline as comment-end and CR as a symbol
    /// constituent. Ground truth (GNU Emacs 30.2):
    ///
    /// ```text
    /// (string-match "\\s-" "\n") => nil    (string-match "\\s-" "\t") => 0
    /// (string-match "\\s-" "\r") => nil    (string-match "\\s-" "\f") => 0
    /// (string-match "\\s-" "\v") => nil    (string-match "\\s-" " ")  => 0
    ///                                      (string-match "\\s-" " ") => 0
    /// ```
    #[test]
    fn syntax_and_word_escapes() {
        assert_eq!(t(r"\w+\s-\sw"), "\\w+[\\t\\x0C\\x{A0} ]\\w");
    }

    /// The set `\s-` translates to must be exactly the standard syntax table's
    /// whitespace characters — pinned by matching, not by string equality, so a
    /// re-spelling of the class body that keeps the same meaning still passes.
    #[test]
    fn whitespace_syntax_class_excludes_line_terminators() {
        let re = fancy_regex::Regex::new(&t(r"\s-")).expect("compiles");
        for yes in ["\t", "\u{0C}", " ", "\u{A0}"] {
            assert!(re.is_match(yes).unwrap(), "\\s- must match {yes:?}");
        }
        for no in ["\n", "\r", "\u{0B}", "a"] {
            assert!(!re.is_match(no).unwrap(), "\\s- must not match {no:?}");
        }
    }

    #[test]
    fn backreference_passes_through() {
        assert_eq!(t(r"\(a\)\1"), r"(a)\1");
    }
}
