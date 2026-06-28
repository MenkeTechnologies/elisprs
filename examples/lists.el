;;; lists.el --- cons cells & list ops as real fusevm heap objects  -*- lexical-binding: nil; -*-

;; Cons cells are genuine heap objects with identity (Value::Obj handles into the
;; ElispHost arena), so structural ops and in-place mutation both work. Each
;; `expect` errors out (non-zero exit) on a mismatch, so CI catches regressions.

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

(expect "cons"       (cons 1 2) (cons 1 2))
(expect "car"        (car (list 1 2 3)) 1)
(expect "cdr"        (cdr (list 1 2 3)) (list 2 3))
(expect "list"       (list 1 2 3) (list 1 2 3))
(expect "length"     (length (list 1 2 3 4)) 4)
(expect "nth"        (nth 2 (list 10 20 30 40)) 30)
(expect "nthcdr"     (nthcdr 2 (list 1 2 3 4)) (list 3 4))
(expect "last"       (last (list 1 2 3)) (list 3))
(expect "reverse"    (reverse (list 1 2 3)) (list 3 2 1))
(expect "append"     (append (list 1 2) (list 3 4)) (list 1 2 3 4))
(expect "member"     (member 2 (list 1 2 3)) (list 2 3))
(expect "memq-miss"  (memq 9 (list 1 2 3)) nil)
(expect "assoc"      (assoc 'b (list (cons 'a 1) (cons 'b 2))) (cons 'b 2))
(expect "delete-dups" (delete-dups (list 1 1 2 3 3 1)) (list 1 2 3))
(expect "number-seq" (number-sequence 1 5) (list 1 2 3 4 5))
(expect "mapconcat"  (mapconcat 'number-to-string (list 1 2 3) "-") "1-2-3")

;; In-place mutation — impossible under an immutable value model.
(setq pair (cons 1 2))
(setcar pair 99)
(setcdr pair 100)
(expect "setcar/setcdr" pair (cons 99 100))

(message "lists: all checks passed on fusevm")
