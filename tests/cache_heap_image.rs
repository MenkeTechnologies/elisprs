//! The rkyv script cache must restore a heap image that behaves like the heap it
//! replaced.
//!
//! A cache hit skips the prelude and re-imports a serialized heap instead. The
//! image records symbols by name, and it used to re-intern *every* one of them
//! into the global obarray on import — including the uninterned ones (a lambda
//! parameter, a `let` binding inside a macro). The prelude binds a local named
//! `exp`, so a warm cache rebound the global `exp` to that copy, which has no
//! function cell:
//!
//! ```text
//! $ elisp script.el     # cold: 0.36787944117144233
//! $ elisp script.el     # warm: Symbol's function definition is void: exp
//! ```
//!
//! Only a symbol the obarray actually maps to *itself* may claim its name back on
//! import (`SerObj::Symbol::interned`). The bug was invisible to a chunk that
//! baked in the builtin's handle at compile time and only bit code that resolved
//! the name at *runtime* — `(eval (read "..."))`, `intern`, `symbol-function` on a
//! read symbol — which is why the fuzz harness found it and the unit tests did not.

use std::process::Command;

/// Run the built `elisp` binary on a script, twice: once cold, once warm.
/// Returns `(cold_stdout, warm_stdout)`.
fn run_cold_then_warm(tag: &str, script: &str) -> (String, String) {
    let exe = env!("CARGO_BIN_EXE_elisp");
    // Per-test directory: the tests run in parallel and each needs its own HOME
    // (and therefore its own cache shard).
    let dir = std::env::temp_dir().join(format!("elisprs-cache-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("script.el");
    std::fs::write(&path, script).expect("write script");

    // Isolate HOME so the test uses its own `~/.elisprs/scripts.rkyv` and never
    // reads or clobbers the developer's cache.
    let run = || -> String {
        let out = Command::new(exe)
            .arg(&path)
            .env("HOME", &dir)
            .output()
            .expect("run elisp");
        assert!(
            out.status.success(),
            "elisp failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let cold = run();
    let warm = run();
    let _ = std::fs::remove_dir_all(&dir);
    (cold, warm)
}

/// A builtin whose name the prelude also uses as a local variable must still be
/// the global function binding after a cache hit.
#[test]
fn warm_cache_does_not_shadow_a_builtin_with_an_uninterned_prelude_local() {
    // `exp` is the case that broke: the prelude binds a local of that name.
    // Resolving it through the *reader* is what exposes the shadowing — a
    // compiled `(exp -1.0)` bakes in the handle and would pass either way.
    let script = r#"
(princ (format "%S %S %S\n"
               (eval (car (read-from-string "(exp -1.0)")) t)
               (eq (car (read-from-string "exp")) 'exp)
               (fboundp (car (read-from-string "exp")))))
"#;
    let (cold, warm) = run_cold_then_warm("shadow", script);
    assert_eq!(cold, "0.36787944117144233 t t\n", "cold run");
    assert_eq!(
        warm, cold,
        "a warm cache hit must behave exactly like a cold run"
    );
}

/// The heap image round-trips the *values* the prelude defined, too — the
/// interning fix must not drop the symbols that legitimately own their names.
#[test]
fn warm_cache_preserves_prelude_definitions() {
    let script = r#"
(princ (format "%S %S %S\n"
               (funcall (car (read-from-string "seq-uniq")) (list 1 1 2))
               (eval (car (read-from-string "(cl-evenp 4)")) t)
               (eval (car (read-from-string "most-positive-fixnum")) t)))
"#;
    let (cold, warm) = run_cold_then_warm("prelude", script);
    assert_eq!(cold, "(1 2) t 2305843009213693951\n", "cold run");
    assert_eq!(warm, cold, "warm cache diverged from cold");
}

/// A cache hit replays the file's chunks onto the restored image, so the image has
/// to be the heap as it stood BEFORE the file ran. Exporting the *post-run* heap
/// double-applied every effect the file had:
///
///   - `make-variable-buffer-local` left `buffer_local_auto` set, so replaying the
///     file's own `(defvar bl-y nil)` created a buffer-local binding the cold run
///     never had, and `local-variable-p` answered t instead of nil;
///   - a prelude object the file mutated came back already mutated — the
///     symbol-plist table returned the previous run's entries and the replay
///     appended to them again.
#[test]
fn warm_cache_does_not_double_apply_the_files_own_effects() {
    let script = r#"
(defvar bl-y nil)
(make-variable-buffer-local 'bl-y)
(put 'pg 'custom-group '(one))
(princ (format "%S %S %S\n"
               (local-variable-p 'bl-y)
               (local-variable-if-set-p 'bl-y)
               (get 'pg 'custom-group)))
"#;
    let (cold, warm) = run_cold_then_warm("effects", script);
    assert_eq!(cold, "nil t (one)\n", "cold run");
    assert_eq!(warm, cold, "a warm cache re-applied the file's own effects");
}

/// The OClosure side table and a closure's captured environment are built when the
/// PRELUDE runs — which a cache hit skips — so both must ride in the image. Without
/// the table every prelude OClosure came back a plain closure
/// (`oclosure--copy: "not an OClosure"`); without the captured env its accessors
/// signalled `void-variable index`.
#[test]
fn warm_cache_restores_oclosures_and_captured_environments() {
    let script = r#"
(oclosure-define oc-pt x y)
(let ((o (oclosure-lambda (oc-pt (x 3) (y 4)) () (+ x y))))
  (princ (format "%S %S %S\n" (funcall o) (oc-pt--x o) (oclosure-type o))))
"#;
    let (cold, warm) = run_cold_then_warm("oclosure", script);
    assert_eq!(cold, "7 3 oc-pt\n", "cold run");
    assert_eq!(
        warm, cold,
        "warm cache lost the OClosure metadata or its captures"
    );
}

/// A closure's *printable source* — its arglist as written and its body forms —
/// has to survive the image too. `SerObj::Closure` carried the compiled body
/// chunk, the params and the captured env, but not `ClosureSrc`, and the
/// importer rebuilt every closure with `ClosureSrc::default()`. The closure
/// still ran, so nothing errored; it just printed as `#[nil () ...]`:
///
/// ```text
/// $ emacs --batch -l script.el     # ground truth (GNU Emacs 30.2)
/// #[(x) ((* x 2)) (t)]
/// $ elisp script.el                # cold
/// #[(x) ((* x 2)) (t)]
/// $ elisp script.el                # warm — source gone
/// #[nil () (t)]
/// ```
///
/// A silent wrong answer on the *default* path from the second run onward, and
/// identically on `--aot-exe` (same importer). Emacs prints the arglist and body
/// for an interpreted closure, so the cold run is the correct answer and the warm
/// run must equal it.
#[test]
fn warm_cache_preserves_closure_printed_source() {
    let script = r#";;; -*- lexical-binding: t -*-
(princ (prin1-to-string (lambda (x) (* x 2))))
(terpri)
(let ((n 5)) (princ (prin1-to-string (lambda () n))))
(terpri)
(defun keeps-src (a &optional b) (list a b))
(princ (prin1-to-string (symbol-function 'keeps-src)))
(terpri)
"#;
    let (cold, warm) = run_cold_then_warm("closure-src", script);
    // Byte-for-byte what `emacs --batch -l` prints for this script.
    assert_eq!(
        cold, "#[(x) ((* x 2)) (t)]\n#[nil (n) ((n . 5))]\n#[(a &optional b) ((list a b)) (t)]\n",
        "cold run must match Emacs"
    );
    assert_eq!(
        warm, cold,
        "warm cache lost the closures' printed source (arglist/body)"
    );
}

/// The initial buffer's syntax table is buffer-local, so the heap image does not
/// carry it — and a cache hit, which skips the prelude, used to leave the buffer
/// on `standard-syntax-table`. Every syntax-derived observable followed:
///
/// ```text
/// $ elisp probe.el   # cold: (char-syntax ?.) => 95   (emacs-lisp-mode table)
/// $ elisp probe.el   # warm: (char-syntax ?.) => 46   (standard table)
/// ```
///
/// 95 is what `emacs -Q --batch -l FILE` answers, because it evaluates the file
/// in `*scratch*` under `lisp-interaction-mode`. `scan-sexps` is in the probe
/// because it is the family that made the drift load-bearing: `.` being
/// punctuation instead of a symbol constituent ends a sexp early.
#[test]
fn warm_cache_keeps_the_initial_buffers_syntax_table() {
    let script = r#"
(insert "foo.bar ;c")
(princ (format "%s %s %s %s\n"
               (char-syntax ?.) (char-syntax ?\;)
               (scan-sexps 1 1) (eq (syntax-table) (standard-syntax-table))))
"#;
    let (cold, warm) = run_cold_then_warm("syntax-table", script);
    assert_eq!(cold, "95 60 8 nil\n", "cold run");
    assert_eq!(warm, cold, "a warm cache must not change the syntax table");
}

/// Round 19 baked *subr values* into the prelude's chunks: inside the prelude, a
/// call to one of the primitives Emacs's byte compiler open-codes loads the subr
/// itself rather than the symbol, so `advice-add` on that symbol cannot reach the
/// advice machinery's own internals. Those constants ride the heap image, so the
/// behaviour has to survive a cache hit — a warm run skips the prelude entirely
/// and never re-lowers a single one of those calls.
///
/// `cdr` and `nth` are in the probe as the negative control: advice on `car` must
/// not reach them, cold or warm. Matches `emacs -Q --batch -l` exactly.
#[test]
fn warm_cache_keeps_advice_working_on_an_open_coded_subr() {
    let script = r#"
(advice-add 'car :filter-return #'1+)
(princ (format "%S %S %S\n" (car '(1 2)) (cdr '(1 2)) (nth 0 '(5 6))))
(advice-remove 'car #'1+)
(princ (format "%S\n" (car '(1 2))))
"#;
    let (cold, warm) = run_cold_then_warm("advice-subr", script);
    assert_eq!(cold, "2 (2) 5\n1\n", "cold run");
    assert_eq!(warm, cold, "a warm cache must not change advice on a subr");
}

/// ERT's own two round-19 behaviours cross the same boundary: `ert-run-tests-batch`
/// is a prelude `defun`, so a cache hit restores it from the image instead of
/// re-compiling it. Tests must still run in *name* order (`aaa-a` before `zzz-b`)
/// and each body inside `with-temp-buffer` — where `(char-syntax ?.)` is 46,
/// against 95 at top level in `*scratch*`.
///
/// Measured identical under `emacs -Q --batch -l`.
#[test]
fn warm_cache_keeps_ert_ordering_and_its_temp_buffer() {
    let script = r#"
(princ (format "top %S %S\n" (buffer-name) (char-syntax ?.)))
(ert-deftest zzz-b () (princ (format "zzz-b %S %S\n" (buffer-name) (char-syntax ?.))))
(ert-deftest aaa-a () (princ "aaa-a\n"))
(ert-run-tests-batch)
"#;
    let (cold, warm) = run_cold_then_warm("ert-order", script);
    assert_eq!(
        cold, "top \"*scratch*\" 95\naaa-a\nzzz-b \" *temp*\" 46\n",
        "cold run"
    );
    assert_eq!(
        warm, cold,
        "a warm cache must not change ERT's ordering or buffer"
    );
}

/// The introspection function cells of the compiler intrinsics live in a side
/// table on the host, not in the arena — so the heap image alone does not carry
/// them, and a cache hit (which skips the prelude that registers them) used to
/// come back with none:
///
/// ```text
/// $ elisp script.el     # cold: (t t (macro . #[(cond &rest body) …]))
/// $ elisp script.el     # warm: (nil t nil)
/// ```
///
/// `Entry::introspection_cells` (shard format v8) now carries it, so `fboundp`
/// and `symbol-function` answer the same on both runs. `macrop` was already
/// right on both — it reads a name table, not the cell — which is why the split
/// showed up as an *inconsistency* between two answers about the same symbol.
///
/// The special forms are covered too, from the other direction: their cells are
/// installed by `builtins::install`, which runs on every startup, so they must be
/// present on a warm run without the image having to carry them.
#[test]
fn warm_cache_keeps_the_introspection_function_cells() {
    let script = r#"
(princ (format "%S %S %S %S %S\n"
               (fboundp 'when) (macrop 'when) (consp (symbol-function 'when))
               (fboundp 'if) (subr-name (symbol-function 'if))))
"#;
    let (cold, warm) = run_cold_then_warm("introspection-cells", script);
    assert_eq!(cold, "t t t t \"if\"\n", "cold run");
    assert_eq!(
        warm, cold,
        "a warm cache must not lose the intrinsic-macro / special-form cells"
    );
}
