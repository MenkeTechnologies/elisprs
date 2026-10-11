//! `cl-block` is lexically scoped, and `cl-defun` / `cl-defmacro` /
//! `cl-function` are cl-macs.el's `cl--transform-lambda`: an implicit block,
//! plain leading arguments kept in the lambda list, and the rest parsed by the
//! code `cl--do-arglist` generates.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch`, `(require 'cl-lib)`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn cl_block_tag_is_lexical() {
    // A `cl-return-from' in a function called from the block does not see it.
    assert_eq!(
        eval(
            "(progn (defun inner-ret () (cl-return-from outer 1)) \
               (condition-case e (cl-block outer (inner-ret) 2) (error e)))"
        ),
        "(void-variable --cl-block-outer--)"
    );
    // A closure that outlives its block throws to a tag nobody catches, even
    // inside a new block of the same name.
    assert_eq!(
        eval(
            "(let ((f nil)) (cl-block b (setq f (lambda () (cl-return-from b 1)))) \
               (condition-case e (cl-block b (funcall f) 2) (no-catch (list 'nc e))))"
        ),
        "(nc (no-catch (--cl-block-b--) 1))"
    );
    assert_eq!(
        eval("(cl-dolist (x '(1 2 3)) (when (= x 2) (cl-return x)))"),
        "2"
    );
}

#[test]
fn cl_block_expansion_drops_unused_blocks() {
    assert_eq!(
        eval("(macroexpand '(cl-block foo (bar)))"),
        "(cl--block-wrapper (let ((--cl-block-foo-- (cons '--cl-block-foo-- nil))) \
         (catch --cl-block-foo-- (bar))))"
    );
    assert_eq!(
        eval("(macroexpand '(cl-return-from foo 1))"),
        "(cl--block-throw --cl-block-foo-- 1)"
    );
    // Nothing returns from it: the wrapper's compiler macro keeps the body.
    assert_eq!(eval("(macroexpand-all '(cl-block foo (bar)))"), "(bar)");
    // A side-effect-free body never builds the block.
    assert_eq!(
        eval("(macroexpand-all '(cl-block foo (car x)))"),
        "(progn (car x))"
    );
    assert_eq!(
        eval("(symbol-function (progn (defun bw (x) (cl-block bw (if x (cl-return-from bw 1) 2))) 'bw))"),
        "#[(x) ((let ((--cl-block-bw-- (cons '--cl-block-bw-- nil))) \
         (catch --cl-block-bw-- (if x (throw --cl-block-bw-- 1) 2)))) (t)]"
    );
}

#[test]
fn cl_defun_body_is_an_implicit_block() {
    assert_eq!(
        eval("(progn (cl-defun g4 (a) (cl-return-from g4 (* 2 a)) 0) (g4 4))"),
        "8"
    );
    assert_eq!(
        eval(
            "(progn (cl-defun h6 (n) (cl-loop for i below n do (when (= i 2) (cl-return-from h6 i))) 'none) \
               (list (h6 5) (h6 1)))"
        ),
        "(2 none)"
    );
}

#[test]
fn simple_arguments_stay_in_the_lambda_list() {
    // A plain arglist is an ordinary function: the arity error names the closure.
    assert_eq!(
        eval("(condition-case e (progn (cl-defun f5 (a b) (list a b)) (f5 1)) (error e))"),
        "(wrong-number-of-arguments #[(a b) ((progn (list a b))) (t)] 1)"
    );
    assert_eq!(
        eval("(condition-case e (progn (cl-defun f6 (a &optional b) (list a b)) (f6 1 2 3)) (error e))"),
        "(wrong-number-of-arguments #[(a &optional b) ((progn (list a b))) (t)] 3)"
    );
    assert_eq!(
        eval("(func-arity (progn (cl-defun h8 (a &key b) a) 'h8))"),
        "(1 . many)"
    );
    // The rest of the arguments arrive in `--cl-rest--'; too many of them is
    // `(wrong-number-of-arguments NAME TOTAL)'.
    assert_eq!(
        eval("(condition-case e (progn (cl-defun g9 (a &optional (b 2)) (list a b)) (g9 1 2 3)) (error e))"),
        "(wrong-number-of-arguments g9 3)"
    );
    assert_eq!(
        eval("(progn (cl-defun h5 (x &key y) \"Doc5.\" (list x y)) (documentation 'h5))"),
        "\"Doc5.\n\n(fn X &key Y)\""
    );
}

#[test]
fn cl_function_takes_a_cl_lambda_list() {
    assert_eq!(eval("(funcall (cl-function (lambda (&key (a 3)) a)))"), "3");
    assert_eq!(
        eval("(funcall (cl-function (lambda (a &optional (b 7)) (list a b))) 1)"),
        "(1 7)"
    );
    assert_eq!(
        eval("(funcall (cl-function (lambda (&rest r &key a &allow-other-keys) (list r a))) :a 1 :b 2)"),
        "((:a 1 :b 2) 1)"
    );
    assert_eq!(
        eval("(mapcar (cl-function (lambda ((a . b)) (+ a b))) '((1 . 2) (3 . 4)))"),
        "(3 7)"
    );
    assert_eq!(eval("(funcall (cl-function (lambda (&aux (z 3)) z)))"), "3");
    assert_eq!(
        eval("(condition-case e (funcall (cl-function (lambda (&key a) a)) :b 1) (error e))"),
        "(error \"Keyword argument :b not one of (:a)\")"
    );
    assert_eq!(
        eval("(condition-case e (funcall (cl-function (lambda (a &key b) (list a b))) 1 :b) (error e))"),
        "(error \"Missing argument for :b\")"
    );
    // `cl-defmethod' builds its method with `cl-function', as cl-generic.el does.
    assert_eq!(
        eval("(progn (cl-defmethod gq2 ((x integer) &optional (y 3)) (+ x y)) (gq2 1))"),
        "4"
    );
}
