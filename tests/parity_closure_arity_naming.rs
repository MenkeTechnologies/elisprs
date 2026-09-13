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

/// An absent body prints as `(nil)`, not `()`.
///
/// Emacs normalizes an empty closure body to the single form `nil`, uniformly
/// across `lambda`, `defun`, and `defmacro`. elisprs stored the empty slice and
/// printed `#[nil () (t)]`. Only the printed source is affected — an empty
/// compiled body already evaluated to nil, which the last row pins so the fix
/// cannot be mistaken for one that inserts a real `nil` form.
#[test]
fn an_empty_closure_body_prints_as_nil() {
    assert_eq!(
        eval("(prin1-to-string (lambda ()))"),
        "\"#[nil (nil) (t)]\""
    );
    assert_eq!(
        eval("(prin1-to-string (lambda (x)))"),
        "\"#[(x) (nil) (t)]\""
    );
    assert_eq!(
        eval("(progn (defun f7 ()) (prin1-to-string (symbol-function 'f7)))"),
        "\"#[nil (nil) (t)]\""
    );
    assert_eq!(
        eval("(progn (defmacro m7 ()) (prin1-to-string (symbol-function 'm7)))"),
        "\"(macro . #[nil (nil) (t)])\""
    );
    // A non-empty body is untouched, and an empty one still evaluates to nil.
    assert_eq!(
        eval("(prin1-to-string (lambda (x) x))"),
        "\"#[(x) (x) (t)]\""
    );
    assert_eq!(eval("(funcall (lambda ()))"), "nil");
}

/// `make-list` is a C subr in `alloc.c` (`Fmake_list`, 2991-3005) and was a
/// prelude `defun` here, so every observable that names the FUNCTION rather
/// than its value disagreed. The differential fuzzer found it as
/// `(apply #'make-list nil)`, where `apply` resolves before calling and Emacs
/// therefore names the subr object rather than the symbol.
#[test]
fn make_list_is_a_subr_not_a_closure() {
    // Resolved before the call: Emacs names `#<subr make-list>`, and elisprs
    // used to print the whole `#[(n x) …]` closure here.
    assert_eq!(
        eval("(condition-case e (apply #'make-list nil) (error e))"),
        "(wrong-number-of-arguments #<subr make-list> 0)"
    );
    assert_eq!(
        eval("(condition-case e (funcall #'make-list 1) (error e))"),
        "(wrong-number-of-arguments #<subr make-list> 1)"
    );
    // Written directly, a subr names the symbol the caller wrote — the same
    // split `a_subr_still_reports_the_designator` pins for the other subrs.
    assert_eq!(
        eval("(condition-case e (make-list 1 2 3) (error e))"),
        "(wrong-number-of-arguments make-list 3)"
    );
    // The function cell itself, and everything that reads it.
    assert_eq!(eval("(subrp (symbol-function 'make-list))"), "t");
    assert_eq!(
        eval("(subr-name (symbol-function 'make-list))"),
        "\"make-list\""
    );
    assert_eq!(eval("(type-of (symbol-function 'make-list))"), "subr");
    assert_eq!(eval("(symbol-function 'make-list)"), "#<subr make-list>");
    assert_eq!(eval("(func-arity 'make-list)"), "(2 . 2)");
    // `CHECK_FIXNAT` names the offending value, whatever type it is. These
    // already matched through the prelude `defun`'s explicit `integerp` test
    // and must keep matching through the C one.
    assert_eq!(
        eval("(condition-case e (make-list -1 'x) (error e))"),
        "(wrong-type-argument wholenump -1)"
    );
    assert_eq!(
        eval("(condition-case e (make-list 1.5 'x) (error e))"),
        "(wrong-type-argument wholenump 1.5)"
    );
    assert_eq!(
        eval("(condition-case e (make-list nil 'x) (error e))"),
        "(wrong-type-argument wholenump nil)"
    );
    assert_eq!(
        eval("(condition-case e (make-list 2305843009213693952 'x) (error e))"),
        "(wrong-type-argument wholenump 2305843009213693952)"
    );
    // One INIT object, shared by every cell — `Fcons (init, val)` in a loop.
    assert_eq!(
        eval("(let* ((x (list 1)) (l (make-list 2 x))) (eq (car l) (cadr l)))"),
        "t"
    );
    assert_eq!(eval("(make-list 0 'x)"), "nil");
    assert_eq!(eval("(make-list 3 'x)"), "(x x x)");
}
