;;; file-name-handler-alist.el --- fileio.c magic-file-name dispatch alist  -*- lexical-binding: nil; -*-

;; Pins `file-name-handler-alist' (C `Vfile_name_handler_alist', fileio.c), the
;; alist of (REGEXP . HANDLER) pairs that file-name operations consult to
;; dispatch magic file names. The C variable initializes to nil; the value
;; asserted here is the post-loadup default Emacs 30.2 reports under
;; `emacs -Q --batch' — the epa, jka-compr, tramp, and `file-name-non-special'
;; handlers. files.el reads and let-binds this variable (e.g. lines 1515/1546),
;; so it must be bound with the exact oracle value before the corpus loads.
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.

(message "== file-name-handler-alist demo ==")

;; ---- bound with the exact 4-entry batch default ----
(ert-deftest fnha-default-shape ()
  (should (boundp 'file-name-handler-alist))
  ;; emacs -Q: (length file-name-handler-alist) => 4
  (should (eq 4 (length file-name-handler-alist)))
  ;; emacs -Q: first key is the .gpg regexp -> epa-file-handler
  (should (equal "\\.gpg\\(~\\|\\.~[0-9]+~\\)?\\'" (caar file-name-handler-alist)))
  (should (eq 'epa-file-handler (cdar file-name-handler-alist)))
  ;; emacs -Q: (car (rassq 'file-name-non-special ...)) => "\\`/:"
  (should (equal "\\`/:" (car (rassq 'file-name-non-special file-name-handler-alist))))
  ;; Every value is a HANDLER symbol.
  (should (equal '(epa-file-handler jka-compr-handler
                   tramp-autoload-file-name-handler file-name-non-special)
                 (mapcar #'cdr file-name-handler-alist))))

;; ---- regexp dispatch via the alist (as file-name ops do internally) ----
(ert-deftest fnha-regexp-dispatch ()
  ;; emacs -Q: a .gz name matches the jka-compr regexp.
  (should (eq 'jka-compr-handler
              (assoc-default "foo.el.gz" file-name-handler-alist 'string-match)))
  ;; emacs -Q: a "/:"-quoted name matches file-name-non-special.
  (should (eq 'file-name-non-special
              (assoc-default "/:/quoted" file-name-handler-alist 'string-match)))
  ;; emacs -Q: an ordinary name matches no handler.
  (should (eq nil
              (assoc-default "/home/x/foo.el" file-name-handler-alist 'string-match))))

;; ---- dynamic (special) variable: let-binding rebinds it, as files.el relies on
;; when it needs to suppress magic-file-name handling around an operation. ----
(ert-deftest fnha-let-rebind ()
  ;; emacs -Q: (let (file-name-handler-alist) file-name-handler-alist) => nil
  (should (eq nil (let (file-name-handler-alist) file-name-handler-alist)))
  ;; The binding is restored after the `let'.
  (should (eq 4 (length file-name-handler-alist)))
  ;; A custom rebind is visible inside the dynamic extent.
  (should (equal '((".x" . my-handler))
                 (let ((file-name-handler-alist '((".x" . my-handler))))
                   file-name-handler-alist))))

(ert-run-tests-batch-and-exit)
