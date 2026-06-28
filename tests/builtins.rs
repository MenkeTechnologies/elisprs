//! Builtin-function coverage: the primitives registered in `builtins.rs` that
//! the broad `eval.rs` smoke test does not exercise. Each case is an end-to-end
//! lowering to fusevm; expectations were captured from the running interpreter,
//! not assumed.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn integer_division_and_modulo() {
    assert_eq!(eval("(% 17 5)"), "2");
    assert_eq!(eval("(% 20 4)"), "0");
    // mod follows the sign of the divisor (Emacs semantics), unlike %.
    assert_eq!(eval("(mod -7 3)"), "2");
    assert_eq!(eval("(mod 7 3)"), "1");
}

#[test]
fn numeric_comparison_chain() {
    assert_eq!(
        eval("(list (= 1 1) (< 1 2) (> 2 1) (<= 2 2) (>= 3 4))"),
        "(t t t t nil)"
    );
    assert_eq!(eval("(/= 1 2)"), "t");
    assert_eq!(eval("(/= 2 2)"), "nil");
    // Chained comparison: every adjacent pair must satisfy the predicate.
    assert_eq!(eval("(< 1 2 3 4)"), "t");
    assert_eq!(eval("(< 1 2 2 4)"), "nil");
}

#[test]
fn number_predicates() {
    assert_eq!(
        eval("(list (zerop 0) (evenp 4) (oddp 3) (natnump 0) (wholenump -1))"),
        "(t t t t nil)"
    );
    assert_eq!(
        eval("(list (plusp 1) (minusp -1) (cl-plusp 0))"),
        "(t t nil)"
    );
}

#[test]
fn min_max_abs() {
    assert_eq!(eval("(min 3 7 2 9)"), "2");
    assert_eq!(eval("(max 3 7 2 9)"), "9");
    assert_eq!(eval("(min 5)"), "5");
    assert_eq!(eval("(max 5)"), "5");
    assert_eq!(eval("(abs -8)"), "8");
    assert_eq!(eval("(abs 8)"), "8");
}

#[test]
fn symbol_plumbing() {
    assert_eq!(eval("(symbol-name 'foo)"), "\"foo\"");
    // intern is idempotent: two interns of the same name are eq.
    assert_eq!(eval("(eq (intern \"a\") (intern \"a\"))"), "t");
    // set / symbol-value round-trip through the global binding.
    assert_eq!(eval("(progn (set 'gv 99) (symbol-value 'gv))"), "99");
}

#[test]
fn make_symbol_is_uninterned() {
    // make-symbol allocates a *fresh* symbol each call (not in the obarray), so
    // two same-named results are distinct objects — unlike intern.
    assert_eq!(eval("(eq (make-symbol \"g\") (make-symbol \"g\"))"), "nil");
    assert_eq!(eval("(symbolp (make-symbol \"g\"))"), "t");
    assert_eq!(eval("(symbol-name (make-symbol \"g\"))"), "\"g\"");
}

#[test]
fn sort_by_predicate() {
    assert_eq!(eval("(sort (list 3 1 2 5 4) #'<)"), "(1 2 3 4 5)");
    assert_eq!(eval("(sort (list 3 1 2 5 4) #'>)"), "(5 4 3 2 1)");
    // ties keep input order (stable) and a vector sorts to a vector.
    assert_eq!(eval("(sort (list 3 1 2 1 3) #'<)"), "(1 1 2 3 3)");
    assert_eq!(eval("(sort (vector 5 3 1 4 2) #'<)"), "[1 2 3 4 5]");
}

#[test]
fn prin1_to_string_roundtrip() {
    assert_eq!(
        eval("(prin1-to-string '(1 \"two\" 3))"),
        "\"(1 \\\"two\\\" 3)\""
    );
    assert_eq!(eval("(prin1-to-string 42)"), "\"42\"");
}

#[test]
fn cons_mutation_setcdr() {
    assert_eq!(eval("(let ((p (cons 1 2))) (setcdr p 9) p)"), "(1 . 9)");
    assert_eq!(eval("(setcdr (cons 1 2) 9)"), "9"); // setcdr returns the new cdr
}

#[test]
fn char_and_string_conversions() {
    assert_eq!(eval("(char-to-string ?z)"), "\"z\"");
    assert_eq!(eval("(string-to-char \"abc\")"), "97");
    assert_eq!(eval("(string 104 105)"), "\"hi\"");
    assert_eq!(eval("(string-to-list \"abc\")"), "(97 98 99)");
}

#[test]
fn string_predicates_and_search() {
    assert_eq!(eval("(string-suffix-p \"bar\" \"foobar\")"), "t");
    assert_eq!(eval("(string-suffix-p \"baz\" \"foobar\")"), "nil");
    assert_eq!(eval("(string-empty-p \"\")"), "t");
    assert_eq!(eval("(string-empty-p \"x\")"), "nil");
    assert_eq!(eval("(string-search \"lo\" \"hello\")"), "3");
    assert_eq!(eval("(string-search \"xy\" \"abc\")"), "nil"); // not found -> nil
}

#[test]
fn number_to_string() {
    assert_eq!(eval("(number-to-string 42)"), "\"42\"");
    assert_eq!(eval("(number-to-string -7)"), "\"-7\"");
}

#[test]
fn format_directives() {
    assert_eq!(
        eval("(format \"%s|%S\" \"hi\" \"hi\")"),
        "\"hi|\\\"hi\\\"\""
    );
    assert_eq!(eval("(format \"%c\" 65)"), "\"A\"");
    assert_eq!(eval("(format \"%d%%\" 50)"), "\"50%\"");
    assert_eq!(eval("(format \"%s\" nil)"), "\"nil\"");
    assert_eq!(eval("(format \"%s\" t)"), "\"t\"");
    // %S of a list yields its read syntax.
    assert_eq!(eval("(format \"%S\" '(a b))"), "\"(a b)\"");
}

#[test]
fn format_width_flags_and_radix() {
    // width, left-justify (-), and zero-pad (0) flags.
    assert_eq!(
        eval("(format \"%5d|%-5d|%05d\" 42 42 42)"),
        "\"   42|42   |00042\""
    );
    // zero-pad keeps the sign in front.
    assert_eq!(eval("(format \"%05d\" -42)"), "\"-0042\"");
    // octal and upper/lower hex.
    assert_eq!(eval("(format \"%o %x %X\" 8 255 255)"), "\"10 ff FF\"");
    // float precision and field width.
    assert_eq!(eval("(format \"%.2f\" 3.14159)"), "\"3.14\"");
    assert_eq!(eval("(format \"[%8.2f]\" 3.14159)"), "\"[    3.14]\"");
}

#[test]
fn append_accepts_vectors_and_strings() {
    // append flattens vectors and strings (chars → ints), not just lists.
    assert_eq!(eval("(append [1 2 3] nil)"), "(1 2 3)");
    assert_eq!(eval("(append [1 2] '(3 4))"), "(1 2 3 4)");
    assert_eq!(eval("(append \"ab\" nil)"), "(97 98)");
}

#[test]
fn hash_table_lifecycle() {
    assert_eq!(
        eval(
            "(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) (hash-table-count h))"
        ),
        "2"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 1 h) (remhash 'a h) (hash-table-count h))"),
        "0"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 1 h) (clrhash h) (hash-table-count h))"),
        "0"
    );
    assert_eq!(eval("(hash-table-p (make-hash-table))"), "t");
    assert_eq!(eval("(hash-table-p '(1 2))"), "nil");
    // copy-hash-table is a deep enough copy to carry entries.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) (puthash \"k\" 5 h) (gethash \"k\" (copy-hash-table h)))"),
        "5"
    );
}

#[test]
fn error_and_signal_are_catchable() {
    // error formats its message; condition-case binds the error object whose
    // cadr is the formatted string.
    assert_eq!(
        eval("(condition-case e (error \"boom %d\" 7) (error (cadr e)))"),
        "\"boom 7\""
    );
    // signal dispatches to the matching condition handler by symbol.
    assert_eq!(
        eval("(condition-case nil (signal 'my-err '(1 2)) (my-err 'caught))"),
        "caught"
    );
    // user-error is catchable as a plain error too.
    assert_eq!(
        eval("(condition-case nil (user-error \"nope\") (error 'handled))"),
        "handled"
    );
}

#[test]
fn identity_and_ignore() {
    assert_eq!(eval("(identity 42)"), "42");
    assert_eq!(eval("(ignore 1 2 3)"), "nil");
    assert_eq!(eval("(always)"), "t");
}
