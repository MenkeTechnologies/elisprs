//! The Lisp major modes loadup.el preloads, the features batch Emacs starts
//! with, and `funcall` of a macro.
//!
//! - prog-mode.el, lisp-mode.el and elisp-mode.el were not ported, so
//!   `(emacs-lisp-mode)` was `void-function` and the initial buffer stayed in
//!   `fundamental-mode` where `emacs -Q --batch` has `*scratch*` in
//!   `lisp-interaction-mode`.
//! - `features` held a dozen entries, so `(require 'cl-generic)`,
//!   `(require 'macroexp)` or `(require 'easymenu)` — no-ops in Emacs, which
//!   dumps them — went looking for a file and signalled `file-missing`.
//! - `setq-local` was defined after `run-mode-hooks` used it, so the use
//!   compiled as a call and every mode with a `syntax-propertize-function` died
//!   with an `error` Emacs does not have.
//! - `funcall` of a macro signalled `(error "macro called as a function …")`
//!   where eval.c's `funcall_general` signals `(invalid-function FUN)`.
//! - `buffer-local-variables` and `sleep-for` were void.
//! - A call form with a dotted argument list was a compile-time error no
//!   `condition-case` could catch.
//!
//! Every expectation below is GNU Emacs 31.1 evaluating the same form under
//! `emacs -Q --batch -l FILE`.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn initial_buffer_is_in_lisp_interaction_mode() {
    assert_eq!(
        eval(
            "(list major-mode mode-name (derived-mode-p 'prog-mode)
                   (eq (syntax-table) lisp-interaction-mode-syntax-table))"
        ),
        "(lisp-interaction-mode \"Lisp Interaction\" prog-mode t)"
    );
}

#[test]
fn emacs_lisp_mode_sets_up_lisp_mode_variables() {
    assert_eq!(
        eval(
            "(with-temp-buffer (emacs-lisp-mode)
               (list major-mode comment-start comment-start-skip comment-add comment-column
                     parse-sexp-ignore-comments indent-line-function
                     (derived-mode-p 'lisp-data-mode)
                     (eq (syntax-table) emacs-lisp-mode-syntax-table)))"
        ),
        "(emacs-lisp-mode \";\" \";+ *\" 1 40 t lisp-indent-line lisp-data-mode t)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (lisp-interaction-mode)
               (list major-mode mode-name lexical-binding (local-variable-p 'lexical-binding)))"
        ),
        "(lisp-interaction-mode \"Lisp Interaction\" t t)"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (prog-mode)
               (list major-mode mode-name parse-sexp-ignore-comments bidi-paragraph-direction))"
        ),
        "(prog-mode \"Prog\" t left-to-right)"
    );
    // `kill-all-local-variables' takes the mode's locals with it.
    assert_eq!(
        eval("(with-temp-buffer (emacs-lisp-mode) (fundamental-mode) (list major-mode comment-start))"),
        "(fundamental-mode nil)"
    );
}

#[test]
fn mode_chain_and_hook_order() {
    assert_eq!(
        eval(
            "(mapcar (lambda (m) (get m 'derived-mode-parent))
                     '(lisp-interaction-mode emacs-lisp-mode lisp-data-mode prog-mode))"
        ),
        "(emacs-lisp-mode lisp-data-mode prog-mode nil)"
    );
    // Parent hooks run first; `push' leaves the last-run hook at the head.
    assert_eq!(
        eval(
            "(let (r)
               (add-hook 'emacs-lisp-mode-hook (lambda () (push 'el r)))
               (add-hook 'prog-mode-hook (lambda () (push 'prog r)))
               (add-hook 'lisp-data-mode-hook (lambda () (push 'data r)))
               (with-temp-buffer (emacs-lisp-mode))
               r)"
        ),
        "(el data prog)"
    );
}

#[test]
fn emacs_lisp_mode_drives_indentation_comments_and_parsing() {
    assert_eq!(
        eval(
            "(with-temp-buffer (emacs-lisp-mode)
               (insert \"(defun f ()\\n(+ 1\\n2))\")
               (indent-region (point-min) (point-max))
               (buffer-string))"
        ),
        "\"(defun f ()\n  (+ 1\n     2))\""
    );
    assert_eq!(
        eval("(with-temp-buffer (emacs-lisp-mode) (insert \"x\") (comment-region 1 2) (buffer-string))"),
        "\";; x\""
    );
    assert_eq!(
        eval("(with-temp-buffer (emacs-lisp-mode) (insert \"(a \\\"b\") (nth 3 (syntax-ppss)))"),
        "34"
    );
}

#[test]
fn preloaded_features_are_provided() {
    assert_eq!(
        eval(
            "(list (require 'cl-generic) (require 'macroexp) (require 'easymenu)
                   (require 'lisp-mode) (featurep 'elisp-mode) (featurep 'font-lock))"
        ),
        "(cl-generic macroexp easymenu lisp-mode t t)"
    );
    assert_eq!(
        eval("(list (car features) (featurep 'cl-preloaded))"),
        "(rmc t)"
    );
}

#[test]
fn funcall_of_a_macro_is_invalid_function() {
    assert_eq!(
        eval("(condition-case e (funcall 'push 1 2) (error e))"),
        "(invalid-function push)"
    );
    // A compiler intrinsic answers the same as a prelude macro.
    assert_eq!(
        eval("(condition-case e (mapcar 'when '(1)) (error e))"),
        "(invalid-function when)"
    );
    assert_eq!(
        eval("(condition-case e (apply #'setq-local '(a 1)) (error e))"),
        "(invalid-function setq-local)"
    );
}

#[test]
fn setq_local_is_subr_el_s() {
    assert_eq!(
        eval("(macroexpand '(setq-local a 1 b 2))"),
        "(progn (setq a (prog1 1 (make-local-variable 'a))) (setq b (prog1 2 (make-local-variable 'b))))"
    );
    assert_eq!(eval("(macroexpand '(setq-local))"), "nil");
    assert_eq!(
        eval("(condition-case e (macroexpand '(setq-local a)) (error e))"),
        "(wrong-number-of-arguments setq-local 1)"
    );
    // `run-mode-hooks' itself uses `setq-local'; it used to compile as a call.
    assert_eq!(
        eval(
            "(with-temp-buffer
               (setq-local syntax-propertize-function #'ignore)
               (run-mode-hooks)
               parse-sexp-lookup-properties)"
        ),
        "t"
    );
}

#[test]
fn progress_reporters_exist() {
    assert_eq!(
        eval("(let ((r (make-progress-reporter \"Working\" 0 10))) (list (car r) (progress-reporter-text r)))"),
        "(1 \"Working...\")"
    );
}

/// buffer.c `Fbuffer_local_variables`: the per-buffer slots in `struct
/// buffer` order (a slot like `fill-column` only once it is local), then the
/// Lisp locals oldest first, a void one as a bare symbol.
#[test]
fn buffer_local_variables_in_buffer_c_order() {
    assert_eq!(
        eval(
            "(with-temp-buffer
               (setq-local b-x 1) (setq-local a-x 2) (setq-local fill-column 3)
               (make-local-variable 'c-x)
               (mapcar (lambda (e) (if (consp e) (car e) e)) (buffer-local-variables)))"
        ),
        "(buffer-undo-list buffer-display-time buffer-display-count buffer-invisibility-spec \
         buffer-file-truename point-before-scroll buffer-auto-save-file-format buffer-file-format \
         enable-multibyte-characters mark-active fill-column mode-name local-minor-modes major-mode \
         buffer-read-only buffer-auto-save-file-name buffer-saved-size buffer-backed-up \
         default-directory buffer-file-name b-x a-x c-x)"
    );
    assert_eq!(
        eval("(with-temp-buffer (setq-local b-x 1) (assq 'b-x (buffer-local-variables)))"),
        "(b-x . 1)"
    );
    assert_eq!(
        eval(
            "(list (local-variable-p 'enable-multibyte-characters) enable-multibyte-characters
                   (default-value 'enable-multibyte-characters))"
        ),
        "(t t t)"
    );
    assert_eq!(
        eval("(condition-case e (buffer-local-variables 5) (error e))"),
        "(wrong-type-argument bufferp 5)"
    );
}

#[test]
fn sleep_for_and_subr_el_aliases() {
    assert_eq!(
        eval(
            "(list (sleep-for 0.01) (sleep-for 0 5) (sleep-for -1)
                   (condition-case e (sleep-for 'x) (error e))
                   (condition-case e (sleep-for 0 1.5) (error e)))"
        ),
        "(nil nil nil (wrong-type-argument numberp x) (wrong-type-argument fixnump 1.5))"
    );
    assert_eq!(
        eval(
            "(list (symbol-function 'rplaca) (symbol-function 'store-match-data)
                   (let ((c (list 1 2))) (rplacd c 9) c))"
        ),
        "(setcar set-match-data (1 . 9))"
    );
}

/// eval.c `eval_sub` on a call form with a dotted argument list: the head is
/// looked up first, then `list_length` signals on the final cdr — when the
/// form runs, so `condition-case` catches it and an unreached one is inert.
#[test]
fn dotted_call_forms_signal_when_run() {
    assert_eq!(
        eval("(condition-case e (car 1 . 2) (error e))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval("(condition-case e (list 1 2 . 3) (error e))"),
        "(wrong-type-argument listp 3)"
    );
    assert_eq!(
        eval("(condition-case e (foo-undefined 1 . 2) (error e))"),
        "(void-function foo-undefined)"
    );
    assert_eq!(
        eval("(condition-case e ((lambda (x) x) 1 . 2) (error e))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval("(condition-case e (funcall (lambda () (car . 2))) (error e))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval("(condition-case e (if t . 1) (error e))"),
        "(wrong-type-argument listp 1)"
    );
    assert_eq!(
        eval("(condition-case e (progn 1 . 2) (error e))"),
        "(wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval("(ignore-errors (nil (nil (65535 . 5000)) (prin1-to-string)))"),
        "nil"
    );
    assert_eq!(
        eval("(list (if nil (car 1 . 2) 'skipped) 'after)"),
        "(skipped after)"
    );
}

/// `elisp FILE`, with an isolated HOME so the run never touches the
/// developer's cache shard.
fn run_file(tag: &str, script: &str) -> String {
    let exe = env!("CARGO_BIN_EXE_elisp");
    let dir = std::env::temp_dir().join(format!("elisprs-modes-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("script.el");
    std::fs::write(&path, script).expect("write script");
    let out = std::process::Command::new(exe)
        .arg(&path)
        .env("HOME", &dir)
        .output()
        .expect("run elisp");
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `load` binds `lexical-binding` to the file's cookie; in `*scratch*` that
/// is the local `lisp-interaction-mode` made, so it stays local.
#[test]
fn load_binds_lexical_binding_to_the_cookie() {
    assert_eq!(run_file("none", "(prin1 lexical-binding)\n"), "nil");
    assert_eq!(
        run_file(
            "nil",
            ";; -*- lexical-binding: nil -*-\n(prin1 lexical-binding)\n"
        ),
        "nil"
    );
    assert_eq!(
        run_file(
            "t",
            ";; -*- mode: emacs-lisp; lexical-binding: t; -*-\n\
             (prin1 (list lexical-binding (local-variable-p 'lexical-binding)))\n"
        ),
        "(t t)"
    );
    assert_eq!(
        run_file(
            "shebang",
            "#!/usr/bin/env emacs\n;; -*- lexical-binding: t -*-\n(prin1 lexical-binding)\n"
        ),
        "t"
    );
    // custom.el's `defcustom' branches on it.
    assert_eq!(
        run_file(
            "defcustom",
            ";;; -*- lexical-binding: t -*-\n\
             (defcustom dcd-x 5 \"d\" :type 'integer :group 'x)\n\
             (prin1 (get 'dcd-x 'standard-value))\n"
        ),
        "((funcall #'#[nil (5) (t)]))"
    );
}
