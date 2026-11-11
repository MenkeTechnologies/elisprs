//! `run_program(src, input)`: the uniform embedding entry point.

#[test]
fn captures_stdout() {
    assert_eq!(
        elisprs::run_program(r#"(princ "hello") (terpri)"#, "").unwrap(),
        "hello\n"
    );
}

/// Batch `read-from-minibuffer`/`read-string` (minibuf.c
/// `read_minibuf_noninteractive`, checked against `emacs --batch`): the prompt
/// goes to stdout, a line ends at `\n` or `\r`, a last line needs no newline,
/// READ parses the line, an empty `read-string` answer is DEFAULT-VALUE, and EOF
/// signals `(end-of-file "Error reading from stdin")`.
#[test]
fn minibuffer_reads_input_as_stdin() {
    let src = r#"
(princ (format "[%S]" (read-from-minibuffer "a: ")))
(princ (format "[%S]" (read-from-minibuffer "b: " nil nil t)))
(princ (format "[%S]" (read-string "c: " nil nil "dflt")))
(princ (format "[%S]" (read-string "d: ")))
(princ (format "[%S]" (condition-case e (read-string "e: ") (error e))))"#;
    assert_eq!(
        elisprs::run_program(src, "one\r(1 2)  \n\nlast").unwrap(),
        r#"a: ["one"]b: [(1 2)]c: ["dflt"]d: ["last"]e: [(end-of-file "Error reading from stdin")]"#
    );
    let src = r#"(condition-case e (read-from-minibuffer "" nil nil t) (error (princ e)))"#;
    assert_eq!(
        elisprs::run_program(src, "1 2\n").unwrap(),
        r#"(invalid-read-syntax Trailing garbage following expression)"#
    );
}

/// An uncaught signal is an error. `kill-emacs` ends the run, not the process
/// (this test would not finish otherwise): it runs `kill-emacs-hook` but no
/// `unwind-protect` cleanup, and its status decides `Ok` versus `Err`.
#[test]
fn errors_and_kill_emacs() {
    let err = elisprs::run_program(r#"(error "boom %d" 7)"#, "").unwrap_err();
    assert_eq!(err, "boom 7");
    let src = r#"
(add-hook 'kill-emacs-hook (lambda () (princ "hook")))
(princ "x")
(unwind-protect (kill-emacs) (princ "cleanup"))
(princ "after")"#;
    assert_eq!(elisprs::run_program(src, "").unwrap(), "xhook");
    assert_eq!(
        elisprs::run_program("(kill-emacs 259)", "").unwrap_err(),
        "kill-emacs: exit status 3"
    );
}

/// Concurrent runs each see their own stdin and return only their own output.
#[test]
fn concurrent_runs_do_not_mix() {
    let src = r#"(let ((n (read-from-minibuffer "" nil nil t))) (dotimes (i 200) (princ (+ n i)) (terpri)))"#;
    let runs: Vec<_> = [1000, 5000]
        .into_iter()
        .map(|n| std::thread::spawn(move || (n, elisprs::run_program(src, &format!("{n}\n")))))
        .collect();
    for run in runs {
        let (n, out) = run.join().unwrap();
        let want: String = (0..200).map(|i| format!("{}\n", n + i)).collect();
        assert_eq!(out.unwrap(), want);
    }
}
