;;; demo.el --- elisprs on fusevm: a smoke test of the pipeline, ERT-tested  -*- lexical-binding: t; -*-
;; Run me:  elisp examples/demo.el
;;
;; Everything here is read by elisprs, lowered to a fusevm Chunk, and executed
;; on fusevm itself — there is no elisp interpreter. The ElispHost (reached from
;; fusevm's extension handler) supplies cons cells, symbols, and the subrs.
(message "== elisprs demo ==")
(message "sum = %d" (+ 1 2 3 4 5))
(message "list = %S" (list 1 2 3))
(message "vec[1] = %d" (aref (vector 10 20 30) 1))

(ert-deftest demo-arithmetic ()
  (should (= (+ 1 2 3 4 5) 15))
  (should (= (1+ 41) 42)))

(ert-deftest demo-cons-and-mutation ()
  "Real cons cells with identity — in-place setcar works."
  (let ((p (cons 1 2)))
    (should (equal p (cons 1 2)))
    (setcar p 99)
    (should (= (car p) 99))))

(ert-deftest demo-conditionals ()
  "elisp truthiness: only nil is false."
  (should (equal (if (eq 1 1) "yes" "no") "yes"))
  (should (eq (when (consp (cons 1 2)) 'ok) 'ok))
  (should-not (when nil 'ok)))

(ert-deftest demo-vectors ()
  (should (= (aref (vector 10 20 30) 1) 20)))

(ert-run-tests-batch-and-exit)
