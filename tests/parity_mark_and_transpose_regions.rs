//! The mark is a marker (`mark-marker`) driven by simple.el's `set-mark`,
//! `push-mark`, `pop-mark` and friends; `region-beginning`/`region-end` are
//! editfns.c's `region_limit`; `transpose-regions` is editfns.c's; and the
//! `zap-*` and `*-dwim` commands are simple.el's and misc.el's.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn buf(body: &str) -> String {
    eval(&format!("(with-temp-buffer (insert \"abcdef\") {body})"))
}

/// `set-mark` activates the mark; the mark is one marker object that moves
/// with the text.
#[test]
fn set_mark_activates_a_marker() {
    assert_eq!(
        buf("(set-mark 2) (list mark-active (mark) (region-beginning) (region-end) (region-active-p) (use-region-p))"),
        "(t 2 2 7 nil nil)"
    );
    assert_eq!(
        buf("(list (mark) mark-active (markerp (mark-marker)) (eq (mark-marker) (mark-marker)))"),
        "(nil nil t t)"
    );
    assert_eq!(
        buf("(set-mark 2) (goto-char 1) (insert \"XX\") (mark)"),
        "4"
    );
    assert_eq!(
        buf("(set-mark 2) (set-mark nil) (list (mark) mark-active)"),
        "(nil nil)"
    );
    assert_eq!(
        buf("(condition-case e (region-beginning) (error e))"),
        "(error \"The mark is not set now, so there is no region\")"
    );
}

/// In Transient Mark mode the region is active only while the mark is, and an
/// inactive mark is an error only once `mark-even-if-inactive` is nil.
#[test]
fn transient_mark_mode_region() {
    assert_eq!(
        buf("(set-mark 2) (let ((transient-mark-mode t)) (list (region-active-p) (use-region-p) (region-noncontiguous-p) (region-bounds)))"),
        "(t t nil ((2 . 7)))"
    );
    assert_eq!(
        buf(
            "(set-mark 2) (let ((transient-mark-mode t)) (deactivate-mark) \
             (list mark-active (region-beginning) \
                   (let ((mark-even-if-inactive nil)) (condition-case e (mark) (error e)))))"
        ),
        "(nil 2 (mark-inactive))"
    );
    assert_eq!(
        buf("(set-mark 3) (goto-char 3) (let ((transient-mark-mode t)) (list (region-active-p) (use-region-p)))"),
        "(t nil)"
    );
    // The region end is clipped to the accessible portion.
    assert_eq!(
        buf("(set-mark 6) (narrow-to-region 3 5) (goto-char 4) (list (region-beginning) (region-end))"),
        "(4 5)"
    );
}

/// `push-mark` rings the old mark, `pop-mark` rotates it back, and
/// `exchange-point-and-mark` swaps them.
#[test]
fn mark_ring_and_exchange() {
    assert_eq!(
        buf("(push-mark 2 t) (push-mark 4 t) (list (mark) (mapcar #'marker-position mark-ring) mark-active)"),
        "(4 (2) t)"
    );
    assert_eq!(
        buf("(push-mark 2 t) (push-mark 4 t) (pop-mark) (list (mark) (mapcar #'marker-position mark-ring))"),
        "(2 (4))"
    );
    assert_eq!(
        buf("(set-mark 2) (exchange-point-and-mark) (list (point) (mark) mark-active)"),
        "(2 7 t)"
    );
    assert_eq!(
        buf("(condition-case e (exchange-point-and-mark) (error e))"),
        "(user-error \"No mark set in this buffer\")"
    );
    assert_eq!(
        buf("(set-mark 1) (save-mark-and-excursion (set-mark 2) (goto-char 3)) (list (mark) (point) mark-active)"),
        "(1 7 t)"
    );
}

/// The two regions trade places with their text properties; point and
/// markers follow their text unless LEAVE-MARKERS, overlays do not.
#[test]
fn transpose_regions() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdefg\") \
             (let ((m (copy-marker 2)) (m2 (copy-marker 6 t)) (o (make-overlay 2 3))) \
             (goto-char 3) (transpose-regions 1 3 5 8) \
             (list (buffer-string) (point) (marker-position m) (marker-position m2) \
                   (overlay-start o) (overlay-end o))))"
        ),
        "(\"efgcdab\" 4 7 2 2 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abcdefg\") (goto-char 3) (transpose-regions 1 3 5 8 t) (list (buffer-string) (point)))"),
        "(\"efgcdab\" 3)"
    );
    assert_eq!(
        buf("(condition-case e (transpose-regions 1 4 3 5) (error e))"),
        "(error \"Transposed regions overlap\")"
    );
    assert_eq!(
        buf("(transpose-regions 1 1 4 6) (buffer-string)"),
        "\"deabcf\""
    );
    assert_eq!(
        buf("(list (transpose-regions 1 3 3 3) (buffer-string))"),
        "(nil \"abcdef\")"
    );
    // The properties of the range the FIRST pair starts and the SECOND pair
    // ends are cleared into one interval before the pieces are grafted back.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdefg\") (put-text-property 1 3 'face 'bold) \
             (put-text-property 3 6 'face 'bold) (transpose-regions 5 8 1 3) (buffer-string))"
        ),
        "#(\"efgcdab\" 0 1 (face bold) 3 4 (face bold) 4 5 (face bold) 5 7 (face bold))"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdefgh\") (put-text-property 3 6 'p 1) \
             (transpose-regions 1 2 7 9) (buffer-string))"
        ),
        "#(\"ghbcdefa\" 3 6 (p 1))"
    );
}

/// `zap-to-char` kills through the ARGth CHAR, `zap-up-to-char` up to it, and
/// the `-dwim` case commands act on the word without an active region.
#[test]
fn zap_and_dwim_commands() {
    let hw = |body: &str| {
        eval(&format!(
            "(with-temp-buffer (insert \"hello world\") (goto-char 1) {body})"
        ))
    };
    assert_eq!(
        hw("(zap-to-char 1 ?o) (list (buffer-string) (car kill-ring))"),
        "(\" world\" \"hello\")"
    );
    assert_eq!(hw("(zap-to-char 2 ?o) (buffer-string)"), "\"rld\"");
    assert_eq!(
        hw("(zap-up-to-char 1 ?o) (list (buffer-string) (point))"),
        "(\"o world\" 1)"
    );
    assert_eq!(
        hw("(condition-case e (zap-up-to-char 1 ?z) (error (list e (point))))"),
        "((search-failed \"z\") 1)"
    );
    assert_eq!(hw("(capitalize-dwim 2) (buffer-string)"), "\"Hello World\"");
    assert_eq!(hw("(mark-word) (list (point) (mark))"), "(1 6)");
    assert_eq!(hw("(mark-sexp) (list (point) (mark))"), "(1 6)");
    assert_eq!(hw("(kill-sexp 1) (buffer-string)"), "\" world\"");
    let abc = |body: &str| eval(&format!("(with-temp-buffer (insert \"a\\nb\\nc\") {body})"));
    assert_eq!(
        abc("(goto-char 1) (kill-whole-line) (buffer-string)"),
        "\"b\nc\""
    );
    assert_eq!(
        abc("(goto-char 3) (kill-whole-line -1) (buffer-string)"),
        "\"a\nc\""
    );
    assert_eq!(
        abc("(goto-char 3) (kill-whole-line 0) (buffer-string)"),
        "\"a\n\nc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"HELLO WORLD\") (goto-char 1) (set-mark 9) (let ((transient-mark-mode t)) (downcase-dwim 1)) (buffer-string))"),
        "\"hello woRLD\""
    );
}
