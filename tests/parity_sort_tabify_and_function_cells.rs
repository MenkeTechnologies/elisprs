//! Round 32: the function cell (`fmakunbound`, `fset` to nil, `functionp` on a
//! special-form subr), `last` on a circular list, the shared empty vector and
//! its `print-circle` treatment, and the buffer primitives and commands that
//! were void — `move-to-column`, `insert-buffer-substring`,
//! `compare-buffer-substrings`, sort.el, tabify.el, replace.el's line filters,
//! `current-word`.
//!
//! Every expectation was byte-checked against GNU Emacs 31.1
//! (`emacs -Q --batch`, `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// data.c `Ffboundp` is `!NILP (function cell)`, so a nil definition is an
/// empty cell: `(fset 'f nil)` and `fmakunbound` both leave F unbound.
#[test]
fn fmakunbound_and_fset_nil_empty_the_cell() {
    assert_eq!(
        eval(
            "(progn (defun r32-f () 1) \
               (list (fmakunbound 'r32-f) (fboundp 'r32-f) (symbol-function 'r32-f) \
                     (condition-case e (r32-f) (error e))))"
        ),
        "(r32-f nil nil (void-function r32-f))"
    );
    assert_eq!(
        eval(
            "(progn (defun r32-g () 2) (fset 'r32-g nil) \
               (list (fboundp 'r32-g) (condition-case e (r32-g) (error e))))"
        ),
        "(nil (void-function r32-g))"
    );
    assert_eq!(
        eval(
            "(list (condition-case e (fmakunbound nil) (error e)) \
                   (condition-case e (fmakunbound t) (error e)) \
                   (condition-case e (fmakunbound 1) (error e)) (fmakunbound :k))"
        ),
        "((setting-constant nil) (setting-constant t) (wrong-type-argument symbolp 1) :k)"
    );
}

/// eval.c `FUNCTIONP`: a subr whose `max_args` is `UNEVALLED` is not a function.
#[test]
fn functionp_rejects_special_form_subr_objects() {
    assert_eq!(
        eval(
            "(list (functionp (symbol-function 'if)) (functionp (symbol-function 'let)) \
                   (functionp (symbol-function 'and)) (functionp (symbol-function 'car)) \
                   (special-form-p (symbol-function 'if)))"
        ),
        "(nil nil nil t t)"
    );
}

/// subr.el `last` is `(nthcdr (1- (safe-length list)) list)`; walking `cdr`
/// until it stops being a cons never returned on a cycle.
#[test]
fn last_terminates_on_a_circular_list() {
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string \
               (last (let ((c (list 122 97 -0.0))) (setcdr (last c) c) c))))"
        ),
        "\"#1=(97 -0.0 122 . #1#)\""
    );
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string \
               (last (let ((c (list t 233 nil))) (setcdr (last c) (cdr c)) c))))"
        ),
        "\"#1=(233 nil . #1#)\""
    );
    assert_eq!(
        eval("(list (last '(1 2 . 3)) (last nil) (last t))"),
        "((2 . 3) nil t)"
    );
}

/// alloc.c `zero_vector`: every empty vector is one object, and neither it nor
/// the empty string earns a `print-circle` label.
#[test]
fn empty_vector_is_shared_and_never_labelled() {
    assert_eq!(
        eval("(list (eq (vector) []) (eq (vconcat nil) (make-vector 0 1)) (eq (record 'a) (record 'a)))"),
        "(t t nil)"
    );
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string \
               (let ((v (vector)) (s (make-string 0 ?a))) (list v v s s (vector v v)))))"
        ),
        "\"([] [] \\\"\\\" \\\"\\\" [[] []])\""
    );
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (let ((v (vector 1))) (list v v))))"),
        "\"(#1=[1] #1#)\""
    );
}

/// indent.c `Fmove_to_column`: stop at the first column >= goal or at eol; a
/// FORCE splits an overshooting tab, and only FORCE `t` extends a short line.
#[test]
fn move_to_column_scans_splits_tabs_and_extends() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\\tdef\") (goto-char 1) \
               (list (move-to-column 5) (point) (move-to-column 20) (point)))"
        ),
        "(8 5 11 8)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\\tdef\") (goto-char 1) \
               (list (move-to-column 5 t) (point) (buffer-string)))"
        ),
        "(5 6 \"abc  \tdef\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (goto-char 1) \
               (list (move-to-column 8 t) (point) (buffer-string)))"
        ),
        "(8 5 \"abc\t\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (list (move-to-column 8 'x) (buffer-string)))"),
        "(3 \"abc\")"
    );
    assert_eq!(
        eval("(list (condition-case e (move-to-column -1) (error e)) (condition-case e (move-to-column 1.5) (error e)))"),
        "((wrong-type-argument wholenump -1) (wrong-type-argument wholenump 1.5))"
    );
    assert_eq!(
        eval(
            "(list (with-temp-buffer (insert \"    foo\") (indent-line-to 2) (buffer-string)) \
                   (with-temp-buffer (insert \"foo\") (indent-line-to 10) (buffer-string)) \
                   (with-temp-buffer (insert \"\\t\\tfoo\") (indent-line-to 3) (buffer-string)))"
        ),
        "(\"  foo\" \"\t  foo\" \"   foo\")"
    );
}

/// editfns.c `Finsert_buffer_substring` and `Fcompare_buffer_substrings`.
#[test]
fn insert_and_compare_buffer_substrings() {
    assert_eq!(
        eval(
            "(let ((b (generate-new-buffer \"src\"))) (with-current-buffer b (insert \"hello\")) \
               (with-temp-buffer (insert \"[\") \
                 (list (insert-buffer-substring b 2 4) (insert-buffer-substring b 4 2) (buffer-string) \
                       (condition-case e (insert-buffer-substring b 0 9) (error e)))))"
        ),
        "(nil nil \"[elel\" (args-out-of-range 0 9))"
    );
    assert_eq!(
        eval(
            "(list (condition-case e (insert-buffer-substring \"no such buf\") (error e)) \
                   (condition-case e (insert-buffer-substring 5) (error e)))"
        ),
        "((error \"No buffer named no such buf\") (wrong-type-argument stringp 5))"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcabdABC\") \
               (list (compare-buffer-substrings nil 1 4 nil 4 7) (compare-buffer-substrings nil 4 7 nil 1 4) \
                     (compare-buffer-substrings nil 1 3 nil 4 7) (compare-buffer-substrings nil 1 4 nil 1 4) \
                     (let ((case-fold-search t)) (compare-buffer-substrings nil 1 4 nil 7 10)) \
                     (let ((case-fold-search nil)) (compare-buffer-substrings nil 1 4 nil 7 10))))"
        ),
        "(-3 3 -3 0 0 1)"
    );
}

/// sort.el through `sort-subr` and `sort-reorder-buffer`.
#[test]
fn sort_el_commands() {
    let buf = |text: &str, call: &str| {
        eval(&format!(
            "(with-temp-buffer (insert \"{text}\") ({call} (point-min) (point-max)) (buffer-string))"
        ))
    };
    assert_eq!(buf("b\\nc\\na\\n", "sort-lines nil"), "\"a\nb\nc\n\"");
    assert_eq!(buf("b\\nc\\na", "sort-lines t"), "\"c\nb\na\"");
    assert_eq!(
        buf("x 3\\ny 10\\nz 2\\n", "sort-numeric-fields 2"),
        "\"z 2\nx 3\ny 10\n\""
    );
    assert_eq!(
        buf("x b\\ny a\\nz c\\n", "sort-fields 2"),
        "\"y a\nx b\nz c\n\""
    );
    assert_eq!(
        buf("x b\\ny a\\nz c\\n", "sort-fields -1"),
        "\"y a\nx b\nz c\n\""
    );
    assert_eq!(
        buf(
            "c=3\\na=1\\nb=2\\n",
            "sort-regexp-fields nil \"^\\\\([a-z]\\\\)=\\\\([0-9]\\\\)$\" \"\\\\2\""
        ),
        "\"a=1\nb=2\nc=3\n\""
    );
    assert_eq!(buf("1\\n2\\n3\\n", "reverse-region"), "\"3\n2\n1\n\"");
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"a\\nb\\na\\nc\\nb\\n\") \
               (list (delete-duplicate-lines (point-min) (point-max)) (buffer-string)))"
        ),
        "(2 \"a\nb\nc\n\")"
    );
}

/// tabify.el, replace.el's `keep-lines` / `flush-lines` / `how-many`, and
/// simple.el `current-word`.
#[test]
fn tabify_line_filters_and_current_word() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\\tb\\t\\tc\") (untabify (point-min) (point-max)) (buffer-string))"),
        "\"a       b               c\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a       b                c\") (tabify (point-min) (point-max)) (buffer-string))"),
        "\"a\tb\t\t c\""
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo bar\\nbaz\\nqux bar\\n\") (goto-char 1) \
               (list (flush-lines \"bar\") (buffer-string)))"
        ),
        "(2 \"baz\n\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo bar\\nbaz\\nqux bar\\n\") (goto-char 1) \
               (list (keep-lines \"bar\") (buffer-string)))"
        ),
        "(nil \"foo bar\nqux bar\n\")"
    );
    // A reversed RSTART/REND counts the same region; an upper-case letter in
    // the regexp turns `case-fold-search' off (`search-upper-case').
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"aAa\\n\\nx\\n\") \
               (list (how-many \"a\" 1) (how-many \"A\" 1) (how-many \"^$\" 1) (how-many \"a\" 4 1) \
                     (count-matches \"x*\" 1)))"
        ),
        "(3 1 2 3 7)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello world-x foo\") (goto-char 8) \
               (list (current-word) (current-word t t)))"
        ),
        "(\"world-x\" \"world\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello   world\") (goto-char 7) (list (current-word) (current-word t)))"),
        "(\"hello\" nil)"
    );
}
