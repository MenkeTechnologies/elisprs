;;; system-identity.el --- build/system identity vars, ERT-tested  -*- lexical-binding: t; -*-

;; Pins the C-level identity variables from emacs.c that real init files read
;; at load time: `emacs-version', `emacs-major-version', `emacs-minor-version',
;; `system-type', `window-system', `noninteractive'. Before these existed,
;; loading stock lisp (bindings.el, version.el, arc-mode.el, hfy-cmap.el, ...)
;; died with "Symbol's value as variable is void: system-type" etc.
;; Every `should' is value-for-value verified against GNU Emacs 30.2 -Q --batch.

(ert-deftest system-identity-emacs-version ()
  ;; `emacs-version' is a non-empty version string beginning "MAJOR.MINOR".
  (should (stringp emacs-version))
  (should (string-match "\\`[0-9]+\\.[0-9]+" emacs-version)))

(ert-deftest system-identity-major-minor ()
  ;; Both are integers; major positive, minor non-negative.
  (should (integerp emacs-major-version))
  (should (integerp emacs-minor-version))
  (should (> emacs-major-version 0))
  (should (>= emacs-minor-version 0))
  ;; Derived from `emacs-version' exactly as lisp/version.el does.
  (should (= emacs-major-version
             (string-to-number
              (progn (string-match "^[0-9]+" emacs-version)
                     (match-string 0 emacs-version)))))
  (should (= emacs-minor-version
             (string-to-number
              (progn (string-match "^[0-9]+\\.\\([0-9]+\\)" emacs-version)
                     (match-string 1 emacs-version))))))

(ert-deftest system-identity-system-type ()
  ;; A symbol, and one of the platform symbols Emacs's configure emits.
  (should (symbolp system-type))
  (should (memq system-type
                '(darwin gnu/linux windows-nt berkeley-unix android gnu cygwin))))

(ert-deftest system-identity-batch-flags ()
  ;; Under -Q --batch (and the elisp interpreter) there is no GUI and execution
  ;; is non-interactive.
  (should (eq window-system nil))
  (should (eq noninteractive t)))

(ert-run-tests-batch-and-exit)
