//! What an `rx` character alternative prints as, and who controls greediness.
//!
//! `rx--charset` concatenated its arguments in the order given. That is only
//! accidentally right: Emacs sorts the characters, merges adjacent ones into
//! ranges, moves `]` to the front and `^`/`-` to the back, and drops the
//! brackets entirely for a single character.
//!
//! ```text
//!                           emacs          elisprs (before)
//! (rx (in ?a ?b "0-9"))     "[0-9ab]"      "[ab0-9]"
//! (rx (in "abc"))           "[a-c]"        "[abc]"
//! (rx (any ?a))             "a"            "[a]"
//! (rx (any "]"))            "]"            "[]]"
//! (rx (any "^-a"))          "[_-a^]"       "[^-a]"
//! ```
//!
//! `rx--string-to-intervals`, `rx--condense-intervals`, `rx--parse-any` and
//! `rx--generate-alt` are ports of the rx.el functions of those names.
//!
//! Every expectation is `emacs -Q --batch` on the installed GNU Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

/// Characters are sorted and adjacent ones merge into a range — and a set that
/// collapses to ONE character loses its brackets.
#[test]
fn a_character_set_is_sorted_and_condensed() {
    assert_eq!(eval("(rx (in ?a ?b \"0-9\"))"), "\"[0-9ab]\"");
    assert_eq!(eval("(rx (any \"a-z\" ?A))"), "\"[Aa-z]\"");
    assert_eq!(eval("(rx (any ?z ?a ?m))"), "\"[amz]\"");
    assert_eq!(eval("(rx (in \"abc\"))"), "\"[a-c]\"");
    assert_eq!(eval("(rx (any \"abcdef\"))"), "\"[a-f]\"");
    assert_eq!(eval("(rx (any \"abc\" \"xyz\"))"), "\"[a-cx-z]\"");
    assert_eq!(eval("(rx (in \"0-9\" \"A-F\" \"a-f\"))"), "\"[0-9A-Fa-f]\"");
    assert_eq!(eval("(rx (any (?a . ?c) ?x))"), "\"[a-cx]\"");
    // One character: no brackets, and `regexp-quote`d.
    assert_eq!(eval("(rx (any ?a))"), "\"a\"");
    assert_eq!(eval("(rx (any \"]\"))"), "\"]\"");
    assert_eq!(eval("(rx (any \"^\"))"), "\"\\\\^\"");
}

/// The three characters that cannot sit just anywhere in a bracket expression:
/// `]` goes first, `^` and `-` go last, and `--x` / `,--` are split.
#[test]
fn the_bracket_metacharacters_are_placed_as_emacs_places_them() {
    assert_eq!(eval("(rx (any \"-a\"))"), "\"[a-]\"");
    assert_eq!(eval("(rx (any \"a-\"))"), "\"[a-]\"");
    assert_eq!(eval("(rx (any ?- ?a))"), "\"[a-]\"");
    assert_eq!(eval("(rx (any \"]-a\"))"), "\"[]-a]\"");
    assert_eq!(eval("(rx (any \"^a\"))"), "\"[a^]\"");
    assert_eq!(eval("(rx (any \"a^\"))"), "\"[a^]\"");
    assert_eq!(eval("(rx (any \"^-a\"))"), "\"[_-a^]\"");
    assert_eq!(eval("(rx (any \",-\"))"), "\"[,-]\"");
    assert_eq!(eval("(rx (any \"--/\"))"), "\"[./-]\"");
}

/// Named classes follow the intervals, and an empty or negated-empty set has
/// its own spellings.
#[test]
fn classes_follow_intervals_and_the_empty_set_is_named() {
    assert_eq!(eval("(rx (any alpha ?z))"), "\"[z[:alpha:]]\"");
    assert_eq!(eval("(rx (any \"a-c\" alpha))"), "\"[a-c[:alpha:]]\"");
    assert_eq!(eval("(rx (any alpha))"), "\"[[:alpha:]]\"");
    // Nothing at all is the never-matching regexp; its negation is any char.
    assert_eq!(eval("(rx (any))"), "\"\\\\`a\\\\`\"");
    assert_eq!(eval("(rx (not (any)))"), "\"[^z-a]\"");
    assert_eq!(eval("(rx (or))"), "\"\\\\`a\\\\`\"");
    // A single `or` branch is that branch, with no shy group.
    assert_eq!(eval("(rx (or \"a\"))"), "\"a\"");
    assert_eq!(eval("(rx (or ?a))"), "\"a\"");
}

/// `not` runs the same machinery negated, and knows the non-charset forms.
#[test]
fn not_negates_through_the_same_machinery() {
    assert_eq!(eval("(rx (not (any \"a\")))"), "\"[^a]\"");
    assert_eq!(eval("(rx (not ?a))"), "\"[^a]\"");
    assert_eq!(eval("(rx (not (any \"]\")))"), "\"[^]]\"");
    assert_eq!(eval("(rx (not (any \"^\")))"), "\"[^^]\"");
    // A single negated newline is `.`, not a bracket expression.
    assert_eq!(eval("(rx (not (any \"\\n\")))"), "\".\"");
    assert_eq!(eval("(rx (not alpha))"), "\"[^[:alpha:]]\"");
    assert_eq!(eval("(rx (not word-boundary))"), "\"\\\\B\"");
    assert_eq!(eval("(rx (not (syntax word)))"), "\"\\\\W\"");
    assert_eq!(eval("(rx (not (syntax whitespace)))"), "\"\\\\S-\"");
    assert_eq!(eval("(rx (not (not (any \"a-z\"))))"), "\"[a-z]\"");
}

/// `(category NAME)` was an error; the rx.el name table is ported.
#[test]
fn categories_translate_to_their_backslash_c_character() {
    assert_eq!(eval("(rx (category latin))"), "\"\\\\cl\"");
    assert_eq!(eval("(rx (category ascii))"), "\"\\\\ca\"");
    assert_eq!(eval("(rx (not (category latin)))"), "\"\\\\Cl\"");
}

/// `minimal-match`/`maximal-match` set the greediness of every quantifier in
/// their body — but only the LONG spellings consult it. rx.el's dispatch gives
/// `*`/`+`/`?` their own greediness and `*?`/`+?`/`??` their own, which is why
/// `(rx (minimal-match (+ "a")))` stays greedy while `(opt "a")` does not.
#[test]
fn greediness_control_reaches_only_the_long_spellings() {
    assert_eq!(eval("(rx (minimal-match (one-or-more \"a\")))"), "\"a+?\"");
    assert_eq!(eval("(rx (maximal-match (one-or-more \"a\")))"), "\"a+\"");
    assert_eq!(eval("(rx (minimal-match (opt \"a\")))"), "\"a??\"");
    assert_eq!(
        eval("(rx (minimal-match (zero-or-more \"ab\")))"),
        "\"\\\\(?:ab\\\\)*?\""
    );
    assert_eq!(
        eval("(rx (minimal-match (one-or-more \"a\") (zero-or-more \"b\")))"),
        "\"a+?b*?\""
    );
    // The operator spellings state their own greediness and ignore it.
    assert_eq!(eval("(rx (minimal-match (+ \"a\")))"), "\"a+\"");
    assert_eq!(eval("(rx (*? \"a\"))"), "\"a*?\"");
    assert_eq!(eval("(rx (+? \"a\"))"), "\"a+?\"");
    // A counted repetition takes no greediness suffix at all.
    assert_eq!(
        eval("(rx (minimal-match (** 2 3 \"a\")))"),
        "\"a\\\\{2,3\\\\}\""
    );
    assert_eq!(eval("(rx (minimal-match (= 2 \"a\")))"), "\"a\\\\{2\\\\}\"");
}
