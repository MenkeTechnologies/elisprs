//! `end-of-file` carries the file being loaded, when there is one.
//!
//! Emacs's `end_of_file_error` (lread.c) is
//!
//! ```c
//! if (STRINGP (Vload_true_file_name))
//!   xsignal1 (Qend_of_file, Vload_true_file_name);
//! xsignal0 (Qend_of_file);
//! ```
//!
//! so the same truncated `read` reports `(end-of-file "/path/to/file.el")` from
//! inside a loaded file and a bare `(end-of-file)` from `--eval`. elisprs signals
//! `end-of-file` from a dozen places in the reader and dropped its data
//! unconditionally, so the loaded case lost the file name.
//!
//! This is a process-level test because the datum comes from
//! `load-true-file-name`, which is only bound while a file's forms run.
//! Measured against `emacs -Q --batch -l` / `emacs -Q --batch --eval` on
//! GNU Emacs 30.2.

use std::process::Command;

/// `elisp SCRIPT`, with an isolated HOME so the run never touches the
/// developer's cache shard.
fn run_script(tag: &str, script: &str) -> (String, String) {
    let exe = env!("CARGO_BIN_EXE_elisp");
    let dir = std::env::temp_dir().join(format!("elisprs-eof-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("script.el");
    std::fs::write(&path, script).expect("write script");
    let out = Command::new(exe)
        .arg(&path)
        .env("HOME", &dir)
        .output()
        .expect("run elisp");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    (stdout, path.to_string_lossy().into_owned())
}

/// Inside a loaded file the datum is the file. Both the `read-from-string` and
/// the `read` spellings go through the same reader, so both must carry it.
#[test]
fn end_of_file_inside_a_load_names_the_file() {
    let script = r#"
(prin1 (condition-case e (read-from-string "(1 2") (error e)))
(terpri)
(prin1 (condition-case e (read "(1") (error e)))
(terpri)
"#;
    let (stdout, path) = run_script("in-load", script);
    assert_eq!(
        stdout,
        format!("(end-of-file \"{path}\")\n(end-of-file \"{path}\")\n")
    );
}

/// With no file being loaded the data list stays empty — the fix must not
/// invent a datum where Emacs has none.
#[test]
fn end_of_file_outside_a_load_has_no_data() {
    let exe = env!("CARGO_BIN_EXE_elisp");
    let dir = std::env::temp_dir().join(format!("elisprs-eof-eval-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = Command::new(exe)
        .args([
            "-e",
            r#"(prin1 (condition-case e (read-from-string "(1 2") (error e)))"#,
        ])
        .env("HOME", &dir)
        .output()
        .expect("run elisp");
    let _ = std::fs::remove_dir_all(&dir);
    // `-e` echoes the form's value after whatever the form printed, so the datum
    // is checked by shape rather than by whole-output equality: an empty data
    // list has no string in it at all.
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.starts_with("(end-of-file)"), "{stdout}");
    assert!(!stdout.contains('"'), "no datum expected, got {stdout}");
}

/// The other data-less conditions must keep their empty data: the new branch is
/// for `end-of-file` alone, and `arith-error` is the neighbour it sits next to
/// in the same table.
#[test]
fn other_dataless_conditions_are_untouched_inside_a_load() {
    let script = r#"
(prin1 (condition-case e (/ 1 0) (error e)))
(terpri)
"#;
    let (stdout, _) = run_script("arith", script);
    assert_eq!(stdout, "(arith-error)\n");
}
