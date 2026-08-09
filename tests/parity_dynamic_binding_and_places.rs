//! Parity gaps closed in the dynamic-binding / generalized-place round.
//!
//! Every expectation is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch --eval '(prin1 EXPR)'` (with `(require 'cl-lib)` where the
//! form uses `cl-*`) — not of the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Evaluate inside a `condition-case` so a signalled error is compared as data,
/// the way the differential fuzzer does.
fn caught(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

// ── `lexical-binding' nil: dynamic closures ──────────────────────────────────

/// A `lambda` created under dynamic binding captures NOTHING: its free variables
/// are looked up in the symbols' value cells when it is *called*, so a binding
/// that has since been unwound is simply gone.
///
/// ```text
/// $ emacs --batch -Q --eval "(prin1 (condition-case e (eval '(funcall (let ((x 1)) (lambda () x))) nil) (error e)))"
/// (void-variable x)
/// ```
#[test]
fn dynamic_binding_closure_does_not_capture() {
    assert_eq!(
        caught("(eval '(funcall (let ((x 1)) (lambda () x))) nil)"),
        "(void-variable x)"
    );
    // The same form with LEXICAL t captures, and answers 1.
    assert_eq!(
        caught("(eval '(funcall (let ((x 1)) (lambda () x))) t)"),
        "1"
    );
    // LEXICAL omitted defaults to nil — dynamic.
    assert_eq!(
        caught("(eval '(funcall (let ((x 1)) (lambda () x))))"),
        "(void-variable x)"
    );
}

/// The loop-variable signature under dynamic binding: every closure reads `i`
/// at call time, and by then the `dotimes` binding is unwound.
/// Emacs: `(void-variable i)`.
#[test]
fn dynamic_binding_loop_closures_see_no_binding() {
    assert_eq!(
        caught(
            "(eval '(let (fs) (dotimes (i 3) (push (lambda () i) fs)) \
             (mapcar (function funcall) fs)) nil)"
        ),
        "(void-variable i)"
    );
}

/// A dynamically-bound function prints with a `nil` environment, where a
/// lexical one with nothing captured prints `(t)` — that distinction is the only
/// way to tell the two apart from Lisp.
///
/// ```text
/// $ emacs --batch -Q --eval "(prin1 (eval '(let ((x 1)) (lambda (y) x)) nil))"
/// #[(y) (x) nil]
/// $ emacs --batch -Q --eval "(prin1 (eval '(let ((x 1)) (lambda (y) x)) t))"
/// #[(y) (x) ((x . 1))]
/// ```
#[test]
fn dynamic_function_prints_nil_environment() {
    assert_eq!(
        eval("(eval '(let ((x 1)) (lambda (y) x)) nil)"),
        "#[(y) (x) nil]"
    );
    assert_eq!(eval("(eval '(lambda (y) x) nil)"), "#[(y) (x) nil]");
    assert_eq!(
        eval("(eval '(let ((x 1)) (lambda (y) x)) t)"),
        "#[(y) (x) ((x . 1))]"
    );
}

/// The mode travels with the function, not with the `eval` that made it: a
/// dynamic function binds its PARAMETERS on the specstack, so a function it
/// calls can read them. Under lexical binding the same call is `void-variable`.
///
/// ```text
/// $ emacs --batch -Q --eval "(progn (defun g () x) (prin1 (funcall (eval '(lambda (x) (g)) nil) 7)))"
/// 7
/// ```
#[test]
fn dynamic_function_binds_parameters_dynamically() {
    assert_eq!(
        eval("(progn (defun g () x) (funcall (eval '(lambda (x) (g)) nil) 7))"),
        "7"
    );
    assert_eq!(
        caught("(progn (defun g () x) (funcall (eval '(lambda (x) (g)) t) 7))"),
        "(void-variable x)"
    );
}

// ── generalized places are evaluated once ────────────────────────────────────

/// A read-modify-write macro mentions its PLACE more than once. Emacs binds the
/// place's subforms first (gv.el's `gv-letplace`), so their side effects happen
/// exactly once; substituting the place FORM at each mention repeated them.
///
/// ```text
/// $ emacs --batch -Q --eval "(progn (require 'cl-lib) (prin1 (let ((n 0) (l (list 1 2))) (cl-incf (car (progn (setq n (1+ n)) l))) (list n l))))"
/// (1 (2 2))
/// $ emacs --batch -Q --eval "(prin1 (let ((n 0) (l (list 1 2 3))) (pop (cdr (progn (setq n (1+ n)) l))) (list n l)))"
/// (1 (1 3))
/// ```
#[test]
fn place_subforms_are_evaluated_once() {
    // cl-incf: was 2.
    assert_eq!(
        eval("(let ((n 0) (l (list 1 2))) (cl-incf (car (progn (setq n (1+ n)) l))) (list n l))"),
        "(1 (2 2))"
    );
    // push: was 2.
    assert_eq!(
        eval("(let ((n 0) (l (list 1 2))) (push 9 (cdr (progn (setq n (1+ n)) l))) (list n l))"),
        "(1 (1 9 2))"
    );
    // pop mentions the place three times: was 3.
    assert_eq!(
        eval("(let ((n 0) (l (list 1 2 3))) (pop (cdr (progn (setq n (1+ n)) l))) (list n l))"),
        "(1 (1 3))"
    );
    // cl-callf, cl-pushnew and an array place go through the same helper.
    assert_eq!(
        eval(
            "(let ((n 0) (l (list 1 2))) (cl-callf 1+ (car (progn (setq n (1+ n)) l))) (list n l))"
        ),
        "(1 (2 2))"
    );
    assert_eq!(
        eval(
            "(let ((n 0) (l (list 1 2 3))) (cl-pushnew 9 (cdr (progn (setq n (1+ n)) l))) (list n l))"
        ),
        "(1 (1 9 2 3))"
    );
    assert_eq!(
        eval("(let ((n 0) (v (vector 1 2))) (cl-incf (aref (progn (setq n (1+ n)) v) 1)) (list n v))"),
        "(1 [1 3])"
    );
    // NEWELT is evaluated before the place's subforms, and also only once.
    assert_eq!(
        eval(
            "(let ((n 0) (m 0) (l (list 1 2))) \
             (push (progn (setq m (1+ m)) 9) (cdr (progn (setq n (1+ n)) l))) (list n m l))"
        ),
        "(1 1 (1 9 2))"
    );
}

/// A *symbol* subform must NOT be rebound to a temporary: some setters assign to
/// the subform itself, and a temporary would redirect the assignment away from
/// the caller's variable. `(setf (nthcdr 0 s) …)` is exactly that case.
#[test]
fn symbol_subforms_stay_assignable() {
    assert_eq!(
        eval("(let ((s (list 1 2 3))) (setf (nthcdr 0 s) (list 9)) s)"),
        "(9)"
    );
    assert_eq!(
        eval("(let ((s (list 1 2 3))) (setf (nthcdr 1 s) (list 9)) s)"),
        "(1 9)"
    );
    // cl-macs.el's `1+' shape for the bare-symbol case is unchanged.
    assert_eq!(eval("(macroexpand '(cl-incf x))"), "(setq x (1+ x))");
    assert_eq!(eval("(macroexpand '(cl-decf x))"), "(setq x (1- x))");
    assert_eq!(eval("(let ((x 5)) (cl-incf x) x)"), "6");
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (list (pop l) l))"),
        "(1 (2 3))"
    );
}

// ── recursion depth ──────────────────────────────────────────────────────────

/// Runaway recursion is a *catchable signal*, not a process abort. Emacs's
/// eval.c raises `excessive-lisp-nesting` at `max-lisp-eval-depth`; elisprs used
/// to run the native stack out and die with `fatal runtime error: stack
/// overflow`, which nothing can catch.
///
/// ```text
/// $ emacs --batch -Q --eval "(prin1 (list max-lisp-eval-depth (condition-case e (letrec ((f (lambda () (funcall f)))) (funcall f)) (error e))))"
/// (1600 (excessive-lisp-nesting 1601))
/// ```
///
/// Reaching the 1600-frame limit needs the real stack budget, so — as in
/// `deep_recursion_reaches_the_emacs_limit` — the body runs on a thread with
/// `elisprs::INTERP_STACK_BYTES`.
#[test]
fn runaway_recursion_signals_instead_of_aborting() {
    elisprs::with_interpreter_stack(|| {
        assert_eq!(eval("max-lisp-eval-depth"), "1600");
        assert_eq!(
            caught("(letrec ((f (lambda () (funcall f)))) (funcall f))"),
            "(excessive-lisp-nesting 1601)"
        );
        // It is an ordinary condition, so a `recursion-error` handler catches it.
        assert_eq!(
            eval(
                "(condition-case e (letrec ((f (lambda () (funcall f)))) (funcall f)) \
                 (recursion-error 'caught))"
            ),
            "caught"
        );
        assert_eq!(
            eval("(get 'excessive-lisp-nesting 'error-conditions)"),
            "(excessive-lisp-nesting recursion-error error)"
        );
        // A `let' around the variable is honoured — and eval.c clamps it up to a
        // floor of 100, so 5, 20 and 100 behave identically while 150 does not:
        //   $ emacs --batch -Q --eval "(prin1 (condition-case e (let ((max-lisp-eval-depth 20)) (letrec ((f (lambda (n) (funcall f (1+ n))))) (funcall f 0))) (error e)))"
        //   (excessive-lisp-nesting 101)
        for depth in ["5", "20", "100"] {
            assert_eq!(
                caught(&format!(
                    "(let ((max-lisp-eval-depth {depth})) \
                     (letrec ((f (lambda (n) (funcall f (1+ n))))) (funcall f 0)))"
                )),
                "(excessive-lisp-nesting 101)",
                "max-lisp-eval-depth {depth} must clamp to the floor of 100"
            );
        }
        assert_eq!(
            caught(
                "(let ((max-lisp-eval-depth 150)) \
                 (letrec ((f (lambda (n) (funcall f (1+ n))))) (funcall f 0)))"
            ),
            "(excessive-lisp-nesting 151)"
        );
    });
}

/// Recursion far past the old ~70-frame ceiling now completes.
///
/// ```text
/// $ emacs --batch -Q --eval "(progn (require 'cl-lib) (prin1 (cl-labels ((go (n acc) (if (= n 0) acc (go (1- n) (+ acc n))))) (go 1500 0))))"
/// 1125750
/// ```
///
/// The evaluation runs on a thread with `elisprs::INTERP_STACK_BYTES` — the same
/// budget `main.rs` gives the `elisp` binary. A cargo test thread's default 2
/// MiB is *smaller* than what the old ceiling had, so without this the test
/// would measure the harness, not the interpreter.
#[test]
fn deep_recursion_reaches_the_emacs_limit() {
    let got = elisprs::with_interpreter_stack(|| {
        eval(
            "(let ((max-lisp-eval-depth 100000)) \
             (cl-labels ((go (n acc) (if (= n 0) acc (go (1- n) (+ acc n))))) (go 1500 0)))",
        )
    });
    assert_eq!(got, "1125750");
}

// ── intrinsic macros report a function cell ──────────────────────────────────

/// `when`/`unless` are lowered by the compiler and own no function cell, but in
/// Emacs they are ordinary macros and introspection sees a `(macro . FUNCTION)`
/// cons: `(type-of (symbol-function 'when))` is `cons`, not `symbol`.
#[test]
fn intrinsic_macros_report_a_macro_function_cell() {
    assert_eq!(eval("(type-of (symbol-function 'when))"), "cons");
    assert_eq!(eval("(type-of (symbol-function 'unless))"), "cons");
    assert_eq!(eval("(car (symbol-function 'when))"), "macro");
    assert_eq!(eval("(fboundp 'when)"), "t");
    assert_eq!(eval("(fboundp 'unless)"), "t");
    // The stand-in really is subr.el's expander.
    assert_eq!(
        eval("(funcall (cdr (symbol-function 'when)) 'a 'b)"),
        "(if a (progn b))"
    );
    assert_eq!(
        eval("(funcall (cdr (symbol-function 'unless)) 'a 'b)"),
        "(if a nil b)"
    );
    // The compiler still lowers the form itself, so the macros keep working.
    assert_eq!(eval("(when t 1)"), "1");
    assert_eq!(eval("(unless nil 2)"), "2");
    assert_eq!(eval("(when nil 1)"), "nil");
    // An undefined name is still unbound.
    assert_eq!(eval("(fboundp 'zzz-nope)"), "nil");
    assert_eq!(eval("(symbol-function 'zzz-nope)"), "nil");
}
