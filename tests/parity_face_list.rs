//! The faces batch Emacs defines, in face-id order (src/lisp/faces-31.el).
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

/// `face-list', `facep', defface specs and documentation, `face-font'.
#[test]
fn faces_of_batch_emacs() {
    let cases = [
        (
            "(length (face-list))",
            "=186",
        ),
        (
            "(face-list)",
            "=(tooltip eldoc-highlight-function-argument elisp-shorthand-font-lock-face vc-ignored-state vc-edited-state vc-missing-state vc-removed-state vc-conflict-state vc-locally-added-state vc-locked-state vc-needs-update-state vc-up-to-date-state vc-state-base elisp-completion-category-definition elisp-completion-category elisp-defcharset elisp-charset elisp-defcoding elisp-coding elisp-defoclosure elis...<clipped, 3450 chars, tail \"derline bold-italic italic bold default)\">",
        ),
        (
            "(list (facep 'default) (facep 'mode-line) (facep 'tooltip) (facep 'nosuch) (get 'mode-line 'face-defface-spec) (face-documentation 'region) (face-font 'default) (face-font 'default t) (condition-case e (face-font 'nosuch) (error e)))",
            "=([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified u...<clipped, 1113 chars, tail \"\\\"tty\\\" nil (error \\\"Invalid face\\\" nosuch))\">",
        ),
        (
            "(progn (make-face 'my-new-face) (list (car (face-list)) (length (face-list))))",
            "=(my-new-face 187)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
