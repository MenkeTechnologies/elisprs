//! End-to-end tests for the AOP pattern-intercept layer (elisprs extension,
//! ported from zshrs). Advice fires on the `call_function` join point via
//! glob-matched patterns — distinct from elisp nadvice. Each test resets the
//! thread-local host and drives real elisp through `eval_str`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn before_and_after_advice_fire_in_order_and_original_runs() {
    // before runs first, the original returns "hi bob", after runs last.
    let out = eval(
        r#"(progn
             (defvar itrace nil)
             (defun igreet (x) (concat "hi " x))
             (intercept 'before "igreet" '(setq itrace (cons 'before itrace)))
             (intercept 'after  "igreet" '(setq itrace (cons 'after itrace)))
             (let ((r (igreet "bob")))
               (list r (reverse itrace))))"#,
    );
    assert_eq!(out, r#"("hi bob" (before after))"#);
}

#[test]
fn before_only_advice_lets_the_original_run() {
    // No around/after: run_intercepts returns None, normal dispatch runs igreet.
    let out = eval(
        r#"(progn
             (defvar iran nil)
             (defun ib (x) (setq iran x) (* x 2))
             (intercept 'before "ib" '(setq iran 'advised))
             (let ((r (ib 21)))
               (list r iran)))"#,
    );
    // The original ran (r = 42) and overwrote iran with its own argument.
    assert_eq!(out, "(42 21)");
}

#[test]
fn around_advice_wraps_via_intercept_proceed() {
    // (intercept-proceed) runs (iadd 5) -> 6; the around form multiplies by 10.
    let out = eval(
        r#"(progn
             (defun iadd (x) (1+ x))
             (intercept 'around "iadd" '(* 10 (intercept-proceed)))
             (iadd 5))"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn around_advice_without_proceed_suppresses_the_original() {
    let out = eval(
        r#"(progn
             (defvar iran nil)
             (defun iproc () (setq iran t) 'original)
             (intercept 'around "iproc" ''suppressed)
             (list (iproc) iran))"#,
    );
    // The original never ran, so iran stays nil and the around value is returned.
    assert_eq!(out, "(suppressed nil)");
}

#[test]
fn after_advice_sees_timing_context_variable() {
    let out = eval(
        r#"(progn
             (defvar ims nil)
             (defun inoop () nil)
             (intercept 'after "inoop" '(setq ims (numberp intercept-ms)))
             (inoop)
             ims)"#,
    );
    assert_eq!(out, "t");
}

#[test]
fn advice_sees_name_and_args_context_variables() {
    let out = eval(
        r#"(progn
             (defvar iseen nil)
             (defun if2 (a b) (list a b))
             (intercept 'before "if2" '(setq iseen (list intercept-name intercept-args)))
             (if2 1 2)
             iseen)"#,
    );
    assert_eq!(out, r#"("if2" (1 2))"#);
}

#[test]
fn glob_pattern_matches_many_functions() {
    let out = eval(
        r#"(progn
             (defvar ihits 0)
             (defun forward-thing () 'ft)
             (intercept 'before "forward-*" '(setq ihits (1+ ihits)))
             (forward-thing)
             (forward-thing)
             ihits)"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn catch_all_pattern_fires_for_any_call() {
    let out = eval(
        r#"(progn
             (defvar iall 0)
             (defun ione () 1)
             (defun itwo () 2)
             (intercept 'before "all" '(setq iall (1+ iall)))
             (ione)
             (itwo)
             iall)"#,
    );
    // Both user calls matched "all". (Advice-body calls are guarded off.)
    assert!(out.parse::<i64>().unwrap() >= 2, "got {out}");
}

#[test]
fn recursion_guard_stops_advice_from_re_firing() {
    // Without the intercept_active guard, the advice calling irec would re-fire
    // before-advice endlessly. With the guard the nested call dispatches normally,
    // so irc lands at exactly 1.
    let out = eval(
        r#"(progn
             (defvar irc 0)
             (defun irec (n) n)
             (intercept 'before "irec"
                        '(progn (setq irc (1+ irc)) (when (< irc 3) (irec 0))))
             (irec 1)
             irc)"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn list_remove_and_clear_manage_registrations() {
    let out = eval(
        r#"(progn
             (intercept 'before "a-fn" nil)
             (intercept 'after  "b-fn" nil)
             (let ((n1 (length (intercept-list)))
                   (r  (intercept-remove 1))
                   (n2 (length (intercept-list)))
                   (c  (intercept-clear))
                   (n3 (length (intercept-list))))
               (list n1 r n2 c n3)))"#,
    );
    // Two registered (n1=2); remove id 1 -> t, one left (n2=1); clear removes the
    // remaining 1 (c=1); none left (n3=0).
    assert_eq!(out, "(2 t 1 1 0)");
}

#[test]
fn intercept_returns_the_new_id() {
    let out = eval(
        r#"(list (intercept 'before "x" nil)
                 (intercept 'after  "y" nil)
                 (intercept 'around "z" nil))"#,
    );
    assert_eq!(out, "(1 2 3)");
}

#[test]
fn unknown_advice_kind_signals() {
    reset_host();
    let err = eval_str(r#"(intercept 'sideways "x" nil)"#).unwrap_err();
    assert!(err.contains("unknown advice kind"), "got {err}");
}
