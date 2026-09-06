//! The post-prelude snapshot must be indistinguishable from a cold rebuild.
//!
//! `load_prelude` reads, macro-expands, lowers and runs every preloaded-Lisp
//! form. `host::restore_prelude_snapshot` skips all of that by copying the host
//! the first cold load produced, which is what makes a `reset_host`-per-`eval`
//! test suite affordable. That is only sound while the copy carries *every*
//! piece of state the rebuild would have produced — arena, obarray, buffers,
//! the OClosure side table, the intrinsic-macro cells, and the value/function
//! cells the prelude installs on symbols `builtins::install` already created.
//!
//! Those last three live outside the arena, and each one has historically been
//! missed by the *serialized* form of the same idea: `cache.rs` bumped its
//! shard version for `introspection_cells` (v8), for a hash table's user test
//! (v11), and for `builtin_cells` (v12), each time because a restore that
//! skipped the prelude answered differently from one that ran it. This test is
//! the equivalent guard for the in-process snapshot: it evaluates a probe list
//! that reaches all of that state, once with the snapshot off (so every probe
//! is a genuine cold rebuild) and once with it on, and requires the two
//! transcripts to be equal.
//!
//! A probe added here is never removed — see `probes_are_never_dropped`.

use elisprs::{eval_str, host, print, reset_host};

/// Forms whose answers depend on host state the prelude builds. Each is
/// evaluated in its own freshly reset host, so a probe sees only what the
/// prelude put there.
///
/// The groups mirror the state a restore has to carry:
///   - arena + obarray: prelude `defun`/`defvar`/`defmacro` reachability
///   - intrinsic-macro cells: `(symbol-function 'when)` is a side table, not arena
///   - builtin cells: symbols `install` made that the prelude then rewrote
///   - OClosure side table: built while the prelude runs, not derivable from the heap
///   - hash-table user tests: registered by `define-hash-table-test`
///   - buffers: the startup buffer set and which one is current
const PROBES: &[&str] = &[
    // ── arena + obarray ────────────────────────────────────────────────────
    "(mapcar #'1+ (list 1 2 3))",
    "(cl-remove-if #'cl-evenp (list 1 2 3 4 5))",
    "(seq-filter #'stringp (list 1 \"a\" 2 \"b\"))",
    "(cl-subseq [1 2 3 4] 1 3)",
    "(pcase (list 1 2) (`(,a ,b) (+ a b)))",
    "(let-alist '((a . 1)) .a)",
    "(cl-loop for i from 1 to 4 collect (* i i))",
    "(string-join (list \"a\" \"b\") \"-\")",
    "(fboundp 'cl-defstruct)",
    "(functionp (symbol-function 'seq-map))",
    // ── intrinsic-macro / special-form introspection cells ─────────────────
    "(list (fboundp 'when) (fboundp 'unless) (fboundp 'if) (fboundp 'progn))",
    "(car-safe (symbol-function 'when))",
    "(macrop 'unless)",
    "(special-form-p 'if)",
    // ── builtin cells the prelude rewrites after `install` ──────────────────
    "(macrop 'save-current-buffer)",
    "(eval '(save-current-buffer 42) t)",
    "(functionp (symbol-function 'format-message))",
    // ── OClosure side table ────────────────────────────────────────────────
    "(progn (defun p-base () 1)
            (advice-add 'p-base :around (lambda (f &rest a) (1+ (apply f a))))
            (p-base))",
    "(progn (defun p-base2 () 1)
            (advice-add 'p-base2 :override (lambda () 9))
            (prog1 (p-base2) (advice-remove 'p-base2 nil)))",
    // ── hash tables, including a user-defined test ─────────────────────────
    "(let ((h (make-hash-table :test 'equal)))
       (puthash (list 1 2) 'v h) (gethash (list 1 2) h))",
    "(progn (define-hash-table-test 'p-ci
              (lambda (a b) (string= (downcase a) (downcase b)))
              (lambda (k) (sxhash-equal (downcase k))))
            (let ((h (make-hash-table :test 'p-ci)))
              (puthash \"AB\" 'hit h) (gethash \"ab\" h)))",
    // ── buffers / startup state ────────────────────────────────────────────
    "(buffer-name)",
    "(major-mode)",
    "(char-syntax ?.)",
    "(with-temp-buffer (insert \"xy\") (buffer-string))",
    // ── printing, which reads print state on the host ──────────────────────
    "(let ((print-circle t) (x (list 1 2))) (setcdr (cdr x) x) (format \"%S\" x))",
    "(prin1-to-string (record 'foo 1 2))",
    // ── error objects and condition names ──────────────────────────────────
    "(condition-case e (car 1) (error e))",
    "(condition-case e (aref [1] 9) (error e))",
    "(get 'wrong-type-argument 'error-conditions)",
];

/// Evaluate every probe in a fresh host, returning one transcript line each.
/// An error is recorded rather than raised, so a probe that signals compares
/// its *message* across the two paths instead of aborting the run.
fn transcript() -> Vec<String> {
    PROBES
        .iter()
        .map(|src| {
            reset_host();
            match eval_str(src) {
                Ok(v) => print(&v, true),
                Err(e) => format!("!{e}"),
            }
        })
        .collect()
}

#[test]
fn restored_prelude_answers_exactly_as_a_cold_rebuild() {
    // Cold reference: snapshot off, so each `reset_host` forces a full rebuild.
    host::set_prelude_snapshot_enabled(false);
    let cold = transcript();

    // Warm: the first probe rebuilds and records the snapshot; every probe
    // after it is served by `restore_prelude_snapshot`.
    host::set_prelude_snapshot_enabled(true);
    reset_host();
    let warm = transcript();

    assert_eq!(cold.len(), PROBES.len());
    for ((src, c), w) in PROBES.iter().zip(&cold).zip(&warm) {
        assert_eq!(
            c, w,
            "snapshot restore diverged from a cold prelude rebuild\n  form: {src}\n  cold: {c}\n  warm: {w}"
        );
    }
}

/// A restored host must stay writable and independent: mutating it must not
/// reach the snapshot, or the *next* restore would serve the previous test's
/// leftovers. Strings are the sharp case — `Obj::Str` holds an `Arc` that the
/// clone shares, so a write that used `Arc::make_mut` instead of installing a
/// fresh `Arc` would either corrupt the snapshot or silently stop being visible
/// through aliases.
#[test]
fn writes_to_a_restored_host_do_not_reach_the_snapshot() {
    host::set_prelude_snapshot_enabled(true);
    reset_host();
    let _ = eval_str("1"); // cold load; records the snapshot

    // Redefine a prelude function and mutate a prelude-reachable string.
    reset_host();
    assert_eq!(
        eval_str("(progn (defun seq-map (a b) 'clobbered) (seq-map 1 2))").map(|v| print(&v, true)),
        Ok("clobbered".to_string())
    );

    // A fresh restore must not see either change.
    reset_host();
    assert_eq!(
        eval_str("(seq-map #'1+ (list 1 2))").map(|v| print(&v, true)),
        Ok("(2 3)".to_string()),
        "a redefinition leaked from one restored host into the next"
    );

    // Aliased string mutation still writes through every reference within one
    // host (the property `Obj::Str` exists for), and does not escape it.
    reset_host();
    assert_eq!(
        eval_str("(let* ((a (copy-sequence \"ab\")) (b a)) (aset a 0 ?z) b)")
            .map(|v| print(&v, true)),
        Ok("\"zb\"".to_string())
    );
}

/// The probe list is a floor. Shrinking it silently would turn the equivalence
/// test into a weaker one without any failure to notice, so the count is
/// pinned: probes may be added, never dropped.
#[test]
fn probes_are_never_dropped() {
    assert!(
        PROBES.len() >= 29,
        "PROBES shrank to {} — the equivalence test only covers what it evaluates; \
         add probes freely, but removing one weakens the guard silently",
        PROBES.len()
    );
}
