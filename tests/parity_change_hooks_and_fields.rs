//! insdel.c's change hooks around every buffer-modifying primitive
//! (`first-change-hook`, `before-change-functions`, `after-change-functions`,
//! `deactivate-mark`), the field constraint editfns.c's line functions apply,
//! and `insert-char`/`indent-to` property inheritance.
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

/// Each insertion argument, deletion and `erase-buffer` is one change; the
/// before hook sees the old text and the after hook (BEG END OLD-LEN).
#[test]
fn text_changes_run_the_hooks() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e (buffer-string)) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l (buffer-string)) log)) nil t) (insert \"ab\" \"cd\") (delete-region 2 4) (goto-char 2) (insert-char ?x 3) (delete-char -1) (erase-buffer) (insert \"q\") (nreverse log)))"),
        "((b 1 1 \"\") (a 1 3 0 \"ab\") (b 3 3 \"ab\") (a 3 5 0 \"abcd\") (b 2 4 \"abcd\") (a 2 2 2 \"ad\") (b 2 2 \"ad\") (a 2 5 0 \"axxxd\") (b 4 5 \"axxxd\") (a 4 4 1 \"axxd\") (b 1 5 \"axxd\") (a 1 1 4 \"\") (b 1 1 \"\") (a 1 2 0 \"q\"))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert \"abcdef\") (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (transpose-regions 1 2 4 6) (upcase-region 1 3) (goto-char 1) (re-search-forward \"c\") (replace-match \"XYZ\") (narrow-to-region 2 3) (erase-buffer) (nreverse log)))"),
        "((a 1 6 5) (a 1 3 2) (a 4 7 1) (a 1 1 8))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (insert \"x\") (indent-to 9) (let ((indent-tabs-mode nil)) (indent-to 12)) (insert-char ?y 0) (insert \"\") (delete-region 2 2) (nreverse log)))"),
        "((a 1 2 0) (a 2 3 0) (a 3 4 0) (a 4 7 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcdef\") (goto-char 3) (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t) (setq kill-ring nil) (delete-char 2 t) (list (buffer-string) (car kill-ring) (nreverse log))))"),
        "(\"abef\" \"cd\" ((3 3 2)))"
    );
}

/// A property change that changes nothing runs no hook.
#[test]
fn text_property_changes_run_the_hooks() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (insert \"abcdef\") (put-text-property 2 4 'face 'bold) (put-text-property 2 4 'face 'bold) (add-text-properties 1 3 '(x 1)) (remove-text-properties 1 7 '(x nil)) (remove-text-properties 1 7 '(zz nil)) (set-text-properties 1 3 nil) (nreverse log)))"),
        "((b 1 1) (a 1 7 0) (b 2 4) (a 2 4 2) (b 1 3) (a 1 3 2) (b 1 7) (a 1 7 6) (b 1 3) (a 1 3 2))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\") (setq deactivate-mark nil) (put-text-property 1 2 'face 'bold) deactivate-mark)"),
        "t"
    );
}

/// `first-change-hook` runs while the buffer is unmodified; the hooks run with
/// `inhibit-modification-hooks` bound to t, which also suppresses them; a
/// signalling hook is reset to nil.
#[test]
fn hook_protocol() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (setq-local first-change-hook (list (lambda () (push 'first log)))) (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (insert \"a\") (insert \"b\") (set-buffer-modified-p nil) (insert \"c\") (nreverse log)))"),
        "(first (b 1 1) (b 2 2) first (b 3 3))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l inhibit-modification-hooks) log)) nil t) (insert \"a\") (let ((inhibit-modification-hooks t)) (insert \"b\")) (nreverse log)))"),
        "((1 2 0 t))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log) (unless (> (buffer-size) 5) (insert \"!\"))) nil t) (insert \"a\") (list (buffer-string) (nreverse log))))"),
        "(\"a!\" ((1 2 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (add-hook 'after-change-functions (lambda (b e l) (error \"boom\")) nil t) (list (condition-case e (insert \"a\") (error e)) after-change-functions (buffer-string)))"),
        "((error \"boom\") nil \"a\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (add-hook 'before-change-functions (lambda (b e) (error \"boom\")) nil t) (list (condition-case e (insert \"a\") (error e)) before-change-functions (buffer-string)))"),
        "((error \"boom\") nil \"\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (setq deactivate-mark nil) (insert \"a\") deactivate-mark)"),
        "t"
    );
    assert_eq!(
        eval("(with-temp-buffer (setq deactivate-mark nil) (let ((inhibit-modification-hooks t)) (insert \"a\")) deactivate-mark)"),
        "nil"
    );
}

/// `insert-char` with INHERIT and `indent-to` take the properties an
/// `insert-and-inherit` would.
#[test]
fn insertions_inherit_properties() {
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\" 'face 'bold)) (goto-char 2) (insert-char ?x 2 t) (indent-to 8) (list (buffer-string)))"),
        "(#(\"axx\tb\" 0 5 (face bold)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\\ndef\") (set-left-margin 1 (point-max) 3) (buffer-string))"),
        "#(\"   abc\n   def\" 3 13 (left-margin 3))"
    );
}

/// `line-beginning-position`, `line-end-position`, `beginning-of-line` and
/// `end-of-line` stay in point's field; `pos-bol` does not.
#[test]
fn line_functions_respect_fields() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"prompt> \") (put-text-property 1 (point) 'field 'prompt) (insert \"input\") (list (progn (beginning-of-line) (point)) (line-beginning-position) (pos-bol) (progn (end-of-line) (point)) (progn (goto-char 3) (end-of-line) (point)) (progn (goto-char 3) (line-end-position))))"),
        "(9 1 1 9 9 9)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'field 'f) \"def\\n\" (propertize \"ghi\" 'field 'g)) (goto-char 5) (list (line-beginning-position) (line-end-position) (line-beginning-position 2) (progn (beginning-of-line 2) (point)) (let ((inhibit-field-text-motion t)) (goto-char 5) (line-beginning-position))))"),
        "(4 7 8 8 1)"
    );
    assert_eq!(
        eval("(list (condition-case e (end-of-line 1.0) (error e)) (condition-case e (beginning-of-line 1.0) (error e)) (condition-case e (line-beginning-position 1.0) (error e)) (condition-case e (pos-eol \"x\") (error e)))"),
        "((wrong-type-argument fixnump 1.0) (wrong-type-argument fixnump 1.0) (wrong-type-argument integerp 1.0) (wrong-type-argument integerp \"x\"))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nb\") (goto-char 1) (list (line-beginning-position (expt 2 70)) (condition-case e (beginning-of-line (expt 2 70)) (error e))))"),
        "(4 (wrong-type-argument fixnump 1180591620717411303424))"
    );
}
