//! `let` over an automatically buffer-local variable, eval.c `specbind`: with
//! a local in the current buffer the local is rebound (SPECPDL_LET_LOCAL);
//! without one the DEFAULT is (SPECPDL_LET_DEFAULT), and while that binding is
//! live a `setq` in the same buffer sets the default too (data.c
//! `let_shadows_buffer_binding_p`), where one in another buffer makes a local.
//!
//! elisprs always rebound a local, so a buffer created inside the `let` never
//! saw the bound value. Expectations are GNU Emacs 31.1 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn let_without_a_local_binds_the_default() {
    check(
        "(progn (defvar-local zzl 1) \
         (list (with-temp-buffer (let ((zzl 2)) (list zzl (local-variable-p 'zzl) (default-value 'zzl) \
                                                      (with-temp-buffer zzl)))) \
               zzl))",
        "((2 nil 2 2) 1)",
    );
}

#[test]
fn let_with_a_local_binds_the_local() {
    check(
        "(progn (defvar-local zzl 1) \
         (with-temp-buffer (setq zzl 7) (let ((zzl 3)) (list zzl (default-value 'zzl)))))",
        "(3 1)",
    );
}

#[test]
fn setq_under_a_default_binding_depends_on_the_buffer() {
    check(
        "(progn (defvar-local zzl 1) \
         (list (with-temp-buffer (let ((zzl 4)) (setq zzl 6) \
                                   (list zzl (local-variable-p 'zzl) (default-value 'zzl)))) \
               (with-temp-buffer (let ((zzl 4)) (with-temp-buffer (setq zzl 5) \
                                   (list zzl (local-variable-p 'zzl) (default-value 'zzl))))) \
               zzl))",
        "((6 nil 6) (5 t 4) 1)",
    );
}

/// buffer.c's per-buffer variables are bound with Emacs's defaults and are
/// automatically buffer-local, so setting one in a buffer leaves every other
/// buffer's value alone, and a `let` of one with no local binds the default.
#[test]
fn per_buffer_c_variables() {
    check(
        "(list (let ((case-fold-search nil)) \
                 (with-temp-buffer (insert \"A\") (goto-char 1) (search-forward \"a\" nil t))) \
               (with-temp-buffer (setq case-fold-search nil) (with-temp-buffer case-fold-search)) \
               (progn (setq tab-width 4) (with-temp-buffer tab-width)) \
               (with-temp-buffer (let ((tab-width 3)) (with-temp-buffer tab-width))) \
               (with-temp-buffer (setq-local tab-width 5) \
                 (let ((tab-width 3)) (list tab-width (default-value 'tab-width) (with-temp-buffer tab-width)))) \
               (with-temp-buffer (setq-local fill-column 33) (list fill-column (default-value 'fill-column))))",
        "(nil t 8 3 (3 8 8) (33 70))",
    );
    check(
        "(mapcar (lambda (v) (list (local-variable-if-set-p v) (default-value v))) \
                 '(fill-column left-margin buffer-file-name truncate-lines word-wrap \
                   buffer-file-coding-system selective-display-ellipses buffer-display-count))",
        "((t 70) (t 0) (t nil) (t nil) (t nil) (t utf-8-unix) (t t) (t 0))",
    );
}

/// buffer.c's always-local slots (`buffer_local_flags` -1): every buffer has
/// its own value from birth, so `local-variable-p` is t in a fresh buffer,
/// `kill-local-variable` keeps the value, and a new buffer starts from
/// `reset_buffer`'s constants — not the defaults — except `default-directory`
/// (the creating buffer's) and `buffer-undo-list` (t in a buffer whose name
/// starts with a space). `kill-all-local-variables` resets only the slots
/// `reset_buffer_local_variables` stores.
#[test]
fn always_local_per_buffer_slots() {
    check(
        "(with-temp-buffer (mapcar (lambda (v) (local-variable-p v)) \
           '(buffer-file-name buffer-file-truename default-directory buffer-backed-up \
             buffer-saved-size buffer-auto-save-file-name buffer-read-only major-mode \
             local-minor-modes mode-name buffer-undo-list mark-active point-before-scroll \
             buffer-invisibility-spec buffer-file-format buffer-auto-save-file-format \
             buffer-display-count buffer-display-time fill-column)))",
        "(t t t t t t t t t t t t t t t t t t nil)",
    );
    check(
        "(list (mapcar #'default-value \
                 '(default-directory major-mode mode-name buffer-undo-list buffer-invisibility-spec)) \
               (with-temp-buffer (list major-mode mode-name buffer-undo-list)) \
               (with-current-buffer (get-buffer-create \"zzb\") buffer-undo-list) \
               (let ((default-directory \"/usr/\")) (with-temp-buffer default-directory)) \
               (progn (setq-default buffer-read-only t major-mode 'text-mode) \
                      (with-temp-buffer (list buffer-read-only major-mode))))",
        "((nil fundamental-mode nil nil t) (fundamental-mode \"Fundamental\" t) nil \"/usr/\" (nil fundamental-mode))",
    );
    check(
        "(with-temp-buffer \
           (setq major-mode 'foo mode-name \"x\" buffer-read-only t default-directory \"/a/\" \
                 buffer-invisibility-spec nil) \
           (kill-local-variable 'buffer-read-only) \
           (let ((r (list buffer-read-only (local-variable-p 'buffer-read-only)))) \
             (kill-all-local-variables) \
             (list r major-mode mode-name buffer-read-only default-directory \
                   buffer-invisibility-spec (local-variable-p 'major-mode))))",
        "((t t) fundamental-mode \"Fundamental\" t \"/a/\" t t)",
    );
}
