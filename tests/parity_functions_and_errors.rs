//! Second round of differential-fuzz findings (`scripts/fuzz_parity.sh`).
//!
//! Expectations are GNU Emacs 30.2's, taken from
//! `emacs -Q --batch --eval '(prin1 EXPR)'`. Round one is in
//! `tests/parity_fuzz_findings.rs` and `tests/bignums.rs`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// An interpreted closure prints as its *source* — `#[ARGLIST BODY ENV]` — where
/// ENV is the captured lexical alist, newest binding first, or `(t)` when nothing
/// is captured. elisprs lowers the body to a fusevm `Chunk`, so the closure has to
/// retain the forms; it used to print an opaque `#<closure>`.
#[test]
fn closures_print_like_emacs() {
    assert_eq!(eval("(lambda (x) (list x x))"), "#[(x) ((list x x)) (t)]");
    assert_eq!(
        eval("(lambda (a &optional b &rest c) a b)"),
        "#[(a &optional b &rest c) (a b) (t)]"
    );
    assert_eq!(
        eval("(let ((x 1)) (lambda (y) (+ x y)))"),
        "#[(y) ((+ x y)) ((x . 1))]"
    );
    assert_eq!(
        eval("(let ((x 1) (z 2)) (lambda () (+ x z)))"),
        "#[nil ((+ x z)) ((z . 2) (x . 1))]"
    );
}

/// `(eval FORM t)` evaluates FORM in an EMPTY lexical environment, not the
/// caller's: `t` means "lexical binding", not "inherit my bindings". Running it in
/// the live scope chain leaked every binding the caller held into FORM — and into
/// any closure FORM created.
#[test]
fn eval_does_not_leak_the_callers_lexical_scope() {
    assert_eq!(err("(let ((x 5)) (eval 'x t))"), "(void-variable x)");
    // A closure built by `eval` captures nothing, so it prints with an empty env.
    assert_eq!(
        eval("(let ((leaked 1)) (prin1-to-string (eval '(lambda (x) x) t)))"),
        "\"#[(x) (x) (t)]\""
    );
    // FORM's own bindings still work.
    assert_eq!(eval("(let ((x 5)) (eval '(let ((y 2)) (+ y 1)) t))"), "3");
}

/// `wrong-number-of-arguments` carries `(FUNCTION COUNT)`. The function is the
/// callee as the caller wrote it — a symbol for a direct call — but `funcall` and
/// `apply` resolve the designator first, so their error names the function object.
#[test]
fn arity_errors_name_the_function_and_the_count() {
    assert_eq!(
        err("(char-to-string)"),
        "(wrong-number-of-arguments char-to-string 0)"
    );
    assert_eq!(
        err("(apply #'char-to-string nil)"),
        "(wrong-number-of-arguments #<subr char-to-string> 0)"
    );
    assert_eq!(
        err("(funcall 'char-to-string)"),
        "(wrong-number-of-arguments #<subr char-to-string> 0)"
    );
    assert_eq!(
        err("(funcall (lambda (x) x) 1 2)"),
        "(wrong-number-of-arguments #[(x) (x) (t)] 2)"
    );
    // The data holds the closure itself, so it cannot be rebuilt by re-reading a
    // printed message — the error object is constructed from real values.
    assert_eq!(err("(funcall 5)"), "(invalid-function 5)");
}

/// Numeric arguments are checked left to right, so the error names the FIRST
/// offender, and `seq-min`/`seq-max` inherit that through `min`/`max`.
#[test]
fn numeric_arguments_are_checked_in_order() {
    assert_eq!(
        err("(max t 'foo)"),
        "(wrong-type-argument number-or-marker-p t)"
    );
    assert_eq!(
        err("(max 1 'foo 2)"),
        "(wrong-type-argument number-or-marker-p foo)"
    );
    assert_eq!(
        err("(seq-max (list t 'foo))"),
        "(wrong-type-argument number-or-marker-p t)"
    );
    assert_eq!(
        err("(seq-min (list nil))"),
        "(wrong-type-argument number-or-marker-p nil)"
    );
}

/// seq.el reports an out-of-range subsequence differently per type: an array
/// signals `args-out-of-range`, a list a plain `error`, a non-sequence another.
/// They used to be silently clamped to an empty result.
#[test]
fn seq_subseq_bounds() {
    assert_eq!(
        err("(seq-subseq (list 1 2) 42)"),
        "(error \"Start index out of bounds: 42\")"
    );
    assert_eq!(
        err("(seq-subseq \"ab\" 42)"),
        "(args-out-of-range \"ab\" 42 nil)"
    );
    assert_eq!(
        err("(seq-subseq (vector 1 2) 5)"),
        "(args-out-of-range [1 2] 5 nil)"
    );
    assert_eq!(
        err("(seq-subseq 5 1)"),
        "(error \"Unsupported sequence: 5\")"
    );
    // In range, unchanged.
    assert_eq!(eval("(seq-subseq (list 1 2 3) 1)"), "(2 3)");
    assert_eq!(eval("(seq-subseq (list 1 2 3) -2)"), "(2 3)");
    assert_eq!(eval("(seq-subseq \"abcd\" 1 3)"), "\"bc\"");
}

/// Type contracts the fuzzer caught returning a value where Emacs signals.
#[test]
fn type_contracts() {
    assert_eq!(
        err("(capitalize nil)"),
        "(wrong-type-argument char-or-string-p nil)"
    );
    assert_eq!(
        err("(upcase-initials nil)"),
        "(wrong-type-argument char-or-string-p nil)"
    );
    assert_eq!(
        err("(sort \"ab\" #'<)"),
        "(wrong-type-argument list-or-vector-p \"ab\")"
    );
    assert_eq!(
        err("(mapcar #'car 97)"),
        "(wrong-type-argument sequencep 97)"
    );
    assert_eq!(err("(mapc #'car 97)"), "(wrong-type-argument sequencep 97)");
    assert_eq!(
        err("(format 1.5 \"x\")"),
        "(wrong-type-argument stringp 1.5)"
    );
    assert_eq!(err("(intern t)"), "(wrong-type-argument stringp t)");
    assert_eq!(
        err("(string-equal-ignore-case \"a\" 1.5)"),
        "(wrong-type-argument stringp 1.5)"
    );
    // …and the ones where Emacs is LENIENT and elisprs was signalling.
    assert_eq!(eval("(last t 0)"), "t");
    assert_eq!(eval("(last t)"), "t");
    assert_eq!(eval("(plist-get 'sym 1)"), "nil");
}

/// An empty separator regexp matches at every position, including before the
/// first character and after the last.
#[test]
fn split_string_with_an_empty_separator() {
    assert_eq!(
        eval("(split-string \"a1b\" \"\")"),
        "(\"\" \"a\" \"1\" \"b\" \"\")"
    );
    assert_eq!(eval("(split-string \"\" \"\")"), "(\"\")");
    assert_eq!(eval("(split-string \"123\" \"\" t)"), "(\"1\" \"2\" \"3\")");
}

/// A shift count can itself be a bignum now: shifting left by it would exhaust
/// memory (Emacs signals), shifting right collapses to the sign.
#[test]
fn ash_with_a_huge_count() {
    assert_eq!(err("(ash 7 4611686018427387902)"), "(overflow-error)");
    assert_eq!(eval("(ash 7 (- 4611686018427387902))"), "0");
    assert_eq!(eval("(ash 1 70)"), "1180591620717411303424");
    assert_eq!(eval("(ash 1024 -3)"), "128");
}
