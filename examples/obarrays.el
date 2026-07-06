;;; obarrays.el --- first-class obarrays, ERT-tested  -*- lexical-binding: nil; -*-

;; First-class obarrays (`obarray-make' and the symbol-table primitives that
;; take an OBARRAY argument), as documented in the Emacs Lisp manual "Creating
;; and Interning Symbols" and implemented in C `obarray.c'/`lread.c':
;;
;;   obarray-make (&optional SIZE), obarrayp, intern (NAME &optional OB),
;;   intern-soft (NAME &optional OB), unintern (NAME &optional OB), mapatoms
;;   (FN &optional OB), and the `obarray' variable (the global obarray object).
;;
;; A private obarray is its own namespace: symbols interned in it are distinct
;; (not `eq') from the like-named global symbols, carry their own value cells,
;; and are enumerated only by `mapatoms' over that obarray.  Emacs 30 no longer
;; treats a plain vector as an obarray, so `obarrayp' on a vector is nil.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Assertions are structural (identity, membership, counts, printed shape) rather
;; than tied to drifting symbol addresses.  Run through fusevm; the batch runner
;; gates the suite.
(message "== first-class obarrays ==")

;; ---- obarray-make: predicate, type, printed form, SIZE validation ----
(ert-deftest ob-make-basic ()
  (let ((ob (obarray-make 5)))
    (should (eq (obarrayp ob) t))
    (should (eq (type-of ob) 'obarray))
    ;; A fresh obarray holds zero symbols and prints its count.
    (should (equal (format "%S" ob) "#<obarray n=0>"))
    ;; SIZE is a vestigial capacity hint but must be a wholenum when supplied.
    (should (eq (obarrayp (obarray-make)) t))
    (should (equal (should-error (obarray-make "x")) '(wrong-type-argument wholenump "x")))
    (should (equal (should-error (obarray-make -1)) '(wrong-type-argument wholenump -1))))
  ;; Emacs 30 vectors and nil are NOT obarrays.
  (should (eq (obarrayp (make-vector 3 0)) nil))
  (should (eq (obarrayp nil) nil))
  ;; The global `obarray' variable is itself a genuine obarray object.
  (should (eq (obarrayp obarray) t)))

;; ---- intern / intern-soft in a private obarray: identity + isolation ----
(ert-deftest ob-intern-isolation ()
  (let* ((ob (obarray-make))
         (s (intern "foo" ob)))
    ;; Interning the same name twice returns the very same symbol.
    (should (eq s (intern "foo" ob)))
    ;; ...but it is distinct from the like-named GLOBAL symbol.
    (should (not (eq s (intern "foo"))))
    (should (string= (symbol-name s) "foo"))
    ;; intern-soft finds a present name, nil for an absent one.
    (should (eq (intern-soft "foo" ob) s))
    (should (eq (intern-soft "absent" ob) nil))
    ;; A private symbol carries its own value cell.
    (set s 42)
    (should (eq (symbol-value s) 42))
    ;; One symbol interned -> the printed count advances.
    (should (equal (format "%S" ob) "#<obarray n=1>"))))

;; ---- unintern: removal + boolean result ----
(ert-deftest ob-unintern ()
  (let ((ob (obarray-make)))
    (intern "a" ob)
    (intern "b" ob)
    ;; unintern returns t when it removed a symbol, nil when the name was absent.
    (should (eq (unintern "a" ob) t))
    (should (eq (unintern "missing" ob) nil))
    (should (eq (intern-soft "a" ob) nil))
    (should (eq (obarrayp (intern-soft "b" ob)) nil)) ; "b" still a symbol, not an obarray
    (should (string= (symbol-name (intern-soft "b" ob)) "b"))
    (should (equal (format "%S" ob) "#<obarray n=1>"))))

;; ---- mapatoms over a private obarray: visits exactly its symbols ----
(ert-deftest ob-mapatoms ()
  (let ((ob (obarray-make))
        (seen nil))
    (intern "one" ob)
    (intern "two" ob)
    (intern "three" ob)
    (mapatoms (lambda (s) (push (symbol-name s) seen)) ob)
    ;; Order is unspecified, so compare as sets.
    (should (equal (sort seen #'string<) '("one" "three" "two")))
    ;; mapatoms returns nil.
    (should (eq (mapatoms #'ignore ob) nil))))

(ert-run-tests-batch-and-exit)
