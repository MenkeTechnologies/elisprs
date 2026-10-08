//! `cl-typep` / `cl-deftype` / `cl-coerce`, `seq-let` / `seq-setq` / `pcase-setq`,
//! macro arity errors, `(lambda ...)` lists as functions, and `commandp` /
//! `interactive-form`.
//!
//! - `cl-typep` is the function body of cl-macs.el's `define-inline`: an atomic
//!   type with a `cl-deftype-satisfies` predicate, a `cl-deftype` handler, the
//!   `(integer|float|number MIN MAX)` bounds (applied only when exactly two are
//!   given), `not`/`and`/`or`/`eql`/`member`/`satisfies`, then `TYPEp`, `TYPE-p`,
//!   `TYPE`, else `Unknown type` or `Bad type spec`. `real`, `character`,
//!   `base-char`, `extended-char`, `natnum`, `keyword` and `command` are the
//!   derived types cl-preloaded.el / cl-macs.el define; `cl-deftype` computes the
//!   atomic predicate as `cl--define-derived-type` receives it.
//! - `cl-coerce` is cl-extra.el's (identity for an object already of TYPE,
//!   characters from one-character strings and symbols, `Can't coerce`).
//! - `seq-let` / `seq-setq` are seq.el's: `pcase-let` / `pcase-setq` over the
//!   `seq` pattern, so nested argument lists and `&rest` work and `seq-setq`
//!   answers the first element (the last assignment). `pcase-setq` is
//!   pcase.el's (pairs, the trivial-pattern `setq`, arity checks).
//! - A macro called with the wrong number of arguments names its expander, or
//!   `(MANDATORY . NONREST)` for the preloaded (byte-compiled) macros.
//! - eval.c: a `(lambda ARGS . BODY)` list is funcallable under dynamic binding,
//!   `functionp`, has a `func-arity`; an autoload is `functionp` unless a macro.
//! - `commandp` / `interactive-form` are eval.c's / data.c's for closures,
//!   lambda lists, keyboard macros and autoloads; a macro is not a closure.
//!
//! Every expectation is `emacs -Q --batch` (GNU Emacs 31.1) printing
//! `(condition-case e FORM (error (list 'signal e)))` with cl-lib and seq loaded.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn cl_typep_follows_the_define_inline_body() {
    check(
        r##"(condition-case e (list (cl-typep 1.0 'real) (cl-typep 'a 'real) (cl-typep 97 'character) (cl-typep -1 'character) (cl-typep 97 'base-char) (cl-typep 4194303 'extended-char)) (error (list 'signal e)))"##,
        r##"(t nil t nil t nil)"##,
    );
    check(
        r##"(condition-case e (list (cl-typep 5 '(integer (5) *)) (cl-typep 6 '(integer (5) *)) (cl-typep 5 '(integer 1)) (cl-typep 0 '(integer 1)) (cl-typep 2 '(number 1 (3))) (cl-typep 3 '(number 1 (3)))) (error (list 'signal e)))"##,
        r##"(nil t t nil t nil)"##,
    );
    check(
        r##"(condition-case e (list (cl-typep 50 '(integer 0 10 20)) (cl-typep 5 '(integer 0 10 20)) (cl-typep 5 '(integer * 3 9)) (cl-typep 5 '(integer 6 . 9)) (cl-typep 5.0 '(float 6)) (cl-typep 5 '(integer))) (error (list 'signal e)))"##,
        r##"(nil t nil nil nil t)"##,
    );
    check(
        r##"(condition-case e (list (cl-typep 1 'natnum) (cl-typep -1 'natnum) (cl-typep :a 'keyword) (cl-typep 'a 'keyword) (cl-typep 1 '(cons)) (cl-typep nil '(and)) (cl-typep nil '(or))) (error (list 'signal e)))"##,
        r##"(t nil t nil nil t nil)"##,
    );
    check(
        r##"(condition-case e (cl-typep 1 '(foo)) (error (list 'signal e)))"##,
        r##"(signal (error "Bad type spec: (foo)"))"##,
    );
    check(
        r##"(condition-case e (cl-typep 1 'cl-zz-unknown) (error (list 'signal e)))"##,
        r##"(signal (error "Unknown type cl-zz-unknown"))"##,
    );
    check(
        r##"(condition-case e (cl-typep [1] 'simple-vector) (error (list 'signal e)))"##,
        r##"(signal (error "Unknown type simple-vector"))"##,
    );
    check(
        r##"(condition-case e (cl-typep 1 '(not integer string)) (error (list 'signal e)))"##,
        r##"(signal (error "Bad type spec: (not integer string)"))"##,
    );
    check(
        r##"(condition-case e (list (cl-typecase 1.5 (integer 'i) (real 'r) (t 'o)) (cl-typecase ?a (character 'c) (t 'o))) (error (list 'signal e)))"##,
        r##"(r c)"##,
    );
    check(
        r##"(condition-case e (progn (cl-deftype zz-small () '(integer 0 9)) (list (cl-typep 5 'zz-small) (cl-typep 15 'zz-small) (functionp (get 'zz-small 'cl-deftype-satisfies)))) (error (list 'signal e)))"##,
        r##"(t nil t)"##,
    );
    check(
        r##"(condition-case e (progn (cl-deftype zz-even () '(satisfies cl-evenp)) (list (get 'zz-even 'cl-deftype-satisfies) (cl-typep 4 'zz-even))) (error (list 'signal e)))"##,
        r##"(cl-evenp t)"##,
    );
    check(
        r##"(condition-case e (progn (cl-deftype zz-rng (lo hi) (list 'integer lo hi)) (list (get 'zz-rng 'cl-deftype-satisfies) (cl-typep 5 '(zz-rng 1 10)))) (error (list 'signal e)))"##,
        r##"(nil t)"##,
    );
    check(
        r##"(condition-case e (progn (cl-deftype zz-opt (&optional n) (list 'integer 0 n)) (list (cl-typep 50 'zz-opt) (cl-typep 50 '(zz-opt 9)))) (error (list 'signal e)))"##,
        r##"(t nil)"##,
    );
    check(
        r##"(condition-case e (progn (cl-defstruct zz-pt x) (list (get 'zz-pt 'cl-deftype-satisfies) (cl-typep (make-zz-pt) 'zz-pt) (cl-typep 1 'zz-pt))) (error (list 'signal e)))"##,
        r##"(zz-pt-p t nil)"##,
    );
}

#[test]
fn cl_coerce_is_cl_extra() {
    check(
        r##"(condition-case e (list (cl-coerce "a" 'character) (cl-coerce 'b 'character) (cl-coerce "ab" 'array) (cl-coerce '(t nil t) 'bool-vector) (cl-coerce [1 2] 'list) (cl-coerce 1 'float)) (error (list 'signal e)))"##,
        r##"(97 98 "ab" #&3"" (1 2) 1.0)"##,
    );
    check(
        r##"(condition-case e (let ((l (list 1 2)) (v (vector 1 2)) (s "ab")) (list (eq l (cl-coerce l 'list)) (eq v (cl-coerce v 'array)) (eq s (cl-coerce s 'string)) (eq v (cl-coerce v 'vector)))) (error (list 'signal e)))"##,
        r##"(t t t t)"##,
    );
    check(
        r##"(condition-case e (cl-coerce "ab" 'character) (error (list 'signal e)))"##,
        r##"(signal (error "Can’t coerce ab to type character"))"##,
    );
    check(
        r##"(condition-case e (cl-coerce 1.5 'integer) (error (list 'signal e)))"##,
        r##"(signal (error "Can’t coerce 1.5 to type integer"))"##,
    );
    check(
        r##"(condition-case e (cl-coerce 2 'integer) (error (list 'signal e)))"##,
        r##"2"##,
    );
    check(
        r##"(condition-case e (cl-coerce [1 2] 'simple-vector) (error (list 'signal e)))"##,
        r##"(signal (error "Unknown type simple-vector"))"##,
    );
}

#[test]
fn seq_let_and_seq_setq_are_pcase() {
    check(
        r##"(condition-case e (seq-let (a (b c)) '(1 (2 3)) (list a b c)) (error (list 'signal e)))"##,
        r##"(1 2 3)"##,
    );
    check(
        r##"(condition-case e (seq-let [a b &rest c] [1 2 3 4] (list a b c)) (error (list 'signal e)))"##,
        r##"(1 2 [3 4])"##,
    );
    check(
        r##"(condition-case e (seq-let (a b c) '(1) (list a b c)) (error (list 'signal e)))"##,
        r##"(1 nil nil)"##,
    );
    check(
        r##"(condition-case e (seq-let (a [b c]) '(1 "xy") (list a b c)) (error (list 'signal e)))"##,
        r##"(1 120 121)"##,
    );
    check(
        r##"(condition-case e (let (a b) (list (seq-setq (a b) '(1 2)) a b)) (error (list 'signal e)))"##,
        r##"(1 1 2)"##,
    );
    check(
        r##"(condition-case e (let (a b c) (list (seq-setq (a &rest b) [1 2 3]) a b)) (error (list 'signal e)))"##,
        r##"(1 1 [2 3])"##,
    );
    check(
        r##"(condition-case e (let (a b c) (list (seq-setq (a (b c)) '(1 (2 3))) a b c)) (error (list 'signal e)))"##,
        r##"(1 1 2 3)"##,
    );
    check(
        r##"(condition-case e (pcase [1 2 3] ((seq a _ c) (list a c))) (error (list 'signal e)))"##,
        r##"(1 3)"##,
    );
    check(
        r##"(condition-case e (pcase '(1 2 3) ((seq a &rest r) (list a r))) (error (list 'signal e)))"##,
        r##"(1 (2 3))"##,
    );
    check(
        r##"(condition-case e (pcase 5 ((seq a) (list 'matched a)) (_ 'no)) (error (list 'signal e)))"##,
        r##"no"##,
    );
}

#[test]
fn pcase_setq_is_pcase_el() {
    check(
        r##"(condition-case e (let (a b) (list (pcase-setq `(,a ,b) '(1 2)) a b)) (error (list 'signal e)))"##,
        r##"(2 1 2)"##,
    );
    check(
        r##"(condition-case e (let (a b) (list (pcase-setq a 1 b 2) a b)) (error (list 'signal e)))"##,
        r##"(2 1 2)"##,
    );
    check(
        r##"(condition-case e (let (a b) (list (pcase-setq `(,a ,b) '(1 2) a 3) a b)) (error (list 'signal e)))"##,
        r##"(3 3 2)"##,
    );
    check(
        r##"(condition-case e (let (a b) (list (pcase-setq (seq a b) [1 2]) a b)) (error (list 'signal e)))"##,
        r##"(1 1 2)"##,
    );
    check(
        r##"(condition-case e (macroexpand '(pcase-setq a 1)) (error (list 'signal e)))"##,
        r##"(setq a 1)"##,
    );
}

#[test]
fn macro_arity_errors_name_the_expander() {
    check(
        r##"(condition-case e (progn (defmacro zz-m1 (a b) a) (macroexpand '(zz-m1 1))) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments #[(a b) (a) (t)] 1))"##,
    );
    check(
        r##"(condition-case e (progn (defmacro zz-m2 (a b &rest c) a) (macroexpand '(zz-m2 1))) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments #[(a b &rest c) (a) (t)] 1))"##,
    );
    check(
        r##"(condition-case e (progn (defmacro zz-m3 (a &optional b) a) (macroexpand '(zz-m3 1 2 3))) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments #[(a &optional b) (a) (t)] 3))"##,
    );
    check(
        r##"(condition-case e (eval '(progn (defmacro zz-m4 (a b) a) (zz-m4 1)) t) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments #[(a b) (a) (t)] 1))"##,
    );
    check(
        r##"(condition-case e (macroexpand '(push 1)) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments (2 . 2) 1))"##,
    );
    check(
        r##"(condition-case e (macroexpand '(dolist)) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments (1 . 1) 0))"##,
    );
    check(
        r##"(condition-case e (macroexpand '(pcase-setq a)) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments (2 . 2) 1))"##,
    );
}

#[test]
fn lambda_lists_are_functions() {
    check(
        r##"(condition-case e (funcall '(lambda (x) (* x 2)) 4) (error (list 'signal e)))"##,
        r##"8"##,
    );
    check(
        r##"(condition-case e (apply '(lambda (&rest x) x) 1 '(2)) (error (list 'signal e)))"##,
        r##"(1 2)"##,
    );
    check(
        r##"(condition-case e (mapcar '(lambda (x) (1+ x)) '(1 2)) (error (list 'signal e)))"##,
        r##"(2 3)"##,
    );
    check(
        r##"(condition-case e (progn (fset 'zz-f1 '(lambda (x) x)) (list (zz-f1 7) (functionp 'zz-f1) (func-arity 'zz-f1))) (error (list 'signal e)))"##,
        r##"(7 t (1 . 1))"##,
    );
    check(
        r##"(condition-case e (list (functionp '(lambda (x) x)) (functionp '(lambda)) (functionp '(closure (t) (x) x)) (functionp 'when) (functionp (symbol-function 'if))) (error (list 'signal e)))"##,
        r##"(t t nil nil nil)"##,
    );
    check(
        r##"(condition-case e (list (func-arity '(lambda (x &optional y) x)) (func-arity '(lambda (a &rest b) a)) (func-arity '(lambda ()))) (error (list 'signal e)))"##,
        r##"((1 . 2) (1 . many) (0 . 0))"##,
    );
    check(
        r##"(condition-case e (func-arity '(lambda)) (error (list 'signal e)))"##,
        r##"(signal (invalid-function (lambda)))"##,
    );
    check(
        r##"(condition-case e (func-arity '(lambda (1))) (error (list 'signal e)))"##,
        r##"(signal (invalid-function (lambda (1))))"##,
    );
    check(
        r##"(condition-case e (funcall '(lambda (x) x)) (error (list 'signal e)))"##,
        r##"(signal (wrong-number-of-arguments (lambda (x) x) 0))"##,
    );
    check(
        r##"(condition-case e (funcall '(lambda)) (error (list 'signal e)))"##,
        r##"(signal (invalid-function (lambda)))"##,
    );
    check(
        r##"(condition-case e (let ((zz-dyn 5)) (funcall '(lambda () (boundp 'zz-dyn)))) (error (list 'signal e)))"##,
        r##"nil"##,
    );
    check(
        r##"(condition-case e (progn (autoload 'zz-auto "zz-file") (autoload 'zz-auto2 "zz-file" nil nil 'macro) (list (functionp 'zz-auto) (functionp 'zz-auto2))) (error (list 'signal e)))"##,
        r##"(t nil)"##,
    );
}

#[test]
fn commandp_and_interactive_form() {
    check(
        r##"(condition-case e (list (commandp (lambda () (interactive) 1)) (commandp (lambda () 1)) (commandp '(lambda () "doc" (interactive "p") 1)) (commandp '(lambda () 1))) (error (list 'signal e)))"##,
        r##"(t nil t nil)"##,
    );
    check(
        r##"(condition-case e (list (commandp "abc") (commandp "abc" t) (commandp [1 2]) (commandp 'car) (commandp nil) (commandp 'zz-undefined) (commandp 'when)) (error (list 'signal e)))"##,
        r##"(t nil t nil nil nil nil)"##,
    );
    check(
        r##"(condition-case e (progn (defun zz-c1 () (interactive) 1) (defun zz-c2 () 1) (list (commandp 'zz-c1) (commandp 'zz-c2))) (error (list 'signal e)))"##,
        r##"(t nil)"##,
    );
    check(
        r##"(condition-case e (progn (autoload 'zz-a1 "f" nil t) (autoload 'zz-a2 "f" nil nil) (list (commandp 'zz-a1) (commandp 'zz-a2))) (error (list 'signal e)))"##,
        r##"(t nil)"##,
    );
    check(
        r##"(condition-case e (list (interactive-form (lambda (x) (interactive "p") x)) (interactive-form '(lambda (x) (interactive "p" foo-mode) x)) (interactive-form '(lambda () (interactive))) (interactive-form 'car) (interactive-form (lambda () 1))) (error (list 'signal e)))"##,
        r##"((interactive "p") (interactive "p") (interactive) nil nil)"##,
    );
    check(
        r##"(condition-case e (progn (defun zz-c3 () (interactive "P") 1) (put 'zz-c4 'interactive-form '(interactive "x")) (defalias 'zz-c4 'zz-c3) (list (interactive-form 'zz-c3) (interactive-form 'zz-c4))) (error (list 'signal e)))"##,
        r##"((interactive "P") (interactive "x"))"##,
    );
    check(
        r##"(condition-case e (list (closurep (symbol-function 'push)) (closurep (lambda () 1))) (error (list 'signal e)))"##,
        r##"(nil t)"##,
    );
}

/// eval.c `Fcommandp` / data.c `Finteractive_form` on a primitive read its
/// `DEFUN` intspec: `forward-char` is a command, `car` is not, an intspec
/// starting with `(` is the form it reads as, and an alias of the same subr
/// object (`search-forward-regexp`) is one too.
#[test]
fn primitive_commands_have_their_intspecs() {
    check(
        r##"(condition-case e (list (commandp 'forward-char) (commandp (symbol-function 'erase-buffer)) (commandp 'search-forward-regexp) (commandp 'goto-char t) (interactive-form 'forward-char) (interactive-form 'goto-char) (interactive-form 'write-region) (interactive-form 'upcase-word) (interactive-form 'car)) (error (list 'signal e)))"##,
        "(t t t t (interactive \"^p\") (interactive (goto-char--read-natnum-interactive \"Go to char: \")) \
(interactive \"r\nFWrite region to file: \ni\ni\ni\np\") (interactive \"p\") nil)",
    );
}
