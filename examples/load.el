;;; load.el --- the `load' builtin: search, dynamic vars, noerror  -*- lexical-binding: nil; -*-

;; Every assertion runs through fusevm. The ERT suite is the regression gate:
;; `ert-run-tests-batch-and-exit' errors (→ non-zero exit) if any test fails.
;; The fixture it loads lives in examples/load-fixtures/ so run_examples.sh's
;; examples/*.el glob does not execute it standalone.
(message "== load demo ==")

;; Forward-declared so the fixture's `setq' has a special var to write into.
(defvar loadtest-file-name-during-load nil)
(defvar loadtest-in-progress-during-load nil)

(defun loadtest-fixture (name)
  "Absolute path of fixture NAME (relative to `default-directory')."
  (expand-file-name (concat "examples/load-fixtures/" name)))

(ert-deftest load-runs-file-and-binds-vars ()
  "load evaluates the file's forms in the live host and binds the load vars."
  ;; Capture the ambient values first: this file is itself run via `load'
  ;; (elisprs's `emacs -l FILE' path), so `load-file-name' is already bound to
  ;; *this* file here — exactly as in Emacs. A nested `load' must SAVE these and
  ;; RESTORE them on return, not clobber them to nil.
  (let ((outer-file load-file-name)
        (outer-in-progress load-in-progress))
    (let ((r (load (loadtest-fixture "helper.el"))))
      ;; Returns t on success.
      (should (eq r t))
      ;; A `defvar' in the loaded file is visible here — same host, not a reset.
      (should (eq loadtest-marker 'helper-was-loaded))
      ;; `load-file-name' was dynamically bound to the resolved path *during* load.
      (should (stringp loadtest-file-name-during-load))
      (should (string-suffix-p "examples/load-fixtures/helper.el"
                               loadtest-file-name-during-load))
      ;; `load-in-progress' was non-nil during the load.
      (should loadtest-in-progress-during-load)
      ;; …and both are unwound back to the OUTER values after `load' returns.
      (should (equal load-file-name outer-file))
      (should (eq load-in-progress outer-in-progress)))))

(ert-deftest load-suffix-search ()
  "load appends `.el' when FILE is given without an extension."
  (should (eq (load (loadtest-fixture "helper")) t)))

(ert-deftest load-noerror-missing ()
  "A missing file with NOERROR non-nil returns nil instead of signalling."
  (should (null (load (loadtest-fixture "does-not-exist") t))))

(ert-deftest load-missing-signals ()
  "A missing file without NOERROR signals an error."
  (should-error (load (loadtest-fixture "does-not-exist"))))

(ert-deftest script-run-binds-load-file-name ()
  "Running FILE via `elisp FILE' (the `eval_file' path) binds `load-file-name'
to FILE, matching `emacs -l FILE'. Real init files (e.g. Spacemacs `init.el')
do `(file-name-directory load-file-name)' to locate sibling files, so an unbound
`load-file-name' breaks them on the first form."
  (should (stringp load-file-name))
  (should (string-suffix-p "examples/load.el" load-file-name))
  (should load-in-progress))

(ert-run-tests-batch-and-exit)
