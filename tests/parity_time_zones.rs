//! `ZONE` arguments against GNU Emacs 31.1's `tzlookup`: an integer is a fixed
//! offset named `+HH[MM[SS]]`, an `(OFFSET ABBR)` list names itself ABBR, and a
//! string is a TZ string that libc resolves (`"America/New_York"`, `"JST-9"`).
//!
//! Every expectation is the output of `emacs -Q --batch` for the same form, with
//! `=` and the printed value, or `!` and the printed error object. The named
//! zones depend on the tz database, which the headless Linux runners ship.

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

#[test]
fn integer_zone_is_named_by_its_offset() {
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 0)"#,
        r#"="00:00:00|UTC|+0000""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 1)"#,
        r#"="00:00:01|+000001|+0000""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 60)"#,
        r#"="00:01:00|+0001|+0001""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 3600)"#,
        r#"="01:00:00|+01|+0100""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 3601)"#,
        r#"="01:00:01|+010001|+0100""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 5400)"#,
        r#"="01:30:00|+0130|+0130""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 -5400)"#,
        r#"="22:30:00|-0130|-0130""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 86400)"#,
        r#"="00:00:00|+24|+2400""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 360000)"#,
        r#"="04:00:00|+100|+10000""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 604799)"#,
        r#"="23:59:59|+1675959|+16759""#,
    );
    check(
        r#"(format-time-string "%H:%M:%S|%Z|%z" 0 604800)"#,
        r#"="00:00:00|UTC|+0000""#,
    );
    check(
        r#"(format-time-string "%:z|%::z|%:::z" 1720000000 19801)"#,
        r#"="+05:30|+05:30:01|+05:30:01""#,
    );
}

#[test]
fn list_zone_carries_offset_and_name() {
    check(
        r#"(format-time-string "%H|%Z|%z" 0 (list 3600 "X"))"#,
        r#"="01|X|+0100""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 0 (list 3600 ""))"#,
        r#"="01||+0100""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 0 (list 0 "X"))"#,
        r#"="00|X|+0000""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 0 '(3600 "X" 5))"#,
        r#"="01|X|+0100""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 0 (list 1000000 "X"))"#,
        r#"="00|UTC|+0000""#,
    );
    check(
        r#"(format-time-string "%H" 0 (list 1 2))"#,
        r#"!(wrong-type-argument stringp 2)"#,
    );
    check(
        r#"(format-time-string "%H" 0 (list 3600 nil))"#,
        r#"!(wrong-type-argument stringp nil)"#,
    );
    check(
        r#"(format-time-string "%H" 0 (list 3600))"#,
        r#"!(error "Invalid time zone specification" (3600))"#,
    );
    check(
        r#"(format-time-string "%H" 0 (list nil "X"))"#,
        r#"!(error "Invalid time zone specification" (nil "X"))"#,
    );
    check(
        r#"(format-time-string "%H" 0 (cons 3600 "X"))"#,
        r#"!(error "Invalid time zone specification" (3600 . "X"))"#,
    );
}

#[test]
fn string_zone_is_a_tz_string() {
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "EST5EDT")"#,
        r#"="17|EST|-0500""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "America/New_York")"#,
        r#"="17|EST|-0500""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1720000000 "America/New_York")"#,
        r#"="05|EDT|-0400""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "Asia/Tokyo")"#,
        r#"="07|JST|+0900""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "JST-9")"#,
        r#"="07|JST|+0900""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "<+03>-3")"#,
        r#"="01|+03|+0300""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "")"#,
        r#"="22|UTC|+0000""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 "NST3:30NDT")"#,
        r#"="18|NST|-0330""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1720000000 "Europe/London")"#,
        r#"="10|BST|+0100""#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 'utc)"#,
        r#"!(error "Invalid time zone specification" utc)"#,
    );
    check(
        r#"(format-time-string "%H|%Z|%z" 1700000000 1.5)"#,
        r#"!(error "Invalid time zone specification" 1.5)"#,
    );
}

#[test]
fn decode_and_encode_honour_the_zone() {
    check(
        r#"(decode-time 1700000000 "America/New_York")"#,
        r#"=(20 13 17 14 11 2023 2 nil -18000)"#,
    );
    check(
        r#"(decode-time 1720000000 "America/New_York")"#,
        r#"=(40 46 5 3 7 2024 3 t -14400)"#,
    );
    check(
        r#"(decode-time 1700000000 "Asia/Tokyo")"#,
        r#"=(20 13 7 15 11 2023 3 nil 32400)"#,
    );
    check(
        r#"(decode-time 1700000000 3600)"#,
        r#"=(20 13 23 14 11 2023 2 nil 3600)"#,
    );
    check(
        r#"(decode-time 1700000000 (list 7200 "XX"))"#,
        r#"=(20 13 0 15 11 2023 3 nil 7200)"#,
    );
    check(
        r#"(encode-time '(0 0 12 1 7 2000 nil -1 "America/New_York"))"#,
        r#"=(14686 5504)"#,
    );
    check(
        r#"(encode-time '(0 0 12 1 7 2000 nil -1 "Asia/Tokyo"))"#,
        r#"=(14685 24240)"#,
    );
    check(
        r#"(encode-time '(0 0 12 1 7 2000 nil -1 (3600 "X")))"#,
        r#"=(14685 53040)"#,
    );
    check(
        r#"(encode-time 0 0 12 1 7 2000 nil nil "Europe/London")"#,
        r#"=(14685 53040)"#,
    );
    check(
        r#"(current-time-string 1720000000 "Asia/Tokyo")"#,
        r#"="Wed Jul  3 18:46:40 2024""#,
    );
    check(
        r#"(current-time-string 1720000000 '(3600 "X"))"#,
        r#"="Wed Jul  3 10:46:40 2024""#,
    );
}
