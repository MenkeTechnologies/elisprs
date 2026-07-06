;;; gv.el --- generalized-variable place expanders, ERT-tested  -*- lexical-binding: t; -*-

;; A faithful port of emacs-lisp/gv.el: a place-expander is a higher-order
;; function (getter setter -> code) -> code stored on the head symbol's
;; `gv-expander' property.  `gv-get'/`gv-letplace' turn a PLACE form into that
;; call, `gv-define-setter'/`gv-define-simple-setter'/`gv-define-expander'
;; populate the registry, and `gv-ref'/`gv-deref' build first-class references.
;; `setf' consults the registry for any place its own cond does not special-case.
;; Every value asserted here was checked against `emacs -Q --batch' (GNU Emacs
;; 30.2).  gv-define-setter runs at top level so the expander it installs is
;; visible when the tests below are macroexpanded.
(message "== gv demo ==")

(gv-define-setter my-car (v x) `(setcar ,x ,v))
(gv-define-simple-setter my-set-value set 'fix)

(ert-deftest gv-define-setter-place ()
  "A user place installed via `gv-define-setter' is assignable through `setf'."
  (let ((l (list 1 2)))
    (setf (my-car l) 9)
    (should (equal l '(9 2)))))

(ert-deftest gv-define-simple-setter-fix-return ()
  "`gv-define-simple-setter' with FIX-RETURN yields VAL, not the setter's result."
  (let ((s (make-symbol "z")))
    (should (= (setf (my-set-value s) 42) 42))
    (should (= (symbol-value s) 42))))

(ert-deftest gv-ref-deref-roundtrip ()
  "`gv-ref' captures a place; `gv-deref' reads it and (setf (gv-deref r) v) writes."
  (let* ((x (list 1))
         (r (gv-ref (car x))))
    (should (= (gv-deref r) 1))
    (setf (gv-deref r) 5)
    (should (= (car x) 5))))

(ert-deftest gv-ref-on-nth-place ()
  "`gv-ref' works on a registered compound place like (nth N L)."
  (let* ((l (list 10 20 30))
         (r (gv-ref (nth 1 l))))
    (setf (gv-deref r) 99)
    (should (= (gv-deref r) 99))
    (should (equal l '(10 99 30)))))

(ert-deftest gv-ref-on-control-flow-place ()
  "The `if' gv-expander lets `gv-ref' reference a place chosen at runtime."
  (let* ((a (list 1))
         (b (list 2))
         (c t)
         (r (gv-ref (car (if c a b)))))
    (setf (gv-deref r) 7)
    (should (equal a '(7)))
    (should (equal b '(2)))))

(ert-deftest setf-through-gv-registry ()
  "`setf' falls through to the gv registry for a place its own cond misses."
  ;; gv-deref is only reachable via the gv-expander fallback in `setf--expand'.
  (let* ((cell (list 0))
         (ref (cons (lambda () (car cell))
                    (lambda (v) (setcar cell v)))))
    (setf (gv-deref ref) 12)
    (should (= (car cell) 12))
    (should (= (gv-deref ref) 12))))

(ert-deftest setf-existing-places-still-work ()
  "The gv registry fallback does not disturb `setf''s built-in place handling."
  (let ((l (list 1 2 3))
        (v (vector 0 0))
        (h (make-hash-table)))
    (setf (car l) 9)
    (setf (nth 2 l) 7)
    (setf (aref v 1) 8)
    (setf (gethash 'k h) 5)
    (setf (alist-get 'a l) 1)
    (should (equal l '((a . 1) 9 2 7)))
    (should (= (aref v 1) 8))
    (should (= (gethash 'k h) 5))))

(ert-run-tests-batch-and-exit)
