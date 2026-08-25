//! The hook API is `add-hook` plus four more entry points, and removing a
//! function from a hook needs `equal` to work on closures.
//!
//! elisprs had `add-hook`, `run-hooks` and `run-hook-with-args`; `remove-hook`,
//! `run-hook-with-args-until-success`, `run-hook-with-args-until-failure` and
//! `run-hook-wrapped` all answered `void-function`. `remove-hook` matches the
//! function to drop with `member` — i.e. with `equal` — so an anonymous hook
//! function is only removable if two structurally identical closures are
//! `equal`, which they are in Emacs and were not here.
//!
//! Every expectation is `emacs -Q --batch` on GNU Emacs 30.2 for the same form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `remove-hook` drops the function and leaves everything else alone; a hook
/// that never had it, or was never bound, is untouched.
#[test]
fn remove_hook_drops_one_function() {
    assert_eq!(
        eval("(progn (defvar ha nil) (add-hook 'ha 'f1) (add-hook 'ha 'f2) (remove-hook 'ha 'f1) ha)"),
        "(f2)"
    );
    assert_eq!(
        eval("(progn (defvar ho nil) (add-hook 'ho 'f1) (add-hook 'ho 'f2) (remove-hook 'ho 'f2) ho)"),
        "(f1)"
    );
    assert_eq!(
        eval("(progn (defvar hb nil) (add-hook 'hb 'f1) (remove-hook 'hb 'f9) hb)"),
        "(f1)"
    );
    assert_eq!(
        eval("(progn (defvar hc nil) (remove-hook 'hc 'f1) hc)"),
        "nil"
    );
    assert_eq!(eval("(remove-hook 'never-bound-hook-zz 'f1)"), "nil");
    // A hook whose value is a bare function, not a list.
    assert_eq!(
        eval("(progn (defvar hp nil) (setq hp 'onlyfn) (remove-hook 'hp 'onlyfn) hp)"),
        "nil"
    );
}

/// The depth entry goes with the function (bug#46414): leaving it behind leaks,
/// and re-adding the same function would then land at the stale depth.
#[test]
fn remove_hook_drops_the_depth_entry_too() {
    assert_eq!(
        eval("(progn (defvar hd nil) (add-hook 'hd 'f1 90) (add-hook 'hd 'f2) (remove-hook 'hd 'f2) hd)"),
        "(f1)"
    );
    assert_eq!(
        eval("(progn (defvar he nil) (add-hook 'he 'f1) (add-hook 'he 'f2 90) (remove-hook 'he 'f2) (default-value (get 'he 'hook--depth-alist)))"),
        "nil"
    );
}

/// `-until-success` stops at the first non-nil and answers it; an empty or
/// unbound hook answers nil.
#[test]
fn run_hook_with_args_until_success() {
    assert_eq!(
        eval("(progn (defvar hh nil) (add-hook 'hh (lambda () nil)) (add-hook 'hh (lambda () 'x)) (run-hook-with-args-until-success 'hh))"),
        "x"
    );
    assert_eq!(
        eval("(progn (defvar hf nil) (run-hook-with-args-until-success 'hf))"),
        "nil"
    );
    assert_eq!(
        eval("(run-hook-with-args-until-success 'unbound-hook-zz1)"),
        "nil"
    );
    // ARGS reach the hook function.
    assert_eq!(
        eval("(progn (defvar hk1 nil) (add-hook 'hk1 (lambda (x) (* x 2))) (run-hook-with-args-until-success 'hk1 5))"),
        "10"
    );
}

/// `-until-failure` stops at the first nil and answers nil; with nothing to
/// fail — no functions at all, or an unbound hook — it answers t.
#[test]
fn run_hook_with_args_until_failure() {
    assert_eq!(
        eval("(progn (defvar hi nil) (add-hook 'hi (lambda () t)) (add-hook 'hi (lambda () nil)) (run-hook-with-args-until-failure 'hi))"),
        "nil"
    );
    assert_eq!(
        eval("(progn (defvar hj nil) (add-hook 'hj (lambda () t)) (run-hook-with-args-until-failure 'hj))"),
        "t"
    );
    assert_eq!(
        eval("(progn (defvar hg nil) (run-hook-with-args-until-failure 'hg))"),
        "t"
    );
    assert_eq!(
        eval("(run-hook-with-args-until-failure 'unbound-hook-zz2)"),
        "t"
    );
}

/// `run-hook-wrapped` hands each function to the wrapper, and stops at the
/// wrapper's first non-nil result.
#[test]
fn run_hook_wrapped_calls_through_the_wrapper() {
    assert_eq!(
        eval("(progn (defvar hl nil) (add-hook 'hl (lambda () 'v)) (run-hook-wrapped 'hl (lambda (f) (funcall f))))"),
        "v"
    );
    assert_eq!(
        eval("(progn (defvar hm nil) (add-hook 'hm (lambda () nil)) (add-hook 'hm (lambda () 'z)) (run-hook-wrapped 'hm (lambda (f) (funcall f))))"),
        "z"
    );
    // A wrapper that always answers nil runs every function, newest first
    // (`add-hook` conses onto the front at depth 0).
    assert_eq!(
        eval("(let (r) (defvar hn nil) (add-hook 'hn (lambda () (push 1 r))) (add-hook 'hn (lambda () (push 2 r))) (run-hook-wrapped 'hn (lambda (f) (funcall f) nil)) r)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(run-hook-wrapped 'unbound-hook-zz3 #'funcall)"),
        "nil"
    );
}

/// An interpreted closure IS its `#[ARGLIST BODY ENV]` structure, so `equal`
/// descends into it. This is what makes an anonymous hook function removable.
#[test]
fn equal_compares_closures_structurally() {
    assert_eq!(eval("(equal (lambda () 1) (lambda () 1))"), "t");
    assert_eq!(eval("(equal (lambda () 1) (lambda () 2))"), "nil");
    assert_eq!(eval("(equal (lambda (x) x) (lambda (y) y))"), "nil");
    assert_eq!(
        eval("(equal (lambda (&optional x) x) (lambda (&optional x) x))"),
        "t"
    );
    assert_eq!(
        eval("(equal (lambda (&rest r) r) (lambda (&rest r) r))"),
        "t"
    );
    assert_eq!(
        eval("(equal (lambda () (list 1 2)) (lambda () (list 1 2)))"),
        "t"
    );
    assert_eq!(eval("(equal (lambda () \"s\") (lambda () \"s\"))"), "t");
    assert_eq!(eval("(equal (lambda () 1) 5)"), "nil");
    // `eq` is unmoved: two closures are two objects.
    assert_eq!(eval("(eq (lambda () 1) (lambda () 1))"), "nil");
    assert_eq!(eval("(let ((f (lambda () 1))) (equal f f))"), "t");
}

/// The CAPTURES are part of the comparison: same body, different captured
/// value, not `equal`.
#[test]
fn equal_compares_a_closures_captures() {
    assert_eq!(
        eval("(let ((a (let ((n 5)) (lambda () n))) (b (let ((n 5)) (lambda () n)))) (equal a b))"),
        "t"
    );
    assert_eq!(
        eval("(let ((a (let ((n 5)) (lambda () n))) (b (let ((n 6)) (lambda () n)))) (equal a b))"),
        "nil"
    );
}

/// Everything built on `equal` sees the same answer — which is the point:
/// `remove-hook` reaches closures through `member`.
#[test]
fn equal_on_closures_reaches_every_caller() {
    assert_eq!(
        eval("(equal (list (lambda () 1)) (list (lambda () 1)))"),
        "t"
    );
    assert_eq!(eval("(equal [1 (lambda () 1)] [1 (lambda () 1)])"), "t");
    assert_eq!(
        eval("(member (lambda () 1) (list (lambda () 2) (lambda () 1)))"),
        "(#[nil (1) (t)])"
    );
    assert_eq!(
        eval("(assoc (lambda () 1) (list (cons (lambda () 1) 'v)))"),
        "(#[nil (1) (t)] . v)"
    );
    // An `equal`-test hash table has to hash closures structurally too, or the
    // key would land in a different bucket from an equal one.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) (puthash (lambda () 1) 'v h) (gethash (lambda () 1) h))"),
        "v"
    );
    assert_eq!(
        eval("(progn (defvar hq nil) (add-hook 'hq (lambda () 1)) (remove-hook 'hq (lambda () 1)) (length hq))"),
        "0"
    );
}
