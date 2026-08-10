//! Round 19: the primitives Emacs's byte compiler open-codes, and the
//! diagnostics whose wording was frozen in elisprs's own source.
//!
//! Every expectation here was produced by running the form under GNU Emacs 30.2
//! (`emacs -Q --batch --eval '(prin1 …)'`) and copying what it printed. Where a
//! value is an artifact of *this* tree rather than of Emacs it says so.

use elisprs::{eval_str, print, reset_host};

/// The form's value, or `!DATA` for the condition it signalled — the same shape
/// the differential probe used, so an expectation can be pasted from a run.
fn eval(src: &str) -> String {
    reset_host();
    let wrapped = format!("(condition-case e {src} (error (format \"!%S\" e)))");
    let v = eval_str(&wrapped).expect("eval failed");
    print(&v, true).trim_matches('"').replace("\\\"", "\"")
}

// ── advice on a subr: the open-coding contract ───────────────────────────────

/// `advice-add` on a Rust subr used to raise `wrong-type-argument` and leave the
/// subr broken for the rest of the process; the advice machinery reached
/// `gv-deref`'s `(car ref)` after it had already advised `car`.
#[test]
fn advice_on_car_applies_and_does_not_corrupt_it() {
    assert_eq!(
        eval("(progn (advice-add 'car :filter-return #'1+) (car '(1 2)))"),
        "2"
    );
    assert_eq!(
        eval("(progn (advice-add 'car :filter-return #'1+) (list (car '(1 2)) (car '(5 6))))"),
        "(2 6)"
    );
    // `symbol-function` alone used to raise the same error.
    assert_eq!(
        eval("(progn (advice-add 'car :filter-return #'1+) (functionp (symbol-function 'car)))"),
        "t"
    );
    assert_eq!(
        eval(
            "(progn (advice-add 'car :filter-return #'1+) (advice-remove 'car #'1+) (car '(1 2)))"
        ),
        "1"
    );
}

/// Advising `car` must not reach `cdr`, `nth` or `assq` — Emacs answers
/// `(2 (2) 5 (a . 1))`, i.e. only the direct `car` call is filtered.
#[test]
fn advice_on_car_leaves_its_neighbours_alone() {
    assert_eq!(
        eval(
            "(progn (advice-add 'car :filter-return #'1+) \
             (list (car '(1 2)) (cdr '(1 2)) (nth 0 '(5 6)) (assq 'a '((a . 1)))))"
        ),
        "(2 (2) 5 (a . 1))"
    );
}

/// `length` is open-coded too, and advising it used to explode inside the
/// advice machinery with `(wrong-number-of-arguments (obj car cdr how props) 6)`.
#[test]
fn advice_on_length_and_message_applies() {
    assert_eq!(
        eval("(progn (advice-add 'length :filter-return #'1+) (length \"abc\"))"),
        "4"
    );
    // `message` is NOT open-coded by Emacs's byte compiler (measured), so
    // prelude callers must keep honouring advice on it.
    assert_eq!(
        eval("(progn (advice-add 'format :filter-return #'upcase) (format \"hi %d\" 3))"),
        "HI 3"
    );
}

/// An *interpreted* user `defun` calling `car` still sees the advice — Emacs
/// answers 2, because only byte-compiled code open-codes the call. Open-coding
/// user code as well would have silently broken this.
#[test]
fn advice_still_reaches_an_interpreted_user_defun() {
    assert_eq!(
        eval(
            "(progn (defun my-g (x) (car x)) (advice-add 'car :filter-return #'1+) (my-g '(1 2)))"
        ),
        "2"
    );
}

// ── string-to-number: whitespace and BASE ────────────────────────────────────

/// `string_to_number` skips exactly SPC and TAB. `trim_start` skipped every
/// Unicode space, so `"\n12"` answered 12 where Emacs answers 0.
#[test]
fn string_to_number_skips_only_space_and_tab() {
    assert_eq!(eval("(string-to-number \"  \\t 12\")"), "12");
    for s in ["\\n12", "\\r12", "\\f12", "\\v12", "\\u00a012", "\\u300012"] {
        assert_eq!(
            eval(&format!("(string-to-number \"{s}\")")),
            "0",
            "leading {s:?} must not be skipped"
        );
    }
}

/// BASE is `CHECK_FIXNUM`, and the check precedes the 2..=16 range check.
#[test]
fn string_to_number_base_is_a_fixnum() {
    assert_eq!(
        eval("(string-to-number \"1\" 'a)"),
        "!(wrong-type-argument fixnump a)"
    );
    assert_eq!(
        eval("(string-to-number \"1\" t)"),
        "!(wrong-type-argument fixnump t)"
    );
    // 2.0 used to be truncated to base 2 and answer 1.
    assert_eq!(
        eval("(string-to-number \"1\" 2.0)"),
        "!(wrong-type-argument fixnump 2.0)"
    );
    assert_eq!(
        eval("(string-to-number \"1\" (expt 2 70))"),
        "!(wrong-type-argument fixnump 1180591620717411303424)"
    );
    // The range check still applies to a fixnum.
    assert_eq!(
        eval("(string-to-number \"1\" -1)"),
        "!(args-out-of-range -1)"
    );
    assert_eq!(eval("(string-to-number \"ff\" 16)"), "255");
}

// ── reader conditions ────────────────────────────────────────────────────────

/// Truncated input is `end-of-file`, not nine flavours of lowercase prose.
#[test]
fn truncated_input_is_end_of_file() {
    for src in ["(", "[1", "(1 2", "'", "`", "?"] {
        assert_eq!(
            eval(&format!("(read \"{src}\")")),
            "!(end-of-file)",
            "for {src:?}"
        );
    }
    // An unterminated string literal, and an empty one.
    assert_eq!(eval("(read \"\\\"ab\")"), "!(end-of-file)");
    assert_eq!(eval("(read \"\")"), "!(end-of-file)");
}

/// A stray closer, and an unrecognised `#` dispatch, are `invalid-read-syntax`
/// whose datum is the offending text.
#[test]
fn stray_closer_and_bad_hash_are_invalid_read_syntax() {
    assert_eq!(eval("(read \")\")"), "!(invalid-read-syntax \")\")");
    assert_eq!(eval("(read \"]\")"), "!(invalid-read-syntax \"]\")");
    assert_eq!(eval("(read \"#z\")"), "!(invalid-read-syntax \"#z\")");
    assert_eq!(eval("(read \"(#)\")"), "!(invalid-read-syntax \"#)\")");
    assert_eq!(eval("(read \"#&3\")"), "!(invalid-read-syntax \"#&\")");
}

/// `##` is the interned empty-name symbol and `#:foo` an uninterned one; both
/// used to come back from `read_atom` as symbols whose names kept the `#`.
#[test]
fn hash_hash_and_hash_colon_read_as_symbols() {
    assert_eq!(eval("(eq (read \"##\") (intern \"\"))"), "t");
    assert_eq!(eval("(symbol-name (read \"#:foo\"))"), "foo");
    assert_eq!(eval("(eq (read \"#:foo\") 'foo)"), "nil");
}

// ── diagnostics whose wording was frozen in source ───────────────────────────

/// The four OClosure accessors said `Wrong type argument: closurep` — the
/// *rendered* form, which `make_error_object` turned into a symbol named
/// `Wrong type argument` holding a string.
#[test]
fn oclosure_accessors_signal_cl_assertion_failed() {
    assert_eq!(
        eval("(oclosure--get 5 0 nil)"),
        "!(cl-assertion-failed (closurep oclosure))"
    );
    assert_eq!(
        eval("(oclosure--set 5 0 nil)"),
        "!(cl-assertion-failed (closurep oclosure))"
    );
}

/// `setf` had two disagreeing texts of its own while `gv-get` already signalled
/// the right condition.
#[test]
fn setf_on_a_bad_place_signals_gv_invalid_place() {
    assert_eq!(eval("(macroexpand '(setf 5 1))"), "!(gv-invalid-place 5)");
    assert_eq!(
        eval("(macroexpand '(setf [1 2] 1))"),
        "!(gv-invalid-place [1 2])"
    );
}

/// The condition's message, and its DATA (the symbol, never its name).
#[test]
fn cyclic_variable_indirection_message_and_data() {
    assert_eq!(
        eval("(get 'cyclic-variable-indirection 'error-message)"),
        "Symbol's chain of variable indirections contains a loop"
    );
    assert_eq!(
        eval("(defvaralias 'q1 'q1)"),
        "!(cyclic-variable-indirection q1)"
    );
}

/// `wrong-type-argument` must name the offender. These five sites dropped it.
#[test]
fn wrong_type_argument_names_the_offending_value() {
    assert_eq!(eval("(setcar 5 1)"), "!(wrong-type-argument consp 5)");
    assert_eq!(eval("(setcdr 5 1)"), "!(wrong-type-argument consp 5)");
    assert_eq!(
        eval("(unintern 5 obarray)"),
        "!(wrong-type-argument stringp 5)"
    );
    assert_eq!(
        eval("(gethash 1 5)"),
        "!(wrong-type-argument hash-table-p 5)"
    );
    assert_eq!(
        eval("(buffer-local-value 'x 5)"),
        "!(wrong-type-argument bufferp 5)"
    );
    assert_eq!(eval("(fset 5 5)"), "!(wrong-type-argument symbolp 5)");
}

/// Three callers of one shared `integerp` accessor, each of which Emacs guards
/// with a different predicate.
#[test]
fn position_and_character_predicates_are_per_caller() {
    assert_eq!(
        eval("(goto-char \"a\")"),
        "!(wrong-type-argument integer-or-marker-p \"a\")"
    );
    assert_eq!(
        eval("(goto-char 1.5)"),
        "!(wrong-type-argument integer-or-marker-p 1.5)"
    );
    assert_eq!(
        eval("(forward-char 1.5)"),
        "!(wrong-type-argument fixnump 1.5)"
    );
    assert_eq!(
        eval("(backward-char (expt 2 70))"),
        "!(wrong-type-argument fixnump 1180591620717411303424)"
    );
    assert_eq!(
        eval("(char-equal 1.5 1)"),
        "!(wrong-type-argument characterp 1.5)"
    );
    assert_eq!(
        eval("(char-equal -1 1)"),
        "!(wrong-type-argument characterp -1)"
    );
    // `goto-char` returns POSITION as given, so a marker comes back a marker.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (markerp (goto-char (point-marker))))"),
        "t"
    );
}

/// A value with no read syntax used to vanish from the DATA entirely, because
/// the list is rebuilt by re-reading the rendered message.
#[test]
fn error_data_keeps_a_value_that_has_no_read_syntax() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\") (markerp (nth 1 (cdr (should-error (forward-char (point-marker)))))))"),
        "t"
    );
}

/// Emacs curls these through `format-message`; three sites had straight quotes.
#[test]
fn char_table_range_uses_curly_quotes() {
    assert_eq!(
        eval("(char-table-range (make-char-table 'test) 'foo)"),
        "!(error \"Invalid RANGE argument to \u{2018}char-table-range\u{2019}\")"
    );
    assert_eq!(
        eval("(set-char-table-range (make-char-table 'test) 'foo 1)"),
        "!(error \"Invalid RANGE argument to \u{2018}set-char-table-range\u{2019}\")"
    );
}

/// chartab.c signals `args_out_of_range`; the invented "Invalid number of extra
/// slots" appears in no Emacs corpus at all.
#[test]
fn make_char_table_rejects_a_bad_slot_count_with_args_out_of_range() {
    assert_eq!(
        eval("(progn (put 'zfoo 'char-table-extra-slots 99) (make-char-table 'zfoo nil))"),
        "!(args-out-of-range 99 nil)"
    );
}

/// `pcase-exhaustive` quotes the value; `map-put!` has its own condition.
#[test]
fn pcase_exhaustive_and_map_not_inplace() {
    assert_eq!(
        eval("(pcase-exhaustive 5 (1 t))"),
        "!(error \"No clause matching \u{2018}5\u{2019}\")"
    );
    assert_eq!(
        eval("(let ((m (list (cons 1 2)))) (map-put! m 3 4))"),
        "!(map-not-inplace ((1 . 2)))"
    );
    assert_eq!(
        eval("(get 'map-not-inplace 'error-message)"),
        "Cannot modify map in-place"
    );
}

/// Five invented `rx:` diagnostics, and one form that is not an error at all.
#[test]
fn rx_diagnostics_use_emacs_wording() {
    assert_eq!(
        eval("(rx-to-string 'zzz)"),
        "!(error \"Unknown rx symbol \u{2018}zzz\u{2019}\")"
    );
    assert_eq!(
        eval("(rx-to-string '(zzz))"),
        "!(error \"Unknown rx form \u{2018}zzz\u{2019}\")"
    );
    assert_eq!(
        eval("(rx-to-string '(syntax zzz))"),
        "!(error \"Unknown rx syntax name \u{2018}zzz\u{2019}\")"
    );
    assert_eq!(
        eval("(rx-to-string [1 2])"),
        "!(error \"Bad rx expression: [1 2]\")"
    );
    assert_eq!(
        eval("(rx-to-string '(?a . ?b))"),
        "!(error \"Bad rx operator \u{2018}97\u{2019}\")"
    );
    // `(not CHAR)` is the complement of one character, not an error.
    assert_eq!(eval("(rx-to-string '(not ?a) t)"), "[^a]");
}

// ── buffer / format / replace-match contracts ────────────────────────────────

/// buffer.c refuses the empty name instead of making an unnameable buffer.
#[test]
fn empty_buffer_name_is_refused() {
    assert_eq!(
        eval("(get-buffer-create \"\")"),
        "!(error \"Empty string for buffer name is not allowed\")"
    );
    assert_eq!(
        eval("(generate-new-buffer \"\")"),
        "!(error \"Empty string for buffer name is not allowed\")"
    );
}

/// A `%` with no conversion character is an error, not a literal `%`.
#[test]
fn trailing_percent_in_a_format_string_signals() {
    assert_eq!(
        eval("(format \"%\")"),
        "!(error \"Format string ends in middle of format specifier\")"
    );
    assert_eq!(
        eval("(format \"abc%\")"),
        "!(error \"Format string ends in middle of format specifier\")"
    );
    // A real conversion still works, and `%%` is still a literal percent.
    assert_eq!(eval("(format \"%d%%\" 7)"), "7%");
}

/// `validate_region` signals rather than clamping, and the buffer object leads
/// the DATA.
#[test]
fn buffer_substring_out_of_range_signals() {
    assert_eq!(
        eval("(with-current-buffer (get-buffer-create \"zb\") (insert \"abc\") (buffer-substring 1 999))"),
        "!(args-out-of-range #<buffer zb> 1 999)"
    );
    assert_eq!(
        eval("(with-current-buffer (get-buffer-create \"zb\") (insert \"abc\") (buffer-substring 0 2))"),
        "!(args-out-of-range #<buffer zb> 0 2)"
    );
    // An inverted but in-range pair is still swapped, not rejected.
    assert_eq!(
        eval("(with-current-buffer (get-buffer-create \"zb\") (insert \"abc\") (buffer-substring 3 1))"),
        "ab"
    );
}

/// Match data outlives the buffer it was set in, and indexing the stale span
/// aborted the interpreter thread. It must signal instead.
///
/// The *numbers* are elisprs's own: each engine starts with different leftover
/// match data (`(match-data)` is `(0 3)` in Emacs, `(9 10)` here), so only the
/// condition symbol is Emacs's.
#[test]
fn replace_match_with_a_stale_span_signals_instead_of_panicking() {
    let out = eval("(with-temp-buffer (insert \"abc\") (replace-match \"x\"))");
    assert!(
        out.starts_with("!(args-out-of-range "),
        "stale buffer span must signal, got {out:?}"
    );
    let out = eval("(replace-match \"x\" nil nil \"ab\")");
    assert!(
        out.starts_with("!(args-out-of-range "),
        "stale string span must signal, got {out:?}"
    );
    // A real match still replaces.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (re-search-forward \"b\") (replace-match \"ZZ\") (buffer-string))"),
        "aZZc"
    );
    assert_eq!(
        eval("(progn (string-match \"b\" \"abc\") (replace-match \"X\" nil nil \"abc\"))"),
        "aXc"
    );
}

/// search.c's two-element `error` DATA, which the render-and-re-read path
/// cannot express.
#[test]
fn replace_match_missing_subexpression_carries_the_index() {
    assert_eq!(
        eval("(progn (string-match \"b\" \"abc\") (replace-match \"X\" nil nil \"abc\" 5))"),
        "!(error \"replace-match subexpression does not exist\" 5)"
    );
}

// ── ERT ordering and the temp buffer ─────────────────────────────────────────

/// Emacs's ERT runs tests in name order, not definition order.
#[test]
fn ert_runs_tests_in_name_order() {
    reset_host();
    let out = eval_str(
        "(let ((ran nil)) \
           (ert-deftest zzz-c () (push 'zzz-c ran)) \
           (ert-deftest aaa-a () (push 'aaa-a ran)) \
           (ert-deftest mmm-b () (push 'mmm-b ran)) \
           (ert-run-tests-batch) \
           (nreverse ran))",
    )
    .expect("eval failed");
    assert_eq!(print(&out, true), "(aaa-a mmm-b zzz-c)");
}

/// `ert--run-test-internal` wraps the body in `with-temp-buffer`, so a body runs
/// in ` *temp*` under the standard syntax table — measured identical in Emacs:
/// `body-buf=" *temp*" body-dot=46 scratch-dot=95`.
#[test]
fn ert_runs_a_test_body_in_a_temp_buffer() {
    reset_host();
    let out = eval_str(
        "(let ((seen nil)) \
           (ert-deftest probe () \
             (setq seen (list (buffer-name) (char-syntax ?.) \
                              (with-current-buffer \"*scratch*\" (char-syntax ?.))))) \
           (ert-run-tests-batch) \
           seen)",
    )
    .expect("eval failed");
    assert_eq!(print(&out, true), "(\" *temp*\" 46 95)");
}
