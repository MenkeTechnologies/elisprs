//! xdisp.c's `current-bidi-paragraph-direction' over bidi.c's paragraph search
//! (`bidi_find_paragraph_start', `find_first_strong_char', isolates skipped).
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

/// The first strong character of point's paragraph decides; a paragraph
/// without one takes the previous paragraph's direction.
#[test]
fn bidi_paragraph_direction() {
    let cases = [
        (
            "(list (current-bidi-paragraph-direction) (with-temp-buffer (current-bidi-paragraph-direction)))",
            "=(left-to-right left-to-right)",
        ),
        (
            "(with-temp-buffer (insert \"\u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"abc \u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"123 \u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"abc\\n\\n\u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (list (current-bidi-paragraph-direction) (progn (goto-char (point-min)) (current-bidi-paragraph-direction)) (progn (goto-char 5) (current-bidi-paragraph-direction))))",
            "=(right-to-left left-to-right left-to-right)",
        ),
        (
            "(with-temp-buffer (insert \"\u{5e9}\u{5dc}\u{5d5}\u{5dd}\\nabc\\n\\n123\\n\") (list (current-bidi-paragraph-direction) (progn (goto-char 7) (current-bidi-paragraph-direction)) (progn (goto-char 11) (current-bidi-paragraph-direction))))",
            "=(right-to-left right-to-left right-to-left)",
        ),
        (
            "(with-temp-buffer (insert \"\u{2067}abc\u{2069} \u{645}\u{631}\u{62d}\u{628}\u{627}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"\u{2067}abc \u{645}\u{631}\u{62d}\u{628}\u{627}\") (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"...!!!\") (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"abc\\n\\n...\\n\\n\u{5de}\") (list (progn (goto-char 7) (current-bidi-paragraph-direction)) (progn (goto-char (point-max)) (current-bidi-paragraph-direction))))",
            "=(left-to-right right-to-left)",
        ),
        (
            "(with-temp-buffer (insert \"\u{5de}\\n\\n...\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (setq bidi-paragraph-direction 'right-to-left) (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (setq bidi-display-reordering nil) (insert \"\u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"abc   \\n  \") (goto-char 4) (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"  \\t\\n\u{5e9}\u{5dc}\u{5d5}\u{5dd}\") (goto-char 2) (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(let ((b (generate-new-buffer \"bb\"))) (with-current-buffer b (insert \"\u{5e9}\")) (list (current-bidi-paragraph-direction b) (condition-case e (current-bidi-paragraph-direction 1) (error e))))",
            "=(right-to-left (wrong-type-argument bufferp 1))",
        ),
        (
            "(with-temp-buffer (insert \"\u{5e9}\\fabc\") (goto-char 4) (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (setq bidi-paragraph-start-re \"^x\") (insert \"\u{5e9}\u{5dc}\u{5d5}\u{5dd}\\nxabc\\nyz\") (goto-char (point-max)) (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"\u{2068}\u{2067}a\u{2069}b\u{2069}\u{5e9}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"\u{2069}\u{5e9}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"\u{2066}\u{5e9}\u{5dc}\u{5d5}\u{5dd}\\nabc\") (list (current-bidi-paragraph-direction) (progn (goto-char 1) (current-bidi-paragraph-direction))))",
            "=(left-to-right left-to-right)",
        ),
        (
            "(with-temp-buffer (insert \"\u{202b}abc\") (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"\u{300}\u{200b}\u{5e9}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"abc\\n\\n\\n\\n\u{5e9}\\n\\n\\n\") (mapcar (lambda (p) (goto-char p) (current-bidi-paragraph-direction)) (number-sequence 1 (point-max))))",
            "=(left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right right-to-left right-to-left right-to-left right-to-left right-to-left)",
        ),
        (
            "(with-temp-buffer (insert \"\u{5e9}\\n\\nabc\\n\\n12\") (mapcar (lambda (p) (goto-char p) (current-bidi-paragraph-direction)) (number-sequence 1 (point-max))))",
            "=(right-to-left right-to-left right-to-left left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right)",
        ),
        (
            "(with-temp-buffer (insert \"abc\\n\\n\u{5e9}\u{5dc}\u{5d5}\u{5dd}\\nxyz\") (narrow-to-region 6 (point-max)) (list (current-bidi-paragraph-direction) (progn (goto-char (point-max)) (current-bidi-paragraph-direction))))",
            "=(right-to-left right-to-left)",
        ),
        (
            "(with-temp-buffer (insert \"abc\\n\\n12\\n\u{5e9}\") (narrow-to-region 6 (point-max)) (goto-char 6) (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (setq bidi-paragraph-separate-re \"^--$\") (insert \"abc\\n--\\n\u{5e9}\") (goto-char (point-max)) (current-bidi-paragraph-direction))",
            "=left-to-right",
        ),
        (
            "(with-temp-buffer (insert \"abc\\f\\f\u{5e9}\") (mapcar (lambda (p) (goto-char p) (current-bidi-paragraph-direction)) (number-sequence 1 (point-max))))",
            "=(left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right left-to-right)",
        ),
        (
            "(with-temp-buffer (insert (make-string 200 ?.) \"\\n\" \"\u{5e9}\") (current-bidi-paragraph-direction))",
            "=right-to-left",
        ),
        (
            "(with-temp-buffer (insert \"  \\n\\t\\n\u{5e9}  \\n  \") (mapcar (lambda (p) (goto-char p) (current-bidi-paragraph-direction)) (number-sequence 1 (point-max))))",
            "=(left-to-right left-to-right left-to-right left-to-right left-to-right right-to-left right-to-left right-to-left right-to-left right-to-left right-to-left right-to-left)",
        ),
    ];
    for (form, want) in cases {
        assert_eq!(drive(form), want, "{form}");
    }
}
