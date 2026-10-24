//! The display state of windows on a frame batch Emacs never displays:
//! margins, fringes, scroll bars, `recenter', `window-at',
//! `coordinates-in-window-p' and the terminal frame's geometry.
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

/// Values window.c, xdisp.c and frame.c give the initial terminal frame.
#[test]
fn window_display_state() {
    let cases = [
        (
            "(list (frame-char-height) (frame-char-width) (window-fringes) (window-margins) (window-scroll-bars) (window-text-width) (window-text-height) (window-old-buffer) (window-bump-use-time) (frame-old-selected-window) (window-old-pixel-width) (window-vscroll) (window-cursor-type) (window-at 0 0) (window-at 10 30) (window-at 5 24) (minibuffer-selected-window))",
            "=(1 1 (0 0 nil nil) (nil) (nil 0 t nil 0 t nil) 80 23 nil nil nil 0 0 t #<window 1 on *scratch*> nil #<window 2 on  *Minibuf-0*> nil)",
        ),
        (
            "(list (set-window-margins nil 2 3) (window-margins) (window-body-width) (set-window-fringes nil 4 4) (window-fringes) (set-window-margins nil 2 3) (set-window-margins nil nil) (window-margins) (window-body-width))",
            "=(t (2 . 3) 75 nil (0 0 nil nil) nil t (nil) 80)",
        ),
        (
            "(with-temp-buffer (insert \"a\\nb\\nc\\nd\\n\") (set-window-buffer nil (current-buffer)) (goto-char 7) (list (recenter) (window-start) (recenter 1) (window-start) (recenter 0) (window-start) (recenter -1) (window-start) (recenter nil t) (window-start)))",
            "=(nil 1 nil 5 nil 7 nil 1 nil 1)",
        ),
        (
            "(list (condition-case e (with-temp-buffer (recenter)) (error e)) (pos-visible-in-window-p) (window-line-height) (coordinates-in-window-p '(1 . 1) (selected-window)) (coordinates-in-window-p '(1 . 23) (selected-window)) (coordinates-in-window-p '(79 . 5) (selected-window)))",
            "=((error \"‘recenter’ing a window that does not display current-buffer\") nil nil (1 . 1) mode-line (79 . 5))",
        ),
        (
            "(let ((w (split-window nil nil t))) (list (coordinates-in-window-p '(39 . 3) (selected-window)) (coordinates-in-window-p '(39 . 23) (selected-window)) (coordinates-in-window-p '(41 . 3) w)))",
            "=(vertical-line mode-line (1 . 3))",
        ),
        (
            "(list (frame-pixel-width) (frame-pixel-height) (frame-text-width) (frame-text-height) (frame-internal-border-width) (frame-right-divider-width) (frame-scroll-bar-width) (frame-parent) (frame-position) (frame-current-scroll-bars) (active-minibuffer-window) (framep (make-frame-visible)) (frame-visible-p (selected-frame)) (frame-edges) (display-graphic-p) (framep-on-display) (line-number-display-width) (line-number-display-width 'columns))",
            "=(80 25 80 25 0 0 0 nil (0 . 0) (nil) nil t t nil nil t 0 0.0)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
