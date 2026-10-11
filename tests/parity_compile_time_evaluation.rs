//! What "compile time" means to interpreted code: `eval-when-compile` /
//! `eval-and-compile` run their body when they are expanded, and a compiler
//! macro installed by any route (`put`, `define-symbol-prop`,
//! `cl-define-compiler-macro`) applies to the eager expansion that follows.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch`, `(require 'cl-lib)`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn eval_when_compile_runs_at_expansion_and_leaves_a_constant() {
    // byte-run.el: `(list 'quote (eval (cons 'progn body) lexical-binding))'.
    assert_eq!(eval("(macroexpand '(eval-when-compile (+ 1 2)))"), "'3");
    assert_eq!(eval("(macroexpand '(eval-and-compile (+ 1 2)))"), "'3");
    assert_eq!(
        eval("(symbol-function (defun ewc-user () (eval-when-compile (list 1 2))))"),
        "#[nil ('(1 2)) (t)]"
    );
    // Expanded once, so a loop around it does not re-run the body.
    assert_eq!(
        eval(
            "(progn (dotimes (_ 3) (eval-when-compile \
               (setq ewc-count (1+ (if (boundp (quote ewc-count)) ewc-count 0))))) ewc-count)"
        ),
        "1"
    );
    // The body is evaluated outside the code around it: no lexical `x'.
    assert_eq!(
        eval("(condition-case e (macroexpand-all '(let ((x 1)) (eval-and-compile x))) (error e))"),
        "(void-variable x)"
    );
    assert_eq!(
        eval("(condition-case e (macroexpand-all '(let ((x 1)) (eval-when-compile x))) (error e))"),
        "(void-variable x)"
    );
}

#[test]
fn compiler_macro_installed_by_put_reaches_eager_expansion() {
    // `cl-define-compiler-macro' stores the handler with `define-symbol-prop';
    // its `eval-and-compile' installs it while the enclosing form expands, so
    // the calls later in the same form already see it.
    assert_eq!(
        eval(
            "(progn (cl-define-compiler-macro my-sq3 (x) `(* ,x ,x)) (defun my-sq3 (x) (+ x x)) \
               (list (macroexpand-all '(my-sq3 4)) (my-sq3 4) \
                     (symbol-function (defun my-sq3-user (a) (my-sq3 a)))))"
        ),
        "((* 4 4) 16 #[(a) ((* a a)) (t)])"
    );
    assert_eq!(
        eval(
            "(progn (put 'my-c1 'compiler-macro (lambda (form x) (if (numberp x) (* 10 x) form))) \
               (defun my-c1 (x) x) (macroexpand-all '(list (my-c1 2) (my-c1 y))))"
        ),
        "(list 20 (my-c1 y))"
    );
}
