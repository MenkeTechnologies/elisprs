//! `seq-into` returns SEQUENCE itself when it already has the target type, and
//! `seq-mapn` converts through it — so a dotted list reaches the `car` walk
//! whole instead of being flattened by `append`.
//!
//! seq.el (identical in GNU Emacs 30.2 and 31.1):
//!
//! ```elisp
//! (defun seq--into-list (sequence)
//!   (if (listp sequence) sequence (append sequence nil)))
//! ```
//!
//! and the `vector`/`string` arms are the same shape over `vconcat`/`concat`.
//! Expectations are `emacs -Q --batch --eval '(prin1 EXPR)'`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// The result is the argument, not a copy: mutating one mutates the other.
/// elisprs rebuilt every sequence through `append`, so all three were `nil`.
#[test]
fn seq_into_its_own_type_is_the_same_object() {
    assert_eq!(eval("(let ((l (list 1 2))) (eq l (seq-into l 'list)))"), "t");
    assert_eq!(
        eval("(let ((v (vector 1 2))) (eq v (seq-into v 'vector)))"),
        "t"
    );
    assert_eq!(
        eval("(let ((s (string 97))) (eq s (seq-into s 'string)))"),
        "t"
    );
    // A dotted list is a list: handed back whole, where `append` signalled.
    assert_eq!(eval("(seq-into (cons 1 2) 'list)"), "(1 . 2)");
    // Conversions between types are unchanged.
    assert_eq!(eval("(seq-into [97 98] 'string)"), "\"ab\"");
    assert_eq!(eval("(seq-into \"ab\" 'vector)"), "[97 98]");
    assert_eq!(
        err("(seq-into '(1.5) 'string)"),
        "(wrong-type-argument characterp 1.5)"
    );
    assert_eq!(err("(seq-into 0 'vector)"), "(wrong-type-argument sequencep 0)");
}

/// With the dotted list kept whole, the first failure is FUNCTION's own on the
/// first elements, not `append`'s on the dotted tail. The first case was
/// recorded in BUGS.md as an open argument-order divergence.
#[test]
fn seq_mapn_walks_a_dotted_list_instead_of_flattening_it() {
    assert_eq!(
        err("(seq-mapn #'string-to-number \"-4.5\" (cons 2 10))"),
        "(wrong-type-argument stringp 45)"
    );
    assert_eq!(
        err("(seq-mapn #'cons '(1 2 3) (cons 1 2))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        err("(seq-mapn nil (cons nil 0) t)"),
        "(wrong-type-argument sequencep t)"
    );
    assert_eq!(eval("(seq-mapn #'+ '(1 2) [3 4 5])"), "(4 6)");
}
