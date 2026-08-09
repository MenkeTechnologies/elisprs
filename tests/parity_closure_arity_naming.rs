//! A wrong-arity call names the *resolved closure*, not the symbol.
//!
//! Emacs signals from two different places with two different first data:
//!
//! - a subr, from `eval_sub`:
//!   `xsignal2 (Qwrong_number_of_arguments, original_fun, make_fixnum (numargs))`
//!   — `original_fun` is the symbol the caller wrote.
//! - a closure, from `funcall_lambda`:
//!   `xsignal2 (Qwrong_number_of_arguments, … fun, make_fixnum (nargs))`
//!   — `fun` is what indirection landed on.
//!
//! elisprs passed the designator in both cases, so `(f1 1 2)` reported `f1` and
//! a `defalias`ed second name reported that second name. Expected values are
//! `emacs -Q --batch` on GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Emacs 30.2: `(wrong-number-of-arguments #[(a) (a) (t)] 2)`.
#[test]
fn a_defun_reports_its_closure() {
    assert_eq!(
        eval("(progn (defun f1 (a) a) (condition-case e (f1 1 2) (error e)))"),
        "(wrong-number-of-arguments #[(a) (a) (t)] 2)"
    );
}

/// Too *few* arguments takes the same path. Emacs 30.2:
/// `(wrong-number-of-arguments #[(a b) (a) (t)] 1)`.
#[test]
fn a_short_call_reports_its_closure_too() {
    assert_eq!(
        eval("(progn (defun f3 (a b) a) (condition-case e (f3 1) (error e)))"),
        "(wrong-number-of-arguments #[(a b) (a) (t)] 1)"
    );
}

/// Indirection through `defalias` is followed: the data names the closure, not
/// the alias. Emacs 30.2: `(wrong-number-of-arguments #[(a) (a) (t)] 2)`.
#[test]
fn an_alias_reports_the_function_it_resolves_to() {
    assert_eq!(
        eval(
            "(progn (defun f2 (a) a) (defalias 'g2 'f2) \
             (condition-case e (g2 1 2) (error e)))"
        ),
        "(wrong-number-of-arguments #[(a) (a) (t)] 2)"
    );
}

/// The subr rows must NOT change with it: a subr called by name reports the
/// name, and the same subr applied as an object reports the object.
/// Emacs 30.2: `(wrong-number-of-arguments car 2)` and
/// `(wrong-number-of-arguments #<subr car> 2)`.
#[test]
fn a_subr_still_reports_the_designator() {
    assert_eq!(
        eval("(condition-case e (car 1 2) (error e))"),
        "(wrong-number-of-arguments car 2)"
    );
    assert_eq!(
        eval("(condition-case e (funcall (symbol-function 'car) 1 2) (error e))"),
        "(wrong-number-of-arguments #<subr car> 2)"
    );
}
