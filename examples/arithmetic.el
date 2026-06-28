;;; arithmetic.el --- integer math lowered to fusevm ops, self-checked  -*- lexical-binding: nil; -*-

;; Every form here is read by elisprs, lowered to a fusevm Chunk, and run on
;; fusevm itself. The file is a self-test: `expect` raises an elisp `error` (→
;; non-zero exit) the moment a result drifts, so CI's example stage fails loudly.

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

(expect "sum"      (+ 1 2 3 4 5) 15)
(expect "product"  (* 2 3 4) 24)
(expect "subtract" (- 10 3 2) 5)
(expect "divide"   (/ 20 4) 5)
(expect "modulo"   (% 17 5) 2)
(expect "nested"   (+ 1 (* 2 (- 5 2))) 7)
(expect "inc"      (1+ 41) 42)
(expect "dec"      (1- 1) 0)
(expect "abs"      (abs -7) 7)
(expect "max"      (max 3 9 2) 9)
(expect "min"      (min 3 9 2) 2)
(expect "mod-fn"   (mod -1 5) 4)

(message "arithmetic: all checks passed on fusevm")
