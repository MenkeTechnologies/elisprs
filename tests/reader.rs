//! Reader coverage: the lexical syntax handled in `reader.rs` — character
//! literals, dotted pairs, quote / function-quote sugar, keyword symbols,
//! numeric classification, and backquote templating. Every form is read,
//! lowered, and run; the value is what we assert on.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn character_literals_are_integers() {
    assert_eq!(eval("?A"), "65");
    assert_eq!(eval("(+ ?A 1)"), "66");
    // Backslash escapes inside char literals.
    assert_eq!(eval("(list ?\\n ?\\t ?\\e ?\\0)"), "(10 9 27 0)");
    // ?\<space> is the space character (32).
    assert_eq!(eval("(equal ?\\  32)"), "t");
}

#[test]
fn dotted_pair_syntax() {
    assert_eq!(eval("'(1 . 2)"), "(1 . 2)");
    // A right-nested dotted chain reads as a proper list.
    assert_eq!(eval("(quote (a . (b . (c . nil))))"), "(a b c)");
}

#[test]
fn quote_and_function_quote() {
    assert_eq!(eval("(quote nil)"), "nil");
    assert_eq!(eval("()"), "nil");
    // #'sym is reader sugar for (function sym); funcall must accept it.
    assert_eq!(eval("(funcall #'+ 1 2 3)"), "6");
    assert_eq!(
        eval("(progn (defun dbl (x) (* x 2)) (mapcar #'dbl '(1 2 3)))"),
        "(2 4 6)"
    );
}

#[test]
fn keyword_symbols_self_evaluate() {
    assert_eq!(eval(":kw"), ":kw");
    // The leading colon is part of the symbol name.
    assert_eq!(eval("(symbol-name :kw)"), "\":kw\"");
}

#[test]
fn numeric_classification() {
    assert_eq!(eval("3.14"), "3.14");
    assert_eq!(eval("-2.5e2"), "-250.0");
    assert_eq!(eval("(list 1 -2 +3)"), "(1 -2 3)");
    // A float anywhere in an arithmetic chain contaminates the result type.
    assert_eq!(eval("(+ 1.5 2)"), "3.5");
}

#[test]
fn string_escapes() {
    // "a\nb" is three characters: the newline counts as one.
    assert_eq!(eval("(length \"a\\nb\")"), "3");
}

#[test]
fn backquote_with_unquote_and_splice() {
    assert_eq!(
        eval("(let ((x 3)) `(a ,(+ x 1) ,@(list 5 6) b))"),
        "(a 4 5 6 b)"
    );
}
