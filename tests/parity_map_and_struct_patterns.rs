//! pcase's `map` and `cl-struct` patterns, and the `map-let` built on the first.
//!
//! elisprs's `pcase` is a hand-written pattern compiler, and both of these
//! patterns were simply absent — the compiler signalled
//! `(error "pcase: unsupported pattern (map a)")` rather than matching. That
//! took `map-let` down with it (map.el:73 is a `pcase-let` over the `map`
//! pattern) and left `mapp` undefined.
//!
//! Every expectation is `emacs -Q --batch` on GNU Emacs 30.2 with `map` and
//! `cl-lib` required, for the same form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `mapp` is the predicate the `map` pattern gates on: a list (nil included),
/// an array, or a hash table.
#[test]
fn mapp_accepts_lists_arrays_and_hash_tables() {
    assert_eq!(eval("(mapp '((a . 1)))"), "t");
    assert_eq!(eval("(mapp nil)"), "t");
    assert_eq!(eval("(mapp [1 2])"), "t");
    assert_eq!(eval("(mapp \"abc\")"), "t");
    assert_eq!(eval("(mapp (make-hash-table))"), "t");
    assert_eq!(eval("(mapp 5)"), "nil");
    assert_eq!(eval("(mapp 1.5)"), "nil");
}

/// A bare symbol key binds itself; a keyword `:k` binds `k`.
#[test]
fn map_pattern_binds_symbol_and_keyword_keys() {
    assert_eq!(
        eval("(pcase '((a . 1) (b . 2)) ((map a b) (list a b)))"),
        "(1 2)"
    );
    assert_eq!(eval("(pcase '((:x . 5)) ((map :x) x))"), "5");
    assert_eq!(eval("(pcase-let (((map a) '((a . 7)))) a)"), "7");
}

/// In the `(KEY VAR [DEFAULT])` form both KEY and DEFAULT are *evaluated*
/// forms, not quoted names — which is how a runtime key reaches the pattern.
#[test]
fn map_pattern_evaluates_a_compound_keys_form() {
    assert_eq!(
        eval("(pcase (list (cons \"k\" 1)) ((map (\"k\" v)) v))"),
        "1"
    );
    assert_eq!(
        eval("(let ((k 'a)) (pcase '((a . 1)) ((map (k v 99)) v)))"),
        "1"
    );
    assert_eq!(
        eval("(let ((k 'z)) (pcase '((a . 1)) ((map (k v 99)) v)))"),
        "99"
    );
}

/// The pattern fails on a non-map instead of erroring, and `nil` IS a map — an
/// empty one, so every key comes back nil.
#[test]
fn map_pattern_gates_on_mapp() {
    assert_eq!(eval("(pcase 5 ((map a) 'yes) (_ 'no))"), "no");
    assert_eq!(
        eval("(pcase nil ((map a) (list 'yes a)) (_ 'no))"),
        "(yes nil)"
    );
}

/// `map-let` is that pattern with a `pcase-let` around it, over any map kind.
#[test]
fn map_let_binds_over_every_map_kind() {
    assert_eq!(
        eval("(map-let (a b) '((a . 1) (b . 2)) (list a b))"),
        "(1 2)"
    );
    assert_eq!(eval("(map-let ((:x x)) '((:x . 5)) x)"), "5");
    assert_eq!(eval("(let ((k 'a)) (map-let ((k v)) '((a . 1)) v))"), "1");
    assert_eq!(
        eval("(map-let (a) (let ((h (make-hash-table))) (puthash 'a 3 h) h) a)"),
        "3"
    );
    // A vector is keyed by index, so a symbol key finds nothing.
    assert_eq!(eval("(map-let (a) [9 8] a)"), "nil");
}

/// `(cl-struct TYPE SLOT…)` binds each slot by name, or to a chosen variable.
#[test]
fn cl_struct_pattern_binds_slots() {
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase (make-pt :x 1 :y 2) ((cl-struct pt x y) (list x y)))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase (make-pt :x 1 :y 2) ((cl-struct pt (x a)) a))"),
        "1"
    );
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase-let (((cl-struct pt x) (make-pt :x 4))) x)"),
        "4"
    );
    // An `:include` parent's pattern matches a child.
    assert_eq!(
        eval("(cl-defstruct an name) (cl-defstruct (dog (:include an)) breed) (pcase (make-dog :name \"r\" :breed \"l\") ((cl-struct an name) name))"),
        "\"r\""
    );
}

/// A value of the wrong type must FAIL the clause, not signal out of it — the
/// slot reads happen inside the binders, which `pcase--clause` establishes
/// around the tests.
#[test]
fn cl_struct_pattern_fails_instead_of_erroring() {
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase 5 ((cl-struct pt x) x) (_ 'no))"),
        "no"
    );
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase \"s\" ((cl-struct pt x) x) (_ 'no))"),
        "no"
    );
    // A `guard` reading a bound slot still works, which is what proves the
    // binders are visible to the tests.
    assert_eq!(
        eval("(cl-defstruct pt x y) (pcase (make-pt :x 1 :y 2) ((and (cl-struct pt x) (guard (> x 0))) 'big) (_ 'no))"),
        "big"
    );
}
