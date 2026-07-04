//! Regexp coverage: the `string-match` family in `builtins.rs` plus the
//! `save-match-data` prelude macro. These exercise the elisp→engine regexp
//! translation (`\(` groups, `\w`, `\|`, POSIX classes), char-indexed match
//! data, and template expansion in `replace-regexp-in-string`. Expectations were
//! captured from the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn string_match_returns_char_index_or_nil() {
    assert_eq!(eval("(string-match \"b.\" \"abcd\")"), "1");
    assert_eq!(eval("(string-match \"xyz\" \"abcd\")"), "nil");
    // anchors
    assert_eq!(eval("(string-match \"^abc$\" \"abc\")"), "0");
    // \w word class and POSIX classes translate.
    assert_eq!(eval("(string-match \"\\\\w+\" \"  hi  \")"), "2");
    assert_eq!(eval("(string-match \"[[:digit:]]+\" \"abc123\")"), "3");
}

#[test]
fn match_positions_and_substrings() {
    // \( \) grouping; group 0 is the whole match.
    assert_eq!(
        eval("(progn (string-match \"\\\\(a+\\\\)\" \"xaab\") (match-beginning 0))"),
        "1"
    );
    assert_eq!(
        eval("(progn (string-match \"\\\\(a+\\\\)\" \"xaab\") (match-end 0))"),
        "3"
    );
    assert_eq!(
        eval("(progn (string-match \"\\\\(a+\\\\)\\\\(b+\\\\)\" \"aabbb\") (match-string 2 \"aabbb\"))"),
        "\"bbb\""
    );
    // a non-participating optional group reports nil.
    assert_eq!(
        eval("(progn (string-match \"\\\\(foo\\\\)?bar\" \"bar\") (match-beginning 1))"),
        "nil"
    );
}

#[test]
fn match_data_is_flat_position_list() {
    // (beg0 end0 beg1 end1 ...) in char positions.
    assert_eq!(
        eval("(progn (string-match \"\\\\(a\\\\)\\\\(b\\\\)\" \"ab\") (match-data))"),
        "(0 2 0 1 1 2)"
    );
}

#[test]
fn string_match_p_does_not_disturb_callers() {
    assert_eq!(eval("(string-match-p \"^[0-9]+$\" \"12345\")"), "0");
    assert_eq!(eval("(string-match-p \"^[0-9]+$\" \"12a45\")"), "nil");
}

#[test]
fn save_match_data_restores_outer_match() {
    // inner string-match inside save-match-data must not clobber the outer match.
    assert_eq!(
        eval("(progn (string-match \"a\" \"a\") (save-match-data (string-match \"bb\" \"xbb\")) (match-beginning 0))"),
        "0"
    );
    // it returns the body's value.
    assert_eq!(
        eval("(save-match-data (string-match \"x\" \"x\") 99)"),
        "99"
    );
}

#[test]
fn shy_and_explicit_group_numbering() {
    // A shy group `\(?:…\)` does not capture: group 1 is the following real group.
    // (emacs 30.2: (match-string 1) => "b".)
    assert_eq!(
        eval("(progn (string-match \"\\\\(?:a\\\\(b\\\\)\\\\)\" \"ab\") (match-string 1 \"ab\"))"),
        "\"b\""
    );
    // Sequentially-numbered explicit groups `\(?1:…\)\(?2:…\)` match positionally,
    // which is the common font-lock case. (emacs 30.2: ("a" "b").)
    assert_eq!(
        eval("(progn (string-match \"\\\\(?1:a\\\\)\\\\(?2:b\\\\)\" \"ab\") (list (match-string 1 \"ab\") (match-string 2 \"ab\")))"),
        "(\"a\" \"b\")"
    );
}

#[test]
fn word_and_symbol_boundaries_and_case_fold() {
    // `\<` / `\>` word boundaries and `\_<` / `\_>` symbol boundaries.
    assert_eq!(eval("(string-match \"\\\\<word\\\\>\" \"a word b\")"), "2");
    assert_eq!(eval("(string-match \"\\\\_<foo\\\\_>\" \"x foo y\")"), "2");
    // case-fold-search: default folds case; nil is case-sensitive.
    assert_eq!(
        eval("(let ((case-fold-search t)) (string-match \"abc\" \"XABCY\"))"),
        "1"
    );
    assert_eq!(
        eval("(let ((case-fold-search nil)) (string-match \"abc\" \"XABCY\"))"),
        "nil"
    );
    // `\`` / `\'` buffer-string anchors and bounded `\{n,m\}` repetition.
    assert_eq!(eval("(string-match \"\\\\`abc\\\\'\" \"abc\")"), "0");
    assert_eq!(eval("(string-match \"a\\\\{2,3\\\\}\" \"baaac\")"), "1");
}

#[test]
fn regexp_quote_escapes_specials() {
    assert_eq!(eval("(regexp-quote \"a.b*c\")"), "\"a\\\\.b\\\\*c\"");
}

#[test]
fn replace_regexp_in_string() {
    assert_eq!(
        eval("(replace-regexp-in-string \"[0-9]+\" \"N\" \"a1b22c333\")"),
        "\"aNbNcN\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"o\" \"0\" \"foo\")"),
        "\"f00\""
    );
    // \1 backreference in the replacement template.
    assert_eq!(
        eval("(replace-regexp-in-string \"\\\\([a-z]\\\\)[0-9]\" \"\\\\1!\" \"a1b2\")"),
        "\"a!b!\""
    );
}
