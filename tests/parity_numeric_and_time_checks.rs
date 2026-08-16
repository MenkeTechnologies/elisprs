//! `lsh`, `logb`, `atan`, the digest range arguments, the time ZONE argument,
//! and `intern`/`intern-soft` on the two symbols elisprs does not put on the
//! heap.
//!
//! All found by the differential fuzzer (`scripts/fuzz_parity.sh`) once its call
//! table reached these subrs. Every expectation is
//! `emacs -Q --batch --eval '(prin1 …)'` on GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// `Flsh` type-checks VALUE with `CHECK_NUMBER` and COUNT with `CHECK_INTEGER`,
/// so the two operands report *different* predicates — and a float VALUE reports
/// the second one. elisprs reported `integerp` for every case.
#[test]
fn lsh_reports_the_predicate_of_the_operand_that_failed() {
    assert_eq!(
        eval(r#"(condition-case e (lsh "" 6) (error e))"#),
        "(wrong-type-argument number-or-marker-p \"\")"
    );
    assert_eq!(
        eval(r#"(condition-case e (lsh 1 "") (error e))"#),
        "(wrong-type-argument integerp \"\")"
    );
    assert_eq!(
        eval("(condition-case e (lsh 1.5 2) (error e))"),
        "(wrong-type-argument integerp 1.5)"
    );
    // `ash` keeps `CHECK_INTEGER` on both, so the two builtins must not converge.
    assert_eq!(
        eval(r#"(condition-case e (ash "" 6) (error e))"#),
        "(wrong-type-argument integerp \"\")"
    );
    // A bignum is a number: the type check must not reject what the shift can do.
    assert_eq!(eval("(lsh 4611686018427387903 7)"), "590295810358705651584");
    assert_eq!(eval("(lsh 1 4)"), "16");
    assert_eq!(eval("(lsh 16 -2)"), "4");
}

/// `logb` of an integer is exact. Converting through `f64` first rounds — 2^61-1
/// rounds *up* to 2^61 — so `(logb 2305843009213693951)` answered 61 where Emacs
/// answers 60. A NaN also passes through with its own sign and payload rather
/// than being rebuilt as a fresh positive NaN.
#[test]
fn logb_is_exact_for_integers_and_preserves_nan() {
    assert_eq!(eval("(logb 2305843009213693951)"), "60");
    assert_eq!(eval("(logb 4611686018427387903)"), "61");
    assert_eq!(eval("(logb (expt 2 70))"), "70");
    assert_eq!(eval("(logb 1023)"), "9");
    assert_eq!(eval("(logb 1024)"), "10");
    assert_eq!(eval("(logb -8)"), "3");
    assert_eq!(eval("(logb -0.0e+NaN)"), "-0.0e+NaN");
    assert_eq!(eval("(logb 0.0e+NaN)"), "0.0e+NaN");
    assert_eq!(eval("(logb 0)"), "-1.0e+INF");
    assert_eq!(eval("(logb 0.5)"), "-1");
}

/// `atan`'s optional X takes the same `CHECK_NUMBER` as Y, which reports
/// `numberp`; elisprs read it through the arithmetic accessor and reported
/// `number-or-marker-p`.
#[test]
fn atan_reports_numberp_for_both_arguments() {
    assert_eq!(
        eval("(condition-case e (atan -2 'car) (error e))"),
        "(wrong-type-argument numberp car)"
    );
    assert_eq!(
        eval("(condition-case e (atan 'car) (error e))"),
        "(wrong-type-argument numberp car)"
    );
    assert_eq!(eval("(atan 1.0 2.0)"), "0.4636476090008061");
}

/// The digest functions index the *encoded bytes*, not the characters — Emacs
/// hashes the string's byte representation — and the bounds go through
/// `validate_subarray`: an integer, negative counting from the end, and
/// `(args-out-of-range OBJECT START END)` for anything outside. elisprs indexed
/// characters and clamped, so `(md5 "abc" 5 nil)` answered the digest of "".
#[test]
fn digest_range_arguments_follow_validate_subarray() {
    // Byte indices: "αβγ" is three characters but six bytes, so 0..3 is a
    // strictly smaller range than the whole string.
    assert_ne!(eval(r#"(md5 "αβγ" 0 3)"#), eval(r#"(md5 "αβγ")"#));
    assert_eq!(
        eval(r#"(md5 "αβγ" 0 3)"#),
        "\"891b1889839aa0f86d817d6162fd8d65\""
    );
    // Out of range signals rather than clamping, and names the bounds as written.
    assert_eq!(
        eval(r#"(condition-case e (md5 "abc" 5 nil) (error e))"#),
        "(args-out-of-range \"abc\" 5 nil)"
    );
    assert_eq!(
        eval(r#"(condition-case e (md5 "abc" 2 1) (error e))"#),
        "(args-out-of-range \"abc\" 2 1)"
    );
    // A negative bound counts from the end.
    assert_eq!(
        eval(r#"(md5 "abc" -1)"#),
        "\"4a8a08f09d37b73795649038408b5f33\""
    );
    // The bounds are integers; a float is a type error, not a truncated index.
    assert_eq!(
        eval(r#"(condition-case e (md5 "abc" 1.5) (error e))"#),
        "(wrong-type-argument integerp 1.5)"
    );
    // The plain digests are unchanged.
    assert_eq!(
        eval(r#"(md5 "abc")"#),
        "\"900150983cd24fb0d6963f7d28e17f72\""
    );
    assert_eq!(
        eval(r#"(secure-hash 'sha256 "abc")"#),
        "\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
    );
}

/// ZONE is `nil`/`wall` (local), `t` (UTC), an integer offset, a TZ string, or a
/// `(OFFSET ABBR)` pair. elisprs accepted anything and read it as UTC, so a
/// float ZONE silently produced a UTC answer and the symbol `wall` — which means
/// *local* — produced a UTC one too.
#[test]
fn time_zone_argument_is_validated() {
    for src in [
        r#"(format-time-string "%Y" 0 1.5)"#,
        r#"(decode-time 0 1.5)"#,
        r#"(current-time-string 0 1.5)"#,
    ] {
        assert_eq!(
            eval(&format!("(condition-case e {src} (error e))")),
            "(error \"Invalid time zone specification\" 1.5)",
            "{src}"
        );
    }
    // Any symbol other than `wall` is rejected, including the plausible `utc`.
    assert_eq!(
        eval(r#"(condition-case e (format-time-string "%Y" 0 'utc) (error e))"#),
        "(error \"Invalid time zone specification\" utc)"
    );
    // A one-element list is not the `(OFFSET ABBR)` shape.
    assert_eq!(
        eval(r#"(condition-case e (format-time-string "%Y" 0 '(3600)) (error e))"#),
        "(error \"Invalid time zone specification\" (3600))"
    );
    // The accepted spellings still work. `wall` is compared against the local
    // reading rather than a fixed string, so this holds in any TZ.
    assert_eq!(eval(r#"(format-time-string "%Y" 0 t)"#), "\"1970\"");
    assert_eq!(eval(r#"(format-time-string "%Y" 0 "UTC0")"#), "\"1970\"");
    assert_eq!(
        eval(r#"(format-time-string "%Y" 0 '(3600 "X"))"#),
        "\"1970\""
    );
    assert_eq!(
        eval(
            r#"(equal (format-time-string "%Y-%m-%dT%H:%M:%S" 0 'wall)
                       (format-time-string "%Y-%m-%dT%H:%M:%S" 0))"#
        ),
        "t"
    );
}

/// `nil` and `t` are interned symbols in Emacs. elisprs represents them as
/// `Value::Undef`/`Value::Bool` rather than heap symbols, so interning their
/// names built a *different* object that printed as `nil` but was not `eq` to
/// it — `(and (intern "nil") 1)` answered 1.
#[test]
fn interning_nil_and_t_yields_those_objects() {
    assert_eq!(eval(r#"(eq (intern "nil") nil)"#), "t");
    assert_eq!(eval(r#"(eq (intern "t") t)"#), "t");
    assert_eq!(eval(r#"(and (intern "nil") 1)"#), "nil");
    assert_eq!(eval("(intern-soft t)"), "t");
    assert_eq!(eval(r#"(intern-soft "t")"#), "t");
    assert_eq!(eval(r#"(intern-soft "nil")"#), "nil");
    // A private obarray starts empty, so it gets its own fresh symbol — the
    // shortcut is the *global* obarray's pre-population, not a name rule.
    assert_eq!(
        eval(r#"(let ((o (obarray-make))) (eq (intern "nil" o) nil))"#),
        "nil"
    );
    // An ordinary name is unaffected.
    assert_eq!(eval(r#"(intern "foo")"#), "foo");
    assert_eq!(eval(r#"(intern-soft "no-such-name-xyz")"#), "nil");
}

/// `indirect-function` of `t`/`nil` is nil: they are symbols with no function
/// cell, not self-evaluating non-symbols. elisprs returned them unchanged.
#[test]
fn indirect_function_of_t_and_nil_is_nil() {
    assert_eq!(eval("(indirect-function t)"), "nil");
    assert_eq!(eval("(indirect-function nil)"), "nil");
    // A genuine non-symbol still comes back as itself.
    assert_eq!(eval("(indirect-function 5)"), "5");
    assert_eq!(eval("(indirect-function 'car)"), "#<subr car>");
}

/// `member-ignore-case` walks LIST with `CHECK_LIST_END`, so a non-nil tail is
/// `(wrong-type-argument listp TAIL)`. elisprs stopped at any non-cons and
/// answered nil, which turned a type error into "not found".
#[test]
fn member_ignore_case_checks_the_list_end() {
    assert_eq!(
        eval(r#"(condition-case e (member-ignore-case "a" 1.5) (error e))"#),
        "(wrong-type-argument listp 1.5)"
    );
    assert_eq!(
        eval(r#"(condition-case e (member-ignore-case "z" '("a" . 5)) (error e))"#),
        "(wrong-type-argument listp 5)"
    );
    // A match found before the improper tail returns normally, as in Emacs.
    assert_eq!(
        eval(r#"(member-ignore-case "a" '("a" . 5))"#),
        "(\"a\" . 5)"
    );
    assert_eq!(
        eval(r#"(member-ignore-case "b" '("A" "B" "C"))"#),
        "(\"B\" \"C\")"
    );
    assert_eq!(eval(r#"(member-ignore-case "a" nil)"#), "nil");
}

/// A `wrong-type-argument` datum that has no read syntax has to travel as the
/// object. Rendering `#<subr +>` into the message and re-reading it dropped it,
/// leaving `(wrong-type-argument char-or-string-p)` with no offender named.
#[test]
fn case_folding_names_the_offending_object() {
    assert_eq!(
        eval("(condition-case e (upcase (symbol-function '+)) (error e))"),
        "(wrong-type-argument char-or-string-p #<subr +>)"
    );
    assert_eq!(
        eval("(condition-case e (downcase (symbol-function 'car)) (error e))"),
        "(wrong-type-argument char-or-string-p #<subr car>)"
    );
    assert_eq!(
        eval("(condition-case e (upcase -1) (error e))"),
        "(wrong-type-argument char-or-string-p -1)"
    );
    // A character above the character range is returned unchanged, not rejected.
    assert_eq!(eval("(upcase 4194304)"), "4194304");
    assert_eq!(eval(r#"(upcase "abc")"#), "\"ABC\"");
}
