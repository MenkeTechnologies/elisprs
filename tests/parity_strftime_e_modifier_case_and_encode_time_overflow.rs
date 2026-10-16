//! `format-time-string`'s `#` flag on the invalid `%Eb` / `%Eh`, and
//! `encode-time` fields whose carry pushes the year out of an `int`.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value, or `!` and the
//! printed error object.

use elisprs::{eval_str, print, reset_host};

fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(condition-case e (eval '{form} t) (:success (concat \"=\" (prin1-to-string e))) \
         (error (concat \"!\" (prin1-to-string e))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}

/// gnulib applies the `#` flag to `%b`/`%h` before rejecting the `E` modifier,
/// so the literal it copies out is upcased; `%EB` rejects first, `%Ea` rejects
/// before the flag is looked at.
#[test]
fn hash_flag_upcases_the_rejected_e_month_abbreviation() {
    check(
        r#"(mapcar (lambda (f) (format-time-string f 0 t))
                    '("%#Eb" "%#Eh" "%#EB" "%#Ea" "%Eb" "%^Eb" "%#8Eb"))"#,
        r##"=("%#EB" "%#EH" "%#EB" "%#Ea" "%Eb" "%^EB" "   %#8EB")"##,
    );
}

/// The normalised year (after carrying months, days, hours...) has to fit
/// `tm_year`; `mktime_z` fails with EOVERFLOW otherwise.
#[test]
fn encode_time_carry_out_of_the_year_range_is_not_representable() {
    let overflow = r#"(error "Specified time is not representable")"#;
    for fields in [
        "0 0 0 1 13 2147485547",
        "0 0 0 32 12 2147485547",
        "0 0 24 31 12 2147485547",
        "0 0 0 1000 1 2147485547",
        "0 0 100000000 1 1 2147485547",
        "0 0 0 1 -1 -2147481748",
        "0 0 0 1 0 -2147481748",
    ] {
        check(
            &format!("(condition-case e (encode-time '({fields} nil -1 3600)) (error e))"),
            &format!("={overflow}"),
        );
    }
    // The boundary itself is fine.
    check(
        "(encode-time '(0 0 0 31 12 2147485547 nil -1 3600))",
        "=(1034058169427 18928)",
    );
    check(
        "(encode-time '(0 0 0 1 1 -2147481748 nil -1 3600))",
        "=(-1034058236844 63984)",
    );
}
