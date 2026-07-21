//! Round-7 differential-fuzz findings.
//!
//! Expectations are GNU Emacs 30.2's (`emacs -Q --batch --eval '(prin1 EXPR)'`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// `split-string`'s SEPARATORS is a regexp, matched the way `string-match` matches —
/// which honours `case-fold-search` (non-nil by default). It was compiled
/// case-sensitively, so `(split-string "aAbB" "a\\|b" t)` answered `("A" "B")` where
/// Emacs answers nil: with the fold on, every character is a separator.
#[test]
fn split_string_honours_case_fold_search() {
    assert_eq!(eval(r#"(split-string "aAbB" "a\\|b" t)"#), "nil");
    assert_eq!(
        eval(r#"(split-string "aAbB" "a\\|b")"#),
        "(\"\" \"\" \"\" \"\" \"\")"
    );
    // …and binding it off restores the case-sensitive split.
    assert_eq!(
        eval(r#"(let ((case-fold-search nil)) (split-string "aAbB" "a\\|b" t))"#),
        "(\"A\" \"B\")"
    );
    assert_eq!(eval(r#"(split-string "a,b" ",")"#), "(\"a\" \"b\")");
}

/// `remq` walks like `memq`: a dotted tail is `CHECK_LIST_END`, naming the WHOLE
/// list. `delete-dups` of a non-list is `sequencep`.
#[test]
fn destructive_list_ops_walk_like_emacs() {
    assert_eq!(
        err(r#"(remq 1 (cons "a" "b"))"#),
        "(wrong-type-argument listp (\"a\" . \"b\"))"
    );
    assert_eq!(eval("(remq 1 (list 1 2))"), "(2)");
    assert_eq!(
        err("(delete-dups (append nil 0))"),
        "(wrong-type-argument sequencep 0)"
    );
    assert_eq!(eval("(delete-dups (list 1 1 2))"), "(1 2)");
}

/// `mapcan` nconcs its results, so a result that is not a list signals `consp`.
#[test]
fn mapcan_requires_list_results() {
    assert_eq!(
        err("(mapcan #'integerp (list 1 2))"),
        "(wrong-type-argument consp t)"
    );
    assert_eq!(eval("(mapcan #'list (list 1 2))"), "(1 2)");
}

/// A character argument must actually be a character: a negative integer signals
/// rather than case-folding into nonsense.
#[test]
fn case_ops_reject_a_non_character() {
    assert_eq!(
        err("(downcase -1)"),
        "(wrong-type-argument char-or-string-p -1)"
    );
    assert_eq!(
        eval("(list (downcase 97) (upcase 97) (upcase 1114112))"),
        "(97 65 1114112)"
    );
}

/// `string<` / `string-lessp` / `string>` take a string or a symbol; anything else
/// is `stringp`. They used to compare whatever they were handed.
#[test]
fn string_comparisons_check_their_arguments() {
    assert_eq!(
        err("(string-lessp [1 2] \"x\")"),
        "(wrong-type-argument stringp [1 2])"
    );
    assert_eq!(
        eval("(list (string< \"a\" \"b\") (string-lessp 'a \"b\"))"),
        "(t t)"
    );
}

/// `natnump`/`wholenump` are subrs, which is what their printed form in an arity
/// error depends on (`#<subr natnump>` when reached through `apply`).
#[test]
fn natnump_is_a_subr() {
    assert_eq!(
        eval("(list (natnump 5) (natnump -1) (natnump \"x\") (wholenump 3))"),
        "(t nil nil t)"
    );
    assert_eq!(eval("(natnump (expt 2 70))"), "t");
    assert_eq!(
        err("(apply #'natnump (list 1 2))"),
        "(wrong-number-of-arguments #<subr natnump> 2)"
    );
}
