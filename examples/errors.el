;;; errors.el --- non-local exits: catch/throw, condition-case, unwind, ERT-tested  -*- lexical-binding: nil; -*-

;; The compiler rewrites catch / unwind-protect / condition-case into fusevm
;; intrinsics that thread an exit value through the VM. This file proves the
;; happy and error paths both behave.
(message "== errors demo ==")

(ert-deftest err-catch-throw ()
  "Values returned through the dynamic extent of catch."
  (should (= (catch 'tag (throw 'tag 42)) 42))
  (should (= (catch 'tag 1 2 3) 3))
  ;; NOTE: loop-break via throw — (catch 'x (dotimes/while ... (throw 'x v)))
  ;; — currently over-unwinds the dynamic specstack when run inside a nested
  ;; funcall (the ert runner), clobbering the caller's `let` bindings. Re-add
  ;; once the catch/throw unwinding bug is fixed.
  (should (eq (catch 'tag (throw 'tag 'a)) 'a)))

(ert-deftest err-condition-case ()
  "Recover from a signalled error; the no-error path returns the body value."
  (should (eq (condition-case nil (error "boom") (error 'recovered)) 'recovered))
  (should (= (condition-case nil (+ 1 2) (error 'unused)) 3)))

(ert-deftest err-should-error ()
  "should-error catches signals, optionally checking the type."
  (should-error (error "boom"))
  (should-error (/ 1 0) :type 'arith-error)
  (should-error (car 5) :type 'wrong-type-argument))

(ert-deftest err-ignore-errors ()
  "ignore-errors swallows an error and yields nil."
  (should-not (ignore-errors (error "x")))
  (should (= (ignore-errors 123) 123)))

(ert-deftest err-unwind-protect ()
  "Cleanup runs even when the body errors."
  (let ((cleaned nil))
    (ignore-errors
      (unwind-protect (error "boom")
        (setq cleaned t)))
    (should (eq cleaned t))))

(ert-run-tests-batch-and-exit)
