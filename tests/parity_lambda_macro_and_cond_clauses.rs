//! `lambda` as the subr.el macro, and `cond` clauses under `macroexpand-all`.
//!
//! - subr.el `(defmacro lambda (&rest cdr) (list 'function (cons 'lambda cdr)))`:
//!   `macroexpand` / `macroexpand-1` of a lambda form give `#'(lambda ...)`, and
//!   `lambda` is fbound to that macro.
//! - macroexp.el expands every element of every `cond` clause as a form
//!   (`macroexp--all-clauses`), so a `(lambda ...)` condition is wrapped too.
//!
//! Every expectation is `emacs -Q --batch` (GNU Emacs 31.1) printing
//! `(condition-case e FORM (error (list 'signal e)))` with cl-lib and seq loaded.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn lambda_is_a_macro() {
    check(
        r##"(condition-case e (list (macroexpand '(lambda (x) x)) (macroexpand-1 '(lambda (x) x)) (macroexpand-all '(lambda (x) x))) (error (list 'signal e)))"##,
        r##"(#'(lambda (x) x) #'(lambda (x) x) #'(lambda (x) x))"##,
    );
    check(
        r##"(condition-case e (list (fboundp 'lambda) (macrop 'lambda) (car (symbol-function 'lambda)) (functionp 'lambda) (special-form-p 'lambda) (func-arity 'lambda)) (error (list 'signal e)))"##,
        r##"(t t macro nil nil (0 . many))"##,
    );
    check(
        r##"(condition-case e (list (funcall (lambda (x) (* 2 x)) 3) (eval '(lambda (x) x) nil)) (error (list 'signal e)))"##,
        r##"(6 #[(x) (x) nil])"##,
    );
}

#[test]
fn cond_clauses_expand_every_form() {
    check(
        r##"(condition-case e (macroexpand-all '(cond ((lambda ()) 1))) (error (list 'signal e)))"##,
        r##"(cond (#'(lambda nil) 1))"##,
    );
    check(
        r##"(condition-case e (macroexpand-all '(cond ((when a b) (unless c d)) (t (lambda () 1)))) (error (list 'signal e)))"##,
        r##"(cond ((if a (progn b)) (if c nil d)) (t #'(lambda nil 1)))"##,
    );
    check(
        r##"(condition-case e (list (macroexpand-all '(cond)) (macroexpand-all '(cond (x)))) (error (list 'signal e)))"##,
        r##"((cond) (cond (x)))"##,
    );
    check(
        r##"(condition-case e (list (cond ((lambda ()) 'yes)) (let ((x 3)) (cond ((> x 2) (when t 'big)) (t 'small)))) (error (list 'signal e)))"##,
        r##"(yes big)"##,
    );
}
