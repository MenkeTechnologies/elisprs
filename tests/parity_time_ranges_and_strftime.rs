//! Time values far from the epoch, and `format-time-string` against gnulib's
//! `nstrftime`: year directives (`%C %y %G %g %Y %F`) on negative and
//! five-digit years, flags and widths, bad-format recovery, and `%N`.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form: `=` and the printed value, or `!` and the printed error
//! object.

use elisprs::{eval_str, print, reset_host};

fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(let ((print-escape-newlines t) (print-circle t)) \
           (condition-case e (eval '{form} t) \
             (:success (concat \"=\" (prin1-to-string e))) \
             (error (concat \"!\" (prin1-to-string e)))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}

#[test]
fn encode_and_decode_reach_the_whole_tm_year_range() {
    check(
        r#"(decode-time -5000000000 t)"#,
        r#"=(40 6 15 23 7 1811 2 nil 0)"#,
    );
    check(
        r#"(decode-time -50000000000 t)"#,
        r#"=(40 6 7 25 7 385 4 nil 0)"#,
    );
    check(
        r#"(decode-time -500000000000 t)"#,
        r#"=(40 6 23 18 8 -13875 6 nil 0)"#,
    );
    check(
        r#"(decode-time -5000000000000 t)"#,
        r#"=(40 6 15 23 4 -156474 5 nil 0)"#,
    );
    check(
        r#"(decode-time 5000000000000 t)"#,
        r#"=(20 53 8 10 9 160413 2 nil 0)"#,
    );
    check(
        r#"(decode-time 50000000000000 t)"#,
        r#"=(20 53 16 4 12 1586406 1 nil 0)"#,
    );
    check(
        r#"(decode-time 67768036191676799 t)"#,
        r#"=(59 59 23 31 12 2147485547 3 nil 0)"#,
    );
    check(
        r#"(decode-time 67768036191676800 t)"#,
        r#"!(error "Specified time is not representable")"#,
    );
    check(
        r#"(decode-time -67768040609740800 t)"#,
        r#"=(0 0 0 1 1 -2147481748 4 nil 0)"#,
    );
    check(
        r#"(decode-time most-positive-fixnum t)"#,
        r#"!(error "Specified time is not representable")"#,
    );
    check(
        r#"(decode-time 1e18 t)"#,
        r#"!(error "Specified time is not representable")"#,
    );
    check(
        r#"(decode-time (expt 2 62) t)"#,
        r#"!(error "Specified time is not representable")"#,
    );
    check(
        r#"(decode-time (expt 2 80) t)"#,
        r#"!(error "Specified time is not representable")"#,
    );
    check(
        r#"(decode-time (encode-time '(1 2 3 4 5 1500 nil -1 t)) t)"#,
        r#"=(1 2 3 4 5 1500 5 nil 0)"#,
    );
    check(
        r#"(encode-time '(1 2 3 4 5 1500 nil -1 t))"#,
        r#"=(-226153 31529)"#,
    );
    check(
        r#"(encode-time '(1 2 3 4 5 1500 nil -1 0))"#,
        r#"=(-226153 31529)"#,
    );
    check(
        r#"(encode-time '(1 2 3 4 5 1500 nil -1 3600))"#,
        r#"=(-226153 27929)"#,
    );
    check(
        r#"(encode-time '(100 100 100 100 100 2024 nil -1 t))"#,
        r#"=(30110 21140)"#,
    );
    check(
        r#"(encode-time '(0 0 0 31 2 2024 nil -1 t))"#,
        r#"=(26082 27648)"#,
    );
    check(
        r#"(encode-time '(0 0 0 0 0 2024 nil -1 t))"#,
        r#"=(25959 53376)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 13 2024 nil -1 t))"#,
        r#"=(26484 34176)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 -1 2024 nil -1 t))"#,
        r#"=(25921 38144)"#,
    );
    check(
        r#"(encode-time '(0 0 0 -5 1 2024 nil -1 t))"#,
        r#"=(25994 6016)"#,
    );
    check(
        r#"(encode-time '(-1 -1 -1 1 1 1970 nil -1 t))"#,
        r#"=(-1 61875)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 1 2147483647 nil -1 t))"#,
        r#"=(1034057254058 51712)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 1 2147485547 nil -1 t))"#,
        r#"=(1034058168947 30208)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 1 -2147481748 nil -1 t))"#,
        r#"=(-1034058236843 2048)"#,
    );
    check(
        r#"(encode-time '(0 0 0 1 1 100000000 nil -1 t))"#,
        r#"=(48151138805 56320)"#,
    );
    check(r#"(encode-time 0 0 0 1 1 1500 t)"#, r#"=(-226315 10240)"#);
    check(
        r#"(encode-time '(0 0 0 1 1 1500 nil -1 t))"#,
        r#"=(-226315 10240)"#,
    );
    check(
        r#"(encode-time '((1 . 2) 0 0 1 1 1500 nil -1 t))"#,
        r#"=(-29663539199 . 2)"#,
    );
    check(
        r#"(encode-time '(1.5 0 0 1 1 1500 nil -1 t))"#,
        r#"=(-66796352037049651068665856 . 4503599627370496)"#,
    );
}

#[test]
fn year_directives_follow_the_gnulib_sign_and_width_rules() {
    check(
        r#"(format-time-string "%C" (encode-time (list 7 8 9 3 2 -1000 nil -1 t)) t)"#,
        r#"="-10""#,
    );
    check(
        r#"(format-time-string "%C" (encode-time (list 7 8 9 3 2 1970 nil -1 t)) t)"#,
        r#"="19""#,
    );
    check(
        r#"(format-time-string "%y" (encode-time (list 7 8 9 3 2 -101 nil -1 t)) t)"#,
        r#"="01""#,
    );
    check(
        r#"(format-time-string "%y" (encode-time (list 7 8 9 3 2 2000 nil -1 t)) t)"#,
        r#"="00""#,
    );
    check(
        r#"(format-time-string "%G" (encode-time (list 7 8 9 3 2 -1 nil -1 t)) t)"#,
        r#"="-001""#,
    );
    check(
        r#"(format-time-string "%G" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="2024""#,
    );
    check(
        r#"(format-time-string "%g" (encode-time (list 7 8 9 3 2 0 nil -1 t)) t)"#,
        r#"="00""#,
    );
    check(
        r#"(format-time-string "%g" (encode-time (list 7 8 9 3 2 9999 nil -1 t)) t)"#,
        r#"="99""#,
    );
    check(
        r#"(format-time-string "%Y" (encode-time (list 7 8 9 3 2 1 nil -1 t)) t)"#,
        r#"="0001""#,
    );
    check(
        r#"(format-time-string "%Y" (encode-time (list 7 8 9 3 2 10000 nil -1 t)) t)"#,
        r#"="10000""#,
    );
    check(
        r#"(format-time-string "%5C" (encode-time (list 7 8 9 3 2 100 nil -1 t)) t)"#,
        r#"="00001""#,
    );
    check(
        r#"(format-time-string "%5C" (encode-time (list 7 8 9 3 2 12345 nil -1 t)) t)"#,
        r#"="00123""#,
    );
    check(
        r#"(format-time-string "%05C" (encode-time (list 7 8 9 3 2 1969 nil -1 t)) t)"#,
        r#"="00019""#,
    );
    check(
        r#"(format-time-string "%-C" (encode-time (list 7 8 9 3 2 -1000 nil -1 t)) t)"#,
        r#"="-10""#,
    );
    check(
        r#"(format-time-string "%-C" (encode-time (list 7 8 9 3 2 1970 nil -1 t)) t)"#,
        r#"="19""#,
    );
    check(
        r#"(format-time-string "%4y" (encode-time (list 7 8 9 3 2 -101 nil -1 t)) t)"#,
        r#"="0001""#,
    );
    check(
        r#"(format-time-string "%4y" (encode-time (list 7 8 9 3 2 2000 nil -1 t)) t)"#,
        r#"="0000""#,
    );
    check(
        r#"(format-time-string "%3G" (encode-time (list 7 8 9 3 2 -1 nil -1 t)) t)"#,
        r#"="-01""#,
    );
    check(
        r#"(format-time-string "%3G" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="2024""#,
    );
    check(
        r#"(format-time-string "%-Y" (encode-time (list 7 8 9 3 2 0 nil -1 t)) t)"#,
        r#"="0""#,
    );
    check(
        r#"(format-time-string "%-Y" (encode-time (list 7 8 9 3 2 9999 nil -1 t)) t)"#,
        r#"="9999""#,
    );
    check(
        r#"(format-time-string "%10Y" (encode-time (list 7 8 9 3 2 1 nil -1 t)) t)"#,
        r#"="0000000001""#,
    );
    check(
        r#"(format-time-string "%10Y" (encode-time (list 7 8 9 3 2 10000 nil -1 t)) t)"#,
        r#"="0000010000""#,
    );
    check(
        r#"(format-time-string "%^Y" (encode-time (list 7 8 9 3 2 100 nil -1 t)) t)"#,
        r#"="0100""#,
    );
    check(
        r#"(format-time-string "%^Y" (encode-time (list 7 8 9 3 2 12345 nil -1 t)) t)"#,
        r#"="12345""#,
    );
    check(
        r#"(format-time-string "%F" (encode-time (list 7 8 9 3 2 1969 nil -1 t)) t)"#,
        r#"="1969-02-03""#,
    );
    check(
        r#"(format-time-string "%-5Y" (encode-time (list 7 8 9 3 2 -1000 nil -1 t)) t)"#,
        r#"="-1000""#,
    );
    check(
        r#"(format-time-string "%-5Y" (encode-time (list 7 8 9 3 2 1970 nil -1 t)) t)"#,
        r#"="1970""#,
    );
}

#[test]
fn bad_and_modified_directives_are_copied_through_with_padding() {
    check(
        r#"(format-time-string "%5%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="   %5%""#,
    );
    check(r#"(format-time-string "%5%" 0 t)"#, r#"="   %5%""#);
    check(
        r#"(format-time-string "%-5%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%-5%""#,
    );
    check(r#"(format-time-string "%-5%" 0 t)"#, r#"="%-5%""#);
    check(
        r#"(format-time-string "%05%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="00%05%""#,
    );
    check(r#"(format-time-string "%05%" 0 t)"#, r#"="00%05%""#);
    check(
        r#"(format-time-string "%_5%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="  %_5%""#,
    );
    check(r#"(format-time-string "%_5%" 0 t)"#, r#"="  %_5%""#);
    check(
        r#"(format-time-string "%5n" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="    \n""#,
    );
    check(r#"(format-time-string "%5n" 0 t)"#, r#"="    \n""#);
    check(
        r#"(format-time-string "%5t" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="    	""#,
    );
    check(r#"(format-time-string "%5t" 0 t)"#, r#"="    	""#);
    check(
        r#"(format-time-string "%Ez" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="+0000""#,
    );
    check(r#"(format-time-string "%Ez" 0 t)"#, r#"="+0000""#);
    check(
        r#"(format-time-string "%Oz" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="+0000""#,
    );
    check(r#"(format-time-string "%Oz" 0 t)"#, r#"="+0000""#);
    check(
        r#"(format-time-string "%-Ey" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="24""#,
    );
    check(r#"(format-time-string "%-Ey" 0 t)"#, r#"="70""#);
    check(
        r#"(format-time-string "%Q" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%Q""#,
    );
    check(r#"(format-time-string "%Q" 0 t)"#, r#"="%Q""#);
    check(
        r#"(format-time-string "%5Q" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="  %5Q""#,
    );
    check(r#"(format-time-string "%5Q" 0 t)"#, r#"="  %5Q""#);
    check(
        r#"(format-time-string "%L" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%L""#,
    );
    check(r#"(format-time-string "%L" 0 t)"#, r#"="%L""#);
    check(
        r#"(format-time-string "%1%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%1%""#,
    );
    check(r#"(format-time-string "%1%" 0 t)"#, r#"="%1%""#);
    check(
        r#"(format-time-string "%-%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%-%""#,
    );
    check(r#"(format-time-string "%-%" 0 t)"#, r#"="%-%""#);
    check(
        r#"(format-time-string "%^%" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="%^%""#,
    );
    check(r#"(format-time-string "%^%" 0 t)"#, r#"="%^%""#);
    check(
        r#"(format-time-string "%Ey" (encode-time (list 7 8 9 3 2 2024 nil -1 t)) t)"#,
        r#"="24""#,
    );
    check(r#"(format-time-string "%Ey" 0 t)"#, r#"="70""#);
}
