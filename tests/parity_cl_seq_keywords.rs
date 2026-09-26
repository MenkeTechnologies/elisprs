//! cl-seq.el's keyword contract and the cl-lib surface that delegates to it.
//!
//! Expectations are GNU Emacs's (`emacs -Q --batch` with `(require 'cl-lib)`),
//! taken from the cl-seq.el / cl-extra.el / cl-lib.el sources, which are
//! identical on these definitions across the emacs-30 and emacs-31 branches.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// `cl--parsing-keywords` binds `cl-if` for every function that lists `:if`,
/// and `cl--check-test-nokey` consults it whenever `cl-test` is nil — so the
/// keyword entry points take a predicate too, and the ITEM is then ignored.
/// `:test-not` / `:if-not` move their function into `cl-test` / `cl-if`, so the
/// negated spelling wins when both are passed.
#[test]
fn if_and_if_not_keywords_reach_the_keyword_entry_points() {
    assert_eq!(eval("(cl-member 1 '(1 2 3) :if #'cl-evenp)"), "(2 3)");
    assert_eq!(eval("(cl-member 1 '(1 2 3) :if-not #'cl-oddp)"), "(2 3)");
    assert_eq!(
        eval("(cl-assoc 9 '((1 . a) (2 . b)) :if #'cl-evenp)"),
        "(2 . b)"
    );
    assert_eq!(
        eval("(cl-rassoc 9 '((a . 1) (b . 2)) :if-not #'cl-oddp)"),
        "(b . 2)"
    );
    assert_eq!(eval("(cl-position 9 '(1 3 4 5) :if #'cl-evenp)"), "2");
    assert_eq!(
        eval("(cl-position 9 [1 3 4 6] :if #'cl-evenp :from-end t)"),
        "3"
    );
    assert_eq!(eval("(cl-count 9 '(1 2 4 5) :if #'cl-evenp)"), "2");
    assert_eq!(eval("(cl-find 9 '(1 3 4 5) :if #'cl-evenp)"), "4");
    assert_eq!(eval("(cl-remove 9 '(1 2 3 4) :if #'cl-evenp)"), "(1 3)");
    assert_eq!(
        eval("(cl-remove 9 '(1 2 3 4) :if-not #'cl-evenp :count 1)"),
        "(2 3 4)"
    );
    assert_eq!(
        eval("(cl-substitute 'x 9 '(1 2 3 4) :if #'cl-oddp)"),
        "(x 2 x 4)"
    );
    // Precedence: the negated keyword overrides its positive twin, and :test
    // beats :if.
    assert_eq!(
        eval("(cl-member 2 '(1 2 3) :test #'< :test-not #'<)"),
        "(1 2 3)"
    );
    assert_eq!(
        eval("(cl-member 1 '(1 2) :if #'cl-evenp :if-not #'cl-evenp)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-member 1 '(1 2 3) :test #'eql :if #'cl-evenp)"),
        "(1 2 3)"
    );
    // The keyword-free path is still `memql`.
    assert_eq!(
        err("(cl-member 1 '(2 . 3))"),
        "(wrong-type-argument listp (2 . 3))"
    );
}
