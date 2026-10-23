//! editfns.c `styled_format`'s integer conversions, fns.c's `length<` family
//! and character.c's `string-width` range.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn fmt(spec: &str, arg: &str) -> String {
    eval(&format!("(format \"{spec}\" {arg})"))
}

/// `%o`/`%x`/`%X` take the `+` and space flags like `%d` does: the sign goes
/// before any `0x` and before zero padding.
#[test]
fn radix_conversions_take_sign_flags() {
    assert_eq!(fmt("% x", "10"), "\" a\"");
    assert_eq!(fmt("%+x", "10"), "\"+a\"");
    assert_eq!(fmt("% o", "8"), "\" 10\"");
    assert_eq!(fmt("%+#o", "8"), "\"+010\"");
    assert_eq!(fmt("% #x", "10"), "\" 0xa\"");
    assert_eq!(fmt("% -5x|", "10"), "\" a   |\"");
    assert_eq!(fmt("% 05x|", "10"), "\" 000a|\"");
    assert_eq!(fmt("% x", "10.5"), "\" a\"");
    assert_eq!(fmt("%+x", "(expt 2 70)"), "\"+400000000000000000\"");
    assert_eq!(fmt("%+.0x|", "0"), "\"+|\"");
}

/// A precision turns the `0` flag off, `#` with `o` only ever adds one zero,
/// and a bignum's precision counts its sign.
#[test]
fn precision_zero_flag_and_bignum_sign() {
    assert_eq!(fmt("%05.2x|", "1"), "\"   01|\"");
    assert_eq!(fmt("%05.2d|", "1"), "\"   01|\"");
    assert_eq!(fmt("%#.0o", "0"), "\"0\"");
    assert_eq!(fmt("%#.3o", "1"), "\"001\"");
    assert_eq!(fmt("%#5o|", "1"), "\"   01|\"");
    assert_eq!(
        fmt("%.25x", "(- (expt 2 70))"),
        "\"-000000400000000000000000\""
    );
    assert_eq!(
        fmt("%.25d", "(- (expt 2 70))"),
        "\"-001180591620717411303424\""
    );
    assert_eq!(fmt("%.25x", "(expt 2 70)"), "\"0000000400000000000000000\"");
}

/// `length<`, `length>` and `length=` walk a list only as far as LENGTH, so
/// a dotted tail past it is never reached; LENGTH must be a fixnum, and it is
/// the C `int` parameter of `length_internal`.
#[test]
fn length_comparisons_walk_only_as_far_as_needed() {
    assert_eq!(eval("(length< '(1 2 . 3) 3)"), "t");
    assert_eq!(eval("(length= '(1 2 . 3) 2)"), "t");
    assert_eq!(eval("(length> '(1 2 . 3) 1)"), "t");
    assert_eq!(eval("(length= '(1) -1)"), "nil");
    assert_eq!(eval("(length< '(1 2) 4294967297)"), "nil");
    assert_eq!(
        eval("(condition-case e (length< \"ab\" 2.0) (error e))"),
        "(wrong-type-argument fixnump 2.0)"
    );
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string (condition-case e \
             (length< (let ((l (list 1 2))) (nconc l l)) 70000) (error e))))"
        ),
        "\"(circular-list #1=(1 2 . #1#))\""
    );
}

/// `string-width` measures FROM..TO, validated like `substring`.
#[test]
fn string_width_honors_from_and_to() {
    assert_eq!(eval("(string-width \"abc\" 1 2)"), "1");
    assert_eq!(eval("(string-width \"日本語abc\" 1)"), "7");
    assert_eq!(
        eval("(condition-case e (string-width \"abc\" 1 5) (error e))"),
        "(args-out-of-range \"abc\" 1 5)"
    );
}
