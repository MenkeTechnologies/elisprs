;;; pcase.el --- structural dispatch with pcase, ERT-tested  -*- lexical-binding: nil; -*-

;; pcase compiles each clause's pattern to tests + bindings at macroexpansion
;; time (see the prelude). This is the non-backquote subset: wildcards, literals,
;; binders, pred, guard, and/or. Backquote patterns wait on lazy backquote.
(message "== pcase demo ==")

(defun classify (x)
  (pcase x
    ((pred stringp) 'string)
    ((and n (guard (and (integerp n) (< n 0)))) 'negative)
    (0 'zero)
    ((pred integerp) 'positive-int)
    ((or 'yes 'no) 'bool-symbol)
    (_ 'unknown)))

(ert-deftest pcase-classify ()
  "pred / guard / literal / or / wildcard clauses dispatch in order."
  (should (eq (classify "hi") 'string))
  (should (eq (classify -4) 'negative))
  (should (eq (classify 0) 'zero))
  (should (eq (classify 7) 'positive-int))
  (should (eq (classify 'yes) 'bool-symbol))
  (should (eq (classify 'no) 'bool-symbol))
  (should (eq (classify 3.5) 'unknown)))

(ert-deftest pcase-binds-and-returns-nil ()
  "A bare symbol binds the value; no matching clause yields nil."
  (should (equal (pcase (list 1 2) (v (length v))) 2))
  (should (null (pcase 5 (1 'a) (2 'b)))))

(ert-run-tests-batch-and-exit)
