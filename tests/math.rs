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
