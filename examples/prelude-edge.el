;;; prelude-edge.el --- edge-case fidelity vs GNU Emacs 30.2, ERT-tested  -*- lexical-binding: nil; -*-

;; Each form here was cross-checked against `emacs -Q --batch` (Emacs 30.2) so
;; the prelude matches Emacs exactly on the awkward corners: cyclic-list length
;; counting, non-integer sequence indices, and destructuring arity errors.
(message "== prelude edge-case fidelity ==")

(ert-deftest safe-length-circular-and-dotted ()
  "safe-length counts cons cells via Brent's tortoise/hare like Emacs 30.2:
a proper list is its length, a dotted tail stops the walk, and a circular
list returns an integer >= the number of distinct cells (a 3-cycle => 5)."
  (should (= (safe-length '(1 2 3)) 3))
  (should (= (safe-length nil) 0))
  (should (= (safe-length '(1 2 . 3)) 2))
  (should (= (let ((l (list 1))) (setcdr l l) (safe-length l)) 1))
  (should (= (let ((l (list 1 2))) (setcdr (cdr l) l) (safe-length l)) 4))
  (should (= (let ((l (list 1 2 3))) (setcdr (cddr l) l) (safe-length l)) 5))
  (should (= (let ((l (make-list 100 0))) (setcdr (nthcdr 99 l) l)
              (safe-length l))
             226)))

(ert-deftest float-index-signals-like-emacs ()
  "A float index is rejected: nthcdr/nth signal integerp, and elt on an array
or string signals fixnump (aref's contract), while elt on a list signals
integerp (nth's contract) -- exactly as Emacs 30.2."
  (should-error (nthcdr 1.5 '(a b c)) :type 'wrong-type-argument)
  (should-error (nthcdr 2.0 '(a b c)) :type 'wrong-type-argument)
  (should (equal (nthcdr 2 '(a b c)) '(c)))
  (should-error (elt '(a b c) 1.5) :type 'wrong-type-argument)
  (should-error (elt [1 2 3] 1.5) :type 'wrong-type-argument)
  (should-error (elt "abc" 1.5) :type 'wrong-type-argument)
  (should (eq (elt '(a b c) 1) 'b))
  (should (= (elt [1 2 3] 1) 2))
  ;; elt on an array reports fixnump; on a list it reports integerp.
  (should (eq (car (condition-case e (elt [1 2 3] 1.5) (error e))) 'wrong-type-argument))
  (should (eq (cadr (condition-case e (elt [1 2 3] 1.5) (error e))) 'fixnump))
  (should (eq (cadr (condition-case e (elt '(a b) 1.5) (error e))) 'integerp)))

(ert-deftest cl-destructuring-bind-arity ()
  "cl-destructuring-bind signals wrong-number-of-arguments on a length
mismatch, reporting the arglist and the actual count; &optional widens the
upper bound, &rest/&key lift it, &aux consumes nothing, and a nested pattern
mismatch is reported against the top-level arglist -- matching Emacs 30.2.
cl-loop's own destructuring stays lenient."
  (should (equal (cl-destructuring-bind (a b) '(1 2) (list a b)) '(1 2)))
  (should (equal (cl-destructuring-bind (a &optional b) '(1) (list a b)) '(1 nil)))
  (should (equal (cl-destructuring-bind (a &rest r) '(1 2 3) (list a r)) '(1 (2 3))))
  (should-error (cl-destructuring-bind (a b) '(1 2 3) t)
                :type 'wrong-number-of-arguments)
  (should-error (cl-destructuring-bind (a b) '(1) t)
                :type 'wrong-number-of-arguments)
  (should-error (cl-destructuring-bind (a &optional b) '(1 2 3) t)
                :type 'wrong-number-of-arguments)
  (should-error (cl-destructuring-bind (a &aux (z 9)) '(1 2) t)
                :type 'wrong-number-of-arguments)
  ;; Signal data: (wrong-number-of-arguments ARGLIST COUNT).
  (should (equal (cdr (condition-case e (cl-destructuring-bind (a b) '(1 2 3) t)
                        (error e)))
                 '((a b) 3)))
  ;; Nested mismatch reports the OUTER arglist with the inner count.
  (should (equal (cdr (condition-case e
                          (cl-destructuring-bind (a (b c)) '(1 (2 3 4)) t)
                        (error e)))
                 '((a (b c)) 3)))
  ;; cl-loop destructuring must NOT enforce arity.
  (should (equal (cl-loop for (a b) in '((1 2) (3 4 5)) collect a) '(1 3))))

(ert-run-tests-batch-and-exit)
