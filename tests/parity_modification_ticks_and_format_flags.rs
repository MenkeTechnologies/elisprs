//! Buffer modification state, byte positions, two `format` corners, and `\?`
//! in replacement text.
//!
//! - `buffer-modified-p`, `set-buffer-modified-p`, `restore-buffer-modified-p`,
//!   `buffer-modified-tick`, `buffer-chars-modified-tick` and
//!   `with-silent-modifications` were void. A text change of N characters
//!   advances `MODIFF` by `modiff_incr` (floor(log2 N) + 1) and sets
//!   `CHARS_MODIFF` to it; a text-property change that changes something
//!   advances `MODIFF` by one; `set-text-properties` answers nil and changes
//!   nothing on an empty range or on a buffer with no interval tree (which
//!   outlives its properties until all of the text is deleted).
//! - `upcase-region` / `downcase-region` / `capitalize-region` and
//!   `subst-char-in-region` deleted and re-inserted the region, collapsing the
//!   markers inside it; they now rewrite it in place as `casify_region` and
//!   `Fsubst_char_in_region` do.
//! - `position-bytes` and `byte-to-position` were void.
//! - `styled_format`: a `%` conversion ignores its flags and width and consumes
//!   no argument; a field number repositions the argument counter
//!   (`(format "%2$s %s" 1 2 3)` is "2 3"); the `#` flag always leaves a
//!   decimal point.
//! - `Freplace_match`: `\?` is kept in a string and is an error in a buffer.
//!
//! Every expectation is `emacs -Q --batch --eval '(prin1 FORM)'` on GNU
//! Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn modification_ticks_follow_modiff_incr() {
    check(
        r##"(condition-case e (list (buffer-modified-p) (buffer-modified-tick) (buffer-chars-modified-tick)) (error (list 'signal e)))"##,
        r##"(nil 1 1)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcd") (list (buffer-modified-tick) (progn (erase-buffer) (buffer-modified-tick)) (progn (insert (make-string 1000 ?a)) (buffer-modified-tick)) (progn (put-text-property 1 3 'face nil) (put-text-property 1 3 'face nil) (buffer-modified-tick)) (progn (set-text-properties 5 6 nil) (buffer-modified-tick)) (progn (remove-text-properties 5 6 '(zz nil)) (buffer-modified-tick)) (progn (narrow-to-region 2 3) (widen) (buffer-modified-tick)) (set-buffer-modified-p nil) (restore-buffer-modified-p t) (list (buffer-modified-p) (buffer-modified-tick)) (progn (upcase-region 1 3) (buffer-modified-tick)) (progn (make-overlay 1 2) (buffer-modified-tick)) (buffer-chars-modified-tick))) (error (list 'signal e)))"##,
        r##"(4 7 17 18 19 19 19 nil t (t 20) 22 22 22)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (progn (put-text-property 1 2 'x 1) (buffer-modified-tick)) (progn (put-text-property 1 2 'x 1) (buffer-modified-tick)) (progn (add-text-properties 1 2 '(x 1)) (buffer-modified-tick)) (progn (set-text-properties 1 2 '(x 1)) (buffer-modified-tick)) (progn (set-text-properties 1 3 nil) (buffer-modified-tick)) (progn (set-text-properties 1 3 nil) (buffer-modified-tick)) (progn (remove-text-properties 1 3 '(x nil)) (buffer-modified-tick)) (buffer-chars-modified-tick))) (error (list 'signal e)))"##,
        r##"(4 4 4 5 6 7 7 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (list (buffer-modified-p) (progn (insert "a") (buffer-modified-p)) (set-buffer-modified-p nil) (buffer-modified-p) (progn (put-text-property 1 2 'q 1) (buffer-modified-p)) (restore-buffer-modified-p 'autosaved) (buffer-modified-p) (progn (insert "b") (buffer-modified-p)))) (error (list 'signal e)))"##,
        r##"(nil t nil nil t autosaved autosaved t)"##,
    );
    check(
        r##"(condition-case e (let ((b (generate-new-buffer "m"))) (prog1 (list (buffer-modified-p b) (buffer-modified-tick b) (with-current-buffer b (insert "x") (buffer-modified-tick)) (buffer-modified-p b) (buffer-chars-modified-tick b)) (kill-buffer b))) (error (list 'signal e)))"##,
        r##"(nil 1 2 t 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (with-silent-modifications (put-text-property 1 2 'face 'bold)) (list (buffer-modified-p) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(t 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (set-buffer-modified-p t)) (error (list 'signal e)))"##,
        r##"nil"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (list (set-buffer-modified-p 'x) (buffer-modified-p) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil t 2)"##,
    );
    check(
        r##"(condition-case e (buffer-modified-p 'foo) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument bufferp foo))"##,
    );
    check(
        r##"(condition-case e (buffer-modified-tick "nonexistent-buf") (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument bufferp "nonexistent-buf"))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (set-text-properties 1 3 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (put-text-property 1 2 'x 1) (remove-text-properties 1 3 '(x nil)) (list (set-text-properties 1 3 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(t 6)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (put-text-property 1 2 'x 1) (erase-buffer) (insert "cd") (list (set-text-properties 1 3 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil 8)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (put-text-property 1 2 'x 1) (delete-region 1 2) (list (set-text-properties 1 2 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(t 6)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert (propertize "a" 'x 1) "b") (list (set-text-properties 2 3 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(t 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (put-text-property 1 2 'x nil) (list (set-text-properties 1 3 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(t 5)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (list (set-text-properties 1 1 nil) (set-text-properties 1 1 '(a 1)) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil nil 3)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ab") (put-text-property 1 2 'x 1) (list (set-text-properties 2 2 nil) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello world") (let ((m (copy-marker 4)) (m2 (copy-marker 8 t))) (goto-char 6) (upcase-region 2 10) (list (buffer-string) (marker-position m) (marker-position m2) (point) (buffer-modified-tick)))) (error (list 'signal e)))"##,
        r##"("hELLO WORld" 4 8 6 9)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert (propertize "abc" 'face 'bold)) (downcase-region 1 3) (upcase-region 1 3) (list (buffer-string) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(#("ABc" 0 3 (face bold)) 7)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (upcase-region 2 2) (buffer-modified-tick)) (error (list 'signal e)))"##,
        r##"3"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (upcase-region 3 1) (list (buffer-string) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"("ABc" 5)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "ABC") (upcase-region 1 4) (buffer-modified-tick)) (error (list 'signal e)))"##,
        r##"5"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (let ((m (copy-marker 5))) (list (subst-char-in-region 1 7 ?b ?x) (buffer-string) (marker-position m) (buffer-modified-tick)))) (error (list 'signal e)))"##,
        r##"(nil "axcaxc" 5 7)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (list (subst-char-in-region 1 7 ?z ?x) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"(nil 4)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abcabc") (subst-char-in-region 1 7 ?c ?x) (buffer-modified-tick)) (error (list 'signal e)))"##,
        r##"7"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "hello world") (capitalize-region 1 12) (list (buffer-string) (buffer-modified-tick))) (error (list 'signal e)))"##,
        r##"("Hello World" 9)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (setq buffer-read-only t) (condition-case e (upcase-region 1 3) (error e))) (error (list 'signal e)))"##,
        r##"(buffer-read-only #<killed buffer>)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (upcase-region 0 3)) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range #<killed buffer> 0 3))"##,
    );
}

#[test]
fn position_bytes_and_byte_to_position() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "héllo中x") (list (mapcar #'position-bytes '(1 2 3 4 6 7 8 9 0)) (mapcar #'byte-to-position '(0 1 2 3 4 5 6 7 8 9 10 11 12 13)))) (error (list 'signal e)))"##,
        r##"((1 2 4 5 7 10 11 nil nil) (nil 1 2 2 3 4 5 6 6 6 7 8 nil nil))"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "héllo") (narrow-to-region 3 4) (list (position-bytes 1) (position-bytes 6) (byte-to-position 1) (byte-to-position 7) (position-bytes (copy-marker 2)))) (error (list 'signal e)))"##,
        r##"(1 7 1 6 2)"##,
    );
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (list (position-bytes 2) (byte-to-position 2))) (error (list 'signal e)))"##,
        r##"(2 2)"##,
    );
    check(
        r##"(condition-case e (condition-case e (position-bytes 1.5) (error e)) (error (list 'signal e)))"##,
        r##"(wrong-type-argument integer-or-marker-p 1.5)"##,
    );
    check(
        r##"(condition-case e (condition-case e (byte-to-position 1.5) (error e)) (error (list 'signal e)))"##,
        r##"(wrong-type-argument fixnump 1.5)"##,
    );
    check(
        r##"(condition-case e (condition-case e (byte-to-position 'a) (error e)) (error (list 'signal e)))"##,
        r##"(wrong-type-argument fixnump a)"##,
    );
    check(
        r##"(condition-case e (condition-case e (position-bytes nil) (error e)) (error (list 'signal e)))"##,
        r##"(wrong-type-argument integer-or-marker-p nil)"##,
    );
    check(
        r##"(condition-case e (condition-case e (byte-to-position (expt 2 70)) (error (list 'signal e))) (error (list 'signal e)))"##,
        r##"(signal (wrong-type-argument fixnump 1180591620717411303424))"##,
    );
}

#[test]
fn format_percent_and_alt_flag_follow_styled_format() {
    check(
        r##"(condition-case e (format "%3%") (error (list 'signal e)))"##,
        r##""%""##,
    );
    check(
        r##"(condition-case e (format "%-5%|%05.2%") (error (list 'signal e)))"##,
        r##""%|%""##,
    );
    check(
        r##"(condition-case e (format "%1$% %s" 'a 'b) (error (list 'signal e)))"##,
        r##""% a""##,
    );
    check(
        r##"(condition-case e (format "%2$% %s" 'a 'b 'c) (error (list 'signal e)))"##,
        r##""% b""##,
    );
    check(
        r##"(condition-case e (format "%2$s %s" 1 2 3) (error (list 'signal e)))"##,
        r##""2 3""##,
    );
    check(
        r##"(condition-case e (format "%#g|%#.3g|%#.0g|%#e|%#.0e|%#.0f|%#.1g|%#5.0f|%-#6.0e|" 100000.0 100000.0 100000.0 100000.0 100000.0 100000.0 100000.0 100000.0 100000.0) (error (list 'signal e)))"##,
        r##""100000.|1.00e+05|1.e+05|1.000000e+05|1.e+05|100000.|1.e+05|100000.|1.e+05|""##,
    );
    check(
        r##"(condition-case e (format "%#g|%#.0g|%#.0e|%#.0f|%#5.0f|" 1.0 1.0 1.0 1.0 1.0) (error (list 'signal e)))"##,
        r##""1.00000|1.|1.e+00|1.|   1.|""##,
    );
    check(
        r##"(condition-case e (format "%#g|%#.0g|%#.0e|%#.0f|%#.1g|" 0.5 0.5 0.5 0.5 0.5) (error (list 'signal e)))"##,
        r##""0.500000|0.5|5.e-01|0.|0.5|""##,
    );
    check(
        r##"(condition-case e (format "%#.0f|%#.0e|%#g" -2.0 -2.0 -2.0) (error (list 'signal e)))"##,
        r##""-2.|-2.e+00|-2.00000""##,
    );
}

#[test]
fn backslash_question_mark_in_replacement_text() {
    check(
        r##"(condition-case e (with-temp-buffer (insert "abc") (goto-char 1) (search-forward "b") (condition-case e (progn (replace-match "\\?") (buffer-string)) (error (list 'signal e)))) (error (list 'signal e)))"##,
        r##"(signal (error "Invalid use of ‘\\’ in replacement text"))"##,
    );
    check(
        r##"(condition-case e (let ((s "abc")) (string-match "b" s) (replace-match "x\\?y" nil nil s)) (error (list 'signal e)))"##,
        r##""ax\\?yc""##,
    );
    check(
        r##"(condition-case e (replace-regexp-in-string "b" "\\?" "abc") (error (list 'signal e)))"##,
        r##""a\\?c""##,
    );
}
