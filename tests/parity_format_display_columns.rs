//! `format`'s field width and `%.Ns` precision are display columns, not
//! characters.
//!
//! Every expected value below is `emacs -Q --batch --eval '(prin1 …)'` on GNU
//! Emacs 30.2. The distinction is invisible for ASCII and decisive for anything
//! else: a TAB is 8 columns, a control character is 2 (`^G`), a newline is 0,
//! and an East-Asian wide character is 2.
//!
//! ```
//! (list (format "%.3s" "\tXY") (format "%.3s" "中XY")
//!       (format "%.3s" "中中") (format "%5s" "a\tb"))
//! => ("" "中X" "中" "a\tb")
//! ```
//!
//! elisprs counted characters, so the first was `"\tXY"` (nothing truncated) and
//! the last was `"  a\tb"` (padded as if the tab were one column).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The four cases from the bug report, verbatim.
#[test]
fn precision_and_width_measure_columns() {
    assert_eq!(
        eval(
            r#"(list (format "%.3s" "\tXY") (format "%.3s" "中XY")
                     (format "%.3s" "中中") (format "%5s" "a\tb"))"#
        ),
        // `prin1` emits the tab raw (`print-escape-control-characters` is nil),
        // so the expectation carries a literal one.
        "(\"\" \"中X\" \"中\" \"a\tb\")"
    );
}

/// A character is kept only when it fits *whole*: the 8-column tab appears at
/// precision 8 and not at 7. Emacs 30.2:
/// `(mapcar (lambda (n) (format (format "%%.%ds" n) "\tXY")) '(0 7 8 9 10))`
/// => `("" "" "\t" "\tX" "\tXY")`.
#[test]
fn a_character_that_would_overflow_the_budget_is_dropped_whole() {
    assert_eq!(
        eval(r#"(mapcar (lambda (n) (format (format "%%.%ds" n) "\tXY")) '(0 7 8 9 10))"#),
        "(\"\" \"\" \"\t\" \"\tX\" \"\tXY\")"
    );
}

/// Width padding is column-based for every conversion, not just `%s`.
/// Emacs 30.2: `("  中|" "  \"中\"|" "    中|" "中    |")`.
#[test]
fn field_width_pads_to_columns_for_every_conversion() {
    assert_eq!(
        eval(
            r#"(list (format "%4c|" ?中) (format "%6S|" "中")
                     (format "%6s|" '中) (format "%-6s|" "中"))"#
        ),
        r#"("  中|" "  \"中\"|" "    中|" "中    |")"#
    );
}

/// `%S`'s precision truncates the *printed* form, quote marks included, and had
/// been ignored entirely. Emacs 30.2: `("\"中" "      中|" "中      |")`.
#[test]
fn precision_applies_to_the_printed_form_of_capital_s() {
    assert_eq!(
        eval(
            r#"(list (format "%.3S" "中中中") (format "%8.3s|" "中中中") (format "%-8.3s|" "中中中"))"#
        ),
        r#"("\"中" "      中|" "中      |")"#
    );
}

/// `char-width` is a subr here because `format` needs the same table from Rust,
/// and one table means one answer. Emacs 30.2:
/// `(8 0 2 2 1 2 9)` and `(wrong-type-argument characterp "a")`.
#[test]
fn char_width_matches_the_c_primitive() {
    assert_eq!(
        eval(
            r#"(list (char-width ?\t) (char-width ?\n) (char-width 7) (char-width 127)
                     (char-width 200) (char-width ?中) (string-width "a\t"))"#
        ),
        "(8 0 2 2 1 2 9)"
    );
    assert_eq!(
        eval(r#"(condition-case e (char-width "a") (error e))"#),
        r#"(wrong-type-argument characterp "a")"#
    );
}
