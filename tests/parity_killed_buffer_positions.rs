//! A killed buffer's point, and the three ways Emacs refuses a buffer argument.
//!
//! `kill-buffer` cleared the slot's text but left `point`, `begv` and `zv` where
//! they were, breaking `EditBuffer::point`'s documented invariant ("always kept
//! within `[begv, zv]`"). When the killed buffer was also the last live one,
//! `kill_buffer`'s `.unwrap_or(0)` then left that dead slot *current*, so:
//!
//! ```elisp
//! (insert "hello") (kill-buffer) (buffer-substring (point-min) (point))
//! ```
//!
//! sliced `text[..5]` of a zero-length vector and aborted the process with a
//! Rust panic. Emacs cannot reach that state: `Fkill_buffer` re-selects with
//! `Fset_buffer (Fother_buffer (…))`, and `other-buffer` ends in
//! `get-scratch-buffer-create` — "If no other buffer exists, return the buffer
//! `*scratch*' (creating it if necessary)".
//!
//! The same change ports `Fget_buffer` / `Fset_buffer` / `nsberror`
//! (`src/buffer.c`), which had collapsed into one message here. Emacs 30.2:
//!
//! ```c
//!   buffer = Fget_buffer (buffer_or_name);          /* CHECK_STRING inside */
//!   if (NILP (buffer)) nsberror (buffer_or_name);   /* "No buffer named %s" */
//!   if (!BUFFER_LIVE_P (…)) error ("Selecting deleted buffer");
//! ```

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Killing the current buffer must leave a *live* buffer current with point at
/// its own beginning. This is the panic repro; a regression aborts the process
/// rather than failing the assertion, which is itself the signal.
///
/// Emacs 30.2 answers `(1 1 1 "")` for the position part (its successor buffer
/// is `*Messages*`, which elisprs does not model, so the name is not asserted).
#[test]
fn killing_the_last_buffer_leaves_point_inside_the_successor() {
    assert_eq!(
        eval(
            "(progn (insert \"hello\") (kill-buffer) \
             (list (point) (point-min) (point-max) (buffer-substring (point-min) (point))))"
        ),
        "(1 1 1 \"\")"
    );
}

/// A buffer object stays resolvable after it is killed, and re-selecting it
/// still cannot resurrect stale positions.
#[test]
fn killing_a_named_buffer_resets_its_positions() {
    assert_eq!(
        eval(
            "(let ((b (generate-new-buffer \"kb\"))) \
               (set-buffer b) (insert \"hello\") (kill-buffer b) \
               (list (buffer-live-p b) (point) (point-max)))"
        ),
        "(nil 1 1)"
    );
}

/// `Fget_buffer` returns a buffer object unchanged, live or not; only the
/// by-name branch can answer nil. Emacs 30.2: `#<killed buffer>`.
#[test]
fn get_buffer_returns_a_killed_buffer_object() {
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"kb\"))) (kill-buffer b) (get-buffer b))"),
        "#<killed buffer>"
    );
}

/// `Fget_buffer`'s `CHECK_STRING` runs before the name lookup, so a non-buffer
/// non-string is a type error and never "no such buffer".
/// Emacs 30.2: `(wrong-type-argument stringp 5)` for both.
#[test]
fn a_non_string_buffer_designator_is_a_type_error() {
    assert_eq!(
        eval("(condition-case e (get-buffer 5) (error e))"),
        "(wrong-type-argument stringp 5)"
    );
    assert_eq!(
        eval("(condition-case e (set-buffer 5) (error e))"),
        "(wrong-type-argument stringp 5)"
    );
}

/// `nsberror` prints the name with `SDATA`, not `prin1` — no quotes.
/// Emacs 30.2: `(error "No buffer named nope")`.
#[test]
fn an_unknown_buffer_name_is_reported_unquoted() {
    assert_eq!(
        eval("(condition-case e (set-buffer \"nope\") (error e))"),
        "(error \"No buffer named nope\")"
    );
}

/// Selecting a killed buffer *object* is its own message, distinct from both of
/// the above. Emacs 30.2: `(error "Selecting deleted buffer")`.
#[test]
fn selecting_a_killed_buffer_has_its_own_message() {
    assert_eq!(
        eval(
            "(let ((b (generate-new-buffer \"kb\"))) (kill-buffer b) \
               (condition-case e (set-buffer b) (error e)))"
        ),
        "(error \"Selecting deleted buffer\")"
    );
}
