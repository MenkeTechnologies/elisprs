;;; nonlocal-exit.el --- catch/throw, unwind-protect & condition-case, ERT-tested  -*- lexical-binding: t; -*-

;; The three ways control leaves a form early, and what each guarantees about
;; the code it jumps over. `catch`/`throw` is the plain non-local exit,
;; `unwind-protect` is the one that runs its cleanup either way, and
;; `condition-case` is the one that inspects what went wrong.
(message "== nonlocal-exit demo ==")

(ert-deftest nx-catch-throw ()
  "`throw' unwinds to the matching `catch' and carries a value."
  (should (= (catch 'tag (throw 'tag 42)) 42))
  ;; A `catch' whose body finishes normally yields the body's value.
  (should (eq (catch 'tag :fell-through) :fell-through))
  ;; The throw leaves the loop from inside, skipping the rest of it.
  (should (= (catch 'found
               (dolist (x (list 1 2 3 4))
                 (when (= x 3) (throw 'found (* x 10))))
               -1)
             30))
  ;; Tags are compared with `eq', so only the matching one catches: the inner
  ;; `catch' lets the outer tag pass straight through it.
  (should (eq (catch 'outer (catch 'inner (throw 'outer :out)) :not-here) :out))
  ;; Nested catches of the SAME tag stop at the innermost.
  (should (eq (catch 'same (list (catch 'same (throw 'same :inner))) :after) :after)))

(ert-deftest nx-unwind-protect ()
  "The cleanup forms run whether the body finishes or unwinds."
  (let (log)
    (should (= (unwind-protect 1 (push :normal log)) 1))
    (should (equal log (list :normal))))
  ;; ... on a thrown exit, and the throw still reaches its catch.
  (let (log)
    (should (eq (catch 'tag (unwind-protect (throw 'tag :thrown) (push :cleanup log)))
                :thrown))
    (should (equal log (list :cleanup))))
  ;; ... and on an error, which keeps propagating afterwards.
  (let (log)
    (should-error (unwind-protect (error "boom") (push :on-error log)))
    (should (equal log (list :on-error))))
  ;; Cleanups of nested forms run innermost first.
  (let (log)
    (ignore-errors
      (unwind-protect (unwind-protect (error "boom") (push :inner log))
        (push :outer log)))
    (should (equal log (list :outer :inner)))))

(ert-deftest nx-condition-case ()
  "`condition-case' selects a handler by error symbol, and binds the error."
  (should (equal (condition-case err (car 1)
                   (wrong-type-argument (list :wta (cadr err)))
                   (error :other))
                 (list :wta 'listp)))
  ;; A more general handler catches what a specific one would have.
  (should (eq (condition-case nil (car 1) (error :caught)) :caught))
  ;; The body's value is used when nothing signals.
  (should (= (condition-case nil 7 (error :caught)) 7))
  ;; `:success' runs with the body's value bound.
  (should (= (condition-case v (+ 1 2) (:success (* v 10)) (error -1)) 30))
  ;; A signal the handler list does not name is not caught here.
  (should (eq (catch 'escaped
                (condition-case nil
                    (catch 'inner (throw 'escaped :past-handlers))
                  (error :wrongly-caught)))
              :past-handlers))
  ;; `signal' with user data, read back out of the binding.
  (should (equal (condition-case e (signal 'arith-error (list :x 1))
                   (arith-error (cdr e)))
                 (list :x 1))))

(ert-deftest nx-error-data ()
  "`error' builds an `error' condition whose data is the formatted string."
  (should (equal (condition-case e (error "bad %d" 7) (error (cdr e)))
                 (list "bad 7")))
  (should (eq (condition-case e (error "x") (error (car e))) 'error))
  ;; `ignore-errors' yields nil for a signalled error and the value otherwise.
  (should-not (ignore-errors (error "x")))
  (should (= (ignore-errors 5) 5)))

(ert-run-tests-batch-and-exit)
