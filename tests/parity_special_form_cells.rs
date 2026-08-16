//! A special form is fbound, is a subr, and cannot be called.
//!
//! In Emacs `if`, `let`, `progn` … are ordinary subrs whose `max_args` is
//! `UNEVALLED`, so they answer `fboundp`, `symbol-function`, `indirect-function`,
//! `subrp` and `subr-name` exactly like `car` does — and `funcall` refuses them
//! with `(invalid-function #<subr if>)` rather than treating them as undefined.
//!
//! elisprs lowers all of them in the compiler and so had no function cell for
//! them at all: `(fboundp 'if)` was nil, `(symbol-function 'if)` was nil, and
//! `(funcall 'if 1 2)` reported `(void-function if)`. The cell now lives in the
//! host's introspection side table — deliberately *not* the symbol's real
//! function cell, because `(functionp 'if)` is nil in Emacs and resolving `if`
//! to something callable would make it t.
//!
//! Every expectation below is `emacs -Q --batch --eval '(prin1 …)'` on
//! GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Emacs 30.2: `(t t t t)`. elisprs answered `(nil nil nil nil)` — every special
/// form looked undefined to any `fboundp` guard.
#[test]
fn special_forms_are_fbound() {
    assert_eq!(
        eval("(list (fboundp 'if) (fboundp 'let) (fboundp 'progn) (fboundp 'while))"),
        "(t t t t)"
    );
    // Through a variable too, so this cannot be a compile-time special case of
    // the literal `'if`.
    assert_eq!(eval("(let ((x 'catch)) (fboundp x))"), "t");
    assert_eq!(
        eval("(mapcar #'fboundp '(if car quote no-such-fn-xyz))"),
        "(t t t nil)"
    );
}

/// The cell is the subr itself, and everything that reads a function cell sees
/// it. Emacs 30.2: `(#<subr if> subr t "if" #<subr if>)`.
#[test]
fn the_cell_is_a_subr_named_after_the_form() {
    assert_eq!(
        eval(
            "(list (symbol-function 'if) (type-of (symbol-function 'if))
                   (subrp (symbol-function 'if)) (subr-name (symbol-function 'if))
                   (indirect-function 'if))"
        ),
        "(#<subr if> subr t \"if\" #<subr if>)"
    );
}

/// `special-form-p` answers for the subr object, not only for the symbol —
/// Emacs's `Fspecial_form_p` dereferences a symbol and then asks the subr.
/// Emacs 30.2: `(t t nil nil)`.
#[test]
fn special_form_p_accepts_the_object_and_the_symbol() {
    assert_eq!(
        eval(
            "(list (special-form-p 'if) (special-form-p (symbol-function 'if))
                   (special-form-p 'car) (special-form-p (symbol-function 'car)))"
        ),
        "(t t nil nil)"
    );
}

/// Having a function cell must NOT make a special form callable or a function.
/// Emacs 30.2: `(nil nil)` — `functionp` excludes `UNEVALLED` subrs.
#[test]
fn a_special_form_is_still_not_a_function() {
    assert_eq!(
        eval("(list (functionp 'if) (functionp 'quote))"),
        "(nil nil)"
    );
}

/// Calling one is an error about the function, not about it being missing.
/// Emacs 30.2: `(invalid-function #<subr if>)`, and the data holds the subr
/// *object* — `(car (cdr e))` is the subr, so it cannot be a string standing in
/// for one.
#[test]
fn calling_a_special_form_signals_invalid_function() {
    assert_eq!(
        eval("(condition-case e (funcall 'if 1 2) (error e))"),
        "(invalid-function #<subr if>)"
    );
    assert_eq!(
        eval("(condition-case e (apply 'quote '(1)) (error e))"),
        "(invalid-function #<subr quote>)"
    );
    // Reached through the object rather than the symbol.
    assert_eq!(
        eval("(condition-case e (funcall (symbol-function 'and) t) (error e))"),
        "(invalid-function #<subr and>)"
    );
    // The datum is the subr, not its printed text.
    assert_eq!(
        eval("(condition-case e (funcall 'if 1 2) (error (subrp (car (cdr e)))))"),
        "t"
    );
}

/// A name that is not a special form must still report the plain miss, so the
/// new branch cannot swallow ordinary `void-function` reporting.
#[test]
fn an_undefined_symbol_is_still_void_function() {
    assert_eq!(
        eval("(condition-case e (funcall 'no-such-fn-xyz 1) (error e))"),
        "(void-function no-such-fn-xyz)"
    );
    assert_eq!(eval("(fboundp 'no-such-fn-xyz)"), "nil");
}

/// `func-arity` already reported `(MIN . unevalled)` from a separate table; it
/// must keep doing so now that a cell exists, rather than reading the stand-in
/// subr's `max` (which is `None`, i.e. `many`).
#[test]
fn func_arity_still_reports_unevalled() {
    assert_eq!(eval("(func-arity 'if)"), "(2 . unevalled)");
    assert_eq!(eval("(func-arity 'progn)"), "(0 . unevalled)");
}
