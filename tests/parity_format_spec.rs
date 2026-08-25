//! `format-spec`'s %-spec is `%<flags><width><precision>CHAR`, not `%CHAR`.
//!
//! The previous implementation substituted the character and copied everything
//! else through, so the flag and width syntax fell out as literal text — a
//! silent wrong answer, not an error:
//!
//! ```text
//!                                            emacs 30.2   elisprs (before)
//! (format-spec "%-5a|" '((?a . "x")))        "x    |"     "5a|"
//! (format-spec "%5a|"  '((?a . "x")))        "    x|"     "a|"
//! ```
//!
//! This is a port of format-spec.el (Emacs 30.2), including `insert-and-inherit`
//! — the insert it uses to carry FORMAT's text properties onto each
//! substitution, which elisprs did not have.
//!
//! Every expectation is `emacs -Q --batch` on GNU Emacs 30.2 for the same form.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// Width pads, `-` pads on the right, `0` pads with zeros.
#[test]
fn width_and_padding_flags() {
    assert_eq!(
        eval("(format-spec \"%a-%b\" '((?a . \"x\") (?b . \"y\")))"),
        "\"x-y\""
    );
    assert_eq!(eval("(format-spec \"%5a|\" '((?a . \"x\")))"), "\"    x|\"");
    assert_eq!(
        eval("(format-spec \"%-5a|\" '((?a . \"x\")))"),
        "\"x    |\""
    );
    assert_eq!(
        eval("(format-spec \"%05d|\" '((?d . \"7\")))"),
        "\"00007|\""
    );
    // Flags combine, and `-` decides which END the zeros go on.
    assert_eq!(
        eval("(format-spec \"%0-5a|\" '((?a . \"x\")))"),
        "\"x0000|\""
    );
}

/// `.N` truncates; `<` and `>` choose which end is kept.
#[test]
fn precision_and_truncation_flags() {
    assert_eq!(
        eval("(format-spec \"%.2a|\" '((?a . \"abcdef\")))"),
        "\"ab|\""
    );
    assert_eq!(
        eval("(format-spec \"%<5a|\" '((?a . \"abcdefg\")))"),
        "\"cdefg|\""
    );
    assert_eq!(
        eval("(format-spec \"%>5a|\" '((?a . \"abcdefg\")))"),
        "\"abcde|\""
    );
    assert_eq!(
        eval("(format-spec \"%<.3a|\" '((?a . \"abcdefg\")))"),
        "\"efg|\""
    );
}

/// `^`/`_` change case; a non-string substitution goes through `format "%s"`;
/// a function substitution is called.
#[test]
fn case_flags_and_value_kinds() {
    assert_eq!(eval("(format-spec \"%^a\" '((?a . \"ab\")))"), "\"AB\"");
    assert_eq!(eval("(format-spec \"%_a\" '((?a . \"AB\")))"), "\"ab\"");
    assert_eq!(eval("(format-spec \"%a\" '((?a . 42)))"), "\"42\"");
    assert_eq!(
        eval("(format-spec \"%a\" (list (cons ?a (lambda () \"fn\"))))"),
        "\"fn\""
    );
}

/// Width and precision are DISPLAY COLUMNS, as in `format`: a double-width
/// character fills a 3-column field on its own, and `%.1a` cannot keep half of
/// one.
#[test]
fn width_is_measured_in_display_columns() {
    assert_eq!(eval("(format-spec \"%3a\" '((?a . \"日本\")))"), "\"日本\"");
    assert_eq!(eval("(format-spec \"%.1a\" '((?a . \"日本\")))"), "\"\"");
}

/// IGNORE-MISSING decides what an unknown %-spec does: signal, stay, or vanish.
/// Anything other than nil/`ignore`/`delete` also leaves `%%` alone.
#[test]
fn ignore_missing_selects_the_treatment_of_an_unknown_spec() {
    assert_eq!(
        err("(format-spec \"%z\" '((?a . \"x\")))"),
        "(error \"Invalid format character: \u{2018}%z\u{2019}\")"
    );
    assert_eq!(
        eval("(format-spec \"%z\" '((?a . \"x\")) 'ignore)"),
        "\"%z\""
    );
    assert_eq!(eval("(format-spec \"%z\" '((?a . \"x\")) 'delete)"), "\"\"");
    assert_eq!(
        eval("(format-spec \"a%zb\" '((?a . \"x\")) 'other)"),
        "\"a%zb\""
    );
    assert_eq!(eval("(format-spec \"%%\" '() 'other)"), "\"%%\"");
    // `%%` is one literal percent under every other setting.
    assert_eq!(eval("(format-spec \"%%a\" '((?a . \"x\")))"), "\"%a\"");
    assert_eq!(
        eval("(format-spec \"100%% %a\" '((?a . \"x\")))"),
        "\"100% x\""
    );
    // A `%` that starts no valid spec at all is a different diagnostic.
    assert_eq!(
        err("(format-spec \"% \" '((?a . \"x\")))"),
        "(error \"Invalid format string\")"
    );
    assert_eq!(eval("(format-spec \"\" '((?a . \"x\")))"), "\"\"");
}

/// With SPLIT the result is the pieces, with each substitution its own element.
#[test]
fn split_returns_the_pieces() {
    assert_eq!(
        eval("(format-spec \"x%ay\" '((?a . \"1\")) nil t)"),
        "(\"x\" \"1\" \"y\")"
    );
    assert_eq!(
        eval("(format-spec \"%a%b\" '((?a . \"1\") (?b . \"2\")) nil t)"),
        "(\"1\" \"2\")"
    );
    assert_eq!(
        eval("(format-spec \"%a\" '((?a . \"1\")) nil t)"),
        "(\"1\")"
    );
    assert_eq!(
        eval("(format-spec \"pre%apost\" '((?a . \"1\")) nil t)"),
        "(\"pre\" \"1\" \"post\")"
    );
}

/// FORMAT's text properties are carried onto the substitution. This is what
/// `insert-and-inherit` is for, and why `format-spec` inserts before deleting.
#[test]
fn format_text_properties_reach_the_substitution() {
    assert_eq!(
        eval("(let ((f (propertize \"%a\" 'face 'bold))) (get-text-property 0 'face (format-spec f '((?a . \"x\")))))"),
        "bold"
    );
    assert_eq!(
        eval("(let ((f (concat \"z\" (propertize \"%a\" 'p 1)))) (get-text-property 1 'p (format-spec f '((?a . \"xy\")))))"),
        "1"
    );
}

/// `insert-and-inherit` in its own right: inherit from the character BEFORE
/// point, honour `rear-nonsticky`, and inherit nothing at the buffer start.
#[test]
fn insert_and_inherit_takes_the_preceding_characters_properties() {
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\" 'p 1)) (goto-char 2) (insert-and-inherit \"X\") (get-text-property 2 'p))"),
        "1"
    );
    // Plain `insert` inherits nothing — the two must stay different.
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\" 'p 1)) (goto-char 2) (insert \"X\") (get-text-property 2 'p))"),
        "nil"
    );
    // At the beginning of the buffer there is no preceding character.
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\" 'p 1)) (goto-char 1) (insert-and-inherit \"X\") (get-text-property 1 'p))"),
        "nil"
    );
    // `rear-nonsticky' stops the property at the character's end.
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\" 'p 1 'rear-nonsticky t)) (goto-char 3) (insert-and-inherit \"X\") (get-text-property 3 'p))"),
        "nil"
    );
}

/// `format-spec-make` pairs characters with values, in order.
#[test]
fn format_spec_make_builds_the_alist() {
    assert_eq!(
        eval("(format-spec-make ?a \"x\" ?b \"y\")"),
        "((97 . \"x\") (98 . \"y\"))"
    );
    assert_eq!(
        err("(format-spec-make ?a)"),
        "(error \"Invalid list of pairs\")"
    );
}
