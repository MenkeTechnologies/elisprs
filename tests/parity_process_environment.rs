//! callproc.c's `process-environment`, `initial-environment` and
//! `getenv-internal`, env.el's `getenv`/`setenv`/`with-environment-variables`
//! over them, and subprocesses started with `process-environment` as their
//! environment (`make_environment_block`).
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// Both lists start as the process environment, sharing their strings.
#[test]
fn process_environment_lists() {
    assert_eq!(
        eval("(list (consp process-environment) (equal process-environment initial-environment) (eq process-environment initial-environment) (eq (car process-environment) (car initial-environment)))"),
        "(t t nil t)"
    );
    assert_eq!(
        eval("(equal (getenv \"HOME\") (let ((e (seq-find (lambda (s) (string-prefix-p \"HOME=\" s)) process-environment))) (substring e 5)))"),
        "t"
    );
}

/// The first entry naming a variable wins; a lone name means unset, and
/// `getenv-internal` of a list ENV answers t for it.
#[test]
fn getenv_reads_process_environment() {
    assert_eq!(
        eval("(let ((process-environment (cons \"FOOZZ=1\" process-environment))) (getenv \"FOOZZ\"))"),
        "\"1\""
    );
    assert_eq!(
        eval("(let ((process-environment (list \"X=1\" \"X=2\" \"Y\" \"Y=3\"))) (list (getenv \"X\") (getenv \"Y\") (getenv \"NOPE\")))"),
        "(\"1\" nil nil)"
    );
    assert_eq!(
        eval("(list (getenv-internal \"A\" '(\"A\")) (getenv-internal \"A\" '(\"AB=1\" \"A=2\")) (getenv-internal \"A\" '(3 \"A=4\")) (getenv-internal \"A\" '(\"B=1\")))"),
        "(t \"2\" \"4\" nil)"
    );
    assert_eq!(
        eval("(getenv-internal 'a)"),
        "(ERR (wrong-type-argument stringp a))"
    );
    assert_eq!(
        eval("(let ((process-environment nil) (initial-environment (list \"DISPLAY=:9\"))) (list (getenv \"DISPLAY\") (getenv \"HOME\")))"),
        "(\":9\" nil)"
    );
}

/// env.el's `setenv-internal`: replace in place, keep a lone name for an
/// unset variable, `substitute-env-vars` on request.
#[test]
fn setenv_edits_process_environment() {
    assert_eq!(
        eval("(let ((process-environment (list \"X=1\" \"X=2\" \"Y\"))) (list (setenv \"Y\" \"3\") (copy-sequence process-environment) (setenv \"X\") process-environment))"),
        "(\"3\" (\"X=1\" \"X=2\" \"Y=3\") nil (\"X\" \"X=2\" \"Y=3\"))"
    );
    assert_eq!(
        eval("(let ((process-environment nil)) (list (setenv \"A\" \"1\") (setenv \"B\" \"2\") (copy-sequence process-environment) (setenv \"A\") process-environment))"),
        "(\"1\" \"2\" (\"B=2\" \"A=1\") nil (\"B=2\" \"A\"))"
    );
    assert_eq!(
        eval("(let ((process-environment (list \"A=1\"))) (setenv \"B\" \"$A-x\" t))"),
        "\"1-x\""
    );
    assert_eq!(
        eval("(setenv \"A=B\" \"1\")"),
        "(ERR (error \"Environment variable name ‘A=B’ contains ‘=’\"))"
    );
    assert_eq!(
        eval("(with-environment-variables ((\"ZZQ\" \"v\") (\"ZZR\" \"w\")) (list (getenv \"ZZQ\") (getenv \"ZZR\")))"),
        "(\"v\" \"w\")"
    );
    assert_eq!(
        eval("(progn (with-environment-variables ((\"ZZQ\" \"v\")) nil) (getenv \"ZZQ\"))"),
        "nil"
    );
}

/// A child sees `process-environment`, not the environment elisprs started
/// with: a binding, a removal and a `setenv` all reach it.
#[test]
fn subprocesses_inherit_process_environment() {
    assert_eq!(
        eval("(let ((process-environment (list \"QQ=a b\" \"PATH=/usr/bin:/bin\"))) (shell-command-to-string \"printf %s \\\"$QQ\\\"\"))"),
        "\"a b\""
    );
    assert_eq!(
        eval("(let ((process-environment (append (list \"QQ\" \"QQ=leaked\") process-environment))) (setenv \"QQ2\" \"x\") (shell-command-to-string \"printf %s \\\"${QQ-unset}:$QQ2\\\"\"))"),
        "\"unset:x\""
    );
    assert_eq!(
        eval("(with-temp-buffer (let ((process-environment (list \"ZQ=hello\" \"PATH=/usr/bin:/bin\"))) (call-process \"printenv\" nil t nil \"ZQ\") (buffer-string)))"),
        "\"hello\n\""
    );
    assert_eq!(
        eval("(let ((process-environment (list \"ZQ=1\" \"ZQ=2\" \"PATH=/usr/bin:/bin\"))) (process-lines \"printenv\" \"ZQ\"))"),
        "(\"1\")"
    );
}

/// fileio.c `get_homedir` reads `HOME` with `egetenv`; `abbreviated-home-dir`
/// was fixed at startup, so a later `HOME` does not abbreviate.
#[test]
fn home_directory_comes_from_process_environment() {
    assert_eq!(
        eval("(let ((process-environment (list \"HOME=/h\"))) (list (expand-file-name \"~\") (expand-file-name \"~/x\") (abbreviate-file-name \"/h/y\")))"),
        "(\"/h\" \"/h/x\" \"/h/y\")"
    );
    assert_eq!(
        eval("(let ((process-environment (list \"HOME\"))) (stringp (expand-file-name \"~\")))"),
        "t"
    );
    assert_eq!(
        eval("(list (stringp abbreviated-home-dir) (equal (get 'abbreviated-home-dir 'home) (expand-file-name \"~\")))"),
        "(t t)"
    );
}

/// timefns.c `set-time-zone-rule` builds a POSIX rule from each kind of ZONE;
/// `setenv` of `TZ` goes through it. `current-time-zone` is `%Z` and the
/// offset of the decoded time, with a numeric name when `%Z` is empty.
#[test]
fn time_zone_rules() {
    assert_eq!(
        eval("(progn (set-time-zone-rule 3600) (prog1 (list (format-time-string \"%Z %z\" 0) (current-time-zone 0)) (set-time-zone-rule nil)))"),
        "(\"+01 +0100\" (3600 \"+01\"))"
    );
    assert_eq!(
        eval("(progn (set-time-zone-rule 5430) (prog1 (format-time-string \"%Z %z\" 0) (set-time-zone-rule nil)))"),
        "\"+013030 +0130\""
    );
    assert_eq!(
        eval("(progn (set-time-zone-rule -3601) (prog1 (format-time-string \"%Z %z\" 0) (set-time-zone-rule nil)))"),
        "\"-010001 -0100\""
    );
    assert_eq!(
        eval("(progn (set-time-zone-rule '(-18000 \"XST\")) (prog1 (format-time-string \"%Z %z\" 0) (set-time-zone-rule nil)))"),
        "\"XST -0500\""
    );
    assert_eq!(
        eval("(progn (set-time-zone-rule t) (prog1 (format-time-string \"%Z %z\" 0) (set-time-zone-rule nil)))"),
        "\"UTC +0000\""
    );
    assert_eq!(
        eval("(set-time-zone-rule 'foo)"),
        "(ERR (error \"Invalid time zone specification\" foo))"
    );
    assert_eq!(
        eval("(set-time-zone-rule '(1 2))"),
        "(ERR (wrong-type-argument stringp 2))"
    );
    assert_eq!(
        eval("(progn (setenv \"TZ\" \"EST5\") (prog1 (list (getenv \"TZ\") (format-time-string \"%Z\" 0)) (setenv \"TZ\")))"),
        "(\"EST5\" \"EST\")"
    );
    assert_eq!(
        eval("(list (current-time-zone 0 t) (current-time-zone 0 3600) (current-time-zone 0 5430) (current-time-zone 0 '(-18000 \"XST\")) (current-time-zone 0 \"JST-9\") (current-time-zone 1.5 \"UTC0\"))"),
        "((0 \"UTC\") (3600 \"+01\") (5430 \"+013030\") (-18000 \"XST\") (32400 \"JST\") (0 \"UTC\"))"
    );
    assert_eq!(
        eval("(list (current-time-zone '(0 0) \"EST5EDT\") (current-time-zone 15000000 \"EST5EDT\") (current-time-zone 0 \"<>5\"))"),
        "((-18000 \"EST\") (-14400 \"EDT\") (-18000 \"-05\"))"
    );
    assert_eq!(
        eval("(current-time-zone 0 'foo)"),
        "(ERR (error \"Invalid time zone specification\" foo))"
    );
    assert_eq!(
        eval("(current-time-zone \"x\")"),
        "(ERR (error \"Invalid time specification\"))"
    );
}
