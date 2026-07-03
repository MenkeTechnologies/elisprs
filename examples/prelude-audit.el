;;; prelude-audit.el --- prelude correctness-audit fixes vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; Differential-tested against real `emacs -Q --batch' 30.2. Each case pins a
;; divergence fixed in prelude.rs: characterp's upper bound, number-sequence's
;; zero-increment guard (was an infinite loop), assoc-string coercing symbol
;; elements under case folding, cl-subst/cl-sublis honoring the :test keyword,
;; cl-pairlis stopping at the shorter list, and cl-remprop.
(require 'cl-lib)
(message "== prelude correctness audit ==")

(ert-deftest characterp-upper-bound ()
  "characterp is bounded at #x3FFFFF (MAX_CHAR); above it is not a character."
  (should (characterp #x3FFFFF))
  (should-not (characterp (1+ #x3FFFFF)))
  (should-not (characterp -1)))

(ert-deftest number-sequence-zero-increment ()
  "A zero increment signals rather than looping forever; FROM=TO short-circuits."
  (should-error (number-sequence 1 10 0))
  (should (equal (number-sequence 5 5 0) '(5)))
  (should (equal (number-sequence 1 7 3) '(1 4 7)))
  (should (equal (number-sequence 5 1 -1) '(5 4 3 2 1))))

(ert-deftest assoc-string-symbol-case-fold ()
  "Symbol elements coerce to their names even when CASE-FOLD is set."
  (should (eq (assoc-string "FOO" '(foo bar) t) 'foo))
  (should (equal (assoc-string "a" '("A" "b") t) "A")))

(ert-deftest cl-subst-keyword-test ()
  "cl-subst honors :test, matching whole subtrees like cl-sublis."
  (should (equal (cl-subst 0 "x" '("x" "y" "x") :test #'equal) '(0 "y" 0)))
  (should (equal (cl-subst 'X '(a) '((a) b (a)) :test #'equal) '(X b X)))
  ;; The default (no keyword) path still uses eql throughout the tree.
  (should (equal (cl-subst 'x 'a '(a b (a c) a)) '(x b (x c) x))))

(ert-deftest cl-sublis-basic ()
  "cl-sublis substitutes per (OLD . NEW) alist across the tree, honoring :test."
  (should (equal (cl-sublis '((a . x) (b . y)) '(a b (a . b))) '(x y (x . y))))
  (should (equal (cl-sublis '(("k" . 9)) '("k" z "k") :test #'equal) '(9 z 9))))

(ert-deftest cl-pairlis-unequal-length ()
  "cl-pairlis stops at the shorter list and prepends to ALIST."
  (should (equal (cl-pairlis '(a b c) '(1 2)) '((a . 1) (b . 2))))
  (should (equal (cl-pairlis '(a b) '(1 2 3)) '((a . 1) (b . 2))))
  (should (equal (cl-pairlis '(a b) '(1 2) '((c . 3))) '((a . 1) (b . 2) (c . 3)))))

(ert-deftest cl-remprop-removes-one ()
  "cl-remprop drops one PROPNAME/value pair, returning t when present else nil."
  (put 'prelude-audit-sym 'a 1)
  (put 'prelude-audit-sym 'b 2)
  (should (eq (cl-remprop 'prelude-audit-sym 'a) t))
  (should (equal (symbol-plist 'prelude-audit-sym) '(b 2)))
  (should-not (cl-remprop 'prelude-audit-sym 'nope)))

(ert-run-tests-batch-and-exit)
