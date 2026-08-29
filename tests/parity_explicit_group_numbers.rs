//! `\(?N:RE\)` names a capture group explicitly, and renumbers what follows.
//!
//! elisprs compiles elisp regexps to `fancy_regex`, which has no
//! explicit-numbering syntax and numbers capture groups positionally. The
//! translator used to drop the `N:` and emit a plain `(`, betting that explicit
//! numbers are always sequential — so every non-sequential pattern reported the
//! wrong group under `match-data`, `match-beginning`, `match-end` and `\N` in a
//! `replace-regexp-in-string` replacement, with no error to show for it.
//!
//! Emacs's `regex-emacs.c` assigns the group number `N` and then sets
//! `regnum = N`, so counting *continues from N + 1*: `\(?5:a\)\(b\)` has groups
//! 5 and 6, and groups 1–4 exist but never match. `regexp::translate_groups` now
//! reports the Emacs number of each emitted group and `run_match` scatters the
//! spans onto them.
//!
//! Every expectation below is `emacs -Q --batch --eval '(prin1 …)'` on
//! GNU Emacs 30.2.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// The base case the old translator got wrong in the simplest possible way: one
/// explicitly-numbered group leaves group 1 unmatched and fills group 2.
/// Emacs 30.2: `(0 1 nil nil 0 1)`; elisprs answered `(0 1 0 1)`.
#[test]
fn an_explicit_number_leaves_the_lower_groups_unmatched() {
    assert_eq!(
        eval(r#"(progn (string-match "\\(?2:a\\)" "a") (match-data))"#),
        "(0 1 nil nil 0 1)"
    );
    assert_eq!(
        eval(r#"(progn (string-match "\\(?3:a\\)" "a") (match-data))"#),
        "(0 1 nil nil nil nil 0 1)"
    );
}

/// `match-beginning`/`match-end` read the same registers, so they have to move
/// with the group and not with the position. Emacs 30.2: `(nil 0)` — the old
/// positional numbering answered `(0 nil)`, i.e. exactly inverted.
#[test]
fn match_beginning_follows_the_explicit_number() {
    assert_eq!(
        eval(
            r#"(progn (string-match "\\(?2:a\\)" "a")
                      (list (match-beginning 1) (match-beginning 2)))"#
        ),
        "(nil 0)"
    );
}

/// Counting continues from `N + 1`, so a plain group after an explicit one is
/// *not* the next positional group. Emacs 30.2 for `\(?5:a\)\(b\)` on "ab":
/// group 5 is (0 . 1) and group 6 is (1 . 2).
#[test]
fn plain_groups_after_an_explicit_one_continue_from_it() {
    assert_eq!(
        eval(r#"(progn (string-match "\\(?5:a\\)\\(b\\)" "ab") (match-data))"#),
        "(0 2 nil nil nil nil nil nil nil nil 0 1 1 2)"
    );
    assert_eq!(
        eval(r#"(progn (string-match "\\(?2:a\\)\\(b\\)" "ab") (match-data))"#),
        "(0 2 nil nil 0 1 1 2)"
    );
    // The explicit group may also come second, after ordinary counting started.
    assert_eq!(
        eval(r#"(progn (string-match "\\(a\\)\\(?5:b\\)" "ab") (match-data))"#),
        "(0 2 0 1 nil nil nil nil nil nil 1 2)"
    );
}

/// Two groups may claim the same number. In Emacs they share one register, so
/// the branch that matched wins and the one that did not must not clear it —
/// which is why the remap skips unmatched groups instead of assigning nil.
#[test]
fn duplicate_numbers_share_one_register() {
    // Both match: the later assignment stands. Emacs 30.2: `(0 2 1 2)`.
    assert_eq!(
        eval(r#"(progn (string-match "\\(?1:a\\)\\(?1:b\\)" "ab") (match-data))"#),
        "(0 2 1 2)"
    );
    // Only the second alternative matches; it must not be erased by the first.
    // Emacs 30.2: `(0 1 0 1)`.
    assert_eq!(
        eval(r#"(progn (string-match "\\(?1:a\\)\\|\\(?1:b\\)" "b") (match-data))"#),
        "(0 1 0 1)"
    );
}

/// A replacement's `\N` reads the same registers, so the fix has to reach
/// `replace-regexp-in-string` too. Emacs 30.2: `"[a]"`; elisprs answered `"[]"`
/// because group 2 was empty under positional numbering.
#[test]
fn replacement_backreferences_use_the_explicit_number() {
    assert_eq!(
        eval(r#"(replace-regexp-in-string "\\(?2:a\\)" "[\\2]" "a")"#),
        "\"[a]\""
    );
}

/// Group 0 is the whole match and cannot be named, and `\(?` followed by
/// anything other than a digit run or `:` is not elisp syntax at all. Emacs 30.2
/// reports its generic `(invalid-regexp "Invalid regular expression")` for both;
/// the second used to surface `fancy_regex`'s own parser text instead.
#[test]
fn malformed_group_prefixes_report_emacs_message() {
    assert_eq!(
        eval(r#"(condition-case e (string-match "\\(?0:a\\)" "a") (error e))"#),
        "(invalid-regexp \"Invalid regular expression\")"
    );
    assert_eq!(
        eval(r#"(condition-case e (string-match "\\(?a:x\\)" "x") (error e))"#),
        "(invalid-regexp \"Invalid regular expression\")"
    );
}

/// The ordinary shapes must be untouched: a shy group still consumes no number
/// and plain groups still count from 1. This is the control for the remap —
/// `compile_cf` skips it entirely when the numbering is already the identity.
#[test]
fn plain_and_shy_groups_are_unchanged() {
    assert_eq!(
        eval(r#"(progn (string-match "\\(a\\)\\(b\\)" "ab") (match-data))"#),
        "(0 2 0 1 1 2)"
    );
    assert_eq!(
        eval(r#"(progn (string-match "\\(?:a\\)\\(b\\)" "ab") (match-data))"#),
        "(0 2 1 2)"
    );
    // Trailing groups that never matched are still trimmed, as Emacs trims them.
    assert_eq!(
        eval(r#"(progn (string-match "\\(a\\)\\(b\\)?" "a") (match-data))"#),
        "(0 1 0 1)"
    );
}

/// An explicit number only ever RAISES Emacs's group counter. `regex-emacs.c`
/// keeps `bufp->re_nsub` as the running maximum (`if (regnum > bufp->re_nsub)
/// bufp->re_nsub = regnum`) and a plain `\(` takes `++bufp->re_nsub`, so a
/// number BELOW the count reached so far leaves the counter alone. The
/// translator instead set its counter to `N + 1` unconditionally, which made the
/// next plain group reuse a number already taken and dropped a register from
/// `match-data`. Emacs 31.1 for `\(a\)\(b\)\(?1:c\)\(d\)` on "abcd":
/// `(0 4 2 3 1 2 3 4)` — elisprs answered `(0 4 2 3 3 4)`. (31.1 rather than the
/// 30.2 this file otherwise names, because 30.2 was not installed; the
/// `regex-emacs.c` lines are identical on the emacs-30 branch.)
#[test]
fn an_explicit_number_below_the_count_does_not_rewind_it() {
    assert_eq!(
        eval(r#"(progn (string-match "\\(a\\)\\(b\\)\\(?1:c\\)\\(d\\)" "abcd") (match-data))"#),
        "(0 4 2 3 1 2 3 4)"
    );
}

/// A back reference names an EMACS group number, and fancy-regex numbers its
/// captures positionally — so `\(?3:a\)\3` has one emitted group, numbered 3
/// there and 1 here, and passing `\3` through made the crate refuse the pattern
/// outright. A number explicit numbering skipped over is a register no group
/// ever writes, and Emacs's `duplicate` on an unset register always fails.
#[test]
fn back_references_are_remapped_to_the_emitted_group() {
    assert_eq!(eval(r#"(string-match "\\(?3:a\\)\\3" "aa")"#), "0");
    assert_eq!(
        eval(r#"(progn (string-match "\\(?3:a\\)\\3" "aa") (match-data))"#),
        "(0 2 nil nil nil nil 0 1)"
    );
    assert_eq!(eval(r#"(string-match "\\(?3:a\\)\\3\\3" "aaa")"#), "0");
    assert_eq!(
        eval(r#"(progn (string-match "\\(?2:a\\)\\2" "aa") (match-data))"#),
        "(0 2 nil nil 0 1)"
    );
    // Group 1 exists as a number but nothing assigns it, so `\1` never matches.
    assert_eq!(eval(r#"(string-match "\\(?2:a\\)\\1" "aa")"#), "nil");
    assert_eq!(eval(r#"(string-match "\\(?2:a\\)\\1" "a")"#), "nil");
}
