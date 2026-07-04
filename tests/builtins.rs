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
fn logb_special_cases_are_floats() {
    // Finite nonzero magnitudes yield the integer binary exponent (frexp e - 1).
    assert_eq!(eval("(logb 1024)"), "10");
    assert_eq!(eval("(logb -8.0)"), "3");
    assert_eq!(eval("(logb 0.5)"), "-1");
    // Zero, ±infinity and NaN fall through to C `logb`, which returns a *float*:
    // -inf for zero (int or float), +inf for either infinity, NaN for NaN.
    assert_eq!(eval("(logb 0.0)"), "-1.0e+INF");
    assert_eq!(eval("(logb 0)"), "-1.0e+INF");
    assert_eq!(eval("(logb (/ 1.0 0.0))"), "1.0e+INF");
    assert_eq!(eval("(logb (/ -1.0 0.0))"), "1.0e+INF");
    assert_eq!(eval("(logb (/ 0.0 0.0))"), "0.0e+NaN");
}

#[test]
fn max_char_and_byteorder() {
    // Default is the max Emacs char code (#x3FFFFF); UNICODE arg caps at #x10FFFF.
    assert_eq!(eval("(max-char)"), "4194303");
    assert_eq!(eval("(max-char t)"), "1114111");
    // Test host is little-endian aarch64/x86_64 → ?l.
    assert_eq!(eval("(byteorder)"), "108");
}

#[test]
fn bare_symbol_p_matches_symbolp() {
    // No symbol-with-position type here, so bare-symbol-p == symbolp: nil and t
    // are symbols, non-symbols are not.
    assert_eq!(
        eval("(list (bare-symbol-p 'foo) (bare-symbol-p nil) (bare-symbol-p t))"),
        "(t t t)"
    );
    assert_eq!(
        eval("(list (bare-symbol-p 5) (bare-symbol-p \"s\") (bare-symbol-p '(a)))"),
        "(nil nil nil)"
    );
}

#[test]
fn car_less_than_car_comparator() {
    assert_eq!(eval("(car-less-than-car '(1 . x) '(2 . y))"), "t");
    assert_eq!(eval("(car-less-than-car '(3 . x) '(2 . y))"), "nil");
    // Equal cars are not strictly less.
    assert_eq!(eval("(car-less-than-car '(2 a) '(2 b))"), "nil");
    // Mixed int/float cars compare numerically.
    assert_eq!(eval("(car-less-than-car '(2.5 a) '(3 b))"), "t");
    // Sorting an alist by its keys uses this as the predicate.
    assert_eq!(
        eval("(sort (list '(3 . c) '(1 . a) '(2 . b)) #'car-less-than-car)"),
        "((1 . a) (2 . b) (3 . c))"
    );
}

#[test]
fn subr_name_of_primitive() {
    assert_eq!(eval("(subr-name (symbol-function 'car))"), "\"car\"");
    assert_eq!(
        eval("(subr-name (symbol-function 'max-char))"),
        "\"max-char\""
    );
    // A plain symbol is not a subr.
    assert_eq!(
        eval("(condition-case e (subr-name 'car) (error (car e)))"),
        "wrong-type-argument"
    );
}

#[test]
fn default_boundp_and_toplevel_value() {
    // No buffer-local bindings: default-boundp tracks boundp, default-toplevel-value
    // tracks symbol-value.
    assert_eq!(
        eval("(progn (setq zz-dv 41) (list (default-boundp 'zz-dv) (default-toplevel-value 'zz-dv)))"),
        "(t 41)"
    );
    assert_eq!(eval("(default-boundp 'no-such-var-zzz)"), "nil");
    // Unbound symbol signals void-variable.
    assert_eq!(
        eval("(condition-case e (default-toplevel-value 'no-such-var-zzz) (error (car e)))"),
        "void-variable"
    );
}

#[test]
fn char_resolve_modifiers_folds_shift_and_ctl() {
    // Reader already resolves \C-a to 1; an explicit CHAR_CTL (2^26) bit folds
    // the same way. Meta (2^27) is left in place, not folded into the code.
    assert_eq!(eval("(char-resolve-modifiers ?\\C-a)"), "1");
    assert_eq!(eval("(char-resolve-modifiers (+ (expt 2 26) ?a))"), "1");
    assert_eq!(eval("(char-resolve-modifiers ?\\M-a)"), "134217825");
    assert_eq!(eval("(char-resolve-modifiers ?\\C-\\M-a)"), "134217729");
    // Shift on a lowercase letter uppercases it and drops the bit.
    assert_eq!(eval("(char-resolve-modifiers ?\\S-a)"), "65");
    // Plain ASCII and \C-@ pass through / resolve to 0; a non-ASCII base char
    // (only modifier bits stripped is still >= 0x80) is returned unchanged.
    assert_eq!(eval("(char-resolve-modifiers ?a)"), "97");
    assert_eq!(eval("(char-resolve-modifiers ?\\C-@)"), "0");
    assert_eq!(eval("(char-resolve-modifiers 4194303)"), "4194303");
}

#[test]
fn text_char_description_caret_forms() {
    // ASCII control chars render as ^X (byte + 64), DEL as ^?, SPC/printables
    // as themselves.
    assert_eq!(eval("(text-char-description 0)"), "\"^@\"");
    assert_eq!(eval("(text-char-description ?\\C-a)"), "\"^A\"");
    assert_eq!(eval("(text-char-description 27)"), "\"^[\"");
    assert_eq!(eval("(text-char-description 31)"), "\"^_\"");
    assert_eq!(eval("(text-char-description 127)"), "\"^?\"");
    assert_eq!(eval("(text-char-description 32)"), "\" \"");
    assert_eq!(eval("(text-char-description ?a)"), "\"a\"");
    // Non-ASCII chars come back as themselves (round-trip via string-to-char).
    assert_eq!(eval("(string-to-char (text-char-description 955))"), "955");
    // A char with modifier bits is not a valid character -> characterp error.
    assert_eq!(
        eval("(condition-case e (text-char-description ?\\M-a) (error (car e)))"),
        "wrong-type-argument"
    );
    assert_eq!(
        eval("(condition-case e (text-char-description -1) (error e))"),
        "(wrong-type-argument characterp -1)"
    );
}

#[test]
fn unibyte_multibyte_char_roundtrip() {
    // ASCII bytes are identity; high bytes 0x80..0xFF become eight-bit chars
    // 0x3FFF00 + byte, and back.
    assert_eq!(eval("(unibyte-char-to-multibyte 65)"), "65");
    assert_eq!(eval("(unibyte-char-to-multibyte 128)"), "4194176");
    assert_eq!(eval("(unibyte-char-to-multibyte 200)"), "4194248");
    assert_eq!(eval("(unibyte-char-to-multibyte 255)"), "4194303");
    assert_eq!(eval("(multibyte-char-to-unibyte 4194248)"), "200");
    assert_eq!(eval("(multibyte-char-to-unibyte 4194303)"), "255");
    // Chars below 256 map to themselves; ordinary multibyte chars have no
    // unibyte form and yield -1.
    assert_eq!(eval("(multibyte-char-to-unibyte 200)"), "200");
    assert_eq!(eval("(multibyte-char-to-unibyte 955)"), "-1");
    assert_eq!(eval("(multibyte-char-to-unibyte 300)"), "-1");
    // A byte above 255 is not a unibyte character (plain `error`); a negative
    // arg fails the characterp check first.
    assert_eq!(
        eval("(condition-case e (unibyte-char-to-multibyte 256) (error (error-message-string e)))"),
        "\"Not a unibyte character: 256\""
    );
    assert_eq!(
        eval("(condition-case e (unibyte-char-to-multibyte -1) (error (car e)))"),
        "wrong-type-argument"
    );
}

#[test]
fn emacs_pid_and_load_average_shape() {
    // emacs-pid is a positive integer; load-average returns the three system
    // load figures as integers by default, floats with a non-nil arg.
    assert_eq!(eval("(integerp (emacs-pid))"), "t");
    assert_eq!(eval("(> (emacs-pid) 0)"), "t");
    assert_eq!(eval("(length (load-average))"), "3");
    assert_eq!(eval("(integerp (car (load-average)))"), "t");
    assert_eq!(eval("(floatp (car (load-average t)))"), "t");
}

#[test]
fn identity_and_ignore() {
    assert_eq!(eval("(identity 42)"), "42");
    assert_eq!(eval("(ignore 1 2 3)"), "nil");
    assert_eq!(eval("(always)"), "t");
}

#[test]
fn string_to_number_trailing_dot_is_integer() {
    // A bare trailing dot keeps the value an integer, matching Emacs:
    // `(string-to-number "1.")` => 1 (integer), not 1.0.
    assert_eq!(eval("(string-to-number \"1.\")"), "1");
    assert_eq!(eval("(string-to-number \"12.\")"), "12");
    assert_eq!(eval("(string-to-number \"-3.\")"), "-3");
    assert_eq!(eval("(string-to-number \"1..\")"), "1");
    // But a digit after the dot, or an exponent, still makes it a float.
    assert_eq!(eval("(string-to-number \"1.5\")"), "1.5");
    assert_eq!(eval("(string-to-number \".5\")"), "0.5");
    assert_eq!(eval("(string-to-number \"1.e3\")"), "1000.0");
    assert_eq!(eval("(string-to-number \"1e3\")"), "1000.0");
    // Type is really integer, not a float that prints without ".0".
    assert_eq!(eval("(integerp (string-to-number \"1.\"))"), "t");
    assert_eq!(eval("(floatp (string-to-number \"1.5\"))"), "t");
}

#[test]
fn string_to_number_base_out_of_range_errors() {
    // Emacs restricts BASE to 2..16 and signals args-out-of-range with the base
    // as its sole datum; a valid base still parses.
    assert_eq!(eval("(string-to-number \"ff\" 16)"), "255");
    assert_eq!(eval("(string-to-number \"101\" 2)"), "5");
    assert_eq!(
        eval("(condition-case e (string-to-number \"z\" 36) (args-out-of-range (cdr e)))"),
        "(36)"
    );
    assert_eq!(
        eval("(condition-case e (string-to-number \"10\" 20) (args-out-of-range (cdr e)))"),
        "(20)"
    );
    assert_eq!(
        eval("(condition-case e (string-to-number \"10\" 1) (args-out-of-range (cdr e)))"),
        "(1)"
    );
}

#[test]
fn substring_error_data_reports_raw_args() {
    // Out-of-range substring reports the *original* FROM/TO arguments (nil for
    // an omitted TO, raw negatives), not the resolved/defaulted values.
    assert_eq!(
        eval("(condition-case e (substring \"abc\" 5) (args-out-of-range (cdr e)))"),
        "(\"abc\" 5 nil)"
    );
    assert_eq!(
        eval("(condition-case e (substring \"abc\" -5) (args-out-of-range (cdr e)))"),
        "(\"abc\" -5 nil)"
    );
    assert_eq!(
        eval("(condition-case e (substring \"abc\" -5 -1) (args-out-of-range (cdr e)))"),
        "(\"abc\" -5 -1)"
    );
    // A non-array first argument fails the `arrayp` type check.
    assert_eq!(
        eval("(condition-case e (substring 5 0) (wrong-type-argument (cdr e)))"),
        "(arrayp 5)"
    );
}

#[test]
fn substring_on_vectors() {
    // Emacs `substring` slices vectors too, returning a fresh vector.
    assert_eq!(eval("(substring [1 2 3 4] 1 3)"), "[2 3]");
    assert_eq!(eval("(substring [1 2 3] -2)"), "[2 3]");
    assert_eq!(eval("(substring [1 2 3])"), "[1 2 3]");
    assert_eq!(
        eval("(condition-case e (substring [1 2 3] 5) (args-out-of-range (cdr e)))"),
        "([1 2 3] 5 nil)"
    );
}

#[test]
fn format_argument_step_errors() {
    // A missing argument for a consuming directive is a plain `error` with the
    // Emacs message, not a wrong-type signal.
    for form in [
        "(format \"%d\")",
        "(format \"%s\")",
        "(format \"%S\")",
        "(format \"%x\")",
        "(format \"%d %%\")",
    ] {
        assert_eq!(
            eval(&format!(
                "(condition-case e {form} (error (list (car e) (cadr e))))"
            )),
            "(error \"Not enough arguments for format string\")",
            "form: {form}"
        );
    }
    // A non-numeric argument to a numeric/char directive is the type-mismatch
    // error (curly apostrophe matches `emacs -Q`).
    let bad_type = "(error \"Format specifier doesn\u{2019}t match argument type\")";
    assert_eq!(
        eval("(condition-case e (format \"%d\" \"x\") (error (list (car e) (cadr e))))"),
        bad_type
    );
    assert_eq!(
        eval("(condition-case e (format \"%c\" \"x\") (error (list (car e) (cadr e))))"),
        bad_type
    );
    // Well-formed calls still work.
    assert_eq!(eval("(format \"%d\" 42)"), "\"42\"");
    assert_eq!(eval("(format \"%2$s %1$s\" \"a\" \"b\")"), "\"b a\"");
}

#[test]
fn hash_table_eql_matches_float_keys() {
    // The default hash test is `eql`, which matches equal floats — a float key
    // stored with puthash must be found again by gethash (was `eq`, missed it).
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1.5 'v h) (gethash 1.5 h))"),
        "v"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eql))) (puthash 1.5 'v h) (gethash 1.5 h))"),
        "v"
    );
    // Re-putting an eql-equal float key overwrites in place (count stays 1).
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 2.0 'a h) (puthash 2.0 'b h) (list (gethash 2.0 h) (hash-table-count h)))"),
        "(b 1)"
    );
    // Under the `eq` test two distinct float objects are not identical, so the
    // key is not found.
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'eq))) (puthash 1.5 'v h) (gethash 1.5 h))"),
        "nil"
    );
}

#[test]
fn format_time_string_subsecond_field() {
    // %N is nanoseconds as a fixed 9-digit number; a width <= 9 keeps that many
    // leading digits (%3N milliseconds, %6N microseconds), width > 9 right-pads.
    assert_eq!(eval("(format-time-string \"%N\" 0 t)"), "\"000000000\"");
    assert_eq!(eval("(format-time-string \"%N\" 1.5 t)"), "\"500000000\"");
    assert_eq!(eval("(format-time-string \"%3N\" 1.5 t)"), "\"500\"");
    assert_eq!(eval("(format-time-string \"%6N\" 1.25 t)"), "\"250000\"");
    assert_eq!(
        eval("(format-time-string \"%9N\" 1.123456789 t)"),
        "\"123456789\""
    );
    assert_eq!(
        eval("(format-time-string \"%12N\" 1.5 t)"),
        "\"500000000000\""
    );
    assert_eq!(eval("(format-time-string \"%3N\" 0 t)"), "\"000\"");
}

#[test]
fn make_vector_and_make_string_reject_negative_length() {
    // Emacs signals (wrong-type-argument wholenump N) rather than silently
    // clamping a negative length to an empty sequence.
    assert_eq!(
        eval("(condition-case e (make-vector -1 0) (error e))"),
        "(wrong-type-argument wholenump -1)"
    );
    assert_eq!(
        eval("(condition-case e (make-string -3 65) (error e))"),
        "(wrong-type-argument wholenump -3)"
    );
    // Non-negative lengths still build normally.
    assert_eq!(eval("(make-vector 3 0)"), "[0 0 0]");
    assert_eq!(eval("(make-string 3 65)"), "\"AAA\"");
}

#[test]
fn vconcat_and_append_report_bad_sequence_value() {
    // The sequencep error DATA must carry the offending value, so a
    // condition-case handler can inspect it (was dropped before).
    assert_eq!(
        eval("(condition-case e (vconcat 5) (error e))"),
        "(wrong-type-argument sequencep 5)"
    );
    assert_eq!(
        eval("(condition-case e (append 5 nil) (error e))"),
        "(wrong-type-argument sequencep 5)"
    );
}

#[test]
fn regexp_quote_does_not_escape_close_bracket() {
    // Emacs's regexp-quote escapes `*.\?+[^$` but NOT `]` (search.c
    // Fregexp_quote). Previously `]` was over-escaped.
    assert_eq!(eval(r#"(regexp-quote "a]b")"#), r#""a]b""#);
    assert_eq!(eval(r#"(regexp-quote "a[b]c")"#), r#""a\\[b]c""#);
    assert_eq!(
        eval(r#"(regexp-quote ".*+?[]^$")"#),
        r#""\\.\\*\\+\\?\\[]\\^\\$""#
    );
}

#[test]
fn string_distance_bytewise_counts_utf8_bytes() {
    // With BYTECOMPARE non-nil Emacs measures the edit distance over UTF-8
    // bytes, so the 2-byte é vs the 1-byte e costs 2, not 1.
    assert_eq!(eval(r#"(string-distance "café" "cafe")"#), "1");
    assert_eq!(eval(r#"(string-distance "café" "cafe" t)"#), "2");
    // Bytewise distance from empty counts the byte length (é = 2 bytes).
    assert_eq!(eval(r#"(string-distance "" "é" t)"#), "2");
    assert_eq!(eval(r#"(string-distance "" "é")"#), "1");
}

#[test]
fn upcase_char_uses_simple_single_char_mapping() {
    // upcase on a *character* folds to one char. ß has a distinct single-char
    // uppercase ẞ (U+1E9E); Rust's full mapping would give "SS" -> 'S'.
    assert_eq!(eval("(upcase ?ß)"), "7838");
    assert_eq!(eval("(char-to-string (upcase ?ß))"), "\"ẞ\"");
    // Greek iota-subscript titlecase forms map to their single titlecase char.
    assert_eq!(eval("(upcase 8064)"), "8072");
    assert_eq!(eval("(upcase 8115)"), "8124");
    assert_eq!(eval("(upcase 8179)"), "8188");
    // A multi-char full mapping with no single simple mapping stays unchanged
    // (the ﬁ ligature), rather than collapsing to its first char 'F'.
    assert_eq!(eval("(upcase 64257)"), "64257");
    // downcase of İ (U+0130) full-maps to two chars -> Emacs leaves it as-is.
    assert_eq!(eval("(downcase 304)"), "304");
    // Whole-string upcase still uses the full mapping.
    assert_eq!(eval(r#"(upcase "ß")"#), "\"SS\"");
}

#[test]
fn mod_float_preserves_signed_zero() {
    // Emacs `mod` on floats is fmod + sign-fix; an exact-multiple negative
    // dividend keeps -0.0 (was flushed to 0.0 by the floor-based formula).
    assert_eq!(eval("(mod -0.0 5)"), "-0.0");
    assert_eq!(eval("(mod -7.5 2.5)"), "-0.0");
    // Ordinary cases unchanged, sign follows the divisor.
    assert_eq!(eval("(mod 5.5 2)"), "1.5");
    assert_eq!(eval("(mod -5.5 2)"), "0.5");
    assert_eq!(eval("(mod 5.5 -2)"), "-0.5");
}

#[test]
fn ldexp_preserves_subnormals() {
    // scalbn semantics: subnormal results survive instead of underflowing to 0
    // (a naive x*2^n overflows 2^n to inf, then to 0).
    assert_eq!(eval("(ldexp 1.0 -1074)"), "5e-324");
    assert_eq!(eval("(ldexp 3.0 -1074)"), "1.5e-323");
    assert_eq!(eval("(ldexp -2.0 -1074)"), "-1e-323");
    // Below the subnormal floor it does round to zero.
    assert_eq!(eval("(ldexp 1.0 -1075)"), "0.0");
    // Overflow and ordinary scaling still behave.
    assert_eq!(eval("(ldexp 1.0 1024)"), "1.0e+INF");
    assert_eq!(eval("(ldexp 1.5 3)"), "12.0");
}

#[test]
fn frexp_is_exact_for_extreme_magnitudes() {
    // Bit-level decomposition: significand in [0.5,1) is exact for huge and
    // subnormal inputs (the log2/divide formula gave 0.0 and inf respectively).
    assert_eq!(eval("(frexp 1e308)"), "(0.5562684646268004 . 1024)");
    assert_eq!(eval("(frexp 5e-324)"), "(0.5 . -1073)");
    assert_eq!(eval("(frexp 8.0)"), "(0.5 . 4)");
    assert_eq!(eval("(frexp 1.0)"), "(0.5 . 1)");
    // Zero, signed zero, and non-finite pass through with exponent 0.
    assert_eq!(eval("(frexp 0.0)"), "(0.0 . 0)");
    assert_eq!(eval("(frexp -0.0)"), "(-0.0 . 0)");
    assert_eq!(eval("(frexp 1.0e+INF)"), "(1.0e+INF . 0)");
}

#[test]
fn char_or_string_p_bounds_the_character_range() {
    // A "character" is an integer in [0, #x3FFFFF]; strings always qualify, and
    // anything past the upper bound or below zero does not (oracle: emacs 30.2).
    assert_eq!(eval("(char-or-string-p 4194303)"), "t");
    assert_eq!(eval("(char-or-string-p 4194304)"), "nil");
    assert_eq!(eval("(char-or-string-p -1)"), "nil");
    assert_eq!(eval("(char-or-string-p 0)"), "t");
    assert_eq!(eval("(char-or-string-p \"x\")"), "t");
    assert_eq!(eval("(char-or-string-p 'sym)"), "nil");
    assert_eq!(eval("(char-or-string-p 1.0)"), "nil");
}

#[test]
fn string_search_bounds_check_start() {
    // START in [0, len] is honoured (len itself allowed); outside signals
    // args-out-of-range with the raw START value (oracle: emacs 30.2).
    assert_eq!(eval("(string-search \"lo\" \"hello\" 3)"), "3");
    assert_eq!(eval("(string-search \"i\" \"hi\" 1)"), "1");
    assert_eq!(eval("(string-search \"\" \"hi\" 2)"), "2");
    // START is a char index; the returned index is a char index too.
    assert_eq!(eval("(string-search \"o\" \"héllo\" 2)"), "4");
    assert_eq!(
        eval("(condition-case e (string-search \"x\" \"hi\" 10) (args-out-of-range (cdr e)))"),
        "(10)"
    );
    assert_eq!(
        eval("(condition-case e (string-search \"x\" \"hi\" -1) (args-out-of-range (cdr e)))"),
        "(-1)"
    );
}

#[test]
fn length_rejects_improper_lists_and_terminates_on_cycles() {
    // A proper list / vector / string counts normally; a dotted (improper) tail
    // signals `wrong-type-argument listp TAIL` with the offending tail value.
    assert_eq!(eval("(length '(1 2 3))"), "3");
    assert_eq!(eval("(length nil)"), "0");
    assert_eq!(eval("(length [1 2 3])"), "3");
    assert_eq!(eval("(length \"héllo\")"), "5");
    assert_eq!(
        eval("(condition-case e (length (cons 1 2)) (wrong-type-argument (cdr e)))"),
        "(listp 2)"
    );
    assert_eq!(
        eval("(condition-case e (length '(1 2 . 3)) (wrong-type-argument (cdr e)))"),
        "(listp 3)"
    );
    // A circular list terminates with `circular-list` (Floyd detection) rather
    // than looping forever.
    assert_eq!(
        eval("(condition-case e (let ((l (list 1 2 3))) (setcdr (cddr l) l) (length l)) (circular-list 'caught))"),
        "caught"
    );
}

#[test]
fn bitwise_and_modulo_reject_floats() {
    // Valid integer arguments still compute; floats/other are rejected with the
    // predicate Emacs uses: `integer-or-marker-p` for `%`/logand/logior/logxor,
    // `integerp` for ash/lsh/lognot/logcount (oracle: emacs 30.2).
    assert_eq!(eval("(% 7 3)"), "1");
    assert_eq!(eval("(logand 3 5)"), "1");
    assert_eq!(eval("(logcount 7)"), "3");
    assert_eq!(
        eval("(condition-case e (% 7.0 2) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p 7.0)"
    );
    assert_eq!(
        eval("(condition-case e (% 7 2.0) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p 2.0)"
    );
    assert_eq!(
        eval("(condition-case e (logand 3.0 2) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p 3.0)"
    );
    assert_eq!(
        eval("(condition-case e (logior 3.0) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p 3.0)"
    );
    assert_eq!(
        eval("(condition-case e (logxor 3.0) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p 3.0)"
    );
    // A non-number reports the value readably (a string stays quoted).
    assert_eq!(
        eval("(condition-case e (logand \"x\" 2) (wrong-type-argument (cdr e)))"),
        "(integer-or-marker-p \"x\")"
    );
    assert_eq!(
        eval("(condition-case e (ash 1.0 2) (wrong-type-argument (cdr e)))"),
        "(integerp 1.0)"
    );
    assert_eq!(
        eval("(condition-case e (lognot 3.0) (wrong-type-argument (cdr e)))"),
        "(integerp 3.0)"
    );
    assert_eq!(
        eval("(condition-case e (logcount 3.0) (wrong-type-argument (cdr e)))"),
        "(integerp 3.0)"
    );
}

#[test]
fn ash_large_shift_fills_with_sign_bit() {
    // A right shift ≥ 64 collapses to the sign bit — 0 for a non-negative value,
    // -1 for a negative one — instead of panicking on the out-of-range count.
    assert_eq!(eval("(ash 1 -100)"), "0");
    assert_eq!(eval("(ash 8 -100)"), "0");
    assert_eq!(eval("(ash -1 -100)"), "-1");
    assert_eq!(eval("(ash -8 -100)"), "-1");
    assert_eq!(eval("(ash 1 -64)"), "0");
    assert_eq!(eval("(ash -1 -64)"), "-1");
    // Ordinary in-range shifts are unaffected.
    assert_eq!(eval("(ash 8 -2)"), "2");
    assert_eq!(eval("(ash 1 4)"), "16");
}

#[test]
fn char_to_string_and_make_string_check_characterp() {
    // Valid characters (including astral ones) convert; out-of-range codes signal
    // `wrong-type-argument characterp CODE` (oracle: emacs 30.2).
    assert_eq!(eval("(char-to-string ?a)"), "\"a\"");
    assert_eq!(eval("(char-to-string 128512)"), "\"😀\"");
    assert_eq!(eval("(make-string 3 ?x)"), "\"xxx\"");
    assert_eq!(eval("(make-string 3 128512)"), "\"😀😀😀\"");
    assert_eq!(
        eval("(condition-case e (char-to-string 4194304) (wrong-type-argument (cdr e)))"),
        "(characterp 4194304)"
    );
    assert_eq!(
        eval("(condition-case e (char-to-string -1) (wrong-type-argument (cdr e)))"),
        "(characterp -1)"
    );
    assert_eq!(
        eval("(condition-case e (make-string 3 -1) (wrong-type-argument (cdr e)))"),
        "(characterp -1)"
    );
    assert_eq!(
        eval("(condition-case e (make-string 3 4194304) (wrong-type-argument (cdr e)))"),
        "(characterp 4194304)"
    );
}

#[test]
fn nth_requires_an_integer_index() {
    // Improper lists are fine for an in-range index, but N itself must be an
    // integer — a float or other type signals `integerp` (oracle: emacs 30.2).
    assert_eq!(eval("(nth 1 '(a b c))"), "b");
    assert_eq!(eval("(nth 0 '(a . 1))"), "a");
    assert_eq!(
        eval("(condition-case e (nth 1.5 '(a b c)) (wrong-type-argument (cdr e)))"),
        "(integerp 1.5)"
    );
    assert_eq!(
        eval("(condition-case e (nth \"a\" '(a b)) (wrong-type-argument (cdr e)))"),
        "(integerp \"a\")"
    );
}

#[test]
fn lsh_is_a_logical_shift_not_arithmetic() {
    // `lsh` differs from `ash` only on a right shift: it treats the fixnum as an
    // unsigned value of the fixnum bit width (62 bits here), so vacated high bits
    // fill with zeros rather than the sign bit (oracle: emacs 30.2).
    assert_eq!(eval("(lsh -1 -1)"), "2305843009213693951");
    assert_eq!(eval("(lsh -2 -1)"), "2305843009213693951");
    assert_eq!(eval("(lsh -8 -2)"), "1152921504606846974");
    assert_eq!(eval("(lsh most-negative-fixnum -1)"), "1152921504606846976");
    assert_eq!(eval("(lsh -1 -61)"), "1");
    assert_eq!(eval("(lsh -1 -100)"), "0");
    // Non-negative values and left shifts behave exactly like `ash`.
    assert_eq!(eval("(lsh 6 -1)"), "3");
    assert_eq!(eval("(lsh 1 4)"), "16");
    assert_eq!(eval("(lsh -1 1)"), "-2");
    // `ash`, by contrast, keeps the sign on a right shift.
    assert_eq!(eval("(ash -1 -1)"), "-1");
    assert_eq!(eval("(ash -8 -2)"), "-2");
}

#[test]
fn format_accepts_percent_i_as_decimal_alias() {
    // `%i` is an accepted alias for `%d` (as in C printf), honouring the same
    // width/precision/sign flags (oracle: emacs 30.2).
    assert_eq!(eval("(format \"%i\" 42)"), "\"42\"");
    assert_eq!(eval("(format \"%i\" -4)"), "\"-4\"");
    assert_eq!(eval("(format \"%i\" 3.9)"), "\"3\"");
    assert_eq!(eval("(format \"%+i\" 5)"), "\"+5\"");
    assert_eq!(eval("(format \"%.3i\" 7)"), "\"007\"");
    assert_eq!(eval("(format \"%5i\" 3)"), "\"    3\"");
    assert_eq!(eval("(format \"%i %d\" 10 20)"), "\"10 20\"");
}

#[test]
fn format_signals_on_invalid_conversion() {
    // An unknown conversion is a plain `error` "Invalid format operation %X" —
    // but Emacs validates argument availability first, so a missing argument is
    // still "Not enough arguments" (oracle: emacs 30.2).
    assert_eq!(
        eval("(condition-case e (format \"%b\" 1) (error (cadr e)))"),
        "\"Invalid format operation %b\""
    );
    assert_eq!(
        eval("(condition-case e (format \"%5z\" 1) (error (cadr e)))"),
        "\"Invalid format operation %z\""
    );
    assert_eq!(
        eval("(condition-case e (format \"%b\") (error (cadr e)))"),
        "\"Not enough arguments for format string\""
    );
}

#[test]
fn string_match_start_counts_from_end_and_checks_bounds() {
    // A negative START counts from the end (`len + START`); START in `[0, len]`
    // is valid (len itself only matches empty), and anything outside that range
    // is args-out-of-range with DATA `(STRING RAW-START)` (oracle: emacs 30.2).
    assert_eq!(eval("(string-match \"c\" \"abc\" -1)"), "2");
    assert_eq!(eval("(string-match \"b\" \"abc\" -3)"), "1");
    assert_eq!(eval("(string-match \"a\" \"abc\" 3)"), "nil");
    assert_eq!(eval("(string-match \"\" \"abc\" 3)"), "3");
    assert_eq!(
        eval("(condition-case e (string-match \"a\" \"abc\" 4) (args-out-of-range (cdr e)))"),
        "(\"abc\" 4)"
    );
    assert_eq!(
        eval("(condition-case e (string-match \"a\" \"abc\" -100) (args-out-of-range (cdr e)))"),
        "(\"abc\" -100)"
    );
}

#[test]
fn read_from_string_honours_start_and_end() {
    // END limits how much of STRING is read; START/END count from the end when
    // negative, default to 0/len, and args-out-of-range reports the raw args
    // (nil for an omitted END) (oracle: emacs 30.2).
    assert_eq!(eval("(read-from-string \"hello\" 0 3)"), "(hel . 3)");
    assert_eq!(eval("(read-from-string \"12345\" 1 3)"), "(23 . 3)");
    assert_eq!(eval("(read-from-string \"hello\" -1)"), "(o . 5)");
    assert_eq!(eval("(read-from-string \"hello\" 0 5)"), "(hello . 5)");
    // START == len (or an empty END window) is end-of-file, not a match.
    assert_eq!(
        eval("(condition-case e (read-from-string \"hello\" 5) (end-of-file 'eof))"),
        "eof"
    );
    assert_eq!(
        eval("(condition-case e (read-from-string \"hello\" 0 10) (args-out-of-range (cdr e)))"),
        "(\"hello\" 0 10)"
    );
    assert_eq!(
        eval("(condition-case e (read-from-string \"hello\" 6) (args-out-of-range (cdr e)))"),
        "(\"hello\" 6 nil)"
    );
    assert_eq!(
        eval("(condition-case e (read-from-string \"hello\" 3 2) (args-out-of-range (cdr e)))"),
        "(\"hello\" 3 2)"
    );
}

#[test]
fn float_rounding_of_infinity_signals_overflow_error() {
    // truncate/round/floor/ceiling of a non-finite float have no integer result;
    // Emacs signals `overflow-error` (empty data) rather than saturating an int.
    // The condition object is `(overflow-error)` (oracle: emacs 30.2).
    assert_eq!(
        eval("(condition-case e (truncate 1.0e+INF) (error e))"),
        "(overflow-error)"
    );
    assert_eq!(
        eval("(condition-case e (round (/ 0.0 0.0)) (error e))"),
        "(overflow-error)"
    );
    // The `overflow-error` condition inherits arith-error/range-error, so those
    // parent handlers catch it too (error-conditions chain, oracle: emacs 30.2).
    assert_eq!(
        eval("(condition-case e (floor 1.0e+INF) (arith-error 'ar))"),
        "ar"
    );
    assert_eq!(
        eval("(condition-case e (ceiling -1.0e+INF) (range-error 'rg))"),
        "rg"
    );
    // The 2-arg DIVISOR form signals it when the quotient is non-finite; a zero
    // divisor still wins as arith-error.
    assert_eq!(
        eval("(condition-case e (floor 1.0e+INF 2.0) (overflow-error 'oe))"),
        "oe"
    );
    assert_eq!(
        eval("(condition-case e (truncate 1.0e+INF 0.0) (arith-error 'z))"),
        "z"
    );
    // Finite floats are unchanged; the float-returning f* forms stay float.
    assert_eq!(
        eval("(list (truncate 5.9) (round 2.5) (floor 3.7))"),
        "(5 2 3)"
    );
    assert_eq!(eval("(ffloor 1.0e+INF)"), "1.0e+INF");
    assert_eq!(eval("(ftruncate -1.0e+INF)"), "-1.0e+INF");
}

#[test]
fn format_of_non_finite_floats() {
    // %e/%f/%g of inf/nan render "inf"/"-inf"/"nan" (Emacs used to be a panic in
    // elisprs). Precision is ignored, the `0` flag falls back to space padding,
    // and +/space signs apply to infinities but not NaN (oracle: emacs 30.2).
    assert_eq!(
        eval(
            "(list (format \"%g\" 1.0e+INF) (format \"%g\" -1.0e+INF) (format \"%g\" (/ 0.0 0.0)))"
        ),
        "(\"inf\" \"-inf\" \"nan\")"
    );
    assert_eq!(
        eval(
            "(list (format \"%e\" 1.0e+INF) (format \"%f\" -1.0e+INF) (format \"%.2f\" 1.0e+INF))"
        ),
        "(\"inf\" \"-inf\" \"inf\")"
    );
    assert_eq!(
        eval(
            "(list (format \"%10g\" 1.0e+INF) (format \"%+g\" 1.0e+INF) (format \"% g\" 1.0e+INF))"
        ),
        "(\"       inf\" \"+inf\" \" inf\")"
    );
    assert_eq!(eval("(format \"%010g\" 1.0e+INF)"), "\"       inf\"");
    assert_eq!(eval("(format \"%+g\" (/ 0.0 0.0))"), "\"nan\"");
}

#[test]
fn log_uses_exact_base_10_and_2() {
    // Emacs computes `(log X 10)`/`(log X 2)` via log10/log2, exact for powers of
    // the base; a naive ln(x)/ln(base) yields 2.9999999999999996 (oracle: 30.2).
    assert_eq!(
        eval("(list (log 1000 10) (log 1024 2) (log 100 10))"),
        "(3.0 10.0 2.0)"
    );
    // Other bases fall back to the ratio form and match Emacs bit-for-bit.
    assert_eq!(eval("(log 8 3)"), "1.892789260714372");
}
