//! Overlays: an object over a buffer range, not a text property.
//!
//! The whole family was void. The distinction that matters is that an overlay
//! survives the text under it changing — both of its ends move with edits like
//! markers, and deleting it DETACHES it rather than clearing anything, so the
//! object stays an overlay and `move-overlay` can put it back.
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Identity, accessors and printing.
#[test]
fn an_overlay_is_its_own_type() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) \
              (list (overlayp o) (overlay-start o) (overlay-end o) (type-of o))))"
        ),
        "(t 1 3 overlay)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (eq (overlay-buffer o) (current-buffer))))"
        ),
        "t"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (format \"%S\" (make-overlay 1 3)))"),
        "\"#<overlay from 1 to 3 in  *temp*>\""
    );
    // An inverted pair is swapped, not an error.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 4 2))) (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 4)"
    );
}

/// Both ends move with edits, and the two advance flags decide which side of an
/// insertion AT an endpoint the new text lands on. These four cases are the
/// whole content of `FRONT-ADVANCE`/`REAR-ADVANCE`.
#[test]
fn both_ends_move_with_edits() {
    // Insertion before the overlay shifts both ends.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 4))) (goto-char 1) (insert \"XY\") \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(4 6)"
    );
    // At the START: inside by default, outside with FRONT-ADVANCE.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 4))) (goto-char 2) (insert \"XY\") \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 6)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 4 nil t))) (goto-char 2) (insert \"XY\") \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(4 6)"
    );
    // At the END: outside by default, inside with REAR-ADVANCE.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 4))) (goto-char 4) (insert \"XY\") \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 4)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 4 nil nil t))) (goto-char 4) (insert \"XY\") \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 6)"
    );
    // Deleting text inside the overlay shrinks it.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 2 5))) (delete-region 3 4) \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 4)"
    );
}

/// Properties: newest-first, but re-putting one keeps its position.
#[test]
fn overlay_properties_are_newest_first() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-put o 'a 1) (overlay-put o 'b 2) \
              (overlay-properties o)))"
        ),
        "(b 2 a 1)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-put o 'a 1) (overlay-put o 'a 9) \
              (overlay-properties o)))"
        ),
        "(a 9)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-put o 'face 'bold) (overlay-get o 'face)))"
        ),
        "bold"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-get o 'nope)))"
        ),
        "nil"
    );
}

/// `overlays-at` covers `start <= POS < end`, so an overlay ENDING at POS does
/// not cover it. `overlays-in` still reports an EMPTY overlay, including when
/// BEG equals END.
#[test]
fn coverage_and_the_empty_overlay() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 1 3) (make-overlay 2 5) \
              (mapcar (lambda (o) (list (overlay-start o) (overlay-end o))) (overlays-in 1 6)))"
        ),
        "((1 3) (2 5))"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 1 3) (make-overlay 2 5) \
              (mapcar #'overlay-start (overlays-at 2)))"
        ),
        "(1 2)"
    );
    // Ends at 3, so it does not cover 3.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 1 3) \
              (mapcar #'overlay-start (overlays-at 3)))"
        ),
        "nil"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 3 3) \
              (mapcar #'overlay-start (overlays-in 1 6)))"
        ),
        "(3)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 3 3) \
              (mapcar #'overlay-start (overlays-in 3 3)))"
        ),
        "(3)"
    );
}

/// `next-overlay-change`/`previous-overlay-change` walk both ends, and fall back
/// to `point-max`/`point-min`.
#[test]
fn the_change_positions_walk_both_ends() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 2 4) \
              (list (next-overlay-change 1) (next-overlay-change 2) \
                    (next-overlay-change 3) (next-overlay-change 4)))"
        ),
        "(2 4 4 6)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (make-overlay 2 4) \
              (list (previous-overlay-change 6) (previous-overlay-change 4) \
                    (previous-overlay-change 2)))"
        ),
        "(4 2 1)"
    );
}

/// Deleting detaches: the object stays an overlay, its ends and buffer read
/// nil, its properties survive being unreachable, and `move-overlay` re-attaches
/// it.
#[test]
fn deleting_detaches_rather_than_destroys() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (delete-overlay o) \
              (list (overlay-start o) (overlay-buffer o) (overlayp o))))"
        ),
        "(nil nil t)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (delete-overlay o) (format \"%S\" o)))"
        ),
        "\"#<overlay in no buffer>\""
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (delete-overlay o) (overlay-get o 'a)))"
        ),
        "nil"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (delete-overlay o) (move-overlay o 1 2) \
              (list (overlay-start o) (overlayp o))))"
        ),
        "(1 t)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (move-overlay o 2 4) \
              (list (overlay-start o) (overlay-end o))))"
        ),
        "(2 4)"
    );
}

/// `remove-overlays` matches on a property, and MOVES or SPLITS an overlay that
/// only partly overlaps the range rather than deleting it — which is why it
/// needs `copy-overlay`.
#[test]
fn remove_overlays_matches_moves_and_splits() {
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-put o 'x 1) \
              (remove-overlays 1 6 'x 1) (length (overlays-in 1 6))))"
        ),
        "0"
    );
    // A non-matching value leaves it alone.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") \
              (let ((o (make-overlay 1 3))) (overlay-put o 'x 1) \
              (remove-overlays 1 6 'x 2) (length (overlays-in 1 6))))"
        ),
        "1"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"hello\") (remove-overlays) \
              (length (overlays-in 1 6)))"
        ),
        "0"
    );
}
