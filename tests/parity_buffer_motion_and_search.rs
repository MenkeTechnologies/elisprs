//! Buffer motion, region arguments, and searching inside a restriction.
//!
//! Every buffer search ran over the WHOLE text, so a narrowed buffer was not
//! narrowed to a search: `(narrow-to-region 2 4)` and then `(search-forward
//! "a" nil t)` found the `a` at 4, outside the restriction, and `\``, `^`,
//! `\'` and `$` anchored at the buffer's ends instead of the restriction's.
//! The same family clamped or ignored what Emacs validates:
//!
//! ```text
//!                                              emacs                 elisprs (before)
//! (forward-line 0) at the end of "abc"         0, point 1            0, point 4
//! (forward-word -1) after "hello"              moves back            does not move
//! (skip-chars-forward "a-z" 2) from 1          1                     3 (LIM ignored)
//! (delete-region 0 2)                          args-out-of-range     deletes 1..2
//! (delete-char -5) from 2                      beginning-of-buffer   deletes to 1
//! (re-search-forward "b" 1 t) from 3           Invalid search bound  nil
//! (search-forward "B") in "abc"                3 (case-fold-search)  nil
//! (match-data) after a buffer search           markers               integers
//! (match-data t)                               ... BUFFER appended   no buffer
//! ```
//!
//! The fixes are ports of `Fforward_line` / `find_newline` / `bol` / `eol`,
//! `scan_words`, `skip_chars`, `validate_region` / `fix_position`,
//! `Fdelete_char`, `search_command`, `Fmatch_data` / `Fset_match_data`, and the
//! indent.c column functions. `current-column` also counted every character as
//! one column; it now uses `tab-width` and `char-width` as `scan_for_column`
//! does.
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1,
//! each form wrapped in `(condition-case e FORM (error (list 'signal e)))`.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn forward_line_and_line_positions_follow_find_newline() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (forward-line 0) (point) (bolp) (eolp) (bobp) (eobp))) (error (list 'signal e)))"##,
        r##"(0 1 t nil t nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef\nghi") (goto-char 6) (list (forward-line 0) (point))) (error (list 'signal e)))"##,
        r##"(0 5)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef\nghi") (goto-char 6) (list (forward-line -1) (point) (forward-line -5) (point))) (error (list 'signal e)))"##,
        r##"(0 1 -5 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef\nghi") (goto-char 1) (list (forward-line 5) (point))) (error (list 'signal e)))"##,
        r##"(2 12)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef\n") (goto-char 1) (list (forward-line 5) (point))) (error (list 'signal e)))"##,
        r##"(3 9)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef\n") (list (forward-line 1) (point))) (error (list 'signal e)))"##,
        r##"(1 9)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (list (forward-line 1) (forward-line -1) (forward-line 0))) (error (list 'signal e)))"##,
        r##"(1 -1 0)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb") (goto-char 1) (list (pos-bol 2) (pos-eol 2) (line-end-position 0) (line-beginning-position 0) (line-end-position -1) (pos-bol 5) (pos-eol 5))) (error (list 'signal e)))"##,
        r##"(3 4 1 1 1 4 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (goto-char (point-max)) (list (pos-bol 0) (pos-eol 0) (pos-bol -1) (pos-eol -1) (pos-bol -9))) (error (list 'signal e)))"##,
        r##"(3 4 1 2 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (goto-char 1) (beginning-of-line 2) (list (point) (progn (end-of-line 2) (point)) (progn (end-of-line 0) (point)))) (error (list 'signal e)))"##,
        r##"(3 6 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (forward-line 2.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integerp 2.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (forward-line (expt 2 70))) (error (list 'signal e)))"##,
        r##"1180591620717411303424"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (list (forward-line (- (expt 2 70))) (point))) (error (list 'signal e)))"##,
        r##"(-1180591620717411303422 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\nb\nc") (pos-bol 1.5)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integerp 1.5))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef") (narrow-to-region 2 6) (goto-char 3) (list (forward-line 1) (point) (forward-line -3) (point))) (error (list 'signal e)))"##,
        r##"(0 5 -2 2)"##,
    );
}

#[test]
fn forward_word_follows_word_syntax_and_answers_nil_at_the_limit() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello world") (goto-char 1) (list (forward-word 1) (point) (forward-word -1) (point))) (error (list 'signal e)))"##,
        r##"(t 6 t 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo_bar a\\b x") (goto-char 1) (list (forward-word) (point) (progn (forward-word 2) (point)) (forward-word -1) (point))) (error (list 'signal e)))"##,
        r##"(t 4 10 t 9)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a b") (goto-char 1) (list (forward-word 5) (point) (forward-word -9) (point))) (error (list 'signal e)))"##,
        r##"(nil 4 nil 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "  ") (goto-char 1) (list (forward-word) (point))) (error (list 'signal e)))"##,
        r##"(nil 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "one two three") (goto-char (point-max)) (list (backward-word 2) (point) (backward-word) (point) (backward-word) (point))) (error (list 'signal e)))"##,
        r##"(t 5 t 1 nil 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "one two three") (forward-word 1.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump 1.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "one two three") (backward-word 'a)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument number-or-marker-p a))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "one two three") (backward-word 1.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump -1.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "one two three") (goto-char 1) (list (forward-word 0) (point))) (error (list 'signal e)))"##,
        r##"(t 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc def ghi") (narrow-to-region 3 7) (goto-char 3) (list (forward-word 3) (point) (forward-word -3) (point))) (error (list 'signal e)))"##,
        r##"(nil 7 nil 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a1 b2_c") (goto-char 1) (list (forward-word 2) (point))) (error (list 'signal e)))"##,
        r##"(t 6)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "x a\\b y") (goto-char 3) (let ((words-include-escapes t)) (list (forward-word) (point)))) (error (list 'signal e)))"##,
        r##"(t 6)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "é ñ") (goto-char 1) (list (forward-word 2) (point))) (error (list 'signal e)))"##,
        r##"(t 4)"##,
    );
}

#[test]
fn searches_stay_inside_the_restriction_and_check_their_bound() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (search-forward "c" nil t) (search-forward "a" nil t) (point))) (error (list 'signal e)))"##,
        r##"(4 nil 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 3) (list (search-backward "a" nil t) (point))) (error (list 'signal e)))"##,
        r##"(nil 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (re-search-forward "c\\'" nil t) (point))) (error (list 'signal e)))"##,
        r##"(4 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (re-search-forward "^b" nil t) (point))) (error (list 'signal e)))"##,
        r##"(3 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (re-search-forward "a" nil t) (point))) (error (list 'signal e)))"##,
        r##"(nil 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 4) (list (re-search-backward "a" nil t) (point))) (error (list 'signal e)))"##,
        r##"(nil 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (looking-at "bc\\'") (looking-at "bca"))) (error (list 'signal e)))"##,
        r##"(t nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 4) (goto-char 2) (list (re-search-forward "\\`b" nil t))) (error (list 'signal e)))"##,
        r##"(3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (search-forward "b" 1 t)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (search-backward "b" 5 t)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (re-search-backward "b" 5 t)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (re-search-forward "b" 1 t)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (search-forward "b" 2.5 t)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integer-or-marker-p 2.5))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (search-forward "c" 100 t) (re-search-forward "c" 100 t))) (error (list 'signal e)))"##,
        r##"(4 7)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (search-forward "c" 0 t)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "c" 3 t) (re-search-forward "c" 4 t))) (error (list 'signal e)))"##,
        r##"(nil 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "\\(b\\)" nil t) (match-data t))) (error (list 'signal e)))"##,
        r##"(3 (2 3 2 3 #<killed buffer>))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "\\(b\\)" nil t) (match-data))) (error (list 'signal e)))"##,
        r##"(3 (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "\\(b\\)" nil t) (let ((m (match-data))) (list (markerp (car m)) (marker-position (car m)))))) (error (list 'signal e)))"##,
        r##"(3 (t 2))"##,
    );
    check(
        r##"(condition-case e (progn (string-match "b" "abc") (match-data t)) (error (list 'signal e)))"##,
        r##"(1 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (re-search-forward "b") (let ((l (list 0 0 0))) (match-data t l))) (error (list 'signal e)))"##,
        r##"(2 3 #<killed buffer>)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab cd") (goto-char 1) (re-search-forward "\\<c") ) (error (list 'signal e)))"##,
        r##"5"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab cd") (goto-char 4) (list (looking-back "ab " nil) (looking-back "b \\=" 1))) (error (list 'signal e)))"##,
        r##"(t t)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (narrow-to-region 2 3) (goto-char 2) (list (looking-at "b\\'") (looking-at "b$"))) (error (list 'signal e)))"##,
        r##"(t t)"##,
    );
}

#[test]
fn columns_and_indentation_follow_indent_c() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\n  def") (goto-char (point-max)) (list (current-indentation) (current-column))) (error (list 'signal e)))"##,
        r##"(2 5)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "\t x") (list (current-indentation) (current-column))) (error (list 'signal e)))"##,
        r##"(9 10)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (goto-char (point-max)) (current-column)) (error (list 'signal e)))"##,
        r##"9"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "中文a") (current-column)) (error (list 'signal e)))"##,
        r##"5"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\001b") (current-column)) (error (list 'signal e)))"##,
        r##"4"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (let ((tab-width 4)) (current-column))) (error (list 'signal e)))"##,
        r##"5"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (let ((tab-width 0)) (current-column))) (error (list 'signal e)))"##,
        r##"9"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (goto-char 1) (list (move-to-column 3) (point))) (error (list 'signal e)))"##,
        r##"(3 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (move-to-column 5 t) (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(5 6 "ab   ")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (move-to-column 5) (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(2 3 "ab")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (move-to-column 5 'x) (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(2 3 "ab")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (list (move-to-column 3) (point))) (error (list 'signal e)))"##,
        r##"(8 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (list (move-to-column 3 'x) (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(3 4 "a  	b")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (list (move-to-column 3 t) (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(3 4 "a  	b")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (move-to-column -1)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument wholenump -1))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\tb") (move-to-column 1.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument wholenump 1.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "x\nabc") (goto-char 1) (list (move-to-column 9) (point))) (error (list 'signal e)))"##,
        r##"(1 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (indent-to 6) (buffer-string))) (error (list 'signal e)))"##,
        r##"(6 "abc   ")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (indent-to 10) (buffer-string))) (error (list 'signal e)))"##,
        r##"(10 "abc	  ")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (indent-to 2) (buffer-string))) (error (list 'signal e)))"##,
        r##"(3 "abc")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (indent-to 2 3) (buffer-string))) (error (list 'signal e)))"##,
        r##"(6 "abc   ")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (let ((indent-tabs-mode nil)) (list (indent-to 17) (buffer-string)))) (error (list 'signal e)))"##,
        r##"(17 "abc              ")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (indent-to 17 nil) (point))) (error (list 'signal e)))"##,
        r##"(17 7)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (indent-to 'a)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump a))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcd") (goto-char 3) (list (current-column) (indent-to 9) (buffer-string))) (error (list 'signal e)))"##,
        r##"(2 9 "ab	 cd")"##,
    );
}

#[test]
fn buffer_helpers_from_subr_simple_and_editfns() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (list (buffer-narrowed-p) (progn (narrow-to-region 2 4) (buffer-narrowed-p)))) (error (list 'signal e)))"##,
        r##"(nil t)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 1) (list (search-forward-regexp "c") (match-end 0) (search-backward-regexp "a") (point))) (error (list 'signal e)))"##,
        r##"(4 4 1 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (count-matches "b")) (error (list 'signal e)))"##,
        r##"2"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (insert-buffer-substring (current-buffer) 1 3) (buffer-string)) (error (list 'signal e)))"##,
        r##""abcab""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a") (let ((b (current-buffer))) (with-temp-buffer (insert-buffer-substring b) (buffer-string)))) (error (list 'signal e)))"##,
        r##""a""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert (propertize "ab" 'face 'bold)) (let ((b (current-buffer))) (with-temp-buffer (insert-buffer-substring b) (buffer-string)))) (error (list 'signal e)))"##,
        r##"#("ab" 0 2 (face bold))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert (propertize "ab" 'face 'bold)) (let ((b (current-buffer))) (with-temp-buffer (insert-buffer-substring-no-properties b) (buffer-string)))) (error (list 'signal e)))"##,
        r##""ab""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (insert-buffer-substring (current-buffer) 0 2)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range 0 2))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (insert-buffer-substring "nope")) (error (list 'signal e)))"##,
        r##"(signal (error "No buffer named nope"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (insert-buffer-substring 5)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument stringp 5))"##,
    );
    check(
        r##"(condition-case e (let ((b (generate-new-buffer "x"))) (kill-buffer b) (with-temp-buffer (insert-buffer-substring b))) (error (list 'signal e)))"##,
        r##"(signal (error "Selecting deleted buffer"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (insert-buffer-substring (current-buffer) 3 1) (buffer-string) (point))) (error (list 'signal e)))"##,
        r##"(nil "abcab" 6)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (marker-position (point-min-marker)) (marker-position (point-max-marker)) (progn (narrow-to-region 2 3) (list (marker-position (point-min-marker)) (marker-position (point-max-marker)))))) (error (list 'signal e)))"##,
        r##"(1 4 (2 3))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (delete-and-extract-region 2 2) (delete-and-extract-region 3 1) (buffer-string))) (error (list 'signal e)))"##,
        r##"("" "ab" "c")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert (propertize "abc" 'x 1)) (delete-and-extract-region 1 3)) (error (list 'signal e)))"##,
        r##"#("ab" 0 2 (x 1))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (delete-and-extract-region 0 3)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range #<killed buffer> 0 3))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 1) (replace-string-in-region "b" "XX") (buffer-string)) (error (list 'signal e)))"##,
        r##""aXXc""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcb") (list (replace-regexp-in-region "b" "Y" 1 5) (buffer-string))) (error (list 'signal e)))"##,
        r##"(2 "aYcY")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "aBcb") (list (replace-regexp-in-region "b" "Y" 1 5) (buffer-string))) (error (list 'signal e)))"##,
        r##"(1 "aBcY")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcb") (list (replace-regexp-in-region "\\(b\\)" "<\\1>" 2) (buffer-string))) (error (list 'signal e)))"##,
        r##"(2 "a<b>c<b>")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcb") (list (replace-string-in-region "z" "Y" 1 5) (buffer-string))) (error (list 'signal e)))"##,
        r##"(nil "abcb")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcb") (replace-string-in-region "b" "Y" 0 5)) (error (list 'signal e)))"##,
        r##"(signal (error "Start before start of buffer"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcb") (replace-string-in-region "b" "Y" 1 9)) (error (list 'signal e)))"##,
        r##"(signal (error "End after end of buffer"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\n  def") (goto-char (point-max)) (back-to-indentation) (list (point) (current-indentation) (current-column))) (error (list 'signal e)))"##,
        r##"(7 2 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "  'x") (back-to-indentation) (point)) (error (list 'signal e)))"##,
        r##"3"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\n\n\n\nb") (goto-char 3) (delete-blank-lines) (buffer-string)) (error (list 'signal e)))"##,
        r##""a

b""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\n\nb") (goto-char 3) (delete-blank-lines) (buffer-string)) (error (list 'signal e)))"##,
        r##""a
b""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\n\n\nb") (goto-char 1) (delete-blank-lines) (buffer-string)) (error (list 'signal e)))"##,
        r##""a
b""##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a\n  \n") (goto-char 3) (delete-blank-lines) (list (point) (buffer-string))) (error (list 'signal e)))"##,
        r##"(3 "a
")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 2) (list (point) (backward-prefix-chars) (point))) (error (list 'signal e)))"##,
        r##"(2 nil 2)"##,
    );
}

#[test]
fn region_arguments_are_validated_not_clamped() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello") (delete-region 0 2)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range #<killed buffer> 0 2))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello") (delete-region 2.5 3)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integer-or-marker-p 2.5))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (delete-region 1 (expt 2 70))) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range #<killed buffer> 1 1180591620717411303424))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello") (goto-char 2) (delete-char -5)) (error (list 'signal e)))"##,
        r##"(signal (beginning-of-buffer))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 2) (delete-char 3)) (error (list 'signal e)))"##,
        r##"(signal (end-of-buffer))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (delete-char 1.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump 1.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab\nc") (count-lines 0 9)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range 0 9))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab\nc\n") (list (count-lines 1 6) (count-lines 6 1) (count-lines 2 2) (count-lines 1 4))) (error (list 'signal e)))"##,
        r##"(2 2 0 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (narrow-to-region 0 3)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range 0 3))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (narrow-to-region 1.5 3)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integer-or-marker-p 1.5))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (narrow-to-region 2 4) (narrow-to-region 1 7) (buffer-string)) (error (list 'signal e)))"##,
        r##""abcdef""##,
    );
}

#[test]
fn skip_chars_stops_at_lim_and_the_restriction() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 1) (list (skip-chars-forward "a-z" 2) (point))) (error (list 'signal e)))"##,
        r##"(1 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 1) (list (skip-chars-forward "a-z" 99) (point) (skip-chars-backward "a-z" -5) (point))) (error (list 'signal e)))"##,
        r##"(3 4 -3 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcdef") (narrow-to-region 2 4) (goto-char 2) (list (skip-chars-forward "a-z") (point) (skip-chars-backward "a-z") (point))) (error (list 'signal e)))"##,
        r##"(2 4 -2 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 3) (list (skip-chars-forward "a-z" 1) (point))) (error (list 'signal e)))"##,
        r##"(0 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (skip-chars-forward "a" 1.5)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument integer-or-marker-p 1.5))"##,
    );
}

#[test]
fn match_data_reports_markers_and_its_buffer() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "fo") (let (r) (save-match-data (re-search-forward "bar") (push (match-beginning 0) r)) (push (match-data) r) r)) (error (list 'signal e)))"##,
        r##"((#<marker in no buffer> #<marker in no buffer>) 5)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "o+") (let ((saved (match-data))) (re-search-forward "bar") (set-match-data saved) (list (match-beginning 0) (match-end 0) (match-string 0)))) (error (list 'signal e)))"##,
        r##"(2 4 "oo")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "o+") (save-match-data (re-search-forward "bar")) (list (match-beginning 0) (match-string 0))) (error (list 'signal e)))"##,
        r##"(2 "oo")"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "\\(z\\)?o") (match-data t)) (error (list 'signal e)))"##,
        r##"(2 3 #<killed buffer>)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "o") (let ((l (list 'a 'b 'c 'd 'e))) (list (match-data t l) l))) (error (list 'signal e)))"##,
        r##"((2 3 #<killed buffer> nil nil) (2 3 #<killed buffer> nil nil))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "o") (let* ((l (list 0 0)) (r (match-data nil l))) (list (eq r l) (mapcar #'marker-position r)))) (error (list 'signal e)))"##,
        r##"(t (2 3))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo bar") (goto-char 1) (re-search-forward "o") (let* ((m (copy-marker 3)) (l (list m 9))) (match-data t l t) (list (marker-position m) l))) (error (list 'signal e)))"##,
        r##"(nil (2 3 #<killed buffer>))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "foo") (set-match-data (list (copy-marker 2) (copy-marker 3))) (list (match-data t) (match-string 0))) (error (list 'signal e)))"##,
        r##"((2 3 #<killed buffer>) "o")"##,
    );
    check(
        r##"(condition-case e (progn (set-match-data '(1 2 nil nil 3 4)) (match-data)) (error (list 'signal e)))"##,
        r##"(1 2 nil nil 3 4)"##,
    );
    check(
        r##"(condition-case e (progn (set-match-data '(1 2 nil nil)) (match-data)) (error (list 'signal e)))"##,
        r##"(1 2)"##,
    );
    check(
        r##"(condition-case e (set-match-data 5) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument listp 5))"##,
    );
    check(
        r##"(condition-case e (progn (string-match "\\(a\\)" "xa") (match-data t)) (error (list 'signal e)))"##,
        r##"(1 2 1 2)"##,
    );
    check(
        r##"(condition-case e (progn (string-match "b" "abc") (match-data nil (list 9 9 9))) (error (list 'signal e)))"##,
        r##"(1 2 nil)"##,
    );
}

#[test]
fn literal_search_folds_case_and_bound_and_count_follow_search_command() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "aBc") (goto-char 1) (list (search-forward "b" nil t) (progn (goto-char 1) (search-forward "C" nil t)) (let ((case-fold-search nil)) (goto-char 1) (search-forward "b" nil t)))) (error (list 'signal e)))"##,
        r##"(3 4 nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "aBc") (goto-char 4) (list (search-backward "b" nil t) (let ((case-fold-search nil)) (search-backward "b" nil t)))) (error (list 'signal e)))"##,
        r##"(2 nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "a.c abc") (goto-char 1) (list (search-forward "b" nil t) (progn (goto-char 1) (search-forward "." nil t)) (search-forward "[" nil t))) (error (list 'signal e)))"##,
        r##"(7 3 nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (search-forward "c" nil t 0) (point))) (error (list 'signal e)))"##,
        r##"(1 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (search-forward "c" 5 t 0)) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid search bound (wrong side of point)"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (search-forward "c" nil t 1.0)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump 1.0))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (search-forward 'c nil t)) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument stringp c))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 4) (list (search-forward "c" nil 1 -1) (point))) (error (list 'signal e)))"##,
        r##"(3 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "a.*" 3 t) (point))) (error (list 'signal e)))"##,
        r##"(3 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 1) (list (re-search-forward "b\\'" 3 t) (re-search-forward "b$" 3 t))) (error (list 'signal e)))"##,
        r##"(nil nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "aaa") (goto-char 4) (list (re-search-backward "a+" nil t) (match-beginning 0) (match-end 0))) (error (list 'signal e)))"##,
        r##"(3 3 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (goto-char 3) (list (re-search-backward "b\\'" nil t) (re-search-backward "b$" nil t))) (error (list 'signal e)))"##,
        r##"(nil nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (narrow-to-region 2 6) (goto-char 6) (list (re-search-backward "\\`b" nil t) (re-search-backward "^c" nil t))) (error (list 'signal e)))"##,
        r##"(2 nil)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc\ndef") (goto-char 1) (list (re-search-forward "^d" nil t) (re-search-backward "c$" nil t))) (error (list 'signal e)))"##,
        r##"(6 3)"##,
    );
}
