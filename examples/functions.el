;;; functions.el --- defun, recursion, lambda/funcall & macros on fusevm, ERT-tested  -*- lexical-binding: nil; -*-

;; A user closure carries a precompiled fusevm::Chunk; calling it runs that chunk
;; on a nested fusevm VM. defmacro expands at compile time before lowering.
(message "== functions demo ==")

;; Definitions used by the tests (must precede the tests that reference them).
(defun square (x) (* x x))
(defun sum-of-squares (a b) (+ (square a) (square b)))
(defun fact (n) (if (< n 2) 1 (* n (fact (1- n)))))
(defun fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(defmacro my-incr (place) (list 'setq place (list '1+ place)))

(ert-deftest fn-defun-and-nesting ()
  (should (= (square 6) 36))
  (should (= (sum-of-squares 3 4) 25)))

(ert-deftest fn-recursion ()
  (should (= (fact 5) 120))
  (should (= (fib 10) 55)))

(ert-deftest fn-lambda ()
  "Anonymous functions are first-class values."
  (should (= (funcall (lambda (a b) (+ a b)) 3 4) 7))
  (let ((adder (lambda (x) (+ x 100))))
    (should (= (funcall adder 5) 105))))

(ert-deftest fn-let-and-macro ()
  (should (= (let ((a 2) (b 3)) (* a b)) 6))
  ;; my-incr expands to (setq counter (1+ counter)) before lowering.
  (let ((counter 41))
    (my-incr counter)
    (should (= counter 42))))

(ert-deftest fn-sequencing ()
  (should (= (prog1 1 2 3) 1))
  (should (= (progn 1 2 3) 3)))

(ert-run-tests-batch-and-exit)
