//! category.c (category sets, tables, `define-category`, `modify-category-entry`)
//! and the `\cC` / `\CC` regexp escapes that read the current category table.
//! The standard table carries characters.el's category definitions and its
//! Unicode-range entries.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// characters.el defines the categories; ASCII is `a` and `l`, CJK ideographs
/// are `C`, `c` and the line-breakable `|`.
#[test]
fn standard_category_table() {
    assert_eq!(
        eval("(list (category-table-p (standard-category-table)) (eq (category-table) (standard-category-table)) (category-docstring ?a) (category-docstring ?|) (get-unused-category))"),
        "(t t \"ASCII\nASCII graphic characters 32-126 (ISO646 IRV:1983[4/0])\" \"line breakable\nWhile filling, we can break a line at this character.\" 33)"
    );
    assert_eq!(
        eval("(list (string-match \"\\\\c|\" \"ab中c\") (string-match \"\\\\C|\" \"中ab\") (string-match \"\\\\ca\" \"中a\") (string-match \"\\\\cl\" \"é\") (string-match \"\\\\cC\" \"abc\"))"),
        "(2 1 1 0 nil)"
    );
    assert_eq!(eval("(memq (aref (char-category-set ?中) ?|) '(t))"), "(t)");
    assert_eq!(
        eval("(list (aref (char-category-set ?a) ?a) (aref (char-category-set ?a) ?|) (aref (char-category-set ?ア) ?K))"),
        "(t nil t)"
    );
}

/// `make-category-table` starts empty but with every set its own;
/// `copy-category-table` shares no set with its source.
#[test]
fn category_tables() {
    assert_eq!(
        eval("(let ((tb (make-category-table))) (list (category-table-p tb) (category-docstring ?a tb) (get-unused-category tb) (bool-vector-p (char-table-range tb ?a)) (length (char-table-range tb ?a))))"),
        "(t nil 32 t 128)"
    );
    assert_eq!(
        eval("(let ((tb (copy-category-table))) (list (eq tb (standard-category-table)) (category-docstring ?a tb) (eq (aref tb ?a) (aref (standard-category-table) ?a)) (equal (aref tb ?a) (aref (standard-category-table) ?a))))"),
        "(nil \"ASCII\nASCII graphic characters 32-126 (ISO646 IRV:1983[4/0])\" nil t)"
    );
    assert_eq!(
        eval("(category-set-mnemonics (make-category-set \"zab\"))"),
        "\"abz\""
    );
    assert_eq!(
        eval("(category-set-mnemonics (make-bool-vector 128 nil))"),
        "\"\""
    );
}

/// A category defined in a buffer's own table is visible to `\cC` in that
/// buffer only, and resetting an entry takes it away again.
#[test]
fn modify_category_entry() {
    assert_eq!(
        eval("(with-temp-buffer (let ((tb (copy-category-table))) (set-category-table tb) (define-category ?Z \"zed\") (modify-category-entry '(?a . ?c) ?Z) (list (string-match \"\\\\cZ\" \"xxb\") (aref (char-category-set ?b) ?Z) (with-temp-buffer (string-match \"\\\\cZ\" \"xxb\")) (progn (modify-category-entry ?b ?Z nil t) (string-match \"\\\\cZ\" \"xxb\")) (string-match \"\\\\CZ\" \"abd\"))))"),
        "(2 t nil nil 1)"
    );
    assert_eq!(
        eval("(with-temp-buffer (let ((tb (make-category-table))) (define-category ?Q \"q\" tb) (modify-category-entry '(#x100 . #x10FFFF) ?Q tb) (set-category-table tb) (list (string-match \"\\\\cQ\" \"aé\\x100\") (string-match \"\\\\cQ\" \"abc\"))))"),
        "(2 nil)"
    );
    assert_eq!(
        eval("(with-temp-buffer (set-category-table (copy-category-table)) (define-category ?Z \"zed\") (modify-category-entry (cons ?a ?中) ?Z) (list (aref (char-category-set ?中) ?C) (aref (char-category-set ?中) ?Z) (aref (char-category-set ?a) ?l) (aref (char-category-set ?a) ?Z) (aref (char-category-set ?中) ?l) (aref (char-category-set ?\\s) ?Z)))"),
        "(t t t t nil nil)"
    );
}

/// The C's argument checks and their error data.
#[test]
fn category_errors() {
    assert_eq!(
        eval("(define-category ?a \"dup\")"),
        "(ERR (error \"Category ‘a’ is already defined\"))"
    );
    assert_eq!(
        eval("(modify-category-entry ?a ?Q)"),
        "(ERR (error \"Undefined category: Q\"))"
    );
    assert_eq!(
        eval("(modify-category-entry 1.0 ?a)"),
        "(ERR (wrong-type-argument consp 1.0))"
    );
    assert_eq!(
        eval("(modify-category-entry ?a 200)"),
        "(ERR (wrong-type-argument categoryp 200))"
    );
    assert_eq!(
        eval("(modify-category-entry '(?a . -1) ?a)"),
        "(ERR (wrong-type-argument characterp -1))"
    );
    assert_eq!(
        eval("(char-category-set -1)"),
        "(ERR (wrong-type-argument characterp -1))"
    );
    assert_eq!(
        eval("(category-set-mnemonics \"abc\")"),
        "(ERR (wrong-type-argument categorysetp \"abc\"))"
    );
    assert_eq!(
        eval("(set-category-table 3)"),
        "(ERR (wrong-type-argument category-table-p 3))"
    );
    assert_eq!(
        eval("(define-category ?\\x7f \"del\")"),
        "(ERR (wrong-type-argument categoryp 127))"
    );
    assert_eq!(
        eval("(string-match \"\\\\c\" \"a\")"),
        "(ERR (invalid-regexp \"Premature end of regular expression\"))"
    );
}
