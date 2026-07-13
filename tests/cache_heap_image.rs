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
