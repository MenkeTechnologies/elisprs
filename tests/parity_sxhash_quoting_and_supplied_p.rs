//! `sxhash-equal` / `sxhash-eql` / `sxhash-eq` values, `text-quoting-style`,
//! supplied-p variables in cl lambda lists, and `pcase-let`'s evaluation order.
//!
//! - fns.c `sxhash_obj` (64-bit build): fixnums are their 62-bit value,
//!   floats their bits, bignums sign plus limbs, strings `hash_char_array` of
//!   the UTF-8 bytes, lists and vectors combined to depth 3 and length 7,
//!   bool-vectors by words; `reduce_emacs_uint_to_fixnum` folds the result into
//!   a (possibly negative) fixnum. A fixnum's `sxhash-eq` / `sxhash-eql` is
//!   `XHASH ^ XTYPE`.
//! - doc.c `Ftext_quoting_style`: `grave` and `straight` are honoured by
//!   `format-message`, `error`, `substitute-command-keys` and help.el's
//!   `substitute-quotes`; minibuffer.el's `format-prompt`.
//! - cl-macs.el: `(VAR DEFAULT SVAR)` binds SVAR to whether the optional or
//!   keyword argument was supplied, and `((KEYWORD VAR) ...)` names the keyword.
//! - pcase.el `pcase-let` evaluates every EXP before binding any pattern.
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
fn sxhash_follows_fns_c() {
    check(
        r##"(condition-case e (list (sxhash-equal 1) (sxhash-equal -1) (sxhash-equal 0.0) (sxhash-equal -0.0) (sxhash-eql 1.5) (sxhash-eql 2.5)) (error (list 'signal e)))"##,
        r##"(1 -1152921504606846976 0 -2305843009213693952 -1151232654746583040 1154328879490400256)"##,
    );
    check(
        r##"(condition-case e (list (sxhash-equal (expt 2 70)) (sxhash-equal (- (expt 2 70))) (sxhash-eql (expt 2 70))) (error (list 'signal e)))"##,
        r##"(80 272 80)"##,
    );
    check(
        r##"(condition-case e (list (sxhash-equal "abc") (sxhash-equal "é") (sxhash-equal "abcdefghijklmnopqrstuvwxyz") (sxhash-equal "") (sxhash-equal "abcdefgh")) (error (list 'signal e)))"##,
        r##"(8059383 33691 2176588137541852513 0 1543256470076781674)"##,
    );
    check(
        r##"(condition-case e (list (sxhash-equal [1 2]) (sxhash-equal (list 1 2.0 "x")) (sxhash-equal '(1 . 2)) (sxhash-equal nil) (sxhash-equal (number-sequence 1 10)) (sxhash-equal '(1 (2 (3 (4 (5))))))) (error (list 'signal e)))"##,
        r##"(662 495 22 0 381241544 120)"##,
    );
    check(
        r##"(condition-case e (list (sxhash-equal (make-bool-vector 3 t)) (sxhash-equal (make-bool-vector 70 t)) (sxhash-equal (make-bool-vector 0 nil))) (error (list 'signal e)))"##,
        r##"(58 22436 0)"##,
    );
    check(
        r##"(condition-case e (list (sxhash-eql 1) (sxhash-eq 2) (sxhash-eql -1) (sxhash-eq most-positive-fixnum) (sxhash-eq most-negative-fixnum)) (error (list 'signal e)))"##,
        r##"(6 0 -1152921504606846969 1729382256910270471 -1729382256910270462)"##,
    );
}

#[test]
fn text_quoting_style_selects_the_quotes() {
    check(
        r##"(condition-case e (let ((text-quoting-style 'grave)) (list (format-message "`foo' %s" "`x'") (substitute-command-keys "`a'") (condition-case e (error "`b'") (error (cadr e))) (text-quoting-style))) (error (list 'signal e)))"##,
        r##"("`foo' `x'" "`a'" "`b'" grave)"##,
    );
    check(
        r##"(condition-case e (let ((text-quoting-style 'straight)) (list (format-message "`foo'") (substitute-command-keys "`a'") (condition-case e (user-error "`b'") (error (cadr e))) (text-quoting-style))) (error (list 'signal e)))"##,
        r##"("'foo'" "'a'" "'b'" straight)"##,
    );
    check(
        r##"(condition-case e (let ((text-quoting-style 'bogus)) (list (format-message "`foo'") (text-quoting-style))) (error (list 'signal e)))"##,
        r##"("‘foo’" curve)"##,
    );
    check(
        r##"(condition-case e (list text-quoting-style (text-quoting-style) (format-message "`x'")) (error (list 'signal e)))"##,
        r##"(nil curve "‘x’")"##,
    );
    check(
        r##"(condition-case e (list (substitute-quotes "`a'") (let ((text-quoting-style 'grave)) (substitute-quotes "`a'")) (let ((text-quoting-style 'straight)) (substitute-quotes "`a'"))) (error (list 'signal e)))"##,
        r##"("‘a’" "`a'" "'a'")"##,
    );
    check(
        r##"(condition-case e (list (format-prompt "Name" "def") (format-prompt "Name" nil) (format-prompt "Name" "") (format-prompt "N %d" '(7 8) 3) (let ((text-quoting-style 'straight)) (format-prompt "`x'" 5))) (error (list 'signal e)))"##,
        r##"("Name (default def): " "Name: " "Name: " "N 3 (default 7): " "'x' (default 5): ")"##,
    );
}

#[test]
fn supplied_p_variables() {
    check(
        r##"(condition-case e (cl-destructuring-bind (a &optional (b 9 bp)) '(1) (list a b bp)) (error (list 'signal e)))"##,
        r##"(1 9 nil)"##,
    );
    check(
        r##"(condition-case e (cl-destructuring-bind (a &optional (b 9 bp)) '(1 nil) (list a b bp)) (error (list 'signal e)))"##,
        r##"(1 nil t)"##,
    );
    check(
        r##"(condition-case e (cl-destructuring-bind (&key (a 1 ap) ((:bee b) 2 bp)) '(:bee 5) (list a ap b bp)) (error (list 'signal e)))"##,
        r##"(1 nil 5 t)"##,
    );
    check(
        r##"(condition-case e (progn (cl-defun zz-k2 (&key (a 1 ap) b) (list a ap b)) (list (zz-k2 :b 2) (zz-k2 :a nil))) (error (list 'signal e)))"##,
        r##"((1 nil 2) (nil t nil))"##,
    );
    check(
        r##"(condition-case e (progn (cl-defun zz-k5 (x &optional (y 2 yp) &key (z 3 zp)) (list x y yp z zp)) (list (zz-k5 1) (zz-k5 1 5 :z 6))) (error (list 'signal e)))"##,
        r##"((1 2 nil 3 nil) (1 5 t 6 t))"##,
    );
}

#[test]
fn pcase_let_evaluates_every_exp_first() {
    check(
        r##"(condition-case e (let ((a 1)) (pcase-let ((a 2) (b a)) (list a b))) (error (list 'signal e)))"##,
        r##"(2 1)"##,
    );
    check(
        r##"(condition-case e (let ((a 1)) (pcase-let ((`(,a) '(2)) (b a)) (list a b))) (error (list 'signal e)))"##,
        r##"(2 1)"##,
    );
    check(
        r##"(condition-case e (let ((a 1)) (pcase-let* ((`(,a) '(2)) (b a)) (list a b))) (error (list 'signal e)))"##,
        r##"(2 2)"##,
    );
    check(
        r##"(condition-case e (pcase-let ((_ 1) (x 2)) x) (error (list 'signal e)))"##,
        r##"2"##,
    );
    check(
        r##"(condition-case e (let ((n 0)) (pcase-let ((`(,a ,b) (list (setq n (1+ n)) n)) (c (setq n (* 10 n)))) (list a b c n))) (error (list 'signal e)))"##,
        r##"(1 1 10 10)"##,
    );
}
