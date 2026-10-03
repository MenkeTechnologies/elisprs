//! Round 33, second batch: timefns.c's exact Lisp timestamps, time-date.el,
//! `eval` of a `progn` one subform at a time, cl-macs.el's `cl-ecase`
//! diagnostics, `cl-copy-list` on a dotted list, `cl-no-applicable-method`'s
//! data, and files.el's `file-relative-name` / `file-name-sans-versions`.
//!
//! Every expectation was byte-checked against GNU Emacs 31.1
//! (`emacs -Q --batch`, `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// timefns.c `decode_float_time`: a float is its exact binary value over
/// `2^(52 - ilogb)`; `time-convert` keeps or rescales the resolution.
#[test]
fn time_convert_is_exact() {
    assert_eq!(
        eval(
            "(list (time-convert 2.0 t) (time-convert -1.5 'list) (time-convert '(0 1 500000) t) \
                   (time-convert 1.5 'list) (time-convert 7 3) (time-convert '(7 . 3) 3) \
                   (time-convert -1 'list) (time-convert '(-1 65535 999999 999999) t) \
                   (time-convert 1.5 'integer) (time-convert -1.5 'integer) (time-convert '(1 2 . 3) t))"
        ),
        "((4503599627370496 . 2251799813685248) (-1 65534 500000 0) (1500000 . 1000000) \
(0 1 500000 0) (21 . 3) (7 . 3) (-1 65535 0 0) (-1 . 1000000000000) 1 -2 (65538000003 . 1000000))"
    );
    assert_eq!(
        eval(
            "(list (condition-case e (time-convert 1 'foo) (error e)) \
                   (condition-case e (time-convert '(1 . 0)) (error e)) \
                   (condition-case e (time-add 0.0e+NaN 1) (error e)) \
                   (condition-case e (time-add 1.0e+INF 1) (error e)) \
                   (condition-case e (time-convert '(1 2 3.5)) (error e)))"
        ),
        "((error \"Invalid time frequency\" foo) (error \"Invalid time specification\") \
(error \"Invalid time specification\") (error \"Specified time is not representable\") \
(error \"Invalid time specification\"))"
    );
}

/// timefns.c `time_arith`: exact rational sums, the result's form following
/// the inputs' and its resolution never coarser than the finer input.
#[test]
fn time_arithmetic_is_exact() {
    assert_eq!(
        eval(
            "(list (time-add 1 2) (time-add 1.5 2) (time-add '(1 . 2) 1) (time-add '(0 1 5) 1) \
                   (time-add 0.1 0.2) (time-subtract '(0 10) 3) (time-subtract 5 5) \
                   (time-add '(1 . 3) '(1 . 6)) (time-add '(1 2 3) '(0 0 0 7)) \
                   (time-less-p '(1 . 0) '(2 . 0)) (time-equal-p nil nil) (time-equal-p 1 '(0 1)) \
                   (time-less-p 1 1.5))"
        ),
        "(3 (0 3 500000 0) (3 . 2) (0 2 5 0) (10808639105689191 . 36028797018963968) 7 (0 0 0 0) \
(2 . 4) (1 2 3 7) t t t t)"
    );
}

/// timefns.c `frac_to_double`: one correct rounding, ties to even.
#[test]
fn float_time_rounds_once() {
    assert_eq!(
        eval(
            "(list (float-time '(1 2 3 4)) (float-time '(-1 65535 999999 999999)) \
                   (float-time '(1 . 3)) (float-time '(9007199254740993 . 1)) \
                   (float-time '(-9007199254740995 . 2)) \
                   (condition-case e (float-time 'x) (error e)))"
        ),
        "(65538.00000300001 -1e-12 0.3333333333333333 9007199254740992.0 -4503599627370498.0 \
(error \"Invalid time specification\"))"
    );
}

/// `decode-time` with FORM t keeps the resolution in the seconds element;
/// `encode-time` accepts one there and answers `(TICKS . HZ)`.
#[test]
fn decode_and_encode_keep_subseconds() {
    assert_eq!(
        eval(
            "(list (decode-time 1.5 t t) (decode-time '(0 1 500000) t t) (decode-time '(-1 . 3) t t) \
                   (decode-time '(1 . 3) t 'integer))"
        ),
        "(((6755399441055744 . 4503599627370496) 0 0 1 1 1970 4 nil 0) \
((1500000 . 1000000) 0 0 1 1 1970 4 nil 0) ((179 . 3) 59 23 31 12 1969 3 nil 0) \
(0 0 0 1 1 1970 4 nil 0))"
    );
    assert_eq!(
        eval(
            "(list (encode-time '((3 . 2) 0 0 1 1 1970 nil nil t)) \
                   (encode-time '(1.5 0 0 1 1 1970 nil nil t)) \
                   (encode-time '(0 0 0 1 1 2000 4 nil 3600)) \
                   (condition-case e (encode-time 0 0 0) (error e)) \
                   (condition-case e (encode-time '(0 0 0)) (error e)) \
                   (condition-case e (encode-time '(0 0 0 1.5 1 2000)) (error e)))"
        ),
        "((3 . 2) (6755399441055744 . 4503599627370496) (14445 13680) \
(wrong-number-of-arguments encode-time 3) (wrong-type-argument consp nil) \
(wrong-type-argument fixnump 1.5))"
    );
}

/// time-date.el.
#[test]
fn time_date_helpers() {
    assert_eq!(
        eval(
            "(list (seconds-to-time 5) (seconds-to-time 1.5) (days-to-time 2) (days-to-time 0.5) \
                   (time-to-days 43200) (time-to-day-in-year 129600) (date-leap-year-p 1900) \
                   (date-days-in-month 2024 2) \
                   (condition-case e (date-days-in-month 2024 13) (error e)) \
                   (date-ordinal-to-time 2024 60) (time-to-number-of-days 86400))"
        ),
        "((0 5 0 0) (0 1 500000 0) (2 41728) (0 43200 0 0) 719163 2 nil 29 \
(error \"Month 13 is invalid\") (nil nil nil 29 2 2024 nil nil nil) 1.0)"
    );
}

/// eval.c `eval_sub` expands a `progn`'s subforms only as it reaches them, so
/// a struct or macro defined early in the body is usable later in it.
#[test]
fn eval_runs_progn_subforms_in_turn() {
    assert_eq!(
        eval(
            "(eval '(progn (cl-defstruct r33-pt x) \
                           (let ((p (make-r33-pt :x 1))) (setf (r33-pt-x p) 9) (cl-incf (r33-pt-x p)) p)) t)"
        ),
        "#s(r33-pt 10)"
    );
    assert_eq!(
        eval("(list (eval '(progn) t) (eval '(progn 1 2) t) (eval '(progn (defmacro r33-m () 7) (r33-m))))"),
        "(nil 2 7)"
    );
}

/// cl-macs.el's `cl--ecase-error-flag` clause, cl-lib.el `cl-copy-list`, and
/// cl-generic.el's default `cl-no-applicable-method`.
#[test]
fn cl_lib_diagnostics() {
    assert_eq!(
        eval(
            "(list (condition-case e (cl-ecase 9 ((1 2) 'a) (3 'b)) (error e)) \
                   (condition-case e (cl-etypecase \"x\" (integer 'i) ((or float null) 'f)) (error e)) \
                   (condition-case e (cl-ecase (list 1) (1 'a)) (error e)) \
                   (cl-ecase 3 ((1 2) 'a) (3 'b)) \
                   (cl-copy-list '(1 2 . 3)) (cl-copy-list nil) (cl-copy-list '(1)))"
        ),
        "((error \"cl-ecase failed: 9, (2 1 3)\") (error \"cl-etypecase failed: x, (integer (or float null))\") \
(error \"cl-ecase failed: (1), (1)\") b (1 2 . 3) nil (1))"
    );
    assert_eq!(
        eval("(progn (cl-defmethod r33-gl ((x symbol)) 'sym) (condition-case e (r33-gl 1) (error e)))"),
        "(cl-no-applicable-method r33-gl 1)"
    );
}

/// files.el.
#[test]
fn file_name_relatives_and_versions() {
    assert_eq!(
        eval(
            "(list (file-name-sans-versions \"foo.txt.~12~\") (file-name-sans-versions \"foo.txt.~12~\" t) \
                   (file-name-sans-versions \"foo~\") (file-relative-name \"/a/b/c\" \"/a/d\") \
                   (file-relative-name \"/a/b\" \"/a/b\") (file-relative-name \"/a/b/\" \"/a/b/c/d\"))"
        ),
        "(\"foo.txt\" \"foo.txt.~12~\" \"foo\" \"../b/c\" \".\" \"../../\")"
    );
}
