//! `(head X)` dispatch, `cl-defgeneric`'s value, `pcase-lambda`, a compound
//! `cl-type`, and `string-pixel-width`.
//!
//! Four gaps that look unrelated and share one shape: each was a construct
//! whose argument was treated as the wrong KIND of thing.
//!
//! ```text
//!                                                     emacs      elisprs (before)
//! (cl-defmethod g ((x (head foo))) …) then (g '(foo 1))  headfoo  (void-variable foo)
//! (cl-defgeneric gz (x))                                 nil      gz
//! (funcall (pcase-lambda (`(,a ,b)) (+ a b)) '(1 2))     3        (invalid-function …)
//! (pcase 3 ((cl-type (integer 0 5)) 'in) (_ 'out))       in       (wrong-type-argument symbolp …)
//! (string-pixel-width "ab\tc")                           9        (void-function …)
//! ```
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1
//! (with `cl-lib` loaded, which is what registers the `cl-type` pattern there).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `(head SYMBOL)` names the symbol the argument's car must be `eq` to; it is
/// not evaluated. Where the name happened to be bound, evaluating it dispatched
/// on THAT value instead — a wrong answer rather than an error.
#[test]
fn the_head_specializer_does_not_evaluate_its_symbol() {
    assert_eq!(
        eval("(progn (cl-defgeneric gh (x)) \
              (cl-defmethod gh ((x (head foo))) 'headfoo) (gh '(foo 1)))"),
        "headfoo"
    );
    assert_eq!(
        eval("(let ((foo 'bar)) (progn (cl-defgeneric gq (x)) \
              (cl-defmethod gq ((x (head foo))) 'hf) (gq '(foo 1))))"),
        "hf"
    );
    // `head` is more specific than the type it also matches.
    assert_eq!(
        eval("(progn (cl-defgeneric gh2 (x)) \
              (cl-defmethod gh2 ((x (head foo))) 'hf) \
              (cl-defmethod gh2 ((x list)) 'lst) \
              (list (gh2 '(foo 1)) (gh2 '(bar 1))))"),
        "(hf lst)"
    );
}

/// `cl-defgeneric` answers nil; the trailing `defun` used to leave the symbol
/// as the expansion's value. A body still becomes the unspecialized default.
#[test]
fn cl_defgeneric_answers_nil_and_still_takes_a_default_body() {
    assert_eq!(eval("(cl-defgeneric gz (x))"), "nil");
    assert_eq!(eval("(progn (cl-defgeneric gy (x) 5) (gy 1))"), "5");
}

/// A `pcase-lambda` parameter may be a pattern. This reader expands backquote
/// eagerly, so `` `(,a ,b) `` arrives as the `(cons a (cons b nil))` pattern —
/// which is why the old error reported that cons as the function.
#[test]
fn pcase_lambda_destructures_its_parameters() {
    assert_eq!(eval("(funcall (pcase-lambda (`(,a ,b)) (+ a b)) '(1 2))"), "3");
    assert_eq!(
        eval("(funcall (pcase-lambda (`(,a . ,b)) (list a b)) '(1 . 2))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(funcall (pcase-lambda (x `(,a ,b)) (list x a b)) 9 '(1 2))"),
        "(9 1 2)"
    );
    assert_eq!(
        eval("(mapcar (pcase-lambda (`(,a ,b)) (+ a b)) '((1 2) (3 4)))"),
        "(3 7)"
    );
    // Plain parameters and the lambda-list keywords are untouched.
    assert_eq!(eval("(funcall (pcase-lambda (x y) (+ x y)) 1 2)"), "3");
    assert_eq!(
        eval("(funcall (pcase-lambda (`(,a ,b) &optional c) (list a b c)) '(1 2))"),
        "(1 2 nil)"
    );
}

/// `pcase`'s `cl-type` names a predicate by mangling the type's symbol name,
/// which a COMPOUND specifier has none of. It routes through `cl-typep`.
#[test]
fn pcase_cl_type_takes_a_compound_specifier() {
    assert_eq!(eval("(pcase 3 ((cl-type (integer 0 5)) 'in) (_ 'out))"), "in");
    assert_eq!(eval("(pcase 9 ((cl-type (integer 0 5)) 'in) (_ 'out))"), "out");
    assert_eq!(eval("(pcase 3.5 ((cl-type (float 0 5)) 'in) (_ 'out))"), "in");
    // The plain symbol spellings still work.
    assert_eq!(eval("(pcase 3 ((cl-type integer) 'i) (_ 'o))"), "i");
    assert_eq!(eval("(pcase \"x\" ((cl-type string) 's) (_ 'o))"), "s");
}

/// `string-pixel-width` is NOT `string-width`: in batch a character is one
/// pixel per display column, so a TAB advances to the next multiple of
/// `tab-width` rather than counting a flat `tab-width` columns, and
/// `window-text-pixel-size` reports the widest line.
#[test]
fn string_pixel_width_measures_display_columns_with_tab_stops() {
    assert_eq!(eval("(string-pixel-width \"abc\")"), "3");
    assert_eq!(eval("(string-pixel-width \"\")"), "0");
    assert_eq!(eval("(string-pixel-width \"ab\\tc\")"), "9");
    // …where `string-width` counts the tab as a flat `tab-width`.
    assert_eq!(eval("(string-width \"ab\\tc\")"), "11");
    assert_eq!(eval("(string-pixel-width \"\\t\")"), "8");
    assert_eq!(eval("(string-pixel-width \"\\t\\t\")"), "16");
    assert_eq!(eval("(string-pixel-width \"abcdefgh\\tx\")"), "17");
    assert_eq!(eval("(let ((tab-width 4)) (string-pixel-width \"ab\\tc\"))"), "5");
    // A newline starts a new line; the widest one wins.
    assert_eq!(eval("(string-pixel-width \"a\\nb\")"), "1");
    // Double-width characters count 2; text properties count 0.
    assert_eq!(eval("(string-pixel-width \"\u{65e5}\u{672c}\")"), "4");
    assert_eq!(eval("(string-pixel-width (propertize \"abc\" 'p 1))"), "3");
}
