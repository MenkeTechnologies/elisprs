//! Text properties follow the characters they are on, through every function
//! that copies those characters into a new string: `concat`, `substring`, the
//! case functions and a `%s` format argument.
//!
//! Emacs stores string properties in interval trees that its C primitives copy
//! along with the text; elisprs stores per-character plists in a side table and
//! carries them the same way (`ElispHost::string_carry_props`). Every
//! expectation below matches GNU Emacs 30.2 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn concat_carries_each_argument_s_properties() {
    assert_eq!(
        eval(r#"(concat (propertize "ab" 'face 'bold) "cd")"#),
        r#"#("abcd" 0 2 (face bold))"#
    );
    assert_eq!(
        eval(r#"(concat "x" (propertize "y" 'p 1) "z")"#),
        r#"#("xyz" 1 2 (p 1))"#
    );
    assert_eq!(
        eval(r#"(concat (propertize "a" 'p 1) (propertize "b" 'q 2))"#),
        r#"#("ab" 0 1 (p 1) 1 2 (q 2))"#
    );
    // A char-list argument contributes characters with no properties, and the
    // offsets after it have to account for them.
    assert_eq!(
        eval(r#"(concat '(?x ?y) (propertize "z" 'p 1))"#),
        r#"#("xyz" 2 3 (p 1))"#
    );
}

#[test]
fn substring_carries_the_slice_and_no_properties_drops_them() {
    assert_eq!(
        eval(r#"(substring (propertize "abcd" 'face 'bold) 1 3)"#),
        r#"#("bc" 0 2 (face bold))"#
    );
    assert_eq!(
        eval(r#"(substring (propertize "abcd" 'face 'bold) 2)"#),
        r#"#("cd" 0 2 (face bold))"#
    );
    assert_eq!(
        eval(r#"(substring-no-properties (propertize "ab" 'p 1))"#),
        r#""ab""#
    );
}

#[test]
fn the_case_functions_carry_them_character_for_character() {
    assert_eq!(
        eval(r#"(upcase (propertize "ab" 'p 1))"#),
        r#"#("AB" 0 2 (p 1))"#
    );
    assert_eq!(
        eval(r#"(downcase (propertize "AB" 'p 1))"#),
        r#"#("ab" 0 2 (p 1))"#
    );
    // `capitalize` is written in elisp and rebuilds its result character by
    // character, so it carries them explicitly.
    assert_eq!(
        eval(r#"(capitalize (propertize "ab cd" 'p 1))"#),
        r#"#("Ab Cd" 0 5 (p 1))"#
    );
}

#[test]
fn a_format_s_argument_carries_its_properties_and_padding_follows_emacs() {
    assert_eq!(
        eval(r#"(format "x%sy" (propertize "A" 'p 1))"#),
        r#"#("xAy" 1 2 (p 1))"#
    );
    assert_eq!(
        eval(r#"(format "x%sy%sz" (propertize "A" 'p 1) (propertize "B" 'q 2))"#),
        r#"#("xAyBz" 1 2 (p 1) 3 4 (q 2))"#
    );
    // Padding that FOLLOWS the argument is inside its interval; padding that
    // precedes it is outside.
    assert_eq!(
        eval(r#"(format "%-4s|" (propertize "ab" 'p 1))"#),
        r#"#("ab  |" 0 4 (p 1))"#
    );
    assert_eq!(
        eval(r#"(format "%4s|" (propertize "ab" 'p 1))"#),
        r#"#("  ab|" 2 4 (p 1))"#
    );
    // A precision truncates the text and its properties with it.
    assert_eq!(
        eval(r#"(format "%.1s" (propertize "abc" 'p 1))"#),
        r#"#("a" 0 1 (p 1))"#
    );
    // `%S` prints the argument's read syntax, which is not the argument's own
    // characters, so nothing is carried.
    assert_eq!(
        eval(r#"(format "%S" (propertize "ab" 'p 1))"#),
        "\"#(\\\"ab\\\" 0 2 (p 1))\""
    );
}

#[test]
fn a_property_whose_value_is_nil_is_still_an_interval() {
    // The plist `(p nil)` is not the same as having no properties, so the two
    // do not merge into one run — a key absent from a plist also reads as nil,
    // which is what the interval merger has to look past.
    assert_eq!(
        eval(r#"(concat (propertize "ab" 'p nil) "c")"#),
        r#"#("abc" 0 2 (p nil))"#
    );
    assert_eq!(
        eval(r#"(get-text-property 0 'p (propertize "ab" 'p nil))"#),
        "nil"
    );
}

#[test]
fn properties_are_reachable_by_index_after_a_copy() {
    assert_eq!(
        eval(r#"(get-text-property 2 'p (concat "xy" (propertize "z" 'p 9)))"#),
        "9"
    );
    assert_eq!(
        eval(r#"(get-text-property 0 'p (concat "xy" (propertize "z" 'p 9)))"#),
        "nil"
    );
}
