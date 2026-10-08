//! `max` / `min` are data.c `minmax_driver`: every argument is coerced by
//! `check_number_coerce_marker`, and a NaN met after the first argument is
//! returned at once, before the remaining arguments are type-checked.
//!
//! Expectations are GNU Emacs 31.1 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn a_marker_answers_as_its_position() {
    check(
        "(with-temp-buffer (insert \"ab\") (list (max (point-marker)) (min (point-marker) 5) \
         (max 1 (copy-marker 2)) (apply #'max (list (point-marker)))))",
        "(3 3 2 3)",
    );
}

#[test]
fn a_later_nan_returns_before_the_rest_is_checked() {
    check(
        "(list (condition-case e (max 1 0.0e+NaN \"x\") (error e)) \
         (condition-case e (apply #'min (list 1 0.0e+NaN 'a)) (error e)) \
         (seq-min (list 0.5 0.0e+NaN 'a)))",
        "(0.0e+NaN 0.0e+NaN 0.0e+NaN)",
    );
    // A NaN in FIRST position is only the accumulator: the next argument is
    // still checked.
    check(
        "(condition-case e (min 0.0e+NaN 'a) (error e))",
        "(wrong-type-argument number-or-marker-p a)",
    );
    check(
        "(list (max 0.0e+NaN 1) (min 1 0.0e+NaN 0) (max 1 2.0) (min 1.0 1) (max 1 1.0))",
        "(0.0e+NaN 0.0e+NaN 2.0 1.0 1)",
    );
}
