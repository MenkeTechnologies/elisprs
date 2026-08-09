//! An uncaught elisp error must not vanish on the AOT path.
//!
//! An elisp error never becomes a fusevm `VMResult::Error`. `host::abort` parks
//! the message in the thread-local host error slot and winds `vm.ip` past the
//! last op, so the VM terminates the way a program that simply ran off the end
//! does. The interpreted driver copes because `host::run_chunk` reads that slot
//! right after `vm.run()`; fusevm's AOT driver (`fusevm_aot_run_embedded`) lives
//! in fusevm, cannot know about the slot, and mapped the clean termination to
//! exit 0.
//!
//! So a standalone AOT binary swallowed every uncaught error. Verified against
//! ground truth on the pre-fix binary (GNU Emacs 30.2):
//!
//! ```text
//! $ cat r.el
//! (error "boom")
//! $ emacs --batch -l r.el ; echo "exit=$?"
//! Error: error ("boom")            # ... plus a backtrace, on stderr
//! exit=255
//! $ ./elisp r.el ; echo "exit=$?"
//! error: boom
//! exit=1
//! $ ./r.bin ; echo "exit=$?"       # the same source, --aot-exe
//! exit=0                           # nothing on stdout, nothing on stderr
//! ```
//!
//! Exit 0 with empty stdout and empty stderr is byte-identical to a successful
//! run of a program that prints nothing, which makes the failure undetectable by
//! any caller — a worse outcome than the error itself.
//!
//! Building a real AOT executable needs `cc` and the platform frameworks, which
//! a unit test should not require. These tests instead drive the exact pair the
//! generated `main` drives — set the VM up through `fusevm_aot_register_builtins`
//! alone, run it, then hand the result to `elisprs_aot_finish` — so they
//! exercise the real hook rather than a re-implementation of it.

use elisprs::aot_runtime::{elisprs_aot_finish, fusevm_aot_register_builtins};
use fusevm::{VMResult, VM};

/// Run `src` exactly as the AOT object does: compile, configure the VM through
/// the AOT registration hook alone (never `host::run_chunk`, which is the
/// interpreter path), run it, and report how fusevm saw the termination.
fn run_via_aot_hook(src: &str) -> VMResult {
    elisprs::reset_host();
    let chunk = elisprs::compile_str(src).expect("compile");
    let mut vm = VM::new(chunk);
    unsafe { fusevm_aot_register_builtins(&mut vm) };
    vm.run()
}

/// The mechanism of the silence: an uncaught elisp error leaves fusevm with a
/// perfectly clean termination, so no amount of inspecting the `VMResult` can
/// reveal it. This is the fact that made the bug invisible, so it is pinned
/// separately from the fix — if fusevm ever starts surfacing the error itself,
/// this test says so.
#[test]
fn uncaught_error_leaves_fusevm_result_clean() {
    let outcome = run_via_aot_hook(r#"(error "boom")"#);
    assert!(
        !matches!(outcome, VMResult::Error(_)),
        "expected the elisp error to be invisible to fusevm, got {outcome:?}"
    );
    // The message is in the host slot the AOT driver never reads. It is stored
    // in the internal `CONDITION: data` form; `format_error` is what renders it
    // as Emacs's `error-message-string` would.
    let err = elisprs::with_host(|h| h.take_error());
    assert_eq!(err.as_deref(), Some("error: boom"));
}

/// The fix: `elisprs_aot_finish` turns that clean termination into a non-zero
/// exit. Ground truth is the interpreted driver, which exits 1 on the same
/// source (`main.rs`), so both execution paths agree.
#[test]
fn finish_reports_uncaught_error_as_failure() {
    for src in [
        r#"(error "boom")"#,
        "(princ (undefined-function-xyz 1))",
        "(princ (/ 1 0))",
        // Output before the error must not make the failure look like success:
        // the process prints `before`, then still has to exit non-zero.
        r#"(princ "before")(error "late")"#,
    ] {
        run_via_aot_hook(src);
        assert_eq!(
            elisprs_aot_finish(0),
            1,
            "uncaught error in `{src}` must exit non-zero, not 0"
        );
    }
}

/// A successful program keeps fusevm's exit code untouched — the check must not
/// invent a failure, and must not clobber a real status.
#[test]
fn finish_passes_success_through() {
    for src in ["(princ (+ 1 2))", "(princ (car '(10 20)))", "nil"] {
        run_via_aot_hook(src);
        assert_eq!(elisprs_aot_finish(0), 0, "`{src}` must exit 0");
    }
    run_via_aot_hook("(princ 1)");
    assert_eq!(
        elisprs_aot_finish(7),
        7,
        "a non-zero VM result must survive"
    );
}

/// A *handled* error must not trip the check. `condition-case`/`ignore-errors`
/// consume the error through the nested `run_chunk`, so the slot is empty by the
/// time the program ends — this is the false-positive the fix could plausibly
/// have introduced, so it is pinned. Ground truth (`emacs --batch`): every one
/// of these prints its value and exits 0.
#[test]
fn finish_ignores_handled_errors() {
    for src in [
        r#"(princ (condition-case e (error "boom") (error "caught")))"#,
        r#"(princ (or (ignore-errors (/ 1 0)) "recovered"))"#,
        r#"(defun g () (error "deep"))(princ (condition-case nil (g) (error "handled")))"#,
        "(princ (catch 'tag (throw 'tag 42)))",
        // A loop that signals and recovers on every iteration: the slot must be
        // clean at the end, not merely clean on the first pass.
        r#"(dotimes (i 3) (princ (condition-case nil (error "e") (error i))))"#,
    ] {
        run_via_aot_hook(src);
        assert_eq!(
            elisprs_aot_finish(0),
            0,
            "handled error in `{src}` must not be reported as a failure"
        );
    }
}
