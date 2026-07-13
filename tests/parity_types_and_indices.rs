//! Round-4 differential-fuzz findings: the type contracts a *widened* corpus
//! reached — hash tables, regexps, text properties, cl-lib, the printer variables
//! and `format`'s directives (`scripts/fuzz/gen.el`).
//!
//! Expectations are GNU Emacs 30.2's, from `emacs -Q --batch --eval '(prin1 EXPR)'`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// `length` of a non-sequence signals. It answered 0 — so `(length 'car)` was a
/// silent 0 and `(ash (length 'car) 5)` a silent 0 too. `safe-length` is the one
/// that answers 0.
#[test]
fn length_of_a_non_sequence_signals() {
    assert_eq!(err("(length 'car)"), "(wrong-type-argument sequencep car)");
    assert_eq!(err("(length t)"), "(wrong-type-argument sequencep t)");
    assert_eq!(eval("(safe-length 'car)"), "0");
    assert_eq!(
        eval("(list (length nil) (length \"ab\") (length [1 2 3]) (length (list 1 2)))"),
        "(0 2 3 2)"
    );
}

/// The bit-logic ops name a predicate that depends on the argument's POSITION.
/// Emacs's `bit_op` checks the first argument with a direct `CHECK_INTEGER`, and
/// each later one for number-ness first — so the same bad value reports differently
/// depending on where it sits. (`%` checks integer directly, in both positions.)
#[test]
fn bit_ops_name_the_predicate_they_actually_checked() {
    // First argument: always `integer-or-marker-p`.
    assert_eq!(
        err("(logand \"x\" 2)"),
        "(wrong-type-argument integer-or-marker-p \"x\")"
    );
    assert_eq!(
        err("(logxor t -1)"),
        "(wrong-type-argument integer-or-marker-p t)"
    );
    // Later argument: a non-number is `number-or-marker-p`…
    assert_eq!(
        err("(logand 2 \"x\")"),
        "(wrong-type-argument number-or-marker-p \"x\")"
    );
    assert_eq!(
        err("(logxor -1 t)"),
        "(wrong-type-argument number-or-marker-p t)"
    );
    // …but a float IS a number, so it is still `integer-or-marker-p`.
    assert_eq!(
        err("(logand 2 1.0)"),
        "(wrong-type-argument integer-or-marker-p 1.0)"
    );
    assert_eq!(
        err("(% \"x\" 2)"),
        "(wrong-type-argument integer-or-marker-p \"x\")"
    );
    // The shifts and lognot/logcount check `integerp` directly.
    assert_eq!(err("(ash 1.0 2)"), "(wrong-type-argument integerp 1.0)");
    assert_eq!(
        eval("(list (logand 12 10) (logior 1 2) (logxor 3 1))"),
        "(8 3 2)"
    );
}

/// An array index is a FIXNUM: a float, a list, or a bignum all signal `fixnump`.
/// A bignum index was being coerced through `as_num` and reported back as a
/// *different* number in the `args-out-of-range` data.
#[test]
fn array_indices_must_be_fixnums() {
    assert_eq!(
        err("(elt \"abc\" 4611686018427387903)"),
        "(wrong-type-argument fixnump 4611686018427387903)"
    );
    assert_eq!(
        err("(aref [1] (list 1))"),
        "(wrong-type-argument fixnump (1))"
    );
    assert_eq!(eval("(list (aref [1 2] 1) (elt \"abc\" 1))"), "(2 98)");
}

/// `prin1-to-string` takes NOESCAPE: a non-nil second argument prints like `princ`.
#[test]
fn prin1_to_string_takes_noescape() {
    assert_eq!(eval("(prin1-to-string \"a\" nil)"), "\"\\\"a\\\"\"");
    assert_eq!(eval("(prin1-to-string \"a\" t)"), "\"a\"");
}

/// A plist has its own predicate, and `plist-get` stays lenient where
/// `plist-member` signals — exactly as Emacs splits them.
#[test]
fn plist_predicates() {
    assert_eq!(err("(plist-put 97 1 2)"), "(wrong-type-argument plistp 97)");
    assert_eq!(
        err("(plist-member 97 1)"),
        "(wrong-type-argument plistp 97)"
    );
    assert_eq!(eval("(plist-get 97 1)"), "nil");
    assert_eq!(eval("(plist-get '(a 1 b 2) 'b)"), "2");
}

/// `isnan` takes a float; anything else signals rather than answering nil.
#[test]
fn isnan_requires_a_float() {
    assert_eq!(err("(isnan 'sym)"), "(wrong-type-argument floatp sym)");
    assert_eq!(err("(isnan nil)"), "(wrong-type-argument floatp nil)");
    assert_eq!(eval("(list (isnan 0.0e+NaN) (isnan 1.0))"), "(t nil)");
}

/// The higher-order primitives resolve their function designator before calling,
/// so an arity error names the resolved function — and an improper list argument
/// names its tail.
#[test]
fn higher_order_primitives_resolve_designators() {
    assert_eq!(
        err("(sort (list -42 1) #'abs)"),
        "(wrong-number-of-arguments #<subr abs> 2)"
    );
    assert_eq!(
        err("(mapcar #'car 97)"),
        "(wrong-type-argument sequencep 97)"
    );
    assert_eq!(
        err("(mapcar #'abs (cons 1 2))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        err("(append (cons 1 2) nil)"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(eval("(sort (list 3 1) #'<)"), "(1 3)");
    assert_eq!(eval("(mapcar #'1+ (list 1 2))"), "(2 3)");
}
