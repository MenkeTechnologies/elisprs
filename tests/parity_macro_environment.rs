//! The ENVIRONMENT argument of `macroexpand`, `macroexpand-1` and
//! `macroexpand-all`.
//!
//! Expectations are GNU Emacs 31.1's: macroexp.el `macroexpand-1` consults
//! `(assq HEAD ENVIRONMENT)` before the function cell — an entry with a
//! function expands through it, an entry with a nil cdr stops expansion there
//! — and `macroexpand-all` binds `macroexpand-all-environment` to ENVIRONMENT
//! while it walks, which is how `cl-tagbody` expands its `go` forms.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

const GO_ENV: &str = "(list (cons 'go (lambda (l) (list 'throw l))))";

#[test]
fn an_environment_expander_shadows_the_function_cell() {
    assert_eq!(
        eval(&format!("(macroexpand '(go x) {GO_ENV})")),
        "(throw x)"
    );
    assert_eq!(
        eval(&format!("(macroexpand-1 '(go x) {GO_ENV})")),
        "(throw x)"
    );
    assert_eq!(
        eval(&format!(
            "(macroexpand-all '(list (go x) (when a (go y))) {GO_ENV})"
        )),
        "(list (throw x) (if a (progn (throw y))))"
    );
}

#[test]
fn a_nil_environment_entry_stops_expansion() {
    assert_eq!(
        eval("(macroexpand '(when a b) '((when . nil)))"),
        "(when a b)"
    );
    assert_eq!(
        eval("(macroexpand-1 '(when a b) '((when . nil)))"),
        "(when a b)"
    );
    assert_eq!(
        eval("(macroexpand-all '(list (when a b)) '((when . nil)))"),
        "(list (when a b))"
    );
}

#[test]
fn macroexpand_all_binds_macroexpand_all_environment() {
    assert_eq!(
        eval(
            "(let ((env (list (cons 'm (lambda () (list 'quote (length macroexpand-all-environment)))))))
               (list (macroexpand-all '(m) env) macroexpand-all-environment))"
        ),
        "('1 nil)"
    );
}
