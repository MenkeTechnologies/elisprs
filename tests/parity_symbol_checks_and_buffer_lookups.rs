//! data.c's variable primitives check their SYMBOL argument, and buffer.c's
//! buffer lookups.
//!
//! - `symbol-value`, `default-value`, `buffer-local-value`,
//!   `default-toplevel-value` answered a non-symbol with `(error "not a
//!   symbol")`; `makunbound`, `set-default`, `make-local-variable`,
//!   `make-variable-buffer-local` and `defvaralias` with `(NAME "not a
//!   symbol")`; `boundp` and `default-boundp` with nil; `kill-local-variable`
//!   returned the argument. Every one is `CHECK_SYMBOL`, i.e.
//!   `(wrong-type-argument symbolp OBJECT)`.
//! - `find-buffer`, `get-file-buffer`, `delete-all-overlays` and
//!   `vector-or-char-table-p` were void.
//!
//! Every expectation below is GNU Emacs 31.1 evaluating the same form under
//! `emacs -Q --batch -l FILE`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn variable_primitives_check_symbol() {
    for (form, want) in [
        ("(symbol-value 1)", "(wrong-type-argument symbolp 1)"),
        ("(symbol-value '(a))", "(wrong-type-argument symbolp (a))"),
        (
            "(default-value \"a\")",
            "(wrong-type-argument symbolp \"a\")",
        ),
        ("(default-value [1])", "(wrong-type-argument symbolp [1])"),
        (
            "(default-toplevel-value 1)",
            "(wrong-type-argument symbolp 1)",
        ),
        (
            "(buffer-local-value 1.5 (current-buffer))",
            "(wrong-type-argument symbolp 1.5)",
        ),
        ("(boundp 1)", "(wrong-type-argument symbolp 1)"),
        ("(default-boundp 1)", "(wrong-type-argument symbolp 1)"),
        ("(makunbound 1)", "(wrong-type-argument symbolp 1)"),
        ("(set-default 1 2)", "(wrong-type-argument symbolp 1)"),
        ("(make-local-variable 1)", "(wrong-type-argument symbolp 1)"),
        (
            "(make-variable-buffer-local 1)",
            "(wrong-type-argument symbolp 1)",
        ),
        ("(kill-local-variable 1)", "(wrong-type-argument symbolp 1)"),
        ("(defvaralias 1 'x)", "(wrong-type-argument symbolp 1)"),
        ("(defvaralias 'zzq 1)", "(wrong-type-argument symbolp 1)"),
    ] {
        assert_eq!(
            eval(&format!("(condition-case e {form} (error e))")),
            want,
            "{form}"
        );
    }
}

/// nil and t are symbols: constants where a write is attempted, bound, and
/// a no-op for `kill-local-variable`.
#[test]
fn nil_and_t_are_symbols_to_the_checks() {
    for (form, want) in [
        ("(makunbound nil)", "(setting-constant nil)"),
        ("(make-local-variable nil)", "(setting-constant nil)"),
        ("(make-local-variable t)", "(setting-constant t)"),
        ("(set-default nil 1)", "(setting-constant nil)"),
        ("(make-variable-buffer-local nil)", "(setting-constant nil)"),
        ("(kill-local-variable nil)", "nil"),
        ("(kill-local-variable t)", "t"),
    ] {
        assert_eq!(
            eval(&format!("(condition-case e {form} (error e))")),
            want,
            "{form}"
        );
    }
    assert_eq!(
        eval("(list (boundp nil) (boundp t) (boundp :k) (default-boundp nil))"),
        "(t t t t)"
    );
}

#[test]
fn find_buffer_and_get_file_buffer() {
    assert_eq!(
        eval(
            "(list (find-buffer 'major-mode 'lisp-interaction-mode) (find-buffer 'major-mode 'nope)
                   (condition-case e (find-buffer 1 2) (error e)))"
        ),
        "(#<buffer *scratch*> nil (wrong-type-argument symbolp 1))"
    );
    // `buffer-local-value' signals in the first buffer that lacks the variable.
    assert_eq!(
        eval("(condition-case e (find-buffer 'zz-void-var 1) (error e))"),
        "(void-variable zz-void-var)"
    );
    assert_eq!(
        eval("(list (get-file-buffer \"/x\") (condition-case e (get-file-buffer 1) (error e)))"),
        "(nil (wrong-type-argument stringp 1))"
    );
    // FILENAME is expanded before it is compared.
    assert_eq!(
        eval(
            "(with-temp-buffer (setq buffer-file-name \"/tmp/qq.el\")
               (eq (get-file-buffer \"/tmp/../tmp/qq.el\") (current-buffer)))"
        ),
        "t"
    );
}

#[test]
fn delete_all_overlays_and_vector_or_char_table_p() {
    // Narrowing does not hide an overlay from it, and an empty one at the end
    // goes too.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\")
               (let ((a (make-overlay 1 2)) (d (make-overlay 6 6)))
                 (make-overlay 3 5) (make-overlay 2 2)
                 (narrow-to-region 3 4)
                 (delete-all-overlays)
                 (widen)
                 (list (overlay-buffer a) (overlay-buffer d) (overlays-in 1 7))))"
        ),
        "(nil nil nil)"
    );
    assert_eq!(
        eval("(condition-case e (delete-all-overlays 5) (error e))"),
        "(wrong-type-argument bufferp 5)"
    );
    assert_eq!(
        eval(
            "(list (vector-or-char-table-p [1]) (vector-or-char-table-p (make-char-table nil))
                   (vector-or-char-table-p \"s\") (vector-or-char-table-p (make-bool-vector 1 t))
                   (vector-or-char-table-p (record 'a)))"
        ),
        "(t t nil nil nil)"
    );
}
