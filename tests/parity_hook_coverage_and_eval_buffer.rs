//! Change hooks and undo for the remaining modifying primitives (printing to
//! a buffer, `move-to-column` with FORCE, `call-process` output,
//! `subst-char-in-region`), overlay and text-property modification hooks and
//! read-only text (textprop.c `verify_interval_modification`), `read` from a
//! function, and lread.c's `eval-buffer` / `eval-region`.
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
fn printing_and_columns_run_the_hooks() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t) (prin1 'hello (current-buffer)) (princ \"xy\" (current-buffer)) (print 3 (current-buffer)) (terpri (current-buffer)) (write-char ?z (current-buffer)) (list (buffer-string) (nreverse log))))"),
        "(\"helloxy\n3\n\nz\" ((1 6 0) (6 8 0) (8 11 0) (11 12 0) (12 13 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log (m (progn (insert \"abc\") (copy-marker 2)))) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t) (princ \"XY\" m) (list (buffer-string) (nreverse log))))"),
        "(\"aXYbc\" ((2 4 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (let ((standard-output (current-buffer))) (princ \"ab\") (prin1 'c)) buffer-undo-list)"),
        "((1 . 4) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert \"a\\tb\") (add-hook 'after-change-functions (lambda (b e l) (push (list b e l (buffer-string)) log)) nil t) (goto-char 1) (list (move-to-column 3 t) (point) (buffer-string) (nreverse log))))"),
        "(3 4 \"a  \tb\" ((2 4 0 \"a  \tb\") (4 4 1 \"a  b\") (4 5 0 \"a  \tb\")))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert \"ab\") (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t) (goto-char 1) (list (move-to-column 6 t) (buffer-string) (nreverse log))))"),
        "(6 \"ab    \" ((3 7 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t) (call-process \"printf\" nil t nil \"abc\") (list (buffer-string) (nreverse log))))"),
        "(\"abc\" ((1 4 0)))"
    );
}

#[test]
fn subst_char_in_region() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert \"abcabc\") (add-hook 'before-change-functions (lambda (b e) (push (list 'b b e) log)) nil t) (add-hook 'after-change-functions (lambda (b e l) (push (list 'a b e l) log)) nil t) (subst-char-in-region 1 7 ?b ?X) (subst-char-in-region 1 7 ?q ?X) (list (buffer-string) (nreverse log))))"),
        "(\"aXcaXc\" ((b 2 7) (a 2 6 4)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcabc\") (undo-boundary) (subst-char-in-region 1 7 ?b ?X) buffer-undo-list)"),
        "((5 . 6) (\"b\" . 5) (2 . 3) (\"b\" . 2) 7 nil (1 . 7) (t . 0))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abcabc\") (set-buffer-modified-p nil) (subst-char-in-region 1 7 ?b ?X t) (list (buffer-modified-p) buffer-undo-list))"),
        "(t ((1 . 7) (t . 0)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (subst-char-in-region 1 9 ?a ?b))"),
        "(ERR (args-out-of-range #<killed buffer> 1 9))"
    );
}

#[test]
fn overlay_and_property_hooks() {
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert \"abcdef\") (let ((ov (make-overlay 2 4))) (overlay-put ov 'modification-hooks (list (lambda (&rest a) (push (cons 'mod (cdr a)) log)))) (overlay-put ov 'insert-in-front-hooks (list (lambda (&rest a) (push (cons 'front (cdr a)) log)))) (overlay-put ov 'insert-behind-hooks (list (lambda (&rest a) (push (cons 'behind (cdr a)) log)))) (goto-char 2) (insert \"X\") (goto-char 5) (insert \"Y\") (delete-region 3 4) (goto-char 1) (insert \"Z\")) (nreverse log)))"),
        "((front nil 2 2) (front t 2 3 0) (behind nil 5 5) (behind t 5 6 0) (mod nil 3 4) (mod t 3 3 1))"
    );
    assert_eq!(
        eval("(with-temp-buffer (let (log) (insert (propertize \"abc\" 'modification-hooks (list (lambda (b e) (push (list 'mod b e) log))) 'insert-behind-hooks (list (lambda (b e) (push (list 'behind b e) log))) 'insert-in-front-hooks (list (lambda (b e) (push (list 'front b e) log))))) (insert \"d\") (goto-char 1) (insert \"e\") (delete-region 2 3) (put-text-property 3 4 'face 'bold) (nreverse log)))"),
        "((behind 4 5) (front 1 2) (mod 2 3) (mod 3 4))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'read-only t)) (list (condition-case e (insert \"x\") (error e)) (condition-case e (progn (goto-char 1) (insert \"y\")) (error e)) (condition-case e (delete-region 1 2) (error e)) (let ((inhibit-read-only t)) (delete-region 1 2) (buffer-string)) (condition-case e (put-text-property 1 2 'face 'bold) (error e))))"),
        "((text-read-only) nil nil #(\"bc\" 0 2 (read-only t)) (text-read-only))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'read-only \"nope\")) (condition-case e (delete-region 1 2) (error e)))"),
        "(text-read-only \"nope\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\" (propertize \"cd\" 'read-only t 'rear-nonsticky t)) (goto-char (point-max)) (list (condition-case e (insert \"x\") (error e)) (buffer-string)))"),
        "(nil #(\"abcdx\" 2 4 (read-only t rear-nonsticky t)))"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"a\") (and (memq (current-buffer) undo-auto--undoably-changed-buffers) t))"),
        "t"
    );
}

/// The values read and where the stream ends up match; the order of the
/// character calls is not followed (see BUGS.md).
#[test]
fn read_from_a_function() {
    assert_eq!(
        eval("(let* ((s \"(a b) c\") (i 0) (f (lambda (&optional ch) (if ch (progn (setq i (1- i)) nil) (if (< i (length s)) (prog1 (aref s i) (setq i (1+ i))) nil))))) (list (read f) i (read f) i))"),
        "((a b) 5 c 7)"
    );
    assert_eq!(
        eval("(let* ((s \"abc def\") (i 0) (log nil) (f (lambda (&optional ch) (if ch (progn (push (list 'unread ch) log) (setq i (1- i)) nil) (if (< i (length s)) (prog1 (aref s i) (setq i (1+ i))) nil))))) (list (read f) i (nreverse log)))"),
        "(abc 3 ((unread 32)))"
    );
    assert_eq!(
        eval("(let* ((s \"123\") (i 0) (f (lambda (&optional ch) (if ch (setq i (1- i)) (if (< i (length s)) (prog1 (aref s i) (setq i (1+ i))) nil))))) (list (read f) i))"),
        "(123 3)"
    );
    assert_eq!(
        eval("(let* ((s \"\\\"x y\\\" z\") (i 0) (f (lambda (&optional ch) (if ch (setq i (1- i)) (if (< i (length s)) (prog1 (aref s i) (setq i (1+ i))) nil))))) (list (read f) i))"),
        "(\"x y\" 5)"
    );
    assert_eq!(
        eval("(let* ((s \"(a\") (i 0) (f (lambda (&optional ch) (if ch (setq i (1- i)) (if (< i (length s)) (prog1 (aref s i) (setq i (1+ i))) nil))))) (condition-case e (read f) (error e)))"),
        "(end-of-file)"
    );
}

#[test]
fn eval_buffer_and_region() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"(setq ev1 (+ 1 2)) ; c\\n(setq ev2 (* ev1 2))\") (list (eval-buffer) ev1 ev2 (point)))"),
        "(nil 3 6 44)"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"evb\"))) (with-current-buffer b (insert \"(setq ev3 (buffer-name))\")) (prog1 (list (eval-buffer b) (stringp ev3)) (kill-buffer b)))"),
        "(nil t)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"1 (setq ev4 'x) 3\") (goto-char 2) (list (eval-region 3 17) ev4 (point)))"),
        "(nil x 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(setq ev5 1\") (condition-case e (eval-buffer) (error (list (car e) (bufferp (cadr e))))))"),
        "(end-of-file t)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \";; -*- lexical-binding: t -*-\\n(setq ev6 (let ((x 1)) (lambda () x)))\") (eval-buffer) (closurep ev6))"),
        "t"
    );
    assert_eq!(
        eval("(list (with-temp-buffer (insert \";; -*- lexical-binding: nil -*-\\n(setq ev9 lexical-binding)\") (eval-buffer) ev9) (with-temp-buffer (insert \"#!/bin/sh\\n;; -*- mode: x; lexical-binding: t -*-\\n(setq ev9 lexical-binding)\") (eval-buffer) ev9) (with-temp-buffer (insert \"(setq ev9 lexical-binding)\") (setq-local lexical-binding nil) (eval-buffer) ev9))"),
        "(nil t nil)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(princ \\\"hi\\\")\") (list (with-output-to-string (eval-buffer)) (with-output-to-string (eval-buffer nil nil nil nil t))))"),
        "(\"\" \"\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"(setq ev8 1) (setq ev8 (1+ ev8))\") (eval-region 1 (point-max) nil (lambda (s) (read s))) ev8)"),
        "2"
    );
    assert_eq!(
        eval("(condition-case e (eval-buffer \"no-such-buffer-zz\") (error e))"),
        "(error \"No such buffer\")"
    );
    assert_eq!(
        eval("(list (special-variable-p 'values) (progn (setq values nil) (with-temp-buffer (insert \"(+ 1 2)\") (let ((standard-output #'ignore)) (eval-buffer nil #'ignore))) values))"),
        "(nil (3))"
    );
}
