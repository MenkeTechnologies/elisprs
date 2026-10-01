//! Search limits, `skip-chars`, `forward-line` shortage, the dynamic-only
//! variable primitives, `print-gensym`, and the simple.el / subr.el region and
//! whitespace commands.
//!
//! Every expectation was byte-checked against GNU Emacs (`emacs -Q --batch`,
//! `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// search.c `search_command`: with no BOUND the limit is ZV/BEGV, so a search
/// in a narrowed buffer cannot find text outside the narrowing — and NOERROR
/// non-t moves point to that limit, not to the end of the whole buffer.
#[test]
fn searches_stop_at_the_narrowing() {
    let fwd = |call: &str| {
        eval(&format!(
            "(with-temp-buffer (insert \"abcabc\") (narrow-to-region 1 4) (goto-char 1) \
             (list ({call} \"c\" nil t) ({call} \"a\" nil t) (point)))"
        ))
    };
    assert_eq!(fwd("search-forward"), "(4 nil 4)");
    assert_eq!(fwd("re-search-forward"), "(4 nil 4)");
    let bwd = |call: &str| {
        eval(&format!(
            "(with-temp-buffer (insert \"abcabc\") (narrow-to-region 4 7) (goto-char (point-max)) \
             (list ({call} \"a\" nil t) ({call} \"a\" nil t) (point)))"
        ))
    };
    assert_eq!(bwd("search-backward"), "(4 nil 4)");
    assert_eq!(bwd("re-search-backward"), "(4 nil 4)");
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (narrow-to-region 1 4) (goto-char 1) \
              (list (search-forward \"x\" 100 1) (point)))"
        ),
        "(nil 4)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (narrow-to-region 4 7) (goto-char (point-max)) \
              (list (search-backward \"x\" nil 1) (point)))"
        ),
        "(nil 4)"
    );
}

/// The regexp sees only the accessible portion: `\``/`^` match at BEGV and
/// `\'`/`$` at ZV.
#[test]
fn regexp_anchors_follow_the_narrowing() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabc\") (narrow-to-region 1 4) (goto-char 1) \
              (list (re-search-forward \"c\\\\'\" nil t) (re-search-forward \"c$\" nil t)))"
        ),
        "(4 nil)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcabc\") (narrow-to-region 4 7) (goto-char 4) \
              (list (re-search-forward \"\\\\`a\" nil t) (progn (goto-char 4) (re-search-forward \"^a\" nil t))))"),
        "(5 5)"
    );
}

/// A BOUND on the wrong side of point is an error even with NOERROR, and a
/// BOUND caps where a forward match may END / a backward match may START.
#[test]
fn search_bound_side_and_extent() {
    for form in [
        "(search-forward \"a\" 1)",
        "(re-search-forward \"a\" 1)",
        "(search-forward \"x\" 0 t)",
        "(progn (goto-char 1) (search-backward \"a\" 3 t))",
    ] {
        assert_eq!(
            eval(&format!(
                "(with-temp-buffer (insert \"abc\") (condition-case e {form} (error e)))"
            )),
            "(error \"Invalid search bound (wrong side of point)\")",
            "{form}"
        );
    }
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) \
              (list (search-forward \"c\" 3 t) (re-search-forward \"c\" 3 t) (search-forward \"c\" 4 t)))"),
        "(nil nil 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (list (search-backward \"a\" 2 t) (search-backward \"a\" 1 t)))"),
        "(nil 1)"
    );
}

/// syntax.c `skip_chars`: LIM is honoured (and clamped to the narrowing),
/// `[:class:]` is a character class, `\` quotes, and `-` is a range only
/// between two characters.
#[test]
fn skip_chars_limits_classes_and_quoting() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"a    b\") (goto-char 2) \
              (list (skip-chars-forward \" \" 3) (point) (skip-chars-forward \" \" 100) (point) \
                    (progn (goto-char 6) (skip-chars-backward \" \" 4)) (point) \
                    (progn (goto-char 2) (skip-chars-forward \" \" 1)) (point) \
                    (progn (narrow-to-region 2 4) (goto-char 2) (skip-chars-forward \" \")) (point)))"),
        "(1 3 3 6 -2 4 0 2 2 4)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"ab12 -x\") (goto-char 1) \
              (list (skip-chars-forward \"[:alpha:]\") (point) \
                    (skip-chars-forward \"0-9[:space:]\") (point) \
                    (skip-chars-forward \"\\\\-\") (point) \
                    (progn (goto-char 1) (skip-chars-forward \"^[:space:]\")) (point) \
                    (progn (goto-char 1) (skip-chars-forward \"a-\")) (point)))"
        ),
        "(2 3 3 6 1 7 4 5 1 2)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc]^-x\") (goto-char 1) (skip-chars-forward \"a-c]^-\"))"
        ),
        "6"
    );
    // An inverted range is empty, not an error.
    assert_eq!(
        eval("(with-temp-buffer (insert \"c-a\") (goto-char 1) (skip-chars-forward \"c-a\"))"),
        "0"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (goto-char 1) \
              (condition-case e (skip-chars-forward \"[:bogus:]\") (error e)))"
        ),
        "(error \"Invalid ISO C character class\")"
    );
}

/// cmds.c `Fforward_line`: COUNT <= 0 goes to the beginning of the line and
/// answers the shortage NEGATED.
#[test]
fn forward_line_backward_shortage_is_negative() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\\ndef\") \
              (list (forward-line 0) (point) \
                    (progn (goto-char 2) (forward-line -1)) (point) \
                    (progn (goto-char 6) (forward-line -1)) (point) \
                    (progn (goto-char 6) (forward-line -3)) (point)))"
        ),
        "(0 5 -1 1 0 1 -2 1)"
    );
}

/// `set`, `symbol-value` and `boundp` reach only the dynamic value: a lexical
/// binding of the same name is invisible to them.
#[test]
fn set_and_symbol_value_ignore_lexical_bindings() {
    assert_eq!(
        eval(
            "(list (let ((qq 1)) (boundp 'qq)) \
                    (let ((qq 1)) (set 'qq 2) (list qq (symbol-value 'qq))) \
                    (let ((qq 3)) (boundp 'qq)))"
        ),
        "(nil (1 2) t)"
    );
    assert_eq!(
        eval("(condition-case e (let ((zz 1)) (symbol-value 'zz)) (error e))"),
        "(void-variable zz)"
    );
    // A special variable's `let' IS the dynamic binding.
    assert_eq!(
        eval("(progn (defvar dd 1) (let ((dd 2)) (set 'dd 5) (list dd (symbol-value 'dd) (boundp 'dd))))"),
        "(5 5 t)"
    );
}

/// print.c `print-gensym`: an uninterned symbol prints as `#:NAME` and, with
/// `print-circle`, a shared one is labelled.
#[test]
fn print_gensym_prefix_and_labels() {
    assert_eq!(
        eval(
            "(let ((print-gensym t)) \
              (list (prin1-to-string (make-symbol \"x\")) (prin1-to-string (make-symbol \"\")) \
                    (prin1-to-string (make-symbol \"1\")) (prin1-to-string 'x) \
                    (format \"%s\" (make-symbol \"x\")) (format \"%S\" (make-symbol \"x\"))))"
        ),
        r##"("#:x" "#:" "#:\\1" "x" "x" "#:x")"##
    );
    assert_eq!(
        eval("(let ((print-gensym t) (print-circle t) (s (make-symbol \"x\"))) (prin1-to-string (list s s)))"),
        r##""(#1=#:x #1#)""##
    );
    assert_eq!(
        eval("(let ((print-gensym t) (s (make-symbol \"x\"))) (prin1-to-string (list s s)))"),
        r#""(#:x #:x)""#
    );
}

/// lread.c: `\ ` and `\<newline>` are elided inside a string literal.
#[test]
fn backslash_space_is_elided_in_strings() {
    assert_eq!(eval("\"a\\ b\\\nc\""), "\"abc\"");
    assert_eq!(eval("(read \"\\\"\\\\x41\\\\ b\\\"\")"), "\"Ab\"");
}

/// `delete-region` validates instead of clamping: out of the accessible
/// portion is `(args-out-of-range BUFFER START END)`.
#[test]
fn delete_region_validates_its_bounds() {
    assert_eq!(
        eval("(with-temp-buffer (condition-case e (delete-region 1 5) (error (cons (car e) (cddr e)))))"),
        "(args-out-of-range 1 5)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (narrow-to-region 2 3) \
              (condition-case e (delete-region 1 2) (error (cddr e))))"
        ),
        "(1 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (delete-region 3 1) (buffer-string))"),
        "\"c\""
    );
}

/// cl-macs.el `cl-check-type`: the signal data is (TYPE-or-STRING VALUE FORM).
#[test]
fn cl_check_type_signal_data() {
    assert_eq!(
        eval("(condition-case e (cl-check-type 1 string) (error e))"),
        "(wrong-type-argument string 1 1)"
    );
    assert_eq!(
        eval("(condition-case e (cl-check-type (+ 1 1) string \"a string\") (error e))"),
        "(wrong-type-argument \"a string\" 2 (+ 1 1))"
    );
    assert_eq!(eval("(cl-check-type \"a\" string)"), "nil");
}

/// subr.el `replace-regexp-in-region` / `replace-string-in-region`.
#[test]
fn replace_in_region() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello world\") (goto-char 1) \
              (list (replace-regexp-in-region \"o\" \"0\") (buffer-string) (point)))"
        ),
        "(2 \"hell0 w0rld\" 1)"
    );
    // The region end tracks the growing replacement.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aaaa\") \
              (list (replace-regexp-in-region \"a\" \"bb\" 1 3) (buffer-string) (point)))"
        ),
        "(2 \"bbbbaa\" 7)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aaaa\") (list (replace-regexp-in-region \"a\" \"\" 1 4) (buffer-string)))"),
        "(3 \"a\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"ABC abc\") (let ((case-fold-search t)) \
              (list (replace-regexp-in-region \"a\" \"x\" 1) (buffer-string))))"
        ),
        "(1 \"ABC xbc\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x=1 y=2\") \
              (list (replace-regexp-in-region \"\\\\([a-z]\\\\)=\\\\([0-9]\\\\)\" \"\\\\2:\\\\1\" 1) (buffer-string)))"),
        "(2 \"1:x 2:y\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (list (replace-regexp-in-region \"b\" \"x\" 3 2) \
              (replace-regexp-in-region \"c\" \"x\" 1 3) (buffer-string)))"
        ),
        "(nil nil \"abc\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"a.b.c\") (goto-char 1) \
              (list (replace-string-in-region \".\" \"--\") (buffer-string) (point)))"
        ),
        "(2 \"a--b--c\" 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\\\b\") (list (replace-string-in-region \"a\" \"\\\\1\" 1) (buffer-string)))"),
        "(1 \"\\\\1\\\\b\")"
    );
    for (form, want) in [
        (
            "(replace-regexp-in-region \"a\" \"x\" 1 10)",
            "(error \"End after end of buffer\")",
        ),
        (
            "(replace-string-in-region \"a\" \"x\" 0)",
            "(error \"Start before start of buffer\")",
        ),
        (
            "(replace-regexp-in-region \"a\" \"x\" 10)",
            "(args-out-of-range 10 4)",
        ),
        (
            "(replace-regexp-in-region \"a\" \"x\" \"1\")",
            "(wrong-type-argument number-or-marker-p \"1\")",
        ),
    ] {
        assert_eq!(
            eval(&format!(
                "(with-temp-buffer (insert \"abc\") (condition-case e {form} (error e)))"
            )),
            want,
            "{form}"
        );
    }
}

/// indent.c / simple.el indentation commands.
#[test]
fn indentation_commands() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n\\t  b\") (back-to-indentation) (point))"),
        "6"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"\\tx\") (list (current-column) (current-indentation)))"),
        "(9 8)"
    );
    // Tabs up to the last tab stop at or before COLUMN, then spaces.
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\") (list (indent-to 20) (buffer-string)))"),
        "(20 \"x\t\t    \")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcdefghij\") (list (indent-to 4 2) (buffer-string)))"),
        "(12 \"abcdefghij  \")"
    );
    assert_eq!(
        eval("(with-temp-buffer (let ((indent-tabs-mode nil)) (list (indent-to 10) (buffer-string))))"),
        "(10 \"          \")"
    );
    assert_eq!(
        eval("(with-temp-buffer (condition-case e (indent-to 1.5) (error e)))"),
        "(wrong-type-argument fixnump 1.5)"
    );
}

/// simple.el whitespace commands.
#[test]
fn whitespace_commands() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"a \\t b\") (goto-char 3) \
              (list (progn (delete-horizontal-space t) (buffer-string)) (point)))"
        ),
        "(\"a\t b\" 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a  \\nb \\t\\n\\n\\n\") (list (delete-trailing-whitespace) (buffer-string)))"),
        "(nil \"a\nb\n\")"
    );
    // Formfeeds survive; END limits the scan.
    assert_eq!(
        eval("(with-temp-buffer (insert \"a \\f\\nb\") (delete-trailing-whitespace) (buffer-string))"),
        "\"a \u{c}\nb\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a  \\nb  \\nc  \") (delete-trailing-whitespace 1 5) (buffer-string))"),
        "\"a\nb  \nc  \""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo   bar\") (goto-char 5) (fixup-whitespace) (list (buffer-string) (point)))"),
        "(\"foo bar\" 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n   b\\n  c\") (goto-char 1) (delete-indentation t) (list (buffer-string) (point)))"),
        "(\"a b\n  c\" 2)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\nb\\nc\\nd\") (delete-indentation nil 1 (point-max)) (buffer-string))"),
        "\"a b c d\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\n\\n\\n\\nb\") (goto-char 4) (delete-blank-lines) (list (buffer-string) (point)))"),
        "(\"a\n\nb\" 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a  \\n\\n  b\") (goto-char 4) (just-one-space -1) (list (buffer-string) (point)))"),
        "(\"a b\" 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a   \\t b\") (goto-char 4) (just-one-space 3) (list (buffer-string) (point)))"),
        "(\"a   b\" 5)"
    );
}

/// subr.el `shell-quote-argument`, POSIX branch.
#[test]
fn shell_quote_argument_posix() {
    assert_eq!(
        eval(
            "(list (shell-quote-argument \"\") (shell-quote-argument \"a/b-c_d.e\") \
                    (shell-quote-argument \"a b'c\\\"$x\") (shell-quote-argument \"l1\\nl2\"))"
        ),
        "(\"''\" \"a/b-c_d.e\" \"a\\\\ b\\\\'c\\\\\\\"\\\\$x\" \"l1'\n'l2\")"
    );
}
