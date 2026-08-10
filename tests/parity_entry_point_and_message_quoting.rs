//! Entry-point state and the `format-message` family, measured against
//! `GNU Emacs 30.2` (`emacs -Q --batch -l FILE`, the invocation `elisp FILE`
//! models — see `scripts/fuzz_parity.sh`).
//!
//! **Hidden buffer names.** `generate-new-buffer-name` for a name that begins
//! with a space appends a *random* `-NNNNNN` before falling back to the `<N>`
//! counter (buffer.c, "See bug#1229"), which is why a nested `load` under Emacs
//! is ` *load*-711551` and not ` *load*<2>`. elisprs walked the counter for every
//! name. Its `IGNORE` argument — a name that may be reused even though a buffer
//! holds it — was accepted and dropped, which also silently disabled
//! `(rename-buffer NAME t)`.
//!
//! **`skip-syntax-forward` / `skip-syntax-backward`.** Absent. They are the
//! syntax-class counterparts of `skip-chars-*` and the base of most
//! word/symbol motion, so `(void-function skip-syntax-forward)` is what every
//! library that scans by class hit.
//!
//! **`format-message` quoting.** `error`, `user-error` and `message` all format
//! through `Fformat_message`, so the default `text-quoting-style` of `curve`
//! turns `` ` `` and `'` in the *template* into `‘` and `’`. elisprs used plain
//! `format` for all three. `substitute-command-keys` has the opposite problem: it
//! *does* honor the `\=` escape, which `format-message` does not, and elisprs
//! implemented both as the same plain replacement.
//!
//! **`message` in batch.** `(message nil)` clears the echo area and answers nil
//! rather than signalling, and `message_to_stderr` first flushes
//! `noninteractive_need_newline` — set by any batch write to stdout — so a
//! `princ` and a following `message` never share a line.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The message a *script* sees for SRC's error, i.e. through `condition-case` +
/// `error-message-string`, which is what `emacs -Q --batch` was measured with.
fn err(src: &str) -> String {
    let v = eval(&format!(
        "(condition-case e {src} (t (error-message-string e)))"
    ));
    v.trim_matches('"').to_string()
}

/// A hidden name that is taken gets a random numeric suffix, not `<2>`.
///
/// Ground truth, GNU Emacs 30.2:
///
/// ```text
/// $ emacs -Q --batch --eval '(progn (get-buffer-create " *h*") \
///     (prin1 (buffer-name (generate-new-buffer " *h*"))))'
/// " *h*-368192"
/// ```
///
/// Six consecutive draws in one Emacs process were all distinct
/// (`-605143 -857340 -810307 -543373 -889436 -796658`), so this is a fresh
/// random each call and not a hash of the name.
#[test]
fn a_taken_hidden_buffer_name_gets_a_random_suffix() {
    // The suffix shape: ` *h*-` followed by 1..6 decimal digits (`r % 1000000`).
    assert_eq!(
        eval(
            r#"(progn (get-buffer-create " *h*")
                 (let ((s (generate-new-buffer-name " *h*")))
                   (list (string-prefix-p " *h*-" s)
                         (integerp (string-to-number (substring s 5)))
                         (<= (length s) 11))))"#
        ),
        "(t t t)"
    );
    // Distinct draws, so the `<N>` counter is not what is answering.
    assert_eq!(
        eval(
            r#"(progn (get-buffer-create " *h*")
                 (let ((a (generate-new-buffer-name " *h*"))
                       (b (generate-new-buffer-name " *h*")))
                   (equal a b)))"#
        ),
        "nil"
    );
    // A *visible* name still walks the counter — the random suffix is gated on
    // the leading space, not on "the name is taken".
    assert_eq!(
        eval(r#"(progn (get-buffer-create "abc") (generate-new-buffer-name "abc"))"#),
        "\"abc<2>\""
    );
    // A free hidden name is returned unchanged (the `Fget_buffer` check runs
    // before the random branch), which is why `with-temp-buffer` is ` *temp*`.
    assert_eq!(
        eval(r#"(generate-new-buffer-name " *free*")"#),
        "\" *free*\""
    );
    assert_eq!(eval("(with-temp-buffer (buffer-name))"), "\" *temp*\"");
}

/// IGNORE names a buffer that may be reused even though it exists — for NAME
/// itself and for every `<N>` candidate.
///
/// Emacs 30.2, with buffers `abc` and `abc<2>` live:
/// `(generate-new-buffer-name "abc" "abc<2>")` => `"abc<2>"`, while
/// `(generate-new-buffer-name "abc" "qqq")` => `"abc<3>"`.
#[test]
fn generate_new_buffer_name_honors_ignore() {
    assert_eq!(
        eval(r#"(progn (get-buffer-create "abc") (generate-new-buffer-name "abc" "abc"))"#),
        "\"abc\""
    );
    assert_eq!(
        eval(
            r#"(progn (get-buffer-create "abc") (get-buffer-create "abc<2>")
                 (generate-new-buffer-name "abc" "abc<2>"))"#
        ),
        "\"abc<2>\""
    );
    // An unrelated IGNORE changes nothing, and neither does nil.
    assert_eq!(
        eval(
            r#"(progn (get-buffer-create "abc") (get-buffer-create "abc<2>")
                 (list (generate-new-buffer-name "abc" "qqq")
                       (generate-new-buffer-name "abc" nil)))"#
        ),
        "(\"abc<3>\" \"abc<3>\")"
    );
}

/// `rename-buffer`'s UNIQUE argument, and the empty-name error.
///
/// Emacs 30.2: `(rename-buffer "taken" t)` => `"taken<2>"`, `(rename-buffer
/// "taken")` on a taken name errors, and renaming a buffer to the name it
/// already has succeeds in both spellings (UNIQUE passes the current name as
/// IGNORE, so the `<N>` loop hands it straight back).
#[test]
fn rename_buffer_takes_a_unique_argument() {
    assert_eq!(
        eval(r#"(progn (get-buffer-create "taken") (with-temp-buffer (rename-buffer "taken" t)))"#),
        "\"taken<2>\""
    );
    assert_eq!(
        err(r#"(progn (get-buffer-create "taken") (with-temp-buffer (rename-buffer "taken")))"#),
        "Buffer name \u{2018}taken\u{2019} is in use"
    );
    assert_eq!(
        eval(r#"(with-temp-buffer (rename-buffer "self") (rename-buffer "self"))"#),
        "\"self\""
    );
    assert_eq!(
        eval(r#"(with-temp-buffer (rename-buffer "s2") (rename-buffer "s2" t))"#),
        "\"s2\""
    );
    assert_eq!(
        err(r#"(with-temp-buffer (rename-buffer ""))"#),
        "Empty string is invalid as a buffer name"
    );
}

/// `skip-syntax-forward` walks the classes its SYNTAX string names.
///
/// Buffer text `"ab .;()  cd"` in a temp buffer (standard syntax table).
/// Ground truth, GNU Emacs 30.2, as `(DISTANCE POINT)` pairs:
///
/// ```text
/// "w"   => (2 3)    "w-"  => (3 4)    "w "  => (3 4)    ""    => (0 1)
/// "Z"   => (0 1)    "wZ"  => (2 3)
/// ```
///
/// `-` is `syntax_spec_code`'s alias for whitespace, and a character that names
/// no class (`Z`) addresses a fastmap slot no real class can match, so it
/// neither contributes nor signals.
#[test]
fn skip_syntax_forward_walks_the_named_classes() {
    let b =
        |body: &str| format!(r#"(with-temp-buffer (insert "ab .;()  cd") (goto-char 1) {body})"#);
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w\") (point))")),
        "(2 3)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w-\") (point))")),
        "(3 4)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w \") (point))")),
        "(3 4)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"\") (point))")),
        "(0 1)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"Z\") (point))")),
        "(0 1)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"wZ\") (point))")),
        "(2 3)"
    );
    // A non-string SYNTAX is `CHECK_STRING`, before anything moves.
    assert_eq!(
        err("(with-temp-buffer (skip-syntax-forward 5))"),
        "Wrong type argument: stringp, 5"
    );
}

/// A leading `^` complements the class set, and LIM is clamped to the accessible
/// portion.
///
/// Emacs 30.2 on the same buffer: `"^"` from 1 => `(11 12)` (every class is in
/// the complement of the empty set, so it runs to point-max); `"^w"` from 3 =>
/// `(7 10)`; LIM 2 => `(1 2)`; LIM 999 => clamped to point-max; LIM -5 =>
/// clamped to point-min, so a forward skip travels nothing.
#[test]
fn skip_syntax_negates_and_clamps_its_limit() {
    let b =
        |body: &str| format!(r#"(with-temp-buffer (insert "ab .;()  cd") (goto-char 1) {body})"#);
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"^\") (point))")),
        "(11 12)"
    );
    assert_eq!(
        eval(&b(
            "(goto-char 3) (list (skip-syntax-forward \"^w\") (point))"
        )),
        "(7 10)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w\" 2) (point))")),
        "(1 2)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w \" 999) (point))")),
        "(3 4)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-forward \"w\" -5) (point))")),
        "(0 1)"
    );
    // A marker is an accepted LIM (`CHECK_FIXNUM_COERCE_MARKER`).
    assert_eq!(
        eval(&b(
            "(let ((m (copy-marker 3))) (list (skip-syntax-forward \"w \" m) (point)))"
        )),
        "(2 3)"
    );
    // Narrowing bounds the scan even when LIM is past it.
    assert_eq!(
        eval(&b(
            "(narrow-to-region 1 2) (goto-char 1) (list (skip-syntax-forward \"w\" 99) (point))"
        )),
        "(1 2)"
    );
    // At point-max there is nothing to travel.
    assert_eq!(
        eval(&b(
            "(goto-char (point-max)) (list (skip-syntax-forward \"w\") (point))"
        )),
        "(0 12)"
    );
}

/// `skip-syntax-backward` returns a negative distance and stops at LIM.
///
/// Emacs 30.2 on `"ab .;()  cd"`: from 3, `"w"` => `(-2 1)`; from 9, `".()"`
/// => `(0 9)` (the char before point is a space); from 3 with LIM 2 => `(-1 2)`;
/// from 9, `"^w"` => `(-6 3)`; at point-min => `(0 1)`.
#[test]
fn skip_syntax_backward_travels_negative() {
    let b =
        |body: &str| format!(r#"(with-temp-buffer (insert "ab .;()  cd") (goto-char 1) {body})"#);
    assert_eq!(
        eval(&b(
            "(goto-char 3) (list (skip-syntax-backward \"w\") (point))"
        )),
        "(-2 1)"
    );
    assert_eq!(
        eval(&b(
            "(goto-char 9) (list (skip-syntax-backward \".()\") (point))"
        )),
        "(0 9)"
    );
    assert_eq!(
        eval(&b(
            "(goto-char 3) (list (skip-syntax-backward \"w\" 2) (point))"
        )),
        "(-1 2)"
    );
    assert_eq!(
        eval(&b(
            "(goto-char 9) (list (skip-syntax-backward \"^w\") (point))"
        )),
        "(-6 3)"
    );
    assert_eq!(
        eval(&b("(list (skip-syntax-backward \"w\") (point))")),
        "(0 1)"
    );
}

/// The skip reads the *current buffer's* syntax table, not the standard one.
///
/// This is the same distinction round 3 pinned for `char-syntax`: `elisp FILE`
/// runs where `emacs -l FILE` does, whose table makes `?.` a symbol
/// constituent, so `"w_"` crosses a dot that the standard table stops at.
/// Verified on GNU Emacs 30.2 with `emacs -Q --batch -l probe.el`: the initial
/// buffer's table answers `7` for `(skip-syntax-forward "w_")` over
/// `"foo.bar ;c"`, and the standard table answers `3`.
#[test]
fn skip_syntax_reads_the_buffers_own_table() {
    let over = |setup: &str| {
        format!(
            r#"(with-temp-buffer {setup} (insert "foo.bar ;c") (goto-char 1)
                 (skip-syntax-forward "w_"))"#
        )
    };
    assert_eq!(
        eval(&over("(set-syntax-table emacs-lisp-mode-syntax-table)")),
        "7"
    );
    assert_eq!(
        eval(&over("(set-syntax-table (standard-syntax-table))")),
        "3"
    );
}

/// `error` and `user-error` curve-quote the template, and only the template.
///
/// Ground truth, GNU Emacs 30.2, reading the signalled message out of the error
/// object so no printing layer can be doing the work:
///
/// ```text
/// (error "a `b' c")        => "a ‘b’ c"
/// (error "a %s" "x `y'")   => "a x `y'"     ; the ARGUMENT keeps its quotes
/// (error "a \\=`b c")      => "a \\=‘b c"   ; `\=' is not an escape here
/// (error "50%% `q'")       => "50% ‘q’"
/// (user-error "a `b'")     => "a ‘b’"
/// (signal 'error '("a `b'")) => "a `b'"     ; `signal' does not format at all
/// ```
#[test]
fn error_and_user_error_format_through_format_message() {
    let msg = |src: &str| eval(&format!("(condition-case e {src} (t (cadr e)))"));
    assert_eq!(msg(r#"(error "a `b' c")"#), "\"a \u{2018}b\u{2019} c\"");
    assert_eq!(msg(r#"(error "a %s" "x `y'")"#), "\"a x `y'\"");
    assert_eq!(msg(r#"(error "a \\=`b c")"#), "\"a \\\\=\u{2018}b c\"");
    assert_eq!(msg(r#"(error "50%% `q'")"#), "\"50% \u{2018}q\u{2019}\"");
    assert_eq!(msg(r#"(user-error "a `b'")"#), "\"a \u{2018}b\u{2019}\"");
    // The control: `signal` hands its data through untouched, and plain `format`
    // never curves — so the change is in the two that go through
    // `Fformat_message`, not in the printer.
    assert_eq!(msg(r#"(signal 'error (list "a `b'"))"#), "\"a `b'\"");
    assert_eq!(eval(r#"(format "a `b' c")"#), "\"a `b' c\"");
}

/// `substitute-command-keys` honors `\=`; `format-message` does not.
///
/// Ground truth, GNU Emacs 30.2:
///
/// ```text
/// (substitute-command-keys "a \\=`b")    => "a `b"
/// (format-message           "a \\=`b")   => "a \\=‘b"
/// (substitute-command-keys "a \\=\\= b") => "a \\= b"
/// (substitute-command-keys "a \\=\\[f] b") => "a \\[f] b"
/// (substitute-command-keys "a \\=' b")   => "a ' b"
/// (substitute-command-keys "a \\=")      => "a \\="      ; nothing follows it
/// ```
#[test]
fn substitute_command_keys_honors_the_backslash_equals_escape() {
    assert_eq!(eval(r#"(substitute-command-keys "a \\=`b")"#), "\"a `b\"");
    assert_eq!(
        eval(r#"(format-message "a \\=`b")"#),
        "\"a \\\\=\u{2018}b\""
    );
    assert_eq!(
        eval(r#"(substitute-command-keys "a \\=\\= b")"#),
        "\"a \\\\= b\""
    );
    assert_eq!(
        eval(r#"(substitute-command-keys "a \\=\\[f] b")"#),
        "\"a \\\\[f] b\""
    );
    assert_eq!(eval(r#"(substitute-command-keys "a \\=' b")"#), "\"a ' b\"");
    assert_eq!(eval(r#"(substitute-command-keys "a \\=")"#), "\"a \\\\=\"");
    // Unescaped quotes still curve, and nil still comes back as nil (error
    // reporting funnels an absent `error-message` property through here).
    assert_eq!(
        eval(r#"(substitute-command-keys "`a' b")"#),
        "\"\u{2018}a\u{2019} b\""
    );
    assert_eq!(eval("(substitute-command-keys nil)"), "nil");
}

/// `message` answers a nil or empty template unchanged instead of signalling.
///
/// Emacs 30.2: `(message nil)` => nil, `(message "")` => `""`, `(message t)` =>
/// `(wrong-type-argument stringp t)`. The first was
/// `(wrong-type-argument stringp nil)` here, which is the error the *third* is
/// supposed to have exclusively.
#[test]
fn message_accepts_a_nil_template() {
    assert_eq!(eval("(message nil)"), "nil");
    assert_eq!(eval(r#"(message "")"#), "\"\"");
    assert_eq!(err("(message t)"), "Wrong type argument: stringp, t");
    // And its return value is curve-quoted like `error`'s.
    assert_eq!(eval(r#"(message "a `b' c")"#), "\"a \u{2018}b\u{2019} c\"");
    assert_eq!(eval(r#"(message "a %s" "`b'")"#), "\"a `b'\"");
}
