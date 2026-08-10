//! Example-script regression gate — runs in CI via `cargo test`.
//!
//! Every `examples/*.el` is a self-testing script: it exercises a slice of the
//! language with `ert-deftest` and ends in `(ert-run-tests-batch-and-exit)`,
//! which raises an elisp `error` (→ non-zero exit) the moment a result drifts.
//! This harness runs each script through the built `elisp` binary.
//!
//! The binary path comes from `CARGO_BIN_EXE_elisp`, which Cargo sets for
//! integration tests — so the build exercised here is always the current one.
//!
//! # What this harness could not see
//!
//! Checking only the exit status leaves a hole that exit status cannot cover:
//! `ert-run-tests-batch-and-exit` exits 0 when it ran *no tests at all*. A
//! renamed `ert-deftest`, a form accidentally moved inside a `when` that is
//! false, a file whose body stopped being read — every one of those drops the
//! example's coverage to zero and still passes. The same applies to a skip:
//! `Ran 5 tests: 0 unexpected, 5 skipped.` is a green run that tested nothing.
//!
//! So the summary line is parsed too, and the count is required to equal the
//! number of top-level `ert-deftest` forms in the file. That is the assertion
//! that ties the file's contents to what actually executed; it costs nothing at
//! runtime, because the output was already being captured for the failure
//! message.
//!
//! Still not covered, and deliberately not faked: this compares no output
//! against Emacs. The examples are self-checking, so a wrong value fails them
//! from the inside, but a wrong value that both the code and its own `should`
//! agree on is invisible here. That is the fuzzer's and the parity suites' job.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `(ert-deftest NAME …)` forms at top level — the number of tests the file
/// says it defines.
fn declared_tests(src: &str) -> usize {
    src.lines()
        .filter(|l| l.starts_with("(ert-deftest "))
        .count()
}

/// The `Ran N tests: U unexpected, S skipped.` line ERT prints last.
fn ert_summary(stderr: &str) -> Option<(usize, usize, usize)> {
    let line = stderr.lines().rev().find(|l| l.starts_with("Ran "))?;
    let nums: Vec<usize> = line
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    match nums[..] {
        [ran, unexpected, skipped] => Some((ran, unexpected, skipped)),
        _ => None,
    }
}

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
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let detail = || {
            format!(
                "\n--- stdout ---\n{}--- stderr ---\n{stderr}",
                String::from_utf8_lossy(&out.stdout)
            )
        };
        if !out.status.success() {
            failures.push(format!(
                "{stem}: exited {:?}{}",
                out.status.code(),
                detail()
            ));
            continue;
        }
        let declared = declared_tests(&fs::read_to_string(script).expect("read example"));
        match ert_summary(&stderr) {
            None => failures.push(format!(
                "{stem}: exited 0 but printed no `Ran N tests:' summary — \
                 the script never reached `ert-run-tests-batch-and-exit'{}",
                detail()
            )),
            Some((ran, unexpected, skipped)) => {
                if ran != declared {
                    failures.push(format!(
                        "{stem}: ran {ran} tests but the file defines {declared} \
                         `ert-deftest' forms — a test stopped being registered{}",
                        detail()
                    ));
                } else if unexpected != 0 {
                    failures.push(format!(
                        "{stem}: {unexpected} unexpected result(s){}",
                        detail()
                    ));
                } else if ran == skipped {
                    failures.push(format!(
                        "{stem}: every one of its {ran} tests was skipped, so it \
                         exercised nothing{}",
                        detail()
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "example self-test failures:\n\n{}",
        failures.join("\n\n")
    );
}
