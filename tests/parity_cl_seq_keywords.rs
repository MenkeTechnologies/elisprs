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

/// subr.el defines all 28 two- to four-letter c[ad]+r compositions, cl-lib.el
/// aliases the 24 three- and four-letter ones under `cl-`, and every one is a
/// place (the compiler macro rewrites it to its car/cdr chain; gv follows the
/// alias). A wrong argument count is exec_byte_code's `(1 . 1)`.
#[test]
fn every_cxr_composition_exists_is_a_place_and_reports_byte_code_arity() {
    assert_eq!(eval("(caaaar '((((1)))))"), "1");
    assert_eq!(eval("(cdddar '((1 2 3 4)))"), "(4)");
    assert_eq!(eval("(cadadr '(1 (2 3)))"), "3");
    assert_eq!(eval("(cl-caddar '((1 2 3)))"), "3");
    assert_eq!(err("(caaddr '(1 2 3))"), "(wrong-type-argument listp 3)");
    assert_eq!(err("(cadr)"), "(wrong-number-of-arguments (1 . 1) 0)");
    assert_eq!(err("(cdaddr)"), "(wrong-number-of-arguments (1 . 1) 0)");
    assert_eq!(
        err("(cl-caddr 1 2)"),
        "(wrong-number-of-arguments (1 . 1) 2)"
    );
    assert_eq!(eval("(symbol-function 'cl-cadadr)"), "cadadr");
    assert_eq!(
        eval("(let ((x (list (list (list (list 1)))))) (setf (caaaar x) 9) x)"),
        "((((9))))"
    );
    assert_eq!(
        eval("(let ((x (list 1 2 3 4 5))) (setf (cl-caddr x) 9) (setf (cadddr x) 8) x)"),
        "(1 2 9 8 5)"
    );
    assert_eq!(
        eval("(let ((x (list 1 2 3 4 5))) (setf (cddddr x) '(z)) (setf (cl-cdddr x) '(y)) x)"),
        "(1 2 3 y)"
    );
    assert_eq!(
        eval("(let ((x (list 1 (list 2 3)))) (cl-incf (cadadr x) 10) x)"),
        "(1 (2 13))"
    );
}

/// cl-seq.el's destructive set operations delegate to their copying forms after
/// the same empty-list shortcuts, and `cl-tree-equal` compares leaves through
/// `cl--check-match` with `cl--parsing-keywords`' keyword check.
#[test]
fn destructive_set_operations_and_tree_equal() {
    assert_eq!(eval("(cl-nunion (list 1 2 3) (list 3 4))"), "(4 1 2 3)");
    assert_eq!(eval("(cl-nunion nil (list 3 4))"), "(3 4)");
    assert_eq!(
        eval("(cl-nintersection (list 1 2 3) (list 3 2 9))"),
        "(2 3)"
    );
    assert_eq!(
        eval("(cl-nintersection (list \"a\" \"b\") (list \"b\") :test #'equal)"),
        "(\"b\")"
    );
    assert_eq!(eval("(cl-nset-difference (list 1 2 3) (list 2))"), "(1 3)");
    assert_eq!(
        eval("(cl-nsubst 'x 2 (list 1 2 (list 2 3)))"),
        "(1 x (x 3))"
    );
    assert_eq!(
        err("(cl-nsubst 'x 2 (list 1 2 (list 2 3)) :test #'<)"),
        "(wrong-type-argument number-or-marker-p (1 2 (2 3)))"
    );
    assert_eq!(eval("(cl-tree-equal '(1 (2 3)) '(1 (2 3)))"), "t");
    assert_eq!(eval("(cl-tree-equal '(1 (2 \"a\")) '(1 (2 \"a\")))"), "nil");
    assert_eq!(
        eval("(cl-tree-equal '(1 (2 \"a\")) '(1 (2 \"a\")) :test #'equal)"),
        "t"
    );
    assert_eq!(eval("(cl-tree-equal '(1 2 . 3) '(1 2 . 3))"), "t");
    assert_eq!(eval("(cl-tree-equal '(1 2) '(1 2 3))"), "nil");
    assert_eq!(eval("(cl-tree-equal '(1 2) '(1 2) :test-not #'eql)"), "nil");
    assert_eq!(
        err("(cl-tree-equal 1 1 :foo 2)"),
        "(error \"Bad keyword argument :foo\")"
    );
}

/// cl-extra.el's mapping family: `cl-mapc`/`cl-mapl` return their first
/// sequence, several lists step together and stop at the shortest, and
/// `cl-mapcon` splices `cl-maplist`'s results.
#[test]
fn cl_mapping_family() {
    assert_eq!(
        eval(
            "(let (acc) (list (cl-mapc (lambda (x y) (push (+ x y) acc)) '(1 2 3) '(10 20)) acc))"
        ),
        "((1 2 3) (22 11))"
    );
    assert_eq!(
        eval("(let (acc) (list (cl-mapc (lambda (x y z) (push (list x y z) acc)) [1 2] '(3 4) \"ab\") acc))"),
        "([1 2] ((2 4 98) (1 3 97)))"
    );
    assert_eq!(
        eval(
            "(let (acc) (list (cl-mapl (lambda (x y) (push (list x y) acc)) '(1 2 3) '(a b)) acc))"
        ),
        "((1 2 3) (((2 3) (b)) ((1 2 3) (a b))))"
    );
    assert_eq!(
        eval("(cl-maplist #'append '(1 2 3) '(a b))"),
        "((1 2 3 a b) (2 3 b))"
    );
    assert_eq!(
        eval("(cl-mapcon (lambda (x y) (list (car x) (car y))) '(1 2 3) '(a b))"),
        "(1 a 2 b)"
    );
}

/// cl-lib.el aliases, `cl-get` with its `put` place, the multiple-value shims,
/// and seq.el's `seq-copy` / `seq-random-elt`.
#[test]
fn cl_aliases_and_seq_copy() {
    assert_eq!(eval("(symbol-function 'cl-copy-seq)"), "copy-sequence");
    assert_eq!(eval("(symbol-function 'cl-svref)"), "aref");
    assert_eq!(eval("(cl-svref [5 6] 1)"), "6");
    assert_eq!(eval("(let ((l (list 1 2))) (eq l (seq-copy l)))"), "nil");
    assert_eq!(eval("(seq-copy \"ab\")"), "\"ab\"");
    assert_eq!(
        eval("(and (memq (seq-random-elt '(7 8 9)) '(7 8 9)) t)"),
        "t"
    );
    assert_eq!(
        err("(seq-random-elt nil)"),
        "(error \"Sequence cannot be empty\")"
    );
    assert_eq!(
        eval("(progn (put 'zz 'p 4) (list (cl-get 'zz 'p) (cl-get 'zz 'q 5)))"),
        "(4 5)"
    );
    assert_eq!(
        eval("(let ((s (make-symbol \"s\"))) (setf (cl-get s 'k) 7) (get s 'k))"),
        "7"
    );
    assert_eq!(eval("(cl-multiple-value-apply #'+ '(1 2))"), "3");
    assert_eq!(eval("(cl-multiple-value-call #'+ 1 '(2 3))"), "6");
}
