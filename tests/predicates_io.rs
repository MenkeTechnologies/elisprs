//! Remaining-gap coverage (from a symbol-vs-test audit): the `atom`/`listp`/
//! `numberp`/`vectorp` type predicates, the side-effecting IO builtins
//! (`prin1`/`princ`/`terpri`), match-data round-tripping, and the `seq-do`/
//! `seq-each`/`pcase-let` forms. Expectations captured from the interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn type_predicates() {
    // atom is "not a cons"; nil is an atom.
    assert_eq!(eval("(list (atom 5) (atom '(1)) (atom nil))"), "(t nil t)");
    // listp accepts nil and conses, rejects non-lists.
    assert_eq!(
        eval("(list (listp '(1)) (listp nil) (listp 5))"),
        "(t t nil)"
    );
    assert_eq!(
        eval("(list (numberp 5) (numberp 5.0) (numberp 'x))"),
        "(t t nil)"
    );
    assert_eq!(
        eval("(list (vectorp (vector 1)) (vectorp '(1)))"),
        "(t nil)"
    );
}

#[test]
fn io_builtins_return_their_argument() {
    // prin1/princ return the printed value (the stdout side effect is separate).
    assert_eq!(eval("(prin1 'sym)"), "sym");
    assert_eq!(eval("(princ \"x\")"), "\"x\"");
    assert_eq!(eval("(terpri)"), "t");
}

#[test]
fn set_match_data_round_trips() {
    // set-match-data installs positions that match-beginning/end then read back.
    assert_eq!(
        eval("(progn (string-match \"a\" \"xa\") (set-match-data '(0 1)) (match-beginning 0))"),
        "0"
    );
    assert_eq!(
        eval("(progn (string-match \"a\" \"xa\") (set-match-data '(2 5)) (match-end 0))"),
        "5"
    );
}

#[test]
fn seq_iteration_for_side_effects() {
    // seq-do / seq-each run the function for effect over the sequence.
    assert_eq!(
        eval("(let ((s 0)) (seq-do (lambda (x) (setq s (+ s x))) '(1 2 3)) s)"),
        "6"
    );
    assert_eq!(
        eval("(let ((s 0)) (seq-each (lambda (x) (setq s (+ s x))) '(4 5 6)) s)"),
        "15"
    );
    // seq-do returns the sequence it iterated.
    assert_eq!(eval("(seq-do #'identity '(1 2 3))"), "(1 2 3)");
}

#[test]
fn pcase_let_binds() {
    assert_eq!(eval("(pcase-let ((a 1)) a)"), "1");
    assert_eq!(eval("(pcase-let ((a 2) (b 3)) (+ a b))"), "5");
}
