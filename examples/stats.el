;;; stats.el --- numeric/stats over data, ERT-tested  -*- lexical-binding: t; -*-

;; Exercises the numeric surface: integer/float arithmetic, sort, cl-loop math
;; accumulators, transcendental fns, float printing, and format precision.
;; Passes under GNU Emacs.
(require 'cl-lib)
(require 'seq)

(defun mean (xs) (/ (float (seq-reduce #'+ xs 0)) (length xs)))

(defun median (xs)
  (let* ((s (sort (copy-sequence xs) #'<))
         (n (length s)))
    (if (cl-oddp n)
        (float (nth (/ n 2) s))
      (/ (+ (nth (1- (/ n 2)) s) (nth (/ n 2) s)) 2.0))))

(defun variance (xs)
  (let ((m (mean xs)))
    (/ (seq-reduce (lambda (acc x) (+ acc (* (- x m) (- x m)))) xs 0.0)
       (length xs))))

(ert-deftest stats-basic ()
  (let ((data '(2 4 4 4 5 5 7 9)))
    (should (= (mean data) 5.0))
    (should (= (median data) 4.5))
    (should (= (variance data) 4.0))
    (should (= (sqrt (variance data)) 2.0))
    (should (= (seq-max data) 9))
    (should (= (seq-min data) 2))))

(ert-deftest stats-cl-loop-accumulators ()
  (let ((xs (number-sequence 1 10)))
    (should (= (cl-loop for x in xs sum x) 55))
    (should (= (cl-loop for x in xs maximize x) 10))
    (should (= (cl-loop for x in xs count (cl-evenp x)) 5))
    (should (equal (cl-loop for x in xs when (> x 7) collect x) '(8 9 10)))
    (should (= (cl-loop for x from 1 to 5 for y = (* x x) sum y) 55))))

(ert-deftest stats-float-printing ()
  (should (equal (number-to-string (/ 1.0 4)) "0.25"))
  (should (equal (number-to-string (* 2.0 3)) "6.0"))
  (should (equal (format "%.3f" float-pi) "3.142"))
  (should (equal (format "%g" 0.0001) "0.0001"))
  (should (equal (format "%e" 12345.0) "1.234500e+04"))
  (should (equal (format "%d apples, %.1f%%" 3 95.5) "3 apples, 95.5%")))

(ert-deftest stats-integer-math ()
  (should (= (expt 2 10) 1024))
  (should (= (cl-gcd 48 36) 12))
  (should (= (cl-lcm 4 6) 12))
  (should (equal (cl-floor 17 5) '(3 2)))
  (should (= (mod -7 3) 2))
  (should (= (abs -5) 5))
  (should (= (truncate 3.99) 3)))

(ert-run-tests-batch-and-exit)
