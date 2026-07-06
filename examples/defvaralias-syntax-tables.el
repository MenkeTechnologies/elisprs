;;; defvaralias-syntax-tables.el --- variable aliases + Lisp-mode syntax tables  -*- lexical-binding: nil; -*-

;; Pins two byte-run.el/elisp-mode.el features that self-contained libraries load
;; through: `defvaralias'/`define-obsolete-variable-alias' (url-vars, url-util,
;; auth-source, epg-config, network-stream, soap-client) and the Lisp-mode syntax
;; tables `lisp-data-mode-syntax-table' / `emacs-lisp-mode-syntax-table'
;; (ietf-drums, rfc2047, mail-parse, the url-* parsers).  Every asserted value was
;; verified against `emacs -Q --batch' on Emacs 30.2.

(message "== defvaralias + syntax tables demo ==")

;; ---- defvaralias: aliased variables share one value cell ----
(ert-deftest dva-setq-alias-sets-base ()
  ;; emacs -Q: (setq alias v) writes the base variable => (99 99)
  (defvar dva-b 10)
  (defvaralias 'dva-a 'dva-b)
  (setq dva-a 99)
  (should (equal '(99 99) (list dva-b dva-a))))

(ert-deftest dva-set-base-sets-alias ()
  ;; emacs -Q: reading the alias reflects a write to the base => 7
  (defvar dvb-b 1)
  (defvaralias 'dvb-a 'dvb-b)
  (setq dvb-b 7)
  (should (= 7 dvb-a)))

(ert-deftest dva-unbound-base-inherits-alias-value ()
  ;; emacs -Q: when BASE is void and ALIAS has a value, BASE inherits it => (42 42)
  (setq dvc-a 42)
  (defvar dvc-b)
  (defvaralias 'dvc-a 'dvc-b)
  (should (equal '(42 42) (list dvc-a dvc-b))))

(ert-deftest dva-indirect-variable ()
  ;; emacs -Q: (indirect-variable alias) => base symbol
  (defvar dvd-b 1)
  (defvaralias 'dvd-a 'dvd-b)
  (should (eq 'dvd-b (indirect-variable 'dvd-a)))
  ;; A plain variable is its own indirection.
  (should (eq 'dvd-b (indirect-variable 'dvd-b))))

(ert-deftest dva-makunbound-clears-both ()
  ;; emacs -Q: makunbound on the alias unbinds the shared cell => (nil nil)
  (defvar dve-b 5)
  (defvaralias 'dve-a 'dve-b)
  (makunbound 'dve-a)
  (should (equal '(nil nil) (list (boundp 'dve-a) (boundp 'dve-b)))))

(ert-deftest dva-special-variable-p-follows-alias ()
  ;; emacs -Q: defvaralias makes the alias (and base) special => (t t)
  (setq dvf-b 3)
  (defvaralias 'dvf-a 'dvf-b)
  (should (equal '(t t)
                 (list (and (special-variable-p 'dvf-a) t)
                       (and (special-variable-p 'dvf-b) t)))))

(ert-deftest dva-let-binds-shared-cell ()
  ;; emacs -Q: let-binding the alias rebinds the base for the dynamic extent,
  ;; then restores it => (50 1)
  (defvar dvg-b 1)
  (defvaralias 'dvg-a 'dvg-b)
  (should (equal '(50 1) (list (let ((dvg-a 50)) dvg-b) dvg-b))))

(ert-deftest dva-define-obsolete-variable-alias ()
  ;; emacs -Q: define-obsolete-variable-alias aliases + marks obsolete => 3
  (defvar dvh-new 3)
  (define-obsolete-variable-alias 'dvh-old 'dvh-new "30.1")
  (should (= 3 dvh-old))
  ;; The obsolete-variable property is recorded for the byte-compiler.
  (should (equal '(dvh-new nil "30.1") (get 'dvh-old 'byte-obsolete-variable))))

(ert-deftest dva-cyclic-indirection-signals ()
  ;; emacs -Q: a self-referential alias chain signals cyclic-variable-indirection.
  (defvar dvi-a 1)
  (defvar dvi-b 2)
  (defvaralias 'dvi-a 'dvi-b)
  (should-error (defvaralias 'dvi-b 'dvi-a)
                :type 'cyclic-variable-indirection))

;; ---- Lisp-mode syntax tables (elisp-mode.el / lisp-mode.el) ----
(ert-deftest dva-emacs-lisp-mode-syntax-table ()
  ;; emacs -Q: char-syntax classes under emacs-lisp-mode-syntax-table.
  (should (syntax-table-p emacs-lisp-mode-syntax-table))
  (with-syntax-table emacs-lisp-mode-syntax-table
    (should (= ?< (char-syntax ?\;)))   ; ; opens a comment
    (should (= ?\( (char-syntax ?\[)))  ; [ is an open-paren
    (should (= ?_ (char-syntax ?@)))    ; @ is a symbol constituent
    (should (= ?w (char-syntax ?a)))    ; letters are words
    (should (= ?\" (char-syntax ?\")))  ; " is a string delimiter
    (should (= ?\\ (char-syntax ?\\)))) ; \ is an escape
  ;; The `@' p-flag is removed vs the lisp-data parent (bug#24542):
  ;; emacs-lisp = (3), lisp-data = (1048579).
  (should (equal '(3) (aref emacs-lisp-mode-syntax-table ?@)))
  (should (equal '(1048579) (aref lisp-data-mode-syntax-table ?@))))

(ert-run-tests-batch-and-exit)
;;; defvaralias-syntax-tables.el ends here
