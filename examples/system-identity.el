;;; system-identity.el --- build/system identity vars, ERT-tested  -*- lexical-binding: t; -*-

;; Pins the C-level identity variables from emacs.c that real init files read
;; at load time: `emacs-version', `emacs-major-version', `emacs-minor-version',
;; `system-type', `window-system', `noninteractive', `emacs-build-system',
;; `emacs-build-time', `emacs-build-number', and the `system-name' subr.
;; Before these existed,
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

(ert-deftest system-identity-system-name ()
  ;; `system-name' (Fsystem_name) returns the host name as a non-empty string.
  (should (fboundp 'system-name))
  (should (stringp (system-name)))
  (should (> (length (system-name)) 0)))

(ert-deftest system-identity-build-vars ()
  ;; version.el defines these three. `emacs-build-system' is a non-empty host
  ;; string. (In a stock binary it is the host Emacs was *built* on, which can
  ;; differ from the runtime `(system-name)'; elisprs has no separate build
  ;; host, so there it equals the runtime host. Only the string-ness is pinned
  ;; here, since that holds in both.)
  (should (stringp emacs-build-system))
  (should (> (length emacs-build-system) 0))
  ;; `emacs-build-number' is the integer 1 in a stock build.
  (should (integerp emacs-build-number))
  (should (= emacs-build-number 1))
  ;; `emacs-build-time' is `(if emacs-build-system (current-time))'. Because the
  ;; build system is non-nil, it is a 4-element timestamp list of integers,
  ;; matching a normally-dumped `emacs -Q --batch'.
  (should emacs-build-time)
  (should (consp emacs-build-time))
  (should (= (length emacs-build-time) 4))
  (should (integerp (nth 0 emacs-build-time)))
  (should (integerp (nth 3 emacs-build-time))))

(ert-run-tests-batch-and-exit)
