//! `substring`/`concat` plist order, `format` returning an existing string, and
//! `lookup-key` over an unbound prefix.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value.

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

/// `copy_text_properties` (substring, concat) re-adds each pair, so the plist
/// comes out reversed; `copy-sequence` copies the interval tree and keeps it.
#[test]
fn substring_and_concat_reverse_the_plist_copy_sequence_keeps_it() {
    let s = r#"(propertize "abc" 'a 1 'b 2 'c 3)"#;
    check(
        &format!("(substring {s} 1)"),
        r#"=#("bc" 0 2 (c 3 b 2 a 1))"#,
    );
    check(
        &format!("(concat {s} \"x\")"),
        r#"=#("abcx" 0 3 (c 3 b 2 a 1))"#,
    );
    check(
        &format!("(concat \"x\" {s})"),
        r#"=#("xabc" 1 4 (c 3 b 2 a 1))"#,
    );
    check(
        &format!("(mapconcat #'identity (list {s} {s}) \"\")"),
        r#"=#("abcabc" 0 3 (c 3 b 2 a 1) 3 6 (c 3 b 2 a 1))"#,
    );
    check(
        &format!("(copy-sequence {s})"),
        r#"=#("abc" 0 3 (a 1 b 2 c 3))"#,
    );
    // Two reversals restore the original order.
    check(
        &format!("(substring (substring {s} 0) 0)"),
        r#"=#("abc" 0 3 (a 1 b 2 c 3))"#,
    );
}

/// styled_format answers the format string itself when it has no `%`, and the
/// argument itself for a bare "%s" over a string.
#[test]
fn format_hands_back_existing_strings() {
    check(
        r#"(let ((u (copy-sequence "abc"))) (list (eq u (format u)) (eq u (format "%s" u)) (eq u (format "%s " u)) (eq u (format "%.2s" u))))"#,
        "=(t t nil nil)",
    );
    check(
        r#"(let ((f (copy-sequence "a%%b"))) (eq f (format f)))"#,
        "=nil",
    );
    check(
        r#"(format "%s" (propertize "abc" 'a 1 'b 2 'c 3))"#,
        r#"=#("abc" 0 3 (a 1 b 2 c 3))"#,
    );
    check(
        r#"(format "x%s" (propertize "abc" 'a 1 'b 2 'c 3))"#,
        r#"=#("xabc" 1 4 (c 3 b 2 a 1))"#,
    );
}

/// `lookup_key_1`: an unbound event with more events behind it answers the
/// number of events consumed, exactly as a bound non-prefix does.
#[test]
fn lookup_key_unbound_prefix_answers_consumed_count() {
    check(
        r#"(let ((m (make-sparse-keymap))) (list (lookup-key m "ab") (lookup-key m "\30a") (lookup-key m [?a ?b ?c]) (lookup-key m "a")))"#,
        "=(1 1 1 nil)",
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (list (lookup-key m "ab") (lookup-key m "zab") (lookup-key m "z")))"#,
        "=(1 1 nil)",
    );
}
