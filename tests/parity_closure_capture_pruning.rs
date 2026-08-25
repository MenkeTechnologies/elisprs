//! An interpreted closure captures only the variables its body references.
//!
//! elisprs captured the whole enclosing scope chain, so
//! `(let ((n 1) (m 2)) (lambda () n))` closed over `((m . 2) (n . 1))` where
//! Emacs closes over `((n . 1))`. That is visible three ways: through `prin1`,
//! through `equal` (which compares the captured environment), and in what the
//! closure keeps alive.
//!
//! Emacs does the pruning in `cconv-make-interpreted-closure`
//! (lisp/emacs-lisp/cconv.el), whose own comment gives the reason: "reduce ENV
//! to the part actually used by the function, so we are closer to the ideal of
//! 'safe for space'". The analysis is `cconv-fv` / `cconv-analyze-form`; the
//! elisprs port lives in `src/freevars.rs` and is applied in
//! `ElispHost::instantiate_closure`.
//!
//! Expectations are `emacs -Q --batch` with a `lexical-binding: t` cookie, on
//! the installed GNU Emacs 31.1. The headline form is a direct cross-check
//! against the 30.2 reading BUGS.md recorded in round 21 — both binaries answer
//! `"#[nil (n) ((n . 1))]"` — so the pruning rule did not move between the two.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The headline: an unreferenced sibling binding is dropped, a referenced one
/// is kept.
#[test]
fn an_unreferenced_binding_is_not_captured() {
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (lambda () n)))"),
        "\"#[nil (n) ((n . 1))]\""
    );
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (lambda () (+ m n))))"),
        "\"#[nil ((+ m n)) ((m . 2) (n . 1))]\""
    );
    assert_eq!(
        eval("(let ((n 1)) (let ((m 2)) (format \"%S\" (lambda () n))))"),
        "\"#[nil (n) ((n . 1))]\""
    );
}

/// A closure that references nothing gets `(t)` — Emacs's marker for "lexically
/// bound, empty environment" — not the enclosing bindings.
#[test]
fn a_closure_capturing_nothing_prints_the_lexical_marker() {
    assert_eq!(eval("(let ((n 1)) (format \"%S\" (lambda () 5)))"), "\"#[nil (5) (t)]\"");
    // A parameter of the same name shadows the outer binding.
    assert_eq!(
        eval("(let ((n 1)) (format \"%S\" (lambda (n) n)))"),
        "\"#[(n) (n) (t)]\""
    );
    // So does a `let` inside the body.
    assert_eq!(
        eval("(let ((n 1)) (format \"%S\" (lambda () (let ((n 2)) n))))"),
        "\"#[nil ((let ((n 2)) n)) (t)]\""
    );
    // And a `condition-case` variable, over its handler bodies.
    assert_eq!(
        eval("(let ((n 1)) (format \"%S\" (lambda () (condition-case n nil (error n)))))"),
        "\"#[nil ((condition-case n nil (error n))) (t)]\""
    );
}

/// The surviving bindings keep the ENVIRONMENT's order, not the order the body
/// mentions them in — Emacs's `(mapcar (lambda (fv) (assq fv env)) …)` reads
/// them out of the environment.
#[test]
fn the_kept_bindings_stay_in_environment_order() {
    assert_eq!(
        eval("(let ((a 1) (b 2) (c 3)) (format \"%S\" (lambda () (list c a))))"),
        "\"#[nil ((list c a)) ((c . 3) (a . 1))]\""
    );
    assert_eq!(
        eval("(let ((a 1) (b 2) (c 3)) (format \"%S\" (lambda () (list a c))))"),
        "\"#[nil ((list a c)) ((c . 3) (a . 1))]\""
    );
    assert_eq!(
        eval("(let ((m 1) (n 2)) (format \"%S\" (lambda () (+ n m))))"),
        "\"#[nil ((+ n m)) ((n . 2) (m . 1))]\""
    );
}

/// A symbol in a position that is not a variable reference does not capture:
/// `'n`, `#'n`, and a call head `(n)` all leave the environment empty.
#[test]
fn a_symbol_that_is_not_a_variable_reference_captures_nothing() {
    assert_eq!(eval("(let ((n 1)) (format \"%S\" (lambda () 'n)))"), "\"#[nil ('n) (t)]\"");
    assert_eq!(
        eval("(let ((n 1)) (format \"%S\" (lambda () '(n))))"),
        "\"#[nil ('(n)) (t)]\""
    );
    assert_eq!(eval("(let ((n 1)) (format \"%S\" (lambda () #'n)))"), "\"#[nil (#'n) (t)]\"");
    assert_eq!(eval("(let ((n 1)) (format \"%S\" (lambda () (n))))"), "\"#[nil ((n)) (t)]\"");
}

/// Writing a variable captures it exactly as reading it does, and a reference
/// from a NESTED lambda escapes to the outer closure.
#[test]
fn setq_and_nested_lambdas_both_capture() {
    assert_eq!(
        eval("(let ((n 1)) (format \"%S\" (lambda () (setq n 2))))"),
        "\"#[nil ((setq n 2)) ((n . 1))]\""
    );
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (lambda () (setq m 3))))"),
        "\"#[nil ((setq m 3)) ((m . 2))]\""
    );
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (lambda () (lambda () m))))"),
        "\"#[nil (#'(lambda nil m)) ((m . 2))]\""
    );
    // A `lambda` built inside `mapcar` sees the loop variable and the outer
    // binding it uses, and nothing else.
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (mapcar (lambda (x) (lambda () (+ x n))) '(1))))"),
        "\"(#[nil ((+ x n)) ((x . 1) (n . 1))])\""
    );
}

/// Pruning must not privatize the binding: the kept nodes share the enclosing
/// chain's value cells, so `setq` still crosses the closure boundary in both
/// directions.
#[test]
fn a_pruned_capture_still_shares_the_binding() {
    assert_eq!(
        eval("(let ((n 1) (m 2)) (let ((f (lambda () (setq n (1+ n))))) (funcall f) n))"),
        "2"
    );
    assert_eq!(
        eval("(let ((n 1) (m 2)) (let ((f (lambda () n))) (setq n 9) (funcall f)))"),
        "9"
    );
    // Two closures over the same binding see each other's writes.
    assert_eq!(
        eval(
            "(let ((n 0)) \
             (let ((inc (lambda () (setq n (1+ n)))) (get (lambda () n))) \
             (funcall inc) (funcall inc) (funcall get)))"
        ),
        "2"
    );
}

/// Under dynamic binding a `lambda` captures nothing at all, so pruning has no
/// effect there — the free variables are looked up in the value cells when the
/// function is called.
#[test]
fn dynamic_binding_is_unaffected() {
    assert_eq!(
        eval("(eval '(let ((n 1)) (funcall (lambda () n))) nil)"),
        "1"
    );
    assert_eq!(
        eval("(eval '(let ((n 1) (m 2)) (funcall (lambda () (+ n m)))) nil)"),
        "3"
    );
}

/// A reference from an UNREACHABLE branch still captures. This is the mechanism
/// `oclosure--lambda` relies on: it emits a dead `(if t nil SLOT…)` branch
/// precisely so that a slot the body never reads is still in the captured
/// environment for the accessors to find. Analysis is syntactic, like Emacs's.
///
/// (`tests/cache_heap_image.rs` exercises the OClosure path end to end.)
#[test]
fn a_reference_in_a_dead_branch_still_captures() {
    assert_eq!(
        eval("(let ((n 1) (m 2)) (format \"%S\" (lambda () (if t nil m))))"),
        "\"#[nil ((if t nil m)) ((m . 2))]\""
    );
    assert_eq!(
        eval("(let ((n 1) (m 2) (k 3)) (format \"%S\" (lambda () (if t nil m k) n)))"),
        "\"#[nil ((if t nil m k) n) ((k . 3) (m . 2) (n . 1))]\""
    );
}
