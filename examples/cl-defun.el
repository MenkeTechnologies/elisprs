;;; cl-defun.el --- cl-defun / cl-defmacro / cl-lambda-lists, ERT-tested  -*- lexical-binding: nil; -*-

;; cl-defun and cl-defmacro accept full cl-lambda-lists (&optional and &key with
;; per-arg defaults, &rest, &aux), built on cl-destructuring-bind in the prelude.
;; Defining a macro at top level (not inside one form) is what makes the macro
;; available to the forms that follow, so this lives in an example script rather
;; than the single-form eval tests.
(message "== cl-defun demo ==")

(cl-defun greet (name &key (greeting "Hello") loud)
  (let ((s (concat greeting ", " name)))
    (if loud (upcase s) s)))

(cl-defun scale (x &optional (factor 2))
  (* x factor))

(cl-defun collect (first &rest more)
  (cons first more))

(cl-defmacro unless2 (cond &rest body)
  `(if ,cond nil (progn ,@body)))

(cl-defmacro inc-by (place &optional (n 1))
  `(setq ,place (+ ,place ,n)))

(ert-deftest cl-defun-key-args ()
  "&key args take keyword values or their defaults."
  (should (equal (greet "Ada") "Hello, Ada"))
  (should (equal (greet "Ada" :greeting "Hi") "Hi, Ada"))
  (should (equal (greet "Ada" :loud t) "HELLO, ADA")))

(ert-deftest cl-defun-optional-and-rest ()
  "&optional uses its default when unsupplied; &rest collects the tail."
  (should (= (scale 5) 10))
  (should (= (scale 5 3) 15))
  (should (equal (collect 1 2 3) '(1 2 3)))
  (should (equal (collect 1) '(1))))

(ert-deftest cl-defmacro-expands ()
  "cl-defmacro defines a working macro with its own cl-lambda-list."
  (should (null (unless2 t (error "should not run"))))
  (should (equal (unless2 nil 'ran) 'ran))
  (let ((c 10))
    (should (= (inc-by c) 11))
    (should (= (inc-by c 5) 16))))

(ert-deftest cl-destructuring-and-values ()
  "cl-destructuring-bind honors &optional/&key defaults; values are lists."
  (should (equal (cl-destructuring-bind (a &optional (b 9)) '(1) (list a b)) '(1 9)))
  (should (equal (cl-destructuring-bind (a &key (b 9) c) '(1 :c 3) (list a b c)) '(1 9 3)))
  (should (= (cl-multiple-value-bind (a b) (list 1 2) (+ a b)) 3))
  (should (equal (cl-values 1 2 3) '(1 2 3))))

(ert-run-tests-batch-and-exit)
