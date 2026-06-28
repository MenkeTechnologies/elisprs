//! End-to-end tests: elisp source is lowered to a fusevm chunk and executed on
//! fusevm (no bespoke interpreter). Each test resets the thread-local host.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn arithmetic() {
    assert_eq!(eval("(+ 1 2 3)"), "6");
    assert_eq!(eval("(* 2 (+ 3 4))"), "14");
    assert_eq!(eval("(/ 7.0 2)"), "3.5");
    assert_eq!(eval("(- 5)"), "-5");
    assert_eq!(eval("(1+ 41)"), "42");
}

#[test]
fn cons_cells_are_real_heap_objects() {
    // The whole point of moving off rust_lisp: dotted pairs + identity + mutation.
    assert_eq!(eval("(cons 1 2)"), "(1 . 2)");
    assert_eq!(eval("(quote (a b . c))"), "(a b . c)");
    assert_eq!(eval("(progn (setq p (cons 1 2)) (setcar p 9) p)"), "(9 . 2)");
    assert_eq!(eval("(car (cons 1 2))"), "1");
    assert_eq!(eval("(cdr (cons 1 2))"), "2");
}

#[test]
fn lists() {
    assert_eq!(eval("(list 1 2 3)"), "(1 2 3)");
    assert_eq!(eval("(reverse (list 1 2 3))"), "(3 2 1)");
    assert_eq!(eval("(length (list 1 2 3 4))"), "4");
    assert_eq!(eval("(append (list 1 2) (list 3 4))"), "(1 2 3 4)");
    assert_eq!(eval("(nth 2 (list 10 20 30 40))"), "30");
}

#[test]
fn special_forms() {
    assert_eq!(eval("(if (eq 1 1) (quote y) (quote n))"), "y");
    assert_eq!(eval("(if nil 1 2)"), "2");
    assert_eq!(eval("(and 1 2 3)"), "3");
    assert_eq!(eval("(or nil nil 7)"), "7");
    assert_eq!(eval("(when t 1 2 3)"), "3");
    assert_eq!(eval("(unless nil 5)"), "5");
    assert_eq!(eval("(progn (setq x 10) (+ x 5))"), "15");
}

#[test]
fn equality_identity_vs_structural() {
    assert_eq!(eval("(eq (quote a) (quote a))"), "t");
    assert_eq!(eval("(equal (list 1 2) (list 1 2))"), "t");
    assert_eq!(eval("(eq (list 1 2) (list 1 2))"), "nil");
}

#[test]
fn predicates() {
    assert_eq!(eval("(list (consp (cons 1 2)) (consp nil) (symbolp (quote x)) (stringp \"s\") (null nil))"),
        "(t nil t t t)");
    assert_eq!(eval("(integerp 5)"), "t");
    assert_eq!(eval("(floatp 5.0)"), "t");
}

#[test]
fn vectors() {
    assert_eq!(eval("(aref (vector 10 20 30) 1)"), "20");
    assert_eq!(eval("(progn (setq v (make-vector 3 0)) (aset v 1 9) v)"), "[0 9 0]");
    assert_eq!(eval("(length (vector 1 2 3))"), "3");
}

#[test]
fn strings_and_format() {
    assert_eq!(eval("(format \"%s=%d hex=%x\" (quote n) 255 255)"), "\"n=255 hex=ff\"");
    assert_eq!(eval("(concat \"foo\" \"bar\")"), "\"foobar\"");
}

#[test]
fn unsupported_special_forms_error_clearly() {
    // defun/let/lambda await the calling-convention milestone; they must fail
    // loudly, not miscompile.
    reset_host();
    assert!(eval_str("(defun f (x) x)").is_err());
    reset_host();
    assert!(eval_str("(let ((x 1)) x)").is_err());
}
