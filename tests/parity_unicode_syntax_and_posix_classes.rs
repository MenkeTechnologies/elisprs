//! Syntax of characters above U+00FF in the standard syntax table, and the
//! POSIX classes that depend on it (`space`, `word`, `punct`) or on the general
//! category (`graph`, `print`, `upper`).
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

/// characters.el classifies the Unicode space; everything it leaves alone keeps
/// the word default.
#[test]
fn standard_table_classifies_characters_above_latin_1() {
    check(
        r#"(with-syntax-table (standard-syntax-table)
             (mapcar #'char-syntax '(#x20AC #x2192 #x2200 #x2003 #x200B #x3000 #x4E2D #x3BB #x2028 #x1F600)))"#,
        "=(95 95 95 32 32 32 119 119 119 119)",
    );
    check(
        r#"(with-syntax-table (standard-syntax-table)
             (list (aref (syntax-table) #x2045) (aref (syntax-table) #x2046) (aref (syntax-table) #x2200)))"#,
        "=((4 . 8262) (5 . 8261) (3))",
    );
}

#[test]
fn syntax_escapes_see_the_extended_table() {
    check(
        r#"(list (string-match "\\sw" "€") (string-match "\\s_" "€") (string-match "\\s-" " ")
                 (string-match "\\sw" "　") (string-match "\\w" "​"))"#,
        "=(nil 0 0 nil nil)",
    );
}

/// `graphicp` / `printablep`: format characters and private use are graphic,
/// separators are printable but not graphic, controls are neither.
#[test]
fn graph_and_print_follow_the_general_category() {
    check(
        r#"(let ((case-fold-search nil))
             (mapcar (lambda (c) (list (string-match "[[:graph:]]" (string c))
                                       (string-match "[[:print:]]" (string c))))
                     '(?a ?é ?€ #x4E2D #x200B #x2003 #x2028 ?\s ?\t #x80)))"#,
        "=((0 0) (0 0) (0 0) (0 0) (0 0) (nil 0) (nil 0) (nil 0) (nil nil) (nil nil))",
    );
}

/// `ISSPACE` / `ISWORD` / `ISPUNCT` ask the syntax table, not Unicode.
#[test]
fn space_word_punct_classes_are_syntax_based() {
    check(
        r#"(let ((case-fold-search nil))
             (mapcar (lambda (c) (list (string-match "[[:space:]]" (string c))
                                       (string-match "[[:word:]]" (string c))
                                       (string-match "[[:punct:]]" (string c))))
                     '(?a ?! ?$ #x20AC #x1F600 #x4E2D #x2003 #x200B #x2028 #xA0)))"#,
        "=((nil 0 nil) (nil nil 0) (nil nil 0) (nil nil 0) (nil 0 nil) (nil 0 nil) (0 nil 0) (0 nil 0) (nil 0 nil) (0 nil 0))",
    );
}

/// `uppercasep` is `downcase (c) != c`: titlecase letters qualify, characters
/// carrying only the `Other_Uppercase` property do not.
#[test]
fn upper_and_lower_follow_the_case_mappings() {
    check(
        r#"(let ((case-fold-search nil))
             (mapcar (lambda (c) (list (string-match "[[:upper:]]" (string c))
                                       (string-match "[[:lower:]]" (string c))))
                     '(?A ?a #x1C5 #x2102 #x24B6 #x2B0 #x130 #x138 #x3A3 #x3C3)))"#,
        "=((0 nil) (nil 0) (0 nil) (nil nil) (0 nil) (nil nil) (nil nil) (nil nil) (0 nil) (nil 0))",
    );
}

/// A compiled pattern that read the syntax table is cached under the table's
/// state, so a change to the table must still be seen.
#[test]
fn syntax_dependent_pattern_cache_tracks_the_table() {
    check(
        r#"(let ((s (make-syntax-table)) r)
             (push (string-match "\\sw" "-a") r)
             (modify-syntax-entry ?- "w" s)
             (push (with-syntax-table s (string-match "\\sw" "-a")) r)
             (push (string-match "\\sw" "-a") r)
             (modify-syntax-entry ?a "." s)
             (push (with-syntax-table s (string-match "\\sw" "-a")) r)
             (push (with-syntax-table s (string-match "\\sw" "-a")) r)
             (nreverse r))"#,
        "=(1 0 1 0 0)",
    );
}
