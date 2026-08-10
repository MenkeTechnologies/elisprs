//! Two unrelated startup-shape gaps, both measured against `GNU Emacs 30.2`.
//!
//! **`defvar` is not `defconst`.** They differ in exactly one way, and it is not
//! the declaration: both mark the variable special, but `defvar`'s initializer
//! runs *only when the variable is still void*, while `defconst` always assigns.
//! elisprs compiled both to an unconditional `SETVAR`, so a user's `(setq foo 5)`
//! did not survive a later `(defvar foo 9)` — which is the whole reason it is
//! safe to `setq` a library's variable before loading the library. A second
//! `defvar` of the same variable also clobbered the first.
//!
//! **The startup buffer list.** A bare `emacs -Q --batch` starts with three
//! buffers, `("*scratch*" " *Minibuf-0*" "*Messages*")`; elisprs started with
//! one. The two missing ones are observable directly — `(get-buffer "*Messages*")`
//! answered nil — and the leading space on ` *Minibuf-0*` is what marks a buffer
//! hidden, so anything that filters on it had nothing to filter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `defvar` leaves an already-valued variable alone.
///
/// Emacs 30.2: `5`. elisprs answered `9`.
#[test]
fn defvar_does_not_overwrite_an_existing_value() {
    assert_eq!(eval("(progn (setq zz 5) (defvar zz 9) zz)"), "5");
    // The same rule makes a repeated `defvar` idempotent. Emacs 30.2: `1`.
    assert_eq!(eval("(progn (defvar yy 1) (defvar yy 2) yy)"), "1");
}

/// `defconst` is the unconditional one.
///
/// Emacs 30.2: `9`. This is the control for the test above — if both answered
/// the same, the distinction would not be implemented.
#[test]
fn defconst_always_assigns() {
    assert_eq!(eval("(progn (setq zz 5) (defconst zz 9) zz)"), "9");
    assert_eq!(eval("(progn (defconst yy 1) (defconst yy 2) yy)"), "2");
}

/// A `defvar` of a void variable still initializes it, and still declares it
/// special.
#[test]
fn defvar_initializes_a_void_variable_and_declares_it_special() {
    assert_eq!(eval("(progn (defvar fresh1 42) fresh1)"), "42");
    assert_eq!(
        eval("(progn (defvar fresh2 1) (special-variable-p 'fresh2))"),
        "t"
    );
    // A bodyless `defvar` declares without assigning, so the variable stays void.
    assert_eq!(eval("(progn (defvar fresh3) (boundp 'fresh3))"), "nil");
    // The form's own value is the symbol in both spellings.
    assert_eq!(eval("(defvar fresh4 1)"), "fresh4");
    assert_eq!(eval("(defconst fresh5 1)"), "fresh5");
}

/// The three buffers `emacs -Q --batch` starts with, in `buffer-list` order.
#[test]
fn startup_buffer_list_matches_emacs() {
    assert_eq!(
        eval("(mapcar #'buffer-name (buffer-list))"),
        "(\"*scratch*\" \" *Minibuf-0*\" \"*Messages*\")"
    );
    assert_eq!(eval("(buffer-name)"), "\"*scratch*\"");
}

/// The two added buffers are real, live, and reachable by name.
///
/// A cosmetic entry in `buffer-list` would pass the test above and fail these.
#[test]
fn the_startup_buffers_are_live_and_addressable() {
    assert_eq!(eval("(bufferp (get-buffer \"*Messages*\"))"), "t");
    assert_eq!(eval("(buffer-live-p (get-buffer \"*Messages*\"))"), "t");
    assert_eq!(
        eval("(buffer-name (get-buffer \" *Minibuf-0*\"))"),
        "\" *Minibuf-0*\""
    );
    // Writable like any other buffer.
    assert_eq!(
        eval(
            "(with-current-buffer (get-buffer \"*Messages*\") \
               (insert \"hi\") (buffer-string))"
        ),
        "\"hi\""
    );
    // And they do not disturb the buffer the program starts in.
    assert_eq!(
        eval("(progn (get-buffer-create \"aaa\") (buffer-name))"),
        "\"*scratch*\""
    );
}

/// A new buffer appends after the startup three, and killing it restores them.
#[test]
fn a_new_buffer_appends_after_the_startup_three() {
    assert_eq!(
        eval(
            "(progn (generate-new-buffer \"zzz\") \
               (mapcar #'buffer-name (buffer-list)))"
        ),
        "(\"*scratch*\" \" *Minibuf-0*\" \"*Messages*\" \"zzz\")"
    );
    assert_eq!(
        eval(
            "(let ((b (generate-new-buffer \"zzz\"))) (kill-buffer b) \
               (mapcar #'buffer-name (buffer-list)))"
        ),
        "(\"*scratch*\" \" *Minibuf-0*\" \"*Messages*\")"
    );
}
