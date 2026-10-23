//! The window and frame objects of batch Emacs (window.c, frame.c): one
//! terminal frame holding the root window on `*scratch*` and the minibuffer
//! window, with window.el's and subr.el's buffer switching and window
//! excursions over them, and the simple.el commands that read window state.
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
fn initial_windows_and_frame() {
    assert_eq!(
        eval("(list (selected-window) (minibuffer-window) (window-list) (windowp (selected-window)) (window-live-p (selected-window)) (framep (selected-frame)) (frame-live-p (selected-frame)) (type-of (selected-window)) (type-of (selected-frame)) (eq (car (frame-list)) (selected-frame)))"),
        "(#<window 1 on *scratch*> #<window 2 on  *Minibuf-0*> (#<window 1 on *scratch*>) t t t t window frame t)"
    );
    assert_eq!(
        eval("(list (window-buffer) (window-point) (window-start) (window-end) (window-minibuffer-p) (window-minibuffer-p (minibuffer-window)) (eq (window-frame) (selected-frame)) (frame-selected-window) (frame-root-window) (window-width) (window-height) (window-body-width) (window-body-height) (window-total-width) (window-total-height) (frame-width) (frame-height) (window-hscroll) (window-dedicated-p) (window-parameters))"),
        "(#<buffer *scratch*> 1 1 1 nil t t #<window 1 on *scratch*> #<window 1 on *scratch*> 80 24 80 23 80 24 80 25 0 nil nil)"
    );
    assert_eq!(
        eval("(list (get-buffer-window \"*scratch*\") (get-buffer-window \" *Minibuf-0*\") (get-buffer-window-list \"*scratch*\") (with-temp-buffer (get-buffer-window (current-buffer))) (window-prev-buffers) (window-next-buffers) (one-window-p) (count-windows) (next-window) (previous-window) (window-system) (frame-parameter nil 'name) (frame-parameter nil 'width) (frame-parameter nil 'buffer-list))"),
        "(#<window 1 on *scratch*> nil (#<window 1 on *scratch*>) nil nil nil t 1 #<window 1 on *scratch*> #<window 1 on *scratch*> nil \"F1\" 80 (#<buffer *scratch*>))"
    );
    assert_eq!(
        eval("(list (prin1-to-string (selected-window)) (format \"%S\" (minibuffer-window)) (string-match-p \"\\\\`#<frame F1 0x[0-9a-f]+>\\\\'\" (prin1-to-string (selected-frame))))"),
        "(\"#<window 1 on *scratch*>\" \"#<window 2 on  *Minibuf-0*>\" 0)"
    );
}

/// The selected window's point is its buffer's point; another window keeps
/// its own. `set-window-buffer` does not change the current buffer.
#[test]
fn window_points_and_buffers() {
    assert_eq!(
        eval("(progn (insert \"abc\") (prog1 (list (window-point) (point) (progn (goto-char 2) (window-point)) (with-temp-buffer (window-point))) (erase-buffer)))"),
        "(4 4 2 2)"
    );
    assert_eq!(
        eval("(let ((b (get-buffer-create \"wtest\"))) (prog1 (list (set-window-buffer nil b) (buffer-name (window-buffer)) (window-point) (buffer-name (current-buffer)) (progn (with-current-buffer b (insert \"hello\")) (window-point)) (set-window-point nil 3) (with-current-buffer b (point)) (set-window-start nil 2) (window-start)) (set-window-buffer nil \"*scratch*\") (kill-buffer b)))"),
        "(nil \"wtest\" 1 \"*scratch*\" 6 3 3 2 2)"
    );
    assert_eq!(
        eval("(let ((w (selected-window))) (list (select-window (minibuffer-window)) (current-buffer) (select-window w) (current-buffer)))"),
        "(#<window 2 on  *Minibuf-0*> #<buffer  *Minibuf-0*> #<window 1 on *scratch*> #<buffer *scratch*>)"
    );
    assert_eq!(
        eval("(with-temp-buffer (list (select-window (selected-window)) (current-buffer)))"),
        "(#<window 1 on *scratch*> #<buffer *scratch*>)"
    );
    assert_eq!(
        eval("(let ((w (selected-window))) (with-current-buffer \"*scratch*\" (erase-buffer) (insert \"abc\") (goto-char 3)) (select-window (minibuffer-window)) (with-current-buffer \"*scratch*\" (goto-char 1) (insert \"XY\")) (prog1 (list (window-point w) (window-start w)) (select-window w) (with-current-buffer \"*scratch*\" (erase-buffer))))"),
        "(5 1)"
    );
    assert_eq!(
        eval(
            "(with-selected-window (minibuffer-window) (list (selected-window) (current-buffer)))"
        ),
        "(#<window 2 on  *Minibuf-0*> #<buffer  *Minibuf-0*>)"
    );
    assert_eq!(
        eval("(progn (set-buffer \"*scratch*\") (insert \"0123456789\") (set-window-start nil 5) (set-window-buffer nil (get-buffer-create \"ub\")) (prog1 (list (window-start) (progn (set-window-buffer nil \"*scratch*\") (window-start))) (kill-buffer \"ub\") (erase-buffer)))"),
        "(1 5)"
    );
    assert_eq!(
        eval("(list (set-window-buffer nil \"no-such-buffer-zz\") (window-buffer))"),
        "(ERR (wrong-type-argument bufferp nil))"
    );
    assert_eq!(
        eval("(let ((b (get-buffer-create \"wdead\"))) (kill-buffer b) (set-window-buffer nil b))"),
        "(ERR (error \"Attempt to display deleted buffer\"))"
    );
    assert_eq!(
        eval("(let ((b (get-buffer-create \"wded\"))) (set-window-dedicated-p nil t) (prog1 (condition-case e (set-window-buffer nil b) (error e)) (set-window-dedicated-p nil nil) (kill-buffer b)))"),
        "(error \"Window is dedicated to ‘*scratch*’\")"
    );
    assert_eq!(
        eval("(let ((b (get-buffer-create \"o2\"))) (set-window-buffer nil b) (kill-buffer b) (list (window-buffer) (current-buffer)))"),
        "(#<buffer *scratch*> #<buffer *scratch*>)"
    );
}

/// `switch-to-buffer` records the buffer (front of `buffer-list`), remembers
/// the previous one in the window, and `other-buffer` skips visible buffers.
#[test]
fn buffer_switching_and_order() {
    assert_eq!(
        eval("(let ((b (get-buffer-create \"o3\"))) (with-current-buffer b (insert \"abc\")) (list (buffer-name (switch-to-buffer b)) (buffer-name (current-buffer)) (buffer-name (window-buffer)) (point) (buffer-name (switch-to-buffer \"*scratch*\")) (seq-remove (lambda (n) (string-prefix-p \" \" n)) (mapcar #'buffer-name (buffer-list))) (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers)) (progn (kill-buffer b) nil)))"),
        "(\"o3\" \"o3\" \"o3\" 4 \"*scratch*\" (\"*scratch*\" \"o3\" \"*Messages*\") (\"o3\") nil)"
    );
    assert_eq!(
        eval("(prog1 (progn (set-buffer (get-buffer-create \"k\")) (kill-buffer) (current-buffer)) (set-buffer \"*scratch*\"))"),
        "#<buffer *Messages*>"
    );
    assert_eq!(
        eval("(progn (get-buffer-create \"k6\") (get-buffer-create \"k7\") (set-buffer \"k7\") (prog1 (list (other-buffer) (other-buffer nil t) (other-buffer (get-buffer \"*scratch*\") t)) (kill-buffer \"k6\") (kill-buffer \"k7\")))"),
        "(#<buffer *Messages*> #<buffer *scratch*> #<buffer *Messages*>)"
    );
    assert_eq!(
        eval("(progn (get-buffer-create \"k5\") (bury-buffer \"k5\") (prog1 (car (last (buffer-list))) (kill-buffer \"k5\")))"),
        "#<killed buffer>"
    );
    assert_eq!(
        eval("(progn (set-buffer \"*scratch*\") (list (buffer-name (save-window-excursion (switch-to-buffer (get-buffer-create \"o4\")) (current-buffer))) (current-buffer) (window-buffer) (prog1 (buffer-live-p (get-buffer \"o4\")) (kill-buffer \"o4\"))))"),
        "(\"o4\" #<buffer *scratch*> #<buffer *scratch*> t)"
    );
    assert_eq!(
        eval("(progn (save-window-excursion nil) (list (window-total-height) (window-top-line (minibuffer-window)) (frame-height) (window-body-height)))"),
        "(23 24 25 22)"
    );
}

/// `end-of-buffer`, `yank-pop`, `append-to-buffer` and `undo` run over the
/// window model.
#[test]
fn window_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\\ny\\nz\") (list (progn (end-of-buffer) (point)) (mark) (progn (beginning-of-buffer) (point))))"),
        "(6 6 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (setq kill-ring nil) (kill-new \"X\") (kill-new \"Y\") (goto-char 1) (yank) (let ((last-command 'yank)) (yank-pop)) (buffer-string))"),
        "\"Xabc\""
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"dst\"))) (with-current-buffer b (insert \"D\")) (with-temp-buffer (insert \"abc\") (append-to-buffer b 1 3) (prepend-to-buffer b 3 4)) (prog1 (with-current-buffer b (list (buffer-string) (point))) (kill-buffer b)))"),
        "(\"Dabc\" 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (delete-region 1 3) (undo-boundary) (let ((last-command nil)) (undo)) (list (buffer-string) (point) (buffer-modified-p)))"),
        "(\"abc\" 4 t)"
    );
    assert_eq!(
        eval("(with-temp-buffer (buffer-enable-undo) (insert \"abc\") (undo-boundary) (insert \"def\") (undo-boundary) (let ((last-command nil)) (undo) (setq last-command 'undo) (undo)) (list (buffer-string) (buffer-modified-p)))"),
        "(\"\" nil)"
    );
}
