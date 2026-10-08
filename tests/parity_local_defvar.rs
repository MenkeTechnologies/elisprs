//! A value-less `(defvar SYM)` under lexical binding, eval.c `Fdefvar`: it
//! conses the bare SYM onto the interpreter environment, so SYM is
//! dynamically bound by the `let`s of the rest of that scope only, and
//! `special-variable-p` stays nil. A closure made in the scope keeps the
//! declaration when its body `let`-binds SYM (cconv.el `cconv--dynbindings`),
//! and an environment holding only declarations is kept whole.
//!
//! elisprs marked SYM special everywhere, for good.
//! Expectations are GNU Emacs 31.1 (`emacs -Q --batch`).

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn declaration_is_local_to_its_scope() {
    check("(let ((x 1)) (defvar x) (special-variable-p 'x))", "nil");
    check(
        "(progn (defvar zz1) (list (special-variable-p 'zz1) (let ((zz1 2)) (symbol-value 'zz1))))",
        "(nil 2)",
    );
    check(
        "(let ((y 1)) (defvar zz3) (let ((zz3 5)) (list (boundp 'zz3) (symbol-value 'zz3))))",
        "(t 5)",
    );
    // The declaration ends with the `let` that holds it.
    check(
        "(progn (let ((y 1)) (defvar zz4)) (let ((zz4 5)) (boundp 'zz4)))",
        "nil",
    );
    // A lexical binding made before the declaration still answers the
    // variable's reads; the inner `let` binds the value cell.
    check(
        "(let ((zz6 1)) (defvar zz6) (list zz6 (let ((zz6 2)) (list zz6 (symbol-value 'zz6)))))",
        "(1 (1 2))",
    );
    // An already special variable stays special; dynamic binding ignores it.
    check(
        "(progn (defvar zz7 3) (defvar zz7) (special-variable-p 'zz7))",
        "t",
    );
    check(
        "(eval '(progn (defvar zz2) (special-variable-p 'zz2)) nil)",
        "nil",
    );
}

#[test]
fn eval_progn_subforms_share_the_declaration() {
    check(
        "(eval '(progn (defvar zz8) (let ((zz8 3)) (boundp 'zz8))) t)",
        "t",
    );
    check(
        "(funcall (eval '(progn (defvar zz10) (lambda () (let ((zz10 3)) (boundp 'zz10)))) t))",
        "t",
    );
}

#[test]
fn closures_keep_declarations_they_bind() {
    check(
        "(let ((y 1)) (defvar x) (lambda () (list x y)))",
        "#[nil ((list x y)) ((y . 1))]",
    );
    check(
        "(let ((y 2)) (defvar ww) (lambda () (let ((ww 1)) (list y ww))))",
        "#[nil ((let ((ww 1)) (list y ww))) ((y . 2) ww)]",
    );
    check(
        "(eval '(progn (defvar zz9) (let ((zz9 3)) (lambda () zz9))) t)",
        "#[nil (zz9) (zz9 t)]",
    );
}

/// At top level the declaration lasts for the rest of the file, and a
/// function defined after it binds the variable dynamically.
#[test]
fn top_level_declaration_lasts_for_the_file() {
    check(
        "(defvar xx)
         (defun f () (let ((xx 1)) (g)))
         (defun g () xx)
         (list (condition-case e (f) (error e)) (symbol-function 'f) (special-variable-p 'xx))",
        "(1 #[nil ((let ((xx 1)) (g))) (xx t)] nil)",
    );
}

/// eval.c `internal--define-uninitialized-variable` (what `defcustom` and the
/// custom initializers call) sets `declared_special` for good, unlike a
/// value-less `defvar`.
#[test]
fn define_uninitialized_variable_is_special_for_good() {
    check(
        "(progn (internal--define-uninitialized-variable 'qqq \"d\") \
                (list (special-variable-p 'qqq) (get 'qqq 'variable-documentation) \
                      (boundp 'qqq) (let ((qqq 1)) (symbol-value 'qqq))))",
        "(t \"d\" nil 1)",
    );
}
