//! Parity gaps found by hand-built differential probe corpora run against GNU
//! Emacs 30.2 in the semantic areas `scripts/fuzz/gen.el` does not generate:
//! `print-circle` shared/circular printing, per-iteration `dolist` binding, the
//! `error`/`quit` condition split, `unwind-protect` cleanup precedence,
//! macro-definition ordering, and the `cl-loop` `while`/`until`/`downfrom`
//! clauses.
//!
//! Every expectation here is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch --eval '(prin1 EXPR)'` — not of the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `print-circle` labels every object reachable more than once with `#N=` on its
/// first appearance and `#N#` afterwards. Without it the printer had no notion of
/// sharing at all, so a circular cdr chain appended to its output buffer forever
/// instead of terminating.
#[test]
fn print_circle_labels_shared_and_circular_structure() {
    // Circular through the cdr — the case that used to hang.
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((x (list 1 2))) (setcdr (cdr x) x) x)))"),
        "\"#1=(1 2 . #1#)\""
    );
    // Circular through the car.
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((x (list 1 2))) (setcar x x) x)))"),
        "\"#1=(#1# 2)\""
    );
    // Shared but acyclic: one label, reused for every later reference.
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((y (list 1))) (list y y))))"),
        "\"(#1=(1) #1#)\""
    );
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((a (list 1))) (list a a a))))"),
        "\"(#1=(1) #1# #1#)\""
    );
    // Vectors and records are labellable containers too, in both directions.
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((v (vector 1 2))) (list v v))))"),
        "\"(#1=[1 2] #1#)\""
    );
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((s (list 1))) (vector s s))))"),
        "\"[#1=(1) #1#]\""
    );
    // Nothing shared: `print-circle` must not perturb ordinary output.
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (list 1 2 3)))"),
        "\"(1 2 3)\""
    );
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string '(1 . 2)))"),
        "\"(1 . 2)\""
    );
}

/// `print-circle` is a dynamic variable: the Rust printer reads the symbol's
/// value cell, so `let`-binding it has to be a special binding, not a lexical one.
#[test]
fn print_circle_is_a_special_variable() {
    assert_eq!(
        eval("(let ((print-circle t)) (symbol-value 'print-circle))"),
        "t"
    );
    assert_eq!(eval("print-circle"), "nil");
}

/// subr.el binds `dolist`'s variable with a `let` INSIDE the loop, so every
/// iteration gets a fresh binding. Under lexical binding that is observable: a
/// closure created in the body captures that iteration's value, not one shared
/// cell holding the final value.
#[test]
fn dolist_binds_its_variable_per_iteration() {
    assert_eq!(
        eval("(let ((r nil)) (dolist (x '(1 2 3) r) (push (lambda () x) r)) (mapcar #'funcall r))"),
        "(3 2 1)"
    );
    // The ordinary (non-capturing) uses keep working, RESULT included.
    assert_eq!(
        eval("(let ((r nil)) (dolist (x '(1 2 3) r) (push x r)))"),
        "(3 2 1)"
    );
    assert_eq!(eval("(dolist (x '(1 2)) x)"), "nil");
    // ... and the expansion is subr.el's, tail variable and all.
    assert_eq!(
        eval("(macroexpand '(dolist (x l r) (f x)))"),
        "(let ((tail l)) (while tail (let ((x (car tail))) (f x) (setq tail (cdr tail)))) r)"
    );
}

/// `dotimes` has the same contract as `dolist`: subr.el runs the body inside
/// `(let ((VAR counter)) …)`, so a closure made in the body captures that
/// iteration's value. Hoisting VAR leaked the *post-loop* count into every one.
#[test]
fn dotimes_binds_its_variable_per_iteration() {
    assert_eq!(
        eval("(let ((fs '())) (dotimes (i 3) (push (lambda () i) fs)) (mapcar #'funcall (nreverse fs)))"),
        "(0 1 2)"
    );
    // RESULT sees the final counter, and the loop still counts.
    assert_eq!(eval("(dotimes (i 3 'done) i)"), "done");
    assert_eq!(
        eval("(let ((r nil)) (dotimes (i 3 r) (push i r)))"),
        "(2 1 0)"
    );
    assert_eq!(
        eval("(let ((n 0)) (dotimes (i 4) (setq n (+ n i))) n)"),
        "6"
    );
    assert_eq!(eval("(dotimes (i 0 'empty) i)"), "empty");
    // The loop variable must not leak past the loop.
    assert_eq!(
        eval("(let ((i 'outer)) (dotimes (i 2) (ignore i)) i)"),
        "outer"
    );
    assert_eq!(
        eval("(macroexpand '(dotimes (i n) b))"),
        "(let ((upper-bound n) (counter 0)) (while (< counter upper-bound) (let ((i counter)) b) (setq counter (1+ counter))))"
    );
}

/// A nested `cl-flet`'s binding list is not a list of call sites, and the names
/// it binds shadow the outer ones. Rewriting the inner binding's own head
/// produced `(funcall G (n) …)`, which failed with "malformed lambda list".
#[test]
fn nested_cl_flet_shadows_instead_of_corrupting_its_bindings() {
    assert_eq!(
        eval("(cl-flet ((f (n) (* n 2))) (cl-flet ((f (n) (+ n 1))) (f 5)))"),
        "6"
    );
    // The outer binding is still reachable outside the shadowing form.
    assert_eq!(
        eval("(cl-flet ((f (n) (* n 2))) (list (cl-flet ((f (n) (+ n 1))) (f 5)) (f 5)))"),
        "(6 10)"
    );
    // Plain and recursive cases are unaffected.
    assert_eq!(eval("(cl-flet ((f (x) (* x 2))) (f 3))"), "6");
    assert_eq!(
        eval("(cl-labels ((f (n) (if (= n 0) 1 (* n (f (1- n)))))) (f 5))"),
        "120"
    );
    assert_eq!(
        eval("(cl-labels ((ev (n) (if (= n 0) t (od (1- n)))) (od (n) (if (= n 0) nil (ev (1- n))))) (ev 10))"),
        "t"
    );
}

/// subr.el expands `pop` through `car-safe`, not `car`.
#[test]
fn pop_expands_through_car_safe() {
    assert_eq!(
        eval("(prin1-to-string (macroexpand '(pop x)))"),
        "\"(car-safe (prog1 x (setq x (cdr x))))\""
    );
    assert_eq!(eval("(let ((x '(1 2))) (list (pop x) x))"), "(1 (2))");
}

/// `quit` is seeded with `error-conditions` = (quit) alone, so it is NOT an
/// `error`: only a `t` handler is a true catch-all. Ordinary errors are
/// unaffected.
#[test]
fn error_handler_does_not_catch_quit() {
    assert_eq!(eval("(get 'quit 'error-conditions)"), "(quit)");
    assert_eq!(
        eval("(condition-case e (signal 'quit nil) (error 'caught) (t 'top))"),
        "top"
    );
    assert_eq!(eval("(condition-case e (signal 'quit nil) (quit 'q))"), "q");
    // Regression guard: `error` still catches everything that derives from it.
    assert_eq!(
        eval("(condition-case e (error \"x\") (error 'caught))"),
        "caught"
    );
    assert_eq!(
        eval("(condition-case e (signal 'arith-error nil) (error 'caught))"),
        "caught"
    );
}

/// An `unwind-protect` cleanup runs outside the body's protection, so an error it
/// signals supersedes whatever the body was doing. Discarding the cleanup's
/// result silently swallowed every failure inside a cleanup form.
#[test]
fn unwind_protect_cleanup_error_supersedes_the_body() {
    assert_eq!(
        eval("(condition-case e (unwind-protect (error \"in\") (error \"cleanup\")) (error (cadr e)))"),
        "\"cleanup\""
    );
    // A cleanup that does not signal still cannot change the body's value, and
    // still runs on both the normal and the error path.
    assert_eq!(
        eval("(let ((n 0)) (list (ignore-errors (unwind-protect 1 (setq n 2))) n))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(let ((n 0)) (list (ignore-errors (unwind-protect (error \"x\") (setq n 2))) n))"),
        "(nil 2)"
    );
    // A throw still passes through the cleanup to its catch.
    assert_eq!(eval("(catch 'a (unwind-protect (throw 'a 1) 2))"), "1");
}

/// A macro defined and used inside the SAME enclosing form has to be installed
/// before its siblings are expanded — Emacs's interpreter gets this for free by
/// evaluating `progn` forms one at a time.
#[test]
fn a_macro_is_usable_in_the_form_that_defines_it() {
    assert_eq!(
        eval("(progn (defmacro fzm1 (a b) (list 'list a b)) (fzm1 1 2))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(progn (defmacro fzm2 (&rest body) (cons 'progn body)) (fzm2 1 2 3))"),
        "3"
    );
    assert_eq!(
        eval("(progn (defmacro fzm3 (x) `(list ,x ,x)) (fzm3 (+ 1 1)))"),
        "(2 2)"
    );
    assert_eq!(
        eval("(let ((v (progn (defmacro fzm4 (x) `(* ,x 2)) (fzm4 21)))) v)"),
        "42"
    );
}

/// `while`/`until` terminate the loop where they appear, so they observe the
/// value the preceding `for` clause just installed — and their exit is a NORMAL
/// termination, so `finally` and the accumulator still produce the loop's value.
#[test]
fn cl_loop_while_and_until_see_the_current_iteration() {
    assert_eq!(
        eval("(cl-loop for i in '(1 2 3) while (< i 3) collect i)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-loop for i in '(1 2 3) while (< i 3) collect i finally return (list :fin))"),
        "(:fin)"
    );
    assert_eq!(
        eval("(cl-loop for i in '(1 2 3) until (> i 2) collect i)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-loop repeat 5 for i from 1 while (< i 3) collect i)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 10 until (> i 3) finally return i)"),
        "4"
    );
    // A `while` with no `for` to observe still gates the loop from the top.
    assert_eq!(eval("(cl-loop while nil collect 1)"), "nil");
    assert_eq!(
        eval("(let ((i 0)) (cl-loop while (< i 3) do (setq i (1+ i))) i)"),
        "3"
    );
}

/// `to` takes its direction from the iteration, so `downfrom 3 to 1` terminates
/// at `(>= i 1)`. Testing `(<= 3 1)` made the body never run.
#[test]
fn cl_loop_downfrom_counts_down_to_its_limit() {
    assert_eq!(eval("(cl-loop for i downfrom 3 to 1 collect i)"), "(3 2 1)");
    assert_eq!(
        eval("(cl-loop for i downfrom 5 above 2 collect i)"),
        "(5 4 3)"
    );
    // Upward ranges are untouched.
    assert_eq!(eval("(cl-loop for i from 1 to 3 collect i)"), "(1 2 3)");
    assert_eq!(
        eval("(cl-loop for i from 1 to 6 by 2 collect i)"),
        "(1 3 5)"
    );
    assert_eq!(eval("(cl-loop for i below 4 collect i)"), "(0 1 2 3)");
}

/// The abnormal loop exits keep returning their own thrown value rather than the
/// accumulator — they use a different tag from the while/until termination.
#[test]
fn cl_loop_abnormal_exits_still_win_over_the_accumulator() {
    assert_eq!(
        eval("(cl-loop for i from 1 to 10 do (when (> i 3) (cl-return i)))"),
        "4"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 5 when (= i 3) return i)"),
        "3"
    );
    assert_eq!(eval("(cl-loop for i from 1 to 3 always (< i 5))"), "t");
    assert_eq!(eval("(cl-loop for i from 1 to 3 never (> i 5))"), "t");
    assert_eq!(
        eval("(cl-loop for i from 1 to 3 thereis (and (= i 2) 'yes))"),
        "yes"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 3 collect i into acc finally return (reverse acc))"),
        "(3 2 1)"
    );
}

/// Emacs 30 renamed the interpreted-closure type, and a macro's function cell is
/// a `(macro . FUNCTION)` cons rather than a function object.
#[test]
fn type_of_names_emacs_30_function_types() {
    assert_eq!(eval("(type-of (lambda ()))"), "interpreted-function");
    assert_eq!(
        eval("(type-of (progn (defmacro zzq (x) x) (symbol-function 'zzq)))"),
        "cons"
    );
    // Unchanged neighbours.
    assert_eq!(eval("(type-of #'car)"), "symbol");
    assert_eq!(eval("(type-of 1)"), "integer");
}

/// print.c writes a newline BEFORE the object as well as after — that leading
/// newline is what separates successive `print` calls.
#[test]
fn print_brackets_its_object_with_newlines() {
    assert_eq!(
        eval("(equal (with-output-to-string (print 'a)) \"\\na\\n\")"),
        "t"
    );
    assert_eq!(
        eval("(equal (with-output-to-string (print 'a) (print 'b)) \"\\na\\n\\nb\\n\")"),
        "t"
    );
    // `princ`/`prin1` are unaffected.
    assert_eq!(
        eval("(equal (with-output-to-string (princ \"a\") (prin1 \"b\")) \"a\\\"b\\\"\")"),
        "t"
    );
}

/// A keyword is its own value, so `symbol-value` never reports `void-variable`
/// for one.
#[test]
fn keywords_are_their_own_value() {
    assert_eq!(eval("(symbol-value :a)"), ":a");
    assert_eq!(eval("(symbol-value :foo-bar)"), ":foo-bar");
}

/// A car that is not a symbol carrying an `error-conditions` chain is not an
/// error symbol at all: Emacs renders the whole object "peculiar error" rather
/// than printing the car as if it named a condition.
#[test]
fn error_message_string_reports_a_peculiar_error() {
    assert_eq!(eval("(error-message-string (list))"), "\"peculiar error\"");
    assert_eq!(
        eval("(error-message-string (list nil t))"),
        "\"peculiar error: t\""
    );
    // Real error symbols are untouched.
    assert_eq!(eval("(error-message-string '(error \"hi\"))"), "\"hi\"");
    assert_eq!(
        eval("(error-message-string '(arith-error))"),
        "\"Arithmetic error\""
    );
    assert_eq!(
        eval("(error-message-string '(wrong-type-argument integerp \"x\"))"),
        "\"Wrong type argument: integerp, \\\"x\\\"\""
    );
}

/// seq.el signals for an unrecognized TYPE instead of handing the input back.
#[test]
fn seq_into_rejects_an_unknown_type_name() {
    assert_eq!(
        eval("(condition-case e (seq-into \"ab\" 'foo) (error e))"),
        "(error \"Not a sequence type name: foo\")"
    );
    assert_eq!(eval("(seq-into \"ab\" 'list)"), "(97 98)");
    assert_eq!(eval("(seq-into '(1 2) 'vector)"), "[1 2]");
}

/// cl-lib.el defines `cl-rem`/`cl-mod` as the remainder half of
/// `cl-truncate`/`cl-floor`, so they take floats — `mod`/`%` are integer-only.
#[test]
fn cl_rem_and_cl_mod_accept_floats() {
    assert_eq!(eval("(cl-rem 7.5 2)"), "1.5");
    assert_eq!(eval("(cl-mod -7.5 2)"), "0.5");
    assert_eq!(
        eval("(cl-rem (expt (+ 0.0 42) 4) (+ 2.5 (abs 1000)))"),
        "938.5"
    );
    // Integer behaviour is unchanged: `cl-rem` truncates, `cl-mod` floors.
    assert_eq!(eval("(cl-rem -7 2)"), "-1");
    assert_eq!(eval("(cl-mod -7 2)"), "1");
    assert_eq!(eval("(cl-mod 7 -2)"), "-1");
}

/// `last` with a count must use `safe-length`: an improper list has no `length`,
/// so counting one signalled instead of answering.
#[test]
fn last_with_a_count_handles_an_improper_list() {
    assert_eq!(eval("(last (cons \"z\" 1.5) 7)"), "(\"z\" . 1.5)");
    assert_eq!(eval("(last (cons 1 2) 1)"), "(1 . 2)");
    assert_eq!(eval("(last '(1 2 . 3))"), "(2 . 3)");
    // Proper lists are unaffected — `safe-length` and `length` agree there.
    assert_eq!(eval("(last '(1 2 3) 2)"), "(2 3)");
    assert_eq!(eval("(last '(1 2 3) 0)"), "nil");
    assert_eq!(eval("(last '(1 2 3) 9)"), "(1 2 3)");
}
