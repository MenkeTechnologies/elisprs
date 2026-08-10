//! Three gaps the `syntax.c` scanner port surfaced, each independent of it.
//!
//! 1. `string-to-syntax` dropped the `c` flag, returned `(13)` for `@` where
//!    Emacs returns nil, reported a different error, and consed a fresh cell
//!    where Emacs shares one per bare class.
//! 2. `cl-defstruct` ignored `:type` entirely, so `(:type list)` and
//!    `(:type vector)` both produced records and their accessors were off by
//!    the tag slot. `syntax.el`'s `ppss` struct is `(:type list)`, which is how
//!    this was found.
//! 3. The higher-order primitives `host::call_function` intercepts by name had
//!    no function cell at all, so `fboundp`, `functionp`, `func-arity`,
//!    `subrp` and `indirect-function` answered as though they were undefined.
//!
//! Every expectation is `emacs -Q --batch` on GNU Emacs 30.2 (with
//! `(require 'cl-lib)` for the struct cases, which elisprs preloads).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn string_to_syntax_flags_inherit_and_errors() {
    // `c' is bit 23 (the third comment style) and was the one flag missing:
    // `. c' was `(1)`, and `w  1234pbnc' was `(8323074)` — the whole descriptor
    // minus that bit.
    assert_eq!(
        eval(
            r#"(list (string-to-syntax ". c") (string-to-syntax "w  1234pbnc")
                     (string-to-syntax "@") (string-to-syntax "-"))"#
        ),
        "((8388609) (16711682) nil (0))"
    );
    // `Fstring_to_syntax' returns Qnil for Sinherit, so `@' is nil, not `(13)`.
    assert_eq!(eval(r#"(string-to-syntax "@")"#), "nil");
}

#[test]
fn a_bare_class_descriptor_returns_the_shared_cons() {
    // syntax.c builds `Vsyntax_code_object' once and hands the same cell back
    // for every flagless, matchless descriptor, so these are `eq'.
    assert_eq!(
        eval(r#"(eq (string-to-syntax "w") (string-to-syntax "w"))"#),
        "t"
    );
}

#[test]
fn string_to_syntax_error_messages() {
    assert_eq!(
        eval(r#"(condition-case e (string-to-syntax "Z") (error e))"#),
        r#"(error "Invalid syntax description letter: Z")"#
    );
    assert_eq!(
        eval(r#"(condition-case e (string-to-syntax 5) (error e))"#),
        "(wrong-type-argument stringp 5)"
    );
}

#[test]
fn cl_defstruct_type_list_builds_a_list() {
    // Constructor makes a list, the accessor is `nth', the slot offset starts
    // at 0 because there is no tag, and no predicate is generated at all.
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (tl (:type list)) a b)
                 (list (make-tl :a 1 :b 2) (tl-a '(7 8)) (fboundp 'tl-p)
                       (cl-struct-slot-offset 'tl 'b)))"#
        ),
        "((1 2) 7 nil 1)"
    );
}

#[test]
fn cl_defstruct_type_vector_builds_an_untagged_vector() {
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (tv (:type vector)) a b)
                 (list (make-tv :a 1 :b 2) (tv-b [7 8])))"#
        ),
        "([1 2] 8)"
    );
}

#[test]
fn cl_defstruct_named_type_list_keeps_the_tag_and_the_predicate() {
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (tn (:type list) :named) a b)
                 (list (make-tn :a 1) (tn-p '(tn 1 2)) (tn-p '(1 2)) (tn-a '(tn 7 8))))"#
        ),
        "((tn 1 nil) t nil 7)"
    );
}

#[test]
fn cl_defstruct_type_list_boa_constructor_defaults_and_copier() {
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (tb (:type list) (:constructor mk-tb (a b))) a b)
                 (mk-tb 7 8))"#
        ),
        "(7 8)"
    );
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (td (:type list)) (a 1) (b 2))
                 (list (make-td) (copy-td '(3 4))))"#
        ),
        "((1 2) (3 4))"
    );
}

#[test]
fn setf_on_a_type_list_slot_is_a_list_position() {
    // A `(:type list)` slot cannot be `aset'; the setter has to be
    // `(setcar (nthcdr INDEX S) V)'.
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct (ts (:type list)) a b)
                 (let ((v (make-ts :a 1 :b 2))) (setf (ts-b v) 9) v))"#
        ),
        "(1 9)"
    );
}

#[test]
fn an_untyped_struct_is_still_a_record() {
    assert_eq!(
        eval(
            r#"(progn (cl-defstruct tr a b) (list (make-tr :a 1) (recordp (make-tr)) (tr-p (make-tr))))"#
        ),
        "(#s(tr 1 nil) t t)"
    );
}

#[test]
fn the_intercepted_higher_order_primitives_have_function_cells() {
    assert_eq!(
        eval(
            r#"(list (fboundp 'mapcar) (functionp 'eval) (func-arity 'funcall)
                     (func-arity 'mapcar) (func-arity 'load)
                     (subrp (symbol-function 'apply)))"#
        ),
        "(t t (1 . many) (2 . 2) (1 . 5) t)"
    );
}

#[test]
fn calling_an_intercepted_primitive_through_its_function_cell_still_works() {
    // The cell exists for introspection; `call_function' has to keep routing
    // the subr object to the intercept, or these deadlock or misbehave.
    assert_eq!(
        eval(
            r#"(list (funcall (symbol-function 'mapcar) #'1+ '(1 2))
                     (apply (symbol-function 'mapc) (list #'ignore '(1)))
                     (funcall (symbol-function 'eval) '(+ 1 2)))"#
        ),
        "((2 3) (1) 3)"
    );
}

#[test]
fn sort_reports_the_subrs_arity_not_a_lisp_shim() {
    // A prelude `(defun sort (lst pred) …)` used to overwrite the cell, so
    // `func-arity' answered `(2 . 2)` and `subrp' nil — while every call went
    // to the native intercept, which handles vectors and the keyword form the
    // shim never did.
    assert_eq!(
        eval(
            r#"(list (func-arity 'sort) (subrp (symbol-function 'sort))
                     (sort (list 3 1 2) #'<) (sort (list 3 1 2) :lessp #'>))"#
        ),
        "((1 . many) t (1 2 3) (3 2 1))"
    );
}

// ---------------------------------------------------------------------------
// `capitalize' / `upcase-initials' (casefiddle.c)
//
// Two bugs, both found by the locale sweep. The word test was "a digit or a
// cased letter" instead of the syntax table's `Sword', and a word's first
// character was upper-cased instead of title-cased. Expectations are
// `emacs -Q --batch --eval', whose buffer (`*scratch*', `lisp-interaction-mode')
// is the one the in-process `eval_str' below also uses.

#[test]
fn a_word_initial_is_title_cased_not_upper_cased() {
    // ß title-cases to "Ss" (two characters) — `upcase' would give ẞ — and ǳ
    // title-cases to the digraph ǲ, which is neither ǳ nor Ǳ.
    assert_eq!(eval(r#"(capitalize "ßäöü")"#), r#""Ssäöü""#);
    assert_eq!(eval(r#"(upcase-initials "ßäöü")"#), r#""Ssäöü""#);
    assert_eq!(eval(r#"(capitalize "ǳa")"#), r#""ǲa""#);
    assert_eq!(eval(r#"(capitalize "ǆungla")"#), r#""ǅungla""#);
}

#[test]
fn a_ligature_starts_a_word_and_expands() {
    // ﬁ has no one-to-one upper case, so the old "cased letter" test called it
    // a non-word character and the *next* letter became the word start:
    // "ﬁnd" answered "ﬁNd".
    assert_eq!(eval(r#"(capitalize "ﬁnd")"#), r#""Find""#);
    assert_eq!(eval(r#"(capitalize "hello ﬁsh")"#), r#""Hello Fish""#);
    assert_eq!(eval(r#"(upcase-initials "ﬁnd")"#), r#""Find""#);
}

#[test]
fn capitalize_downcases_the_rest_and_upcase_initials_does_not() {
    assert_eq!(eval(r#"(capitalize "HELLO WORLD")"#), r#""Hello World""#);
    assert_eq!(
        eval(r#"(upcase-initials "HELLO WORLD")"#),
        r#""HELLO WORLD""#
    );
    // `-' and `_' are symbol constituents, not word ones, so they break words.
    assert_eq!(eval(r#"(capitalize "foo-bar")"#), r#""Foo-Bar""#);
    assert_eq!(eval(r#"(capitalize "foo_bar")"#), r#""Foo_Bar""#);
    // …until `case-symbols-as-words' says otherwise.
    assert_eq!(
        eval(r#"(let ((case-symbols-as-words t)) (capitalize "foo_bar"))"#),
        r#""Foo_bar""#
    );
}

#[test]
fn a_capital_sigma_that_ends_a_word_downcases_to_the_final_sigma() {
    assert_eq!(eval(r#"(capitalize "ΟΔΟΣ")"#), r#""Οδος""#);
    assert_eq!(eval(r#"(capitalize "ΑΣ ΑΣΑ")"#), r#""Ας Ασα""#);
}

#[test]
fn a_character_argument_takes_the_one_to_one_title_mapping() {
    // ǳ has a single title character; ß and ﬁ do not, so they fall back to
    // `upcase' — which for ﬁ is the character itself.
    assert_eq!(
        eval("(list (capitalize ?ǳ) (capitalize ?ß) (capitalize ?ﬁ) (capitalize ?a))"),
        "(498 7838 64257 65)"
    );
    assert_eq!(
        eval("(list (upcase-initials ?ǳ) (upcase-initials ?ß) (upcase-initials ?a))"),
        "(498 7838 65)"
    );
}

#[test]
fn capitalize_keeps_text_properties_when_no_character_expands() {
    assert_eq!(
        eval(r#"(let ((s (propertize "abc def" 'p 1))) (capitalize s))"#),
        r##"#("Abc Def" 0 7 (p 1))"##
    );
}
