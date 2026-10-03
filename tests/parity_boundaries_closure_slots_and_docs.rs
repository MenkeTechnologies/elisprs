//! Round 33, third batch: word and symbol boundaries decided by the syntax
//! table, `match-data--translate` and Emacs 31's `replace-regexp-in-string`,
//! `looking-at` inside a narrowing, `setq`/`set` on a non-variable, the
//! bool-vector set operations, `float-output-format`, the slots of an
//! interpreted closure (docstring and interactive spec), and `documentation`.
//!
//! Every expectation was byte-checked against GNU Emacs 31.1
//! (`emacs -Q --batch`, `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// regex-emacs.c `wordbeg`/`wordend`/`symbeg`/`symend`/`wordbound`: syntax
/// classes, not Unicode `\w`, and the subject's ends always bound a word.
#[test]
fn boundaries_follow_the_syntax_table() {
    assert_eq!(
        eval(
            "(list (string-match \"\\\\_<foo\\\\_>\" \"a foo-bar foo\") \
                   (string-match \"\\\\<bar\" \"foo_bar bar\") (string-match \"bar\\\\>\" \"bar_x bar\") \
                   (string-match \"\\\\bfoo\" \"_foo\") (string-match \"\\\\Bo\" \"foo\") \
                   (string-match \"\\\\_>\" \"a-b c\") (string-match \"\\\\_<x\" \"a-x\" 2) \
                   (string-match \"\\\\b\" \"\") (string-match \"\\\\B\" \"\") \
                   (string-match \"\\\\b\" \" \") (string-match \"\\\\B \" \" x\"))"
        ),
        "(10 4 0 1 1 3 nil 0 nil 0 nil)"
    );
    assert_eq!(
        eval(
            "(list (replace-regexp-in-string \"\\\\<\" \"^\" \"ab cd\") \
                   (replace-regexp-in-string \"\\\\b\" \"|\" \"ab cd\") \
                   (replace-regexp-in-string \"\\\\B\" \"|\" \"ab cd\") \
                   (split-string \"foo-bar baz\" \"\\\\_>\" t))"
        ),
        "(\"^ab ^cd\" \"|ab| |cd|\" \"a|b c|d\" (\"foo-bar\" \" baz\"))"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"ab cd\") (narrow-to-region 2 4) (goto-char 2) \
               (list (looking-at \"\\\\b\") (progn (goto-char 4) (looking-at \"\\\\b\")) \
                     (progn (goto-char 2) (looking-at \"\\\\`b\"))))"
        ),
        "(t t t)"
    );
}

/// eval.c `Fsetq` leaves the target to `Fset`: constants are
/// `setting-constant`, anything else not a symbol is `symbolp`.
#[test]
fn setq_checks_its_target_at_run_time() {
    assert_eq!(
        eval(
            "(list (condition-case e (setq nil 1) (error e)) (condition-case e (setq t 1) (error e)) \
                   (condition-case e (setq 1 2) (error e)) (condition-case e (set \"s\" 2) (error e)) \
                   (let ((x 0)) (condition-case e (setq nil (setq x 5)) (error (list e x)))) \
                   (if nil (setq nil 1) 'ok))"
        ),
        "((setting-constant nil) (setting-constant t) (wrong-type-argument symbolp 1) \
(wrong-type-argument symbolp \"s\") ((setting-constant nil) 5) ok)"
    );
}

/// data.c `bool_vector_binop_driver` and `Fbool_vector_count_consecutive`.
#[test]
fn bool_vector_set_operations() {
    assert_eq!(
        eval(
            "(list (bool-vector-union (bool-vector t nil) (bool-vector nil t)) \
                   (bool-vector-intersection (bool-vector t t nil) (bool-vector t nil nil)) \
                   (bool-vector-exclusive-or (bool-vector t t) (bool-vector t nil)) \
                   (bool-vector-set-difference (bool-vector t t) (bool-vector t nil)) \
                   (let ((c (bool-vector nil nil))) \
                     (list (bool-vector-union (bool-vector t nil) (bool-vector nil t) c) c \
                           (bool-vector-union (bool-vector t nil) (bool-vector nil t) c))) \
                   (condition-case e (bool-vector-union (bool-vector t) (bool-vector t nil)) (error e)) \
                   (condition-case e (bool-vector-union (bool-vector t) (bool-vector t) (bool-vector nil nil)) (error e)))"
        ),
        "(#&2\"\u{3}\" #&3\"\u{1}\" #&2\"\u{2}\" #&2\"\u{2}\" (#&2\"\u{3}\" #&2\"\u{3}\" nil) \
(wrong-length-argument 1 2) (wrong-length-argument 1 1 2))"
    );
    assert_eq!(
        eval(
            "(list (bool-vector-count-consecutive (bool-vector t t nil t) t 0) \
                   (bool-vector-count-consecutive (bool-vector t t nil t) nil 2) \
                   (bool-vector-count-consecutive (bool-vector t t nil t) t 4) \
                   (bool-vector-count-consecutive (make-bool-vector 100 t) t 3) \
                   (condition-case e (bool-vector-count-consecutive (bool-vector t) t -1) (error e)))"
        ),
        "(2 1 0 97 (wrong-type-argument wholenump -1))"
    );
}

/// print.c `float_to_string` honours a valid `float-output-format`.
#[test]
fn float_output_format() {
    assert_eq!(
        eval(
            "(list (let ((float-output-format \"%.2f\")) \
                     (list (prin1-to-string 1.2345) (number-to-string 2.0) (format \"%s\" 3.14159))) \
                   (let ((float-output-format \"%.0f\")) (prin1-to-string 2.5)) \
                   (let ((float-output-format \"%.3e\")) (prin1-to-string 12345.678)) \
                   (let ((float-output-format \"%.4g\")) (list (prin1-to-string 100.0) (prin1-to-string 0.5))) \
                   (let ((float-output-format \"%.0g\")) (prin1-to-string 0.1)) \
                   (let ((float-output-format \"%5.2f\")) (prin1-to-string 0.1)) \
                   (let ((float-output-format \"%.2f\")) (prin1-to-string 1.0e+INF)))"
        ),
        "((\"1.23\" \"2.00\" \"3.14\") \"2\" \"1.235e+04\" (\"100.0\" \"0.5\") \"0.1\" \"0.1\" \"1.0e+INF\")"
    );
}

/// eval.c `make-interpreted-closure`: the docstring is slot 4 and the
/// interactive spec slot 5, so they print there and `aref` reads them.
#[test]
fn closure_docstring_and_interactive_slots() {
    assert_eq!(
        eval(
            "(list (progn (defun r33-f (x) \"Doc here.\" x) (symbol-function 'r33-f)) \
                   (lambda () \"only\") (lambda (a) (interactive \"p\") a) \
                   (lambda (a) \"doc\" (interactive) a) \
                   (lambda () (interactive \"p\" foo-mode bar-mode) 1))"
        ),
        "(#[(x) (x) (t) nil \"Doc here.\"] #[nil (\"only\") (t)] #[(a) (a) (t) nil nil \"p\"] \
#[(a) (a) (t) nil \"doc\" nil] #[nil (1) (t) nil nil [\"p\" (foo-mode bar-mode)]])"
    );
    assert_eq!(
        eval(
            "(let ((f (lambda (x) \"d\" (interactive \"p\") x))) \
               (list (aref f 0) (aref f 1) (aref f 4) (aref f 5) (length f) \
                     (length (lambda () 1)) (macroexpand-all '(lambda () (interactive (when t 1)) 2))))"
        ),
        "((x) (x) \"d\" \"p\" 6 3 #'(lambda nil (interactive (if t (progn 1))) 2))"
    );
}

/// doc.c `documentation` / `documentation-property` and simple.el's
/// `function-documentation`; `defvar`/`defconst` keep their docstrings.
#[test]
fn documentation_lookup() {
    assert_eq!(
        eval(
            "(progn (defun r33-g () \"Use \\\\=`quote'.\" 1) (defmacro r33-m () \"MD\" 1) \
                    (defun r33-h () 1) (put 'r33-h 'function-documentation \"PD\") \
                    (defvar r33-v 1 \"VD\") (defconst r33-c 1 \"CD\") \
                    (put 'r33-w 'variable-documentation '(concat \"a\" \"b\")) \
               (list (documentation 'r33-g t) (documentation 'r33-m) (documentation 'r33-h) \
                     (documentation (lambda () 1)) (documentation '(lambda (x) \"LD\" x)) \
                     (condition-case e (documentation 'r33-nosuch) (error e)) \
                     (documentation-property 'r33-v 'variable-documentation) \
                     (documentation-property 'r33-c 'variable-documentation) \
                     (documentation-property 'r33-w 'variable-documentation)))"
        ),
        "(\"Use \\\\=`quote'.\" \"MD\" \"PD\" nil \"LD\" (void-function r33-nosuch) \"VD\" \"CD\" \"ab\")"
    );
}
