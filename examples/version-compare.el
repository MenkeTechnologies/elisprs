;;; version-compare.el --- faithful version comparison (subr.el:6944) -*- lexical-binding: nil; -*-

;; Regression gate for the version-comparison cluster ported from lisp/subr.el:
;;   version-to-list, version-list-<, version-list-=, version-list-<=,
;;   version-list-not-zero, version<, version<=, version=,
;;   plus the version-regexp-alist / version-separator data.
;;
;; Before this port, loading any file that guards on its own version at load
;; time (126 files in the Emacs 30.2 lisp tree: package.el, cl-lib, many
;; org-*.el, gnus, tramp, ...) failed with
;;   "Symbol's function definition is void: version<".
;;
;; The parser turns non-numeric qualifiers into negative priorities via
;; version-regexp-alist: snapshot/cvs/git/unknown => -4, alpha => -3,
;; beta => -2, pre/rc => -1; a trailing single letter 22.3a => ...3.1.
;;
;; Every asserted value below is what the real Emacs 30.2 binary reports
;; (emacs -Q --batch). ERT batch runner exits non-zero on any failure.
(message "== version-compare demo ==")

(ert-deftest vc-to-list-numeric ()
  "Plain dotted numbers parse to their integer list; leading `.' gets a 0.
Real Emacs: (version-to-list \"1.0.7.5\") => (1 0 7 5); \".5\" => (0 5)."
  (should (equal (version-to-list "1.0.7.5") '(1 0 7 5)))
  (should (equal (version-to-list ".5") '(0 5)))
  (should (equal (version-to-list "30.2") '(30 2)))
  (should (equal (version-to-list "1") '(1))))

(ert-deftest vc-to-list-qualifiers ()
  "Non-numeric qualifiers map to their alist priorities (case-insensitive).
Real Emacs: snapshot/git/cvs => -4, alpha => -3, beta => -2, pre/rc => -1."
  (should (equal (version-to-list "0.9snapshot") '(0 9 -4)))
  (should (equal (version-to-list "1.0-git") '(1 0 -4)))
  (should (equal (version-to-list "1.0.cvs") '(1 0 -4)))
  (should (equal (version-to-list "0.9AlphA1") '(0 9 -3 1)))
  (should (equal (version-to-list "22.8beta3") '(22 8 -2 3)))
  (should (equal (version-to-list "1.0PRE2") '(1 0 -1 2)))
  (should (equal (version-to-list "10.11.12rc1") '(10 11 12 -1 1))))

(ert-deftest vc-to-list-trailing-letter ()
  "A single trailing letter becomes its 1-based alphabet position.
Real Emacs: (version-to-list \"22.3a\") => (22 3 1)."
  (should (equal (version-to-list "22.3a") '(22 3 1))))

(ert-deftest vc-to-list-invalid ()
  "Invalid syntax signals an `error'; the two failure classes differ in message.
Real Emacs: non-number start => `(must start with a number)'; bad tail => plain."
  (should-error (version-to-list "1.0prepre2"))
  (should-error (version-to-list "22.8X3"))
  (should-error (version-to-list "alpha3.2"))
  (should-error (version-to-list 42)))

(ert-deftest vc-list-not-zero ()
  "version-list-not-zero returns the first non-zero, else 0.
Real Emacs: (version-list-not-zero '(0 0 3 4)) => 3; all-zero/nil => 0."
  (should (= (version-list-not-zero '(0 0 3 4)) 3))
  (should (= (version-list-not-zero '(0 0 0)) 0))
  (should (= (version-list-not-zero '(-2 5)) -2))
  (should (= (version-list-not-zero nil) 0)))

(ert-deftest vc-string-compare-numeric ()
  "Trailing .0 is insignificant; longer numeric tail is newer.
Real Emacs: (version< \"1\" \"1.0\") => nil; (version= \"1\" \"1.0\") => t."
  (should (eq (version< "1" "1.0") nil))
  (should (eq (version= "1" "1.0") t))
  (should (eq (version<= "1" "1.0") t))
  (should (eq (version< "24.4" "24.5") t))
  (should (eq (version< "1.0" "1.0.0.1") t))
  (should (eq (version= "1.0" "1.0.0.1") nil)))

(ert-deftest vc-string-compare-qualifier-ordering ()
  "Release > pre > beta > alpha > snapshot at the same numeric prefix.
Real Emacs: (version< \"1pre\" \"1\") => t, (version< \"1beta\" \"1pre\") => t."
  (should (eq (version< "1snapshot" "1alpha") t))
  (should (eq (version< "1alpha" "1beta") t))
  (should (eq (version< "1beta" "1pre") t))
  (should (eq (version< "1pre" "1") t))
  (should (eq (version< "22.8beta2" "22.8beta3") t))
  (should (eq (version= "22.8beta3" "22.8beta3") t)))

(ert-deftest vc-regexp-alist-data ()
  "The ported priority data matches subr.el value-for-value.
Real Emacs: (cdr (assoc \"^[-._+ ]?alpha$\" version-regexp-alist)) => -3."
  (should (string= version-separator "."))
  (should (= (cdr (assoc "^[-._+ ]?alpha$" version-regexp-alist)) -3))
  (should (= (cdr (assoc "^[-._+ ]?beta$" version-regexp-alist)) -2))
  (should (= (cdr (assoc "^[-._+ ]?snapshot$" version-regexp-alist)) -4)))

(ert-run-tests-batch-and-exit)
;;; version-compare.el ends here
