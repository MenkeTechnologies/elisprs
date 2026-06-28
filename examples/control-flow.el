;;; control-flow.el --- conditionals, loops & boolean logic on fusevm, ERT-tested  -*- lexical-binding: nil; -*-

;; elisp truthiness (only nil is false) and the looping macros all lower to
;; fusevm branch/jump ops.
(message "== control-flow demo ==")

(defun classify (n)
  (cond ((< n 0) 'neg)
        ((= n 0) 'zero)
        (t 'pos)))

(ert-deftest cf-conditionals ()
  "if / when / unless."
  (should (equal (if (eq 1 1) "yes" "no") "yes"))
  (should (equal (if (eq 1 2) "yes" "no") "no"))
  (should (eq (when (eq 1 1) 'fired) 'fired))
  (should-not (when nil 'fired))
  (should (eq (unless nil 'fired) 'fired)))

(ert-deftest cf-cond ()
  (should (eq (classify -3) 'neg))
  (should (eq (classify 0) 'zero))
  (should (eq (classify 7) 'pos)))

(ert-deftest cf-logic ()
  "and / or short-circuit."
  (should (= (and 1 2 3) 3))
  (should-not (and 1 nil 3))
  (should (= (or nil nil 5) 5))
  (should-not (or nil nil)))

(ert-deftest cf-loops ()
  "while / dotimes / dolist."
  (let ((i 0) (total 0))
    (while (< i 5) (setq total (+ total i)) (setq i (1+ i)))
    (should (= total 10)))
  (let ((sum 0))
    (dotimes (k 4) (setq sum (+ sum k)))
    (should (= sum 6)))
  (let ((collected nil))
    (dolist (x (list 1 2 3)) (setq collected (cons (* x x) collected)))
    (should (equal collected (list 9 4 1)))))

(ert-run-tests-batch-and-exit)
