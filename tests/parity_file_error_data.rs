//! File operations signal fileio.c's `report_file_errno` shape,
//! `(CONDITION STRING ERRSTRING NAME...)`, with every NAME expanded against
//! `default-directory` the way each C function's `Fexpand_file_name` does.
//!
//! elisprs signalled one flattened message string —
//! `(file-missing "Opening input file: No such file: x")` — as plain
//! `file-error` for most operations, and resolved relative names against the
//! process cwd. Expectations are GNU Emacs 31.1 (`emacs -Q --batch`); the
//! temp-directory rows build their directories from Rust so the test needs
//! nothing from the machine but a writable temp dir.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

fn in_missing_dir(form: &str) -> String {
    format!("(let ((default-directory \"/nonexistent-dd/\")) (condition-case e {form} (error e)))")
}

#[test]
fn errors_carry_the_operation_strerror_and_expanded_names() {
    let rows = [
        (
            "(insert-file-contents \"x\")",
            "(file-missing \"Opening input file\" \"No such file or directory\" \"/nonexistent-dd/x\")",
        ),
        (
            "(write-region \"a\" nil \"sub/../y\")",
            "(file-missing \"Opening output file\" \"No such file or directory\" \"/nonexistent-dd/y\")",
        ),
        (
            "(directory-files \"d\")",
            "(file-missing \"Opening directory\" \"No such file or directory\" \"/nonexistent-dd/d\")",
        ),
        (
            "(rename-file \"a\" \"b\")",
            "(file-missing \"Renaming\" \"No such file or directory\" \"/nonexistent-dd/a\" \"/nonexistent-dd/b\")",
        ),
        (
            "(copy-file \"a\" \"b\")",
            "(file-missing \"Opening input file\" \"No such file or directory\" \"/nonexistent-dd/a\")",
        ),
        (
            "(make-directory \"a\")",
            "(file-missing \"Creating directory\" \"No such file or directory\" \"/nonexistent-dd/a\")",
        ),
        // `delete-file` on a file that is already gone is not an error.
        ("(delete-file \"a\")", "nil"),
    ];
    for (form, want) in rows {
        check(&in_missing_dir(form), want);
    }
}

#[test]
fn load_and_program_search_keep_the_name_as_given() {
    check(
        "(condition-case e (load \"/nonexistent-dd/zz\") (error e))",
        "(file-missing \"Cannot open load file\" \"No such file or directory\" \"/nonexistent-dd/zz\")",
    );
    for f in ["call-process", "process-lines"] {
        check(
            &format!("(condition-case e ({f} \"nonexistent-zz-prog\") (error e))"),
            "(file-missing \"Searching for program\" \"No such file or directory\" \"nonexistent-zz-prog\")",
        );
    }
}

fn temp_dir(tag: &str) -> String {
    let d = std::env::temp_dir().join(format!("elisprs-fe-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

/// files.el `make-directory`: with PARENTS an existing directory answers t and
/// missing parents are created; without, EEXIST is `file-already-exists` with
/// no operation string.
#[test]
fn make_directory_is_files_el() {
    let d = temp_dir("mkdir");
    check(
        &format!(
            "(let ((d {d:?})) (list (make-directory d t) \
             (condition-case e (make-directory d) (error (list (car e) (cadr e) (equal (caddr e) d)))) \
             (make-directory (concat d \"/x/y\") t) (file-directory-p (concat d \"/x/y\"))))"
        ),
        "(t (file-already-exists \"File exists\" t) nil t)",
    );
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn copy_onto_an_existing_file_and_reading_a_directory() {
    let d = temp_dir("copy");
    let f = format!("{d}/f");
    std::fs::write(&f, "hi").unwrap();
    check(
        &format!(
            "(list (condition-case e (copy-file {f:?} {f:?}) (error (list (car e) (cadr e) (equal (caddr e) {f:?})))) \
             (condition-case e (with-temp-buffer (insert-file-contents {d:?})) (error (list (car e) (cadr e) (caddr e)))))"
        ),
        "((file-already-exists \"File already exists\" t) (file-error \"Read error\" \"Is a directory\"))",
    );
    std::fs::remove_dir_all(&d).unwrap();
}

/// Emacs 31's `signal` takes a whole error object when DATA is omitted.
#[test]
fn signal_accepts_a_whole_error_object() {
    check(
        "(list (func-arity 'signal) (condition-case e (signal 'error) (error e)) \
         (condition-case e (signal '(foo . 1)) (error e)) \
         (condition-case e (signal '(error \"x\") '(1)) (error e)) \
         (condition-case e (signal '(error \"x\")) (error e)))",
        "((1 . 2) (error) (error \"Invalid error symbol\" foo) (wrong-type-argument symbolp (error \"x\")) (error \"x\"))",
    );
}
