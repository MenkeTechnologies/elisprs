//! ring.el parity — faithful port of emacs-lisp/ring.el (Emacs 30.2).
//!
//! A ring is the cons (HD LN . VEC): HD indexes the oldest element, LN is the
//! count, VEC is the storage. `ring-ref` indexes newest-first (0 = newest).
//! Every expectation matches GNU Emacs 30.2 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The public API is bound after prelude load.
#[test]
fn ring_api_is_bound() {
    assert_eq!(
        eval("(mapcar #'fboundp '(make-ring ring-insert ring-ref ring-length ring-empty-p ring-elements ring-remove ring-insert-at-beginning ring-p ring-size ring-member ring-next ring-previous ring-copy ring-extend))"),
        "(t t t t t t t t t t t t t t t)"
    );
}

/// Fresh ring: empty, correct size, ring-p.
#[test]
fn ring_fresh_state() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (list (ring-p r) (ring-empty-p r) (ring-length r) (ring-size r)))"),
        "(t t 0 3)"
    );
}

/// Insert orders newest-first; ring-ref 0 is newest.
#[test]
fn ring_insert_and_ref() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (list (ring-ref r 0) (ring-ref r 1) (ring-ref r 2) (ring-length r)))"),
        "(c b a 3)"
    );
    // ring-ref indexes modulo the ring length.
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (ring-ref r 3))"),
        "c"
    );
}

/// ring-elements returns the contents newest-first.
#[test]
fn ring_elements_order() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (ring-elements r))"),
        "(c b a)"
    );
}

/// Overflow drops the oldest element to make room.
#[test]
fn ring_overflow_drops_oldest() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (dolist (x '(a b c d)) (ring-insert r x)) (list (ring-elements r) (ring-length r)))"),
        "((d c b) 3)"
    );
}

/// ring-insert-at-beginning adds as the oldest item.
#[test]
fn ring_insert_at_beginning() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert-at-beginning r 'z) (ring-elements r))"),
        "(b a z)"
    );
}

/// ring-remove with no index removes the oldest, returns it, shrinks length.
#[test]
fn ring_remove_oldest() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (let ((got (ring-remove r))) (list got (ring-elements r) (ring-length r))))"),
        "(a (c b) 2)"
    );
}

/// ring-remove with an explicit (newest-first) index removes that element.
#[test]
fn ring_remove_indexed() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (let ((got (ring-remove r 0))) (list got (ring-elements r))))"),
        "(c (b a))"
    );
}

/// ring-remove on an empty ring signals "Ring empty".
#[test]
fn ring_remove_empty_errors() {
    reset_host();
    let e = eval_str("(ring-remove (make-ring 2))").unwrap_err();
    assert!(e.contains("Ring empty"), "unexpected error: {e}");
}

/// ring-member returns the newest-first index (via `equal`), nil when absent.
#[test]
fn ring_member() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (list (ring-member r 'c) (ring-member r 'a) (ring-member r 'x)))"),
        "(0 2 nil)"
    );
}

/// ring-next moves toward older, ring-previous toward newer (with wraparound).
#[test]
fn ring_next_previous() {
    assert_eq!(
        eval("(let ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (list (ring-next r 'c) (ring-previous r 'c)))"),
        "(b a)"
    );
}

/// ring-copy produces an independent ring (mutating the copy leaves the
/// original intact).
#[test]
fn ring_copy_independent() {
    assert_eq!(
        eval("(let* ((r (make-ring 3))) (ring-insert r 'a) (ring-insert r 'b) (let ((c (ring-copy r))) (ring-insert c 'q) (list (ring-elements r) (ring-elements c))))"),
        "((b a) (q b a))"
    );
}

/// ring-extend grows capacity without dropping elements.
#[test]
fn ring_extend_grows() {
    assert_eq!(
        eval("(let ((r (make-ring 2))) (ring-insert r 'a) (ring-insert r 'b) (ring-extend r 2) (ring-insert r 'c) (ring-insert r 'd) (list (ring-size r) (ring-elements r)))"),
        "(4 (d c b a))"
    );
}

/// ring-convert-sequence-to-ring builds a ring from a list.
#[test]
fn ring_convert_sequence() {
    assert_eq!(
        eval("(ring-elements (ring-convert-sequence-to-ring '(a b c)))"),
        "(a b c)"
    );
}
