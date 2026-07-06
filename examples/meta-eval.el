;;; meta-eval.el --- macros, eval, symbols, recursion, apply  -*- lexical-binding: t; -*-

;; Exercises the metaprogramming surface: defmacro/macroexpand, eval/read,
;; gensym hygiene, apply/funcall, recursion (named-let, cl-labels), and
;; backquote. Passes under GNU Emacs.
(require 'cl-lib)
(require 'subr-x)

(defmacro my-when (cond &rest body)
  `(if ,cond (progn ,@body)))

(defmacro swap! (a b)
  (let ((tmp (gensym)))
    `(let ((,tmp ,a)) (setq ,a ,b) (setq ,b ,tmp))))

(defmacro accumulate (var init &rest body)
  "Bind VAR to an empty list; BODY pushes to it; return it reversed."
  `(let ((,var nil)) ,@body (nreverse ,var)))

(ert-deftest meta-macros ()
  (should (equal (my-when t 1 2 3) 3))
  (should (null (my-when nil (error "no"))))
  (let ((x 1) (y 2)) (swap! x y) (should (equal (list x y) '(2 1))))
  (should (equal (accumulate acc nil
                   (dotimes (i 3) (push (* i i) acc)))
                 '(0 1 4))))

(ert-deftest meta-eval-read ()
  (should (= (eval '(+ 1 2 3) t) 6))
  (should (equal (read "(a (b c) . d)") '(a (b c) . d)))
  (should (= (eval (read "(* 6 7)") t) 42))
  ;; Self-evaluating constant symbols: `t' is its own value (Emacs `eval_sub'
  ;; returns the symbol's value slot, which for t/nil/keywords holds itself).
  (should (eq (eval t) t))
  (should (eq (eval 't) t))
  (should (eq (eval nil) nil))
  (should (eq (eval :kw) :kw))
  (should (equal (macroexpand '(my-when c x)) '(if c (progn x)))))

(ert-deftest meta-apply-funcall ()
  (should (= (apply #'+ 1 2 '(3 4)) 10))
  (should (= (funcall (lambda (&rest xs) (apply #'+ xs)) 1 2 3) 6))
  (should (equal (mapcar (apply-partially #'* 2) '(1 2 3)) '(2 4 6)))
  (should (= (funcall (cl-constantly 9) 'ignored) 9)))

(ert-deftest meta-recursion ()
  (cl-labels ((fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))))
    (should (= (fib 10) 55)))
  (should (= (named-let loop ((n 5) (acc 1))
               (if (= n 0) acc (loop (1- n) (* acc n))))
             120)))

(ert-deftest meta-symbols ()
  (should (eq (intern "foo") 'foo))
  (should (equal (symbol-name 'bar) "bar"))
  (should (not (eq (make-symbol "g") (make-symbol "g"))))
  (put 'my-sym 'color 'blue)
  (should (eq (get 'my-sym 'color) 'blue)))

(ert-run-tests-batch-and-exit)
