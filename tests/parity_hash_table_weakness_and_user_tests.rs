//! A hash table's weakness and `define-hash-table-test` name through
//! `make-hash-table`, the printer, the `#s(hash-table …)` reader and
//! `copy-hash-table`.
//!
//! Every expectation is GNU Emacs 31.1 (`emacs -Q --batch`) output.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn weakness_is_normalized_and_printed() {
    check(
        "(list (make-hash-table :weakness t) (hash-table-weakness (make-hash-table :weakness t)))",
        "(#s(hash-table weakness key-and-value) key-and-value)",
    );
    check(
        "(make-hash-table :weakness 'value :test 'equal)",
        "#s(hash-table test equal weakness value)",
    );
    check(
        "(let ((h (make-hash-table :weakness 'key-or-value))) (puthash 1 2 h) h)",
        "#s(hash-table weakness key-or-value data (1 2))",
    );
    check(
        "(copy-hash-table (make-hash-table :weakness 'value))",
        "#s(hash-table weakness value)",
    );
}

#[test]
fn reader_takes_weakness() {
    check(
        "(hash-table-weakness (read \"#s(hash-table weakness key)\"))",
        "key",
    );
    check(
        "(read \"#s(hash-table weakness t)\")",
        "#s(hash-table weakness key-and-value)",
    );
    check(
        "(read \"#s(hash-table weakness value test eq data (1 2))\")",
        "#s(hash-table test eq weakness value data (1 2))",
    );
    check(
        "(condition-case e (read \"#s(hash-table weakness bogus)\") (error (list 'err e)))",
        "(err (error \"Invalid hash table weakness\" bogus))",
    );
}

#[test]
fn user_test_name_prints_and_survives_copy() {
    let def = "(define-hash-table-test 'my-t #'equal #'sxhash-equal)";
    check(
        &format!("(progn {def} (make-hash-table :test 'my-t))"),
        "#s(hash-table test my-t)",
    );
    check(
        &format!("(progn {def} (let ((h (make-hash-table :test 'my-t))) (puthash \"a\" 1 h) h))"),
        "#s(hash-table test my-t data (\"a\" 1))",
    );
    check(
        &format!(
            "(progn {def} (let ((u (make-hash-table :test 'my-t))) (puthash \"a\" 1 u) \
             (list (hash-table-test (copy-hash-table u)) (gethash (copy-sequence \"a\") (copy-hash-table u)))))"
        ),
        "(my-t 1)",
    );
}

/// The copy keeps the original's free list, so the next insertion lands in
/// the hole a `remhash` left, in both tables independently.
#[test]
fn copy_keeps_the_free_list() {
    check(
        "(let ((h (make-hash-table))) (puthash 1 1 h) (puthash 2 2 h) (puthash 3 3 h) (remhash 1 h) \
         (let ((c (copy-hash-table h))) (puthash 4 4 c) (puthash 5 5 h) (list c h)))",
        "(#s(hash-table data (4 4 2 2 3 3)) #s(hash-table data (5 5 2 2 3 3)))",
    );
}
