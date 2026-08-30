;;; hash-tables.el --- make-hash-table, the :test kinds & iteration, ERT-tested  -*- lexical-binding: t; -*-

;; A hash table's `:test' decides which keys collide, and it is the only thing
;; that separates the three default tables from one another.
(message "== hash-tables demo ==")

(defun ht-of (test &rest pairs)
  "A table built with TEST, filled from PAIRS as key/value in order."
  (let ((h (make-hash-table :test test)))
    (while pairs
      (puthash (pop pairs) (pop pairs) h))
    h))

(ert-deftest ht-basics ()
  "put / get / count / remove, and the default a missing key returns."
  (let ((h (ht-of 'equal "a" 1 "b" 2)))
    (should (= (hash-table-count h) 2))
    (should (= (gethash "a" h) 1))
    ;; A missing key is nil unless a default is given.
    (should-not (gethash "zz" h))
    (should (= (gethash "zz" h 99) 99))
    ;; Re-putting an existing key replaces rather than adds.
    (puthash "a" 10 h)
    (should (= (hash-table-count h) 2))
    (should (= (gethash "a" h) 10))
    (remhash "a" h)
    (should (= (hash-table-count h) 1))
    (should-not (gethash "a" h))
    ;; Removing a key that is not there is not an error.
    (remhash "gone" h)
    (should (= (hash-table-count h) 1))
    (clrhash h)
    (should (= (hash-table-count h) 0))))

(ert-deftest ht-test-kinds ()
  "`eq' compares identity, `eql' also numbers, `equal' also structure."
  ;; Two equal strings are not `eq', so an `eq' table keeps them apart while an
  ;; `equal' table treats them as one key.
  (let ((heq (ht-of 'eq))
        (heqv (ht-of 'equal))
        (k1 (copy-sequence "k"))
        (k2 (copy-sequence "k")))
    (puthash k1 :first heq)
    (puthash k2 :second heq)
    (should (= (hash-table-count heq) 2))
    (puthash k1 :first heqv)
    (puthash k2 :second heqv)
    (should (= (hash-table-count heqv) 1))
    (should (eq (gethash "k" heqv) :second)))
  ;; A list key needs `equal'; `eql' sees two fresh lists as distinct.
  (let ((hl (ht-of 'equal)))
    (puthash (list 1 2) :v hl)
    (should (eq (gethash (list 1 2) hl) :v)))
  (let ((hn (ht-of 'eql)))
    (puthash (list 1 2) :v hn)
    (should-not (gethash (list 1 2) hn)))
  ;; `eql' distinguishes a float from the integer of the same value; `equal'
  ;; agrees with it, so neither collapses 1 and 1.0.
  (let ((hf (ht-of 'eql)))
    (puthash 1 :int hf)
    (puthash 1.0 :float hf)
    (should (= (hash-table-count hf) 2))
    (should (eq (gethash 1 hf) :int))
    (should (eq (gethash 1.0 hf) :float))))

(ert-deftest ht-iteration ()
  "`maphash' visits every live entry exactly once."
  (let ((h (ht-of 'equal "a" 1 "b" 2 "c" 3))
        (seen nil)
        (total 0))
    (maphash (lambda (k v) (push k seen) (setq total (+ total v))) h)
    (should (= total 6))
    (should (= (length seen) 3))
    ;; Order is not part of the contract, so compare as a set.
    (should (equal (sort seen #'string<) (list "a" "b" "c"))))
  ;; An empty table calls the function not at all.
  (let ((n 0))
    (maphash (lambda (_k _v) (setq n (1+ n))) (ht-of 'equal))
    (should (= n 0))))

(ert-deftest ht-predicates ()
  "The table is its own type, and reports the test it was built with."
  (let ((h (ht-of 'equal "a" 1)))
    (should (hash-table-p h))
    (should-not (hash-table-p (list 1 2)))
    (should (eq (hash-table-test h) 'equal))
    ;; A copy is independent of the original.
    (let ((c (copy-hash-table h)))
      (puthash "b" 2 c)
      (should (= (hash-table-count h) 1))
      (should (= (hash-table-count c) 2)))))

(ert-run-tests-batch-and-exit)
