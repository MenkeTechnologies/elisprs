//! lread.c `end_of_file_error` per source: a string reports bare
//! `(end-of-file)`, a buffer reports `(end-of-file BUFFER)`, and a file being
//! loaded — read form by form through its load buffer, so the forms before a
//! truncated one have run — reports that (killed) buffer. Also the small C
//! predicates of this round: `invisible-p`, `buffer-base-buffer`,
//! `recent-auto-save-p`, `visited-file-modtime`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

#[test]
fn end_of_file_data() {
    assert_eq!(
        eval("(condition-case e (read-from-string \"(\") (error e))"),
        "(end-of-file)"
    );
    assert_eq!(
        eval("(condition-case e (read \"(\") (error e))"),
        "(end-of-file)"
    );
    assert_eq!(
        eval("(condition-case e (car (read-from-string \"\\\"abc\")) (error e))"),
        "(end-of-file)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a\") (goto-char 1) (condition-case e (read (current-buffer)) (error (list (car e) (bufferp (cadr e)) (eq (cadr e) (current-buffer))))))"),
        "(end-of-file t t)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a\") (let ((m (copy-marker 1))) (condition-case e (read m) (error (list (car e) (cdr e))))))"),
        "(end-of-file nil)"
    );
    assert_eq!(
        eval("(let ((f (expand-file-name (format \"elisprs-eof-%d.el\" (emacs-pid)) temporary-file-directory))) (write-region \"(setq zz-eof 1)\\n(foo\" nil f nil 'silent) (prog1 (list (condition-case e (load f nil t) (error (list (car e) (mapcar (lambda (x) (if (bufferp x) (buffer-live-p x) x)) (cdr e))))) (boundp 'zz-eof)) (delete-file f)))"),
        "((end-of-file (nil)) t)"
    );
}

#[test]
fn display_and_buffer_predicates() {
    assert_eq!(
        eval("(list (invisible-p 'foo) (let ((buffer-invisibility-spec '(foo (bar . t)))) (list (invisible-p 'foo) (invisible-p 'bar) (invisible-p '(x bar)) (invisible-p 'baz))) (with-temp-buffer (insert (propertize \"ab\" 'invisible t) \"c\") (list (invisible-p 1) (invisible-p 3) (invisible-p (copy-marker 2)))) (let ((buffer-invisibility-spec t)) (list (invisible-p 'x) (invisible-p nil))))"),
        "(t (t 2 2 nil) (t nil t) (t nil))"
    );
    assert_eq!(
        eval("(list (buffer-base-buffer) (condition-case e (buffer-base-buffer 3) (error e)) (recent-auto-save-p) (visited-file-modtime))"),
        "(nil (wrong-type-argument bufferp 3) nil 0)"
    );
}
