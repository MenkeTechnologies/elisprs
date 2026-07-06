;;; mounted-file-systems.el --- files.el pure-data regexps  -*- lexical-binding: nil; -*-

;; Pins two build-independent regexp variables from GNU Emacs 30.2 files.el:
;;   `mounted-file-systems'            (defcustom, files.el:1722)
;;   `locate-dominating-stop-dir-regexp' (defvar,  files.el:1180)
;; Both are fixed data strings read by directory-traversal code
;; (`temporary-file-directory' / `locate-dominating-file'), so they must be
;; bound with the exact oracle value before the corpus loads. `mounted-file-
;; systems' keeps its platform `if' intact so the value is correct on any
;; `system-type'; on darwin/linux the else branch (the regexp-opt alternation)
;; is used. Every asserted value/behavior was verified against
;; `emacs -Q --batch' on Emacs 30.2.

(message "== mounted-file-systems demo ==")

;; ---- exact oracle values ----
(ert-deftest mfs-exact-values ()
  (should (boundp 'mounted-file-systems))
  (should (boundp 'locate-dominating-stop-dir-regexp))
  ;; emacs -Q (darwin): the regexp-opt alternation branch.
  (should (equal "^\\(?:/\\(?:afs/\\|m\\(?:edia/\\|nt\\)\\|\\(?:ne\\|tmp_mn\\)t/\\)\\)"
                 mounted-file-systems))
  ;; emacs -Q: the purecopy'd stop-dir regexp.
  (should (equal "\\`\\(?:[\\/][\\/][^\\/]+[\\/]\\|/\\(?:net\\|afs\\|\\.\\.\\.\\)/\\)\\'"
                 locate-dominating-stop-dir-regexp)))

;; ---- `mounted-file-systems' matches mount-point prefixes ----
(ert-deftest mfs-regexp-dispatch ()
  ;; emacs -Q: /afs/ /media/ /mnt /net/ /tmp_mnt/ all match; /home does not.
  (should (string-match mounted-file-systems "/afs/x"))
  (should (string-match mounted-file-systems "/media/x"))
  (should (string-match mounted-file-systems "/mnt"))
  (should (string-match mounted-file-systems "/net/x"))
  (should (string-match mounted-file-systems "/tmp_mnt/x"))
  (should (eq nil (string-match mounted-file-systems "/home/x"))))

;; ---- `locate-dominating-stop-dir-regexp' is fully anchored (\` ... \') ----
(ert-deftest mfs-stop-dir-anchored ()
  ;; emacs -Q: whole-string matches only.
  (should (string-match locate-dominating-stop-dir-regexp "//host/"))
  (should (string-match locate-dominating-stop-dir-regexp "/net/"))
  (should (string-match locate-dominating-stop-dir-regexp "/afs/"))
  (should (string-match locate-dominating-stop-dir-regexp "/.../"))
  ;; A trailing component defeats the closing `\'' anchor.
  (should (eq nil (string-match locate-dominating-stop-dir-regexp "/net/x")))
  (should (eq nil (string-match locate-dominating-stop-dir-regexp "/home/"))))

;; ---- defcustom metadata: `mounted-file-systems' carries a standard-value ----
(ert-deftest mfs-custom-metadata ()
  ;; defcustom records a standard-value list (custom-declare-variable).
  (should (get 'mounted-file-systems 'standard-value))
  ;; Dynamic (special) variable: `let' rebinds it, value restored afterward.
  (should (equal "^//[^/]+/"
                 (let ((mounted-file-systems "^//[^/]+/")) mounted-file-systems)))
  (should (equal "^\\(?:/\\(?:afs/\\|m\\(?:edia/\\|nt\\)\\|\\(?:ne\\|tmp_mn\\)t/\\)\\)"
                 mounted-file-systems)))

(ert-run-tests-batch-and-exit)
