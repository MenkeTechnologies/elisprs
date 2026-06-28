;;; arithmetic.el --- integer math lowered to fusevm ops, ERT-tested  -*- lexical-binding: nil; -*-

;; Every form here is read by elisprs, lowered to a fusevm Chunk, and run on
;; fusevm itself. The ERT suite is the regression gate:
;; `ert-run-tests-batch-and-exit` errors (→ non-zero exit) if any test fails.
(message "== arithmetic demo ==")

(ert-deftest arith-basic ()
  "Sum, product, difference, quotient, remainder."
  (should (= (+ 1 2 3 4 5) 15))
  (should (= (* 2 3 4) 24))
  (should (= (- 10 3 2) 5))
  (should (= (/ 20 4) 5))
  (should (= (% 17 5) 2))
  (should (= (+ 1 (* 2 (- 5 2))) 7)))

(ert-deftest arith-unary-and-helpers ()
  "Increment/decrement, abs, max/min, floored mod."
  (should (= (1+ 41) 42))
  (should (= (1- 1) 0))
  (should (= (abs -7) 7))
  (should (= (max 3 9 2) 9))
  (should (= (min 3 9 2) 2))
  (should (= (mod -1 5) 4)))

(ert-deftest arith-errors ()
  "Division by zero signals arith-error."
  (should-error (/ 1 0) :type 'arith-error)
  (should-error (% 1 0) :type 'arith-error))

(ert-run-tests-batch-and-exit)
