;;; custom-load-preload.el --- preloaded custom.el var + faithful `require'  -*- lexical-binding: nil; -*-

;; Regression gate for the preloaded-Emacs definitions elisprs supplies so a real
;; init file (Spacemacs `init.el') can load: the `custom-theme-load-path' defvar
;; (custom.el:1216) and the faithful `require' primitive (C `Frequire', fns.c),
;; which loads a feature's file from `load-path' instead of erroring. Each value
;; is asserted against what the real Emacs 30.2 binary reports.
;;
;; ERT batch runner errors (→ non-zero exit) on any failure, so this doubles as
;; the CI self-test for these ports.
(message "== custom + require preload demo ==")

(defun cust-fixture (name)
  "Absolute path of load-fixture NAME."
  (expand-file-name (concat "examples/load-fixtures/" name)))

(ert-deftest custom-theme-load-path-default ()
  "`custom-theme-load-path' is the preloaded defvar from custom.el:1216.
Real Emacs 30.2: (default-value 'custom-theme-load-path) => (custom-theme-directory t)."
  ;; The two symbols are quoted in the list, so the value does not depend on
  ;; `custom-theme-directory' being bound.
  (should (equal (default-value 'custom-theme-load-path)
                 '(custom-theme-directory t)))
  ;; init files mutate it with `add-to-list' (Spacemacs core-load-paths.el:116).
  (let ((custom-theme-load-path (default-value 'custom-theme-load-path)))
    (add-to-list 'custom-theme-load-path "/some/theme/dir")
    (should (member "/some/theme/dir" custom-theme-load-path))
    (should (member 'custom-theme-directory custom-theme-load-path))))

(ert-deftest require-loads-feature-file ()
  "`require' loads the feature's file when not yet provided, and returns FEATURE.
Real Emacs: (require 'reqfeat FILE) => reqfeat, and the file's forms run once."
  (should-not (featurep 'reqfeat))
  (let ((r (require 'reqfeat (cust-fixture "reqfeat.el"))))
    (should (eq r 'reqfeat))
    (should (featurep 'reqfeat))
    ;; The loaded file's `provide' + `defvar' took effect in the live host.
    (should (eq reqfeat-marker 'reqfeat-was-loaded))
    (should (= reqfeat-load-count 1)))
  ;; A second `require' is a no-op: already provided, file is NOT re-loaded.
  (should (eq (require 'reqfeat (cust-fixture "reqfeat.el")) 'reqfeat))
  (should (= reqfeat-load-count 1)))

(ert-deftest require-already-provided-short-circuits ()
  "`require' of an already-provided feature returns it without loading a file.
Real Emacs: (progn (provide 'x) (require 'x)) => x."
  (provide 'cust-preload-already)
  (should (eq (require 'cust-preload-already) 'cust-preload-already))
  ;; No file named cust-preload-already.el exists anywhere; still succeeds.
  (should (eq (require 'cust-preload-already "/no/such/path.el")
              'cust-preload-already)))

(ert-deftest require-noerror-missing-file-returns-nil ()
  "With NOERROR, `require' of a missing file returns nil (not an error).
Real Emacs: (require 'no-such-xyz nil t) => nil."
  (should-not (featurep 'cust-no-such-xyz))
  (should (eq (require 'cust-no-such-xyz nil t) nil)))

(ert-deftest require-missing-file-signals-file-missing ()
  "Without NOERROR, `require' of a missing file signals `file-missing' (from `load').
Real Emacs: (require 'no-such-xyz) => signal file-missing \"Cannot open load file\"."
  (should-not (featurep 'cust-no-such-xyz2))
  (should (eq (condition-case _ (require 'cust-no-such-xyz2)
               (file-missing 'caught))
              'caught)))

(ert-deftest require-file-without-provide-errors ()
  "If the loaded file does not `provide' the feature, `require' signals an error.
Real Emacs 30.2: \"Loading file FILE failed to provide feature `noprov'\"."
  (should-not (featurep 'noprov))
  (should (eq (condition-case _ (require 'noprov (cust-fixture "noprov.el"))
               (error 'caught))
              'caught))
  ;; The file WAS loaded (its side effect ran) before the error was raised.
  (should (eq noprov-marker 'noprov-was-loaded)))

(ert-run-tests-batch-and-exit)
;;; custom-load-preload.el ends here
