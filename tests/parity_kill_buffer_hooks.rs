//! buffer.c `Fkill_buffer''s Lisp: `kill-buffer-query-functions',
//! `kill-buffer-hook', INHIBIT-BUFFER-HOOKS and subr.el's `with-temp-buffer'.
//!
//! Expectations are GNU Emacs 31.1's, each form evaluated in a fresh
//! `emacs -Q --batch -l scripts/fuzz/drive.el`, whose printing this file's
//! helper repeats: `=VALUE` or `!ERROR` under `print-circle` and
//! `print-escape-newlines`, clipped past 400 characters.

use elisprs::{eval_str, print, reset_host};

/// FORM evaluated in a fresh host and printed as drive.el prints it.
fn drive(form: &str) -> String {
    reset_host();
    let v = eval_str(&format!(
        "(let ((print-circle t) (print-escape-newlines t)) (let ((s (condition-case e (concat \"=\" (prin1-to-string (eval '{form} t))) (error (concat \"!\" (prin1-to-string e)))))) (if (> (length s) 400) (format \"%s...<clipped, %d chars, tail %S>\" (substring s 0 400) (length s) (substring s -40)) s)))"
    ))
    .expect("eval failed");
    print(&v, false)
}

/// A query function can refuse, the hook runs with the buffer current,
/// a buffer made with INHIBIT-BUFFER-HOOKS runs neither, and the buffer named
/// at the start is the one killed though `replace-buffer-in-windows' made
/// another current.
#[test]
fn kill_buffer_runs_its_hooks() {
    let cases = [
        (
            "(let ((log nil)) (with-current-buffer (get-buffer-create \"k1\") (add-hook 'kill-buffer-hook (lambda () (push (list 'hook (current-buffer)) log)) nil t)) (list (kill-buffer \"k1\") log))",
            "=(t ((hook #<killed buffer>)))",
        ),
        (
            "(let ((log nil)) (setq kill-buffer-query-functions (list (lambda () (push (buffer-name) log) nil))) (get-buffer-create \"k2\") (list (kill-buffer \"k2\") (buffer-live-p (get-buffer \"k2\")) log))",
            "=(nil t (\"k2\"))",
        ),
        (
            "(let ((log nil)) (let ((b (get-buffer-create \"k3\" t))) (with-current-buffer b (add-hook 'kill-buffer-hook (lambda () (push 'ran log)) nil t)) (list (kill-buffer b) log)))",
            "=(t nil)",
        ),
        (
            "(let ((log nil)) (add-hook 'kill-buffer-hook (lambda () (push (buffer-name) log))) (with-temp-buffer (insert \"x\")) (generate-new-buffer \"k4\") (kill-buffer \"k4\") (generate-new-buffer \"k5\" t) (kill-buffer \"k5\") log)",
            "=(\"k4\")",
        ),
        (
            "(let ((log nil)) (with-current-buffer (get-buffer-create \"k5\") (add-hook 'kill-buffer-hook (lambda () (kill-buffer (current-buffer)) (push 'inner log)) nil t)) (list (kill-buffer \"k5\") log))",
            "!(excessive-lisp-nesting 1601)",
        ),
        (
            "(let ((b (get-buffer-create \"k6\"))) (switch-to-buffer b) (list (kill-buffer b) (current-buffer) (window-buffer)))",
            "=(t #<buffer *scratch*> #<buffer *scratch*>)",
        ),
        (
            "(list kill-buffer-query-functions (get-buffer-process (current-buffer)) (macroexpand-1 '(with-temp-buffer 1)))",
            "=((process-kill-buffer-query-function) nil (let ((temp-buffer (generate-new-buffer \" *temp*\" t))) (with-current-buffer temp-buffer (unwind-protect (progn 1) (and (buffer-name temp-buffer) (kill-buffer temp-buffer))))))",
        ),
        (
            "(let ((b (current-buffer))) (insert \"hello\") (list (kill-buffer) (buffer-live-p b) (current-buffer) (other-buffer b)))",
            "=(t nil #<buffer *Messages*> #<buffer *Messages*>)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
