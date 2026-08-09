//! `\sC`, `\SC`, `\w` and `\W` read the syntax table.
//!
//! `regexp::translate_escape` used to map a handful of class letters to fixed
//! character sets and fall back to the whitespace set for every letter it did
//! not know, with no access to the host at all. So `\s_` matched whitespace,
//! `\s<` and `\s>` matched whitespace, and none of the classes — including the
//! ones that were "right" — noticed `with-syntax-table` or
//! `modify-syntax-entry`.
//!
//! Emacs asks `SYNTAX (c) == class` per character while matching. elisprs
//! compiles to `fancy_regex` up front, so `ElispHost::syntax_class_ranges`
//! answers the same question for the whole character space at compile time; a
//! `CharTable` stores runs, so the breakpoints of the table and its parents
//! bound every place the answer can change.
//!
//! Every expectation below is `emacs -Q --batch --eval '(prin1 …)'` on GNU
//! Emacs 30.2, whose current buffer (`*scratch*`, `lisp-interaction-mode`) has
//! the same table elisprs installs in its initial buffer.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The three classes named in the bug report, plus the two that already worked.
/// Emacs 30.2: `(0 0 0 0 0)`; elisprs answered `(nil nil nil 0 0)`.
#[test]
fn symbol_and_comment_classes_come_from_the_table() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\s_" "-") (string-match "\\s<" ";")
                     (string-match "\\s>" "\n") (string-match "\\s-" " ")
                     (string-match "\\sw" "a"))"#
        ),
        "(0 0 0 0 0)"
    );
}

/// The whitespace class must stay the table's, not the regex crate's `\s`: the
/// line terminators are *not* whitespace under this table. Emacs 30.2:
/// `(nil nil nil 0 0)`.
#[test]
fn whitespace_excludes_the_line_terminators() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\s-" "\n") (string-match "\\s-" "\r")
                     (string-match "\\s-" "\v") (string-match "\\s-" "\t")
                     (string-match "\\s-" "\f"))"#
        ),
        "(nil nil nil 0 0)"
    );
}

/// The paired and quote classes were previously indistinguishable from
/// whitespace. Emacs 30.2: `(0 0 0 nil)`.
#[test]
fn paired_and_quote_classes_are_distinct() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\s(" "(") (string-match "\\s)" ")")
                     (string-match "\\s\"" "\"") (string-match "\\s." "!"))"#
        ),
        "(0 0 0 nil)"
    );
}

/// `\SC` is the complement of the same set: `a` is neither whitespace nor a
/// symbol constituent, and `-` is not a word constituent.
/// Emacs 30.2: `(0 0 0)`, against `(nil nil nil)` for the positive forms.
#[test]
fn the_negated_form_complements_the_class() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\S-" "a") (string-match "\\Sw" "-")
                     (string-match "\\S_" "a"))"#
        ),
        "(0 0 0)"
    );
    assert_eq!(
        eval(
            r#"(list (string-match "\\s-" "a") (string-match "\\sw" "-")
                     (string-match "\\s_" "a"))"#
        ),
        "(nil nil nil)"
    );
}

/// A different table changes the answer — the whole point of reading one.
/// In `standard-syntax-table` `;` is punctuation, not a comment start.
/// Emacs 30.2: `(0 nil 0)`.
#[test]
fn with_syntax_table_changes_the_class() {
    assert_eq!(
        eval(
            r#"(with-syntax-table (standard-syntax-table)
                 (list (string-match "\\s_" "-") (string-match "\\s<" ";")
                       (string-match "\\s." ";")))"#
        ),
        "(0 nil 0)"
    );
}

/// `modify-syntax-entry` moves a character between classes, and the class is
/// never case-folded: `case-fold-search` is t here, and letting its `(?i)` reach
/// the emitted class made `a` match the class's `A-Z` run.
/// Emacs 30.2: `(95 nil 0)`.
#[test]
fn a_reclassified_character_leaves_its_old_class() {
    assert_eq!(
        eval(
            r#"(with-temp-buffer (modify-syntax-entry ?a "_")
                 (list (char-syntax ?a) (string-match "\\sw" "a")
                       (string-match "\\s_" "a")))"#
        ),
        "(95 nil 0)"
    );
}

/// `\w` is the table's word class too (`regex-emacs.c` tests `SYNTAX (c) ==
/// Sword`), not the crate's `[0-9A-Za-z_]`. Under any lisp-mode table `_` is a
/// symbol constituent. Emacs 30.2: `(nil 0 0 nil 0)`.
#[test]
fn backslash_w_is_the_tables_word_class() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\w" "-") (string-match "\\w" "a")
                     (string-match "\\W" "-") (string-match "\\w" "_")
                     (string-match "\\W" "_"))"#
        ),
        "(nil 0 0 nil 0)"
    );
}

/// Reclassifying a letter takes it out of `\w` as well. Emacs 30.2: `(nil 0)`.
#[test]
fn backslash_w_follows_modify_syntax_entry() {
    assert_eq!(
        eval(
            r#"(progn (modify-syntax-entry ?a "_")
                 (list (string-match "\\w" "a") (string-match "\\W" "a")))"#
        ),
        "(nil 0)"
    );
}

/// The classes still compose with quantifiers, anchors and the word-boundary
/// escapes. Emacs 30.2: `(0 3 2 2 0)`.
#[test]
fn syntax_classes_compose_with_the_rest_of_the_dialect() {
    assert_eq!(
        eval(
            r#"(list (string-match "\\w+" "abc def") (match-end 0)
                     (string-match "\\bfoo\\b" "a foo b")
                     (string-match "\\<foo\\>" "a foo b")
                     (string-match "\\w-\\w" "a-b"))"#
        ),
        "(0 3 2 2 0)"
    );
}
