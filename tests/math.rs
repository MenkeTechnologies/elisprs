//! Numeric coverage: the float-rounding conversions and the integer bitwise
//! ops in `builtins.rs` that the value-parity test in `eval.rs` leaves out.
//! Expectations captured from the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn float_to_int_rounding() {
    // floor/ceiling/truncate round toward -inf / +inf / zero respectively.
    assert_eq!(eval("(floor 3.7)"), "3");
    assert_eq!(eval("(floor -3.1)"), "-4");
    assert_eq!(eval("(ceiling 3.2)"), "4");
    assert_eq!(eval("(truncate 3.9)"), "3");
    assert_eq!(eval("(truncate -3.7)"), "-3");
}

#[test]
fn int_to_float_coercion() {
    assert_eq!(eval("(float 5)"), "5.0");
    assert_eq!(eval("(floatp (float 5))"), "t");
}

#[test]
fn bitwise_ops() {
    // 12 = 1100, 10 = 1010
    assert_eq!(eval("(logand 12 10)"), "8"); // 1000
    assert_eq!(eval("(logior 12 10)"), "14"); // 1110
    assert_eq!(eval("(logxor 12 10)"), "6"); // 0110
    assert_eq!(eval("(lognot 0)"), "-1");
    assert_eq!(eval("(logand 6)"), "6"); // single arg is identity
}

#[test]
fn arithmetic_shifts() {
    assert_eq!(eval("(ash 1 4)"), "16"); // left shift
    assert_eq!(eval("(ash 16 -2)"), "4"); // right shift
    assert_eq!(eval("(ash -8 -1)"), "-4"); // arithmetic (sign-preserving) shift
    assert_eq!(eval("(lsh 1 4)"), "16");
}

/// Emacs compares an integer against a float on their *exact* values
/// (`arithcompare` in data.c), never on `f64` images of them. Past 2^53 an
/// `f64` has no mantissa left, so the two are different numbers that share one
/// image: `(expt 3 34)` = 16677181699666569 and `(float (expt 3 34))` =
/// 16677181699666568.0.
///
/// Ground truth, `emacs --batch` (GNU Emacs 30.2), with L and F bound to those:
///
/// ```text
/// (=  L F) => nil   (<  L F) => nil   (>  L F) => t
/// (=  F L) => nil   (<  F L) => t     (>  F L) => nil
/// ```
///
/// A two-argument comparison is lowered to a fusevm op (`compiler.rs`,
/// `try_native_op`), so these go through `funcall` and the multi-argument forms
/// to reach the `cmp` subr — the path elisprs owns outright.
#[test]
fn integer_and_float_compare_exactly_past_2_pow_53() {
    let l = "(expt 3 34)";
    let f = "(float (expt 3 34))";
    assert_eq!(eval(&format!("(funcall (function =) {l} {f})")), "nil");
    assert_eq!(eval(&format!("(funcall (function <) {l} {f})")), "nil");
    assert_eq!(eval(&format!("(funcall (function >) {l} {f})")), "t");
    assert_eq!(eval(&format!("(funcall (function <=) {l} {f})")), "nil");
    assert_eq!(eval(&format!("(funcall (function >=) {l} {f})")), "t");
    // Mirrored, float first.
    assert_eq!(eval(&format!("(funcall (function =) {f} {l})")), "nil");
    assert_eq!(eval(&format!("(funcall (function <) {f} {l})")), "t");
    assert_eq!(eval(&format!("(funcall (function >) {f} {l})")), "nil");
    assert_eq!(eval(&format!("(funcall (function <=) {f} {l})")), "t");
    assert_eq!(eval(&format!("(funcall (function >=) {f} {l})")), "nil");
    // Three arguments never reach the fusevm op either.
    // Ground truth: (= L F L) => nil, (< F L (1+ L)) => t.
    assert_eq!(eval(&format!("(= {l} {f} {l})")), "nil");
    assert_eq!(eval(&format!("(< {f} {l} (1+ {l}))")), "t");
    // Exactness must not disturb what an f64 represents exactly.
    // Ground truth: (= (expt 2 53) (float (expt 2 53))) => t,
    // (> (1+ (expt 2 53)) (float (expt 2 53))) => t, (= 0 -0.0) => t.
    assert_eq!(
        eval("(funcall (function =) (expt 2 53) (float (expt 2 53)))"),
        "t"
    );
    assert_eq!(
        eval("(funcall (function >) (1+ (expt 2 53)) (float (expt 2 53)))"),
        "t"
    );
    assert_eq!(eval("(funcall (function =) 0 -0.0)"), "t");
    assert_eq!(eval("(funcall (function <) 1 1.5)"), "t");
}

/// `max`/`min` pick their winner with `arithcompare` too, so a mixed pair is
/// decided on exact values and the *argument itself* is returned — which makes
/// the type observable. Ground truth (`emacs --batch`), L and F as above:
///
/// ```text
/// (max L F) => 16677181699666569     (min L F) => 16677181699666568.0
/// (max F L) => 16677181699666569     (min F L) => 16677181699666568.0
/// ```
///
/// `min` is the telling one: it must answer the *float*, because F is the
/// smaller number even though L and F round to the same `f64`.
#[test]
fn max_and_min_decide_a_mixed_pair_exactly() {
    let l = "(expt 3 34)";
    let f = "(float (expt 3 34))";
    assert_eq!(eval(&format!("(max {l} {f})")), "16677181699666569");
    assert_eq!(eval(&format!("(min {l} {f})")), "16677181699666568.0");
    assert_eq!(eval(&format!("(max {f} {l})")), "16677181699666569");
    assert_eq!(eval(&format!("(min {f} {l})")), "16677181699666568.0");
    // Ground truth: (max 1 F L) => 16677181699666569,
    // (min L F (expt 3 35)) => 16677181699666568.0.
    assert_eq!(eval(&format!("(max 1 {f} {l})")), "16677181699666569");
    assert_eq!(
        eval(&format!("(min {l} {f} (expt 3 35))")),
        "16677181699666568.0"
    );
    // A NaN still wins, unchanged. Ground truth: (max 1 0.0e+NaN) => 0.0e+NaN.
    assert_eq!(eval("(max 1 0.0e+NaN)"), "0.0e+NaN");
    assert_eq!(eval("(min 1 0.0e+NaN)"), "0.0e+NaN");
}

/// The *directly lowered* two-argument comparison is exact too.
///
/// This is the path the test above deliberately avoids by going through
/// `funcall`: `(= L F)` written literally is lowered to a fusevm op
/// (`compiler.rs`, `try_native_op`) rather than reaching the `cmp` subr. fusevm
/// 0.17.0 answered a mixed `Int`/`Float` pair natively, on the rounded images,
/// so `(= L F)` was `t` where Emacs says nil; 0.22.0 delegates the pair to the
/// host hook, which compares exact values.
///
/// Nothing else pins this — a fusevm downgrade would silently reintroduce the
/// wrong answer while every `funcall`-routed row above stayed green. Ground
/// truth, `emacs -Q --batch` (GNU Emacs 30.2), L = `(expt 3 34)`, F = `(float L)`:
///
/// ```text
/// (=  L F) => nil   (/= L F) => t     (<  L F) => nil   (>  L F) => t
/// (<= L F) => nil   (>= L F) => t
/// ```
#[test]
fn the_lowered_two_argument_comparison_is_exact_too() {
    let l = "(expt 3 34)";
    let f = "(float (expt 3 34))";
    assert_eq!(eval(&format!("(= {l} {f})")), "nil");
    assert_eq!(eval(&format!("(/= {l} {f})")), "t");
    assert_eq!(eval(&format!("(< {l} {f})")), "nil");
    assert_eq!(eval(&format!("(> {l} {f})")), "t");
    assert_eq!(eval(&format!("(<= {l} {f})")), "nil");
    assert_eq!(eval(&format!("(>= {l} {f})")), "t");
    // Mirrored, float first.
    assert_eq!(eval(&format!("(= {f} {l})")), "nil");
    assert_eq!(eval(&format!("(< {f} {l})")), "t");
    assert_eq!(eval(&format!("(> {f} {l})")), "nil");
    // A value an f64 does represent exactly must still compare equal, so the
    // fix cannot be "call every mixed pair unequal".
    assert_eq!(eval("(= (expt 2 53) (float (expt 2 53)))"), "t");
    assert_eq!(eval("(= 0 -0.0)"), "t");
    assert_eq!(eval("(< 1 1.5)"), "t");
}
