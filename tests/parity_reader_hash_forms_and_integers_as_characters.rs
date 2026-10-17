//! `#_NAME`, `#!` shebang lines, a trailing backslash at end of input, and
//! `print-integers-as-characters`.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value, or `!` and the
//! printed error object.

use elisprs::{eval_str, print, reset_host};

fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(let ((print-escape-newlines t) (print-circle t)) \
           (condition-case e (eval '{form} t) \
             (:success (concat \"=\" (prin1-to-string e))) \
             (error (concat \"!\" (prin1-to-string e)))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}

/// `#_` reads a symbol without shorthand expansion: always a symbol, possibly
/// empty.
#[test]
fn hash_underscore_reads_a_bare_symbol() {
    check(r###"(read "#_foo")"###, "=foo");
    check(r###"(read "(#_a #_b)")"###, "=(a b)");
    check(
        r##"(list (read "#_1") (symbolp (read "#_1")) (eq (read "#_") (intern "")))"##,
        r"=(\1 t t)",
    );
    check(r###"(symbol-name (read "#_\\ x"))"###, r###"=" x""###);
}

/// `#!` at the start of a token is a comment to end of line.
#[test]
fn hash_bang_is_a_line_comment() {
    check(r###"(read "#!x\ny")"###, "=y");
    check(r###"(read "(a #!x\n b)")"###, "=(a b)");
    check(
        r###"(condition-case e (read "#!x") (error e))"###,
        "=(end-of-file)",
    );
}

/// A backslash with nothing after it is end of file, not a bespoke error.
#[test]
fn trailing_backslash_is_end_of_file() {
    for src in [r###"\\"###, r###"a\\"###, r###"(a\\"###] {
        check(
            &format!(r###"(condition-case e (read "{src}") (error e))"###),
            "=(end-of-file)",
        );
    }
}

#[test]
fn integers_print_as_characters_when_asked() {
    check(
        r###"(let ((print-integers-as-characters t))
             (mapcar #'prin1-to-string '(-1 7 8 9 10 11 12 13 27 32 33 34 39 40 41 59 63 91 92 93 123 125 126 127 128 161 173 200 256 768 8203 8232 12288 57344 65535 128512 917505 1114112)))"###,
        r##"=("-1" "7" "?\\b" "?\\t" "?\\n" "11" "?\\f" "?\\r" "27" "?\\s" "?!" "?\\\"" "?\\'" "?\\(" "?\\)" "?\\;" "??" "?\\[" "?\\\\" "?\\]" "?\\{" "?\\}" "?~" "127" "128" "?¡" "173" "?È" "?Ā" "768" "8203" "8232" "12288" "?" "65535" "?😀" "917505" "1114112")"##,
    );
    check(
        r###"(let ((print-integers-as-characters t)) (list (prin1-to-string '(97 . 98)) (prin1-to-string [97 "a" 98.0]) (format "%S %s %d" 97 97 97) (prin1-to-string (expt 2 70))))"###,
        r##"=("(?a . ?b)" "[?a \"a\" 98.0]" "?a ?a 97" "1180591620717411303424")"##,
    );
    check(r###"(prin1-to-string '(97 10))"###, r###"="(97 10)""###);
}
