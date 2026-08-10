//! `macroexpand`/`macroexpand-1`/`macroexpand-all` unfolding the intrinsic
//! `when`/`unless` macros that elisprs lowers as compiler special forms.
//!
//! Every expectation is re-measured against GNU Emacs 30.2 (`emacs -Q --batch`),
//! whose `subr.el` defines:
//!
//!   (defmacro when (cond &rest body)
//!     (if body (list 'if cond (cons 'progn body))
//!       (macroexp-warn-and-return … (list 'progn cond nil) '(empty-body when) t)))
//!
//! The header used to quote only the first arm and call it the whole
//! definition, which is why the two EMPTY-BODY rows below were pinned as
//! `(if x (progn))` and `(if x nil)`. GNU Emacs 30.2 answers `(progn x nil)`
//! for both — no version of Emacs produced the pinned strings, and the code
//! matched them. Corrected in both places: the expansion, and the expectation.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `when` -> `(if COND (progn BODY...))`.
#[test]
fn macroexpand_when() {
    assert_eq!(eval("(macroexpand '(when t 1))"), "(if t (progn 1))");
    assert_eq!(eval("(macroexpand '(when t 1 2))"), "(if t (progn 1 2))");
    // An empty body takes subr.el's OTHER arm: COND is still evaluated and the
    // form is still nil, but the expansion is `(progn COND nil)` — the shape
    // that carries the "`when' with empty body" warning upstream.
    assert_eq!(eval("(macroexpand '(when x))"), "(progn x nil)");
    assert_eq!(eval("(macroexpand-all '(when x))"), "(progn x nil)");
    // …and it still evaluates COND exactly once.
    assert_eq!(eval("(let ((n 0)) (list (when (setq n 1)) n))"), "(nil 1)");
}

/// `unless` -> `(if COND nil BODY...)`.
#[test]
fn macroexpand_unless() {
    assert_eq!(eval("(macroexpand '(unless c a))"), "(if c nil a)");
    assert_eq!(eval("(macroexpand '(unless c a b))"), "(if c nil a b)");
    assert_eq!(eval("(macroexpand '(unless x))"), "(progn x nil)");
    assert_eq!(
        eval("(let ((n 0)) (list (unless (setq n 1)) n))"),
        "(nil 1)"
    );
}

/// `macroexpand-1` performs exactly one step and yields the same shape here
/// (the result's head `if` is a special form, not a macro).
#[test]
fn macroexpand_1_single_step() {
    assert_eq!(eval("(macroexpand-1 '(when t 1 2))"), "(if t (progn 1 2))");
    assert_eq!(eval("(macroexpand-1 '(unless c a))"), "(if c nil a)");
}

/// `macroexpand-all` recurses into sub-forms, unfolding nested intrinsics and
/// descending through binding forms without touching the binding VARs.
#[test]
fn macroexpand_all_recursive() {
    assert_eq!(
        eval("(macroexpand-all '(when t (unless c a)))"),
        "(if t (progn (if c nil a)))"
    );
    assert_eq!(
        eval("(macroexpand-all '(let ((a 1)) (when a (unless b c))))"),
        "(let ((a 1)) (if a (progn (if b nil c))))"
    );
}

/// A user macro shadowing `when` wins over the intrinsic fallback (the callers
/// consult `macroexpand_1` — the real closure — before the intrinsic table).
#[test]
fn user_macro_shadows_intrinsic() {
    assert_eq!(
        eval("(defmacro when (c &rest b) (list 'MY c)) (macroexpand-1 '(when x 1))"),
        "(MY x)"
    );
}

/// The prelude's own `dolist`/`dotimes` macros still expand (they are genuine
/// macros, unaffected by the intrinsic path).
#[test]
fn dolist_dotimes_still_expand() {
    // The expansion is a `let`+`while`; asserting the head keeps this robust
    // against interior naming while proving expansion happened.
    assert_eq!(eval("(car (macroexpand '(dolist (x lst) (foo x))))"), "let");
    assert_eq!(eval("(car (macroexpand '(dotimes (i 3) (foo i))))"), "let");
}

/// Runtime `when`/`unless` are unchanged: the compiler still lowers them via its
/// dedicated fast path (the intrinsic expansion is off the compile pipeline).
#[test]
fn runtime_when_unless_unchanged() {
    assert_eq!(eval("(when t 1 2 3)"), "3");
    assert_eq!(eval("(when nil 1)"), "nil");
    assert_eq!(eval("(unless nil 'yes)"), "yes");
    assert_eq!(eval("(unless t 'no)"), "nil");
}

/// Classification predicates match Emacs: `when`/`unless` are macros, not
/// special forms.
#[test]
fn classification_predicates() {
    assert_eq!(eval("(list (macrop 'when) (macrop 'unless))"), "(t t)");
    assert_eq!(
        eval("(list (special-form-p 'when) (special-form-p 'unless))"),
        "(nil nil)"
    );
}
