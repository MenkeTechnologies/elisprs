//! A string is a MUTABLE object, and mutating it is visible through every
//! reference to it.
//!
//! An elisp string used to be a `Value::Str(Arc<String>)` — an immutable value.
//! `aset` on one signalled `(wrong-type-argument arrayp "ab")` where Emacs
//! writes the character, `store-substring` and `clear-string` were void, and
//! `fillarray` refused anything but a vector. Making it writable is not a
//! one-line change, because the obvious way to write through a shared
//! `Arc<String>` — `Arc::make_mut` — copies whenever the buffer is shared, so
//! the write would land on a private copy and be invisible to every alias. That
//! also destroys the pointer identity the previous round established for `eq`.
//!
//! The fix is that a string is an arena OBJECT (`Obj::Str`) like a cons: the
//! handle IS the identity, and the text behind it can be replaced in place. The
//! four properties that have to hold together:
//!
//! ```text
//!                                                        emacs   elisprs (before)
//! (let ((s (copy-sequence "ab"))) (aset s 0 ?z) s)        "zb"    (wrong-type-argument arrayp "ab")
//! (let ((s "abc")) (eq s s))                              t       t
//! (let* ((a (copy-sequence "ab")) (b a)) (aset a 0 ?z) b) "zb"    (wrong-type-argument arrayp "ab")
//! (progn (defun f () "ab") (aset (f) 0 ?z) (f))           "zb"    (wrong-type-argument arrayp "ab")
//! ```
//!
//! Every expectation below is `emacs -Q --batch` on the installed GNU Emacs
//! 31.1. (The 30.2 binary earlier rounds quoted is no longer installed on this
//! machine; where a form is also recorded in an earlier round's BUGS.md entry
//! the two agree — see the closure-capture file for a direct 30.2/31.1
//! cross-check on a form both rounds ran.)

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The four properties that had to land together: the write happens, the write
/// is visible through an alias, `eq` still answers identity, and a literal is
/// itself a writable object (Emacs 30 removed pure space, so the string a
/// function returns is one cell, not a fresh copy per call).
#[test]
fn aset_writes_through_every_reference_without_losing_eq() {
    assert_eq!(eval("(let ((s (copy-sequence \"ab\"))) (aset s 0 ?z) s)"), "\"zb\"");
    assert_eq!(eval("(let ((s \"abc\")) (eq s s))"), "t");
    assert_eq!(
        eval("(let* ((a (copy-sequence \"ab\")) (b a)) (aset a 0 ?z) b)"),
        "\"zb\""
    );
    assert_eq!(
        eval("(progn (defun f1 () \"ab\") (aset (f1) 0 ?z) (f1))"),
        "\"zb\""
    );
    // Two equal literals are still separate objects, and the shared empty
    // string is still shared — mutability did not merge or split anything.
    assert_eq!(eval("(let* ((a \"lit\") (b \"lit\")) (eq a b))"), "nil");
    assert_eq!(eval("(eq \"\" (make-string 0 ?a))"), "t");
}

/// A string stored in a structure is the same object, so a later `aset` shows
/// through the structure — this is what `Arc::make_mut` could not have given.
#[test]
fn a_stored_string_sees_a_later_write() {
    assert_eq!(
        eval("(let* ((s (copy-sequence \"abc\")) (l (list s))) (aset s 0 ?z) (car l))"),
        "\"zbc\""
    );
    assert_eq!(
        eval("(let* ((s (copy-sequence \"abc\")) (v (vector s))) (aset s 0 ?z) (aref v 0))"),
        "\"zbc\""
    );
    // A hash table keyed by the string does NOT rehash: Emacs hashes at
    // `puthash` time, so the entry is stranded under the old text.
    assert_eq!(
        eval(
            "(let ((h (make-hash-table :test 'equal)) (s (copy-sequence \"abc\"))) \
             (puthash s 1 h) (aset s 0 ?z) (gethash \"zbc\" h))"
        ),
        "nil"
    );
}

/// `Faset` (data.c) checks the INDEX before the character, and returns the
/// character it stored rather than the string.
#[test]
fn aset_argument_checks_are_in_emacs_order() {
    assert_eq!(eval("(let ((s (copy-sequence \"ab\"))) (aset s 0 ?z))"), "122");
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"ab\"))) (aset s 5 'x)) (error e))"),
        "(args-out-of-range \"ab\" 5)"
    );
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"ab\"))) (aset s 0 'x)) (error e))"),
        "(wrong-type-argument characterp x)"
    );
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"ab\"))) (aset s -1 ?z)) (error e))"),
        "(args-out-of-range \"ab\" -1)"
    );
    // The length never changes, and the object is still a string afterwards.
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (list (length s) (type-of s)))"),
        "(3 string)"
    );
}

/// Text properties travel with the write: `string_props` is keyed by the text
/// allocation, and a write installs a fresh one, so the entry has to be
/// re-keyed or `(aset (propertize ...) ...)` would silently drop the interval.
#[test]
fn a_write_keeps_the_strings_text_properties() {
    assert_eq!(
        eval("(let ((s (propertize \"abc\" 'p 1))) (aset s 0 ?z) s)"),
        "#(\"zbc\" 0 3 (p 1))"
    );
}

/// `Fstore_substring` (editfns.c) writes character by character and only then
/// runs off the end, so a too-long OBJ leaves the prefix written AND names the
/// partially-written string in the error.
#[test]
fn store_substring_writes_what_fits_before_signalling() {
    assert_eq!(
        eval("(let ((s (copy-sequence \"abcdef\"))) (store-substring s 1 \"XY\") s)"),
        "\"aXYdef\""
    );
    // The return value is the whole string, not the written part.
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (store-substring s 1 \"XY\"))"),
        "\"aXY\""
    );
    // A character OBJ writes one character.
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (store-substring s 1 ?Z) s)"),
        "\"aZc\""
    );
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"abc\"))) (store-substring s 1 \"XYZW\")) (error e))"),
        "(args-out-of-range \"aXY\" 3)"
    );
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"abc\"))) (store-substring s 3 \"X\")) (error e))"),
        "(args-out-of-range \"abc\" 3)"
    );
    assert_eq!(
        eval("(condition-case e (let ((s (copy-sequence \"abc\"))) (store-substring s -1 \"X\")) (error e))"),
        "(args-out-of-range \"abc\" -1)"
    );
}

/// `Fclear_string` (fns.c) overwrites with NUL — not with spaces — keeps the
/// length, and answers nil. `Ffillarray` accepts a string as well as a vector.
#[test]
fn clear_string_nulls_in_place_and_fillarray_takes_a_string() {
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (clear-string s) (append s nil))"),
        "(0 0 0)"
    );
    assert_eq!(eval("(let ((s (copy-sequence \"abc\"))) (clear-string s))"), "nil");
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (clear-string s) (length s))"),
        "3"
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"ab\"))) (fillarray s ?z) s)"),
        "\"zz\""
    );
    assert_eq!(eval("(fillarray (make-vector 2 0) ?z)"), "[122 122]");
}

/// Everything downstream reads the CURRENT text, not the text the string was
/// allocated with — the failure mode of a half-converted read site.
#[test]
fn every_reader_sees_the_written_text() {
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (equal s \"zbc\"))"),
        "t"
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (concat s \"!\"))"),
        "\"zbc!\""
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (substring s 0 2))"),
        "\"zb\""
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (symbol-name (intern s)))"),
        "\"zbc\""
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 0 ?z) (string-match \"z\" s))"),
        "0"
    );
    assert_eq!(
        eval("(let ((s (copy-sequence \"abc\"))) (aset s 1 ?Z) (aset s 2 ?W) s)"),
        "\"aZW\""
    );
    // A non-ASCII character in the string does not confuse the character index.
    assert_eq!(
        eval("(let ((s (copy-sequence \"ab\u{e9}\"))) (aset s 0 ?z) s)"),
        "\"zb\u{e9}\""
    );
}

/// The functions that build a copy still build a copy: `upcase` on a string
/// leaves the original alone, and `aset` on the result does not write back.
#[test]
fn the_copying_string_functions_still_copy() {
    assert_eq!(
        eval("(let ((s (copy-sequence \"hello\"))) (upcase s) s)"),
        "\"hello\""
    );
    assert_eq!(
        eval("(let* ((s (copy-sequence \"ab\")) (u (upcase s))) (aset u 0 ?z) s)"),
        "\"ab\""
    );
    assert_eq!(
        eval("(let* ((s (copy-sequence \"ab\")) (c (concat s \"\"))) (aset c 0 ?z) s)"),
        "\"ab\""
    );
    assert_eq!(
        eval("(let* ((s (copy-sequence \"abc\")) (c (copy-sequence s))) (aset c 0 ?z) s)"),
        "\"abc\""
    );
}
