;;; adversarial-panics.el --- adversarial robustness vs GNU Emacs 30.2  -*- lexical-binding: nil; -*-

;; A faithful interpreter must NEVER panic/abort on any input. This file pins
;; the fixes from an adversarial panic hunt: malformed/extreme inputs that used
;; to abort the process (Rust panic / stack overflow) now signal the exact
;; elisp condition Emacs 30.2 signals, or return Emacs's exact value. Every
;; `should` below was oracle-verified against `emacs -Q --batch`.
(message "== adversarial-panics demo ==")

;; ── format: huge precision (Rust's u16 precision field panics >= 65536) ──
(ert-deftest format-huge-precision-no-panic ()
  "%f/%e/%g with a precision past Rust's u16 formatter limit used to panic
(\"Formatting argument out of range\"). Emacs pads past the value's exact
decimal expansion with zeros; %g trims trailing zeros so it stays short."
  ;; %f: exact double expansion + zero padding.
  (should (= (length (format "%.70000f" 1.5)) 70002))
  (should (equal (format "%.70000f" 1.5) (concat "1.5" (make-string 69999 ?0))))
  ;; Negative and signed forms are unaffected by the cap+pad path.
  (should (equal (format "%.70000f" -2.25)
                 (concat "-2.25" (make-string 69998 ?0))))
  ;; %e: mantissa padded with zeros before the exponent.
  (should (= (length (format "%.70000e" 1.5)) 70006))
  (should (string-suffix-p "0e+00" (format "%.70000e" 1.5)))
  (should (string-prefix-p "1.500000" (format "%.70000e" 1.5)))
  ;; %g trims trailing zeros -> huge precision still yields the short form.
  (should (equal (format "%.70000g" 1.5) "1.5"))
  ;; The just-under-the-cap boundary keeps working exactly.
  (should (= (length (format "%.65535f" 1.0)) 65537)))

;; ── higher-order primitives called with too few args ──
;; These re-enter the evaluator (funcall/apply/mapcar/mapc/sort/maphash) and
;; used to index an empty arg slice -> "index out of bounds" panic. Emacs
;; signals wrong-number-of-arguments (with the subr name and the count given).
(ert-deftest higher-order-arity-no-panic ()
  "funcall/mapcar/mapc/sort/maphash with too few args signal
wrong-number-of-arguments, not a Rust index panic."
  (should (equal (condition-case e (funcall) (error e))
                 '(wrong-number-of-arguments funcall 0)))
  (should (equal (condition-case e (mapcar #'car) (error e))
                 '(wrong-number-of-arguments mapcar 1)))
  (should (equal (condition-case e (mapc #'car) (error e))
                 '(wrong-number-of-arguments mapc 1)))
  (should (equal (condition-case e (sort) (error e))
                 '(wrong-number-of-arguments sort 0)))
  (should (equal (condition-case e (maphash) (error e))
                 '(wrong-number-of-arguments maphash 0)))
  (should (equal (condition-case e (maphash #'ignore) (error e))
                 '(wrong-number-of-arguments maphash 1))))

(ert-deftest apply-arity-and-spread-no-panic ()
  "apply with no args signals wrong-number-of-arguments; apply spreads its
LAST argument, which must be a list -- with a single arg that last IS the
function, so `(apply '+)' fails `(wrong-type-argument listp +)' like Emacs.
Well-formed applies are unaffected."
  (should (equal (condition-case e (apply) (error e))
                 '(wrong-number-of-arguments apply 0)))
  (should (equal (condition-case e (apply '+) (error e))
                 '(wrong-type-argument listp +)))
  (should (equal (condition-case e (apply '+ 5) (error e))
                 '(wrong-type-argument listp 5)))
  ;; Normal spread forms still evaluate.
  (should (= (apply '+ '(1 2 3)) 6))
  (should (= (apply '+ 1 2 '(3 4)) 10))
  (should (equal (apply 'list 1 '(2 3)) '(1 2 3))))

(ert-run-tests-batch-and-exit)
