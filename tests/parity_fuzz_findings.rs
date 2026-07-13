//! Parity gaps found by the differential fuzzer (`scripts/fuzz_parity.sh`).
//!
//! Each expectation here is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch --eval '(prin1 EXPR)'` — not of the running interpreter.
//! These are the cases the fuzzer surfaced that were NOT bignum-related; the
//! bignum ones live in `tests/bignums.rs`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Emacs prints a float as the shortest string that reads back as the same float,
/// laid out like C's `%g` — so it goes exponential once the decimal exponent
/// reaches the precision, and a subnormal collapses to one significant digit.
#[test]
fn floats_print_like_emacs() {
    assert_eq!(
        eval("(float most-positive-fixnum)"),
        "2.305843009213694e+18"
    );
    assert_eq!(eval("1e10"), "10000000000.0");
    assert_eq!(eval("1e-10"), "1e-10");
    assert_eq!(eval("1e300"), "1e+300");
    assert_eq!(eval("100.0"), "100.0");
    assert_eq!(eval("0.1"), "0.1");
    assert_eq!(eval("-0.0"), "-0.0");
    assert_eq!(eval("(/ 1.0 3)"), "0.3333333333333333");
    // Subnormals: gnulib's ftoastr starts its search at one significant digit.
    assert_eq!(eval("(ldexp 1.0 -1074)"), "5e-324");
    assert_eq!(
        eval("(list 1.0e+INF -1.0e+INF 0.0e+NaN)"),
        "(1.0e+INF -1.0e+INF 0.0e+NaN)"
    );
}

/// `match-data` reports only up to the last group that participated: a trailing
/// optional group that did not match contributes no `nil nil` pair.
#[test]
fn match_data_drops_trailing_unmatched_groups() {
    assert_eq!(
        eval("(progn (string-match \"\\\\(a+\\\\)\\\\(b\\\\)?\" \"xaaa\") (match-data))"),
        "(1 4 1 4)"
    );
    // A group that matched in the middle still reports its nil placeholder.
    assert_eq!(
        eval("(progn (string-match \"\\\\(x\\\\)?\\\\(a\\\\)\" \"a\") (match-data))"),
        "(0 1 nil nil 0 1)"
    );
}

/// The regexp errors carry Emacs's own wording, because elisp code catches
/// `invalid-regexp` and prints the message. Emacs also *tolerates* a repetition
/// operator with nothing to repeat and a reversed range.
#[test]
fn regexp_diagnostics_match_emacs() {
    let err = |re: &str| {
        eval(&format!(
            "(condition-case e (string-match {re} \"x\") (error e))"
        ))
    };
    assert_eq!(err(r#""[""#), r#"(invalid-regexp "Unmatched [ or [^")"#);
    assert_eq!(err(r#""\\(""#), r#"(invalid-regexp "Unmatched ( or \\(")"#);
    assert_eq!(err(r#""\\)""#), r#"(invalid-regexp "Unmatched ) or \\)")"#);
    assert_eq!(
        err(r#""a\\{2,1\\}""#),
        r#"(invalid-regexp "Invalid content of \\{\\}")"#
    );
    assert_eq!(err(r#""a\\{""#), r#"(invalid-regexp "Unmatched \\{")"#);
    assert_eq!(err(r#""\\""#), r#"(invalid-regexp "Trailing backslash")"#);
    // Not errors in Emacs: a leading `*` is a literal, `[z-a]` just never matches.
    assert_eq!(eval(r#"(string-match "*x" "*x")"#), "0");
    assert_eq!(eval(r#"(string-match "[z-a]" "x")"#), "nil");
}

/// `print-escape-control-characters` renders every remaining control character as
/// a backslash + octal escape.
#[test]
fn print_escape_control_characters() {
    assert_eq!(
        eval("(let ((print-escape-control-characters t)) (prin1-to-string \"a\\tb\"))"),
        r#""\"a\\11b\"""#
    );
    assert_eq!(
        eval("(let ((print-escape-control-characters t) (print-escape-newlines t)) (prin1-to-string \"a\\n\\tb\"))"),
        r#""\"a\\n\\11b\"""#
    );
}

/// Emacs's error *data* names the offending value and the predicate the specific
/// builtin checks — the two are not interchangeable, and a bare predicate with no
/// value is not what elisp code catches.
#[test]
fn error_data_carries_predicate_and_value() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // Bit *logic* takes a marker; the shifts and lognot/logcount do not.
    assert_eq!(
        e("(logand 1.0 2)"),
        "(wrong-type-argument integer-or-marker-p 1.0)"
    );
    assert_eq!(e("(ash 1.0 2)"), "(wrong-type-argument integerp 1.0)");
    assert_eq!(e("(lognot 3.0)"), "(wrong-type-argument integerp 3.0)");
    // The value, not a bare predicate, and never a raw heap handle.
    assert_eq!(
        e("(downcase nil)"),
        "(wrong-type-argument char-or-string-p nil)"
    );
    assert_eq!(e("(concat 45)"), "(wrong-type-argument sequencep 45)");
    assert_eq!(
        e("(string-join (list 1 2))"),
        "(wrong-type-argument sequencep 1)"
    );
    assert_eq!(
        e("(string= \"a\" 1.5)"),
        "(wrong-type-argument stringp 1.5)"
    );
    assert_eq!(e("(remq 1 t)"), "(wrong-type-argument listp t)");
    assert_eq!(
        e("(delq 1 (cons t 2))"),
        "(wrong-type-argument listp (t . 2))"
    );
    // The callee and the argument count.
    assert_eq!(
        e("(char-to-string)"),
        "(wrong-number-of-arguments char-to-string 0)"
    );
}

/// Sequence functions that Emacs defines in Lisp inherit that definition's
/// tolerances: `string-suffix-p` length-tests before it type-checks, `nconc`'s
/// last argument may be any object, `string-to-vector` takes any sequence.
#[test]
fn sequence_functions_match_their_lisp_definitions() {
    // `(- (length STRING) (length SUFFIX))` is negative → nil, no type check.
    assert_eq!(eval("(string-suffix-p \"aAbB\" [1 2])"), "nil");
    assert_eq!(eval("(nconc (list 1) (cons 2 \"s\"))"), "(1 2 . \"s\")");
    assert_eq!(eval("(string-to-vector nil)"), "[]");
    assert_eq!(eval("(string-to-list [1 2])"), "(1 2)");
    // seq-union dedups WITHIN the first sequence too, not just across the two.
    assert_eq!(eval("(seq-union (list 1 1 2) (list 2 3))"), "(1 2 3)");
    assert_eq!(eval("(seq-union \"line\" \"\")"), "(108 105 110 101)");
}
