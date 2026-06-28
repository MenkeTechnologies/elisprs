//! Example-script regression gate — runs in CI via `cargo test`.
//!
//! Every `examples/*.el` is a self-testing script: it exercises a slice of the
//! language, checks the results with a tiny `expect` helper, and raises an elisp
//! `error` (→ non-zero exit) the moment a result drifts. This harness runs each
//! script through the built `elisp` binary and requires it to exit successfully,
//! so a regression in any lowered feature fails the corresponding example, which
//! fails this test.
//!
//! The binary path comes from `CARGO_BIN_EXE_elisp`, which Cargo sets for
//! integration tests — so the build exercised here is always the current one.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Sorted list of `examples/*.el` scripts.
fn example_scripts(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .expect("examples/ dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "el"))
        .collect();
    v.sort();
    v
}

#[test]
fn examples_self_tests_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bin = env!("CARGO_BIN_EXE_elisp");
    let scripts = example_scripts(&root.join("examples"));
    assert!(!scripts.is_empty(), "no examples/*.el scripts found");

    let mut failures: Vec<String> = Vec::new();
    for script in &scripts {
        let stem = script.file_stem().unwrap().to_str().unwrap();
        let out = Command::new(bin).arg(script).output().expect("spawn elisp");
        if !out.status.success() {
            failures.push(format!(
                "{stem}: exited {:?}\n--- stdout ---\n{}--- stderr ---\n{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "example self-test failures:\n\n{}",
        failures.join("\n\n")
    );
}
