//! `syntax.c`'s sexp/comment scanner: `scan-lists`, `scan-sexps`,
//! `parse-partial-sexp`, `forward-comment`, and the motion commands built on
//! them.
//!
//! These were all void. They are one port, not seven: `Fforward_sexp` is
//! `scan_lists`, and `scan_lists` and `Fparse_partial_sexp` share the
//! comment/string state machine (`scan_sexps_forward`, `forw_comment`,
//! `back_comment`, `char_quoted`). A parens-only subset would answer plausibly
//! on balanced parentheses and wrongly inside a string or a comment, so the
//! cases below deliberately concentrate on the parts a subset gets wrong:
//! two-character comment delimiters, comment styles, nested comments, generic
//! string and comment fences, `parse-sexp-ignore-comments` in both settings,
//! resumption from a partial state, and the `syntax-table` text property.
//!
//! Every expectation is `emacs -Q --batch` on GNU Emacs 30.2. The probes use
//! `with-temp-buffer` and, where the standard table is not the point, an
//! explicit `set-syntax-table`, so no expectation depends on which entry point
//! minted it — `--eval`, `-l` and `--script` differ in the initial buffer's
//! syntax table, and a scanner expectation captured under the wrong one is a
//! plausible wrong number.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// A C-flavoured table: `/* */` (style a), `//` to end of line (style b),
/// `"` strings, `\` escape. Two-character delimiters that share their first
/// character are what separate a real port from a paren counter.
const C_TABLE: &str = r#"(let ((tbl (make-syntax-table)))
  (modify-syntax-entry ?/ ". 124" tbl) (modify-syntax-entry ?* ". 23b" tbl)
  (modify-syntax-entry ?\n ">" tbl) (modify-syntax-entry ?\" "\"" tbl)
  (modify-syntax-entry ?\\ "\\" tbl) (modify-syntax-entry ?_ "_" tbl)
  (set-syntax-table tbl))"#;

/// A Lisp-flavoured table with `;` to end of line and *nestable* `#| |#`.
const LISP_TABLE: &str = r#"(let ((tbl (make-syntax-table)))
  (modify-syntax-entry ?\; "<" tbl) (modify-syntax-entry ?\n ">" tbl)
  (modify-syntax-entry ?# "' 14b" tbl) (modify-syntax-entry ?| "_ 23bn" tbl)
  (modify-syntax-entry ?\" "\"" tbl) (modify-syntax-entry ?\\ "\\" tbl)
  (set-syntax-table tbl))"#;

#[test]
fn scan_sexps_and_scan_lists_over_balanced_text() {
    // `(a b (c d) e) f`: one sexp forward, two, one back from the end, then
    // `scan-lists` with DEPTH -1 (down a level) and DEPTH 1 (up out of one).
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "(a b (c d) e) f")
                 (list (scan-sexps 1 1) (scan-sexps 1 2) (scan-sexps 15 -1)
                       (scan-lists 1 1 -1) (scan-lists 6 1 1)))"#
        ),
        "(14 16 1 2 14)"
    );
}

#[test]
fn a_string_hides_its_parens_from_the_scanner() {
    // The `(` inside `"b (c"` must not open a level, and position 6 must report
    // the string terminator in element 3.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a \"b (c\" d")
                 (list (scan-sexps 1 2) (scan-sexps 1 3)
                       (nth 3 (parse-partial-sexp 1 6))))"#
        ),
        "(9 11 34)"
    );
}

#[test]
fn parse_sexp_ignore_comments_changes_the_answer_both_ways() {
    // With comments ignored, `/* b (c */` is whitespace and the second sexp is
    // `d`. With them not ignored, `/` and `*` are punctuation, the `(` inside
    // the comment really opens a level, and the third sexp scan hits the end of
    // the buffer inside it: `scan-error`.
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a /* b (c */ d") {C_TABLE}
                 (let ((parse-sexp-ignore-comments t))
                   (list (scan-sexps 1 2) (scan-sexps (point-max) -1))))"#
        )),
        "(15 14)"
    );
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a /* b (c */ d") {C_TABLE}
                 (let ((parse-sexp-ignore-comments nil))
                   (list (scan-sexps 1 2)
                         (condition-case e (scan-sexps 1 3) (error (car e))))))"#
        )),
        "(7 scan-error)"
    );
}

#[test]
fn parse_partial_sexp_reports_being_inside_a_comment() {
    // Position 7 is inside `/* b */`: element 4 is t (non-nestable comment),
    // element 7 the comment style, element 8 where the comment started.
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a /* b */ c") {C_TABLE}
                 (parse-partial-sexp 1 7))"#
        )),
        "(0 nil 1 nil t nil 0 1 3 nil nil)"
    );
}

#[test]
fn a_state_resumed_mid_delimiter_carries_the_two_character_syntax() {
    // Stopping at 4 lands between `/` and `*`. Element 10 of the state is the
    // syntax of that pending first character (720897 = Spunct | comstart-first
    // | comstart-second); resuming with it must still see the comment, and end
    // outside it.
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a /* b */ c") {C_TABLE}
                 (let ((s1 (parse-partial-sexp 1 4)))
                   (list s1 (parse-partial-sexp 4 (point-max) nil nil s1))))"#
        )),
        "((0 nil 1 nil nil nil 0 nil nil nil 720897) \
          (0 nil 11 nil nil nil 0 nil nil nil nil))"
    );
}

#[test]
fn nested_block_comments_count_their_depth() {
    // `#| b #| c |# d |# e` with the `n` flag: at position 12 the parse is two
    // comments deep and remembers the outermost start.
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a #| b #| c |# d |# e") {LISP_TABLE}
                 (list (nth 4 (parse-partial-sexp 1 12))
                       (nth 8 (parse-partial-sexp 1 12))
                       (let ((parse-sexp-ignore-comments t)) (scan-sexps 1 2))))"#
        )),
        "(2 3 22)"
    );
}

#[test]
fn forward_comment_moves_over_a_comment_in_both_directions() {
    assert_eq!(
        eval(&format!(
            r#"(with-temp-buffer (insert "a // b (c\nd") {C_TABLE}
                 (list (progn (goto-char 3) (forward-comment 1) (point))
                       (progn (goto-char (point-max)) (forward-comment -1) (point))))"#
        )),
        "(11 12)"
    );
}

#[test]
fn generic_string_and_comment_fences() {
    // `|` (string fence) and `!` (comment fence) are the two classes with no
    // paired delimiter: the same character closes what it opened.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a @b c@ d") (modify-syntax-entry ?@ "|")
                 (list (nth 3 (parse-partial-sexp 1 5)) (scan-sexps 1 2)))"#
        ),
        "(t 8)"
    );
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a %b c% d") (modify-syntax-entry ?% "!")
                 (list (nth 4 (parse-partial-sexp 1 5))
                       (nth 7 (parse-partial-sexp 1 5))))"#
        ),
        "(t syntax-table)"
    );
}

#[test]
fn math_class_pairs_a_character_with_itself() {
    // `$…$` (TeX): the Smath branch is the one that reaches `scan_lists`'s
    // `close1`/`open2` labels through a fallthrough.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a $x$ b") (modify-syntax-entry ?$ "$")
                 (list (scan-sexps 1 2) (scan-sexps (point-max) -1)))"#
        ),
        "(6 7)"
    );
}

#[test]
fn the_syntax_table_text_property_is_read_only_when_asked() {
    // A `(1)` (punctuation) property on the `"` stops it opening a string —
    // but only with `parse-sexp-lookup-properties`, which is nil by default.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a \"b c")
                 (put-text-property 3 4 'syntax-table '(1))
                 (let ((parse-sexp-lookup-properties t))
                   (nth 3 (parse-partial-sexp (point-min) (point-max)))))"#
        ),
        "nil"
    );
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a \"b c")
                 (put-text-property 3 4 'syntax-table '(1))
                 (nth 3 (parse-partial-sexp (point-min) (point-max))))"#
        ),
        "34"
    );
}

#[test]
fn open_paren_depth_and_min_depth_are_tracked_separately() {
    assert_eq!(
        eval(r#"(with-temp-buffer (insert "(((((a") (parse-partial-sexp 1 (point-max)))"#),
        "(5 5 6 nil nil nil 0 nil nil (1 2 3 4 5) nil)"
    );
    // Closing below the starting level: depth -2, and element 6 records it.
    assert_eq!(
        eval(r#"(with-temp-buffer (insert "a) b) c") (parse-partial-sexp 1 (point-max)))"#),
        "(-2 nil 7 nil nil nil -2 nil nil nil nil)"
    );
}

#[test]
fn out_of_range_positions_are_rejected_not_clamped() {
    // `validate_region' signals rather than clamping; clamping would answer a
    // plausible state for a region that does not exist.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "abc")
                 (condition-case e (parse-partial-sexp 1 40)
                   (error (list (car e) (nth 2 e) (nth 3 e)))))"#
        ),
        "(args-out-of-range 1 40)"
    );
}

#[test]
fn scan_errors_carry_their_message_and_both_positions() {
    assert_eq!(
        eval(r#"(with-temp-buffer (insert "(a b") (condition-case e (scan-sexps 1 1) (error e)))"#),
        r#"(scan-error "Unbalanced parentheses" 1 5)"#
    );
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a) b") (condition-case e (scan-lists 1 1 0) (error e)))"#
        ),
        r#"(scan-error "Containing expression ends prematurely" 2 3)"#
    );
}

#[test]
fn sexp_and_list_motion_commands() {
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "(a b) c \"d\"") (goto-char 1)
                 (list (progn (forward-sexp) (point)) (progn (forward-sexp) (point))
                       (progn (forward-sexp) (point))))"#
        ),
        "(6 8 12)"
    );
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "x (a) (b) y") (goto-char 1)
                 (list (progn (forward-list) (point)) (progn (forward-list) (point))))"#
        ),
        "(6 10)"
    );
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "(a (b c) d)")
                 (list (progn (goto-char 1) (down-list) (point)) (progn (down-list) (point))
                       (progn (up-list) (point)) (progn (up-list) (point))))"#
        ),
        "(2 5 9 12)"
    );
    // `backward-prefix-chars' moves back over the quote class AND the `p' flag.
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a ',@b") (goto-char (point-max))
                 (backward-prefix-chars) (point))"#
        ),
        "7"
    );
}

#[test]
fn syntax_ppss_and_its_accessors() {
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "a \"b") (goto-char (point-max))
                 (list (nth 3 (syntax-ppss)) (syntax-ppss-context (syntax-ppss))
                       (nth 8 (syntax-ppss))))"#
        ),
        "(34 string 3)"
    );
}

#[test]
fn matching_paren_and_syntax_after() {
    assert_eq!(
        eval(r#"(list (matching-paren ?\() (matching-paren ?\]) (matching-paren ?a))"#),
        "(41 91 nil)"
    );
    // `syntax-after' is nil outside the accessible portion (position 4 of a
    // three-character buffer is point-max).
    assert_eq!(
        eval(
            r#"(with-temp-buffer (insert "(a)")
                 (list (syntax-after 1) (syntax-after 2) (syntax-after 4)))"#
        ),
        "((4 . 41) (2) nil)"
    );
}
