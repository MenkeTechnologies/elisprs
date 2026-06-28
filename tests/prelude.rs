//! Prelude-library coverage: the elisp defined in `prelude.rs` (alist/plist
//! accessors, the seq-* and cl-* families, the c[ad]+r combinators, list
//! utilities, and the place-mutating macros). These run as ordinary elisp on
//! top of the builtins, so they exercise re-entrancy and closure capture too.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn cadr_family() {
    assert_eq!(eval("(caddr '(1 2 3 4))"), "3");
    assert_eq!(eval("(cddr '(1 2 3 4))"), "(3 4)");
    assert_eq!(eval("(list (caar '((1 2) 3)) (cdar '((1 . 9))))"), "(1 9)");
    assert_eq!(eval("(cadddr '(1 2 3 4 5))"), "4");
}

#[test]
fn alist_accessors() {
    assert_eq!(eval("(assoc 'b '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(eval("(assq 'b '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(eval("(alist-get 'b '((a . 1) (b . 2)))"), "2");
    assert_eq!(eval("(alist-get 'z '((a . 1)))"), "nil");
    assert_eq!(eval("(rassq 2 '((a . 1) (b . 2)))"), "(b . 2)");
    // assoc with list-valued entries composes with cadr.
    assert_eq!(eval("(cadr (assoc 'b '((a 1) (b 2) (c 3))))"), "2");
}

#[test]
fn plist_accessors() {
    assert_eq!(eval("(plist-get '(:a 1 :b 2) :b)"), "2");
    assert_eq!(eval("(plist-get '(:a 1 :b 2) :missing)"), "nil");
    assert_eq!(eval("(plist-member '(:a 1 :b 2) :b)"), "(:b 2)");
}

#[test]
fn membership() {
    assert_eq!(eval("(member 3 '(1 2 3 4))"), "(3 4)");
    assert_eq!(eval("(member 9 '(1 2 3))"), "nil");
    assert_eq!(eval("(memq 'c '(a b c d))"), "(c d)");
    assert_eq!(eval("(keywordp :foo)"), "t");
    assert_eq!(eval("(keywordp 'foo)"), "nil");
}

#[test]
fn list_construction_and_removal() {
    assert_eq!(eval("(number-sequence 1 5)"), "(1 2 3 4 5)");
    assert_eq!(eval("(number-sequence 0 10 2)"), "(0 2 4 6 8 10)");
    assert_eq!(eval("(make-list 3 'x)"), "(x x x)");
    assert_eq!(eval("(delq 2 (list 1 2 3 2 4))"), "(1 3 4)");
    assert_eq!(eval("(remove 2 '(1 2 3 2 4))"), "(1 3 4)");
    assert_eq!(eval("(delete 2 (list 1 2 2 3))"), "(1 3)");
    assert_eq!(eval("(remq 'a '(a b a c))"), "(b c)");
}

#[test]
fn list_accessors() {
    assert_eq!(eval("(last '(1 2 3 4))"), "(4)");
    assert_eq!(eval("(elt '(a b c) 1)"), "b");
    assert_eq!(eval("(elt (vector 10 20 30) 2)"), "30");
    assert_eq!(eval("(safe-length '(1 2 3))"), "3");
    assert_eq!(eval("(nthcdr 2 '(a b c d))"), "(c d)");
}

#[test]
fn seq_family() {
    assert_eq!(eval("(seq-map (lambda (x) (* x x)) '(1 2 3))"), "(1 4 9)");
    assert_eq!(eval("(seq-find (lambda (x) (> x 2)) '(1 2 3 4))"), "3");
    assert_eq!(eval("(seq-every-p (lambda (x) (> x 0)) '(1 2 3))"), "t");
    assert_eq!(eval("(seq-every-p (lambda (x) (> x 1)) '(1 2 3))"), "nil");
    assert_eq!(
        eval("(seq-some (lambda (x) (and (> x 2) x)) '(1 2 3))"),
        "3"
    );
    assert_eq!(eval("(seq-contains-p '(1 2 3) 2)"), "t");
    assert_eq!(
        eval("(seq-count (lambda (x) (evenp x)) '(1 2 3 4 5 6))"),
        "3"
    );
    assert_eq!(eval("(seq-reverse '(1 2 3))"), "(3 2 1)");
    assert_eq!(eval("(seq-empty-p nil)"), "t");
    assert_eq!(eval("(seq-empty-p '(1))"), "nil");
}

#[test]
fn cl_family() {
    assert_eq!(eval("(cl-reduce (lambda (a b) (+ a b)) '(1 2 3 4))"), "10");
    assert_eq!(
        eval("(cl-remove-if (lambda (x) (evenp x)) '(1 2 3 4 5))"),
        "(1 3 5)"
    );
    assert_eq!(
        eval("(cl-remove-if-not (lambda (x) (evenp x)) '(1 2 3 4 5))"),
        "(2 4)"
    );
    assert_eq!(eval("(cl-find-if (lambda (x) (> x 3)) '(1 2 3 4 5))"), "4");
    assert_eq!(eval("(cl-some (lambda (x) (> x 4)) '(1 2 3 4 5))"), "t");
    assert_eq!(eval("(cl-every (lambda (x) (> x 0)) '(1 2 3))"), "t");
    assert_eq!(
        eval("(list (cl-first '(a b c)) (cl-second '(a b c)) (cl-third '(a b c)))"),
        "(a b c)"
    );
    assert_eq!(eval("(cl-rest '(a b c))"), "(b c)");
}

#[test]
fn place_mutating_macros() {
    assert_eq!(eval("(let ((x 5)) (incf x) x)"), "6");
    assert_eq!(eval("(let ((x 5)) (decf x) x)"), "4");
    assert_eq!(eval("(let ((l '(1 2 3))) (pop l))"), "1");
    assert_eq!(eval("(let ((l '(1 2 3))) (pop l) l)"), "(2 3)");
    assert_eq!(eval("(let ((l '(2 3))) (push 1 l) l)"), "(1 2 3)");
}

#[test]
fn mapconcat_over_function_quote() {
    assert_eq!(
        eval("(mapconcat #'number-to-string '(1 2 3) \"+\")"),
        "\"1+2+3\""
    );
}
