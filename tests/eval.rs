//! End-to-end evaluation tests driving the public `Interp` API.

use elisprs::Interp;

fn eval(src: &str) -> String {
    let mut it = Interp::new();
    let v = it.eval_str(src).expect("eval failed");
    it.print(&v, true)
}

#[test]
fn arithmetic() {
    assert_eq!(eval("(+ 1 2 3)"), "6");
    assert_eq!(eval("(* 2 (+ 3 4))"), "14");
    assert_eq!(eval("(/ 7 2)"), "3");
    assert_eq!(eval("(/ 7.0 2)"), "3.5");
    assert_eq!(eval("(- 5)"), "-5");
    assert_eq!(eval("(1+ 41)"), "42");
}

#[test]
fn recursion() {
    assert_eq!(
        eval("(progn (defun fact (n) (if (<= n 1) 1 (* n (fact (1- n))))) (fact 6))"),
        "720"
    );
}

#[test]
fn lists_and_hof() {
    assert_eq!(eval("(mapcar (lambda (x) (* x x)) '(1 2 3 4))"), "(1 4 9 16)");
    assert_eq!(eval("(mapcar #'1+ '(10 20 30))"), "(11 21 31)");
    assert_eq!(eval("(car '(a b c))"), "a");
    assert_eq!(eval("(cdr '(a b c))"), "(b c)");
    assert_eq!(eval("(length '(1 2 3 4))"), "4");
    assert_eq!(eval("(reverse '(1 2 3))"), "(3 2 1)");
}

#[test]
fn special_forms() {
    assert_eq!(eval("(let ((x 10) (y 20)) (+ x y))"), "30");
    assert_eq!(eval("(let* ((x 2) (y (* x 3))) y)"), "6");
    assert_eq!(eval("(if nil 1 2)"), "2");
    assert_eq!(eval("(cond ((= 1 2) 'a) ((= 1 1) 'b) (t 'c))"), "b");
    assert_eq!(eval("(and 1 2 3)"), "3");
    assert_eq!(eval("(or nil nil 7)"), "7");
}

#[test]
fn iteration() {
    assert_eq!(
        eval("(let ((acc 0) (i 1)) (while (<= i 5) (setq acc (+ acc i)) (setq i (1+ i))) acc)"),
        "15"
    );
}

#[test]
fn macros() {
    assert_eq!(
        eval("(progn (defmacro my-when (c &rest body) (list 'if c (cons 'progn body))) (my-when t 1 2 3))"),
        "3"
    );
}

#[test]
fn predicates() {
    assert_eq!(eval("(list (consp '(1)) (consp nil) (symbolp 'x) (stringp \"s\") (null nil))"),
        "(t nil t t t)");
    assert_eq!(eval("(eq 'a 'a)"), "t");
    assert_eq!(eval("(equal '(1 2) '(1 2))"), "t");
    assert_eq!(eval("(eq '(1 2) '(1 2))"), "nil");
}

#[test]
fn strings_and_format() {
    assert_eq!(eval("(format \"%s=%d hex=%x\" 'n 255 255)"), "\"n=255 hex=ff\"");
    assert_eq!(eval("(concat \"foo\" \"bar\")"), "\"foobar\"");
    assert_eq!(eval("(upcase \"abc\")"), "\"ABC\"");
}

#[test]
fn error_handling() {
    assert_eq!(
        eval("(condition-case e (/ 1 0) (arith-error 'handled))"),
        "handled"
    );
}

#[test]
fn dynamic_scope_is_the_default() {
    // Under lexical-binding nil (the m1 default), the returned lambda does NOT
    // close over `n`, so calling it later signals void-variable. This asserts
    // the documented behavior rather than a lexical-closure result.
    let mut it = Interp::new();
    let r = it.eval_str("(progn (defun adder (n) (lambda (x) (+ x n))) (funcall (adder 10) 5))");
    assert!(r.is_err(), "expected void-variable under dynamic scope");
}
