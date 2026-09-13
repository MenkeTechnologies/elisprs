//! The reader's `#N=` / `#N#` labels (`read0`'s `RE_numbered`, lread.c:4248-4268
//! and 4552-4606) and the `#`-then-digits form they share with `#NrDIGITS`.
//!
//! Before this, `#1=(1 2 . #1#)` reached `read_radix` — which had already
//! consumed the digits and demanded an `r` — and came back as
//! "malformed radix literal". The printer had emitted `#N=` since the
//! `print-circle` work, so elisprs printed syntax its own reader rejected and
//! nothing it printed with a label could be read back.
//!
//! Every expectation is the output of `emacs -Q --batch` for the same form,
//! except where a case is marked as the measured 30.2/31.1 drift.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    reset_host();
    match eval_str(src) {
        Ok(v) => panic!("expected a signal, got {}", print(&v, true)),
        Err(e) => e,
    }
}

/// A label on a CONS is closed by repurposing the placeholder: Emacs copies the
/// read object's car and cdr into the placeholder cons and hands that back, so
/// every `#N#` read inside the object already points at the result and no
/// rewriting is needed (lread.c:4562-4576).
#[test]
fn a_cons_label_closes_onto_its_own_placeholder() {
    // Circular through the cdr: the tail IS the head.
    assert_eq!(eval("(let ((o '#1=(1 2 . #1#))) (eq o (cddr o)))"), "t");
    assert_eq!(eval("(nth 7 '#1=(1 2 . #1#))"), "2");
    // Circular through the car.
    assert_eq!(eval("(let ((o '#1=(#1# 2))) (eq o (car o)))"), "t");
    // Both slots of one cons.
    assert_eq!(
        eval("(let ((o '#1=(#1# . #1#))) (and (eq o (car o)) (eq o (cdr o))))"),
        "t"
    );
    // Two labels that close through each other.
    assert_eq!(
        eval("(let ((o '#1=(1 . #2=(2 . #1#)))) (eq o (cdr (cdr o))))"),
        "t"
    );
    // Shared but NOT circular: one object named twice, which must stay ONE
    // object rather than being copied.
    assert_eq!(eval("(let ((o '(#1=(a) #1#))) (eq (car o) (cadr o)))"), "t");
    assert_eq!(
        eval("(let ((o '[#1=(1) #1#])) (eq (aref o 0) (aref o 1)))"),
        "t"
    );
}

/// A label on anything that is NOT a cons cannot repurpose the placeholder, so
/// Emacs rewrites every reference to the placeholder inside the object
/// (`substitute_object_recurse`, lread.c:4632-4708) and repoints the label at
/// the object itself.
#[test]
fn a_non_cons_label_substitutes_the_placeholder_away() {
    assert_eq!(eval("(let ((o '#1=[1 #1#])) (eq o (aref o 1)))"), "t");
    assert_eq!(eval("(let ((o '#1=#s(r #1#))) (eq o (aref o 1)))"), "t");
    // Nested one level down: the walk is recursive, not just top-level.
    assert_eq!(
        eval("(let ((o '#1=[1 [2 #1#]])) (eq o (aref (aref o 1) 1)))"),
        "t"
    );
    // Through a text-property plist — lread.c:4693-4702.
    assert_eq!(
        eval("(let ((o '#1=#(\"ab\" 0 2 (p #1#)))) (eq o (get-text-property 0 'p o)))"),
        "t"
    );
    // A label on an atom is just the atom; `#N#` then names it.
    assert_eq!(eval("'#1=5"), "5");
    assert_eq!(eval("'(#1=sym #1#)"), "(sym sym)");
}

/// A labelled object round-trips: what the printer writes, the reader reads back
/// to the same shape. That equivalence is the whole point — before this the two
/// halves disagreed and only the printer half existed.
#[test]
fn print_and_read_round_trip_through_the_label() {
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (car (read-from-string \"#1=(1 2 . #1#)\"))))"),
        "\"#1=(1 2 . #1#)\""
    );
    assert_eq!(
        eval("(let ((print-circle t)) (prin1-to-string (car (read-from-string \"#1=[1 #1#]\"))))"),
        "\"#1=[1 #1#]\""
    );
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string (car (read-from-string \"(#1=(a) #1#)\"))))"
        ),
        "\"(#1=(a) #1#)\""
    );
    // The label NUMBER is chosen by the printer, so a corpus label of `0` comes
    // back as `1` — the structure round-trips, the spelling need not.
    assert_eq!(
        eval(
            "(let ((print-circle t)) (prin1-to-string (car (read-from-string \"#0=(#0# . 2)\"))))"
        ),
        "\"#1=(#1# . 2)\""
    );
}

/// A cycle that runs through a string's TEXT PROPERTIES has to be labelled by
/// the printer too — print.c:1431-1436 preprocesses each interval's plist. The
/// reader could not build one before, but `put-text-property` always could, and
/// printing it recursed until the stack overflowed.
#[test]
fn a_cycle_through_a_text_property_is_labelled() {
    assert_eq!(
        eval(
            "(let ((print-circle t) (s (copy-sequence \"ab\"))) \
              (put-text-property 0 2 'p s s) (prin1-to-string s))"
        ),
        "\"#1=#(\\\"ab\\\" 0 2 (p #1#))\""
    );
    // A value shared by one RUN of characters is one interval, so it is reached
    // once and earns no label. Emacs walks intervals; this heap stores a plist
    // per character, and counting per character would label this `#1=(1)`.
    assert_eq!(
        eval(
            "(let ((print-circle t) (s (copy-sequence \"abc\"))) \
              (put-text-property 0 3 'p '(1) s) (prin1-to-string s))"
        ),
        "\"#(\\\"abc\\\" 0 3 (p (1)))\""
    );
    // The same value in two SEPARATE runs is reached twice, and is labelled.
    assert_eq!(
        eval(
            "(let ((print-circle t) (s (copy-sequence \"abcd\")) (v (list 1))) \
              (put-text-property 0 1 'p v s) (put-text-property 3 4 'p v s) \
              (prin1-to-string s))"
        ),
        "\"#(\\\"abcd\\\" 0 1 (p #1=(1)) 3 4 (p #1#))\""
    );
}

/// A label is scoped to ONE top-level read: Emacs rebuilds `read_objects_map`
/// per read (lread.c:2744-2775), so the second form below cannot see the first
/// form's label.
#[test]
fn a_label_does_not_escape_its_own_top_level_form() {
    assert_eq!(
        err("(progn '#1=(1) nil) (progn '#1#)"),
        "invalid-read-syntax: #1#"
    );
}

/// The error datum is `invalid_syntax (read_buffer, …)` (lread.c:3931-3935):
/// every character the reader had buffered, which is the `#`, the digits, and
/// the character that ended them.
#[test]
fn a_malformed_label_reports_the_characters_it_buffered() {
    assert_eq!(err("'#1z"), "invalid-read-syntax: #1z");
    assert_eq!(err("'#1"), "invalid-read-syntax: #1");
    // A reference to a number nothing was labelled with.
    assert_eq!(err("'#2#"), "invalid-read-syntax: #2#");
    // lread.c:4558-4560 — "Catch silly games like #1=#1#".
    assert_eq!(
        err("'#1=#1#"),
        "invalid-read-syntax: nonsensical self-reference"
    );
    // The count overflows at the digit that overflows it, and the buffer stops
    // there — note there is no `=` in the datum even though the input has one.
    assert_eq!(
        err("'#99999999999999999999=(1)"),
        "invalid-read-syntax: #9999999999999999999"
    );
    // Above `most-positive-fixnum` but not an overflow: the gate at
    // lread.c:4246 rejects it with the FULL buffer, `=` included.
    assert_eq!(
        err("'#2305843009213693952=(1)"),
        "invalid-read-syntax: #2305843009213693952="
    );
    // One below it is a legal label.
    assert_eq!(eval("'#2305843009213693951=(1)"), "(1)");
}

/// `#NrDIGITS` shares the digit scan with `#N=`, so the radix literal has to
/// keep working — and `read_integer` (lread.c:3129-3185) is a faithful port
/// rather than a scan-to-the-next-delimiter, which changes three answers.
#[test]
fn radix_literals_follow_read_integer() {
    assert_eq!(eval("#x1f"), "31");
    assert_eq!(eval("#16rFF"), "255");
    assert_eq!(eval("#b101"), "5");
    assert_eq!(eval("#o17"), "15");
    assert_eq!(eval("#16r-ff"), "-255");
    assert_eq!(eval("#x007"), "7");
    assert_eq!(eval("#36rZZ"), "1295");
    // No width limit: a radix literal that does not fit a fixnum is a bignum.
    assert_eq!(eval("#xFFFFFFFFFFFFFFFFFF"), "4722366482869645213695");
    // `digit_to_number` returns -2 for a character that is not a digit in ANY
    // radix, which ENDS the literal without consuming it: `#x1.5` is the
    // integer 1 followed by `.5`, not a bad token.
    assert_eq!(eval("(read-from-string \"#x1.5\")"), "(1 . 3)");
    // -1 is a letter or digit this radix does not have: consumed, and the
    // literal is invalid. The message names the RADIX, never the digits
    // (`invalid_radix_integer`, lread.c:3115-3122).
    assert_eq!(err("#2r2"), "invalid-read-syntax: integer, radix 2");
    assert_eq!(err("#16rZZ"), "invalid-read-syntax: integer, radix 16");
    assert_eq!(err("#xZZ"), "invalid-read-syntax: integer, radix 16");
    // No digits at all is "incomplete", which is also invalid.
    assert_eq!(err("#x"), "invalid-read-syntax: integer, radix 16");
    assert_eq!(err("#2r"), "invalid-read-syntax: integer, radix 2");
    assert_eq!(err("#37r1"), "invalid-read-syntax: integer, radix 37");
}

/// ORACLE DRIFT, pinned to 30.2. The radix gate is `if (n < 0 || n > 36)` in
/// emacs-30.2 (lread.c:4241-4242) and `if (n < 2 || n > 36)` in emacs-31.1
/// (lread.c:4026-4027). On the pin, radix 0 and 1 reach `read_integer`, whose
/// leading-zero branch accepts an all-zeros spelling; `string_to_number` then
/// reads `0` as digit 0 in radix 1 and finds no digit at all in radix 0. On
/// 31.1 all four of these are `integer, radix N` instead.
#[test]
fn radix_under_two_follows_the_thirty_two_gate() {
    assert_eq!(eval("#1r0"), "0");
    assert_eq!(eval("#1r00"), "0");
    assert_eq!(eval("#0r0"), "nil");
    // A digit the radix cannot supply is rejected on BOTH versions, with the
    // same message — the drift is confined to the all-zeros spelling.
    assert_eq!(err("#1r1"), "invalid-read-syntax: integer, radix 1");
}
