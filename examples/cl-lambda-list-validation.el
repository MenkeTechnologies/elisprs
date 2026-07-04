;;; cl-lambda-list-validation.el --- &key keyword checking + cl-flet cl-lambda-lists  -*- lexical-binding: nil; -*-

;; Two behaviors pinned to emacs 30.2 (oracle-verified):
;;   1. cl-destructuring-bind / cl-defun reject a keyword not among the &key
;;      names, unless the lambda-list has &allow-other-keys or the plist carries
;;      :allow-other-keys t. Message matches cl-macs.el exactly.
;;   2. cl-flet / cl-labels accept full cl-lambda-lists (per-arg &optional
;;      defaults, &key, &rest) for their local functions, not just plain args.
(message "== cl lambda-list validation demo ==")

(cl-defun kfun (a &key x) (list a x))

(ert-deftest cl-key-unknown-keyword-signals ()
  "A keyword outside the &key set errors like emacs' cl-destructuring-bind."
  ;; emacs 30.2: (error \"Keyword argument :y not one of (:x)\")
  (should (equal (condition-case e (kfun 1 :x 2 :y 3) (error e))
                 '(error "Keyword argument :y not one of (:x)")))
  ;; Missing value for a known keyword.
  (should (equal (condition-case e
                     (cl-destructuring-bind (a &key x) '(1 :x) (list a x))
                     (error e))
                 '(error "Missing argument for :x")))
  ;; Two allowed keys are listed in the message in lambda-list order.
  (should (equal (condition-case e
                     (cl-destructuring-bind (a &key x y) '(1 :z 9) (list a x y))
                     (error e))
                 '(error "Keyword argument :z not one of (:x :y)"))))

(ert-deftest cl-key-escape-hatches ()
  "&allow-other-keys in the lambda-list or :allow-other-keys t suppresses it."
  (should (equal (cl-destructuring-bind (a &key x &allow-other-keys)
                     '(1 :x 2 :y 3) (list a x))
                 '(1 2)))
  (should (equal (cl-destructuring-bind (a &key x)
                     '(1 :x 2 :y 3 :allow-other-keys t) (list a x))
                 '(1 2)))
  ;; Valid keyword usage still works.
  (should (equal (kfun 1 :x 2) '(1 2)))
  (should (equal (kfun 1) '(1 nil))))

(ert-deftest cl-flet-cl-lambda-lists ()
  "cl-flet / cl-labels local functions accept cl-lambda-lists."
  (should (equal (cl-flet ((f (x &optional (y 10)) (list x y))) (f 1))
                 '(1 10)))
  (should (equal (cl-flet ((f (x &optional (y 10)) (list x y))) (f 1 2))
                 '(1 2)))
  (should (equal (cl-flet ((f (&key x (y 5)) (list x y))) (f :x 1))
                 '(1 5)))
  ;; cl-labels recursion with an &optional accumulator default.
  (should (= (cl-labels ((sumdown (n &optional (acc 0))
                           (if (= n 0) acc (sumdown (1- n) (+ acc n)))))
               (sumdown 4))
             10)))

(ert-run-tests-batch-and-exit)
