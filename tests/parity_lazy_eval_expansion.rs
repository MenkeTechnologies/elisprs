//! `eval` expands a macro call only when the call is reached (eval.c
//! `eval_sub`), so an expander's error or `throw` happens at that point, inside
//! whatever `condition-case` or `catch` the evaluated form sets up.
//!
//! elisprs expands the whole form before compiling it, so every row below
//! used to escape the form entirely. Expectations are GNU Emacs 31.1
//! (`emacs -Q --batch`), each form passed through `(eval FORM t)` and
//! `(eval FORM nil)`, which agree on all of them.

use elisprs::{eval_str, print, reset_host};

const MACROS: &str = "(defmacro m () (error \"boom\")) (defmacro mt () (throw 'x 'thrown))";

fn check(form: &str, want: &str) {
    for lexical in ["t", "nil"] {
        reset_host();
        let src = format!(
            "(progn {MACROS} (condition-case e (eval '{form} {lexical}) (error (list 'outer e))))"
        );
        let v = eval_str(&src).expect("eval failed");
        assert_eq!(print(&v, true), want, "{form} / lexical {lexical}");
    }
}

#[test]
fn an_unreached_macro_call_is_never_expanded() {
    check(
        "(condition-case e (if nil (m) 'ok) (error (list 'err e)))",
        "ok",
    );
    check(
        "(condition-case e (funcall (lambda () (if nil (m) 'ok2))) (error (list 'err e)))",
        "ok2",
    );
    // A closure built with nothing to capture keeps its body unexpanded.
    check(
        "(condition-case e (let ((f (lambda () (m)))) 'made) (error (list 'err e)))",
        "made",
    );
}

#[test]
fn the_expanders_error_is_caught_inside_the_form() {
    check(
        "(condition-case e (m) (error (list 'err e)))",
        "(err (error \"boom\"))",
    );
    // Forms before the macro call have already run.
    check(
        "(let ((n 0)) (condition-case e (progn (setq n 1) (m)) (error (list n e))))",
        "(1 (error \"boom\"))",
    );
}

#[test]
fn the_expanders_throw_reaches_a_catch_inside_the_form() {
    check("(catch 'x (if nil (mt) 'ok3))", "ok3");
    check("(catch 'x (list 1 (mt)))", "thrown");
    check("(mt)", "(outer (no-catch x thrown))");
}
