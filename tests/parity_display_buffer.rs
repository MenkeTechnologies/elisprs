//! window.el's `display-buffer' machinery over the window tree:
//! `pop-to-buffer', `switch-to-buffer-other-window', `quit-window', side
//! windows, window states and `kill-buffer''s `replace-buffer-in-windows'.
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

/// Which windows are made, reused, quit or given other buffers.
#[test]
fn display_buffer_and_friends() {
    let cases = [
        (
            "(let ((b (get-buffer-create \"dbuf\"))) (list (display-buffer b) (window-list) (mapcar #'window-edges (window-list)) (get-buffer-window b)))",
            "=(#<window 4 on dbuf> (#<window 1 on *scratch*> #<window 4 on dbuf>) ((0 1 80 13) (0 13 80 25)) #<window 4 on dbuf>)",
        ),
        (
            "(let* ((b (get-buffer-create \"dbuf\")) (w (display-buffer b))) (list (window-prev-buffers w) (window-parameter w 'quit-restore) (progn (quit-window nil w) (window-list))))",
            "=(nil (window window #<window 1 on *scratch*> #<buffer dbuf>) (#<window 1 on *scratch*>))",
        ),
        (
            "(progn (pop-to-buffer \"pbuf\") (list (selected-window) (window-list) (current-buffer) (mapcar #'window-edges (window-list))))",
            "=(#<window 4 on pbuf> (#<window 4 on pbuf> #<window 1 on *scratch*>) #<buffer pbuf> ((0 13 80 25) (0 1 80 13)))",
        ),
        (
            "(progn (switch-to-buffer-other-window \"obuf\") (list (selected-window) (window-list) (current-buffer) (progn (other-window 1) (selected-window))))",
            "=(#<window 4 on obuf> (#<window 4 on obuf> #<window 1 on *scratch*>) #<buffer obuf> #<window 1 on *scratch*>)",
        ),
        (
            "(progn (split-window) (split-window-right) (list (window-in-direction 'below) (window-in-direction 'right) (window-at-side-p nil 'top) (window-at-side-list nil 'bottom)))",
            "=(#<window 4 on *scratch*> #<window 6 on *scratch*> t (#<window 4 on *scratch*>))",
        ),
        (
            "(progn (display-buffer \"dbuf\" '(display-buffer-pop-up-window)) (display-buffer (get-buffer-create \"pbuf\") '(display-buffer-in-side-window (side . right))) (list (window-tree) (mapcar (lambda (w) (list (window-buffer w) (window-edges w) (window-parameter w 'window-side))) (window-list))))",
            "!(error \"Invalid buffer\")",
        ),
        (
            "(progn (split-window-sensibly) (mapcar #'window-edges (window-list)))",
            "=((0 1 80 13) (0 13 80 25))",
        ),
        (
            "(let ((s (progn (split-window) (window-state-get)))) (delete-other-windows) (window-state-put s) (list (window-list) (mapcar #'window-edges (window-list))))",
            "=((#<window 6 on *scratch*> #<window 8 on *scratch*>) ((0 1 80 13) (0 13 80 25)))",
        ),
        (
            "(let ((w (split-window))) (set-window-dedicated-p w t) (list (window-deletable-p w) (progn (set-window-buffer w (get-buffer-create \"dd\")) (kill-buffer \"dd\") (window-list))))",
            "!(error \"Window is dedicated to ‘*scratch*’\")",
        ),
        (
            "(progn (split-window) (kill-buffer (get-buffer-create \"other\")) (switch-to-buffer-other-window (get-buffer-create \"obuf\")) (kill-buffer \"obuf\") (list (window-list) (mapcar #'window-buffer (window-list))))",
            "=((#<window 4 on *scratch*> #<window 1 on *scratch*>) (#<buffer *scratch*> #<buffer *scratch*>))",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
