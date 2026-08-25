//! Compiler macros, `expand-file-name`'s absoluteness, and `narrow-to-region`'s
//! bounds check.
//!
//! Three gaps found by sweeping surfaces the other parity files had not
//! reached. They share nothing except that each was a silent wrong answer
//! rather than an error.
//!
//! A function may declare a rewrite for its own call sites, and Emacs applies
//! it in `macroexp--expand-all` — which is why the same form answers
//! differently through `load` and through plain `eval`:
//!
//! ```text
//! (eval '(let ((l nil)) (add-to-list 'l 1) l) t)          ; (void-variable l)
//! (macroexpand-all '(let ((l nil)) (add-to-list 'l 1) l))
//!   => (let ((l nil)) (if (member 1 l) l (setq l (cons 1 l))) l)
//! ```
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.
//! The `expand-file-name` cases use an explicit DIR so the answer does not
//! depend on where the test happens to run.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `add-to-list` cannot serve a lexical variable — it reads and writes a
/// symbol's value cell — so its compiler macro rewrites those call sites. The
/// rewrite is what `macroexpand-all` (and therefore `load`) does.
#[test]
fn a_compiler_macro_rewrites_add_to_list_on_a_lexical_variable() {
    assert_eq!(
        eval("(let ((l nil)) (add-to-list 'l 1) (add-to-list 'l 1) l)"),
        "(1)"
    );
    assert_eq!(eval("(let ((l (list 2))) (add-to-list 'l 1 t) l)"), "(2 1)");
    assert_eq!(
        eval("(let ((l (list 1 2))) (add-to-list 'l 3) l)"),
        "(3 1 2)"
    );
    assert_eq!(
        eval("(let ((l (list \"a\"))) (add-to-list 'l \"a\") l)"),
        "(\"a\")"
    );
    assert_eq!(
        eval("(format \"%S\" (macroexpand-all '(let ((l nil)) (add-to-list 'l 1) l)))"),
        "\"(let ((l nil)) (if (member 1 l) l (setq l (cons 1 l))) l)\""
    );
    assert_eq!(
        eval("(format \"%S\" (macroexpand-all '(let ((l nil)) (add-to-list 'l 1 t) l)))"),
        "\"(let ((l nil)) (if (member 1 l) l (setq l (append l (list 1)))) l)\""
    );
}

/// `eval` does NOT apply compiler macros — it walks the form itself — so the
/// same call that works in a loaded file signals through `eval`. That
/// difference is the point of the mechanism, not an accident of it.
#[test]
fn eval_does_not_apply_compiler_macros() {
    assert_eq!(
        eval("(condition-case e (eval '(let ((l nil)) (add-to-list 'l 1) l) t) (error (car e)))"),
        "void-variable"
    );
}

/// The handler DECLINES for a special variable, so the ordinary function runs —
/// and that function is subr.el's, membership test and all.
#[test]
fn a_dynamic_variable_still_goes_through_the_function() {
    assert_eq!(
        eval(
            "(progn (defvar dyn-l nil) (setq dyn-l nil) \
              (add-to-list 'dyn-l 1) (add-to-list 'dyn-l 1) dyn-l)"
        ),
        "(1)"
    );
    assert_eq!(
        eval("(progn (defvar dyn-m nil) (setq dyn-m (list 2)) (add-to-list 'dyn-m 1 t) dyn-m)"),
        "(2 1)"
    );
    // COMPARE-FN picks the membership test, and how many times it is called is
    // observable: subr.el walks the list by hand and stops at the first match,
    // so a 3-element list with no match calls it three times.
    assert_eq!(
        eval(
            "(let ((n 0)) (progn (defvar dl2 (list 1 2 3)) \
              (add-to-list 'dl2 9 nil (lambda (a b) (setq n (1+ n)) (eq a b))) n))"
        ),
        "3"
    );
}

/// `expand-file-name` always answers an ABSOLUTE name: a relative, empty or
/// `~`-prefixed DIR is itself expanded first.
#[test]
fn expand_file_name_is_always_absolute() {
    assert_eq!(eval("(expand-file-name \"b\" \"/a\")"), "\"/a/b\"");
    assert_eq!(eval("(expand-file-name \"/b\" \"/a\")"), "\"/b\"");
    assert_eq!(eval("(expand-file-name \"../b\" \"/a/c\")"), "\"/a/b\"");
    assert_eq!(eval("(expand-file-name \"a//b\" \"/x\")"), "\"/x/a/b\"");
    assert_eq!(
        eval("(let ((default-directory \"/d/\")) (expand-file-name \"b\" \"a\"))"),
        "\"/d/a/b\""
    );
    assert_eq!(
        eval("(let ((default-directory \"/d/\")) (expand-file-name \"x\" \"\"))"),
        "\"/d/x\""
    );
    assert_eq!(
        eval(
            "(let ((process-environment nil)) (setenv \"HOME\" \"/h\") \
              (expand-file-name \"a\" \"~\"))"
        ),
        "\"/h/a\""
    );
    // The fully degenerate call: Emacs's C has nothing to make absolute.
    assert_eq!(eval("(expand-file-name \"\" \"\")"), "\"\"");
}

/// The trailing slash comes from NAME, not from the joined path — DIR always
/// has one, so reading it off the join made every empty NAME answer a
/// directory.
#[test]
fn the_trailing_slash_comes_from_the_name() {
    assert_eq!(eval("(expand-file-name \"\" \"/a\")"), "\"/a\"");
    assert_eq!(eval("(expand-file-name \"b/\" \"/a\")"), "\"/a/b/\"");
    assert_eq!(eval("(expand-file-name \"b//\" \"/a\")"), "\"/a/b/\"");
    assert_eq!(eval("(expand-file-name \"./\" \"/a\")"), "\"/a/\"");
    assert_eq!(eval("(expand-file-name \"../\" \"/a/b\")"), "\"/a/\"");
    assert_eq!(eval("(expand-file-name \".\" \"/a\")"), "\"/a\"");
    assert_eq!(eval("(expand-file-name \"/a/\" \"/x\")"), "\"/a/\"");
}

/// `narrow-to-region` swaps an inverted pair but SIGNALS on one outside the
/// buffer — it does not clamp. The error names the arguments in the order they
/// were given, not the swapped order.
#[test]
fn narrow_to_region_signals_rather_than_clamping() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") \
              (condition-case e (narrow-to-region 0 3) (error e)))"
        ),
        "(args-out-of-range 0 3)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") \
              (condition-case e (narrow-to-region 3 0) (error e)))"
        ),
        "(args-out-of-range 3 0)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") \
              (condition-case e (narrow-to-region 1 8) (error e)))"
        ),
        "(args-out-of-range 1 8)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") \
              (condition-case e (narrow-to-region 99 1) (error e)))"
        ),
        "(args-out-of-range 99 1)"
    );
    // An inverted but in-range pair is accepted, swapped.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (narrow-to-region 5 2) \
              (list (point-min) (point-max)))"
        ),
        "(2 5)"
    );
    // The bound is the BUFFER, not the current restriction: narrowing wider
    // than the current one is legal.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (narrow-to-region 2 4) \
              (narrow-to-region 1 6) (list (point-min) (point-max)))"
        ),
        "(1 6)"
    );
    // `point-max` is Z, so the whole buffer is in range.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (narrow-to-region 1 7) \
              (list (point-min) (point-max)))"
        ),
        "(1 7)"
    );
}
