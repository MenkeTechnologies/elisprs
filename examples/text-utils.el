;;; text-utils.el --- string/format/alist/map/seq pipelines, ERT-tested  -*- lexical-binding: nil; -*-

;; Stresses the data-munging surface a real script leans on: format directives,
;; string trimming/splitting/joining, alist & map manipulation, sorting with
;; keys, and seq pipelines. Passes under GNU Emacs too.
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'map)

(defun parse-kv (line)
  "Parse \"key=value\" into a (KEY . VALUE) cons, trimming whitespace."
  (let ((parts (split-string line "=")))
    (cons (string-trim (car parts)) (string-trim (cadr parts)))))

(defun render-table (alist)
  "Render ALIST as aligned \"key : value\" lines sorted by key."
  (let* ((sorted (sort (copy-sequence alist)
                       (lambda (a b) (string< (car a) (car b)))))
         (width (seq-max (cons 0 (mapcar (lambda (kv) (length (car kv))) sorted)))))
    (mapconcat (lambda (kv)
                 (format "%s : %s" (string-pad (car kv) width) (cdr kv)))
               sorted "\n")))

(ert-deftest text-utils-parse-and-render ()
  (let* ((lines '(" name = Ada " "lang=elisp" "year = 1843"))
         (alist (mapcar #'parse-kv lines)))
    (should (equal alist '(("name" . "Ada") ("lang" . "elisp") ("year" . "1843"))))
    (should (equal (render-table alist)
                   "lang : elisp\nname : Ada\nyear : 1843"))))

(ert-deftest text-utils-format-directives ()
  (should (equal (format "%05.2f|%+d|%x|%c" 3.14159 7 255 ?A) "03.14|+7|ff|A"))
  (should (equal (format "%-8s|%8s|" "L" "R") "L       |       R|"))
  (should (equal (format "%S" '(1 "two" 3.0)) "(1 \"two\" 3.0)"))
  (should (equal (number-to-string (/ 22.0 7)) "3.142857142857143")))

(ert-deftest text-utils-seq-and-map ()
  (let ((m (map-into '((a . 1) (b . 2) (c . 3)) 'hash-table)))
    (should (= (map-elt m 'b) 2))
    (should (= (seq-reduce #'+ (map-values m) 0) 6)))
  ;; seq pipeline: evens, squared, summed.
  (should (= (seq-reduce #'+
                         (seq-map (lambda (x) (* x x))
                                  (seq-filter #'cl-evenp (number-sequence 1 10)))
                         0)
             220))
  ;; group-by: assert each group (Emacs's group ORDER is an internal quirk, so
  ;; look groups up by key rather than depending on their order).
  (let ((g (seq-group-by (lambda (n) (if (cl-evenp n) 'even 'odd)) '(1 2 3 4 5))))
    (should (equal (assq 'odd g) '(odd 1 3 5)))
    (should (equal (assq 'even g) '(even 2 4)))))

(ert-deftest text-utils-string-ops ()
  (should (equal (string-join (split-string "a, b ,c" " *, *") "|") "a|b|c"))
  (should (equal (string-pad "x" 4 ?.) "x..."))
  (should (equal (string-trim "\n  hi  \t") "hi"))
  (should (equal (mapconcat #'upcase '("a" "b" "c") "-") "A-B-C"))
  (should (equal (string-replace "_" " " "a_b_c") "a b c")))

(ert-run-tests-batch-and-exit)
