;;; custom-autoload.el --- faithful `custom-autoload' (custom.el:659)  -*- lexical-binding: nil; -*-

;; Regression gate for `custom-autoload', the function generated `loaddefs.el'
;; files (loaddefs.el, ps-print-loaddefs.el, dired-loaddefs.el, gnus.el,
;; tramp-loaddefs.el, org-loaddefs.el ...) call at load time to register a
;; customizable variable's autoload dependency. Before this port, loading any
;; such file failed with "Symbol's function definition is void: custom-autoload".
;;
;; Faithful body (custom.el:659):
;;   (put symbol 'custom-autoload (if noset 'noset t))
;;   (custom-add-load symbol load)
;;
;; Every asserted value below is what the real Emacs 30.2 binary reports
;; (emacs -Q --batch). ERT batch runner exits non-zero on any failure.
(message "== custom-autoload demo ==")

(ert-deftest custom-autoload-sets-autoload-prop-t ()
  "With NOSET nil, the `custom-autoload' property is set to t and LOAD registered.
Real Emacs: (get 'v 'custom-autoload) => t, (get 'v 'custom-loads) => (\"lib\")."
  (custom-autoload 'ca-foo "ca-foo-lib")
  (should (eq (get 'ca-foo 'custom-autoload) t))
  (should (equal (get 'ca-foo 'custom-loads) '("ca-foo-lib"))))

(ert-deftest custom-autoload-noset-marks-noset ()
  "With NOSET non-nil, the `custom-autoload' property is the symbol `noset'.
Real Emacs: (get 'v 'custom-autoload) => noset."
  (custom-autoload 'ca-bar "ca-bar-lib" t)
  (should (eq (get 'ca-bar 'custom-autoload) 'noset))
  (should (equal (get 'ca-bar 'custom-loads) '("ca-bar-lib"))))

(ert-deftest custom-autoload-dedups-loads ()
  "Re-registering the same LOAD does not duplicate it (custom-add-load `member').
Real Emacs: two calls with the same load => loads list stays (\"lib\")."
  (custom-autoload 'ca-dup "ca-dup-lib")
  (custom-autoload 'ca-dup "ca-dup-lib")
  (should (equal (get 'ca-dup 'custom-loads) '("ca-dup-lib"))))

(ert-deftest custom-autoload-prepends-new-load ()
  "A distinct LOAD is consed on the front of the existing `custom-loads' list.
Real Emacs: after (\"l1\") then l2 => (\"l2\" \"l1\")."
  (custom-autoload 'ca-two "ca-l1")
  (custom-autoload 'ca-two "ca-l2")
  (should (equal (get 'ca-two 'custom-loads) '("ca-l2" "ca-l1"))))

(ert-deftest custom-autoload-returns-loads-list ()
  "`custom-autoload' returns the value of the tail `custom-add-load' call,
i.e. the `put' of `custom-loads' => the new loads list.
Real Emacs: (custom-autoload 'v \"lib\") => (\"lib\")."
  (should (equal (custom-autoload 'ca-ret "ca-ret-lib") '("ca-ret-lib"))))

(ert-run-tests-batch-and-exit)
;;; custom-autoload.el ends here
