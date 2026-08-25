//! A hash table whose test came from `define-hash-table-test` calls elisp.
//!
//! `define-hash-table-test` was void and `make-hash-table :test MY=` silently
//! fell back to `eql`, so a table declared with a case-insensitive test found
//! nothing. The missing builtin was never the obstacle: TESTFN and HASHFN are
//! elisp, so a lookup on such a table has to CALL elisp — and `hash_eq` /
//! `hash_key` ran inside the `&mut ElispHost` a subr body holds, where a nested
//! call cannot happen.
//!
//! `gethash`/`puthash`/`remhash`/`make-hash-table` therefore joined
//! `mapcar`/`sort`/`maphash`/`mapatoms` on the intercepted path: registered
//! with `defsubr`, so `subrp` and `#'NAME` are unchanged, but dispatched from
//! `host::call_function` OUTSIDE the borrow. A built-in test still runs the
//! whole operation in one borrow; only a user test re-enters.
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The declaration for a case-insensitive string table, reused below.
const CI: &str = "(define-hash-table-test 'ci \
                  (lambda (a b) (string= (downcase a) (downcase b))) \
                  (lambda (a) (sxhash-equal (downcase a))))";

/// A lookup finds a key the elisp TESTFN calls equal, however it was spelled.
#[test]
fn a_user_test_decides_what_a_lookup_finds() {
    assert_eq!(
        eval(&format!(
            "(progn {CI} (let ((h (make-hash-table :test 'ci))) \
             (puthash \"Foo\" 1 h) \
             (list (gethash \"foo\" h) (gethash \"FOO\" h) (gethash \"bar\" h))))"
        )),
        "(1 1 nil)"
    );
    assert_eq!(
        eval(&format!(
            "(progn {CI} (let ((h (make-hash-table :test 'ci))) \
             (puthash \"Foo\" 1 h) (remhash \"FOO\" h) (hash-table-count h)))"
        )),
        "0"
    );
}

/// A `puthash` onto a key the test already matches keeps the ORIGINAL key
/// object and replaces only the value — so the count stays 1 and `maphash`
/// still reports the key as first stored.
#[test]
fn a_matching_puthash_replaces_the_value_and_keeps_the_key() {
    assert_eq!(
        eval(&format!(
            "(progn {CI} (let ((h (make-hash-table :test 'ci))) \
             (puthash \"Foo\" 1 h) (puthash \"FOO\" 2 h) \
             (list (hash-table-count h) (gethash \"foo\" h))))"
        )),
        "(1 2)"
    );
    assert_eq!(
        eval(&format!(
            "(progn {CI} (let ((h (make-hash-table :test 'ci))) \
             (puthash \"Foo\" 1 h) (puthash \"FOO\" 2 h) \
             (let (acc) (maphash (lambda (k v) (setq acc (cons (cons k v) acc))) h) acc)))"
        )),
        "((\"Foo\" . 2))"
    );
}

/// The declaration goes on the symbol's `hash-table-test` property, and the
/// table reports the NAME it was made with rather than a built-in test symbol.
#[test]
fn the_declaration_and_the_reported_test_name() {
    assert_eq!(
        eval(&format!(
            "(progn {CI} (let ((h (make-hash-table :test 'ci))) (hash-table-test h)))"
        )),
        "ci"
    );
    // The declaration goes on the symbol's `hash-table-test` property as the
    // two-element list `(TESTFN HASHFN)`.
    assert_eq!(
        eval("(progn (define-hash-table-test 'my= (lambda (a b) (= a b)) (lambda (a) (floor a))) \
              (length (get 'my= 'hash-table-test)))"),
        "2"
    );
    assert_eq!(
        eval("(progn (define-hash-table-test 'my2 (lambda (a b) (= a b)) (lambda (a) (floor a))) \
              (mapcar #'functionp (get 'my2 'hash-table-test)))"),
        "(t t)"
    );
    // The built-in tests are untouched.
    assert_eq!(eval("(hash-table-test (make-hash-table :test 'equal))"), "equal");
    assert_eq!(eval("(hash-table-test (make-hash-table :test 'eq))"), "eq");
    assert_eq!(eval("(hash-table-test (make-hash-table))"), "eql");
}

/// A `:test` naming something never declared is an error, not a silent `eql`.
#[test]
fn an_undeclared_test_signals() {
    assert_eq!(
        eval("(condition-case e (make-hash-table :test 'nosuchtest) (error e))"),
        "(error \"Invalid hash table test\" nosuchtest)"
    );
}

/// Interception must not have changed what these names ARE: Emacs's `gethash`
/// and friends are C subrs, and code that goes through `symbol-function` or
/// `funcall` has to keep working.
#[test]
fn the_intercepted_names_are_still_subrs() {
    assert_eq!(eval("(subrp (symbol-function 'gethash))"), "t");
    assert_eq!(eval("(subrp (symbol-function 'puthash))"), "t");
    assert_eq!(eval("(subrp (symbol-function 'remhash))"), "t");
    assert_eq!(eval("(subrp (symbol-function 'make-hash-table))"), "t");
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) \
              (funcall (symbol-function 'puthash) \"k\" 7 h) \
              (funcall #'gethash \"k\" h))"),
        "7"
    );
    // A built-in test still answers exactly as before.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) \
              (puthash (list 1 2) 'v h) (gethash (list 1 2) h))"),
        "v"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1.5 'f h) (gethash 1.5 h))"),
        "f"
    );
}
