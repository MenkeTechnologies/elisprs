//! undo.c's records in `buffer-undo-list` (insertions, deletions with marker
//! adjustments, changes, property changes, the first change, point after a
//! boundary), `undo-boundary`, and simple.el's `primitive-undo` and subr.el's
//! change groups over them, with `cycle-spacing` on top.
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
fn undo_records() {
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"a\") (insert \"b\") buffer-undo-list)"),
        "((1 . 3) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (delete-region 1 3) buffer-undo-list)"),
        "((\"ab\" . 1) 4 nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (goto-char 2) (delete-char 1) buffer-undo-list)"),
        "((\"b\" . 2) 4 nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (goto-char 3) (delete-char -1) buffer-undo-list)"),
        "((\"b\" . -2) 4 nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcdef\") (let ((m (copy-marker 3)) (m2 (copy-marker 4 t))) (undo-boundary) (delete-region 2 5) (list (mapcar (lambda (e) (if (markerp (car-safe e)) (list 'M (marker-insertion-type (car e)) (cdr e)) e)) buffer-undo-list) (marker-position m))))"),
        "(((\"bcd\" . 2) (M nil -1) (M t 1) 7 nil (1 . 7) (t . 0)) 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert (propertize \"ab\" 'face 'bold)) (undo-boundary) (delete-region 1 3) (car buffer-undo-list))"),
        "(#(\"ab\" 0 2 (face bold)) . -1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (set-buffer-modified-p nil) (insert \"d\") buffer-undo-list)"),
        "((4 . 5) (t . 0) (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (upcase-region 1 3) buffer-undo-list)"),
        "((1 . 3) (\"ab\" . 1) 4 nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcd\") (undo-boundary) (transpose-regions 1 2 3 4) buffer-undo-list)"),
        "((3 . 4) (\"c\" . 3) (1 . 2) (\"a\" . 1) 5 nil (1 . 5) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcdef\") (undo-boundary) (transpose-regions 1 2 3 6) buffer-undo-list)"),
        "((1 . 6) (\"abcde\" . 1) 7 nil (1 . 7) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (put-text-property 1 3 'face 'bold) (remove-text-properties 2 3 '(face nil)) buffer-undo-list)"),
        "((nil face bold 2 . 3) (nil face nil 1 . 3) nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert (propertize \"ab\" 'x 1)) (undo-boundary) (set-text-properties 1 3 '(y 2)) buffer-undo-list)"),
        "((nil y nil 1 . 3) (nil x 1 1 . 3) nil (1 . 3) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (erase-buffer) buffer-undo-list)"),
        "((\"abc\" . -1) 4 nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (goto-char 2) (undo-boundary) (insert \"X\") buffer-undo-list)"),
        "((2 . 3) nil (1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (delete-region 1 2) buffer-undo-list)"),
        "t"
    );
}

#[test]
fn primitive_undo_and_change_groups() {
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (goto-char 1) (insert \"X\") (undo-boundary) (primitive-undo 1 (cdr buffer-undo-list)) (list (buffer-string) (point)))"),
        "(\"abc\" 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcdef\") (undo-boundary) (put-text-property 2 4 'face 'bold) (delete-region 1 3) (primitive-undo 2 buffer-undo-list) (list (buffer-string) (point)))"),
        "(\"\" 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (let ((g (prepare-change-group))) (activate-change-group g) (insert \"zz\") (delete-region 1 2) (cancel-change-group g)) (buffer-string))"),
        "\"abc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (atomic-change-group (insert \"1\") (insert \"2\")) (list (buffer-string) buffer-undo-list))"),
        "(\"abc12\" ((4 . 6) nil (1 . 4) (t . 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (condition-case nil (atomic-change-group (insert \"1\") (error \"x\")) (error nil)) (buffer-string))"),
        "\"abc\""
    );
}

#[test]
fn cycle_spacing() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"a    b\") (goto-char 3) (cycle-spacing) (buffer-string))"
        ),
        "\"a b\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a    b\") (goto-char 3) (cycle-spacing 0) (buffer-string))"),
        "\"ab\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a    b\") (goto-char 3) (cycle-spacing 3) (buffer-string))"),
        "\"a   b\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a  \\n\\n  b\") (goto-char 3) (cycle-spacing -1) (buffer-string))"),
        "\"a b\""
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"a    b\") (goto-char 3) (let ((last-command nil) (this-command 'cycle-spacing)) (cycle-spacing) (setq last-command 'cycle-spacing) (cycle-spacing) (cycle-spacing) (list (buffer-string) (point))))"),
        "(\"a    b\" 3)"
    );
}
