;;; cl-loop.el --- the cl-loop iteration macro (common subset), ERT-tested  -*- lexical-binding: nil; -*-

;; cl-loop expands to a `(let* … (catch … (while …)))` driver in the prelude.
;; Supported: numeric `for`, `for … in/on`, `repeat`, `while`/`until`, the
;; `collect`/`append`/`nconc`/`sum`/`count`/`maximize`/`minimize` accumulators,
;; `do`, and `finally [return]`.
(message "== cl-loop demo ==")

(ert-deftest cl-loop-numeric ()
  (should (equal (cl-loop for i from 1 to 5 collect i) (list 1 2 3 4 5)))
  (should (equal (cl-loop for i from 0 below 5 collect i) (list 0 1 2 3 4)))
  (should (equal (cl-loop for i from 10 downto 7 collect i) (list 10 9 8 7)))
  (should (equal (cl-loop for i from 1 to 10 by 2 collect i) (list 1 3 5 7 9))))

(ert-deftest cl-loop-lists ()
  (should (equal (cl-loop for x in (list 1 2 3) collect (* x x)) (list 1 4 9)))
  (should (equal (cl-loop for c on (list 1 2 3) collect (car c)) (list 1 2 3)))
  (should (equal (cl-loop for x in (list (list 1) (list 2 3)) append x) (list 1 2 3))))

(ert-deftest cl-loop-accumulate ()
  (should (= (cl-loop for x in (list 1 2 3 4) sum x) 10))
  (should (= (cl-loop for x in (list 1 2 3 4) count (cl-evenp x)) 2))
  (should (= (cl-loop for i from 1 to 4 maximize (* i i)) 16))
  (should (= (cl-loop for i from 3 to 9 minimize i) 3)))

(ert-deftest cl-loop-control ()
  (should (equal (cl-loop repeat 3 collect 9) (list 9 9 9)))
  (should (equal (cl-loop for i from 1 to 100 until (> i 3) collect i) (list 1 2 3)))
  (should (= (let ((s 0)) (cl-loop for i from 1 to 5 do (setq s (+ s i))) s) 15))
  (should (= (cl-loop for i from 1 to 5 do (ignore i) finally return 42) 42)))

(ert-deftest cl-loop-conditionals ()
  (should (equal (cl-loop for x in (list 1 2 3 4 5) when (cl-evenp x) collect x) (list 2 4)))
  (should (equal (cl-loop for x in (list 1 2 3) if (cl-oddp x) collect x else collect (- x))
                 (list 1 -2 3)))
  (should (= (cl-loop for x in (list 1 2 3 4 5) when (> x 2) sum x) 12))
  (should (cl-loop for x in (list 2 4 6) always (cl-evenp x)))
  (should (cl-loop for x in (list 1 3 5) never (cl-evenp x)))
  (should (= (cl-loop for x in (list 1 2 3) thereis (and (cl-evenp x) x)) 2)))

(ert-deftest cl-loop-with-into ()
  (should (= (cl-loop with total = 0 for x in (list 1 2 3) do (setq total (+ total x))
                      finally return total)
             6))
  (should (equal (cl-loop for x in (list 1 2 3) collect x into ys finally return (cons 'done ys))
                 (list 'done 1 2 3))))

(ert-run-tests-batch-and-exit)
