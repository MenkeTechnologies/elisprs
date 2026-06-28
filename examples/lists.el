;;; lists.el --- cons cells & list ops as real fusevm heap objects, ERT-tested  -*- lexical-binding: nil; -*-

;; Cons cells are genuine heap objects with identity (Value::Obj handles into the
;; ElispHost arena), so structural ops and in-place mutation both work.
(message "== lists demo ==")

(ert-deftest lists-access ()
  "cons / car / cdr / length / nth / nthcdr / last."
  (should (equal (cons 1 2) (cons 1 2)))
  (should (= (car (list 1 2 3)) 1))
  (should (equal (cdr (list 1 2 3)) (list 2 3)))
  (should (= (length (list 1 2 3 4)) 4))
  (should (= (nth 2 (list 10 20 30 40)) 30))
  (should (equal (nthcdr 2 (list 1 2 3 4)) (list 3 4)))
  (should (equal (last (list 1 2 3)) (list 3))))

(ert-deftest lists-transform ()
  "reverse / append / member / memq / assoc."
  (should (equal (reverse (list 1 2 3)) (list 3 2 1)))
  (should (equal (append (list 1 2) (list 3 4)) (list 1 2 3 4)))
  (should (equal (member 2 (list 1 2 3)) (list 2 3)))
  (should-not (memq 9 (list 1 2 3)))
  (should (equal (assoc 'b (list (cons 'a 1) (cons 'b 2))) (cons 'b 2))))

(ert-deftest lists-derived ()
  "Prelude-defined helpers: delete-dups / number-sequence / mapconcat."
  (should (equal (delete-dups (list 1 1 2 3 3 1)) (list 1 2 3)))
  (should (equal (number-sequence 1 5) (list 1 2 3 4 5)))
  (should (equal (mapconcat 'number-to-string (list 1 2 3) "-") "1-2-3")))

(ert-deftest lists-mutation ()
  "In-place setcar/setcdr — impossible under an immutable value model."
  (let ((pair (cons 1 2)))
    (setcar pair 99)
    (setcdr pair 100)
    (should (equal pair (cons 99 100)))))

(ert-run-tests-batch-and-exit)
