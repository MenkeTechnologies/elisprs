//! `rx` is the verbatim Emacs 31.1 `rx.el`, loaded on first use through its
//! autoloads; `pcase-defmacro` defines user pattern constructors.
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `rx` is a macro autoload: expanding the first use loads the library.
#[test]
fn rx_is_not_loaded_until_first_use() {
    assert_eq!(eval("(featurep 'rx)"), "nil");
    assert_eq!(eval("(progn (rx (+ digit)) (featurep 'rx))"), "t");
}

/// Non-constant `literal`/`regexp` operands make `rx` expand to a runtime
/// concatenation instead of a constant string.
#[test]
fn rx_literal_and_regexp_take_runtime_values() {
    assert_eq!(
        eval(r#"(let ((v "a.b")) (rx (literal v) "c"))"#),
        r#""a\\.bc""#
    );
    assert_eq!(
        eval(r#"(let ((v "a+")) (rx (regexp v) "c"))"#),
        r#""\\(?:a+\\)c""#
    );
}

/// `rx-let` / `rx-define` / `rx-let-eval` bind user definitions, with and
/// without parameters.
#[test]
fn rx_user_definitions() {
    assert_eq!(
        eval("(rx-let ((digs (+ digit))) (rx digs \"-\" digs))"),
        r#""[[:digit:]]+-[[:digit:]]+""#
    );
    assert_eq!(
        eval("(rx-let ((p (a) (seq a a))) (rx (p \"x\")))"),
        r#""xx""#
    );
    assert_eq!(
        eval("(rx-let-eval '((p (a) (seq a \"-\" a))) (rx-to-string '(p digit) t))"),
        r#""[[:digit:]]-[[:digit:]]""#
    );
}

#[test]
fn rx_forms_the_previous_subset_lacked() {
    assert_eq!(
        eval("(rx (intersection (any \"a-z\") (not (any \"m-p\"))))"),
        r#""[a-lq-z]""#
    );
    assert_eq!(eval("(rx \"a\" (eval (concat \"b\" \"c\")))"), r#""abc""#);
    assert_eq!(eval("(rx (+ word))"), r#""[[:word:]]+""#);
    assert_eq!(eval("(rx (or))"), r#""\\`a\\`""#);
    assert_eq!(eval("regexp-unmatchable"), r#""\\`a\\`""#);
}

/// The `rx` pcase pattern binds `(let VAR RX...)` submatches and supports
/// `(backref VAR)`.
#[test]
fn rx_pcase_pattern_binds_submatches() {
    assert_eq!(
        eval(r#"(pcase "abc" ((rx "a" (let x "b")) x))"#),
        r#""b""#
    );
    assert_eq!(
        eval(r#"(pcase "abab" ((rx (let z "ab") (backref z)) z))"#),
        r#""ab""#
    );
    assert_eq!(eval(r#"(pcase "abc" ((rx "a") 'ok))"#), "ok");
    assert_eq!(eval(r#"(pcase "xyz" ((rx "a") 'ok) (_ 'no))"#), "no");
}

#[test]
fn pcase_defmacro_defines_a_pattern() {
    assert_eq!(
        eval(
            "(progn (pcase-defmacro my-pair (a b) `(and (pred consp) (app car ,a) (app cdr ,b)))
                    (pcase '(1 . 2) ((my-pair x y) (list x y))))"
        ),
        "(1 2)"
    );
}

/// `funcall`/`apply` on a symbol whose definition is an autoload load the file
/// first; they used to hand the autoload object itself to the call path.
#[test]
fn funcall_and_apply_load_autoloads() {
    assert_eq!(
        eval("(funcall 'rx-to-string '(+ digit) t)"),
        r#""[[:digit:]]+""#
    );
    assert_eq!(
        eval(r#"(apply 'parse-time-string '("2000-01-02"))"#),
        "(nil nil nil 2 1 2000 nil -1 nil)"
    );
}
