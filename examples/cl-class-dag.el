;;; cl-class-dag.el --- built-in + struct class DAG, ERT-tested  -*- lexical-binding: t; -*-

;; A faithful port of cl-preloaded.el's class registry: every built-in type and
;; every `cl-defstruct' type registers a class descriptor on its symbol's
;; `cl--class' property, and `cl--class-allparents' linearises the parent DAG.
;; cl-generic's typeof generalizer walks this DAG to dispatch on built-in and
;; struct types, so getting it byte-identical to Emacs is what lets the real
;; emacs-lisp/cl-generic.el load and dispatch on `integer', `string', `list',
;; etc. Every value asserted here was checked against `emacs -Q --batch'
;; (GNU Emacs 30.2, `cl--class-allparents' / `type-of').
(require 'cl-lib)
(message "== cl class DAG demo ==")

(cl-defstruct dag-p a b)                        ; plain struct, parent cl-structure-object
(cl-defstruct (dag-c (:include dag-p)) c)       ; :include chains through dag-p

(ert-deftest cl-class-dag-builtin-numbers ()
  "The numeric-tower built-in classes linearise exactly as in Emacs."
  (should (equal (cl--class-allparents (cl--find-class 'fixnum))
                 '(fixnum integer number integer-or-marker number-or-marker atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'integer))
                 '(integer number integer-or-marker number-or-marker atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'number))
                 '(number number-or-marker atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'float))
                 '(float number number-or-marker atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'marker))
                 '(marker integer-or-marker number-or-marker atom t))))

(ert-deftest cl-class-dag-builtin-sequences ()
  "Sequence/array built-in classes linearise exactly as in Emacs."
  (should (equal (cl--class-allparents (cl--find-class 'sequence)) '(sequence t)))
  (should (equal (cl--class-allparents (cl--find-class 'list)) '(list sequence t)))
  (should (equal (cl--class-allparents (cl--find-class 'cons)) '(cons list sequence t)))
  (should (equal (cl--class-allparents (cl--find-class 'array)) '(array sequence atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'string))
                 '(string array sequence atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'vector))
                 '(vector array sequence atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'symbol)) '(symbol atom t)))
  ;; `null' sits under both `boolean' (hence `symbol') and `list'.
  (should (equal (cl--class-allparents (cl--find-class 'null))
                 '(null boolean symbol atom list sequence t))))

(ert-deftest cl-class-dag-builtin-class-type ()
  "Built-in type descriptors are `built-in-class' objects."
  (should (eq (type-of (cl--find-class 'integer)) 'built-in-class))
  (should (eq (type-of (cl--find-class 'sequence)) 'built-in-class))
  (should (built-in-class-p (cl--find-class 'cons)))
  ;; A symbol with no registered class returns nil.
  (should (eq (cl--find-class 'no-such-type-xyzzy) nil)))

(ert-deftest cl-class-dag-metaclasses ()
  "The metaclass chain record -> cl-structure-object -> cl--class ->
cl-structure-class linearises exactly as in Emacs."
  (should (equal (cl--class-allparents (cl--find-class 'record)) '(record atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'cl-structure-object))
                 '(cl-structure-object record atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'cl--class))
                 '(cl--class cl-structure-object record atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'cl-structure-class))
                 '(cl-structure-class cl--class cl-structure-object record atom t))))

(ert-deftest cl-class-dag-defstruct-registers-class ()
  "`cl-defstruct' registers a `cl-structure-class', and `:include' chains it."
  (should (eq (type-of (cl--find-class 'dag-p)) 'cl-structure-class))
  (should (cl--struct-class-p (cl--find-class 'dag-p)))
  (should (equal (cl--class-allparents (cl--find-class 'dag-p))
                 '(dag-p cl-structure-object record atom t)))
  (should (equal (cl--class-allparents (cl--find-class 'dag-c))
                 '(dag-c dag-p cl-structure-object record atom t))))

(ert-run-tests-batch-and-exit)
