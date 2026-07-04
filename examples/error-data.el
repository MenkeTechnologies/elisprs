;;; error-data.el --- condition-case error DATA fidelity vs GNU Emacs  -*- lexical-binding: nil; -*-

;; The object a `condition-case` handler binds is `(ERROR-SYMBOL . DATA)`. The
;; human-readable text lives in the symbol's `error-message`, NOT in DATA. So:
;;   * nil-data conditions  -> `(SYMBOL)`            (arith-error, end-of-file, …)
;;   * `(error "msg")`      -> `(error "msg")`       (message IS the data)
;;   * value-carrying ones  -> `(SYMBOL VAL …)`      (wrong-type-argument, …)
;; Each `should` below is oracle-verified against `emacs -Q --batch` 30.2.
(message "== error-data demo ==")

(ert-deftest err-data-nil-data ()
  "Arithmetic / EOF / buffer-edge conditions carry an EMPTY data list."
  (should (equal (condition-case e (/ 1 0) (error e)) '(arith-error)))
  (should (equal (condition-case e (% 7 0) (error e)) '(arith-error)))
  (should (equal (condition-case e (mod 7 0) (error e)) '(arith-error)))
  (should (equal (condition-case e (read "") (error e)) '(end-of-file))))

(ert-deftest err-data-message-kept ()
  "Generic `error`/`user-error` keep the message string AS the data."
  (should (equal (condition-case e (error "boom") (error e)) '(error "boom")))
  (should (equal (condition-case e (signal 'error '("x")) (error e)) '(error "x")))
  (should (equal (condition-case e (user-error "u") (error e)) '(user-error "u"))))

(ert-deftest err-data-values-kept ()
  "Value-carrying conditions keep their structured DATA list."
  (should (equal (condition-case e (car 5) (error e))
                 '(wrong-type-argument listp 5)))
  (should (equal (condition-case e (aref [1 2] 5) (error e))
                 '(args-out-of-range [1 2] 5)))
  (should (equal (condition-case e (symbol-value 'zzz-none) (error e))
                 '(void-variable zzz-none)))
  (should (equal (condition-case e (zzz-none) (error e))
                 '(void-function zzz-none))))

(ert-deftest err-data-explicit-signal ()
  "`signal` with an explicit nil data list matches the raised-condition path."
  (should (equal (condition-case e (signal 'arith-error nil) (error e))
                 '(arith-error))))

(ert-run-tests-batch-and-exit)
