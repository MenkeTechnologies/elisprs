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
