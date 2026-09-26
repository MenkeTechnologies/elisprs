//! print.c's PRINTCHARFUN contract and `standard-output`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch`): a buffer receives
//! the text at its point, a marker receives it at the marker (which then sits
//! after it), a function is called once per character, nil defers to
//! `standard-output`, and `with-output-to-string` is subr.el's buffer-backed
//! macro, so `standard-output` inside it is that buffer.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

#[test]
fn a_function_printcharfun_is_called_once_per_character() {
    assert_eq!(
        eval("(let (acc) (princ \"ab\" (lambda (c) (push c acc))) acc)"),
        "(98 97)"
    );
    assert_eq!(
        eval(
            "(let (acc) (let ((standard-output (lambda (c) (push c acc)))) \
             (princ 12) (terpri)) acc)"
        ),
        "(10 50 49)"
    );
    assert_eq!(
        eval("(let (l) (defun pcf-t (c) (push c l)) (print 'z 'pcf-t) (concat (nreverse l)))"),
        "\"\nz\n\""
    );
    assert_eq!(
        err("(princ \"a\" 'no-such-fn)"),
        "(void-function no-such-fn)"
    );
}

#[test]
fn a_buffer_printcharfun_inserts_at_its_point() {
    assert_eq!(
        eval(
            "(with-temp-buffer (princ \"hi\" (current-buffer)) (prin1 \"q\" (current-buffer)) \
             (terpri (current-buffer)) (buffer-string))"
        ),
        "\"hi\\\"q\\\"\n\""
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (let ((standard-output (current-buffer))) \
             (princ 1) (prin1 \"x\") (print 'y)) (buffer-string))"
        ),
        "\"1\\\"x\\\"\ny\n\""
    );
    assert_eq!(
        err("(let ((b (generate-new-buffer \"x\"))) (kill-buffer b) (princ 1 b))"),
        "(error \"Selecting deleted buffer\")"
    );
}

#[test]
fn a_marker_printcharfun_inserts_at_the_marker_and_advances_it() {
    // Point after the marker shifts by the inserted length; point before it
    // stays put.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (let ((m (copy-marker 3))) (goto-char 5) \
             (princ \"XY\" m) (prin1 'q m) (list (buffer-string) (point) (marker-position m))))"
        ),
        "(\"abXYqcdef\" 8 6)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (let ((m (copy-marker 5))) (goto-char 2) \
             (princ \"XY\" m) (list (buffer-string) (point) (marker-position m))))"
        ),
        "(\"abcdXYef\" 2 7)"
    );
}

#[test]
fn terpri_ensure_only_breaks_a_line_that_is_not_at_its_start() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"x\") \
             (list (terpri (current-buffer) t) (terpri (current-buffer) t) (buffer-string)))"
        ),
        "(t nil \"x\n\")"
    );
    assert_eq!(
        eval("(with-output-to-string (princ \"a\") (terpri nil t) (terpri nil t))"),
        "\"a\n\""
    );
    assert_eq!(eval("(with-output-to-string (terpri nil t))"), "\"\"");
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"ab\\n\") (let ((m (copy-marker 4))) \
             (list (terpri m t) (terpri m t) (buffer-string))))"
        ),
        "(nil nil \"ab\n\")"
    );
    assert_eq!(
        err("(terpri (lambda (c) c) t)"),
        "(error \"Unsupported function argument\" #[(c) (c) (t)])"
    );
}

#[test]
fn with_output_to_string_binds_standard_output_to_a_buffer() {
    assert_eq!(
        eval("(with-output-to-string (princ (bufferp standard-output)))"),
        "\"t\""
    );
    assert_eq!(
        eval(
            "(with-output-to-string (princ \"a\") \
             (princ (with-output-to-string (princ \"in\"))) (princ \"b\"))"
        ),
        "\"ainb\""
    );
}

#[test]
fn write_char_outputs_one_character() {
    assert_eq!(
        eval("(with-output-to-string (write-char ?x) (terpri) (print \"s\"))"),
        "\"x\n\n\\\"s\\\"\n\""
    );
    assert_eq!(
        eval("(with-temp-buffer (write-char ?k (current-buffer)) (buffer-string))"),
        "\"k\""
    );
    assert_eq!(
        err("(write-char \"a\")"),
        "(wrong-type-argument fixnump \"a\")"
    );
}
