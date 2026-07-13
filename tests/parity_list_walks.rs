//! Round-6 differential-fuzz findings: how a list walk fails, and which predicate
//! each primitive actually checks.
//!
//! Emacs is not uniform here, and the differences are observable. Expectations are
//! GNU Emacs 30.2's (`emacs -Q --batch --eval '(prin1 EXPR)'`; the `subr-x` cases
//! with that library loaded, as `scripts/fuzz/drive.el` does).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// An improper list is reported differently depending on the primitive walking it.
/// `memq`/`member`/`memql`/`rassq` use Emacs's `CHECK_LIST_END`, which names the
/// WHOLE list; `reverse`/`append`/`sort` name the offending TAIL. These used to
/// stop silently at the dotted tail and answer nil.
#[test]
fn improper_lists_report_what_emacs_reports() {
    // CHECK_LIST_END: the whole list.
    assert_eq!(
        err("(memq 1 (cons 9 3))"),
        "(wrong-type-argument listp (9 . 3))"
    );
    assert_eq!(
        err("(rassq 1 (cons (cons 1 2) 3))"),
        "(wrong-type-argument listp ((1 . 2) . 3))"
    );
    // The tail.
    assert_eq!(err("(reverse (cons 1 2))"), "(wrong-type-argument listp 2)");
    assert_eq!(
        err("(append (cons 1 2) nil)"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        err("(sort (cons 97 [1 2]) #'<)"),
        "(wrong-type-argument listp [1 2])"
    );
    // A walk that FINDS its element before reaching the bad tail never signals.
    assert_eq!(eval("(assq 1 (cons (cons 1 2) 3))"), "(1 . 2)");
    // …and the ordinary cases still work.
    assert_eq!(eval("(memq 2 (list 1 2 3))"), "(2 3)");
    assert_eq!(eval("(member \"a\" (list \"a\"))"), "(\"a\")");
    assert_eq!(eval("(rassq 2 (list (cons 1 2)))"), "(1 . 2)");
}

/// `t` and `nil` ARE symbols in Emacs — they simply have no function cell — so
/// calling one is `void-function`, not `invalid-function`. elisprs represents them
/// as `Value::Bool`/`Value::Undef` rather than heap symbols, which is why they need
/// naming explicitly.
#[test]
fn calling_t_or_nil_is_void_function() {
    assert_eq!(err("(funcall t)"), "(void-function t)");
    assert_eq!(err("(funcall nil)"), "(void-function nil)");
    // A genuinely non-callable object is still `invalid-function`.
    assert_eq!(err("(funcall 5)"), "(invalid-function 5)");
}

/// The float math primitives take strictly a NUMBER (`numberp`), not the
/// arithmetic ops' `number-or-marker-p`.
#[test]
fn float_math_signals_numberp() {
    assert_eq!(err("(sin 'car)"), "(wrong-type-argument numberp car)");
    assert_eq!(err("(cos \"x\")"), "(wrong-type-argument numberp \"x\")");
    assert_eq!(err("(sqrt 'c)"), "(wrong-type-argument numberp c)");
    assert_eq!(eval("(list (sin 0) (exp 0) (sqrt 4))"), "(0.0 1.0 2.0)");
}

/// Type contracts a widened corpus reached.
#[test]
fn assorted_type_contracts() {
    // `copy-sequence` of a non-sequence signals; it used to hand the value back.
    assert_eq!(
        err("(copy-sequence t)"),
        "(wrong-type-argument sequencep t)"
    );
    assert_eq!(eval("(copy-sequence (list 1 2))"), "(1 2)");
    // `string-width` wants a string, though it is built on `string-to-list`, which
    // takes any sequence.
    assert_eq!(err("(string-width 97)"), "(wrong-type-argument stringp 97)");
    assert_eq!(eval("(string-width \"ab\")"), "2");
    // `plist-put` validates the plist's SHAPE, naming the whole plist.
    assert_eq!(
        err("(plist-put (list t 1 \"x\") 'a 2)"),
        "(wrong-type-argument plistp (t 1 \"x\"))"
    );
    // `upcase-initials` of a character upcases it, like `upcase`/`capitalize`.
    assert_eq!(
        eval("(list (upcase-initials 97) (capitalize 97))"),
        "(65 65)"
    );
    assert_eq!(eval("(upcase-initials \"ab cd\")"), "\"Ab Cd\"");
    // `assoc-string` skips an element it cannot compare, and stops at a dotted tail.
    assert_eq!(eval("(assoc-string \"x\" (list [1] \"x\"))"), "\"x\"");
    assert_eq!(eval("(assoc-string \"x\" (cons (vector 1) -7))"), "nil");
}
