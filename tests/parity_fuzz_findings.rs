//! Parity gaps found by the differential fuzzer (`scripts/fuzz_parity.sh`).
//!
//! Each expectation here is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch --eval '(prin1 EXPR)'` — not of the running interpreter.
//! These are the cases the fuzzer surfaced that were NOT bignum-related; the
//! bignum ones live in `tests/bignums.rs`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Emacs prints a float as the shortest string that reads back as the same float,
/// laid out like C's `%g` — so it goes exponential once the decimal exponent
/// reaches the precision, and a subnormal collapses to one significant digit.
#[test]
fn floats_print_like_emacs() {
    assert_eq!(
        eval("(float most-positive-fixnum)"),
        "2.305843009213694e+18"
    );
    assert_eq!(eval("1e10"), "10000000000.0");
    assert_eq!(eval("1e-10"), "1e-10");
    assert_eq!(eval("1e300"), "1e+300");
    assert_eq!(eval("100.0"), "100.0");
    assert_eq!(eval("0.1"), "0.1");
    assert_eq!(eval("-0.0"), "-0.0");
    assert_eq!(eval("(/ 1.0 3)"), "0.3333333333333333");
    // Subnormals: gnulib's ftoastr starts its search at one significant digit.
    assert_eq!(eval("(ldexp 1.0 -1074)"), "5e-324");
    assert_eq!(
        eval("(list 1.0e+INF -1.0e+INF 0.0e+NaN)"),
        "(1.0e+INF -1.0e+INF 0.0e+NaN)"
    );
}

/// `match-data` reports only up to the last group that participated: a trailing
/// optional group that did not match contributes no `nil nil` pair.
#[test]
fn match_data_drops_trailing_unmatched_groups() {
    assert_eq!(
        eval("(progn (string-match \"\\\\(a+\\\\)\\\\(b\\\\)?\" \"xaaa\") (match-data))"),
        "(1 4 1 4)"
    );
    // A group that matched in the middle still reports its nil placeholder.
    assert_eq!(
        eval("(progn (string-match \"\\\\(x\\\\)?\\\\(a\\\\)\" \"a\") (match-data))"),
        "(0 1 nil nil 0 1)"
    );
}

/// The regexp errors carry Emacs's own wording, because elisp code catches
/// `invalid-regexp` and prints the message. Emacs also *tolerates* a repetition
/// operator with nothing to repeat and a reversed range.
#[test]
fn regexp_diagnostics_match_emacs() {
    let err = |re: &str| {
        eval(&format!(
            "(condition-case e (string-match {re} \"x\") (error e))"
        ))
    };
    assert_eq!(err(r#""[""#), r#"(invalid-regexp "Unmatched [ or [^")"#);
    assert_eq!(err(r#""\\(""#), r#"(invalid-regexp "Unmatched ( or \\(")"#);
    assert_eq!(err(r#""\\)""#), r#"(invalid-regexp "Unmatched ) or \\)")"#);
    assert_eq!(
        err(r#""a\\{2,1\\}""#),
        r#"(invalid-regexp "Invalid content of \\{\\}")"#
    );
    assert_eq!(err(r#""a\\{""#), r#"(invalid-regexp "Unmatched \\{")"#);
    assert_eq!(err(r#""\\""#), r#"(invalid-regexp "Trailing backslash")"#);
    // Not errors in Emacs: a leading `*` is a literal, `[z-a]` just never matches.
    assert_eq!(eval(r#"(string-match "*x" "*x")"#), "0");
    assert_eq!(eval(r#"(string-match "[z-a]" "x")"#), "nil");
}

/// `print-escape-control-characters` renders every remaining control character as
/// a backslash + octal escape.
#[test]
fn print_escape_control_characters() {
    assert_eq!(
        eval("(let ((print-escape-control-characters t)) (prin1-to-string \"a\\tb\"))"),
        r#""\"a\\11b\"""#
    );
    assert_eq!(
        eval("(let ((print-escape-control-characters t) (print-escape-newlines t)) (prin1-to-string \"a\\n\\tb\"))"),
        r#""\"a\\n\\11b\"""#
    );
}

/// Emacs's error *data* names the offending value and the predicate the specific
/// builtin checks — the two are not interchangeable, and a bare predicate with no
/// value is not what elisp code catches.
#[test]
fn error_data_carries_predicate_and_value() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // Bit *logic* takes a marker; the shifts and lognot/logcount do not.
    assert_eq!(
        e("(logand 1.0 2)"),
        "(wrong-type-argument integer-or-marker-p 1.0)"
    );
    assert_eq!(e("(ash 1.0 2)"), "(wrong-type-argument integerp 1.0)");
    assert_eq!(e("(lognot 3.0)"), "(wrong-type-argument integerp 3.0)");
    // The value, not a bare predicate, and never a raw heap handle.
    assert_eq!(
        e("(downcase nil)"),
        "(wrong-type-argument char-or-string-p nil)"
    );
    assert_eq!(e("(concat 45)"), "(wrong-type-argument sequencep 45)");
    assert_eq!(
        e("(string-join (list 1 2))"),
        "(wrong-type-argument sequencep 1)"
    );
    assert_eq!(
        e("(string= \"a\" 1.5)"),
        "(wrong-type-argument stringp 1.5)"
    );
    assert_eq!(e("(remq 1 t)"), "(wrong-type-argument listp t)");
    assert_eq!(
        e("(delq 1 (cons t 2))"),
        "(wrong-type-argument listp (t . 2))"
    );
    // The callee and the argument count.
    assert_eq!(
        e("(char-to-string)"),
        "(wrong-number-of-arguments char-to-string 0)"
    );
}

/// Sequence functions that Emacs defines in Lisp inherit that definition's
/// tolerances: `string-suffix-p` length-tests before it type-checks, `nconc`'s
/// last argument may be any object, `string-to-vector` takes any sequence.
#[test]
fn sequence_functions_match_their_lisp_definitions() {
    // `(- (length STRING) (length SUFFIX))` is negative → nil, no type check.
    assert_eq!(eval("(string-suffix-p \"aAbB\" [1 2])"), "nil");
    assert_eq!(eval("(nconc (list 1) (cons 2 \"s\"))"), "(1 2 . \"s\")");
    assert_eq!(eval("(string-to-vector nil)"), "[]");
    assert_eq!(eval("(string-to-list [1 2])"), "(1 2)");
    // seq-union dedups WITHIN the first sequence too, not just across the two.
    assert_eq!(eval("(seq-union (list 1 1 2) (list 2 3))"), "(1 2 3)");
    assert_eq!(eval("(seq-union \"line\" \"\")"), "(108 105 110 101)");
}

/// The string functions Emacs defines in Lisp (subr.el / subr-x.el) inherit
/// that definition's *check order*: `string-prefix-p`/`string-suffix-p` take
/// both `length`s before `compare-strings` sees a string, `string-join` is
/// `mapconcat` (whose `length` call names an improper list's tail), and
/// `string-remove-prefix` returns STRING untouched when the prefix cannot fit.
#[test]
fn string_fns_check_like_their_lisp_definitions() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // (length PREFIX) first: the prefix is the named non-sequence.
    assert_eq!(
        e("(string-prefix-p 97 \"ab\")"),
        "(wrong-type-argument sequencep 97)"
    );
    assert_eq!(
        e("(string-prefix-p \"str\" 1.5)"),
        "(wrong-type-argument sequencep 1.5)"
    );
    // A too-long PREFIX answers nil before any string check.
    assert_eq!(eval("(string-prefix-p \"hello world\" [1 2])"), "nil");
    // Both fit → compare-strings signals stringp, PREFIX first.
    assert_eq!(
        e("(string-prefix-p \"ab\" [97 98])"),
        "(wrong-type-argument stringp [97 98])"
    );
    assert_eq!(
        e("(string-prefix-p \"\" nil)"),
        "(wrong-type-argument stringp nil)"
    );
    // string-suffix-p takes (length STRING) first — 0 is named, not 97.
    assert_eq!(
        e("(string-suffix-p 97 0)"),
        "(wrong-type-argument sequencep 0)"
    );
    assert_eq!(
        e("(string-suffix-p 'sym \"hello world\")"),
        "(wrong-type-argument sequencep sym)"
    );
    // string-join = (mapconcat #'identity STRINGS SEP): length first, so an
    // improper list names its tail; a proper list's elements go through concat.
    assert_eq!(
        e("(string-join (cons '- 9) 1.5)"),
        "(wrong-type-argument listp 9)"
    );
    assert_eq!(e("(string-join '(-))"), "(wrong-type-argument sequencep -)");
    // string-remove-prefix riding string-prefix-p's nil: nil comes back as-is.
    assert_eq!(eval("(string-remove-prefix \"Hello, World\" nil)"), "nil");
    assert_eq!(
        e("(string-remove-prefix -1 \"-4.5\")"),
        "(wrong-type-argument sequencep -1)"
    );
    assert_eq!(
        e("(string-remove-suffix 'sym \"quoted\")"),
        "(wrong-type-argument sequencep sym)"
    );
    // string-empty-p is (string= STRING "") — a symbol compares by name.
    assert_eq!(eval("(string-empty-p \"\")"), "t");
    assert_eq!(eval("(string-empty-p nil)"), "nil");
    assert_eq!(e("(string-empty-p 5)"), "(wrong-type-argument stringp 5)");
    // string-equal-ignore-case is compare-strings underneath: strings only.
    assert_eq!(
        e("(string-equal-ignore-case nil \"x\")"),
        "(wrong-type-argument stringp nil)"
    );
    assert_eq!(eval("(string-equal-ignore-case \"aB\" \"Ab\")"), "t");
    // string-lessp takes a string or symbol, first argument checked first.
    assert_eq!(e("(string< 97 \"aa\")"), "(wrong-type-argument stringp 97)");
    assert_eq!(
        e("(string< \"a,b,,c\" [1 2])"),
        "(wrong-type-argument stringp [1 2])"
    );
    assert_eq!(eval("(string-lessp 'abc \"abd\")"), "t");
    // string> reverses the arguments into string-lessp, so the SECOND is
    // checked first.
    assert_eq!(
        e("(string> \"-4.5\" 1.5)"),
        "(wrong-type-argument stringp 1.5)"
    );
}

/// A character argument to the case functions: negative is not a character
/// (`char-or-string-p`), but one above the character range comes back
/// UNCHANGED (Emacs treats the high bits as event modifiers).
#[test]
fn case_functions_check_character_range() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(upcase -1)"),
        "(wrong-type-argument char-or-string-p -1)"
    );
    assert_eq!(
        e("(capitalize -1)"),
        "(wrong-type-argument char-or-string-p -1)"
    );
    assert_eq!(
        e("(downcase -1)"),
        "(wrong-type-argument char-or-string-p -1)"
    );
    assert_eq!(eval("(upcase 4194304)"), "4194304");
    // upcase-initials words are cased letters, not just ASCII.
    assert_eq!(eval("(upcase-initials \"αβγ\")"), "\"Αβγ\"");
    assert_eq!(
        eval("(upcase-initials \"αβγline\\nbreaka,b,,c\")"),
        "\"Αβγline
Breaka,B,,C\""
    );
}

/// `replace-regexp-in-string` is a Lisp function (subr.el), and its whole
/// contract falls out of that definition: `(length STRING)` first, REGEXP
/// type-checked only when the loop runs, REP untouched until a match happens
/// (it may be a function), nil STRING reaching `substring` (`arrayp`), and
/// `(< start l)` cutting the empty-match loop before end-of-string.
#[test]
fn replace_regexp_in_string_matches_lisp_definition() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(replace-regexp-in-string \"\\\\.\" \"x\" 'car)"),
        "(wrong-type-argument sequencep car)"
    );
    assert_eq!(
        e("(replace-regexp-in-string \"abc\" -1 t)"),
        "(wrong-type-argument sequencep t)"
    );
    assert_eq!(
        e("(replace-regexp-in-string 'sym [1 2] \"a\")"),
        "(wrong-type-argument stringp sym)"
    );
    // l = 0 skips the loop entirely: the invalid regexp is never compiled, and
    // nil reaches the leftover (substring nil 0 0).
    assert_eq!(
        e("(replace-regexp-in-string \"\\\\(\" \"x\" nil t)"),
        "(wrong-type-argument arrayp nil)"
    );
    // No match → REP is never examined, even when it could never be applied.
    assert_eq!(
        eval("(replace-regexp-in-string \"123\" [1 2] \"line\\nbreak\")"),
        "\"line
break\""
    );
    // The empty regexp replaces before every char but NOT at end-of-string.
    assert_eq!(
        eval("(replace-regexp-in-string \"\" \"t\" \"-4.5\")"),
        "\"t-t4t.t5\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"\" \"a,b,,c\" \"A\")"),
        "\"a,b,,cA\""
    );
    // Function REP still works, fed the matched text.
    assert_eq!(
        eval("(replace-regexp-in-string \"[0-9]+\" (lambda (m) (number-to-string (* 2 (string-to-number m)))) \"a5b10\")"),
        "\"a10b20\""
    );
}

/// String matching folds case under `case-fold-search` (default t in batch) —
/// including `split-string`'s separator and a backreference's comparison.
#[test]
fn case_fold_search_applies_to_string_matching() {
    assert_eq!(eval("(split-string \"HELLO\" \"hello\")"), "(\"\" \"\")");
    assert_eq!(
        eval("(let ((case-fold-search nil)) (split-string \"HELLO\" \"hello\"))"),
        "(\"HELLO\")"
    );
    // \1 must match "A" against the captured "a" when folding.
    assert_eq!(eval("(string-match \"\\\\(a\\\\)\\\\1\" \"aA\")"), "0");
    assert_eq!(
        eval("(string-trim \"aAbB\" \"\\\\(a\\\\)\\\\1\" \"a+\")"),
        "\"bB\""
    );
}

/// `substring` checks the array before the indices; `substring-no-properties`
/// insists on a string. `split-string` type-checks its separator regexp before
/// the string, as `string-match` would.
#[test]
fn substring_and_split_string_check_order() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(substring -1 1.5)"), "(wrong-type-argument arrayp -1)");
    assert_eq!(
        e("(substring \"abc\" 1.5)"),
        "(wrong-type-argument integerp 1.5)"
    );
    assert_eq!(
        e("(substring -1 (abs -2305843009213693952))"),
        "(wrong-type-argument arrayp -1)"
    );
    assert_eq!(
        e("(substring-no-properties 97)"),
        "(wrong-type-argument stringp 97)"
    );
    assert_eq!(
        e("(split-string [1 2] 97)"),
        "(wrong-type-argument stringp 97)"
    );
}

/// GNU regex diagnoses `\{` content it can never accept as "Invalid content"
/// even when the pattern then ends; only running out of pattern while the
/// interval is still well-formed is "Unmatched \{". An empty interval is valid.
#[test]
fn brace_interval_diagnostics_split_like_gnu_regex() {
    let err = |re: &str| {
        eval(&format!(
            "(condition-case e (string-match {re} \"x\") (error e))"
        ))
    };
    assert_eq!(err(r#""a\\{2,""#), r#"(invalid-regexp "Unmatched \\{")"#);
    assert_eq!(
        err(r#""a\\{x""#),
        r#"(invalid-regexp "Invalid content of \\{\\}")"#
    );
    assert_eq!(
        err(r#""a\\{2\\)""#),
        r#"(invalid-regexp "Invalid content of \\{\\}")"#
    );
    assert_eq!(
        err(r#""\\`\\(?:a\\{\\)""#),
        r#"(invalid-regexp "Invalid content of \\{\\}")"#
    );
    assert_eq!(eval(r#"(string-match "a\\{\\}" "b")"#), "0");
    assert_eq!(eval(r#"(string-match "a\\{,2\\}b" "ab")"#), "0");
}

/// `%d` renders a non-finite float as the word "nan"/"inf"/"-inf" — space-padded
/// to width (the `0` flag is ignored), the `+` flag applying to infinities but
/// never to NaN — while a finite float truncates toward zero at full precision.
/// The unsigned radix conversions (`%o`/`%x`/`%X`) instead signal
/// `overflow-error`, and `%c` rejects any float outright.
#[test]
fn format_integer_directives_on_floats() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(eval(r#"(format "%d%%" 0.0e+NaN)"#), r#""nan%""#);
    assert_eq!(eval(r#"(format "%d%%" -1.0e+INF)"#), r#""-inf%""#);
    assert_eq!(eval(r#"(format "%5d" 0.0e+NaN)"#), r#""  nan""#);
    assert_eq!(eval(r#"(format "%05d" 0.0e+NaN)"#), r#""  nan""#);
    assert_eq!(eval(r#"(format "%-6d" 1.0e+INF)"#), r#""inf   ""#);
    assert_eq!(eval(r#"(format "%+d" 1.0e+INF)"#), r#""+inf""#);
    assert_eq!(eval(r#"(format "%+d" -1.0e+INF)"#), r#""-inf""#);
    assert_eq!(eval(r#"(format "%+d" 0.0e+NaN)"#), r#""nan""#);
    // Finite floats truncate toward zero — exactly, even beyond fixnum range.
    assert_eq!(eval(r#"(format "%d" 1.5)"#), r#""1""#);
    assert_eq!(eval(r#"(format "%d" -1.5)"#), r#""-1""#);
    assert_eq!(
        eval(r#"(format "%d" 1.0e30)"#),
        r#""1000000000000000019884624838656""#
    );
    assert_eq!(e(r#"(format "%x" 1.0e+INF)"#), "(overflow-error)");
    assert_eq!(e(r#"(format "%o" 0.0e+NaN)"#), "(overflow-error)");
    assert_eq!(eval(r#"(format "%x" 1.5)"#), r#""1""#);
    assert_eq!(
        e(r#"(format "%c" 65.0)"#),
        "(error \"Format specifier doesn\u{2019}t match argument type\")"
    );
}

/// `make-symbol`/`intern-soft` name the offending non-string in their
/// `stringp` error data (Emacs `CHECK_STRING`); a symbol argument to
/// `intern-soft` is looked up by its own name.
#[test]
fn symbol_constructors_name_the_offender() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(make-symbol 1.5)"), "(wrong-type-argument stringp 1.5)");
    assert_eq!(e("(make-symbol -1)"), "(wrong-type-argument stringp -1)");
    assert_eq!(e("(intern-soft -1)"), "(wrong-type-argument stringp -1)");
    assert_eq!(
        e("(intern-soft (list 1 2))"),
        "(wrong-type-argument stringp (1 2))"
    );
    assert_eq!(eval("(intern-soft 'car)"), "car");
}

/// A symbol whose name reads back as a number — including the non-finite float
/// syntax `0.0e+NaN`/`1.0e+INF` and a bignum-sized integer — prints with a
/// leading `\` so it stays a symbol.
#[test]
fn number_lookalike_symbols_print_escaped() {
    assert_eq!(
        eval(r#"(prin1-to-string (intern "0.0e+NaN"))"#),
        r#""\\0.0e+NaN""#
    );
    assert_eq!(
        eval(r#"(prin1-to-string (intern "1.0e+INF"))"#),
        r#""\\1.0e+INF""#
    );
    assert_eq!(
        eval(r#"(prin1-to-string (intern "12345678901234567890123"))"#),
        r#""\\12345678901234567890123""#
    );
    assert_eq!(eval(r#"(prin1-to-string (intern "1."))"#), r#""\\1.""#);
    // `sqrt` of a negative yields a NaN whose sign bit is implementation-defined:
    // x86-64 `sqrtsd` returns a negative NaN, aarch64 `fsqrt` a positive one, and
    // GNU Emacs (libm `sqrt`) exhibits the same per-platform split. So accept
    // either sign — the point of this finding is that the NaN string interns to a
    // symbol that prints escaped, not which sign the host FPU produced.
    let nan_sym = eval("(intern (number-to-string (sqrt -1.5)))");
    assert!(
        nan_sym == "\\0.0e+NaN" || nan_sym == "\\-0.0e+NaN",
        "expected an escaped NaN symbol, got {nan_sym}"
    );
}

/// The `f*` float-rounding subrs demand exactly a float (`CHECK_FLOAT`): an
/// integer draws the same `floatp` signal a non-number does.
#[test]
fn float_rounding_subrs_demand_floats() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(fround 0)"), "(wrong-type-argument floatp 0)");
    assert_eq!(e("(ftruncate nil)"), "(wrong-type-argument floatp nil)");
    assert_eq!(e("(fround 'sym)"), "(wrong-type-argument floatp sym)");
    assert_eq!(
        e("(fround \"str\")"),
        "(wrong-type-argument floatp \"str\")"
    );
    assert_eq!(e("(fround [1 2])"), "(wrong-type-argument floatp [1 2])");
    assert_eq!(
        e("(ffloor (list 1 2))"),
        "(wrong-type-argument floatp (1 2))"
    );
    assert_eq!(e("(fceiling \"\")"), "(wrong-type-argument floatp \"\")");
    assert_eq!(eval("(fround 1.5)"), "2.0");
    assert_eq!(eval("(ffloor 1.5)"), "1.0");
}

/// Unary `-` on -0.0 is float negation: `(- -0.0)` is 0.0, never the integer 0
/// (and never -0.0, which `(+ -0.0)` and `(- -0.0 0)` keep).
#[test]
fn unary_minus_negates_signed_zero() {
    assert_eq!(eval("(- -0.0)"), "0.0");
    assert_eq!(eval("(- 0.0)"), "-0.0");
    assert_eq!(eval("(+ -0.0)"), "-0.0");
    assert_eq!(eval("(- -0.0 0)"), "-0.0");
}

/// `symbolp` is t for nil however nil was produced — `(= 1 2)` returns the
/// same symbol nil that a literal does.
#[test]
fn symbolp_accepts_computed_nil() {
    assert_eq!(eval("(symbolp (= 1 2))"), "t");
    assert_eq!(eval("(symbolp (cl-evenp 7))"), "t");
    assert_eq!(eval("(symbolp nil)"), "t");
    assert_eq!(eval("(symbolp t)"), "t");
}

/// Error-object identity of the fuzzer's predicate pool: `natnump`/`nlistp` are
/// C subrs in Emacs (`#<subr natnump>` in error data), `not` is an alias of
/// `null` (so `#'not` IS `#<subr null>`), and byte-compiled `cl-evenp`/`cl-oddp`
/// report their arity as the `(MANDATORY . NONREST)` cons, never as a closure.
#[test]
fn predicate_error_object_identity() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(sort (list 1 2) #'natnump)"),
        "(wrong-number-of-arguments #<subr natnump> 2)"
    );
    assert_eq!(
        e("(cl-reduce #'nlistp '(car 2.5 t))"),
        "(wrong-number-of-arguments #<subr nlistp> 2)"
    );
    assert_eq!(
        e("(seq-sort #'not (make-string 2 9))"),
        "(wrong-number-of-arguments #<subr null> 2)"
    );
    assert_eq!(eval("(symbol-function 'not)"), "null");
    assert_eq!(
        e("(sort (list 1 2) #'cl-evenp)"),
        "(wrong-number-of-arguments (1 . 1) 2)"
    );
    assert_eq!(
        e("(apply #'cl-evenp (list))"),
        "(wrong-number-of-arguments (1 . 1) 0)"
    );
    assert_eq!(e("(cl-evenp 1 2)"), "(wrong-number-of-arguments (1 . 1) 2)");
    // A direct call to a subr by name still names the symbol, as Emacs does.
    assert_eq!(e("(natnump 1 2)"), "(wrong-number-of-arguments natnump 2)");
    // The predicates still predicate.
    assert_eq!(eval("(natnump 0)"), "t");
    assert_eq!(eval("(natnump -1)"), "nil");
    assert_eq!(eval("(natnump 1.0)"), "nil");
    assert_eq!(eval("(nlistp nil)"), "nil");
    assert_eq!(eval("(nlistp \"x\")"), "t");
    assert_eq!(eval("(cl-evenp 4)"), "t");
    assert_eq!(
        e("(cl-oddp 2.5)"),
        "(wrong-type-argument integer-or-marker-p 2.5)"
    );
}

/// The text-property fns validate OBJECT with `buffer-or-string-p`, and one
/// `propertize` call covering a range is ONE printed interval, even when the
/// plist's key is a string.
#[test]
fn text_property_object_checks_and_intervals() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(get-text-property 1 't 97)"),
        "(wrong-type-argument buffer-or-string-p 97)"
    );
    assert_eq!(
        e("(get-text-property 2 'a 'car)"),
        "(wrong-type-argument buffer-or-string-p car)"
    );
    assert_eq!(
        e("(text-properties-at 3 t)"),
        "(wrong-type-argument buffer-or-string-p t)"
    );
    assert_eq!(
        e("(text-properties-at 7 (list 1 2))"),
        "(wrong-type-argument buffer-or-string-p (1 2))"
    );
    assert_eq!(
        eval(
            r#"(prin1-to-string (propertize "  padded  " 'baz (propertize (make-string 6 97) "" (ash -2 3))))"#
        ),
        r##""#(\"  padded  \" 0 10 (baz #(\"aaaaaa\" 0 6 (\"\" -16))))""##
    );
}

/// The `assoc-string` walk keeps a separate DONE flag: a matched element may
/// itself be nil (key nil matches an element nil via `symbol-name`), and
/// looping on the result being non-nil spun forever, allocating until the
/// process died. Fuzz form #2745 (seed 424242).
#[test]
fn assoc_string_nil_key_terminates() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(assoc-string (functionp (and t \"aAbB\")) '(4611686018427387903 t nil))"),
        "nil"
    );
    // The symbol T still matches the string key "t" and is returned as itself.
    assert_eq!(e("(assoc-string \"t\" '(4611686018427387903 t nil))"), "t");
    assert_eq!(e("(assoc-string nil '(nil))"), "nil");
    assert_eq!(e("(assoc-string \"nil\" '(a nil b))"), "nil");
}

/// `concat` (and through it `mapconcat`) enforces Fconcat's contract: each arg
/// is a string, nil, or a list/vector of CHARACTERS; a list arg's structure is
/// validated before its elements; a non-char element is `characterp`; a
/// non-sequence is `sequencep`.
#[test]
fn concat_contract_is_fconcats() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(concat '(32 . 32))"), "(wrong-type-argument listp 32)");
    assert_eq!(e("(concat '(a a))"), "(wrong-type-argument characterp a)");
    assert_eq!(
        e("(concat '(\"x\"))"),
        "(wrong-type-argument characterp \"x\")"
    );
    // Structure before elements: the dotted tail wins over the bad car.
    assert_eq!(e("(concat '(a . 2))"), "(wrong-type-argument listp 2)");
    assert_eq!(e("(concat t)"), "(wrong-type-argument sequencep t)");
    assert_eq!(eval("(concat [97 98] nil \"c\")"), "\"abc\"");
    // vconcat shares the list walk but takes any element.
    assert_eq!(e("(vconcat '(t . 9))"), "(wrong-type-argument listp 9)");
    assert_eq!(eval("(vconcat '(t) \"a\")"), "[t 97]");
}

/// `mapconcat` is Fmapconcat: `length` validates SEQ first, FUNCTION runs over
/// every element before any concatenation, and results + separator go to
/// `concat` — so a cons result reports `listp`/`characterp`, a separator is
/// only reached with two or more elements, and an empty SEQ is "" untouched.
#[test]
fn mapconcat_maps_all_then_concats() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(mapconcat (lambda (x) (cons x x)) \"  padded  \")"),
        "(wrong-type-argument listp 32)"
    );
    assert_eq!(
        e("(mapconcat (lambda (x) (list x x)) (vector 'a '+))"),
        "(wrong-type-argument characterp a)"
    );
    // SEQ's own dotted tail is length's error, before FUNCTION ever runs.
    assert_eq!(
        e("(mapconcat #'identity (cons '- 9) 1.5)"),
        "(wrong-type-argument listp 9)"
    );
    // Mapping runs to completion first: the second element's error fires even
    // though the first result (t) could never be concatenated.
    assert_eq!(
        e("(mapconcat #'zerop (vector 0 'nil))"),
        "(wrong-type-argument number-or-marker-p nil)"
    );
    // A non-sequence separator only signals once two results flank it.
    assert_eq!(
        e("(mapconcat #'identity '(\"a\" \"b\") 32)"),
        "(wrong-type-argument sequencep 32)"
    );
    assert_eq!(eval("(mapconcat #'identity nil 'sep)"), "\"\"");
    assert_eq!(
        eval("(mapconcat (lambda (x) (list ?a ?b)) '(1 2))"),
        "\"abab\""
    );
}

/// `cl-sort` (cl-seq.el) accepts ANY sequence: a non-list is coerced with
/// (append SEQ nil) — whose failure is `sequencep`, unlike bare `sort`'s
/// list-or-vector-p — a string round-trips through vconcat/concat, and a
/// vector is sorted as a list and written back in place.
#[test]
fn cl_sort_accepts_all_sequences() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(cl-sort -1 #'1+)"), "(wrong-type-argument sequencep -1)");
    assert_eq!(
        e("(cl-sort 'car #'nlistp)"),
        "(wrong-type-argument sequencep car)"
    );
    assert_eq!(eval("(cl-sort \"bca\" #'<)"), "\"abc\"");
    assert_eq!(eval("(cl-sort [3 1 2] #'<)"), "[1 2 3]");
    assert_eq!(eval("(cl-sort [3 1 2] #'< :key #'-)"), "[3 2 1]");
    // `sort` itself keeps its narrower contract.
    assert_eq!(
        e("(sort \"str\" #'<)"),
        "(wrong-type-argument list-or-vector-p \"str\")"
    );
}

/// The seq.el fns inherit their index errors from what they defer to:
/// `seq-drop` on a list is nthcdr (`integerp`), elsewhere the generic (<= n 0)
/// (`number-or-marker-p`); `seq-subseq` on arrays is `substring` (`integerp`);
/// `seq-partition` with N < 1 is nil without touching SEQ (it used to loop
/// forever on zero progress); `butlast` with N <= 0 returns LIST unvalidated.
#[test]
fn seq_fns_defer_like_seq_el() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(
        e("(seq-drop (list 1e-10) (list 1 2))"),
        "(wrong-type-argument integerp (1 2))"
    );
    assert_eq!(
        e("(seq-drop \"abc\" 'x)"),
        "(wrong-type-argument number-or-marker-p x)"
    );
    assert_eq!(
        e("(seq-subseq \"\" 'car)"),
        "(wrong-type-argument integerp car)"
    );
    assert_eq!(
        e("(seq-subseq (vector nil) \"\")"),
        "(wrong-type-argument integerp \"\")"
    );
    assert_eq!(eval("(seq-partition 0 0)"), "nil");
    assert_eq!(eval("(seq-partition \"ab\" 0)"), "nil");
    assert_eq!(eval("(seq-partition \"abc\" 2)"), "(\"ab\" \"c\")");
    assert_eq!(eval("(butlast 0 0)"), "0");
    assert_eq!(eval("(butlast '(1 2) 0)"), "(1 2)");
    assert_eq!(eval("(butlast '(1 2 3) 2)"), "(1)");
    // The list branch of seq-subseq reports out-of-range as a plain `error'.
    assert_eq!(
        e("(seq-subseq '(1 2 3) 5)"),
        "(error \"Start index out of bounds: 5\")"
    );
    assert_eq!(eval("(seq-subseq '(1 2 3) 1)"), "(2 3)");
}

/// `aref` dispatches on the array's type BEFORE any bounds check — a
/// non-array names itself even with a negative index — while a real array
/// with a bad index is `args-out-of-range`. `rassq-delete-all` and
/// `assq-delete-all` hit `(car alist)` first, so a non-list is `listp`.
#[test]
fn aref_and_alist_delete_contracts() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(e("(aref 97 -7)"), "(wrong-type-argument arrayp 97)");
    assert_eq!(e("(aref 0 -1)"), "(wrong-type-argument arrayp 0)");
    assert_eq!(e("(aref \"ab\" -1)"), "(args-out-of-range \"ab\" -1)");
    assert_eq!(e("(aref 97 'x)"), "(wrong-type-argument fixnump x)");
    assert_eq!(
        e("(rassq-delete-all \"x\" -1)"),
        "(wrong-type-argument listp -1)"
    );
    assert_eq!(
        e("(assq-delete-all \"x\" -1)"),
        "(wrong-type-argument listp -1)"
    );
    assert_eq!(
        eval("(rassq-delete-all 2 (list (cons 'a 1) (cons 'b 2) (cons 'c 2)))"),
        "((a . 1))"
    );
    assert_eq!(
        eval("(assq-delete-all 'a (list (cons 'a 1) (cons 'b 2) (cons 'a 3)))"),
        "((b . 2))"
    );
}

/// Seed-555001 round: `plist-get` (Fplist_get, FOR_EACH_TAIL_SAFE) never
/// signals — it breaks on a non-cons cdr BEFORE testing the key — while
/// `plist-member`/`plist-put` end with CHECK_TYPE naming the WHOLE plist under
/// `plistp`. plist-member tests the key FIRST, so a present key on a dotted
/// pair still answers; plist-put breaks BEFORE its key test, so the same
/// shapes signal even when the key is present.
#[test]
fn plist_walks_signal_like_emacs_30() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // The original fuzz form.
    assert_eq!(eval("(plist-get (cons (float 42) t) (subrp t))"), "nil");
    assert_eq!(eval("(plist-get '(1 . 2) 1)"), "nil");
    assert_eq!(eval("(plist-get '(1) 1)"), "nil");
    assert_eq!(eval("(plist-get 5 1)"), "nil");
    assert_eq!(eval("(plist-get '(1 2 . 3) 1)"), "2");
    assert_eq!(eval("(plist-get '(1 2 . 3) 3)"), "nil");
    assert_eq!(eval("(plist-member '(1 . 2) 1)"), "(1 . 2)");
    assert_eq!(eval("(plist-member '(1) 1)"), "(1)");
    assert_eq!(eval("(plist-member '(1 2 3 . 4) 3)"), "(3 . 4)");
    assert_eq!(
        e("(plist-member '(1 . 2) 2)"),
        "(wrong-type-argument plistp (1 . 2))"
    );
    assert_eq!(
        e("(plist-member '(1 2 . 3) 99)"),
        "(wrong-type-argument plistp (1 2 . 3))"
    );
    assert_eq!(e("(plist-member 5 1)"), "(wrong-type-argument plistp 5)");
    assert_eq!(
        e("(plist-put '(1) 1 9)"),
        "(wrong-type-argument plistp (1))"
    );
    assert_eq!(
        e("(plist-put '(1 . 2) 1 9)"),
        "(wrong-type-argument plistp (1 . 2))"
    );
    assert_eq!(
        e("(plist-put '(1 2 . 3) 5 9)"),
        "(wrong-type-argument plistp (1 2 . 3))"
    );
    assert_eq!(e("(plist-put 5 1 2)"), "(wrong-type-argument plistp 5)");
    assert_eq!(eval("(plist-put nil 'a 1)"), "(a 1)");
    assert_eq!(eval("(plist-put '(1 2) 3 4)"), "(1 2 3 4)");
    assert_eq!(
        eval("(let ((pl (list 'a 1 'b 2))) (plist-put pl 'b 9) pl)"),
        "(a 1 b 9)"
    );
}

/// Emacs keeps ONE shared empty-string object (`empty_unibyte_string`,
/// alloc.c): every 0-length string construction returns it, so all empty
/// strings are `eq`. The observable fuzz hit: plist-put's default `eq` test
/// REPLACES under an equal-but-differently-constructed "" key instead of
/// appending a second pair.
#[test]
fn empty_strings_are_eq_singletons() {
    assert_eq!(eval("(eq \"\" \"\")"), "t");
    assert_eq!(eval("(eq (make-string 0 10) \"\")"), "t");
    assert_eq!(eval("(eq (substring \"abc\" 0 0) \"\")"), "t");
    assert_eq!(eval("(eq \"a\" \"a\")"), "nil");
    // The original fuzz form: the value is replaced, not appended.
    assert_eq!(
        eval("(plist-put '(\"\" -65536) \"\" '(nil nil nil nil nil nil))"),
        "(\"\" (nil nil nil nil nil nil))"
    );
    assert_eq!(eval("(plist-get '(\"\" 5) \"\")"), "5");
    // Nonempty equal literals stay distinct objects, so plist-put appends.
    assert_eq!(eval("(plist-put '(\"a\" 1) \"a\" 2)"), "(\"a\" 1 \"a\" 2)");
}

/// Fdelq/Fdelete rebind LIST past head removals before CHECK_LIST_END, so the
/// `listp` error data is the list AFTER any deletions, while `remq`
/// (subr.el) dies inside `memq` naming the WHOLE list, or inside
/// `copy-sequence` naming the TAIL when the element was found. `remove`
/// (subr.el) is delete-over-copy-sequence, and a non-sequence is `sequencep`
/// through delete, `listp` through delq.
#[test]
fn delete_family_error_data_matches_emacs() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // The original fuzz forms.
    assert_eq!(
        e("(remq \"x\" (cons \"αβγ\" 2305843009213693951))"),
        "(wrong-type-argument listp (\"αβγ\" . 2305843009213693951))"
    );
    assert_eq!(
        e("(delete \"str\" '(\"a\" . [1 2]))"),
        "(wrong-type-argument listp (\"a\" . [1 2]))"
    );
    assert_eq!(
        e("(delete \"a\" '(\"a\" . [1 2]))"),
        "(wrong-type-argument listp [1 2])"
    );
    assert_eq!(
        e("(remove \"str\" '(\"a\" . [1 2]))"),
        "(wrong-type-argument listp [1 2])"
    );
    assert_eq!(
        e("(delq 'x (cons 'a 5))"),
        "(wrong-type-argument listp (a . 5))"
    );
    assert_eq!(e("(delq 'a (cons 'a 5))"), "(wrong-type-argument listp 5)");
    assert_eq!(
        e("(delq 'a '(x a . 3))"),
        "(wrong-type-argument listp (x . 3))"
    );
    assert_eq!(e("(remq 'a '(x a . 3))"), "(wrong-type-argument listp 3)");
    assert_eq!(e("(delete 5 5)"), "(wrong-type-argument sequencep 5)");
    assert_eq!(e("(remove 5 5)"), "(wrong-type-argument sequencep 5)");
    assert_eq!(e("(delq 5 5)"), "(wrong-type-argument listp 5)");
    assert_eq!(e("(remq 5 5)"), "(wrong-type-argument listp 5)");
    // Working cases keep their shapes (delete copies arrays, delq splices).
    assert_eq!(eval("(delete 2 [1 2 3 2])"), "[1 3]");
    assert_eq!(eval("(delete ?a \"abca\")"), "\"bc\"");
    assert_eq!(eval("(remove ?a \"abca\")"), "\"bc\"");
    assert_eq!(eval("(remq 'a '(a b a))"), "(b)");
    assert_eq!(eval("(let ((l (list 1 2 1 3))) (delq 1 l) l)"), "(1 2 3)");
}

/// Emacs 30 sorts with tim_sort (src/sort.c, the CPython listsort port); for
/// arrays under MAX_MINRUN it is count_run + binary insertion, whose FIRST
/// predicate call is pred(a[1], a[0]). A throwing predicate therefore names
/// a[1] — 1.0 in the fuzz form — and a logging predicate observes the exact
/// binary-insertion probe sequence.
#[test]
fn sort_comparison_order_is_timsort() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // The original fuzz form (cl-sort passes through to sort after coercion).
    assert_eq!(
        e("(cl-sort '(t 1.0 nil) #'string-to-number)"),
        "(wrong-type-argument stringp 1.0)"
    );
    assert_eq!(
        e("(sort '(3 1 2) #'string-to-number)"),
        "(wrong-type-argument stringp 1)"
    );
    // Emacs 30.2 comparison logs: descending-run detection then binary probes.
    assert_eq!(
        eval(
            "(let ((log nil)) (sort (list 5 1 4 2 3) (lambda (a b) (push (cons a b) log) (< a b))) (reverse log))"
        ),
        "((1 . 5) (4 . 1) (4 . 5) (4 . 1) (2 . 4) (2 . 1) (3 . 4) (3 . 2))"
    );
    assert_eq!(
        eval(
            "(let ((log nil)) (sort (list 2 3 1 4) (lambda (a b) (push (cons a b) log) (< a b))) (reverse log))"
        ),
        "((3 . 2) (1 . 3) (1 . 3) (1 . 2) (4 . 2) (4 . 3))"
    );
    // An already-descending run costs exactly n-1 comparisons and one reversal.
    assert_eq!(
        eval(
            "(let ((log nil)) (list (sort (list 3 2 1) (lambda (a b) (push (cons a b) log) (< a b))) (reverse log)))"
        ),
        "((1 2 3) ((2 . 3) (1 . 2)))"
    );
    // Stability: equal keys keep encounter order.
    assert_eq!(
        eval(
            "(sort (list (cons 2 'a) (cons 1 'b) (cons 2 'c) (cons 1 'd)) (lambda (x y) (< (car x) (car y))))"
        ),
        "((1 . b) (1 . d) (2 . a) (2 . c))"
    );
}

/// `zerop` is a byte-compiled defsubst from subr.el in Emacs, NOT a C subr:
/// (subrp (symbol-function 'zerop)) is nil and a wrong argument count is
/// exec_byte_code's ((MANDATORY . NONREST) NARGS) shape, never #<subr zerop>.
#[test]
fn zerop_is_lisp_not_a_subr() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    assert_eq!(eval("(subrp (symbol-function 'zerop))"), "nil");
    // The original fuzz shape: seq-reduce hands (ACC ELT) to a 1-ary fn.
    assert_eq!(
        e("(seq-reduce #'zerop \"ab\" 0)"),
        "(wrong-number-of-arguments (1 . 1) 2)"
    );
    assert_eq!(e("(zerop)"), "(wrong-number-of-arguments (1 . 1) 0)");
    assert_eq!(e("(zerop 1 2)"), "(wrong-number-of-arguments (1 . 1) 2)");
    assert_eq!(
        e("(apply #'zerop '(1 2 3))"),
        "(wrong-number-of-arguments (1 . 1) 3)"
    );
    assert_eq!(eval("(zerop 0)"), "t");
    assert_eq!(eval("(zerop 0.0)"), "t");
    assert_eq!(eval("(zerop 1)"), "nil");
    assert_eq!(
        e("(zerop \"a\")"),
        "(wrong-type-argument number-or-marker-p \"a\")"
    );
}

/// subr-x's string-trim runs the RIGHT trim first —
/// (string-trim-left (string-trim-right S TRIM-RIGHT) TRIM-LEFT) — so with two
/// bad regexps the right one's compile error wins, and a bare "[" is
/// "Unmatched [ or [^".
#[test]
fn string_trim_trims_right_first() {
    let e = |src: &str| eval(&format!("(condition-case e {src} (error e))"));
    // The original fuzz form.
    assert_eq!(
        e("(string-trim (make-string 0 10) \"\\\\(\" \"[\")"),
        "(invalid-regexp \"Unmatched [ or [^\")"
    );
    assert_eq!(
        e("(string-trim \"\" \"[\" \"\\\\(\")"),
        "(invalid-regexp \"Unmatched ( or \\\\(\")"
    );
    assert_eq!(eval("(string-trim \" a \")"), "\"a\"");
    assert_eq!(eval("(string-trim \"xax\" \"x\" \"x\")"), "\"a\"");
}
