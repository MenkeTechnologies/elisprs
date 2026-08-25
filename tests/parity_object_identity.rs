//! A string is an OBJECT, and `eq` is object identity — not content equality
//! and not "always nil".
//!
//! `Value::Str` is an `Arc<String>`, so two references to the same string share
//! one allocation and two equal literals do not. `el_eq` used to answer `t` only
//! for the empty string and `nil` for every other string pair, which made every
//! identity-based primitive lie about strings:
//!
//! ```text
//!                                   emacs 30.2   elisprs (before)
//! (let ((s "abc")) (eq s s))        t            nil
//! (let ((s "abc")) (memq s (list s)))  ("abc")   nil
//! (let ((s "ab"))  (delq s (list s 1)))  (1)     ("ab" 1)
//! ```
//!
//! The same round fixed `copy-sequence`, which returned the argument itself for
//! a string or a vector. That was worse than an identity gap: `(aset COPY 0 9)`
//! wrote through to the original, so every caller that copies before mutating
//! shared one object.
//!
//! Every expectation below is `emacs -Q --batch` on GNU Emacs 30.2 for the same
//! form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Two references to ONE string are `eq`; two separate literals are not.
#[test]
fn same_string_object_is_eq_but_equal_literals_are_not() {
    assert_eq!(eval("(let ((s \"abc\")) (eq s s))"), "t");
    assert_eq!(eval("(let ((s (copy-sequence \"abc\"))) (eq s s))"), "t");
    assert_eq!(eval("(eq \"abc\" \"abc\")"), "nil");
    assert_eq!(eval("(let ((a \"x\") (b \"x\")) (eq a b))"), "nil");
    assert_eq!(eval("(let ((s \"abc\")) (eql s s))"), "t");
}

/// Emacs keeps ONE shared empty string (`empty_unibyte_string`, alloc.c), so
/// every zero-length string is `eq` to every other however it was built.
#[test]
fn the_empty_string_is_still_one_shared_object() {
    assert_eq!(eval("(eq \"\" \"\")"), "t");
    assert_eq!(eval("(eq (make-string 0 ?a) \"\")"), "t");
    assert_eq!(eval("(let ((s \"\")) (eq s (copy-sequence s)))"), "t");
}

/// The identity-based list primitives all read `eq`, so they all changed answer
/// together. These are the four that a string key actually reaches.
#[test]
fn identity_list_ops_find_a_string_by_identity() {
    assert_eq!(
        eval("(let ((s \"abc\")) (memq s (list 1 s 2)))"),
        "(\"abc\" 2)"
    );
    assert_eq!(eval("(let ((s \"abc\")) (memql s (list s)))"), "(\"abc\")");
    assert_eq!(
        eval("(let ((s \"abc\")) (assq s (list (cons s 1))))"),
        "(\"abc\" . 1)"
    );
    assert_eq!(eval("(let ((s \"ab\")) (delq s (list s 1)))"), "(1)");
    assert_eq!(eval("(let ((s \"ab\")) (remq s (list s 1)))"), "(1)");
}

/// Every copier returns a FRESH object: `copy-sequence`, `substring` and
/// `concat` are each `nil` under `eq` against their argument.
#[test]
fn copiers_return_fresh_objects() {
    assert_eq!(eval("(let ((s \"abc\")) (eq s (copy-sequence s)))"), "nil");
    assert_eq!(eval("(let ((s \"abc\")) (eq s (substring s 0)))"), "nil");
    assert_eq!(eval("(let ((s \"abc\")) (eq s (concat s)))"), "nil");
    assert_eq!(eval("(let ((v [1 2])) (eq v (copy-sequence v)))"), "nil");
    // A list copy is fresh too — that path was already right.
    assert_eq!(eval("(let ((l '(1 2))) (eq l (copy-sequence l)))"), "nil");
}

/// The reason a fresh object matters: writing to the copy must NOT write to the
/// original. This is the regression `copy-sequence` returning its argument hid.
#[test]
fn a_vector_copy_does_not_alias_the_original() {
    assert_eq!(
        eval("(let* ((v (vector 1 2)) (c (copy-sequence v))) (aset c 0 9) (list v c))"),
        "([1 2] [9 2])"
    );
    assert_eq!(
        eval("(let* ((v (vector 1 2)) (c (copy-sequence v))) (equal v c))"),
        "t"
    );
}

/// `copy-sequence` on a string carries its text properties (Emacs copies the
/// interval tree with the characters), which is why the copier is `substring`
/// and not a property-dropping rebuild.
#[test]
fn a_string_copy_carries_text_properties() {
    assert_eq!(
        eval("(let ((s (propertize \"ab\" 'p 1))) (get-text-property 0 'p (copy-sequence s)))"),
        "1"
    );
    assert_eq!(eval("(let ((s \"abc\")) (equal s (copy-sequence s)))"), "t");
}

/// `copy-tree`'s second argument (VECTORS-AND-RECORDS, subr.el:877) descends
/// into vectors and records; without it they are shared, not copied.
#[test]
fn copy_tree_takes_the_vectors_and_records_flag() {
    // Without the flag a vector is the tree LEAF: it comes back as itself.
    assert_eq!(eval("(let ((v (vector 1))) (eq v (copy-tree v)))"), "t");
    assert_eq!(eval("(let ((v (vector 1))) (eq v (copy-tree v t)))"), "nil");
    // With the flag the vector's ELEMENTS are copied too — and so are a
    // record's slots, which is the branch `recordp` adds over `vectorp`.
    assert_eq!(
        eval("(let* ((v (vector (list 1))) (c (copy-tree v t))) (eq (aref v 0) (aref c 0)))"),
        "nil"
    );
    assert_eq!(
        eval("(let ((r (record 'x (list 1)))) (eq (aref r 1) (aref (copy-tree r t) 1)))"),
        "nil"
    );
    // The cdr walk copies a dotted tail verbatim.
    assert_eq!(eval("(copy-tree '(1 (2) . 3))"), "(1 (2) . 3)");
    assert_eq!(
        eval("(let* ((l (list (list 1))) (c (copy-tree l))) (eq (car l) (car c)))"),
        "nil"
    );
}
