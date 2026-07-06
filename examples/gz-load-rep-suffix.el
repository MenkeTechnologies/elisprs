;;; gz-load-rep-suffix.el --- `load' finds and gunzips `*.el.gz' libraries  -*- lexical-binding: nil; -*-

;; The stock Emacs lisp tree ships every library compressed as `*.el.gz'. Emacs's
;; `load' finds them transparently via jka-compr's `load-file-rep-suffixes' =
;; `("" ".gz")': for each `load-suffixes' entry it also tries a `.gz' variant, and
;; a resolved `.gz' file is decompressed in memory before evaluation. So a library
;; that exists ONLY as `foo.el.gz' still loads by `(load "foo")', returns t, and
;; its defuns become available — exactly as if `foo.el' were present uncompressed.
;;
;; This exercises three resolution paths, each oracle-verified against
;; `emacs -Q --batch' (30.2): an explicit `.el.gz' path, a directory-qualified
;; basename (`load-suffixes' x rep-suffix search), and `load-path' resolution of a
;; `.gz'-only library. A plain `.el' is written, gzip'd, and the plain file deleted
;; so ONLY the `.gz' remains — the load must not fall back to an uncompressed copy.
(message "== gz-load rep-suffix demo ==")

;; Write ELISP to BASE.el, gzip it to BASE.el.gz, and delete the plain BASE.el so
;; only the compressed form remains. Returns BASE (the extension-less path).
(defun gzlt--make-gz-lib (base elisp)
  (let ((el (concat base ".el")))
    (write-region elisp nil el)
    (unless (= 0 (call-process "gzip" nil nil nil "-f" el))
      (error "gzip failed for %s" el))
    (when (file-exists-p el)
      (error "plain %s still present after gzip" el))
    (unless (file-exists-p (concat base ".el.gz"))
      (error "compressed %s.el.gz was not produced" base))
    base))

(defvar gzlt--tmp
  (expand-file-name (make-temp-name "elisprs-gz-") temporary-file-directory))

(ert-deftest gz-load-explicit-path ()
  "Loading an explicit `.el.gz' path gunzips it and defines its functions."
  (let ((base (gzlt--make-gz-lib (concat gzlt--tmp "-explicit")
                                 "(defun gzlt-explicit-fn () 42)\n")))
    (should (eq t (load (concat base ".el.gz"))))
    (should (fboundp 'gzlt-explicit-fn))
    (should (= 42 (gzlt-explicit-fn)))
    (delete-file (concat base ".el.gz"))))

(ert-deftest gz-load-basename-rep-suffix ()
  "A directory-qualified basename resolves BASE.el.gz via the `.gz' rep-suffix."
  (let ((base (gzlt--make-gz-lib (concat gzlt--tmp "-base")
                                 "(defun gzlt-base-fn () 99)\n")))
    ;; No `.el' exists; only `BASE.el.gz'. `(load BASE)' must still find it.
    (should (eq t (load base)))
    (should (= 99 (gzlt-base-fn)))
    (delete-file (concat base ".el.gz"))))

(ert-deftest gz-load-from-load-path ()
  "A `.gz'-only library is found by bare name through `load-path'."
  (let* ((dir (file-name-as-directory
               (concat gzlt--tmp "-lpdir")))
         (lib "gzltlplib"))
    (make-directory dir t)
    (gzlt--make-gz-lib (concat dir lib) "(defun gzlt-lp-fn () 7)\n")
    (let ((load-path (cons dir load-path)))
      (should (eq t (load lib)))
      (should (= 7 (gzlt-lp-fn))))
    (delete-file (concat dir lib ".el.gz"))))

(ert-run-tests-batch-and-exit)
