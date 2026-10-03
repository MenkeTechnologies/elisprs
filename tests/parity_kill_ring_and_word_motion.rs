//! Round 33: literal searches under `case-fold-search`, syntax.c word motion,
//! the simple.el kill ring and transpose commands, `buffer-read-only`, the
//! return values of the text-property mutators, plist copying when text moves
//! between objects, and the thingatpt.el core.
//!
//! Every expectation was byte-checked against GNU Emacs 31.1
//! (`emacs -Q --batch`, `lexical-binding` t).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// search.c `search_buffer_non_re` compares through the case canon table when
/// `case-fold-search` is non-nil, in both directions and beyond ASCII.
#[test]
fn literal_search_folds_case() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"Hello ÉTÉ\") (goto-char 1) \
               (list (search-forward \"hello\" nil t) (search-forward \"été\" nil t) \
                     (progn (goto-char (point-max)) (search-backward \"HEL\" nil t)) \
                     (let ((case-fold-search nil)) (goto-char 1) (search-forward \"hello\" nil t))))"
        ),
        "(6 10 1 nil)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"Hello\") (goto-char 1) (search-forward \"hello\") \
               (replace-match \"world\") (buffer-string))"
        ),
        "\"World\""
    );
}

/// syntax.c `scan_words` / `Fforward_word`: negative counts, the accessible
/// portion as the limit, and t only when every word was found.
#[test]
fn forward_word_is_scan_words() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo-bar_baz qux\") (goto-char 1) \
               (list (forward-word 2) (point) (forward-word 5) (point) \
                     (backward-word 1) (point) (forward-word -9) (point)))"
        ),
        "(t 8 nil 16 t 13 nil 1)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"ab cd ef\") (narrow-to-region 4 6) (goto-char 4) \
               (list (forward-word 2) (point) (forward-word -3) (point)))"
        ),
        "(nil 6 nil 4)"
    );
    // casefiddle.c `casify_word` with a negative count works on the words
    // before point and leaves point where it was.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello world\") (goto-char 8) (upcase-word -5) \
               (list (point) (buffer-string)))"
        ),
        "(8 \"HELLO World\")"
    );
}

/// simple.el: kills land in `kill-ring`, `yank` reinserts and leaves the mark
/// at the start, and with no command loop `last-command' never appends.
#[test]
fn kill_ring_commands() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello world foo\") (goto-char 1) (kill-word 1) \
               (kill-word 1) (goto-char (point-max)) (yank) \
               (list (buffer-string) kill-ring (mark) (point) this-command))"
        ),
        "(\" foo world\" (\" world\" \"hello\") 5 11 yank)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"l1\\nl2\\nl3\") (goto-char 1) \
               (list (progn (kill-line) (buffer-string)) (progn (kill-line) (buffer-string)) \
                     (progn (kill-line 2) (buffer-string)) kill-ring))"
        ),
        "(\"\nl2\nl3\" \"l2\nl3\" \"\" (\"l2\nl3\" \"\n\" \"l1\"))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"l1\") (condition-case e (kill-line) (error e)))"),
        "(end-of-buffer)"
    );
    assert_eq!(
        eval(
            "(progn (kill-new \"a\") (kill-new \"b\") (kill-new \"c\") \
               (list (current-kill 0) (current-kill 1) (current-kill 1) \
                     kill-ring-yank-pointer (current-kill -1 t)))"
        ),
        "(\"c\" \"b\" \"a\" (\"a\") \"b\")"
    );
    assert_eq!(
        eval(
            "(list (condition-case e (current-kill 0) (error e)) \
                   (progn (kill-new \"a\") (kill-append \"b\" nil) (kill-append \"z\" t) \
                          (kill-new \"y\" t) kill-ring))"
        ),
        "((error \"Kill ring is empty\") (\"y\"))"
    );
    assert_eq!(
        eval("(let ((kill-ring-max 2)) (kill-new \"a\") (kill-new \"b\") (kill-new \"c\") kill-ring)"),
        "(\"c\" \"b\")"
    );
}

/// simple.el `transpose-subr`, driven by characters, words and lines.
#[test]
fn transpose_commands() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcd\") (goto-char 2) \
               (list (progn (transpose-chars 2) (list (point) (buffer-string))) \
                     (progn (goto-char (point-max)) (transpose-chars 1) (list (point) (buffer-string))) \
                     (progn (goto-char 1) (condition-case e (transpose-chars 1) (error e)))))"
        ),
        "((4 \"bcad\") (5 \"bcda\") (beginning-of-buffer))"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"one two three\") (goto-char 5) (transpose-words 1) \
               (list (point) (buffer-string)))"
        ),
        "(8 \"two one three\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"l1\\nl2\\nl3\") (goto-char 5) (transpose-lines 1) \
               (list (point) (buffer-string)))"
        ),
        "(7 \"l2\nl1\nl3\")"
    );
}

/// insdel.c `prepare_to_modify_buffer`: a non-empty change to a read-only
/// buffer signals, `inhibit-read-only` lifts it, and `kill-region` still
/// copies the text before re-signalling. cmds.c `Fdelete_char` signals past
/// either end instead of clamping.
#[test]
fn read_only_buffers_and_delete_char_bounds() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (setq buffer-read-only t) \
               (list (condition-case e (insert \"x\") (error (car e))) \
                     (condition-case e (delete-region 1 2) (error (car e))) \
                     (condition-case e (erase-buffer) (error (car e))) \
                     (condition-case e (kill-region 1 3) (error (list (car e) (car kill-ring)))) \
                     (insert \"\") \
                     (let ((inhibit-read-only t)) (insert \"z\") (buffer-string))))"
        ),
        "(buffer-read-only buffer-read-only buffer-read-only (buffer-read-only \"ab\") nil \"abcz\")"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (goto-char 3) \
               (list (condition-case e (delete-char 2) (error e)) \
                     (condition-case e (delete-char -3) (error e)) \
                     (progn (delete-char -2) (buffer-string))))"
        ),
        "((end-of-buffer) (beginning-of-buffer) \"c\")"
    );
}

/// textprop.c: `add-text-properties` / `remove-text-properties` answer t only
/// when something changed, `set-text-properties` answers nil only when there
/// was nothing to clear.
#[test]
fn text_property_mutators_report_change() {
    assert_eq!(
        eval(
            "(let ((s (propertize \"ab\" 'a 1))) \
               (list (add-text-properties 0 2 '(a 1) s) (add-text-properties 0 2 '(a 2) s) \
                     (remove-text-properties 0 2 '(zz nil) s) (remove-text-properties 0 1 '(a nil) s) \
                     (set-text-properties 0 2 nil s) (set-text-properties 0 2 nil s) \
                     (set-text-properties 0 2 '(q 1) s) (remove-list-of-text-properties 0 2 '(q) s)))"
        ),
        "(nil t nil t t nil t t)"
    );
}

/// intervals.c `copy_properties`: every copy of propertized text gets its own
/// plist, so `print-circle` finds nothing shared.
#[test]
fn copied_text_does_not_share_plists() {
    assert_eq!(
        eval(
            "(let* ((s (propertize \"ab\" 'a 1)) (print-circle t)) \
               (prin1-to-string (list s (upcase s) (capitalize s) (substring s 0 1) (concat s) \
                 (with-temp-buffer (insert s) \
                   (list (text-properties-at 1) (buffer-string) (buffer-substring 1 2))))))"
        ),
        "\"(#(\\\"ab\\\" 0 2 (a 1)) #(\\\"AB\\\" 0 2 (a 1)) #(\\\"Ab\\\" 0 2 (a 1)) #(\\\"a\\\" 0 1 (a 1)) #(\\\"ab\\\" 0 2 (a 1)) ((a 1) #(\\\"ab\\\" 0 2 (a 1)) #(\\\"a\\\" 0 1 (a 1))))\""
    );
}

/// thingatpt.el and subr.el's symbol/whitespace motion.
#[test]
fn thing_at_point_core() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo-bar (b c) 0xff 12.5\") \
               (list (progn (goto-char 3) (list (thing-at-point 'symbol) (thing-at-point 'word) \
                                                (bounds-of-thing-at-point 'word) (symbol-at-point))) \
                     (progn (goto-char 11) (list (thing-at-point 'sexp) (thing-at-point 'list) (sexp-at-point))) \
                     (progn (goto-char 17) (number-at-point)) \
                     (progn (goto-char 22) (thing-at-point 'number))))"
        ),
        "((\"foo-bar\" \"foo\" (1 . 4) foo-bar) (\"b\" \"(b c)\" b) 255 12.5)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"l1\\nl2\\nl3\") (goto-char 5) \
               (list (thing-at-point 'line) (bounds-of-thing-at-point 'line) \
                     (thing-at-point 'nosuchthing)))"
        ),
        "(\"l2\n\" (4 . 7) nil)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"foo.bar baz\") (goto-char 1) \
               (list (forward-symbol 1) (point) (forward-symbol 2) (point) (forward-symbol -1) (point)))"
        ),
        "(4 4 12 12 nil 9)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"a  b\\nc\") (goto-char 1) \
               (list (forward-whitespace 1) (point) (forward-whitespace 2) (point) \
                     (forward-whitespace -1) (point)))"
        ),
        "(4 4 nil 7 nil 5)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\") (narrow-to-region 2 3) \
               (list (buffer-narrowed-p) (progn (goto-char 2) (search-forward-regexp \"b\")) \
                     (search-backward-regexp \"b\")))"
        ),
        "(t 3 2)"
    );
}
