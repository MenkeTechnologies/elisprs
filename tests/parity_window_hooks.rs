//! `window-scroll-functions' run by `set_window_buffer' (split, set-window-buffer),
//! `run-window-configuration-change-hook' and window.c's variables.
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

/// Hook runs and the variables' values.
#[test]
fn window_scroll_and_configuration_hooks() {
    let cases = [
        (
            "(let ((n 0) (m 0)) (add-hook 'window-configuration-change-hook (lambda () (setq n (1+ n)))) (add-hook 'window-scroll-functions (lambda (w s) (setq m (1+ m)))) (run-window-configuration-change-hook) (split-window) (delete-other-windows) (run-window-scroll-functions) (list n m))",
            "=(1 3)",
        ),
        (
            "(let ((m nil)) (add-hook 'window-scroll-functions (lambda (w s) (push (list w s (current-buffer)) m))) (with-current-buffer (get-buffer-create \"x\") (insert \"abc\")) (push 'split m) (split-window) (push 'dow m) (delete-other-windows) (push 'swb m) (set-window-buffer nil \"x\") (push 'stb m) (switch-to-buffer \"*scratch*\") (nreverse m))",
            "=(split (#<window 4> 1 #<buffer *scratch*>) (#<window 4> 1 #<buffer *scratch*>) dow swb (#<window 1 on *scratch*> 1 #<buffer x>) stb (#<window 1 on *scratch*> 1 #<buffer *scratch*>))",
        ),
        (
            "(list window-configuration-change-hook (default-value 'window-configuration-change-hook) window-combination-limit window-persistent-parameters recenter-redisplay)",
            "=(#1=(window--adjust-process-windows) #1# window-size ((context . writable) (clone-of . t)) tty)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
