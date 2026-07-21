//! `Obj::Record` parity: records are a distinct primitive type (not vectors),
//! with the type symbol in slot 0. This pins the fix for the slot-0 leak where
//! `(aref (record 'foo 1 2) 0)` used to return the internal `cl-struct-foo` tag
//! instead of `foo`.
//!
//! Every expectation was taken from GNU Emacs 30.2
//! (`emacs -Q --batch -l …`) and matches byte-for-byte.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// The core regression: slot 0 of a record is the *bare* type symbol, readable
/// via `aref`, `type-of`, and the printer alike — no `cl-struct-` tag leaks.
#[test]
fn record_slot0_is_the_bare_type_symbol() {
    assert_eq!(eval("(aref (record 'foo 1 2) 0)"), "foo");
    assert_eq!(eval("(type-of (record 'foo 1 2))"), "foo");
    assert_eq!(eval("(format \"%S\" (record 'foo 1 2))"), "\"#s(foo 1 2)\"");
    // `type-of` reports slot 0 verbatim, even when it is not a symbol.
    assert_eq!(eval("(type-of (record 5 1 2))"), "5");
}

/// A record is its own type: `recordp` accepts it, `vectorp` rejects it. This is
/// the type split that makes cl-defstruct/pcase/EIEIO dispatch faithful.
#[test]
fn record_is_distinct_from_vector() {
    assert_eq!(eval("(recordp (record 'foo 1 2))"), "t");
    assert_eq!(eval("(vectorp (record 'foo 1 2))"), "nil");
    assert_eq!(eval("(recordp [1 2 3])"), "nil");
    assert_eq!(eval("(vectorp [1 2 3])"), "t");
    assert_eq!(eval("(recordp (read \"#s(pt 3 4)\"))"), "t");
    assert_eq!(eval("(vectorp (read \"#s(pt 3 4)\"))"), "nil");
}

/// The array ops records DO support: `aref`, `aset`, `length`, and `equal`
/// (element-wise, including slot 0).
#[test]
fn record_supports_array_ops() {
    assert_eq!(eval("(length (record 'foo 1 2))"), "3");
    assert_eq!(
        eval("(let ((r (record 'foo 1 2))) (aset r 1 9) (aref r 1))"),
        "9"
    );
    assert_eq!(eval("(equal (record 'foo 1 2) (record 'foo 1 2))"), "t");
    // Different type slot -> not equal.
    assert_eq!(eval("(equal (record 'foo 1 2) (record 'bar 1 2))"), "nil");
    // Out-of-range names the record itself.
    assert_eq!(
        err("(aref (record 'foo 1 2) 5)"),
        "(args-out-of-range #s(foo 1 2) 5)"
    );
}

/// A record is NOT a sequence: the sequence combinators signal `sequencep`,
/// unlike a vector.
#[test]
fn record_is_not_a_sequence() {
    assert_eq!(
        err("(vconcat (record 'foo 1 2))"),
        "(wrong-type-argument sequencep #s(foo 1 2))"
    );
    assert_eq!(
        err("(append (record 'foo 1 2) nil)"),
        "(wrong-type-argument sequencep #s(foo 1 2))"
    );
    assert_eq!(
        err("(mapcar #'identity (record 'foo 1 2))"),
        "(wrong-type-argument sequencep #s(foo 1 2))"
    );
}

/// `copy-sequence` on a record returns a fresh record (type preserved) — this is
/// exactly the copier a cl-defstruct `copy-NAME' uses.
#[test]
fn copy_sequence_preserves_record_ness() {
    assert_eq!(
        eval("(let ((r (record 'foo 1 2)) ) (list (recordp (copy-sequence r)) (eq r (copy-sequence r)) (equal r (copy-sequence r))))"),
        "(t nil t)"
    );
}

/// `make-record` builds an INIT-filled record with the type in slot 0, and
/// signals exactly as Emacs does on a bad count or an over-large allocation.
#[test]
fn make_record_semantics_and_errors() {
    assert_eq!(
        eval("(format \"%S\" (make-record 'foo 3 'z))"),
        "\"#s(foo z z z)\""
    );
    assert_eq!(eval("(length (make-record 'foo 3 'z))"), "4");
    assert_eq!(
        err("(make-record 'foo -1 0)"),
        "(wrong-type-argument wholenump -1)"
    );
    assert_eq!(
        err("(make-record 'foo 1.5 0)"),
        "(wrong-type-argument wholenump 1.5)"
    );
    assert_eq!(
        err("(make-record 'foo 5000 0)"),
        "(error \"Attempt to allocate a record of 5001 slots; max is 4095\")"
    );
}

/// A `#s(NAME slot…)` literal reads back as a record whose slot 0 is NAME.
#[test]
fn record_read_syntax_roundtrips() {
    assert_eq!(
        eval(
            "(let ((r (read \"#s(pt 3 4)\"))) (list (aref r 0) (aref r 1) (aref r 2) (type-of r)))"
        ),
        "(pt 3 4 pt)"
    );
    // A record prints in a form that reads back equal.
    assert_eq!(
        eval("(equal (record 'q 1 'a) (read (format \"%S\" (record 'q 1 'a))))"),
        "t"
    );
}

/// cl-defstruct instances are records now: `aref …0` is the bare struct name,
/// `vectorp` is nil, and the printed form is `#s(NAME …)`.
#[test]
fn cl_defstruct_instances_are_records() {
    assert_eq!(
        eval("(cl-defstruct point x y) (let ((p (make-point :x 3 :y 4))) (list (aref p 0) (type-of p) (recordp p) (vectorp p)))"),
        "(point point t nil)"
    );
    assert_eq!(
        eval("(cl-defstruct point x y) (format \"%S\" (make-point :x 3 :y 4))"),
        "\"#s(point 3 4)\""
    );
    // A subtype's slot 0 is its own name; the supertype predicate still accepts it.
    assert_eq!(
        eval("(cl-defstruct an name) (cl-defstruct (dog (:include an)) breed) (let ((d (make-dog :name \"Rex\" :breed \"Lab\"))) (list (aref d 0) (an-p d) (dog-p (make-an))))"),
        "(dog t nil)"
    );
}
