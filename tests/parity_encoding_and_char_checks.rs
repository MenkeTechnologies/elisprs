//! base64 / URL encoding, and the argument checks Emacs spells `characterp`
//! and `floatp`.
//!
//! Found by feeding the differential fuzzer (`scripts/fuzz_parity.sh -c`) a
//! corpus over the subrs the generated corpus never reaches. Every expectation
//! is `emacs -Q --batch --eval '(prin1 …)'` on GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Emacs encodes a string one *byte* per character. elisprs was handing the
/// UTF-8 expansion to the encoder, so `(base64-encode-string "\303\251")`
/// answered `"w4PCqQ=="` where Emacs answers `"w6k="` — and, worse, the value did
/// not round-trip through `base64-decode-string`, which already decoded one
/// character per byte.
#[test]
fn base64_encodes_characters_as_bytes() {
    assert_eq!(eval(r#"(base64-encode-string "\303\251")"#), "\"w6k=\"");
    assert_eq!(eval(r#"(base64-encode-string "abc")"#), "\"YWJj\"");
    assert_eq!(eval(r#"(base64-encode-string "")"#), "\"\"");
    // Round trip, checked on the character codes so the printer is not in the way.
    assert_eq!(
        eval(r#"(string-to-list (base64-decode-string (base64-encode-string "\303\251")))"#),
        "(195 169)"
    );
}

/// A character above 255 cannot be a byte, and Emacs refuses it rather than
/// encoding some expansion of it. Emacs 30.2:
/// `(error "Multibyte character in data for base64 encoding")`; elisprs answered
/// `"zrHOsw=="`.
#[test]
fn base64_refuses_a_character_that_is_not_a_byte() {
    assert_eq!(
        eval(r#"(condition-case e (base64-encode-string "αβγ") (error e))"#),
        "(error \"Multibyte character in data for base64 encoding\")"
    );
}

/// The padded decoder is strict. Reading the input as a loose bit stream
/// accepted everything below; Emacs signals for all of them.
#[test]
fn base64_decode_rejects_malformed_input() {
    for src in [
        r#"(base64-decode-string "YWJ")"#,   // not a whole quadruple
        r#"(base64-decode-string "YWJj=")"#, // five characters
        r#"(base64-decode-string "=")"#,     // padding only
        r#"(base64-decode-string "====")"#,  // a quadruple with no data
        r#"(base64-decode-string "A===")"#,  // too few bits for one byte
        r#"(base64-decode-string "AB=C")"#,  // data after padding
        r#"(base64-decode-string "-_-_")"#,  // the base64url alphabet
    ] {
        assert_eq!(
            eval(&format!("(condition-case e {src} (error e))")),
            "(error \"Invalid base64 data\")",
            "{src}"
        );
    }
}

/// …and accepts exactly what Emacs accepts, so the strictness cannot have been
/// bought by rejecting valid input. Whitespace is ignored anywhere, and a padded
/// quadruple may sit in the middle of the input.
#[test]
fn base64_decode_accepts_what_emacs_accepts() {
    assert_eq!(eval(r#"(base64-decode-string "YWJj")"#), "\"abc\"");
    assert_eq!(eval(r#"(base64-decode-string "YQ==")"#), "\"a\"");
    assert_eq!(eval(r#"(base64-decode-string "YWI=")"#), "\"ab\"");
    assert_eq!(eval(r#"(base64-decode-string "YQ==YQ==")"#), "\"aa\"");
    assert_eq!(eval(r#"(base64-decode-string "  YWJj  ")"#), "\"abc\"");
    assert_eq!(eval(r#"(base64-decode-string "")"#), "\"\"");
    // BASE64URL reading is the unpadded one, over the `-_` alphabet.
    assert_eq!(eval(r#"(base64-decode-string "YWJj" t)"#), "\"abc\"");
    assert_eq!(eval(r#"(base64-decode-string "YWJ" t)"#), "\"ab\"");
}

/// `url-unhex-string` hands back the bytes the escapes spelled and leaves any
/// decoding to the caller. elisprs re-assembled them into characters when they
/// happened to be valid UTF-8, so `%CE%B1` came back as one character (945)
/// instead of two (206 177) — a length difference, not just a printing one.
#[test]
fn url_unhex_string_yields_bytes() {
    assert_eq!(
        eval(r#"(string-to-list (url-unhex-string "%CE%B1"))"#),
        "(206 177)"
    );
    assert_eq!(eval(r#"(length (url-unhex-string "%CE%B1"))"#), "2");
    assert_eq!(eval(r#"(url-unhex-string "a%20b")"#), "\"a b\"");
    // A malformed escape is left alone, not consumed.
    assert_eq!(eval(r#"(url-unhex-string "%zz")"#), "\"%zz\"");
    assert_eq!(
        eval(r#"(string-to-list (url-unhex-string (url-hexify-string "αβ")))"#),
        "(206 177 206 178)"
    );
}

/// Emacs's `secure_hash` message for an algorithm it does not know.
#[test]
fn secure_hash_reports_emacs_message_for_an_unknown_algorithm() {
    assert_eq!(
        eval("(condition-case e (secure-hash 'bogus \"abc\") (error e))"),
        "(error \"Invalid algorithm arg: bogus\")"
    );
    // The supported ones still work — the message change is not a rejection.
    assert_eq!(
        eval("(secure-hash 'sha1 \"abc\")"),
        "\"a9993e364706816aba3e25717850c26c9cd0d89d\""
    );
}

/// `CHECK_CHARACTER`, not `CHECK_FIXNUM`: `(string "a")` is
/// `(wrong-type-argument characterp "a")` and an out-of-range integer is
/// rejected rather than silently dropped.
#[test]
fn string_checks_characters() {
    assert_eq!(
        eval(r#"(condition-case e (string "a") (error e))"#),
        "(wrong-type-argument characterp \"a\")"
    );
    assert_eq!(
        eval("(condition-case e (string -1) (error e))"),
        "(wrong-type-argument characterp -1)"
    );
    assert_eq!(eval("(string ?a ?b ?c)"), "\"abc\"");
    assert_eq!(eval("(string)"), "\"\"");
}

/// `format`'s `%c` applies the same check. Emacs 30.2:
/// `(wrong-type-argument characterp -1)`; elisprs answered `""`. A float still
/// takes the *other* error — Emacs distinguishes the two.
#[test]
fn format_percent_c_checks_characters() {
    assert_eq!(
        eval(r#"(condition-case e (format "%c" -1) (error e))"#),
        "(wrong-type-argument characterp -1)"
    );
    assert_eq!(
        eval(r#"(condition-case e (format "%c" 4194304) (error e))"#),
        "(wrong-type-argument characterp 4194304)"
    );
    assert_eq!(
        eval(r#"(condition-case e (format "%c" 1.5) (error e))"#),
        "(error \"Format specifier doesn’t match argument type\")"
    );
    assert_eq!(eval(r#"(format "%c" 97)"#), "\"a\"");
    assert_eq!(eval(r#"(format "%c" 256)"#), "\"Ā\"");
}

/// `copysign` is `CHECK_TYPE (FLOATP …)` in Emacs, unlike the rest of the float
/// library — an integer signals instead of coercing. elisprs answered `1.0` for
/// the first form.
#[test]
fn copysign_requires_floats() {
    assert_eq!(
        eval("(condition-case e (copysign 1 2.0) (error e))"),
        "(wrong-type-argument floatp 1)"
    );
    assert_eq!(
        eval("(condition-case e (copysign 1.0 2) (error e))"),
        "(wrong-type-argument floatp 2)"
    );
    assert_eq!(eval("(copysign 1.0 -2.0)"), "-1.0");
    assert_eq!(eval("(copysign -1.0 2.0)"), "1.0");
    assert_eq!(eval("(copysign 0.0 -1.0)"), "-0.0");
}
