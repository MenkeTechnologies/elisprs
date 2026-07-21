//! `Obj::BoolVector` parity: `make-bool-vector`/`bool-vector`, the `#&N"…"`
//! reader/printer, the array ops, and `bool-vector-count-population`/`-subsetp`/
//! `-not`. A bool-vector is an array and a sequence but NOT a vector.
//!
//! Every expectation was taken from GNU Emacs 30.2 (`emacs -Q --batch -l …`)
//! and matches byte-for-byte, including the packed `#&N"…"` byte string and the
//! `wrong-length-argument` data.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// The `#&LEN"PACKED"` printed form: bits LSB-first, `ceil(LEN/8)` bytes, with
/// print.c's byte escaping (`\OOO` octal for bytes >= 128, raw otherwise).
#[test]
fn bool_vector_print_form() {
    // 8 set bits -> one 0xFF byte, octal-escaped.
    assert_eq!(
        eval("(format \"%S\" (make-bool-vector 8 t))"),
        "\"#&8\\\"\\\\377\\\"\""
    );
    // 10 set bits -> 0xFF then 0x03 (a raw control byte).
    assert_eq!(
        eval("(format \"%S\" (make-bool-vector 10 t))"),
        "\"#&10\\\"\\\\377\u{3}\\\"\""
    );
    // Bits 0 and 6 set -> byte 0b01000001 = 65 = 'A' (a printable byte).
    assert_eq!(
        eval("(format \"%S\" (bool-vector t nil nil nil nil nil t nil))"),
        "\"#&8\\\"A\\\"\""
    );
    // 3 clear bits -> a single NUL byte.
    assert_eq!(
        eval("(format \"%S\" (make-bool-vector 3 nil))"),
        "\"#&3\\\"\u{0}\\\"\""
    );
    // The empty bool-vector.
    assert_eq!(eval("(format \"%S\" (bool-vector))"), "\"#&0\\\"\\\"\"");
}

/// A bool-vector is its own type: `type-of` names it, `bool-vector-p` accepts it,
/// `vectorp` rejects it, and it counts as both an array and a sequence.
#[test]
fn bool_vector_type_predicates() {
    assert_eq!(eval("(type-of (make-bool-vector 3 t))"), "bool-vector");
    assert_eq!(eval("(bool-vector-p (make-bool-vector 3 t))"), "t");
    assert_eq!(eval("(bool-vector-p [1 2])"), "nil");
    assert_eq!(eval("(vectorp (make-bool-vector 3 t))"), "nil");
    assert_eq!(eval("(arrayp (make-bool-vector 3 t))"), "t");
    assert_eq!(eval("(sequencep (make-bool-vector 3 t))"), "t");
}

/// `aref`/`aset`/`length`/`elt` on a bool-vector return / accept `t`/`nil`; a
/// non-nil aset VALUE is stored as `t`.
#[test]
fn bool_vector_array_ops() {
    assert_eq!(eval("(length (make-bool-vector 10 t))"), "10");
    assert_eq!(eval("(aref (bool-vector t nil t) 0)"), "t");
    assert_eq!(eval("(aref (bool-vector t nil t) 1)"), "nil");
    assert_eq!(eval("(elt (bool-vector t nil t) 2)"), "t");
    // aset stores any non-nil as t, and returns the passed value.
    assert_eq!(
        eval("(let ((b (make-bool-vector 3 nil))) (list (aset b 1 5) (aref b 1) (aref b 0)))"),
        "(5 t nil)"
    );
    assert_eq!(
        err("(aref (make-bool-vector 3 t) 9)"),
        "(args-out-of-range #&3\"\u{7}\" 9)"
    );
}

/// A bool-vector is a sequence: `append`/`mapcar` yield its `t`/`nil` elements,
/// and `vconcat` builds a plain vector from them.
#[test]
fn bool_vector_as_sequence() {
    assert_eq!(eval("(append (bool-vector t nil t) nil)"), "(t nil t)");
    assert_eq!(
        eval("(mapcar (lambda (x) x) (bool-vector t nil t))"),
        "(t nil t)"
    );
    assert_eq!(eval("(vconcat (bool-vector t nil t))"), "[t nil t]");
}

/// `copy-sequence` returns a fresh, independent bool-vector.
#[test]
fn bool_vector_copy_sequence_is_independent() {
    assert_eq!(
        eval("(let* ((b (make-bool-vector 3 nil)) (c (copy-sequence b))) (aset c 0 t) (list (bool-vector-p c) (aref b 0) (aref c 0)))"),
        "(t nil t)"
    );
    assert_eq!(
        eval("(let ((b (bool-vector t nil t))) (eq b (copy-sequence b)))"),
        "nil"
    );
}

/// `equal` compares bool-vectors element-wise, and never equates one with a
/// like-valued plain vector.
#[test]
fn bool_vector_equal() {
    assert_eq!(
        eval("(equal (bool-vector t nil t) (bool-vector t nil t))"),
        "t"
    );
    assert_eq!(
        eval("(equal (bool-vector t nil t) (bool-vector t nil nil))"),
        "nil"
    );
    assert_eq!(eval("(equal (bool-vector t nil t) [t nil t])"), "nil");
}

/// The named set operations.
#[test]
fn bool_vector_set_operations() {
    assert_eq!(
        eval("(bool-vector-count-population (bool-vector t nil t t))"),
        "3"
    );
    assert_eq!(
        eval("(bool-vector-subsetp (bool-vector t nil nil) (bool-vector t nil t))"),
        "t"
    );
    assert_eq!(
        eval("(bool-vector-subsetp (bool-vector t nil t) (bool-vector t nil nil))"),
        "nil"
    );
    // bool-vector-not into a fresh vector.
    assert_eq!(
        eval("(append (bool-vector-not (bool-vector t nil t)) nil)"),
        "(nil t nil)"
    );
    // bool-vector-not into a supplied destination returns that destination.
    assert_eq!(
        eval("(let ((d (make-bool-vector 3 nil))) (list (eq (bool-vector-not (bool-vector t nil t) d) d) (append d nil)))"),
        "(t (nil t nil))"
    );
}

/// Error conditions match Emacs, including the (len-A len-B len-B) shape of
/// `bool-vector-subsetp`'s `wrong-length-argument`.
#[test]
fn bool_vector_errors() {
    assert_eq!(
        err("(bool-vector-count-population [t nil])"),
        "(wrong-type-argument bool-vector-p [t nil])"
    );
    assert_eq!(
        err("(bool-vector-subsetp (make-bool-vector 5 nil) (make-bool-vector 2 nil))"),
        "(wrong-length-argument 5 2 2)"
    );
    assert_eq!(
        err("(bool-vector-not (make-bool-vector 4 nil) (make-bool-vector 6 nil))"),
        "(wrong-length-argument 4 6)"
    );
}

/// The `#&N"PACKED"` reader literal round-trips: bits unpack LSB-first, and a
/// printed bool-vector reads back `equal`.
#[test]
fn bool_vector_read_syntax() {
    // #&10"\377\3" -> 8 low bits set, then bits 8 and 9 set.
    assert_eq!(
        eval("(let ((r (read \"#&10\\\"\\377\\3\\\"\"))) (list (length r) (aref r 0) (aref r 7) (aref r 8) (aref r 9)))"),
        "(10 t t t t)"
    );
    // #&8"A" -> byte 65 = bits 0 and 6.
    assert_eq!(
        eval("(equal (read \"#&8\\\"A\\\"\") (bool-vector t nil nil nil nil nil t nil))"),
        "t"
    );
    // A printed bool-vector reads back equal.
    assert_eq!(
        eval("(equal (bool-vector t nil t t nil) (read (format \"%S\" (bool-vector t nil t t nil))))"),
        "t"
    );
}
