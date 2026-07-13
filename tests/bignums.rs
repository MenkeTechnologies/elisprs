//! Bignums: an integer that leaves fixnum range stays exact, as in Emacs.
//!
//! Emacs has no fixed-width integers. `(expt 2 70)` is 2^70, `(* 1e12 1e12)` is
//! exact, and `(1+ most-positive-fixnum)` is a bignum — even though it still fits
//! an `i64`, because Emacs's fixnums are 62-bit (two tag bits). Every expectation
//! below was taken from `emacs -Q --batch --eval '(prin1 EXPR)'` on GNU Emacs
//! 30.2; the fuzz harness (`scripts/fuzz_parity.sh`) is what surfaced them.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The arithmetic that used to wrap silently: `+`/`-`/`*` are lowered to fusevm
/// ops, so these are the cases the VM hands back to the host on overflow.
#[test]
fn arithmetic_promotes_past_fixnum_range() {
    assert_eq!(
        eval("(* 1000000000000 1000000000000)"),
        "1000000000000000000000000"
    );
    assert_eq!(eval("(+ most-positive-fixnum 1)"), "2305843009213693952");
    assert_eq!(eval("(- most-negative-fixnum 1)"), "-2305843009213693953");
    assert_eq!(eval("(- 0 most-negative-fixnum)"), "2305843009213693952");
    assert_eq!(eval("(1+ most-positive-fixnum)"), "2305843009213693952");
    // …and a result that fits stays a plain fixnum.
    assert_eq!(eval("(+ 1 2)"), "3");
    assert_eq!(eval("(* 6 7)"), "42");
}

/// A hot loop runs in JIT-compiled native code once it is warm; the overflow
/// check has to survive that, or the promotion silently stops happening.
#[test]
fn promotion_survives_the_jit() {
    // Enough iterations to cross fusevm's block/trace warmup thresholds.
    assert_eq!(
        eval("(let ((s 0)) (dotimes (_ 200) (setq s (+ s 1))) s)"),
        "200"
    );
    // The same loop, but each iteration overflows fixnum range and must promote.
    assert_eq!(
        eval("(let ((n 0)) (dotimes (_ 50) (setq n (+ most-positive-fixnum 1))) n)"),
        "2305843009213693952"
    );
}

#[test]
fn expt_ash_lsh_abs_are_exact() {
    assert_eq!(eval("(expt 2 70)"), "1180591620717411303424");
    assert_eq!(
        eval("(expt 123456789 5)"),
        "28679718602997181072337614380936720482949"
    );
    assert_eq!(eval("(ash 1 70)"), "1180591620717411303424");
    assert_eq!(eval("(lsh 1 70)"), "1180591620717411303424");
    assert_eq!(eval("(ash (expt 2 70) -68)"), "4");
    assert_eq!(eval("(abs most-negative-fixnum)"), "2305843009213693952");
}

#[test]
fn division_rounding_and_bitwise_are_exact() {
    assert_eq!(eval("(/ (expt 2 70) 3)"), "393530540239137101141");
    assert_eq!(eval("(% (expt 2 70) 7)"), "2");
    assert_eq!(eval("(mod (- (expt 2 70)) 7)"), "5");
    assert_eq!(eval("(floor (expt 2 70) 3)"), "393530540239137101141");
    assert_eq!(
        eval("(logand (expt 2 70) (expt 2 70))"),
        "1180591620717411303424"
    );
    assert_eq!(eval("(logior 1 (expt 2 70))"), "1180591620717411303425");
    assert_eq!(eval("(lognot (expt 2 70))"), "-1180591620717411303425");
    assert_eq!(eval("(logcount (expt 2 70))"), "1");
    // A float too large for an i64 rounds to the exact integer, not a clamp.
    assert_eq!(eval("(truncate 1e30)"), "1000000000000000019884624838656");
}

/// A bignum is an `integer`, not a type of its own — only `fixnump`/`bignump`
/// tell them apart. Two equal bignums are `eql`/`equal` but never `eq`.
#[test]
fn type_and_equality_semantics() {
    assert_eq!(eval("(type-of (expt 2 70))"), "integer");
    assert_eq!(eval("(integerp (expt 2 70))"), "t");
    assert_eq!(eval("(numberp (expt 2 70))"), "t");
    assert_eq!(eval("(fixnump (expt 2 70))"), "nil");
    assert_eq!(eval("(bignump (expt 2 70))"), "t");
    assert_eq!(eval("(fixnump 1)"), "t");
    assert_eq!(eval("(bignump 1)"), "nil");

    assert_eq!(eval("(eql (expt 2 70) (expt 2 70))"), "t");
    assert_eq!(eval("(equal (expt 2 70) (expt 2 70))"), "t");
    assert_eq!(eval("(eq (expt 2 70) (expt 2 70))"), "nil");
    // Equal bignums are `eql`, so they must land in the same hash bucket.
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash (expt 2 70) 'v h) (gethash (expt 2 70) h))"),
        "v"
    );
}

/// Comparison of two integers is exact. Routing it through `f64` (which the old
/// implementation did) runs out of mantissa at 2^53 and calls distinct fixnums
/// equal.
#[test]
fn comparison_is_exact_not_float() {
    assert_eq!(eval("(= 2305843009213693950 2305843009213693951)"), "nil");
    assert_eq!(eval("(< (expt 2 70) (expt 2 71))"), "t");
    assert_eq!(eval("(> (expt 2 70) most-positive-fixnum)"), "t");
    assert_eq!(eval("(max 1 (expt 2 70))"), "1180591620717411303424");
}

/// Reader, printer and the string/format conversions all round-trip a bignum.
#[test]
fn read_print_and_convert() {
    // The reader used to fall back to a float for an integer too big for i64,
    // silently changing both the value and the type.
    assert_eq!(
        eval("(car (read-from-string \"1180591620717411303424\"))"),
        "1180591620717411303424"
    );
    assert_eq!(
        eval("(type-of (car (read-from-string \"1180591620717411303424\")))"),
        "integer"
    );
    assert_eq!(
        eval("(number-to-string (expt 2 70))"),
        "\"1180591620717411303424\""
    );
    assert_eq!(
        eval("(string-to-number \"1180591620717411303424\")"),
        "1180591620717411303424"
    );
    assert_eq!(
        eval("(format \"%d|%x|%o\" (expt 2 70) (expt 2 70) (expt 2 70))"),
        "\"1180591620717411303424|400000000000000000|200000000000000000000000\""
    );
    // Mixing in a float is float-contagious, as in Emacs.
    assert_eq!(eval("(float (expt 2 70))"), "1.1805916207174113e+21");
    assert_eq!(eval("(+ 1.5 (* 1000000000000 1000000000000))"), "1e+24");
}

/// elisp arithmetic signals on a non-number where fusevm's own ops would coerce
/// it awk-style (`"a"` → 0.0). The check has to hold on the JIT-compiled path
/// too, which is why the strict-numeric hook exists in fusevm.
#[test]
fn arithmetic_signals_on_non_numbers() {
    assert_eq!(
        eval("(condition-case e (+ 1 \"a\") (error e))"),
        "(wrong-type-argument number-or-marker-p \"a\")"
    );
    assert_eq!(
        eval("(condition-case e (* 2 (list 1)) (error e))"),
        "(wrong-type-argument number-or-marker-p (1))"
    );
    assert_eq!(
        eval("(condition-case e (min \"str\" 1) (error e))"),
        "(wrong-type-argument number-or-marker-p \"str\")"
    );
    // Emacs uses a *different* predicate for these: strictly a number, no marker.
    assert_eq!(
        eval("(condition-case e (abs 'c) (error e))"),
        "(wrong-type-argument numberp c)"
    );
    assert_eq!(
        eval("(condition-case e (floor 'c) (error e))"),
        "(wrong-type-argument numberp c)"
    );
    assert_eq!(
        eval("(condition-case e (number-to-string (list 1)) (error e))"),
        "(wrong-type-argument numberp (1))"
    );
}
