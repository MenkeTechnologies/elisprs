//! A hash table's SLOT ORDER is observable, and it is not "insertion order".
//!
//! Emacs stores entries in a slot vector plus a free list (`struct
//! Lisp_Hash_Table`, fns.c): `remhash` frees a slot and the next `puthash`
//! reuses the most recently freed one. `maphash`, the `#s(hash-table …)`
//! printer and `hash-table-keys` all walk slots in index order, so the reuse is
//! directly visible. elisprs stored a packed association vector, which appended
//! instead:
//!
//! ```text
//! (let ((h (make-hash-table)))
//!   (dotimes (i 5) (puthash i i h))
//!   (remhash 2 h) (puthash 9 9 h)
//!   (let (acc) (maphash (lambda (k _v) (push k acc)) h) (nreverse acc)))
//!
//!   emacs 30.2       (0 1 9 3 4)
//!   elisprs (before) (0 1 3 4 9)
//! ```
//!
//! The same round replaced the linear scan with a hash index, which is where the
//! stale-key expectations below come from: the key's hash is taken once, when it
//! goes in, so mutating a key afterwards loses it — in Emacs too.
//!
//! Every expectation is `emacs -Q --batch --eval '(prin1 …)'` on GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The keys a `maphash` walk yields, in slot order.
const WALK: &str = "(let (acc) (maphash (lambda (k _v) (push k acc)) h) (nreverse acc))";

/// A freed slot is reused by the next new key, LIFO, and the walk shows it.
#[test]
fn remhash_frees_a_slot_that_the_next_puthash_reuses() {
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (dotimes (i 5) (puthash i i h)) \
             (remhash 2 h) (puthash 9 9 h) {WALK})"
        )),
        "(0 1 9 3 4)"
    );
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) \
             (remhash 'a h) (puthash 'c 3 h) {WALK})"
        )),
        "(c b)"
    );
    // Two frees, two fills: the free list is LIFO, so the SECOND free is used
    // first (slot 3 before slot 1).
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (dotimes (i 5) (puthash i i h)) \
             (remhash 1 h) (remhash 3 h) (puthash 8 8 h) (puthash 9 9 h) {WALK})"
        )),
        "(0 9 2 8 4)"
    );
    // A hole with nothing to fill it is simply skipped.
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (dotimes (i 4) (puthash i i h)) \
             (remhash 0 h) (remhash 2 h) (puthash 7 7 h) {WALK})"
        )),
        "(1 7 3)"
    );
}

/// `clrhash` drops the slot vector outright, so the next key starts at slot 0
/// rather than landing on a stale free list.
#[test]
fn clrhash_resets_the_slot_vector() {
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (dotimes (i 3) (puthash i i h)) \
             (clrhash h) (puthash 5 5 h) (puthash 6 6 h) {WALK})"
        )),
        "(5 6)"
    );
}

/// subr-x's `hash-table-keys`/`hash-table-values` `push` onto a list inside a
/// `maphash` and never reverse it, so both come back in REVERSE slot order.
#[test]
fn hash_table_keys_and_values_come_back_reversed() {
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) (hash-table-keys h))"),
        "(b a)"
    );
    assert_eq!(
        eval(
            "(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) (hash-table-values h))"
        ),
        "(2 1)"
    );
    assert_eq!(
        eval(
            "(let ((h (make-hash-table))) (dotimes (i 3) (puthash i i h)) (remhash 0 h) \
             (hash-table-keys h))"
        ),
        "(2 1)"
    );
}

/// The key's hash is taken ONCE, when it goes in. Mutating the key afterwards
/// leaves the entry in the table but unreachable — Emacs behaves the same way,
/// which is why its manual says not to mutate a key in place.
#[test]
fn a_key_mutated_after_insertion_is_lost() {
    assert_eq!(
        eval("(let* ((k (list 1)) (h (make-hash-table :test 'equal))) (puthash k 'v h) (setcar k 2) (gethash k h))"),
        "nil"
    );
    // The entry is still there — only the lookup path is broken.
    assert_eq!(
        eval("(let* ((k (list 1)) (h (make-hash-table :test 'equal))) (puthash k 'v h) (setcar k 2) (hash-table-count h))"),
        "1"
    );
}

/// `hash-table-size` is the ALLOCATION size, not the entry count: it is the
/// `:size` argument until the table outgrows it, and then Emacs's growth series.
#[test]
fn hash_table_size_reports_the_allocation() {
    assert_eq!(eval("(hash-table-size (make-hash-table))"), "0");
    assert_eq!(eval("(hash-table-size (make-hash-table :size 10))"), "10");
    // 0 → 6 → 24 → 96, and a `:size N` table jumps to max(4N, 24).
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1 1 h) (hash-table-size h))"),
        "6"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (dotimes (i 7) (puthash i i h)) (hash-table-size h))"),
        "24"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (dotimes (i 25) (puthash i i h)) (hash-table-size h))"),
        "96"
    );
    assert_eq!(
        eval(
            "(let ((h (make-hash-table :size 3))) (dotimes (i 4) (puthash i i h)) (hash-table-size h))"
        ),
        "24"
    );
    assert_eq!(
        eval(
            "(let ((h (make-hash-table :size 10))) (dotimes (i 11) (puthash i i h)) (hash-table-size h))"
        ),
        "40"
    );
}

/// `:weakness` is reported back verbatim. elisprs has no GC that can drop
/// entries, so the symbol is all that is observable.
#[test]
fn weakness_round_trips() {
    assert_eq!(
        eval("(hash-table-weakness (make-hash-table :weakness 'key))"),
        "key"
    );
    assert_eq!(eval("(hash-table-weakness (make-hash-table))"), "nil");
}

/// Emacs 30 dropped the per-table rehash parameters; both accessors survive and
/// answer one constant for every table.
#[test]
fn rehash_parameters_are_constants() {
    assert_eq!(eval("(hash-table-rehash-size (make-hash-table))"), "1.5");
    assert_eq!(
        eval("(hash-table-rehash-threshold (make-hash-table))"),
        "0.8125"
    );
    assert_eq!(
        eval("(hash-table-rehash-size (make-hash-table :size 99))"),
        "1.5"
    );
}

/// The index has to agree with the table's TEST, key kind by key kind. These are
/// the kinds whose hash is not just the object handle.
#[test]
fn every_test_finds_its_own_key_kinds() {
    // `equal`: strings, lists, vectors, records, bool-vectors, bignums.
    for (key, lookup) in [
        ("\"a\"", "\"a\""),
        ("'(1 2)", "'(1 2)"),
        ("[1 2]", "[1 2]"),
        ("(record 'r 1)", "(record 'r 1)"),
        ("(bool-vector t nil)", "(bool-vector t nil)"),
        ("(expt 2 70)", "(expt 2 70)"),
        ("(propertize \"a\" 'p 1)", "\"a\""),
        ("nil", "nil"),
        ("t", "t"),
    ] {
        assert_eq!(
            eval(&format!(
                "(let ((h (make-hash-table :test 'equal))) (puthash {key} 'v h) (gethash {lookup} h))"
            )),
            "v",
            "equal-test key {key}"
        );
    }
    // `eql`: floats by bit pattern (NaN included) and bignums by value.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eql))) (puthash 0.0 'z h) (gethash -0.0 h))"),
        "nil"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eql))) (puthash (/ 0.0 0.0) 'n h) (gethash (/ 0.0 0.0) h))"),
        "n"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eql))) (puthash (expt 2 70) 'b h) (gethash (expt 2 70) h))"),
        "b"
    );
    // `eq`: two separately built bignums are two OBJECTS, so the same
    // arithmetic does not find the entry — the value-based hash puts them in
    // one bucket and `eq` then rejects the match, which is the answer Emacs
    // gives. Two equal strings behave the same way.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eq))) (puthash (expt 2 70) 'b h) (gethash (expt 2 70) h))"),
        "nil"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eq))) (puthash \"a\" 1 h) (gethash \"a\" h))"),
        "nil"
    );
    assert_eq!(
        eval("(let* ((s (copy-sequence \"abc\")) (h (make-hash-table :test 'eq))) (puthash s 1 h) (gethash s h))"),
        "1"
    );
}

/// A circular key must not hang the hasher. Emacs signals from `equal` here;
/// what matters is that the table takes the key, keeps it findable by identity,
/// and terminates.
#[test]
fn a_circular_key_terminates() {
    assert_eq!(
        eval(
            "(let ((k (list 1 2)) (h (make-hash-table :test 'eq))) (setcdr (cdr k) k) \
             (puthash k 'v h) (gethash k h))"
        ),
        "v"
    );
}

/// `copy-hash-table` reproduces the slot order, the test and the allocation
/// size, and is independent of the original.
#[test]
fn copy_hash_table_reproduces_the_walk() {
    assert_eq!(
        eval(&format!(
            "(let ((h (make-hash-table))) (dotimes (i 4) (puthash i i h)) (remhash 1 h) \
             (setq h (copy-hash-table h)) {WALK})"
        )),
        "(0 2 3)"
    );
    assert_eq!(
        eval(
            "(let* ((h (make-hash-table :test 'equal)) (c (progn (puthash \"k\" 1 h) (copy-hash-table h)))) \
             (puthash \"k\" 2 c) (list (gethash \"k\" h) (gethash \"k\" c) (hash-table-test c)))"
        ),
        "(1 2 equal)"
    );
}
