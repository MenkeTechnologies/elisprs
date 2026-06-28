;;; demo.el --- a milestone-1 elisprs smoke test  -*- lexical-binding: nil -*-

;; Recursion + arithmetic.
(defun fact (n)
  (if (<= n 1) 1 (* n (fact (1- n)))))

(message "6! = %d" (fact 6))

;; Higher-order functions.
(message "squares: %S" (mapcar (lambda (x) (* x x)) '(1 2 3 4 5)))

;; Iterative accumulation with dynamic let + while.
(defun sum-to (n)
  (let ((acc 0) (i 1))
    (while (<= i n)
      (setq acc (+ acc i))
      (setq i (1+ i)))
    acc))

(message "sum 1..10 = %d" (sum-to 10))

;; Macros.
(defmacro my-unless (cond &rest body)
  (list 'if cond nil (cons 'progn body)))

(message "my-unless: %S" (my-unless nil 'reached))

;; Error handling.
(message "caught: %S"
         (condition-case e (/ 1 0)
           (arith-error (format "%s" e))))

(message "done.")
