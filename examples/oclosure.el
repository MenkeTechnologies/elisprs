;;; oclosure.el --- OClosure define/lambda/accessors/type, ERT-tested  -*- lexical-binding: t; -*-

;; A faithful port of emacs-lisp/oclosure.el: an OClosure is a closure that also
;; carries a type (for cl-generic dispatch) and named, optionally-mutable slots
;; reachable from outside via generated accessors. Every value asserted here was
;; checked against `emacs -Q --batch' (GNU Emacs 30.2). Types are defined at top
;; level (not inside a single form) so `oclosure-lambda' can see the class at
;; macroexpansion time — which is why this lives in an example script.
(message "== oclosure demo ==")

(oclosure-define foo a b)

(oclosure-define (point3 (:predicate point3-p)) x (y) (z :mutable t))
(oclosure-define (cpoint (:parent point3)) color)

(oclosure-define (counter) (n :mutable t))

;; The slotless type cl-generic uses for its no-next-method closures. Defined at
;; top level so `oclosure-lambda' can resolve the class when the test below is
;; macroexpanded (exactly as cl-generic.el defines it at top level).
(oclosure-define (cl--generic-nnm))

(ert-deftest oclosure-call-and-accessors ()
  "An instance is callable and its immutable slots read back through accessors."
  (let ((o (oclosure-lambda (foo (a 1) (b 2)) () (+ a b))))
    (should (= (funcall o) 3))
    (should (= (foo--a o) 1))
    (should (= (foo--b o) 2))))

(ert-deftest oclosure-type-and-predicate ()
  "`oclosure-type' returns the instance's type; plain lambdas are not OClosures."
  (let ((o (oclosure-lambda (foo (a 1) (b 2)) () (+ a b))))
    (should (eq (oclosure-type o) 'foo))
    (should (oclosure--p o))
    ;; The generated predicate walks the allparents list (memq returns the tail).
    (should (equal (foo--internal-p o) '(foo oclosure)))
    (should (eq (oclosure-type (lambda () 1)) nil))
    (should (eq (oclosure--p (lambda () 1)) nil))
    (should (eq (closurep o) t))))

(ert-deftest oclosure-slots-are-closure-locals ()
  "Slot values are also in-scope locals of the body (same storage)."
  (let ((o (oclosure-lambda (foo (a 10) (b 5)) (k) (* k (+ a b)))))
    (should (= (funcall o 3) 45))))

(ert-deftest oclosure-mutable-slot-set-and-setf ()
  "A `:mutable' slot is settable via `oclosure--set-slot-value' and `setf', and
the mutation is visible to the body (shared cell)."
  (let ((o (oclosure-lambda (point3 (x 10) (y 20) (z 30)) (d) (+ x y z d))))
    (should (= (point3--z o) 30))
    (oclosure--set-slot-value o 'z 300)
    (should (= (point3--z o) 300))
    (should (= (funcall o 5) 335))
    (setf (point3--z o) 999)
    (should (= (point3--z o) 999))
    (should (= (funcall o 5) 1034))))

(ert-deftest oclosure-immutable-slot-signals ()
  "Setting an immutable slot signals `setting-constant'."
  (let ((o (oclosure-lambda (point3 (x 1) (y 2) (z 3)) () x)))
    (should-error (oclosure--set-slot-value o 'x 99) :type 'setting-constant)))

(ert-deftest oclosure-inheritance ()
  "A child type inherits parent slots (in order) and satisfies the parent predicate."
  (let ((c (oclosure-lambda (cpoint (x 1) (y 2) (z 3) (color 99)) () color)))
    (should (eq (oclosure-type c) 'cpoint))
    (should (= (point3--x c) 1))
    (should (= (cpoint--color c) 99))
    (should (equal (point3-p c) '(point3 oclosure)))
    (should (equal (cpoint--internal-p c) '(cpoint point3 oclosure)))
    ;; A parent instance is not a child.
    (let ((p (oclosure-lambda (point3 (x 1) (y 2) (z 3)) () x)))
      (should (eq (cpoint--internal-p p) nil)))))

(ert-deftest oclosure-accessor-introspection ()
  "The accessor objects are themselves OClosures of type `oclosure-accessor'
carrying their own type/slot metadata."
  (let ((acc (symbol-function 'foo--a)))
    (should (eq (accessor--type acc) 'foo))
    (should (eq (accessor--slot acc) 'a))
    (should (eq (oclosure-type acc) 'oclosure-accessor))))

(ert-deftest oclosure-independent-instances ()
  "Two instances of the same type carry independent slot values and code."
  (let ((a (oclosure-lambda (foo (a 1) (b 1)) () (+ a b)))
        (b (oclosure-lambda (foo (a 100) (b 200)) () (- a b))))
    (should (= (funcall a) 2))
    (should (= (funcall b) -100))
    (should (= (foo--a a) 1))
    (should (= (foo--a b) 100))))

(ert-deftest oclosure-nnm-dispatch-pattern ()
  "The exact pattern cl-generic uses: a slotless nnm type whose identity is a
mere `oclosure-type' test (`cl--generic-isnot-nnm-p')."
  (let ((nnm (oclosure-lambda (cl--generic-nnm) (&rest args) (list :nnm args)))
        (plain (lambda (x) x)))
    (should (eq (oclosure-type nnm) 'cl--generic-nnm))
    (should (equal (funcall nnm 1 2) '(:nnm (1 2))))
    (should (eq (not (eq (oclosure-type nnm) 'cl--generic-nnm)) nil))
    (should (eq (not (eq (oclosure-type plain) 'cl--generic-nnm)) t))))

(ert-deftest oclosure-slot-value-by-name ()
  "`oclosure--slot-value' reads a slot by name through the class index table."
  (let ((c (oclosure-lambda (cpoint (x 7) (y 8) (z 9) (color 3)) () color)))
    (should (= (oclosure--slot-value c 'x) 7))
    (should (= (oclosure--slot-value c 'color) 3))))

(ert-run-tests-batch-and-exit)
