//! `cl-defstruct` constructors are cl-macs.el's: a `cl-defun` whose lambda list
//! carries `&cl-defs (nil . DESCS)`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`, lexical
//! binding, each form wrapped in `(condition-case e FORM (error (list 'ERR e)))`).
//! A keyword constructor is called through `funcall` in the oracle where it
//! matters: Emacs inlines `cl-defsubst` constructors during eager expansion,
//! and elisprs has no `cl-defsubst`, so the two agree on the called function,
//! not on the inlined form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// An unknown keyword, a keyword without a value, and a non-keyword argument
/// all signal; a repeated keyword takes the first value, and
/// `:allow-other-keys t` in the call admits unknown keywords.
#[test]
fn keyword_constructor_checks_its_keywords() {
    let def = "(cl-defstruct q2 a)";
    assert_eq!(
        err(&format!("(progn {def} (make-q2 :z 1))")),
        "(error \"Keyword argument :z not one of (:a)\")"
    );
    assert_eq!(
        err(&format!("(progn {def} (make-q2 :a))")),
        "(error \"Missing argument for :a\")"
    );
    assert_eq!(
        err(&format!("(progn {def} (make-q2 :a 1 :z))")),
        "(error \"Keyword argument :z not one of (:a)\")"
    );
    assert_eq!(
        err(&format!("(progn {def} (make-q2 1))")),
        "(error \"Keyword argument 1 not one of (:a)\")"
    );
    assert_eq!(
        err(&format!("(progn {def} (make-q2 nil))")),
        "(error \"Keyword argument nil not one of (:a)\")"
    );
    assert_eq!(
        eval(&format!("(progn {def} (make-q2 :a 1 :a 2))")),
        "#s(q2 1)"
    );
    assert_eq!(
        eval(&format!(
            "(progn {def} (make-q2 :a 1 :allow-other-keys t :zz 3))"
        )),
        "#s(q2 1)"
    );
    // `&allow-other-keys` in the constructor's own lambda list.
    assert_eq!(
        eval("(progn (cl-defstruct (ok (:constructor make-ok (&key a &allow-other-keys))) a) (make-ok :zz 1 :a 2))"),
        "#s(ok 2)"
    );
}

/// A slot's initform is evaluated only when its argument is not supplied, in
/// slot order, so a later initform reads an earlier slot's value.
#[test]
fn slot_initforms_are_argument_defaults() {
    let def = "(cl-defstruct r (w 1) (h (* w 2)))";
    assert_eq!(eval(&format!("(progn {def} (make-r :w 5))")), "#s(r 5 10)");
    assert_eq!(eval(&format!("(progn {def} (make-r))")), "#s(r 1 2)");
    assert_eq!(eval(&format!("(progn {def} (make-r :h 3))")), "#s(r 1 3)");
    // A side-effecting initform does not run when the slot is supplied.
    let counted = "(progn (defvar cnt 0) (cl-defstruct s (a (setq cnt (1+ cnt)))) ";
    assert_eq!(eval(&format!("{counted} (make-s :a 1) cnt)")), "0");
    assert_eq!(eval(&format!("{counted} (make-s) cnt)")), "1");
}

/// A BOA constructor's lambda list is a full cl lambda list: `&optional (b 7)`
/// has its default, `&rest` collects, `&key` is parsed out of the rest, and
/// `&aux` binds from the arguments already bound.  A slot that is not an
/// argument is filled from its initform.
#[test]
fn boa_constructor_lambda_list() {
    let def = "(cl-defstruct (boa (:constructor new-boa (a &optional (b 7) &rest more &key k &aux (z (list a b)))) (:constructor mk-boa (a b))) a (b 3) more k z)";
    assert_eq!(
        eval(&format!("(progn {def} (new-boa 1))")),
        "#s(boa 1 7 nil nil (1 7))"
    );
    assert_eq!(
        eval(&format!("(progn {def} (new-boa 1 2 :k 5))")),
        "#s(boa 1 2 (:k 5) 5 (1 2))"
    );
    assert_eq!(
        err(&format!("(progn {def} (new-boa 1 2 3 4 :k 5))")),
        "(error \"Keyword argument 3 not one of (:k)\")"
    );
    // An unlisted slot takes its initform, not nil.
    assert_eq!(
        eval(&format!("(progn {def} (mk-boa 1 2))")),
        "#s(boa 1 2 nil nil nil)"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (v6 (:constructor nv6 (a))) (x 9) a) (nv6 5))"),
        "#s(v6 9 5)"
    );
    // An `&optional` argument named like a slot defaults to the slot's initform.
    assert_eq!(
        eval("(progn (cl-defstruct (sd (:constructor mk-sd (name &optional initform))) name (initform 'none)) (mk-sd 'n))"),
        "#s(sd n none)"
    );
    assert_eq!(
        eval(&format!("(progn {def} (func-arity 'new-boa))")),
        "(1 . many)"
    );
    assert_eq!(
        eval(&format!("(progn {def} (func-arity 'mk-boa))")),
        "(2 . 2)"
    );
}

/// The arity error of a BOA constructor that `cl--do-arglist` parses names the
/// constructor; one a plain `lambda` parses names the closure.
#[test]
fn boa_constructor_arity_errors() {
    assert_eq!(
        err("(progn (cl-defstruct (boa (:constructor new-boa (a &optional (b 7)))) a (b 3)) (new-boa 1 2 3))"),
        "(wrong-number-of-arguments new-boa 3)"
    );
    assert_eq!(
        err("(progn (cl-defstruct (p2 (:constructor make-p2 (a))) a) (make-p2 :a 1))"),
        "(wrong-number-of-arguments #[(a) ((record 'p2 a)) (t) nil \"Constructor for objects of type `p2'.\"] 2)"
    );
}

/// `:constructor` options read in order: a BOA constructor leaves the default
/// `make-NAME` in place unless it takes its name; `(:constructor NAME)` renames
/// the keyword constructor and `(:constructor nil)` drops whatever name it has.
#[test]
fn constructor_options_are_read_in_order() {
    assert_eq!(
        eval("(progn (cl-defstruct (boa (:constructor new-boa (a))) a) (fboundp 'make-boa))"),
        "t"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (p2 (:constructor make-p2 (a))) a) (make-p2 1))"),
        "#s(p2 1)"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (p3 (:constructor nil) (:constructor new-p3 (&key a))) a) (list (fboundp 'make-p3) (new-p3 :a 1)))"),
        "(nil #s(p3 1))"
    );
    assert_eq!(
        eval(
            "(progn (cl-defstruct (p4 (:constructor mk4)) a) (list (fboundp 'make-p4) (mk4 :a 1)))"
        ),
        "(nil #s(p4 1))"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (p5 (:constructor mk5) (:constructor nil)) a) (list (fboundp 'make-p5) (fboundp 'mk5)))"),
        "(nil nil)"
    );
}

/// A constructor's docstring is the option's, or cl-macs.el's default, with
/// the usage line `cl--transform-lambda` appends.
#[test]
fn constructor_docstrings() {
    assert_eq!(
        eval("(progn (cl-defstruct (p (:constructor new-p (a b) \"Make a p.\")) a b) (documentation 'new-p))"),
        "\"Make a p.\""
    );
    assert_eq!(
        eval("(progn (cl-defstruct q2 a) (documentation 'make-q2))"),
        "\"Constructor for objects of type ‘q2’.\n\n(fn &key A)\""
    );
}

/// `:include`d slots and `:type list` / `:type vector` structs go through the
/// same constructor, keyword checks included.
#[test]
fn included_and_typed_structs() {
    let def = "(progn (cl-defstruct boa a (b 3)) (cl-defstruct (inc (:include boa)) (c 11))";
    assert_eq!(eval(&format!("{def} (make-inc))")), "#s(inc nil 3 11)");
    assert_eq!(
        eval(&format!("{def} (make-inc :c 1 :a 2))")),
        "#s(inc 2 3 1)"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (lt (:type list) :named) a (b 9)) (make-lt :b 1))"),
        "(lt nil 1)"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (lu (:type list)) a (b 9)) (make-lu :b 1))"),
        "(nil 1)"
    );
    assert_eq!(
        err("(progn (cl-defstruct (lu (:type list)) a (b 9)) (make-lu :z 1))"),
        "(error \"Keyword argument :z not one of (:a :b)\")"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (lv (:type vector)) a (b 9)) (make-lv))"),
        "[nil 9]"
    );
    assert_eq!(
        err("(progn (cl-defstruct (lv (:type vector)) a (b 9)) (make-lv :b 2 :c 3))"),
        "(error \"Keyword argument :c not one of (:a :b)\")"
    );
}
