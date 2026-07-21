//! nadvice.el parity: `advice-add`/`advice-remove`/`add-function`/
//! `remove-function`/`define-advice`/`advice-member-p` and every combinator.
//! This is source-symbol advice (nadvice.el), distinct from the glob-AOP
//! intercept layer (src/intercepts.rs).
//!
//! Every expectation was taken from GNU Emacs 30.2 (`emacs -Q --batch`, which
//! auto-loads the real nadvice.el) and matches byte-for-byte.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// All six public entry points exist.
#[test]
fn nadvice_api_is_bound() {
    assert_eq!(
        eval("(list (fboundp 'advice-add) (fboundp 'advice-remove) (fboundp 'add-function) (fboundp 'remove-function) (fboundp 'define-advice) (fboundp 'advice-member-p))"),
        "(t t t t t t)"
    );
}

/// The core combinators that wrap the original call.
#[test]
fn combinators_around_override_filters() {
    assert_eq!(
        eval("(defun f (x) (* x 10)) (advice-add 'f :around (lambda (o x) (list 'A (funcall o x)))) (f 5)"),
        "(A 50)"
    );
    assert_eq!(
        eval("(defun f (x) (* x 10)) (advice-add 'f :override (lambda (x) (list 'O x))) (f 5)"),
        "(O 5)"
    );
    assert_eq!(
        eval("(defun f (x) (* x 10)) (advice-add 'f :filter-args (lambda (args) (list (* 2 (car args))))) (f 5)"),
        "100"
    );
    assert_eq!(
        eval("(defun f (x) (* x 10)) (advice-add 'f :filter-return (lambda (r) (+ r 3))) (f 5)"),
        "53"
    );
}

/// `:before`/`:after` run for side effects and return the original's value.
#[test]
fn combinators_before_after_side_effects() {
    assert_eq!(
        eval("(defvar lg nil) (defun f (x) (* x 10)) (advice-add 'f :before (lambda (x) (push (list 'b x) lg))) (advice-add 'f :after (lambda (x) (push (list 'a x) lg))) (list (f 5) (reverse lg))"),
        "(50 ((b 5) (a 5)))"
    );
}

/// The short-circuiting combinators.
#[test]
fn combinators_while_until() {
    // :before-while runs the main only when the advice is non-nil.
    assert_eq!(
        eval("(defun f (x) (list 'main x)) (advice-add 'f :before-while (lambda (x) (> x 0))) (list (f 5) (f -1))"),
        "((main 5) nil)"
    );
    // :before-until runs the main only when the advice is nil.
    assert_eq!(
        eval("(defun f (x) (list 'main x)) (advice-add 'f :before-until (lambda (x) (and (< x 0) 'neg))) (list (f -2) (f 5))"),
        "(neg (main 5))"
    );
    // :after-while runs the advice only when the main is non-nil.
    assert_eq!(
        eval("(defun f (x) (if (> x 0) (list 'main x))) (advice-add 'f :after-while (lambda (x) (list 'aw x))) (list (f 5) (f -1))"),
        "((aw 5) nil)"
    );
    // :after-until runs the advice only when the main is nil.
    assert_eq!(
        eval("(defun f (x) (if (> x 0) (list 'main x))) (advice-add 'f :after-until (lambda (x) (list 'au x))) (list (f 5) (f -1))"),
        "((main 5) (au -1))"
    );
}

/// Stacking two advices composes them; `depth` orders inner vs outer.
#[test]
fn stacking_and_depth() {
    assert_eq!(
        eval("(defun f (x) (* x 10)) (advice-add 'f :around (lambda (o x) (+ 1 (funcall o x)))) (advice-add 'f :filter-return (lambda (r) (list 'fr r))) (f 5)"),
        "(fr 51)"
    );
    // A depth-100 advice is installed innermost (closest to the original).
    assert_eq!(
        eval("(defun f () (list 'main)) (advice-add 'f :filter-return (lambda (r) (cons 'outer r))) (advice-add 'f :filter-return (lambda (r) (cons 'inner r)) '((depth . 100))) (f)"),
        "(outer inner main)"
    );
}

/// A named advice can be tested with `advice-member-p` and removed by name.
#[test]
fn named_advice_member_and_remove() {
    assert_eq!(
        eval("(defun f () 'main) (advice-add 'f :override (lambda () 'ov) '((name . myov))) (let ((before (list (and (advice-member-p 'myov 'f) t) (f)))) (advice-remove 'f 'myov) (append before (list (advice-member-p 'myov 'f) (f))))"),
        "(t ov nil main)"
    );
}

/// Re-adding the identical advice does not duplicate it.
#[test]
fn re_add_does_not_duplicate() {
    assert_eq!(
        eval("(defun f () 0) (let ((a (lambda (r) (1+ r)))) (advice-add 'f :filter-return a) (advice-add 'f :filter-return a) (f))"),
        "1"
    );
}

/// `advice-remove` restores the original definition once the last advice is gone.
#[test]
fn remove_restores_original() {
    assert_eq!(
        eval("(defun f (x) (* x 10)) (let ((a (lambda (o x) (funcall o (1+ x))))) (advice-add 'f :around a) (let ((advised (f 5))) (advice-remove 'f a) (list advised (f 5))))"),
        "(60 50)"
    );
}

/// `define-advice` defines a named advice `SYMBOL@NAME` and installs it.
#[test]
fn define_advice_named() {
    assert_eq!(
        eval("(defun f (x) (* x 10)) (define-advice f (:filter-return (r) plus1) (1+ r)) (list (f 5) (fboundp 'f@plus1) (and (advice-member-p 'plus1 'f) t))"),
        "(51 t t)"
    );
}

/// `add-function`/`remove-function` operate on a function-valued place (here a
/// lexical `var`), independent of any named symbol.
#[test]
fn add_function_on_a_var_place() {
    assert_eq!(
        eval("(let ((fn (lambda (x) (* x 2)))) (add-function :filter-return (var fn) (lambda (r) (+ r 100))) (let ((a (funcall fn 5))) (remove-function (var fn) (lambda (r) (+ r 100))) a))"),
        "110"
    );
}

/// `advice-mapc` enumerates every installed advice with its props.
#[test]
fn advice_mapc_enumerates() {
    assert_eq!(
        eval("(defun f () 0) (advice-add 'f :filter-return (lambda (r) r) '((name . n1))) (advice-add 'f :filter-return (lambda (r) r) '((name . n2))) (let ((names nil)) (advice-mapc (lambda (_f p) (push (cdr (assq 'name p)) names)) 'f) names)"),
        "(n1 n2)"
    );
}
