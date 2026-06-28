;;; control-flow.el --- conditionals, loops & boolean logic on fusevm  -*- lexical-binding: nil; -*-

;; elisp truthiness (only nil is false) and the looping macros all lower to
;; fusevm branch/jump ops. A failed `expect` raises an error → non-zero exit.

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

;; if / when / unless
(expect "if-then"  (if (eq 1 1) "yes" "no") "yes")
(expect "if-else"  (if (eq 1 2) "yes" "no") "no")
(expect "when"     (when (eq 1 1) 'fired) 'fired)
(expect "when-nil" (when nil 'fired) nil)
(expect "unless"   (unless nil 'fired) 'fired)

;; cond
(defun classify (n)
  (cond ((< n 0) 'neg)
        ((= n 0) 'zero)
        (t 'pos)))
(expect "cond-neg"  (classify -3) 'neg)
(expect "cond-zero" (classify 0) 'zero)
(expect "cond-pos"  (classify 7) 'pos)

;; and / or short-circuit
(expect "and-all"   (and 1 2 3) 3)
(expect "and-stop"  (and 1 nil 3) nil)
(expect "or-first"  (or nil nil 5) 5)
(expect "or-none"   (or nil nil) nil)

;; while
(setq i 0)
(setq total 0)
(while (< i 5)
  (setq total (+ total i))
  (setq i (1+ i)))
(expect "while" total 10)

;; dotimes / dolist (prelude macros)
(setq sum 0)
(dotimes (k 4) (setq sum (+ sum k)))
(expect "dotimes" sum 6)

(setq collected nil)
(dolist (x (list 1 2 3)) (setq collected (cons (* x x) collected)))
(expect "dolist" collected (list 9 4 1))

(message "control-flow: all checks passed on fusevm")
