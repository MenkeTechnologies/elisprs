//! A subr's arity is checked BEFORE its arguments are evaluated.
//!
//! Emacs's `eval_sub` resolves the head symbol's function cell first; when that
//! cell holds a subr it reads `XSUBR (fun)->max_args` and signals straight from
//! the argument-count switch, so no argument form ever runs:
//!
//! ```elisp
//! (let ((x 0) (y 0))
//!   (ignore-errors (car (setq x 1) (setq y 2)))
//!   (list x y))                     ; Emacs 30.2 => (0 0), elisprs was (1 2)
//! ```
//!
//! elisprs lowered the argument code ahead of the `CALL` op and enforced arity in
//! `call_function`, once the arguments were already evaluated onto the stack. A
//! `CHECK_ARITY` guard now precedes the argument code.
//!
//! The rule is narrower than "arity is checked first", and every case below was
//! measured against `GNU Emacs 30.2` with `emacs --batch`:
//!
//! - it holds for **subrs only**. A closure's arguments are evaluated into a
//!   vector before `funcall_lambda` ever compares counts, so `(f1 (setq x 1))`
//!   against a 0-argument `defun` still sets `x` — in Emacs too.
//! - the verdict is taken at **run time**, not compile time. `fset` and
//!   `defalias` retarget a function cell between compilation and the call, and
//!   Emacs honours whichever cell is live when the call happens: pointing `car`
//!   at a 2-argument lambda makes `(car 1 2)` legal, and pointing a fresh symbol
//!   at `(symbol-function 'car)` makes a 2-argument call to it signal.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The surplus argument of a fixed-arity subr must not run.
///
/// Emacs 30.2: `(0 0)`.
#[test]
fn a_surplus_argument_to_a_subr_is_not_evaluated() {
    assert_eq!(
        eval("(let ((x 0) (y 0)) (ignore-errors (car (setq x 1) (setq y 2))) (list x y))"),
        "(0 0)"
    );
    // The repro recorded in BUGS.md round 11. Emacs 30.2: `0`.
    assert_eq!(
        eval("(let ((n 0)) (ignore-errors (nth 1 t (setq n 9))) n)"),
        "0"
    );
    // A surplus argument to a zero-argument subr. Emacs 30.2: `0`.
    assert_eq!(
        eval("(let ((n 0)) (ignore-errors (point (setq n 3))) n)"),
        "0"
    );
}

/// Too FEW arguments is the same switch in `eval_sub`, so it is just as early.
///
/// Emacs 30.2: `0`.
#[test]
fn a_missing_argument_also_signals_before_the_supplied_ones_run() {
    assert_eq!(
        eval("(let ((n 0)) (ignore-errors (cons (setq n 1))) n)"),
        "0"
    );
}

/// The error itself is unchanged: `(wrong-number-of-arguments FUNC COUNT)` where
/// FUNC is the callee **as the caller wrote it**.
///
/// Emacs 30.2 prints exactly these.
#[test]
fn the_signalled_error_still_names_the_callee_as_written() {
    assert_eq!(
        eval("(condition-case e (car 1 2) (error e))"),
        "(wrong-number-of-arguments car 2)"
    );
    assert_eq!(
        eval("(condition-case e (cons 1) (error e))"),
        "(wrong-number-of-arguments cons 1)"
    );
    assert_eq!(
        eval("(condition-case e (nth 1) (error e))"),
        "(wrong-number-of-arguments nth 1)"
    );
}

/// An argument that would itself signal must not get the chance to: the arity
/// error wins because it is raised before the argument runs.
///
/// Emacs 30.2: `(wrong-number-of-arguments car 2)`, NOT `(error "inner")`.
#[test]
fn the_arity_error_wins_over_an_error_inside_an_argument() {
    assert_eq!(
        eval("(condition-case e (car (error \"inner\") 2) (error e))"),
        "(wrong-number-of-arguments car 2)"
    );
}

/// A closure keeps the opposite order — its arguments are evaluated first — so
/// the guard must not fire for one.
///
/// Emacs 30.2: `(1 2)` for the surplus case and `1` for the missing case.
#[test]
fn a_closures_arguments_are_still_evaluated_before_its_arity_is_checked() {
    assert_eq!(
        eval(
            "(let ((x 0) (y 0)) \
               (defun f1 (a) a) \
               (ignore-errors (f1 (setq x 1) (setq y 2))) \
               (list x y))"
        ),
        "(1 2)"
    );
    assert_eq!(
        eval("(let ((x 0)) (defun f2 (a b) (list a b)) (ignore-errors (f2 (setq x 1))) x)"),
        "1"
    );
}

/// `fset` retargeting a subr's symbol at a wider function makes the call legal —
/// the guard must consult the live cell, never a compile-time snapshot.
///
/// Emacs 30.2: `((mycar 1 2) 1 2)`.
#[test]
fn fset_can_widen_a_subr_and_the_call_then_succeeds() {
    assert_eq!(
        eval(
            "(let ((x 0) (y 0)) \
               (fset 'car (lambda (a b) (list 'mycar a b))) \
               (let ((r (car (setq x 1) (setq y 2)))) (list r x y)))"
        ),
        "((mycar 1 2) 1 2)"
    );
}

/// The converse: a symbol that named nothing at compile time, pointed at a subr
/// by `fset` before the call, checks that subr's arity — still before its
/// arguments run. This is the case a compile-time-only check cannot see.
///
/// Emacs 30.2: `(0 0)`, and the error names `myfn`, not `car`.
#[test]
fn fset_can_point_a_fresh_symbol_at_a_subr_and_the_guard_still_fires() {
    assert_eq!(
        eval(
            "(let ((x 0) (y 0)) \
               (fset 'myfn (symbol-function 'car)) \
               (ignore-errors (myfn (setq x 1) (setq y 2))) \
               (list x y))"
        ),
        "(0 0)"
    );
    assert_eq!(
        eval(
            "(progn (fset 'myfn (symbol-function 'car)) \
               (condition-case e (myfn 1 2) (error e)))"
        ),
        "(wrong-number-of-arguments myfn 2)"
    );
}

/// `defalias` to a subr goes through the same alias chain.
///
/// Emacs 30.2: `(0 0)` and `(wrong-number-of-arguments mycar 2)`.
#[test]
fn defalias_to_a_subr_resolves_through_the_alias_chain() {
    assert_eq!(
        eval(
            "(let ((x 0) (y 0)) \
               (defalias 'mycar 'car) \
               (ignore-errors (mycar (setq x 1) (setq y 2))) \
               (list x y))"
        ),
        "(0 0)"
    );
    assert_eq!(
        eval("(progn (defalias 'mycar 'car) (condition-case e (mycar 1 2) (error e)))"),
        "(wrong-number-of-arguments mycar 2)"
    );
}

/// A symbol already pointed at a subr, then pointed at a **narrower** one before
/// the call: the cell live when the call compiled would have accepted the count,
/// so only the run-time verdict can catch it. Found by `scripts/fuzz_parity.sh`
/// once the generator learned to emit these (7/1500 forms).
///
/// Emacs 30.2: `0` for both.
#[test]
fn a_symbol_retargeted_at_a_narrower_subr_is_still_caught() {
    assert_eq!(
        eval(
            "(progn (fset 'fzwa (symbol-function 'cons)) \
               (let ((n 0)) \
                 (fset 'fzwa (symbol-function 'cdr)) \
                 (ignore-errors (fzwa 1 (setq n 9))) \
                 n))"
        ),
        "0"
    );
    assert_eq!(
        eval(
            "(progn (defalias 'fzwa 'cons) \
               (let ((n 0)) (defalias 'fzwa 'cdr) (ignore-errors (fzwa 1 (setq n 9))) n))"
        ),
        "0"
    );
}

/// …and retargeted at a *lambda* it must go back to evaluating its arguments,
/// even though the symbol is on the "has named a subr" list.
///
/// Emacs 30.2: `((9 2) 9)`.
#[test]
fn a_symbol_retargeted_from_a_subr_to_a_lambda_evaluates_its_arguments() {
    assert_eq!(
        eval(
            "(progn (fset 'fzwa (symbol-function 'car)) \
               (let ((n 0)) \
                 (fset 'fzwa (lambda (a b) (list a b))) \
                 (list (fzwa (setq n 9) 2) n)))"
        ),
        "((9 2) 9)"
    );
}

/// A `MANY`-arity subr takes any count, so the guard must never fire for one.
///
/// Emacs 30.2: `10`, `(1 2 3 4 5)`.
#[test]
fn a_many_arity_subr_is_never_rejected() {
    assert_eq!(eval("(+ 1 2 3 4)"), "10");
    assert_eq!(eval("(list 1 2 3 4 5)"), "(1 2 3 4 5)");
    assert_eq!(eval("(let ((n 0)) (list (setq n 1) (setq n 2)) n)"), "2");
}

/// `funcall` and `apply` resolve the designator themselves, so their own arity is
/// what the call site is checked against — their arguments still evaluate.
///
/// Emacs 30.2 evaluates both `setq`s here and signals from the inner call.
#[test]
fn funcall_still_evaluates_its_arguments() {
    assert_eq!(
        eval("(let ((x 0) (y 0)) (ignore-errors (funcall 'car (setq x 1) (setq y 2))) (list x y))"),
        "(1 2)"
    );
}
