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
fn dot_reader_syntax_matches_emacs() {
    // A lone separator `.` is invalid read syntax wherever it cannot form a
    // dotted pair — top level, a vector element, or a list with no preceding car.
    // (emacs 30.2: all signal `(invalid-read-syntax ".")`.)
    assert_eq!(
        eval("(condition-case e (read \".\") (error e))"),
        "(invalid-read-syntax \".\")"
    );
    assert_eq!(
        eval("(condition-case e (read \"(. a)\") (error e))"),
        "(invalid-read-syntax \".\")"
    );
    assert_eq!(
        eval("(condition-case e (read \"[a . b]\") (error e))"),
        "(invalid-read-syntax \".\")"
    );
    // A missing cdr (`(a . )`) or a second dot / extra form before the close.
    assert_eq!(
        eval("(condition-case e (read \"(a . )\") (error e))"),
        "(invalid-read-syntax \")\")"
    );
    assert_eq!(
        eval("(condition-case e (read \"(a . b . c)\") (error e))"),
        "(invalid-read-syntax \"expected )\")"
    );
    // A `.` immediately before `)` is the symbol `\\.`, not a separator (emacs
    // `(a .)` => `(a \\.)`, `(.)` => `(\\.)`).
    assert_eq!(eval("(read \"(a .)\")"), "(a \\.)");
    assert_eq!(eval("(read \"(.)\")"), "(\\.)");
    // Valid dotted pairs are unaffected.
    assert_eq!(eval("(read \"(1 . 2)\")"), "(1 . 2)");
    assert_eq!(eval("(read \"(1 2 . 3)\")"), "(1 2 . 3)");
    // `.5` is a float, `...` is a symbol, `a.b` a symbol — never a bare dot.
    assert_eq!(eval("(read \".5\")"), "0.5");
    assert_eq!(eval("(symbol-name (read \"...\"))"), "\"...\"");
    assert_eq!(eval("(read \"(a.b c)\")"), "(a.b c)");
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
fn vector_literals() {
    // [..] reads as a self-evaluating vector; elements are read verbatim,
    // not evaluated.
    assert_eq!(eval("[1 2 3]"), "[1 2 3]");
    assert_eq!(eval("[(+ 1 2) foo \"s\"]"), "[(+ 1 2) foo \"s\"]");
    assert_eq!(eval("(aref [10 20 30] 1)"), "20");
    assert_eq!(eval("(length [1 2 3 4])"), "4");
    assert_eq!(eval("(equal [1 2 3] (vector 1 2 3))"), "t");
    assert_eq!(eval("[]"), "[]");
}

#[test]
fn backquote_with_unquote_and_splice() {
    assert_eq!(
        eval("(let ((x 3)) `(a ,(+ x 1) ,@(list 5 6) b))"),
        "(a 4 5 6 b)"
    );
}
