//! Standard-library coverage: the string, cl-lib, seq, and list-utility surface
//! in `prelude.rs` (plus a few builtins) that the other suites don't reach.
//! Expectations captured from the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn string_comparison_and_predicates() {
    assert_eq!(eval("(string-equal \"ab\" \"ab\")"), "t");
    assert_eq!(eval("(string-equal \"ab\" \"ac\")"), "nil");
    assert_eq!(eval("(string-lessp \"abc\" \"abd\")"), "t");
    assert_eq!(eval("(string-greaterp \"abd\" \"abc\")"), "t");
    assert_eq!(eval("(string-or-null-p \"x\")"), "t");
    assert_eq!(eval("(string-or-null-p nil)"), "t");
    assert_eq!(eval("(string-or-null-p 5)"), "nil");
}

#[test]
fn string_trimming() {
    assert_eq!(eval("(string-trim-left \"  hi  \")"), "\"hi  \"");
    assert_eq!(eval("(string-trim-right \"  hi  \")"), "\"  hi\"");
}

#[test]
fn cl_ordinals() {
    assert_eq!(
        eval("(list (cl-fifth '(1 2 3 4 5 6)) (cl-sixth '(1 2 3 4 5 6)))"),
        "(5 6)"
    );
    assert_eq!(
        eval("(list (cl-seventh '(1 2 3 4 5 6 7 8)) (cl-eighth '(1 2 3 4 5 6 7 8)))"),
        "(7 8)"
    );
    assert_eq!(
        eval("(list (cl-ninth '(1 2 3 4 5 6 7 8 9 10)) (cl-tenth '(1 2 3 4 5 6 7 8 9 10)))"),
        "(9 10)"
    );
}

#[test]
fn cl_predicates_and_places() {
    assert_eq!(eval("(cl-minusp -3)"), "t");
    assert_eq!(eval("(cl-minusp 3)"), "nil");
    // cl-incf/cl-decf take an optional step amount.
    assert_eq!(eval("(let ((x 5)) (cl-incf x 2) x)"), "7");
    assert_eq!(eval("(let ((x 5)) (cl-decf x) x)"), "4");
    assert_eq!(eval("(cl-assoc 'b '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(eval("(cl-delete 2 (list 1 2 3 2))"), "(1 3)");
}

#[test]
fn seq_extras() {
    assert_eq!(eval("(seq-union '(1 2 3) '(3 4 5))"), "(1 2 3 4 5)");
    assert_eq!(eval("(seq-elt '(a b c) 2)"), "c");
    assert_eq!(eval("(seq-first '(10 20))"), "10");
    assert_eq!(eval("(seq-rest '(10 20 30))"), "(20 30)");
    assert_eq!(eval("(seq-length '(1 2 3))"), "3");
    assert_eq!(eval("(seq-remove #'evenp '(1 2 3 4))"), "(1 3)");
    // seq-do-indexed passes (element index) to the function, in order.
    assert_eq!(
        eval(
            "(let ((s nil)) (seq-do-indexed (lambda (x i) (setq s (cons (cons i x) s))) '(a b)) s)"
        ),
        "((1 . b) (0 . a))"
    );
}

#[test]
fn deep_cxr_and_list_utils() {
    assert_eq!(eval("(caaar '(((1))))"), "1");
    assert_eq!(eval("(cdddr '(1 2 3 4 5))"), "(4 5)");
    assert_eq!(eval("(cddddr '(1 2 3 4 5))"), "(5)");
    assert_eq!(eval("(nreverse (list 1 2 3))"), "(3 2 1)");
    assert_eq!(eval("(copy-sequence '(1 2 3))"), "(1 2 3)");
    assert_eq!(eval("(flatten-list '(1 (2 (3 4)) 5))"), "(1 2 3 4 5)");
    assert_eq!(eval("(ntake 2 (list 1 2 3 4))"), "(1 2)");
}

#[test]
fn add_to_list_on_special_var() {
    // add-to-list mutates a (special) variable's list value; new items go on
    // the front, present items are a no-op.
    assert_eq!(
        eval("(progn (defvar al '(a b)) (add-to-list 'al 'c) al)"),
        "(c a b)"
    );
    assert_eq!(
        eval("(progn (defvar al2 '(a b)) (add-to-list 'al2 'a) al2)"),
        "(a b)"
    );
}

#[test]
fn hash_table_keys_and_values() {
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 1 h) (hash-table-keys h))"),
        "(a)"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 9 h) (hash-table-values h))"),
        "(9)"
    );
}

#[test]
fn type_predicates() {
    assert_eq!(eval("(characterp 65)"), "t");
    assert_eq!(eval("(characterp 'x)"), "nil");
    assert_eq!(eval("(arrayp (vector 1 2))"), "t");
    assert_eq!(eval("(arrayp \"str\")"), "t");
    assert_eq!(eval("(arrayp '(1 2))"), "nil");
}

#[test]
fn control_macros() {
    assert_eq!(eval("(prog2 1 2 3)"), "2");
    // with-demoted-errors returns the body value on success, nil on a demoted error.
    assert_eq!(eval("(with-demoted-errors \"e: %s\" (+ 1 2))"), "3");
    assert_eq!(
        eval("(with-demoted-errors \"e: %s\" (error \"boom\"))"),
        "nil"
    );
}
