//! `split-string` against GNU Emacs 31.1's subr.el, which replaced the 30.x
//! `push-one` walk with an index walk. Every expectation is the output of
//! `emacs -Q --batch --eval '(prin1 FORM)'` for the same form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// A separator that can match the empty string advances `next` by one past each
/// search, so it splits between every character and keeps both end items; OMIT-EMPTY
/// drops them again.
#[test]
fn empty_matching_separators_split_between_characters() {
    assert_eq!(
        eval(r#"(split-string "abab" "x*")"#),
        r#"("" "a" "b" "a" "b" "")"#
    );
    assert_eq!(eval(r#"(split-string "abab" "x*" t)"#), r#"("a" "b" "a" "b")"#);
    assert_eq!(eval(r#"(split-string "abc" "\\b")"#), r#"("" "abc" "")"#);
    assert_eq!(eval(r#"(split-string "ab" "\\`")"#), r#"("" "ab")"#);
    assert_eq!(eval(r#"(split-string "ab" "$")"#), r#"("ab" "")"#);
    assert_eq!(
        eval(r#"(split-string "a b" "" nil)"#),
        r#"("" "a" " " "b" "")"#
    );
    assert_eq!(eval(r#"(split-string "" "x*")"#), r#"("" "")"#);
}

/// TRIM is spliced into anchored regexps with `concat`: any sequence of
/// characters is accepted, an empty TRIM trims nothing, and a trim that empties
/// an item keeps it only under keep-empty.
#[test]
fn trim_is_concatenated_into_anchored_regexps() {
    assert_eq!(
        eval(r#"(split-string "abc" "b" nil (list ?a))"#),
        r#"("" "c")"#
    );
    assert_eq!(eval(r#"(split-string "a,b" "," nil "")"#), r#"("a" "b")"#);
    assert_eq!(
        eval(r#"(split-string ",a," "," nil "x*")"#),
        r#"("" "a" "")"#
    );
    assert_eq!(
        eval(r#"(split-string "  x  y " nil nil " ")"#),
        r#"("x" "y")"#
    );
    assert_eq!(
        eval(r#"(condition-case e (split-string "abc" "b" nil 97) (error e))"#),
        "(wrong-type-argument sequencep 97)"
    );
}

/// STRING's `length` is taken before anything is matched, so a non-sequence is
/// `sequencep`; a sequence that is not a string reaches `string-match` and is
/// `stringp`.
#[test]
fn argument_checks_follow_the_lisp_definition_order() {
    let err = |form: &str| eval(&format!("(condition-case e {form} (error e))"));
    assert_eq!(err("(split-string 0)"), "(wrong-type-argument sequencep 0)");
    assert_eq!(err("(split-string 1.5)"), "(wrong-type-argument sequencep 1.5)");
    assert_eq!(err("(split-string 'car)"), "(wrong-type-argument sequencep car)");
    assert_eq!(err("(split-string nil)"), "(wrong-type-argument stringp nil)");
    assert_eq!(err("(split-string [97])"), "(wrong-type-argument stringp [97])");
}

/// The match data is the last `string-match` the walk ran, and the single item
/// of an unsplit string is STRING itself.
#[test]
fn match_data_and_identity_follow_the_walk() {
    assert_eq!(
        eval(r#"(progn (set-match-data nil) (split-string "a,b,c" ",") (match-data))"#),
        "(3 4)"
    );
    assert_eq!(
        eval(r#"(progn (split-string "a,b" "," nil "a") (match-data))"#),
        "(0 1)"
    );
    assert_eq!(
        eval(r#"(let ((s "abc")) (eq s (car (split-string s "x"))))"#),
        "t"
    );
}

/// `split-string-default-separators` is read at call time, as in subr.el.
#[test]
fn default_separators_variable_is_honoured() {
    assert_eq!(
        eval(r#"(let ((split-string-default-separators ",")) (split-string "a,b c"))"#),
        r#"("a" "b c")"#
    );
}
