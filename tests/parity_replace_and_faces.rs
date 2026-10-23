//! replace.el's `perform-replace` and the commands over it, isearch.el's
//! search core, the faces they name (xfaces.c's global face table) and
//! timer.el's timers, which the lazy highlighting creates.
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
fn replacement_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo foo\") (goto-char 1) (replace-string \"foo\" \"bar\") (buffer-string))"),
        "\"bar bar\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo foo\") (goto-char 1) (replace-regexp \"f\\\\(o\\\\)\" \"g\\\\1\") (buffer-string))"),
        "\"goo goo\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"Foo foo FOO\") (goto-char 1) (replace-string \"foo\" \"bar\") (buffer-string))"),
        "\"Bar bar BAR\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar foo\") (goto-char 1) (replace-string \"foo\" \"X\" t) (buffer-string))"),
        "\"X bar X\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc abc\") (goto-char 1) (replace-string \"abc\" \"X\" nil 3 7) (buffer-string))"),
        "\"abc abc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x foo y foo\") (goto-char 1) (replace-regexp \"foo\" \"\\\\#-\\\\&\") (buffer-string))"),
        "(ERR (error \"Invalid use of ‘\\\\’ in replacement text\"))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"foo foo\") (undo-boundary) (goto-char 1) (replace-string \"foo\" \"bar\") (primitive-undo 1 buffer-undo-list) (buffer-string))"),
        "\"foo foo\""
    );
}

#[test]
fn perform_replace() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo foo foo\") (goto-char 1) (perform-replace \"foo\" \"x\" nil nil nil 2) (buffer-string))"),
        "\"x x x\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar\") (goto-char 1) (list (perform-replace \"o\" \"0\" nil nil nil) (point) (buffer-string)))"),
        "(nil 4 \"f00 bar\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a b a\") (goto-char 1) (perform-replace \"a\" '(\"x\" \"y\") nil nil nil) (buffer-string))"),
        "\"x b y\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab ab\") (goto-char 1) (perform-replace \"a\\\\(b\\\\)\" \"\\\\1\\\\1\" nil t nil) (list (buffer-string) replace-count))"),
        "(\"bb bb\" 0)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaa\") (goto-char 1) (perform-replace \"a\" \"bb\" nil nil nil nil nil 1 3) (buffer-string))"),
        "\"bbbba\""
    );
}

#[test]
fn line_matching_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"aXbXc\") (goto-char 1) (list (how-many \"X\") (count-matches \"X\" 1 3)))"),
        "(2 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nfoo\\nb\\nfoo2\\n\") (goto-char 1) (keep-lines \"foo\") (buffer-string))"),
        "\"foo\nfoo2\n\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nfoo\\nb\\nfoo2\\n\") (goto-char 1) (flush-lines \"foo\") (buffer-string))"),
        "\"a\nb\n\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nb\\na\\n\") (goto-char 1) (setq kill-ring nil) (list (copy-matching-lines \"a\") (car kill-ring)))"),
        "(2 \"a\na\n\")"
    );
}

/// The global attribute vector of a face is all `unspecified` in batch.
#[test]
fn faces_and_timers() {
    assert_eq!(
        eval("(list (facep 'query-replace) (facep 'isearch-group-1) (facep 'lazy-highlight) (facep 'nonexistent-face) (facep \"isearch\"))"),
        "([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] nil [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])"
    );
    assert_eq!(
        eval("(let ((f (make-symbol \"zz\"))) (list (internal-make-lisp-face f) (eq (internal-make-lisp-face f) (internal-make-lisp-face f))))"),
        "([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] t)"
    );
    assert_eq!(
        eval("(internal-make-lisp-face \"x\")"),
        "(ERR (wrong-type-argument symbolp \"x\"))"
    );
    assert_eq!(
        eval("(list (timerp (run-with-idle-timer 10 nil #'ignore)) (progn (setq timer-idle-list nil) (length timer-idle-list)) (redisplay))"),
        "(t 0 t)"
    );
}
