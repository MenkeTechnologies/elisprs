//! callproc.c `call_process`: INFILE, every DESTINATION form, the order of
//! its checks, a child killed by a signal; and `shell-command-to-string`
//! running `shell-file-name` with `shell-command-switch`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};
use std::path::{Path, PathBuf};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// A fresh directory holding `in.txt` ("line1\nline2\n").
fn fixture(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("elisprs-callproc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let base = base.canonicalize().unwrap();
    std::fs::write(base.join("in.txt"), "line1\nline2\n").unwrap();
    base
}

/// FORM run in a temporary buffer holding "<>" with point between the two,
/// `default-directory` the fixture D: `(VALUE BUFFER-STRING POINT)`, with D
/// replaced by "D".
fn in_buffer(d: &Path, form: &str) -> String {
    let d = d.to_string_lossy();
    eval(&format!(
        "(let ((default-directory \"{d}/\")) (with-temp-buffer (insert \"<>\") (goto-char 2) (let ((r {form})) (list r (buffer-string) (point)))))"
    ))
    .replace(&*d, "D")
}

/// INFILE is the child's standard input, expanded against
/// `default-directory`; nil is the null device.
#[test]
fn infile_is_standard_input() {
    let d = fixture("infile");
    assert_eq!(
        in_buffer(&d, "(call-process \"cat\" \"in.txt\" t)"),
        "(0 \"<line1\nline2\n>\" 14)"
    );
    assert_eq!(
        in_buffer(&d, "(call-process \"cat\" nil t)"),
        "(0 \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(&d, "(call-process \"cat\" \"nope.txt\" t)"),
        "(ERR (file-missing \"Opening process input file\" \"No such file or directory\" \"D/nope.txt\"))"
    );
}

/// t mixes standard error into the buffer; `(REAL STDERR)` discards it
/// (nil), mixes it (t) or writes it to a file; `(:file FILE)` writes both
/// streams to FILE; nil discards; a number does not wait and answers nil;
/// a buffer name is created.
#[test]
fn destination_forms() {
    let d = fixture("destination");
    let sh = "nil \"-c\" \"echo out; echo err >&2\"";
    assert_eq!(
        in_buffer(&d, &format!("(call-process \"sh\" nil t {sh})")),
        "(0 \"<out\nerr\n>\" 10)"
    );
    assert_eq!(
        in_buffer(&d, &format!("(call-process \"sh\" nil '(t nil) {sh})")),
        "(0 \"<out\n>\" 6)"
    );
    assert_eq!(
        in_buffer(&d, &format!("(call-process \"sh\" nil '(t t) {sh})")),
        "(0 \"<out\nerr\n>\" 10)"
    );
    assert_eq!(
        in_buffer(&d, &format!("(list (call-process \"sh\" nil '(t \"err.txt\") {sh}) (with-temp-buffer (insert-file-contents \"err.txt\") (buffer-string)))")),
        "((0 \"err\n\") \"<out\n>\" 6)"
    );
    assert_eq!(
        in_buffer(&d, &format!("(list (call-process \"sh\" nil '(:file \"out.txt\") {sh}) (with-temp-buffer (insert-file-contents \"out.txt\") (buffer-string)))")),
        "((0 \"out\nerr\n\") \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(&d, "(call-process \"sh\" nil nil nil \"-c\" \"echo out\")"),
        "(0 \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(&d, "(call-process \"sh\" nil 0 nil \"-c\" \"echo out\")"),
        "(nil \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(&d, "(list (call-process \"sh\" nil \"cpbuf\" nil \"-c\" \"echo out\") (with-current-buffer \"cpbuf\" (buffer-string)))"),
        "((0 \"out\n\") \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(&d, "(list (call-process \"sh\" \"in.txt\" '(\"cpb3\" t) nil \"-c\" \"cat; echo e >&2\") (with-current-buffer \"cpb3\" (buffer-string)))"),
        "((0 \"line1\nline2\ne\n\") \"<>\" 2)"
    );
    assert_eq!(
        in_buffer(
            &d,
            "(call-process \"sh\" nil '(t . \"x\") nil \"-c\" \"echo x; echo y >&2\")"
        ),
        "(0 \"<x\ny\n>\" 6)"
    );
    // Output with no characters inserts nothing, so a read-only buffer is fine.
    assert_eq!(
        eval("(with-temp-buffer (setq buffer-read-only t) (call-process \"true\" nil t))"),
        "0"
    );
    assert_eq!(
        eval("(car (with-temp-buffer (setq buffer-read-only t) (condition-case e (call-process \"echo\" nil t nil \"ro\") (error e))))"),
        "buffer-read-only"
    );
}

/// The checks come in callproc.c's order: INFILE's type, PROGRAM and ARGS,
/// DESTINATION, `default-directory`, INFILE opened, PROGRAM searched for,
/// then the output and error files.
#[test]
fn check_order() {
    let d = fixture("order");
    let cases = [
        ("(call-process \"nonexist\" \"nope.txt\")", "(ERR (file-missing \"Opening process input file\" \"No such file or directory\" \"D/nope.txt\"))"),
        ("(call-process 5 3 'foo nil 4)", "(ERR (wrong-type-argument stringp 3))"),
        ("(call-process \"echo\" nil 'foo nil 3)", "(ERR (wrong-type-argument stringp 3))"),
        ("(call-process \"nonexist\" nil 'foo)", "(ERR (wrong-type-argument stringp foo))"),
        ("(let ((default-directory \"/nonexist/\")) (call-process \"nonexist\" nil 'foo))", "(ERR (wrong-type-argument stringp foo))"),
        ("(let ((default-directory \"/nonexist/\")) (call-process \"echo\" \"in.txt\"))", "(ERR (file-missing \"Setting current directory\" \"No such file or directory\" \"/nonexist/\"))"),
        ("(call-process \"nonexist\" nil '(:file \"nodir/o\"))", "(ERR (file-missing \"Searching for program\" \"No such file or directory\" \"nonexist\"))"),
        ("(call-process \"echo\" \"nope.txt\" '(:file \"nodir/o\"))", "(ERR (file-missing \"Opening process input file\" \"No such file or directory\" \"D/nope.txt\"))"),
        ("(call-process \"echo\" nil '(:file \"nodir/o\"))", "(ERR (file-missing \"Opening process output file\" \"No such file or directory\" \"D/nodir/o\"))"),
        ("(call-process \"echo\" nil '(t \"nodir/e\"))", "(ERR (file-missing \"Cannot redirect stderr\" \"No such file or directory\" \"D/nodir/e\"))"),
        ("(call-process \"echo\" nil 1.5)", "(ERR (wrong-type-argument stringp 1.5))"),
        ("(call-process \"echo\" nil -1)", "nil"),
        ("(let ((b (get-buffer-create \"dead\"))) (kill-buffer b) (call-process \"echo\" nil b nil \"z\"))", "(ERR (error \"Selecting deleted buffer\"))"),
    ];
    let dir = d.to_string_lossy();
    for (form, want) in cases {
        let got =
            eval(&format!("(let ((default-directory \"{dir}/\")) {form})")).replace(&*dir, "D");
        assert_eq!(got, want, "{form}");
    }
}

/// A child killed by a signal answers `strsignal`'s description of it (the
/// C library's text: "Terminated" on glibc, "Terminated: 15" on macOS).
#[test]
fn signal_answers_its_description() {
    assert_eq!(
        eval("(let ((r (call-process \"sh\" nil nil nil \"-c\" \"kill -TERM $$\"))) (and (stringp r) (string-prefix-p \"Terminated\" r)))"),
        "t"
    );
    assert_eq!(
        eval("(let ((r (call-process \"sh\" nil nil nil \"-c\" \"kill -KILL $$\"))) (and (stringp r) (string-prefix-p \"Killed\" r)))"),
        "t"
    );
    assert_eq!(
        eval("(call-process \"sh\" nil nil nil \"-c\" \"exit 7\")"),
        "7"
    );
}

/// `shell-command-to-string` runs `shell-file-name` with
/// `shell-command-switch`, standard error mixed into the output in order.
#[test]
fn shell_command_to_string_runs_shell_file_name() {
    assert_eq!(
        eval("(list (stringp shell-file-name) shell-command-switch)"),
        "(t \"-c\")"
    );
    assert_eq!(
        eval("(shell-command-to-string \"echo a; echo b >&2; echo c\")"),
        "\"a\nb\nc\n\""
    );
    assert_eq!(
        eval("(let ((shell-file-name \"/bin/sh\")) (shell-command-to-string \"echo $0\"))"),
        "\"/bin/sh\n\""
    );
    assert_eq!(
        eval("(let ((shell-file-name \"/bin/sh\") (shell-command-switch \"-ec\")) (shell-command-to-string \"false; echo x\"))"),
        "\"\""
    );
    assert_eq!(
        eval("(let ((shell-file-name \"/nonexist\")) (shell-command-to-string \"echo\"))"),
        "(ERR (file-missing \"Searching for program\" \"No such file or directory\" \"/nonexist\"))"
    );
}
