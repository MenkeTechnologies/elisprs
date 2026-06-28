;;; functions.el --- defun, recursion, lambda/funcall & macros on fusevm  -*- lexical-binding: nil; -*-

;; A user closure carries a precompiled fusevm::Chunk; calling it runs that chunk
;; on a nested fusevm VM. defmacro expands at compile time before lowering. Each
;; `expect` errors out (non-zero exit) on a mismatch so CI flags any regression.

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

;; Plain defun + nested calls.
(defun square (x) (* x x))
(defun sum-of-squares (a b) (+ (square a) (square b)))
(expect "defun"       (square 6) 36)
(expect "nested-call" (sum-of-squares 3 4) 25)

;; Recursion.
(defun fact (n) (if (< n 2) 1 (* n (fact (1- n)))))
(expect "factorial" (fact 5) 120)

(defun fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(expect "fibonacci" (fib 10) 55)

;; lambda + funcall (anonymous functions are first-class values).
(expect "lambda" (funcall (lambda (a b) (+ a b)) 3 4) 7)

(setq adder (lambda (x) (+ x 100)))
(expect "lambda-var" (funcall adder 5) 105)

;; let-bound locals.
(expect "let" (let ((a 2) (b 3)) (* a b)) 6)

;; defmacro: expands to (setq v (1+ v)) before lowering.
(defmacro my-incr (place) (list 'setq place (list '1+ place)))
(setq counter 41)
(my-incr counter)
(expect "defmacro" counter 42)

;; prog1 / progn evaluation order.
(expect "prog1" (prog1 1 2 3) 1)
(expect "progn" (progn 1 2 3) 3)

(message "functions: all checks passed on fusevm")
