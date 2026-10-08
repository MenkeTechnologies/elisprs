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

fn printed(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// eval.c `Ffunction` passes the body as written to
/// `cconv-make-interpreted-closure`, which keeps it when no lexical variable
/// is in scope and otherwise replaces it by its `macroexpand-all` (computing
/// the captures from that expansion). GNU Emacs 31.1, `emacs -Q --batch`.
#[test]
fn an_eval_closure_prints_its_body_as_cconv_leaves_it() {
    let rows = [
        (
            "(eval '(lambda (x) (when x 1)) nil)",
            "#[(x) ((when x 1)) nil]",
        ),
        (
            "(eval '(lambda (x) (when x 1)) t)",
            "#[(x) ((when x 1)) (t)]",
        ),
        (
            "(eval '(function (lambda () (push 1 z))) t)",
            "#[nil ((push 1 z)) (t)]",
        ),
        (
            "(eval '(let ((y 1)) (lambda (x) (when x y))) t)",
            "#[(x) ((if x (progn y))) ((y . 1))]",
        ),
        (
            "(eval '(let ((y 2)) (lambda () (add-to-list 'y 3))) t)",
            "#[nil ((if (member 3 y) y (setq y (cons 3 y)))) ((y . 2))]",
        ),
        (
            "(eval '(progn (defun ff1 (x) (when x (push x l))) (symbol-function 'ff1)) t)",
            "#[(x) ((when x (push x l))) (t)]",
        ),
        (
            "(eval '(let ((y 1)) (defun ff2 (x) (when x y)) (symbol-function 'ff2)) t)",
            "#[(x) ((if x (progn y))) ((y . 1))]",
        ),
    ];
    for (src, want) in rows {
        assert_eq!(printed(src), want, "{src}");
    }
}

/// With a lexical variable in scope the expansion happens when the closure is
/// made, so an expander's error is signalled there.
#[test]
fn an_expander_error_in_a_capturing_closure_is_signalled_at_creation() {
    // Under dynamic binding `Ffunction` makes no lexical closure, so nothing
    // is expanded and the body prints as written.
    let form = "(condition-case e (let ((y 1)) (lambda () (m) y)) (error (list 'err e)))";
    for (lexical, want) in [
        ("t", "(err (error \"boom\"))"),
        ("nil", "#[nil ((m) y) nil]"),
    ] {
        assert_eq!(
            printed(&format!("(progn {MACROS} (eval '{form} {lexical}))")),
            want,
            "lexical {lexical}"
        );
    }
    // With nothing to capture the body is kept, and only a call expands it.
    check(
        "(condition-case e (let ((f (lambda () (m)))) 'made) (error (list 'err e)))",
        "made",
    );
}
