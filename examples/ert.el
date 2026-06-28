;;; ert.el --- ERT (Emacs Lisp Regression Testing) demo, self-checked  -*- lexical-binding: t; -*-
;; Run me:  elisp examples/ert.el
;;
;; elisprs ports a subset of ERT — `should` / `should-not` / `should-error`
;; assertions and `ert-deftest` to name tests. A failing assertion signals
;; `ert-test-failed`, which the batch runner catches and counts.
;; `ert-run-tests-batch-and-exit` errors out (→ non-zero exit) if any test
;; failed, so this file doubles as a CI regression gate.
(message "== ERT demo ==")

(ert-deftest my-math-test ()
  "Ensure basic addition works."
  (should (= (+ 1 2) 3))
  (should-error (/ 1 0) :type 'arith-error))

(ert-deftest ert-should-forms ()
  "should / should-not / should-error, with and without :type."
  (should (equal (list 1 2) '(1 2)))
  (should-not (eq 'a 'b))
  (should-error (car 5))                          ; any error is fine here
  (should-error (signal 'my-error nil) :type 'my-error))

(ert-deftest ert-uses-the-language ()
  "Tests are ordinary elisp — closures, recursion, higher-order all work."
  (let ((double (lambda (x) (* x 2))))
    (should (= (funcall double 21) 42)))
  (should (equal (mapcar #'1+ '(1 2 3)) '(2 3 4))))

(ert-deftest ert-skip-example ()
  "skip-unless short-circuits a test when a precondition is unmet."
  (skip-unless nil)            ; precondition false → this test is skipped
  (should nil))                ; never reached

(ert-deftest ert-known-limitation ()
  "An expected failure (XFAIL) documents a not-yet-implemented feature without
reddening the suite. elisprs has no buffer/editor object model yet, so buffer
primitives are unbound. Drop :expected-result when buffers land."
  :expected-result :failed
  (should (bufferp (current-buffer))))

;; Exits 0: the skip and the XFAIL are both *expected*, so neither counts.
(ert-run-tests-batch-and-exit)
