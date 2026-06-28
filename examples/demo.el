;;; demo.el --- elisprs on fusevm: a smoke test of the current pipe

;; Everything here is read by elisprs, lowered to a fusevm Chunk, and executed
;; on fusevm itself — there is no elisp interpreter. The ElispHost (reached from
;; fusevm's extension handler) supplies cons cells, symbols, and the subrs.

;; Arithmetic on the fusevm value stack.
(message "sum = %d" (+ 1 2 3 4 5))

;; Real cons cells (heap objects with identity), including dotted pairs.
(message "pair = %S" (cons 1 2))
(message "list = %S" (list 1 2 3))

;; In-place mutation — impossible under the old rust_lisp list model.
(setq p (cons 1 2))
(setcar p 99)
(message "after setcar = %S" p)

;; Conditionals (elisp truthiness, only nil is false).
(message "branch = %s" (if (eq 1 1) "yes" "no"))
(message "guard = %S" (when (consp p) (quote ok)))

;; Vectors.
(message "vec[1] = %d" (aref (vector 10 20 30) 1))

;; Dynamic variables.
(setq counter 41)
(message "counter+1 = %d" (1+ counter))

(message "done — ran on fusevm.")
