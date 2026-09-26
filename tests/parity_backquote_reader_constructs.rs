//! Backquote as Emacs reads, expands and prints it.
//!
//! Expectations are GNU Emacs 31.1's: the reader makes `` `X ``, `,X`, `,@X`
//! into `` (\` X) ``, `(\, X)`, `(\,@ X)`; the `` \` `` macro is
//! backquote.el's `backquote-process`; the printer abbreviates `(\, X)` only
//! inside a backquote; and `pcase` matches `` `QPAT `` structurally.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

#[test]
fn backquote_reads_as_data() {
    assert_eq!(eval("(car '`(a ,b))"), "\\`");
    assert_eq!(
        eval("(mapcar #'symbol-name (list (car '`a) (car (cadr '`,a)) (car (cadr '`,@a))))"),
        "(\"`\" \",\" \",@\")"
    );
    assert_eq!(eval("'`(a ,b ,@c)"), "`(a ,b ,@c)");
    assert_eq!(eval("(read \"(a . ,b)\")"), "(a \\, b)");
}

#[test]
fn nested_backquotes_keep_their_levels() {
    assert_eq!(eval("(let ((x 'y)) ``(a ,,x))"), "`(a ,y)");
    assert_eq!(eval("(let ((x '(+ 1 2))) ``(,,x))"), "`(,(+ 1 2))");
    assert_eq!(eval("(let ((xs '(1 2))) ``(a ,@,xs))"), "`(a ,@(1 2))");
    assert_eq!(eval("(let ((xs '(1 2))) ``(a ,,@xs))"), "`(a (\\, 1 2))");
}

#[test]
fn the_expansion_is_backquote_els() {
    assert_eq!(
        eval(
            "(list (macroexpand '`(a ,b)) (macroexpand '`(a ,@b c)) (macroexpand '`[a ,b]) \
              (macroexpand '`(a . ,b)) (macroexpand '`a) (macroexpand '`,a) \
              (macroexpand '`(1 2)) (macroexpand '`(a ,@b)))"
        ),
        "((list 'a b) (cons 'a (append b '(c))) (vector 'a b) (cons 'a b) 'a a '(1 2) (cons 'a b))"
    );
    // A constant template is one quoted object, as in Emacs.
    assert_eq!(
        eval("(progn (defun bq-k () `(1 2)) (eq (bq-k) (bq-k)))"),
        "t"
    );
    assert_eq!(
        err("(macroexpand '(\\` (a (\\, d e))))"),
        "(error \"Multiple args to , are not supported: (\\\\, d e)\")"
    );
}

#[test]
fn comma_abbreviates_only_inside_a_backquote() {
    assert_eq!(eval("'((\\` a) (\\, b))"), "(`a (\\, b))");
    assert_eq!(eval("'(\\` (\\, (\\, (\\, a))))"), "`,(\\, (\\, a))");
    assert_eq!(eval("'(\\` [(\\, a) (\\,@ b)])"), "`[,a ,@b]");
    assert_eq!(
        eval("(let ((print-quoted nil)) (prin1-to-string '(\\` (\\, a))))"),
        "\"(\\\\` (\\\\, a))\""
    );
}

#[test]
fn pcase_matches_backquote_patterns() {
    assert_eq!(
        eval("(pcase '(1 (2 3)) (`(,a (,b ,c)) (list c b a)))"),
        "(3 2 1)"
    );
    assert_eq!(eval("(pcase '(k . 5) (`(k . ,v) v))"), "5");
    assert_eq!(eval("(pcase '(j . 5) (`(k . ,v) v) (_ 'no))"), "no");
    assert_eq!(eval("(pcase [1 2] (`[,a ,b] (+ a b)))"), "3");
    assert_eq!(eval("(pcase [1 2 3] (`[,a ,b] (+ a b)) (_ 'len))"), "len");
    assert_eq!(eval("(pcase 7 (`[,a] a) (_ 'not-vector))"), "not-vector");
    assert_eq!(
        eval("(funcall (pcase-lambda (`(,a ,b)) (+ a b)) (list 1 2))"),
        "3"
    );
}
