//! text-property-search.el parity — faithful port of
//! emacs-lisp/text-property-search.el (Emacs 30.2).
//!
//! `text-property-search-forward`/`-backward` walk the current buffer for
//! regions matching a text property, returning a `prop-match` struct. Every
//! expectation matches GNU Emacs 30.2 (`emacs -Q --batch`).
//!
//! Fixture buffer (built by the `buf` macro): "AAA" + "BBB"(face bold) + "CCC"
//! + "DDD"(face italic), so face=bold spans [4,7) and face=italic spans [10,13)
//!   in Emacs's 1-based buffer coordinates.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// A `buf` macro wrapping a `with-temp-buffer` fixture, prepended to each case.
const FIXTURE: &str = r#"(defmacro buf (&rest body)
  `(with-temp-buffer
     (insert "AAA")
     (insert (propertize "BBB" 'face 'bold))
     (insert "CCC")
     (insert (propertize "DDD" 'face 'italic))
     ,@body)) "#;

fn eval_buf(body: &str) -> String {
    eval(&format!("{FIXTURE}(buf {body})"))
}

/// The public API and the `prop-match` accessors are bound.
#[test]
fn tps_api_is_bound() {
    assert_eq!(
        eval("(mapcar #'fboundp '(text-property-search-forward text-property-search-backward prop-match-beginning prop-match-end prop-match-value make-prop-match prop-match-p))"),
        "(t t t t t t t)"
    );
}

/// Forward search with the default nil predicate yields each distinct non-nil
/// region of the property, newest region last, moving point to each region end.
#[test]
fn tps_forward_distinct_regions() {
    assert_eq!(
        eval_buf(
            "(goto-char (point-min)) \
             (let (r m) \
               (while (setq m (text-property-search-forward 'face)) \
                 (push (list (prop-match-beginning m) (prop-match-end m) (prop-match-value m)) r)) \
               (nreverse r))"
        ),
        "((4 7 bold) (10 13 italic))"
    );
}

/// Backward search from the buffer end returns the same regions in reverse.
#[test]
fn tps_backward_distinct_regions() {
    assert_eq!(
        eval_buf(
            "(goto-char (point-max)) \
             (let (r m) \
               (while (setq m (text-property-search-backward 'face)) \
                 (push (list (prop-match-beginning m) (prop-match-end m) (prop-match-value m)) r)) \
               (nreverse r))"
        ),
        "((10 13 italic) (4 7 bold))"
    );
}

/// PREDICATE t requires an `equal` match: searching for the exact `bold` value
/// lands only on the bold region.
#[test]
fn tps_forward_value_equal_predicate() {
    assert_eq!(
        eval_buf(
            "(goto-char (point-min)) \
             (let ((m (text-property-search-forward 'face 'bold t))) \
               (list (prop-match-beginning m) (prop-match-end m) (prop-match-value m)))"
        ),
        "(4 7 bold)"
    );
}

/// No match: returns nil and leaves point where it started (point-min = 1).
#[test]
fn tps_forward_no_match_keeps_point() {
    assert_eq!(
        eval_buf(
            "(goto-char (point-min)) \
             (list (text-property-search-forward 'face 'nosuch t) (point))"
        ),
        "(nil 1)"
    );
}

/// The returned object satisfies `prop-match-p`.
#[test]
fn tps_returns_prop_match() {
    assert_eq!(
        eval_buf("(goto-char (point-min)) (prop-match-p (text-property-search-forward 'face))"),
        "t"
    );
}

/// A custom function predicate is called with (VALUE PROP-VALUE); returning a
/// match on the italic region only.
#[test]
fn tps_function_predicate() {
    assert_eq!(
        eval_buf(
            "(goto-char (point-min)) \
             (let ((m (text-property-search-forward 'face nil \
                        (lambda (_v pv) (eq pv 'italic))))) \
               (list (prop-match-beginning m) (prop-match-end m) (prop-match-value m)))"
        ),
        "(10 13 italic)"
    );
}
