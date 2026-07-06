;;; locate-library.el --- get-load-suffixes / locate-library  -*- lexical-binding: nil; -*-

;; Regression gate for the load-file resolution helpers:
;;   - `get-load-suffixes' (C `Fget_load_suffixes'): the cross product of
;;     `load-suffixes' with `load-file-rep-suffixes', in nreverse-cons order.
;;   - `locate-library' (subr.el): resolve a library name to the file `load'
;;     would pick, searching `load-path' with those suffixes.
;; Every value below was checked against `emacs -Q --batch'.
(message "== locate-library demo ==")

(ert-deftest get-load-suffixes-default ()
  "Default suffixes are the load × rep cross product; jka-compr's default
rep = (\"\" \".gz\") adds a `.gz' variant after each entry, so a stock `*.el.gz'
library resolves. (Emacs also prepends `.so'/`.dylib' module variants, which
elisprs omits — it loads no native modules.)"
  (should (equal (get-load-suffixes) '(".elc" ".elc.gz" ".el" ".el.gz"))))

(ert-deftest get-load-suffixes-cross-product-order ()
  "Order matches the C loop: suffix0+rep0, suffix0+rep1, suffix1+rep0, ..."
  (let ((load-suffixes '(".elc" ".el"))
        (load-file-rep-suffixes '("" ".gz")))
    (should (equal (get-load-suffixes)
                   '(".elc" ".elc.gz" ".el" ".el.gz")))))

(ert-deftest locate-library-finds-file ()
  "locate-library returns the absolute path found on the given PATH."
  (let* ((dir (file-name-as-directory
               (expand-file-name "examples/load-fixtures" default-directory)))
         (load-path (list dir)))
    ;; helper.el exists in the fixtures dir; suffix is added automatically.
    (should (equal (locate-library "helper" nil load-path)
                   (concat dir "helper.el")))
    ;; Explicit suffix also resolves.
    (should (equal (locate-library "helper.el" nil load-path)
                   (concat dir "helper.el")))
    ;; No match anywhere on the path -> nil.
    (should (eq (locate-library "no-such-lib-xyz" nil load-path) nil))))

(ert-deftest data-directory-is-absolute-string ()
  "data-directory is a real directory string (used by libraries via
expand-file-name), ending in a slash."
  (should (stringp data-directory))
  (should (string-suffix-p "/" data-directory))
  (should (file-name-absolute-p data-directory)))

(ert-run-tests-batch-and-exit)
