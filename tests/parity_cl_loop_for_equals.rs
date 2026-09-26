//! cl-loop's `for VAR = INIT [then STEP]` clause.
//!
//! cl-macs.el (GNU Emacs 31.1) sets VAR at the top of every iteration, in
//! clause order with the other `for` clauses — INIT on the first pass, STEP
//! afterwards, or INIT every pass when there is no `then` — so INIT and STEP
//! see the value an earlier `for ... in` clause has just stepped to.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn init_and_step_see_earlier_clauses_of_the_same_iteration() {
    assert_eq!(
        eval("(cl-loop for y in '(1 2 3) for x = (* y y) collect x)"),
        "(1 4 9)"
    );
    assert_eq!(
        eval("(cl-loop for y in '(1 2 3) for x = y then (* 10 y) collect x)"),
        "(1 20 30)"
    );
    assert_eq!(
        eval("(cl-loop for name in '((x 5) (y)) for v = (or (cadr name) (car name)) collect v)"),
        "(5 y)"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 3 for sq = (* i i) collect sq)"),
        "(1 4 9)"
    );
}

#[test]
fn a_then_clause_starts_from_init() {
    assert_eq!(
        eval("(cl-loop for x = 0 then (1+ x) while (< x 3) finally return x)"),
        "3"
    );
    assert_eq!(
        eval("(cl-loop for x = 1 then (* 2 x) repeat 5 collect x)"),
        "(1 2 4 8 16)"
    );
    assert_eq!(
        eval("(cl-loop for x = 5 then (1- x) until (= x 0) sum x)"),
        "15"
    );
}
