//! Parity gaps closed in round 13: the `#("TEXT" START END PLIST …)` read
//! syntax (and the text properties it carries into error DATA), the cl-seq
//! `*-if` family's nil-predicate contract, the two VM spellings of `nil`, and
//! the two syntax tables (`standard-syntax-table` vs the current buffer's).
//!
//! Every expectation here is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch -l PROBE` — not of the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The value, or the printed error object a `condition-case` catches.
fn caught(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

// ── #("TEXT" START END PLIST …) ──────────────────────────────────────────────

/// lread.c's `#(` arm reads the string, then applies each START/END/PLIST
/// triple with `Fset_text_properties`. elisprs used to read the string and drop
/// the intervals on the floor, so a propertized literal silently lost every
/// property it was written with.
#[test]
fn propertized_string_read_syntax_keeps_its_intervals() {
    assert_eq!(
        eval(r##"(read "#(\"foo\" 0 3 (a 1))")"##),
        r##"#("foo" 0 3 (a 1))"##
    );
    assert_eq!(
        eval(r##"(get-text-property 1 'a #("abc" 0 3 (a 1)))"##),
        "1"
    );
    // Several intervals, and a partial one.
    assert_eq!(
        eval(r##"(read "#(\"abcd\" 0 2 (a 1) 2 4 (b 2))")"##),
        r##"#("abcd" 0 2 (a 1) 2 4 (b 2))"##
    );
    assert_eq!(
        eval(r##"(read "#(\"abcd\" 1 3 (a 1))")"##),
        r##"#("abcd" 1 3 (a 1))"##
    );
    // `Fset_text_properties' SETS, so a later triple replaces an earlier one …
    assert_eq!(
        eval(r##"(read "#(\"foo\" 0 3 (a 1) 0 1 (b 2))")"##),
        r##"#("foo" 0 1 (b 2) 1 3 (a 1))"##
    );
    // … including replacing it with nothing.
    assert_eq!(
        eval(r##"(read "#(\"abc\" 0 3 (a 1) 1 2 nil)")"##),
        r##"#("abc" 0 1 (a 1) 2 3 (a 1))"##
    );
    // A nil plist leaves an ordinary string, and so does an empty range.
    assert_eq!(eval(r##"(read "#(\"foo\" 0 3 nil)")"##), r##""foo""##);
    assert_eq!(eval(r##"(read "#(\"foo\" 3 3 (a 1))")"##), r##""foo""##);
    assert_eq!(eval(r##"(read "#(\"foo\")")"##), r##""foo""##);
    // `validate_interval_range' swaps an inverted range before applying it.
    assert_eq!(
        eval(r##"(read "#(\"foo\" 2 1 (a 1))")"##),
        r##"#("foo" 1 2 (a 1))"##
    );
    // `validate_plist' wraps a non-list PLIST as the single pair (PLIST nil).
    assert_eq!(
        eval(r##"(read "#(\"foo\" 0 3 5)")"##),
        r##"#("foo" 0 3 (5 nil))"##
    );
}

/// The diagnostics are lread.c's and textprop.c's own, down to the wording.
#[test]
fn propertized_string_read_syntax_rejects_what_emacs_rejects() {
    // A first element that is not a string — `#()` included.
    assert_eq!(
        caught(r##"(read "#()")"##),
        r##"(invalid-read-syntax "#")"##
    );
    assert_eq!(
        caught(r##"(read "#(1 2 3)")"##),
        r##"(invalid-read-syntax "#")"##
    );
    // A trailing group of fewer than three elements.
    assert_eq!(
        caught(r##"(read "#(\"foo\" 0 3)")"##),
        r##"(invalid-read-syntax "Invalid string property list")"##
    );
    assert_eq!(
        caught(r##"(read "#(\"foo\" 0 3 (a 1) 1 2)")"##),
        r##"(invalid-read-syntax "Invalid string property list")"##
    );
    // Bounds and bound types come from `validate_interval_range'.
    assert_eq!(
        caught(r##"(read "#(\"foo\" 0 9 (a 1))")"##),
        "(args-out-of-range 0 9)"
    );
    assert_eq!(
        caught(r##"(read "#(\"foo\" -1 3 (a 1))")"##),
        "(args-out-of-range -1 3)"
    );
    assert_eq!(
        caught(r##"(read "#(\"foo\" 0 3.5 (a 1))")"##),
        "(wrong-type-argument integer-or-marker-p 3.5)"
    );
    // An odd-length PLIST is `validate_plist''s plain `error'.
    assert_eq!(
        caught(r##"(read "#(\"foo\" 0 3 (a))")"##),
        r##"(error "Odd length text property list")"##
    );
}

/// `make_error_object` rebuilds a condition's DATA by re-reading the rendered
/// message, so a reader that could not read `#(…)` produced error data whose
/// offending value had silently lost its properties.
#[test]
fn error_data_keeps_a_strings_text_properties() {
    assert_eq!(
        caught("(car (propertize \"foo\" 'a 1))"),
        r##"(wrong-type-argument listp #("foo" 0 3 (a 1)))"##
    );
    assert_eq!(
        caught("(elt (propertize \"abc\" 'a 1) 9)"),
        r##"(args-out-of-range #("abc" 0 3 (a 1)) 9)"##
    );
    assert_eq!(
        caught("(aref (vector (propertize \"foo\" 'a 1)) 5)"),
        r##"(args-out-of-range [#("foo" 0 3 (a 1))] 5)"##
    );
}

// ── cl-seq `*-if' with a nil predicate ───────────────────────────────────────

/// cl-seq.el's `*-if' functions are wrappers that pass their predicate through
/// as `:if'; `cl--check-test-nokey' only calls it when it is non-nil, and falls
/// through to `(eql ITEM X)' — with the implicit nil ITEM — when it is not.
/// So a nil predicate matches the nil elements; it is never funcalled.
#[test]
fn a_nil_predicate_matches_the_nil_elements() {
    assert_eq!(eval("(cl-position-if nil '(1 nil 2))"), "1");
    assert_eq!(eval("(cl-position-if nil '(1 2))"), "nil");
    assert_eq!(eval("(cl-position-if nil \"foo2\")"), "nil");
    assert_eq!(eval("(cl-position-if-not nil '(1 nil 2))"), "1");
    assert_eq!(eval("(cl-find-if nil '(1 nil 2))"), "nil");
    assert_eq!(eval("(cl-find-if-not nil '(1 nil 2))"), "nil");
    assert_eq!(eval("(cl-count-if nil '(1 nil 2 nil))"), "2");
    assert_eq!(eval("(cl-count-if-not nil '(1 nil 2))"), "1");
    assert_eq!(eval("(cl-member-if nil '(1 nil 2))"), "(nil 2)");
    assert_eq!(eval("(cl-member-if-not nil '(1 nil 2))"), "(nil 2)");
    assert_eq!(eval("(cl-assoc-if nil '((1 . 2) (nil . 3)))"), "(nil . 3)");
    assert_eq!(
        eval("(cl-assoc-if-not nil '((1 . 2) (nil . 3)))"),
        "(nil . 3)"
    );
    assert_eq!(eval("(cl-rassoc-if nil '((1 . 2) (3 . nil)))"), "(3)");
    assert_eq!(eval("(cl-substitute-if 9 nil '(1 nil 2))"), "(1 9 2)");
    assert_eq!(eval("(cl-substitute-if-not 9 nil '(1 nil 2))"), "(1 9 2)");
    assert_eq!(eval("(cl-remove-if nil '(1 nil 2))"), "(1 2)");
    assert_eq!(eval("(cl-remove-if-not nil '(1 nil 2))"), "(1 2)");
}

/// A real predicate still behaves, and the `-not' wrappers still negate.
#[test]
fn a_real_predicate_is_unaffected_by_the_nil_fallback() {
    assert_eq!(eval("(cl-position-if #'numberp '(nil 2))"), "1");
    assert_eq!(eval("(cl-position-if-not #'numberp '(1 nil 2))"), "1");
    assert_eq!(eval("(cl-find-if #'numberp '(nil 2))"), "2");
    assert_eq!(eval("(cl-count-if #'numberp '(nil 2 3))"), "2");
    assert_eq!(eval("(cl-member-if #'numberp '(nil 2))"), "(2)");
    assert_eq!(eval("(cl-remove-if-not #'numberp '(nil 2))"), "(2)");
}

/// `cl-subst-if' and its three siblings were missing entirely. They are
/// `cl-sublis' over the one-entry alist `((nil . NEW))' with the predicate as
/// `:if', which is why a nil predicate replaces the list's terminating nil too.
#[test]
fn subst_if_substitutes_through_the_whole_tree() {
    assert_eq!(eval("(cl-subst-if 9 nil '(1 nil 2))"), "(1 9 2 . 9)");
    assert_eq!(eval("(cl-nsubst-if 9 nil '(1 nil 2))"), "(1 9 2 . 9)");
    assert_eq!(eval("(cl-subst-if-not 9 nil '(1 nil 2))"), "(1 9 2 . 9)");
    assert_eq!(eval("(cl-nsubst-if-not 9 nil '(1 nil 2))"), "(1 9 2 . 9)");
    // A real predicate matches every node it accepts, conses included, so the
    // whole tree collapses to NEW as soon as the root matches.
    assert_eq!(eval("(cl-subst-if 9 #'numberp '(1 nil 2))"), "(9 nil 9)");
    assert_eq!(eval("(cl-subst-if-not 9 #'numberp '(1 nil 2))"), "9");
    // :test / :test-not / :key still reach cl-sublis unchanged.
    assert_eq!(eval("(cl-sublis '((1 . 9)) '(1 2 1))"), "(9 2 9)");
    assert_eq!(eval("(cl-sublis '((1 . 9)) '(1 2) :test #'eq)"), "(9 2)");
}

// ── nil's two VM spellings ───────────────────────────────────────────────────

/// A literal `nil` compiles to fusevm `Undef`, but a comparison that answers
/// false produces `Value::Bool(false)`. Both are elisp's one `nil`, and every
/// place that accepts nil has to accept both — treating only `Undef` as the
/// empty list made `(length (= 5 42))` signal `(wrong-type-argument sequencep
/// nil)`, an error naming the very value it refused to recognise.
#[test]
fn a_false_comparison_is_nil_everywhere_a_literal_nil_is() {
    assert_eq!(eval("(length (= 5 42))"), "0");
    assert_eq!(eval("(reverse (= 5 42))"), "nil");
    assert_eq!(eval("(mapcar #'identity (= 5 42))"), "nil");
    assert_eq!(eval("(mapconcat #'identity (= 5 42))"), "\"\"");
    assert_eq!(eval("(apply #'list (= 5 42))"), "nil");
    assert_eq!(eval("(sort (= 5 42))"), "nil");
    assert_eq!(eval("(symbol-name (= 5 42))"), "\"nil\"");
    assert_eq!(eval("(symbol-value (= 5 42))"), "nil");
    assert_eq!(eval("(bare-symbol-p (= 5 42))"), "t");
    assert_eq!(eval("(seq-empty-p (= 5 42))"), "t");
    assert_eq!(eval("(delete-dups (= 5 42))"), "nil");
    assert_eq!(eval("(butlast (= 5 42))"), "nil");
    assert_eq!(eval("(string-join (= 5 42))"), "\"\"");
    assert_eq!(eval("(cl-list-length (= 5 42))"), "0");
    assert_eq!(
        eval("(assoc-string (= 5 42) (list \"a\" 9.3 \"b\"))"),
        "nil"
    );
    assert_eq!(eval("(run-hooks (= 5 42))"), "nil");
    // A non-nil non-sequence still signals, naming itself.
    assert_eq!(
        caught("(length (= 5 5))"),
        "(wrong-type-argument sequencep t)"
    );
}

// ── the two syntax tables ────────────────────────────────────────────────────

/// `standard-syntax-table` and the current buffer's table are different tables
/// and answer differently, which is why reading one and writing the other gets
/// both wrong. `emacs -Q --batch` starts in `lisp-interaction-mode`:
///
/// ```text
/// $ emacs -Q --batch --eval '(prin1 (list (char-syntax 1) (char-syntax ?\n) (char-syntax ?\r) (char-syntax 127) (char-syntax ?\;) (char-syntax ?$) (char-syntax ?{)))'
/// (95 62 95 95 60 95 95)
/// $ emacs -Q --batch --eval '(with-syntax-table (standard-syntax-table) (prin1 (list (char-syntax 1) (char-syntax ?\n) (char-syntax ?\r) (char-syntax 127) (char-syntax ?\;) (char-syntax ?$) (char-syntax ?{))))'
/// (46 32 32 46 46 119 40)
/// ```
#[test]
fn the_buffer_table_and_the_standard_table_stay_distinct() {
    assert_eq!(
        eval(
            "(list (char-syntax 1) (char-syntax ?\\n) (char-syntax ?\\r) \
                   (char-syntax 127) (char-syntax ?\\;) (char-syntax ?$) (char-syntax ?{))"
        ),
        "(95 62 95 95 60 95 95)"
    );
    assert_eq!(
        eval(
            "(with-syntax-table (standard-syntax-table) \
               (list (char-syntax 1) (char-syntax ?\\n) (char-syntax ?\\r) \
                     (char-syntax 127) (char-syntax ?\\;) (char-syntax ?$) (char-syntax ?{)))"
        ),
        "(46 32 32 46 46 119 40)"
    );
    // Both tables, all 256 characters, against the two `emacs -Q --batch` runs
    // above extended over `(number-sequence 0 255)`.
    let buffer_table = "_________ >_ ___________________ _\"'___'()__'___wwwwwwwwww_<_____wwwwwwwwwwwwwwwwwwwwwwwwww(\\)__'wwwwwwwwwwwwwwwwwwwwwwwwww_____wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww .___w_.___.______ww_w___w_.___.wwwwwwwwwwwwwwwwwwwwwww_wwwwwwwwwwwwwwwwwwwwwwwwwwwwwww_wwwwwwww";
    let standard_table = ".........  .  .................. .\".ww_.()__._._wwwwwwwwww..___..wwwwwwwwwwwwwwwwwwwwwwwwww(\\)._.wwwwwwwwwwwwwwwwwwwwwwwwww(_)..wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww .___w_.___.______ww_w___w_.___.wwwwwwwwwwwwwwwwwwwwwww_wwwwwwwwwwwwwwwwwwwwwwwwwwwwwww_wwwwwwww";
    let dump = "(mapconcat (lambda (c) (string (char-syntax c))) (number-sequence 0 255) \"\")";
    assert_eq!(eval(dump), format!("{buffer_table:?}"));
    assert_eq!(
        eval(&format!(
            "(with-syntax-table (standard-syntax-table) {dump})"
        )),
        format!("{standard_table:?}")
    );
}

/// syntax.c defaults everything at or above U+0080 to word, and
/// characters.el's Latin-1 block then reclassifies the punctuation and the
/// symbols. elisprs stopped at the default, so `?¡` answered `?w` instead of
/// `?.` and every Latin-1 symbol answered `?w` instead of `?_`.
#[test]
fn latin1_syntax_classes_match_the_standard_table() {
    // The whole block in one eval: `emacs -Q --batch` prints this exact string
    // for `(mapconcat (lambda (c) (string (char-syntax c))) (number-sequence
    // 128 255) "")` with the standard syntax table current.
    let expect: String = (128u32..256)
        .map(|c| match c {
            160 => ' ',
            161 | 167 | 171 | 187 | 191 => '.',
            162..=164 | 166 | 168..=170 | 172..=177 | 180 | 182..=184 | 186 | 188..=190 => '_',
            215 | 247 => '_',
            _ => 'w',
        })
        .collect();
    assert_eq!(
        eval(
            "(with-syntax-table (standard-syntax-table) \
               (mapconcat (lambda (c) (string (char-syntax c))) \
                          (number-sequence 128 255) \"\"))"
        ),
        format!("{expect:?}")
    );
}
