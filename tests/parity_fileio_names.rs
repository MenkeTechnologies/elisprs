//! The file-name primitives are fileio.c's (Unix branch), and the file-name
//! functions built on them are files.el's and env.el's.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`, each form
//! wrapped in `(condition-case e FORM (error e))`). Names are absolute or use
//! users that exist everywhere (`root`) or nowhere, so the results do not
//! depend on the machine's home or working directory.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

fn err(src: &str) -> String {
    eval(&format!("(condition-case e {src} (error e))"))
}

/// `directory_file_name` strips every trailing slash but leaves "//" alone,
/// and the walks reject a non-string with `stringp`.
#[test]
fn directory_and_nondirectory_walks() {
    assert_eq!(eval("(directory-file-name \"/a/b///\")"), "\"/a/b\"");
    assert_eq!(eval("(directory-file-name \"//\")"), "\"//\"");
    assert_eq!(eval("(directory-file-name \"///\")"), "\"/\"");
    assert_eq!(eval("(file-name-as-directory \"\")"), "\"./\"");
    assert_eq!(eval("(file-name-directory \"a/b/\")"), "\"a/b/\"");
    assert_eq!(eval("(file-name-directory \"a\")"), "nil");
    for f in [
        "file-name-directory",
        "file-name-nondirectory",
        "directory-file-name",
        "file-name-absolute-p",
        "expand-file-name",
    ] {
        assert_eq!(
            err(&format!("({f} 'a)")),
            "(wrong-type-argument stringp a)",
            "{f}"
        );
    }
}

/// `file-name-concat` skips nil and "", and checks every other element.
#[test]
fn file_name_concat_skips_nil_and_checks_strings() {
    assert_eq!(eval("(file-name-concat nil \"a\")"), "\"a\"");
    assert_eq!(eval("(file-name-concat \"a\" \"\" \"b\" nil)"), "\"a/b\"");
    assert_eq!(eval("(file-name-concat \"a//\" \"b\")"), "\"a//b\"");
    assert_eq!(
        err("(file-name-concat \"a\" 'b)"),
        "(wrong-type-argument stringp b)"
    );
}

/// `~USER` is absolute only for a user that exists; an unknown one leaves
/// the name relative, so expansion joins it to the default directory.
#[test]
fn tilde_user_needs_a_real_user() {
    assert_eq!(eval("(file-name-absolute-p \"~root/a\")"), "t");
    assert_eq!(eval("(file-name-absolute-p \"~nobodyxyz/x\")"), "nil");
    assert_eq!(
        eval("(expand-file-name \"~nobodyxyz/x\" \"/d/\")"),
        "\"/d/~nobodyxyz/x\""
    );
    assert_eq!(
        eval("(string-prefix-p \"/\" (expand-file-name \"~root/x\" \"/d/\"))"),
        "t"
    );
}

/// The canonicalization keeps the "superroot" `/..`, a leading "//", and
/// an already-expanded name as the very same object.
#[test]
fn expand_file_name_canonicalizes_as_fileio_does() {
    assert_eq!(eval("(expand-file-name \"/..\")"), "\"/..\"");
    assert_eq!(eval("(expand-file-name \"/a/b/../../../c\")"), "\"/../c\"");
    assert_eq!(eval("(expand-file-name \"//a\")"), "\"//a\"");
    assert_eq!(eval("(expand-file-name \"///a\")"), "\"/a\"");
    assert_eq!(eval("(expand-file-name \"x\" \"//\")"), "\"//x\"");
    assert_eq!(eval("(expand-file-name \"/.\")"), "\"/\"");
    assert_eq!(eval("(expand-file-name \"a\" 'b)"), "\"/a\"");
    assert_eq!(
        eval("(let ((s \"/a/b\")) (eq s (expand-file-name s)))"),
        "t"
    );
    assert_eq!(
        eval("(let ((s \"/a//b\")) (eq s (expand-file-name s)))"),
        "nil"
    );
    assert_eq!(
        eval("(let ((default-directory \"/x/y/\")) (expand-file-name \"z\"))"),
        "\"/x/y/z\""
    );
    assert_eq!(
        eval("(condition-case e (expand-file-name \"a\\0b\") (error (list (car e) (cadr e) (length (caddr e)))))"),
        "(wrong-type-argument filenamep 3)"
    );
}

/// files.el removes version and backup suffixes before taking the extension,
/// and `file-name-split` is its component loop.
#[test]
fn files_el_extension_and_split() {
    assert_eq!(eval("(file-name-extension \"foo.c~\")"), "\"c\"");
    assert_eq!(eval("(file-name-extension \"foo.c.~1~\")"), "\"c\"");
    assert_eq!(eval("(file-name-sans-extension \"foo.c.~1~\")"), "\"foo\"");
    assert_eq!(eval("(file-name-split \"//a//b\")"), "(\"/\" \"a\" \"b\")");
    assert_eq!(eval("(file-name-split \"\")"), "nil");
    assert_eq!(eval("(file-name-split \"/\")"), "(\"\" \"\" \"\")");
    assert_eq!(eval("(file-name-unquote \"/:\")"), "\"/\"");
}

/// `substitute-in-file-name` restarts at an embedded absolute name and leaves
/// an undefined variable as written.
#[test]
fn substitute_in_file_name() {
    assert_eq!(eval("(substitute-in-file-name \"/a//b\")"), "\"/b\"");
    assert_eq!(
        eval("(substitute-in-file-name \"/a/~root/b\")"),
        "\"~root/b\""
    );
    assert_eq!(
        eval("(substitute-in-file-name \"/a/~nobodyxyz/b\")"),
        "\"/a/~nobodyxyz/b\""
    );
    assert_eq!(
        eval("(substitute-in-file-name \"$NOPEXYZ/a\")"),
        "\"$NOPEXYZ/a\""
    );
    assert_eq!(eval("(substitute-in-file-name \"a$$b\")"), "\"a$b\"");
    assert_eq!(
        eval("(let ((s \"abc\")) (eq s (substitute-in-file-name s)))"),
        "t"
    );
    assert_eq!(
        eval("(progn (setenv \"ELQ\" \"/q\") (substitute-in-file-name \"x/$ELQ/y\"))"),
        "\"/q/y\""
    );
}

/// `abbreviate-file-name` replaces the home directory with `~`, but only at a
/// directory boundary.
#[test]
fn abbreviate_file_name_uses_the_home_regexp() {
    assert_eq!(eval("(abbreviate-file-name (getenv \"HOME\"))"), "\"~\"");
    assert_eq!(
        eval("(abbreviate-file-name (concat (getenv \"HOME\") \"/x\"))"),
        "\"~/x\""
    );
    assert_eq!(
        eval("(let ((h (getenv \"HOME\"))) (equal (abbreviate-file-name (concat h \"x/y\")) (concat h \"x/y\")))"),
        "t"
    );
    assert_eq!(
        eval("(let ((directory-abbrev-alist '((\"\\\\`/tmp/\" . \"/T/\")))) (abbreviate-file-name \"/tmp/a\"))"),
        "\"/T/a\""
    );
}
