//! simple.el, emacs-lisp/lisp.el and text-mode.el commands loaded verbatim,
//! and casefiddle.c's `upcase-initials-region`.
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

/// `goto-line` widens unless RELATIVE, `count-words-region` and `what-line`
/// report (the message is the return value too).
#[test]
fn line_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"line1\\nline2\\nline3\\n\") (goto-line 2) (point))"),
        "7"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"line1\\nline2\\nline3\\n\") (narrow-to-region 7 12) (goto-line 3) (list (point) (point-min)))"),
        "(13 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"line1\\nline2\\nline3\\n\") (narrow-to-region 7 18) (goto-line 2 nil t) (point))"),
        "13"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"one two three\\nfour\") (count-words-region 1 (point-max)))"),
        "4"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\\ny\\nz\") (goto-char 3) (what-line))"),
        "\"Line 2\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (split-line) (list (buffer-string) (point)))"),
        "(\"a\n bc\" 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  abc\") (goto-char 4) (split-line) (list (buffer-string) (point)))"),
        "(\"  a\n   bc\" 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (newline-and-indent) (buffer-string))"),
        "\"abc\n\""
    );
}

#[test]
fn whitespace_and_indentation_motion() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"a  \\n\\n   b\") (goto-char 3) (delete-all-space) (buffer-string))"),
        "\"ab\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"  foo\\n  bar\") (goto-char 1) (list (forward-to-indentation 1) (point) (backward-to-indentation 1) (point)))"),
        "(2 9 2 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\tb\") (goto-char 3) (let ((backward-delete-char-untabify-method 'untabify)) (backward-delete-char-untabify 1)) (buffer-string))"),
        "\"a      b\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcdef\") (goto-char 4) (delete-backward-char 2) (list (buffer-string) (point)))"),
        "(\"adef\" 2)"
    );
}

#[test]
fn buffer_and_kill_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (beginning-of-buffer) (list (point) (mark)))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (mark-whole-buffer) (list (point) (mark)))"),
        "(1 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar/baz qux\") (goto-char (point-max)) (setq kill-ring nil) (unix-word-rubout 1) (list (buffer-string) (car kill-ring)))"),
        "(\"foo bar/baz \" \"qux\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar/baz qux\") (goto-char (point-max)) (unix-filename-rubout 2) (buffer-string))"),
        "\"foo bar/\""
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo bar\") (goto-char 5) (forward-unix-word -1) (point))"
        ),
        "1"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcdef\") (goto-char 2) (setq kill-ring nil) (kill-forward-chars 2) (list (buffer-string) (car kill-ring)))"),
        "(\"adef\" \"bc\")"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"src\"))) (with-current-buffer b (insert \"SRC\")) (with-temp-buffer (insert \"xy\") (goto-char 2) (insert-buffer b) (prog1 (list (buffer-string) (point) (mark)) (kill-buffer b))))"),
        "(\"xSRCy\" 2 5)"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"dst\"))) (with-current-buffer b (insert \"D\")) (with-temp-buffer (insert \"abc\") (prepend-to-buffer b 2 4)) (prog1 (with-current-buffer b (list (buffer-string) (point))) (kill-buffer b)))"),
        "(\"Dbc\" 2)"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"cp\"))) (with-current-buffer b (insert \"old\")) (with-temp-buffer (insert \"abc\") (copy-to-buffer b 2 4)) (prog1 (with-current-buffer b (buffer-string)) (kill-buffer b)))"),
        "\"bc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (set-fill-column 33) fill-column)"),
        "33"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello world foo\") (upcase-initials-region 1 (point-max)) (buffer-string))"),
        "\"Hello World Foo\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hELLO wORLD\") (upcase-initials-region 3 (point-max)) (buffer-string))"),
        "\"hELLO WORLD\""
    );
}

/// lisp.el's defun motion in Fundamental mode (`open-paren-in-column-0` and
/// `beginning-of-defun-raw`), pairs, `raise-sexp` and `check-parens`.
#[test]
fn sexp_and_defun_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar baz\") (goto-char 5) (transpose-sexps 1) (buffer-string))"),
        "\"bar foo baz\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a) (b c)\") (goto-char 4) (transpose-sexps 1) (list (buffer-string) (point)))"),
        "(\"(b c) (a)\" 10)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(defun a () 1)\\n(defun b () 2)\\n\") (goto-char 20) (beginning-of-defun) (point))"),
        "16"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(defun a () 1)\\n(defun b () 2)\\n\") (goto-char 2) (end-of-defun) (point))"),
        "16"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(defun a () 1)\\n\\n(defun b () 2)\\n\") (goto-char 20) (mark-defun) (list (point) (mark)))"),
        "(16 32)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(defun a () 1)\\n(defun b () 2)\\n\") (goto-char 20) (narrow-to-defun) (buffer-string))"),
        "\"(defun b () 2)\n\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a (b)\") (check-parens))"),
        "(ERR (user-error \"Unmatched bracket or quote\"))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a (b))\") (check-parens))"),
        "nil"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo\") (goto-char 1) (insert-pair nil ?\\( ?\\)) (list (buffer-string) (point)))"),
        "(\"() foo\" 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar\") (goto-char 1) (insert-parentheses 1) (buffer-string))"),
        "\"(foo) bar\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(foo bar)\") (goto-char 1) (let ((delete-pair-blink-delay 0)) (delete-pair)) (buffer-string))"),
        "\"foo bar\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a (b c) d)\") (goto-char 5) (raise-sexp) (buffer-string))"),
        "\"(a b d)\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(a (b c) d)\") (goto-char 7) (kill-backward-up-list) (buffer-string))"),
        "\"(a c d)\""
    );
}
