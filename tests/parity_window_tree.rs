//! window.c's window tree under window.el's `split-window', `delete-window',
//! `delete-other-windows', resizing and window configurations.
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

/// Edges, pixel and total sizes, normal sizes, siblings and combinations;
/// the batch frame's menu bar line that `window-resize-apply-total' and
/// `set-window-configuration' move the root window below.
#[test]
fn window_tree_split_delete_resize_configurations() {
    let cases = [
        (
            "(list (window-edges) (window-body-height) (window-pixel-edges) (window-normal-size nil) (window-new-total) (window-next-sibling (frame-root-window)) (window-prev-sibling (minibuffer-window)))",
            "=((0 0 80 24) 23 (0 0 80 24) 1.0 0 #<window 2 on  *Minibuf-0*> #<window 1 on *scratch*>)",
        ),
        (
            "(let ((w (split-window))) (list w (window-list) (window-edges) (window-edges w) (window-pixel-edges) (window-pixel-edges w) (window-parent w) (window-top-child (window-parent w)) (window-left-child (window-parent w)) (window-normal-size w) (window-edges (minibuffer-window)) (window-live-p (window-parent w)) (window-valid-p (window-parent w)) (window-buffer (window-parent w)) (condition-case e (window-combination-limit w) (error e))))",
            "=(#<window 4 on *scratch*> (#<window 1 on *scratch*> #<window 4 on *scratch*>) (0 1 80 13) (0 13 80 25) (0 0 80 12) (0 12 80 24) #<window 3> #<window 1 on *scratch*> nil 0.5 (0 25 80 26) nil t nil (error \"Combination limit is meaningful for internal windows only\"))",
        ),
        (
            "(let ((w (split-window nil nil t))) (list (window-edges) (window-edges w) (window-body-width) (window-body-width w) (window-tree)))",
            "=((0 0 40 24) (40 0 80 24) 39 40 ((nil (0 1 80 25) #<window 1 on *scratch*> #<window 4 on *scratch*>) #<window 2 on  *Minibuf-0*>))",
        ),
        (
            "(progn (split-window) (delete-window) (list (window-list) (selected-window) (window-edges) (window-normal-size) (frame-root-window)))",
            "=((#<window 4 on *scratch*>) #<window 4 on *scratch*> (0 1 80 25) 1.0 #<window 4 on *scratch*>)",
        ),
        (
            "(progn (split-window) (split-window nil nil t) (balance-windows) (list (window-tree) (mapcar #'window-edges (window-list)) (next-window) (previous-window) (window-list nil t) (window-list-1 (next-window))))",
            "=(((t (0 1 80 25) (nil (0 1 80 13) #<window 1 on *scratch*> #<window 6 on *scratch*>) #<window 4 on *scratch*>) #<window 2 on  *Minibuf-0*>) ((0 1 40 13) (40 1 80 13) (0 13 80 25)) #<window 6 on *scratch*> #<window 4 on *scratch*> (#<window 1 on *scratch*> #<window 6 on *scratch*> #<window 4 on *scratch*> #<window 2 on  *Minibuf-0*>) (#<window 6 on *scratch*> #<window 4 on *scratch*> #<window 1 on...<clipped, 413 chars, tail \"on *scratch*> #<window 1 on *scratch*>))\">",
        ),
        (
            "(progn (split-window nil 5) (list (mapcar #'window-total-height (window-list)) (condition-case e (split-window nil 1) (error e)) (condition-case e (split-window (minibuffer-window)) (error e)) (progn (delete-other-windows) (condition-case e (delete-window) (error e)))))",
            "=((5 19) #<window 5> (error \"Attempt to split minibuffer window\") (error \"Attempt to delete minibuffer or sole ordinary window\"))",
        ),
        (
            "(progn (split-window) (enlarge-window 3) (shrink-window-horizontally 0) (let ((a (mapcar #'window-edges (window-list)))) (maximize-window) (list a (mapcar #'window-total-height (window-list)) (progn (minimize-window) (mapcar #'window-total-height (window-list))))))",
            "=(((0 1 80 16) (0 16 80 25)) (20 4) (4 20))",
        ),
        (
            "(list (save-window-excursion (split-window) (length (window-list))) (window-list) (window-edges) (window-pixel-edges) (window-edges (minibuffer-window)) (frame-height))",
            "=(2 (#<window 1 on *scratch*>) (0 1 80 24) (0 1 80 24) (0 24 80 25) 25)",
        ),
        (
            "(let ((c (progn (split-window) (current-window-configuration)))) (delete-other-windows) (list (window-edges) (progn (set-window-configuration c) (window-list)) (mapcar #'window-edges (window-list)) (window-edges (minibuffer-window))))",
            "=((0 1 80 25) (#<window 1 on *scratch*> #<window 4 on *scratch*>) ((0 1 80 13) (0 13 80 24)) (0 24 80 25))",
        ),
        (
            "(let ((b (get-buffer-create \"kb2\"))) (split-window) (set-window-buffer (next-window) b) (let ((c (current-window-configuration))) (delete-other-windows) (kill-buffer b) (set-window-configuration c) (list (window-list) (mapcar #'window-buffer (window-list)))))",
            "=((#<window 1 on *scratch*> #<window 4 on *Messages*>) (#<buffer *scratch*> #<buffer *Messages*>))",
        ),
        (
            "(progn (split-window nil nil 'left) (split-window nil nil 'above) (window-swap-states) (list (window-tree) (mapcar #'window-edges (window-list)) (let (l) (walk-window-tree (lambda (w) (push (window-total-width w) l))) l)))",
            "=(((nil (0 1 80 25) #<window 4 on *scratch*> (t (40 1 80 25) #<window 6 on *scratch*> #<window 1 on *scratch*>)) #<window 2 on  *Minibuf-0*>) ((0 1 40 25) (40 1 80 13) (40 13 80 25)) (40 40 40))",
        ),
        (
            "(progn (with-current-buffer (get-buffer-create \"long\") (dotimes (i 100) (insert \"line\\n\"))) (switch-to-buffer \"long\") (goto-char (point-min)) (forward-line 50) (split-window) (list (window-start) (window-start (next-window)) (window-point (next-window)) (point) (progn (select-window (next-window)) (recenter 2) (window-start)) (progn (delete-other-windows) (window-start))))",
            "=(1 1 251 251 241 181)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
