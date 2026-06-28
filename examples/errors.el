;;; errors.el --- non-local exits: catch/throw, condition-case, unwind  -*- lexical-binding: nil; -*-

;; The compiler rewrites catch / unwind-protect / condition-case into fusevm
;; intrinsics that thread an exit value through the VM. This file proves the
;; happy and error paths both behave; a wrong result errors out (non-zero exit).

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

;; catch / throw — value returned through the dynamic extent.
(expect "catch-throw" (catch 'tag (throw 'tag 42)) 42)
(expect "catch-fall"  (catch 'tag 1 2 3) 3)
(expect "catch-break"
        (catch 'found
          (dotimes (i 100)
            (when (= i 7) (throw 'found i))))
        7)

;; condition-case — recover from a signalled error.
(expect "condcase-catch" (condition-case nil (error "boom") (error 'recovered)) 'recovered)
(expect "condcase-ok"    (condition-case nil (+ 1 2) (error 'unused)) 3)

;; ignore-errors — swallow an error, yield nil.
(expect "ignore-errors-err" (ignore-errors (error "x")) nil)
(expect "ignore-errors-ok"  (ignore-errors 123) 123)

;; unwind-protect — cleanup runs even when the body errors.
(setq cleaned nil)
(ignore-errors
  (unwind-protect
      (error "boom")
    (setq cleaned t)))
(expect "unwind-protect" cleaned t)

(message "errors: all checks passed on fusevm")
