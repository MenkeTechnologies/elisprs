//! Libraries bundled verbatim from the Emacs tree and loaded on demand
//! (`parse-time`, `iso8601`, `char-fold`), the `autoload` call path that
//! reaches them, and `time-date.el`'s functions.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value, or `!` and the
//! printed error object.

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
fn an_autoload_object_loads_its_file_on_the_first_call() {
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (zzfoo))"#,
        r#"!(file-missing "Cannot open load file" "No such file or directory" "zzbar")"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (funcall 'zzfoo 1))"#,
        r#"!(file-missing "Cannot open load file" "No such file or directory" "zzbar")"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (symbol-function 'zzfoo))"#,
        r#"=(autoload "zzbar" nil nil nil)"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar" "doc" t) (list (commandp 'zzfoo) (autoloadp (symbol-function 'zzfoo))))"#,
        r#"=(t t)"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (fboundp 'zzfoo))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (condition-case e (apply 'zzfoo nil) (error e)))"#,
        r#"=(file-missing "Cannot open load file" "No such file or directory" "zzbar")"#,
    );
    check(
        r#"(progn (autoload 'zzfoo "zzbar") (mapcar 'zzfoo '(1)))"#,
        r#"!(file-missing "Cannot open load file" "No such file or directory" "zzbar")"#,
    );
    check(
        r#"(autoload-do-load '(autoload "zzbar" nil nil nil) 'zzfoo)"#,
        r#"!(file-missing "Cannot open load file" "No such file or directory" "zzbar")"#,
    );
    check(r#"(autoload-do-load 1 'zzfoo)"#, r#"=1"#);
}

#[test]
fn parse_time_string_reads_the_stock_formats() {
    check(
        r#"(parse-time-string "2024-03-05T10:20:30Z")"#,
        r#"=(30 20 10 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(parse-time-string "2024-065")"#,
        r#"=(nil nil nil 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "2024")"#,
        r#"=(nil nil nil nil nil 2024 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "T10:20")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "2024-03-05T10:20:30,5+05:30")"#,
        r#"=(30 20 10 5 3 2024 nil -1 19800)"#,
    );
    check(
        r#"(parse-time-string "2024-W53-7")"#,
        r#"=(nil nil nil 5 1 2025 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "P1Y2M3DT4H5M6S")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "P-1D")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "2024-01-01/P1M")"#,
        r#"=(nil nil nil 1 1 2024 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "R5/2024-01-01/P1D")"#,
        r#"=(nil nil nil 1 1 2024 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "2024-03-05T10:20:30-08:00")"#,
        r#"=(30 20 10 5 3 2024 nil -1 -28800)"#,
    );
    check(
        r#"(parse-time-string "1999-12-31T23:59:60Z")"#,
        r#"=(60 59 23 31 12 1999 nil nil 0)"#,
    );
    check(
        r#"(parse-time-string "12:30")"#,
        r#"=(0 30 12 nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "1230")"#,
        r#"=(nil nil nil nil nil 1230 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "202403")"#,
        r#"=(nil nil nil nil nil 202403 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "Mon, 5 Mar 2024 10:20:30 +0100")"#,
        r#"=(30 20 10 5 3 2024 1 -1 3600)"#,
    );
    check(
        r#"(parse-time-string "Jan 3, 2023")"#,
        r#"=(nil nil nil 3 1 2023 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "12am")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "Sat, 01 Jan 2000 00:00:00 GMT")"#,
        r#"=(0 0 0 1 1 2000 6 nil 0)"#,
    );
    check(
        r#"(parse-time-string "29/02/2024")"#,
        r#"=(nil nil nil 29 nil 2002 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "Wed, 30 Feb 2023")"#,
        r#"=(nil nil nil 30 2 2023 3 -1 nil)"#,
    );
    check(
        r#"(parse-time-string "10:20 CET")"#,
        r#"=(0 20 10 nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "10:20 UT")"#,
        r#"=(0 20 10 nil nil nil nil nil 0)"#,
    );
    check(
        r#"(parse-time-string "noon")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "'23")"#,
        r#"=(nil nil nil 23 nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "2024-03-05 10:20:30.5")"#,
        r#"=(30 20 10 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "tuesday 12:00")"#,
        r#"=(0 0 12 nil nil nil 2 -1 nil)"#,
    );
    check(
        r#"(parse-time-string "a b c")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(parse-time-string "1e3")"#,
        r#"=(nil nil nil nil nil nil nil -1 nil)"#,
    );
}

#[test]
fn date_to_time_with_an_explicit_zone() {
    check(
        r#"(date-to-time "2024-03-05T10:20:30Z")"#,
        r#"=(26086 61934)"#,
    );
    check(
        r#"(date-to-time "20240305T102030+0100")"#,
        r#"=(26086 58334)"#,
    );
    check(
        r#"(date-to-time "2024-03-05T10:20:30.123Z")"#,
        r#"=(26086 61934)"#,
    );
    check(
        r#"(date-to-time "2024-03-05T24:00:00Z")"#,
        r#"=(26087 45568)"#,
    );
    check(
        r#"(date-to-time "2024-03-05T10:20:30+0530")"#,
        r#"=(26086 42134)"#,
    );
    check(r#"(date-to-time "2024-03-05T10Z")"#, r#"=(26086 60704)"#);
    check(
        r#"(date-to-time "1999-12-31T23:59:60Z")"#,
        r#"=(14445 17280)"#,
    );
    check(r#"(date-to-time "Z")"#, r#"!(error "Invalid date: Z")"#);
    check(
        r#"(date-to-time "Mon, 5 Mar 2024 10:20:30 +0100")"#,
        r#"=(26086 58334)"#,
    );
    check(
        r#"(date-to-time "Tue Jan  3 12:00:00 PST 2023")"#,
        r#"=(25524 35136)"#,
    );
    check(
        r#"(date-to-time "Sat, 01 Jan 2000 00:00:00 GMT")"#,
        r#"=(14445 17280)"#,
    );
    check(
        r#"(date-to-time "2000-01-01T00:00:00-0500")"#,
        r#"=(14445 35280)"#,
    );
    check(
        r#"(progn (setenv "TZ" "EST5") (date-to-time "Thu, 1 Jan 70 00:00:00 UTC"))"#,
        r#"=(0 18000)"#,
    );
    check(
        r#"(date-to-time "10:20 EST")"#,
        r#"!(error "Invalid date: 10:20 EST")"#,
    );
    check(
        r#"(date-to-time "10:20 EDT")"#,
        r#"!(error "Invalid date: 10:20 EDT")"#,
    );
    check(
        r#"(date-to-time "10:20 CET")"#,
        r#"!(error "Invalid date: 10:20 CET")"#,
    );
    check(
        r#"(date-to-time "10:20 +0530")"#,
        r#"!(error "Invalid date: 10:20 +0530")"#,
    );
    check(
        r#"(date-to-time "10:20 -0800")"#,
        r#"!(error "Invalid date: 10:20 -0800")"#,
    );
    check(
        r#"(date-to-time "10:20 Z")"#,
        r#"!(error "Invalid date: 10:20 Z")"#,
    );
}

#[test]
fn iso8601_parsers_follow_the_stock_library() {
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10:20:30Z"))"#,
        r#"=(30 20 10 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-03-05T10:20:30Z"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10:20:30Z" 'end))"#,
        r#"=(30 20 10 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "20240305T102030+0100"))"#,
        r#"!(wrong-type-argument "20240305T102030+0100")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "20240305T102030+0100" 'start))"#,
        r#"=(30 20 10 5 3 2024 nil -1 3600)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024-W10-2"))"#,
        r#"!(wrong-type-argument "2024-W10-2")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024-W10-2"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024-065"))"#,
        r#"=(nil nil nil 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024-065"))"#,
        r#"!(wrong-type-argument "2024-065")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05"))"#,
        r#"=(nil nil nil 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-03-05"))"#,
        r#"!(wrong-type-argument "2024-03-05")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05" 'end))"#,
        r#"=(nil nil nil 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "2024-03"))"#,
        r#"!(wrong-type-argument "2024-03")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03" 'start))"#,
        r#"=(nil nil nil nil 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024"))"#,
        r#"=(0 24 20 nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "20240305"))"#,
        r#"=(nil nil nil 5 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "20240305"))"#,
        r#"!(wrong-type-argument "20240305")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "10:20:30"))"#,
        r#"!(wrong-type-argument "10:20:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "10:20:30"))"#,
        r#"!(wrong-type-argument "10:20:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "10:20:30" 'end))"#,
        r#"!(wrong-type-argument "10:20:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "T10:20"))"#,
        r#"!(wrong-type-argument "T10:20")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "T10:20" 'start))"#,
        r#"!(wrong-type-argument "T10:20")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "T1020"))"#,
        r#"!(wrong-type-argument "T1020")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "T1020"))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024-03-05T10:20:30.123Z"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30.123Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024-03-05T10:20:30.123Z"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30.123Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10:20:30,5+05:30"))"#,
        r#"=(30 20 10 5 3 2024 nil -1 19800)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-03-05T10:20:30,5+05:30"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30,5+05:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10:20:30,5+05:30" 'end))"#,
        r#"=(30 20 10 5 3 2024 nil -1 19800)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "2024-03-05 10:20:30"))"#,
        r#"!(wrong-type-argument "2024-03-05 10:20:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05 10:20:30" 'start))"#,
        r#"!(wrong-type-argument "2024-03-05 10:20:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024-13-45"))"#,
        r#"!(wrong-type-argument "2024-13-45")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024-13-45"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024-W53-7"))"#,
        r#"=(nil nil nil 5 1 2025 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024-W53-7"))"#,
        r#"!(wrong-type-argument "2024-W53-7")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "W10"))"#,
        r#"!(wrong-type-argument "W10")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "W10"))"#,
        r#"!(wrong-type-argument "W10")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "W10" 'end))"#,
        r#"!(wrong-type-argument "W10")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "2024-W10"))"#,
        r#"!(wrong-type-argument "2024-W10")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-W10" 'start))"#,
        r#"=(nil nil nil 3 3 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "P1Y2M3DT4H5M6S"))"#,
        r#"!(wrong-type-argument "P1Y2M3DT4H5M6S")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "P1Y2M3DT4H5M6S"))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "P1W"))"#,
        r#"!(wrong-type-argument "P1W")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "P1W"))"#,
        r#"!(wrong-type-argument "P1W")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "PT0.5S"))"#,
        r#"!(wrong-type-argument "PT0.5S")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "PT0.5S"))"#,
        r#"!(wrong-type-argument "PT0.5S")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "PT0.5S" 'end))"#,
        r#"!(wrong-type-argument "PT0.5S")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "P-1D"))"#,
        r#"!(wrong-type-argument "P-1D")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "P-1D" 'start))"#,
        r#"!(wrong-type-argument "P-1D")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "P1.5D"))"#,
        r#"!(wrong-type-argument "P1.5D")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "P1.5D"))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024-01-01/2024-02-01"))"#,
        r#"!(wrong-type-argument "2024-01-01/2024-02-01")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024-01-01/2024-02-01"))"#,
        r#"=((nil nil nil 1 1 2024 nil -1 nil) (nil nil nil 1 2 2024 nil -1 nil) (0 0 0 1 2 1970 0 nil 0))"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-01-01/P1M"))"#,
        r#"!(wrong-type-argument "2024-01-01/P1M")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-01-01/P1M"))"#,
        r#"!(wrong-type-argument "2024-01-01/P1M")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-01-01/P1M" 'end))"#,
        r#"!(wrong-type-argument "2024-01-01/P1M")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "P1M/2024-02-01"))"#,
        r#"!(wrong-type-argument "P1M/2024-02-01")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "P1M/2024-02-01" 'start))"#,
        r#"!(wrong-type-argument "P1M/2024-02-01")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024-01-01T00:00/P1DT2H"))"#,
        r#"!(wrong-type-argument "2024-01-01T00:00/P1DT2H")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024-01-01T00:00/P1DT2H"))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "R5/2024-01-01/P1D"))"#,
        r#"!(wrong-type-argument "R5/2024-01-01/P1D")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "R5/2024-01-01/P1D"))"#,
        r#"!(wrong-type-argument "R5/2024-01-01/P1D")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T24:00:00Z"))"#,
        r#"=(0 0 24 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-03-05T24:00:00Z"))"#,
        r#"!(wrong-type-argument "2024-03-05T24:00:00Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T24:00:00Z" 'end))"#,
        r#"=(0 0 24 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "--03-05"))"#,
        r#"!(wrong-type-argument "--03-05")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "--03-05" 'start))"#,
        r#"=(nil nil nil 5 3 nil nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024-03-05T10:20:30-08:00"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30-08:00")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024-03-05T10:20:30-08:00"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024-03-05T10:20:30+0530"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30+0530")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024-03-05T10:20:30+0530"))"#,
        r#"!(wrong-type-argument "2024-03-05T10:20:30+0530")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10Z"))"#,
        r#"=(0 0 10 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-03-05T10Z"))"#,
        r#"!(wrong-type-argument "2024-03-05T10Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-03-05T10Z" 'end))"#,
        r#"=(0 0 10 5 3 2024 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "1999-12-31T23:59:60Z"))"#,
        r#"!(wrong-type-argument "1999-12-31T23:59:60Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "1999-12-31T23:59:60Z" 'start))"#,
        r#"=(60 59 23 31 12 1999 nil nil 0)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "0000-01-01"))"#,
        r#"!(wrong-type-argument "0000-01-01")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "0000-01-01"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "9999-12-31"))"#,
        r#"=(nil nil nil 31 12 9999 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "9999-12-31"))"#,
        r#"!(wrong-type-argument "9999-12-31")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "12:30"))"#,
        r#"!(wrong-type-argument "12:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "12:30"))"#,
        r#"!(wrong-type-argument "12:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "12:30" 'end))"#,
        r#"!(wrong-type-argument "12:30")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "12:30:45.5"))"#,
        r#"!(wrong-type-argument "12:30:45.5")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "12:30:45.5" 'start))"#,
        r#"!(wrong-type-argument "12:30:45.5")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "12"))"#,
        r#"=(0 0 12 nil nil nil nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "12"))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "1230"))"#,
        r#"=(nil nil nil nil nil 1230 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "1230"))"#,
        r#"!(wrong-type-argument "1230")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "T12"))"#,
        r#"!(wrong-type-argument "T12")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "T12"))"#,
        r#"!(wrong-type-argument "T12")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "T12" 'end))"#,
        r#"!(wrong-type-argument "T12")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "Z"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "Z" 'start))"#,
        r#"!(wrong-type-argument "Z")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time ""))"#,
        r#"!(wrong-type-argument "")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p ""))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "x"))"#,
        r#"!(wrong-type-argument "x")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "x"))"#,
        r#"!(wrong-type-argument "x")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-3-5"))"#,
        r#"!(wrong-type-argument "2024-3-5")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-duration "2024-3-5"))"#,
        r#"!(wrong-type-argument "2024-3-5")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "2024-3-5" 'end))"#,
        r#"!(wrong-type-argument "2024-3-5")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-zone "202403"))"#,
        r#"!(wrong-type-argument "202403")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse "202403" 'start))"#,
        r#"!(wrong-type-argument "202403")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-time "2024-060"))"#,
        r#"!(wrong-type-argument "2024-060")"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-valid-p "2024-060"))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-date "2024060"))"#,
        r#"=(nil nil nil 29 2 2024 nil -1 nil)"#,
    );
    check(
        r#"(progn (require 'iso8601) (iso8601-parse-interval "2024060"))"#,
        r#"!(wrong-type-argument "2024060")"#,
    );
}

#[test]
fn char_fold_to_regexp_uses_the_stock_equivalence_table() {
    check(
        r#"(char-fold-to-regexp "abc")"#,
        r#"="\\(?:a[̀-̄̆-̨̣̥̊̌̏̑]\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\)\\(?:b[̣̱̇]\\|[bᵇḃḅḇⓑｂ𝐛𝑏𝒃𝒷𝓫𝔟𝕓𝖇𝖻𝗯𝘣𝙗𝚋]\\)\\(?:c[̧́̂̇̌]\\|[cçćĉċčᶜḉⅽⓒｃ𝐜𝑐𝒄𝒸𝓬𝔠𝕔𝖈𝖼𝗰𝘤𝙘𝚌]\\)""#,
    );
    check(r#"(char-fold-to-regexp "é")"#, r#"="\\(?:é\\|é\\)""#);
    check(
        r#"(char-fold-to-regexp "a b")"#,
        r#"="\\(?:a[̀-̄̆-̨̣̥̊̌̏̑]\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\)\\(?: \\|\\(?: \\(?:ٌّ\\|ٍّ\\|َّ\\|ُّ\\|ِّ\\|ّٰ\\|[́̃-ً̧̨̳̈̊̋̓̔͂ͅ-゙゚ْ]\\)\\|[  ¨¯´¸˘-˝ͺ΄΅᾽᾿῀῁῍῎῏῝῞῟῭΅´῾ - ‗ ‾ 　゛゜ﱞ-ﱣ﹉-﹌ﹰﹲﹴﹶﹸﹺﹼﹾ￣]\\)\\)\\(?:b[̣̱̇]\\|[bᵇḃḅḇⓑｂ𝐛𝑏𝒃𝒷𝓫𝔟𝕓𝖇𝖻𝗯𝘣𝙗𝚋]\\)""#,
    );
    check(r#"(char-fold-to-regexp "")"#, r#"="""#);
    check(
        r#"(char-fold-to-regexp "f")"#,
        r#"="\\(?:ḟ\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "f" t)"#,
        r#"="\\(?:\\(?:ḟ\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\)\\|ﬄ\\|ﬃ\\|ﬂ\\|ﬁ\\|ﬀ\\|㎙\\|ḟ\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "fi")"#,
        r#"="\\(?:\\(?:ḟ\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\|ﬁ\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "ffi" t)"#,
        r#"="\\(?:\\(?:ḟ\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\)\\(?:\\(?:ḟ\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\)\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\|ﬁ\\)\\|ﬀ\\(?:i[̀-̨̣̰̄̆̈̉̌̏̑]\\|[iì-ïĩīĭįǐȉȋᵢḭḯỉịⁱℹⅈⅰⓘｉ𝐢𝑖𝒊𝒾𝓲𝔦𝕚𝖎𝗂𝗶𝘪𝙞𝚒]\\)\\|ﬃ\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "1/2")"#,
        r#"="[1¹₁①１𜳱𝟏𝟙𝟣𝟭𝟷🯱][/／][2²₂②２𜳲𝟐𝟚𝟤𝟮𝟸🯲]""#,
    );
    check(
        r#"(char-fold-to-regexp "ss")"#,
        r#"="\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)\\(?:s[̧̣̦́̂̇̌]\\|[sśŝşšſșˢṡṣṥṧṩẛₛⓢﬅｓ𝐬𝑠𝒔𝓈𝓼𝔰𝕤𝖘𝗌𝘀𝘴𝙨𝚜]\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "  ")"#,
        r#"="\\(?:  \\|\\(?: \\(?:ٌّ\\|ٍّ\\|َّ\\|ُّ\\|ِّ\\|ّٰ\\|[́̃-ً̧̨̳̈̊̋̓̔͂ͅ-゙゚ْ]\\)\\|[  ¨¯´¸˘-˝ͺ΄΅᾽᾿῀῁῍῎῏῝῞῟῭΅´῾ - ‗ ‾ 　゛゜ﱞ-ﱣ﹉-﹌ﹰﹲﹴﹶﹸﹺﹼﹾ￣]\\)\\(?: \\(?:ٌّ\\|ٍّ\\|َّ\\|ُّ\\|ِّ\\|ّٰ\\|[́̃-ً̧̨̳̈̊̋̓̔͂ͅ-゙゚ْ]\\)\\|[  ¨¯´¸˘-˝ͺ΄΅᾽᾿῀῁῍῎῏῝῞῟῭΅´῾ - ‗ ‾ 　゛゜ﱞ-ﱣ﹉-﹌ﹰﹲﹴﹶﹸﹺﹼﹾ￣]\\)\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "a  b")"#,
        r#"="\\(?:a[̀-̄̆-̨̣̥̊̌̏̑]\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\)\\(?:  \\|\\(?: \\(?:ٌّ\\|ٍّ\\|َّ\\|ُّ\\|ِّ\\|ّٰ\\|[́̃-ً̧̨̳̈̊̋̓̔͂ͅ-゙゚ْ]\\)\\|[  ¨¯´¸˘-˝ͺ΄΅᾽᾿῀῁῍῎῏῝῞῟῭΅´῾ - ‗ ‾ 　゛゜ﱞ-ﱣ﹉-﹌ﹰﹲﹴﹶﹸﹺﹼﹾ￣]\\)\\(?: \\(?:ٌّ\\|ٍّ\\|َّ\\|ُّ\\|ِّ\\|ّٰ\\|[́̃-ً̧̨̳̈̊̋̓̔͂ͅ-゙゚ْ]\\)\\|[  ¨¯´¸˘-˝ͺ΄΅᾽᾿῀῁῍῎῏῝῞῟῭΅´῾ - ‗ ‾ 　゛゜ﱞ-ﱣ﹉-﹌ﹰﹲﹴﹶﹸﹺﹼﹾ￣]\\)\\)\\(?:b[̣̱̇]\\|[bᵇḃḅḇⓑｂ𝐛𝑏𝒃𝒷𝓫𝔟𝕓𝖇𝖻𝗯𝘣𝙗𝚋]\\)""#,
    );
    check(r#"(char-fold-to-regexp "日本")"#, r#"="[⽇㊐日]本""#);
    check(
        r#"(char-fold-to-regexp "\"")"#,
        r#"="[\"«»“-‟❝❞❠⹂〝〞〟＂🙶🙷🙸]""#,
    );
    check(r#"(char-fold-to-regexp "'")"#, r#"="['‘-‛‹›❛❜❟❮❯＇󠀢]""#);
    check(r#"(char-fold-to-regexp "ΐ")"#, r#"="\\(?:ΐ\\|[ΐΐ]\\)""#);
    check(
        r#"(char-fold-to-regexp "abc" nil 1)"#,
        r#"="\\(?:b[̣̱̇]\\|[bᵇḃḅḇⓑｂ𝐛𝑏𝒃𝒷𝓫𝔟𝕓𝖇𝖻𝗯𝘣𝙗𝚋]\\)\\(?:c[̧́̂̇̌]\\|[cçćĉċčᶜḉⅽⓒｃ𝐜𝑐𝒄𝒸𝓬𝔠𝕔𝖈𝖼𝗰𝘤𝙘𝚌]\\)""#,
    );
    check(
        r#"(char-fold-to-regexp "a." )"#,
        r#"="\\(?:a[̀-̄̆-̨̣̥̊̌̏̑]\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\)[.․︙︰﹒．]""#,
    );
    check(r#"(char-fold-to-regexp "\\")"#, r#"="[\\﹨＼]""#);
    check(
        r#"(char-fold-to-regexp 5)"#,
        r#"!(wrong-type-argument sequencep 5)"#,
    );
    check(r#"(string-match (char-fold-to-regexp "a") "ä")"#, r#"=0"#);
    check(r#"(string-match (char-fold-to-regexp "ä") "a")"#, r#"=nil"#);
    check(r#"(string-match (char-fold-to-regexp "ä") "ä")"#, r#"=0"#);
    check(r#"(string-match (char-fold-to-regexp "fi") "ﬁ")"#, r#"=0"#);
    check(
        r#"(string-match (char-fold-to-regexp "ﬁ") "fi")"#,
        r#"=nil"#,
    );
}

#[test]
fn format_seconds_floors_its_fraction() {
    check(r#"(format-seconds "%h:%m:%s" 3661)"#, r#"="1:1:1""#);
    check(
        r#"(format-seconds "%y %d %h %m %s" 100000000)"#,
        r#"="3 62 9 46 40""#,
    );
    check(
        r#"(format-seconds "%Y, %D, %H, %M, %z%S" 100000)"#,
        r#"="1 day, 3 hours, 46 minutes, 40 seconds""#,
    );
    check(r#"(format-seconds "%x%d" 0)"#, r#"="""#);
    check(r#"(format-seconds "%z%h" 0)"#, r#"="0""#);
    check(r#"(format-seconds "%.2h" 3600)"#, r#"="01""#);
    check(r#"(format-seconds "%d%%" 86400)"#, r#"="1%""#);
    check(r#"(format-seconds "%s" 1.5)"#, r#"="1""#);
    check(r#"(format-seconds "%S" 0.3)"#, r#"="0 seconds""#);
    check(r#"(format-seconds "%.2s" 1.5)"#, r#"="01""#);
    check(r#"(format-seconds "%.3s" 1.5)"#, r#"="001""#);
    check(r#"(format-seconds "%m" -61)"#, r#"="-1""#);
    check(
        r#"(format-seconds "%w" 8)"#,
        r#"!(error "Bad format specifier: ‘w’")"#,
    );
    check(r#"(format-seconds "%M" 61)"#, r#"="1 minute""#);
}
