//! Parity gaps closed in round 9: printing a cycle with `print-circle` NIL
//! (print.c's Brent tail marker and its `being_printed` back-reference),
//! `print-level`'s cons-only scope, `print-circle` label NUMBERING order,
//! `Fnthcdr` on a bignum index and on a circular list, and the Unicode reach of
//! Emacs's `[:class:]` character classes.
//!
//! Every expectation here is the output of GNU Emacs 30.2 for the same form —
//! `emacs -Q --batch -l PROBE` — not of the running interpreter.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// A cons cell built into a cycle of PERIOD p, with RHO leading cells before the
/// cycle entry, printed with `print-circle` nil.
fn cyc(rho: usize, lam: usize) -> String {
    eval(&format!(
        "(let* ((cyc (number-sequence 0 {})) \
                (pre (if (= {rho} 0) nil (number-sequence -{} -1))) \
                (head (append pre cyc))) \
           (setcdr (nthcdr {} head) (nthcdr {rho} head)) (prin1-to-string head))",
        lam - 1,
        rho.max(1),
        rho + lam - 1,
    ))
}

/// print.c prints ` . #N` from Brent's teleporting tortoise when `print-circle`
/// is nil, where N is `tortoise_idx` — the tortoise's position at its LAST
/// teleport, not the cycle's period. Because the teleport takes precedence over
/// the equality test, N only ever takes the values 2^k - 2 (0, 2, 6, 14, …), and
/// how far the list is unrolled before the marker depends on both RHO and the
/// period. elisprs used to abort the whole print with "Apparently circular
/// structure being printed" instead.
#[test]
fn print_circle_nil_emits_brents_tail_marker() {
    // The pure cycles: rho = 0, period 1..8. The index schedule is
    // 0, 2, 2, 6, 6, 6, 6, 14 — and the ELEMENTS printed before it differ per
    // period, which is what makes this more than a table of seven constants.
    assert_eq!(
        eval("(let ((x (list 0))) (setcdr x x) (prin1-to-string x))"),
        "\"(0 . #0)\""
    );
    assert_eq!(cyc(0, 2), "\"(0 1 0 1 . #2)\"");
    assert_eq!(cyc(0, 3), "\"(0 1 2 0 1 . #2)\"");
    assert_eq!(cyc(0, 4), "\"(0 1 2 3 0 1 2 3 0 1 . #6)\"");
    assert_eq!(cyc(0, 5), "\"(0 1 2 3 4 0 1 2 3 4 0 . #6)\"");
    assert_eq!(cyc(0, 6), "\"(0 1 2 3 4 5 0 1 2 3 4 5 . #6)\"");
    assert_eq!(cyc(0, 7), "\"(0 1 2 3 4 5 6 0 1 2 3 4 5 . #6)\"");
    assert_eq!(
        cyc(0, 8),
        "\"(0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 . #14)\""
    );

    // A non-empty RHO shifts the whole schedule: the same period 1 cycle reports
    // #2 behind one leading cell and #6 behind three, because the tortoise has
    // already teleported once or twice by the time the hare enters the cycle.
    assert_eq!(cyc(1, 1), "\"(-1 0 0 . #2)\"");
    assert_eq!(cyc(2, 1), "\"(-2 -1 0 . #2)\"");
    assert_eq!(cyc(3, 1), "\"(-3 -2 -1 0 0 0 0 . #6)\"");
    assert_eq!(cyc(3, 5), "\"(-3 -2 -1 0 1 2 3 4 0 1 2 . #6)\"");

    // `print-length` is decremented BEFORE the tortoise runs, so a short limit
    // wins over the marker.
    assert_eq!(
        eval(
            "(let ((l (list 1 2 3))) (setcdr (nthcdr 2 l) l) \
               (let ((print-length 4)) (prin1-to-string l)))"
        ),
        "\"(1 2 3 1 ...)\""
    );
}

/// The other half of print.c's `NILP (Vprint_circle)` arm: `being_printed[]`, a
/// scan of the objects currently open at every shallower depth. An object that is
/// its own ancestor prints as `#I` — no dot, no trailing `#` — where I is that
/// depth. This is what terminates a cycle that closes through a CAR or through a
/// vector, record or hash-table slot rather than through the cdr chain.
#[test]
fn print_circle_nil_emits_being_printed_backreference() {
    assert_eq!(
        eval("(let ((x (list 1))) (setcar x x) (prin1-to-string x))"),
        "\"(#0)\""
    );
    assert_eq!(
        eval("(let ((x (list 1 2))) (setcar (cdr x) x) (prin1-to-string x))"),
        "\"(1 #0)\""
    );
    assert_eq!(
        eval("(let ((v (vector 1 2))) (aset v 0 v) (prin1-to-string v))"),
        "\"[#0 2]\""
    );
    assert_eq!(
        eval("(let ((r (record 'foo 1))) (aset r 1 r) (prin1-to-string r))"),
        "\"#s(foo #0)\""
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'k h h) (prin1-to-string h))"),
        "\"#s(hash-table data (k #0))\""
    );
    // Reached from a list element rather than the head: the index is still 0,
    // because the cdr walk does not add a level.
    assert_eq!(
        eval("(let ((a (list 1 2 3))) (setcar (nthcdr 2 a) a) (prin1-to-string a))"),
        "\"(1 2 #0)\""
    );
}

/// The `PRINT_CIRCLE` = 200 depth ceiling lives INSIDE print.c's
/// `if (NILP (Vprint_circle))`. With the label table on there is no ceiling at
/// all, because every container that could close a cycle carries a `#N=` instead.
/// elisprs applied the ceiling unconditionally, so a deep-but-finite nest
/// signalled under `print-circle` t where Emacs prints it.
#[test]
fn print_circle_depth_ceiling_is_gated_on_print_circle_nil() {
    let deep = "(let ((x nil)) (dotimes (_ 250) (setq x (list x))) x)";
    assert_eq!(
        eval(&format!(
            "(let ((print-circle t)) (substring (prin1-to-string {deep}) 0 20))"
        )),
        "\"((((((((((((((((((((\""
    );
    // With `print-circle` nil the ceiling still applies, and still signals.
    assert_eq!(
        eval(&format!(
            "(condition-case e (prin1-to-string {deep}) (error (car (cdr e))))"
        )),
        "\"Apparently circular structure being printed\""
    );
}

/// print.c tests `Vprint_level` in exactly one place — `case Lisp_Cons` — so only
/// a LIST is ever replaced by `...`. A vector or record still costs a level
/// (`print_depth++` runs for every object) but can never be truncated itself.
#[test]
fn print_level_truncates_conses_only() {
    assert_eq!(
        eval("(let ((print-level 2)) (prin1-to-string [[[[1]]]]))"),
        "\"[[[[1]]]]\""
    );
    assert_eq!(
        eval("(let ((print-level 2)) (prin1-to-string (record 'a (record 'b (record 'c 1)))))"),
        "\"#s(a #s(b #s(c 1)))\""
    );
    // A list nested the same distance down IS truncated, and the vectors it sits
    // inside still count toward the depth that truncates it.
    assert_eq!(
        eval("(let ((print-level 2)) (prin1-to-string '((((1))))))"),
        "\"((...))\""
    );
    assert_eq!(
        eval("(let ((print-level 2)) (prin1-to-string '(([[(1)]]))))"),
        "\"(([[...]]))\""
    );
    assert_eq!(
        eval("(let ((print-level 3)) (prin1-to-string '(1 [2 (3 [4 (5)])])))"),
        "\"(1 [2 (3 [4 ...])])\""
    );
}

/// print.c assigns each `#N=` label in `print_preprocess`, at the moment an
/// object is met for the SECOND time in a car-before-cdr DFS — not when it is
/// finally printed. The two orders differ, and the difference is visible whenever
/// the object printed first is met-twice later than one printed after it.
#[test]
fn print_circle_labels_are_numbered_in_preprocess_order() {
    // The record is printed first but numbered #2; the cons printed later inside
    // the second vector is met twice earlier in the traversal and gets #1.
    assert_eq!(
        eval(
            "(let* ((print-circle t) (r (record 'r (record 'r 0))) (c (cons 9 9))) \
               (prin1-to-string (list (vector r (vector c (record 'r 3))) c (list r 3))))"
        ),
        "\"([#2=#s(r #s(r 0)) [#1=(9 . 9) #s(r 3)]] #1# (#2# 3))\""
    );
    // Plain first-come order still holds when the two orders agree.
    assert_eq!(
        eval(
            "(let* ((print-circle t) (a (list 1)) (b (list 2))) (prin1-to-string (list a b a b)))"
        ),
        "\"(#1=(1) #2=(2) #1# #2#)\""
    );
}

/// fns.c `Fnthcdr` runs `CHECK_INTEGER`, which accepts a bignum, and guards the
/// walk with Brent's tortoise so a circular LIST terminates. elisprs rejected a
/// bignum index outright and spun forever on a cycle.
#[test]
fn nthcdr_accepts_bignum_indices_and_terminates_on_cycles() {
    // 4611686018427387903 is past elisprs's fixnum range, so it arrives as a
    // bignum; walking off the end of a short list is nil, not a type error.
    assert_eq!(eval("(nth 4611686018427387903 '(a b c))"), "nil");
    assert_eq!(eval("(nthcdr (floor 1.5e+300) '(a))"), "nil");
    // A negative bignum returns LIST untouched, as `Fnthcdr` does before walking.
    assert_eq!(eval("(nthcdr (- (floor 1.5e+300)) '(a b))"), "(a b)");
    assert_eq!(eval("(nthcdr -5 '(a b))"), "(a b)");

    // Circular: the answer is the cycle position the full walk would reach, found
    // by reducing the remaining count modulo the distance the hare travelled.
    assert_eq!(
        eval("(let ((l (number-sequence 0 6))) (setcdr (nthcdr 6 l) l) (car (nthcdr 300 l)))"),
        "6"
    );
    assert_eq!(
        eval("(let ((l (number-sequence 0 6))) (setcdr (nthcdr 6 l) l) (car (nthcdr 301 l)))"),
        "0"
    );
    // A cycle entered after a non-empty prefix, indexed by a bignum.
    assert_eq!(
        eval(
            "(let ((l (number-sequence 0 7))) (setcdr (nthcdr 7 l) (nthcdr 3 l)) \
               (car (nthcdr (+ most-positive-fixnum 7) l)))"
        ),
        "3"
    );

    // The type and list-end contracts are unchanged.
    assert_eq!(
        eval("(condition-case e (nthcdr 1.5 '(a b)) (error e))"),
        "(wrong-type-argument integerp 1.5)"
    );
    assert_eq!(
        eval("(condition-case e (nthcdr 2 '(1 . 2)) (error e))"),
        "(wrong-type-argument listp (1 . 2))"
    );
    assert_eq!(eval("(nthcdr 1 '(1 . 2))"), "2");
}

/// subr.el's `split-string` applies TRIM as a regexp at both ends of every
/// substring, and its default SEPARATORS are six ASCII characters — not
/// "whitespace" in the Unicode sense. TRIM was previously ignored outright.
#[test]
fn split_string_applies_and_type_checks_trim() {
    assert_eq!(
        eval("(condition-case e (split-string \"abc\" \"b\" nil 97) (error e))"),
        "(wrong-type-argument stringp 97)"
    );
    assert_eq!(
        eval("(split-string \"  a  b  \" \",\" nil \" +\")"),
        "(\"a  b\")"
    );
    assert_eq!(eval("(split-string \"  ,  ,a\" \",\" t \" +\")"), "(\"a\")");
    // Trimming can empty a substring; OMIT-NULLS is re-checked afterwards.
    assert_eq!(
        eval("(split-string \"xx,xx\" \",\" nil \"x+\")"),
        "(\"\" \"\")"
    );
    assert_eq!(eval("(split-string \"xx,xx\" \",\" t \"x+\")"), "nil");
    // A leading TRIM that runs past the end of the segment leaves
    // this-start > this-end, and `substring` signals rather than yielding "".
    assert_eq!(
        eval("(condition-case e (split-string \"aXb\" \"X\" nil \"a.\") (error e))"),
        "(args-out-of-range \"aXb\" 2 1)"
    );
    // A no-break space is not one of the default separators.
    assert_eq!(eval("(split-string \"a\u{a0}b\")"), "(\"a\u{a0}b\")");
    assert_eq!(
        eval("(equal split-string-default-separators \"[ \\f\\t\\n\\r\\v]+\")"),
        "t"
    );
}

/// Emacs's `[:class:]` names are defined over the whole character set, not just
/// ASCII, so the `regex` crate's own POSIX classes were the wrong target.
#[test]
fn posix_char_classes_reach_beyond_ascii() {
    // Letters outside ASCII, including a combining mark.
    assert_eq!(eval("(string-match \"[[:alpha:]]\" \"Ü\")"), "0");
    assert_eq!(eval("(string-match \"[[:alpha:]]\" \"α\")"), "0");
    assert_eq!(eval("(string-match \"[[:alpha:]]\" (string 768))"), "0");
    assert_eq!(eval("(string-match \"[[:alnum:]]\" \"Ü\")"), "0");
    // `[:cntrl:]` is ASCII 0-31; DEL is NOT a control character to Emacs.
    assert_eq!(eval("(string-match \"[[:cntrl:]]\" \"\u{7f}\")"), "nil");
    assert_eq!(eval("(string-match \"[[:cntrl:]]\" \"\u{1}\")"), "0");
    // The Emacs-only byte-width classes, which the crate does not know at all.
    assert_eq!(eval("(string-match \"[[:nonascii:]]\" \"a\")"), "nil");
    assert_eq!(eval("(string-match \"[[:nonascii:]]\" \"Ü\")"), "0");
    assert_eq!(eval("(string-match \"[[:multibyte:]]\" \"。\")"), "0");
    assert_eq!(eval("(string-match \"[[:unibyte:]]\" \"a\")"), "0");
    // A class used as a separator now consumes the whole non-ASCII word.
    assert_eq!(eval("(split-string \"ÜñîçøðÉ\" \"[[:alpha:]]+\" t)"), "nil");
}

/// seq.el checks TYPE before it touches SEQUENCE, and `seq-split` is not an alias
/// for `seq-partition` — the two disagree on a non-positive length.
#[test]
fn seq_into_checks_type_first_and_seq_split_signals() {
    assert_eq!(
        eval("(condition-case e (seq-into 0 'foo) (error e))"),
        "(error \"Not a sequence type name: foo\")"
    );
    assert_eq!(
        eval("(condition-case e (seq-into 1.5 'car) (error e))"),
        "(error \"Not a sequence type name: car\")"
    );
    assert_eq!(
        eval("(condition-case e (seq-into 0 'list) (error e))"),
        "(wrong-type-argument sequencep 0)"
    );
    assert_eq!(
        eval("(condition-case e (seq-split '(\"a\") 0) (error e))"),
        "(error \"Sub-sequence length must be larger than zero\")"
    );
    // `seq-partition` keeps its own, opposite guard.
    assert_eq!(eval("(seq-partition '(1 2 3) 0)"), "nil");
    assert_eq!(eval("(seq-split '(1 2 3 4 5) 2)"), "((1 2) (3 4) (5))");
    assert_eq!(eval("(seq-split \"abcde\" 2)"), "(\"ab\" \"cd\" \"e\")");
}

/// fns.c `Fstring_version_lessp` takes `SYMBOL_NAME` for a symbol and then
/// `CHECK_STRING`s both operands, so `t` and `car` are legal arguments while a
/// number is `stringp`, not `sequencep`.
#[test]
fn string_version_lessp_accepts_symbols_and_checks_strings() {
    assert_eq!(eval("(string-version-lessp \"ab\" t)"), "t");
    assert_eq!(eval("(string-version-lessp 'foo2 'foo12)"), "t");
    assert_eq!(
        eval("(condition-case e (string-version-lessp 0 \"Hello\") (error e))"),
        "(wrong-type-argument stringp 0)"
    );
    assert_eq!(
        eval("(condition-case e (string-version-lessp \"Hello\" 97) (error e))"),
        "(wrong-type-argument stringp 97)"
    );
    assert_eq!(
        eval("(condition-case e (string-version-lessp '(1 2) 0) (error e))"),
        "(wrong-type-argument stringp (1 2))"
    );
    assert_eq!(
        eval("(string-version-lessp \"foo2.png\" \"foo12.png\")"),
        "t"
    );
}
