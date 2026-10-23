//! lread.c `read_char_escape`'s `\x`, `\u`/`\U` and `\N{…}` branches, and
//! `read_char_literal`'s raw-byte collapse.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn read(src: &str) -> String {
    reset_host();
    let v = eval_str(&format!("(condition-case e (read {src:?}) (error e))")).expect("eval failed");
    print(&v, true)
}

/// `\x` takes any number of digits up to the modifier bits, and a value whose
/// character part is a raw byte reads as that byte.
#[test]
fn hex_escapes() {
    assert_eq!(read("?\\xfffffff"), "264241407");
    assert_eq!(read("?\\x80"), "128");
    assert_eq!(read("?\\x080"), "128");
    assert_eq!(
        read("?\\x10000000"),
        "(error \"Hex character out of range: \\\\x10000000...\")"
    );
    assert_eq!(
        read("?\\xg"),
        "(error \"Invalid escape char syntax: \\\\x not followed by hex digit\")"
    );
}

/// `\u` and `\U` read exactly 4 or 8 digits, each failure with its own message.
#[test]
fn unicode_escapes() {
    assert_eq!(
        read("?\\u12"),
        "(error \"Malformed Unicode escape: \\\\u12\")"
    );
    assert_eq!(
        read("\"\\u"),
        "(error \"Malformed Unicode escape: \\\\u0\")"
    );
    assert_eq!(
        read("?\\u12g4"),
        "(error \"Non-hex character used for Unicode escape: g (103)\")"
    );
    assert_eq!(
        read("?\\U00110000"),
        "(error \"Non-Unicode character: 0x110000\")"
    );
    assert_eq!(read("?\\u00e9"), "233");
}

/// `\N{…}` needs its brace, an ASCII non-empty name, and for `U+` a hex
/// scalar value that is not a surrogate.
#[test]
fn named_escapes() {
    assert_eq!(
        read("?\\N"),
        "(invalid-read-syntax \"Expected opening brace after \\\\N\")"
    );
    assert_eq!(
        read("?\\N{}"),
        "(invalid-read-syntax \"Empty character name\")"
    );
    assert_eq!(
        read("?\\N{U+110000}"),
        "(invalid-read-syntax \"\\\\N{U+110000}\")"
    );
    assert_eq!(
        read("?\\N{U+D800}"),
        "(invalid-read-syntax \"\\\\N{U+D800}\")"
    );
    assert_eq!(
        read("?\\N{U+-41}"),
        "(invalid-read-syntax \"\\\\N{U+-41}\")"
    );
    assert_eq!(
        read("?\\N{é}"),
        "(invalid-read-syntax \"Invalid character U+00E9 in character name\")"
    );
    assert_eq!(read("?\\N{U+41}"), "65");
}
