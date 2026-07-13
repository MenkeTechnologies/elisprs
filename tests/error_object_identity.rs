//! An error's *object* must belong to the error actually being signalled.
//!
//! elisprs carries an error on two channels: the message rides back as the `Err`
//! string of a `Result`, while the structured object `(SYMBOL . DATA)` — the value
//! `condition-case` binds — is parked on the host. Nothing paired the two, so an
//! object left behind by an error that had ALREADY been handled was picked up by
//! the next error that only produced a message:
//!
//! ```elisp
//! (condition-case e (progn (ignore-errors (error "boom")) (car 1)) (error e))
//! ;; => (error "boom")            ; the caught inner error, not the real one
//! ;; Emacs: (wrong-type-argument listp 1)
//! ```
//!
//! The object is now recorded together with the message it belongs to, and is only
//! used for that message. Found by `scripts/fuzz_parity.sh`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// A caught error must not lend its identity to the next one.
#[test]
fn a_handled_error_does_not_poison_the_next_one() {
    assert_eq!(
        eval("(condition-case e (progn (ignore-errors (error \"boom\")) (car 1)) (error e))"),
        "(wrong-type-argument listp 1)"
    );
    assert_eq!(
        eval(
            "(condition-case e \
               (progn (condition-case nil (signal 'arith-error nil) (error nil)) (car 1)) \
               (error e))"
        ),
        "(wrong-type-argument listp 1)"
    );
}

/// The same hazard through `eval`: resolving a form's head is *speculative* —
/// `macroexpand-1` asks "does this name a macro?" — so a failed probe must leave
/// nothing behind. A `cond` clause whose test is a call (`((car 1) 1)`) probes a
/// non-symbol head, and that probe used to register an `invalid-function` object
/// which then replaced the clause's real error.
#[test]
fn a_speculative_function_lookup_leaves_no_error_behind() {
    assert_eq!(
        eval("(condition-case e (eval '(cond ((car 1) 1)) t) (error e))"),
        "(wrong-type-argument listp 1)"
    );
    assert_eq!(
        eval("(condition-case e (eval '(cond ((= 1 nil) 1) (t 2)) t) (error e))"),
        "(wrong-type-argument number-or-marker-p nil)"
    );
    // A cond whose test simply evaluates still works.
    assert_eq!(eval("(eval '(cond ((null 1) 1) (t 2)) t)"), "2");
    assert_eq!(eval("(eval '(cond (1 'yes)) t)"), "yes");
}

/// The errors that do carry a structured object still produce it.
#[test]
fn structured_errors_keep_their_data() {
    assert_eq!(
        eval("(condition-case e (error \"x %d\" 1) (error e))"),
        "(error \"x 1\")"
    );
    assert_eq!(
        eval("(condition-case e (signal 'wrong-type-argument (list 'listp 1)) (error e))"),
        "(wrong-type-argument listp 1)"
    );
    assert_eq!(
        eval("(condition-case e (funcall 5) (error e))"),
        "(invalid-function 5)"
    );
    assert_eq!(
        eval("(condition-case e (funcall (lambda (x) x) 1 2) (error e))"),
        "(wrong-number-of-arguments #[(x) (x) (t)] 2)"
    );
    assert_eq!(eval("(ignore-errors (car 1))"), "nil");
}

/// `min`/`max` are subrs, like Emacs — which is what their arity error and their
/// printed form depend on (`seq-min` reaches them through `apply`).
#[test]
fn min_and_max_are_subrs() {
    assert_eq!(
        eval("(list (max 1 5 3) (min 1 5 3) (max 1.5 2))"),
        "(5 1 2)"
    );
    assert_eq!(eval("(max (expt 2 70) 1)"), "1180591620717411303424");
    assert_eq!(eval("(min 1 0.0e+NaN)"), "0.0e+NaN");
    assert_eq!(
        eval("(condition-case e (min) (error e))"),
        "(wrong-number-of-arguments min 0)"
    );
    assert_eq!(
        eval("(condition-case e (seq-min (vector)) (error e))"),
        "(wrong-number-of-arguments #<subr min> 0)"
    );
}

/// An improper list names its offending TAIL, not the whole object; a
/// non-sequence names itself.
#[test]
fn improper_lists_name_their_tail() {
    assert_eq!(
        eval("(condition-case e (reverse (cons 1 2)) (error e))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval("(condition-case e (mapcar #'car 97) (error e))"),
        "(wrong-type-argument sequencep 97)"
    );
}

/// `substring`'s FROM/TO are integers — a float signals rather than truncating —
/// and `seq-take`/`seq-drop` check N before touching the sequence.
#[test]
fn index_arguments_are_checked() {
    assert_eq!(
        eval("(condition-case e (substring \"abcd\" 1.5) (error e))"),
        "(wrong-type-argument integerp 1.5)"
    );
    assert_eq!(eval("(substring \"abcd\" 1 3)"), "\"bc\"");
    assert_eq!(
        eval("(condition-case e (seq-take -1 'car) (error e))"),
        "(wrong-type-argument number-or-marker-p car)"
    );
    assert_eq!(eval("(seq-take (list 1 2 3) 2)"), "(1 2)");
}
