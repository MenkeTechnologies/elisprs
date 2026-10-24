//! print.c's `print_finish` inserts what was printed into a buffer and then
//! always calls `signal_after_change`; `insert_1_both` of no characters
//! returns before `prepare_to_modify_buffer`. So printing nothing — `terpri`
//! with ENSURE at the beginning of a line, `princ` of an empty string — runs
//! the after-change functions over an empty span and no before-change half.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// SRC's value as `prin1` prints it with `print-escape-newlines` t.
fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(&format!(
        "(let ((print-escape-newlines t)) (prin1-to-string (condition-case e {src} (error (list 'ERR e)))))"
    ))
    .expect("eval failed");
    print(&v, false)
}

#[test]
fn printing_to_a_buffer_runs_after_change_even_when_empty() {
    let cases = [
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer) t) (nreverse log) (buffer-string))))",
            "(t ((b 3 3) (a 3 4 0)) \"ab\\n\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\\n\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer) t) (nreverse log) (buffer-string))))",
            "(nil ((a 4 4 0)) \"ab\\n\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer) t) (nreverse log) (buffer-string))))",
            "(nil ((a 1 1 0)) \"\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (goto-char 2) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer) t) (nreverse log) (buffer-string) (point))))",
            "(t ((b 2 2) (a 2 3 0)) \"a\\nb\" 3)",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"a\\nb\") (goto-char 3) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer) t) (nreverse log) (buffer-string) (point))))",
            "(nil ((a 3 3 0)) \"a\\nb\" 3)",
        ),
        (
            "(with-temp-buffer (let ((log nil) (m nil)) (insert \"ab\\ncd\") (setq m (copy-marker 3)) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri m t) (terpri m t) (nreverse log) (buffer-string) (marker-position m))))",
            "(t nil ((b 3 3) (a 3 4 0) (a 4 4 0)) \"ab\\n\\ncd\" 4)",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (terpri (current-buffer)) (nreverse log) (buffer-string))))",
            "(t ((a 3 4 0)) \"ab\\n\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (princ \"\" (current-buffer)) (nreverse log) (buffer-string))))",
            "(\"\" ((a 3 3 0)) \"ab\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (prin1 \"\" (current-buffer)) (nreverse log) (buffer-string))))",
            "(\"\" ((b 3 3) (a 3 5 0)) \"ab\\\"\\\"\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (list (print 'x (current-buffer)) (nreverse log) (buffer-string))))",
            "(x ((b 3 3) (a 3 6 0)) \"ab\\nx\\n\")",
        ),
        (
            "(with-temp-buffer (let ((log nil)) (insert \"ab\") (add-hook 'first-change-hook (lambda () (push 'first log)) nil t) (set-buffer-modified-p nil) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l (buffer-modified-p)) log)) nil t) (list (princ \"\" (current-buffer)) (nreverse log) (buffer-modified-p))))",
            "(\"\" ((a 3 3 0 nil)) nil)",
        ),
    ];
    for (src, want) in cases {
        assert_eq!(eval(src), want, "{src}");
    }
}
