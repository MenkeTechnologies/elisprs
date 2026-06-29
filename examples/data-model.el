;;; data-model.el --- structs, generics, pcase, hashing, cl-loop  -*- lexical-binding: nil; -*-

;; Exercises the OO/data surface the way real packages do: cl-defstruct with
;; :include inheritance, cl-defmethod dispatch, pcase destructuring, hash-table
;; aggregation, and cl-loop. ERT-tested; passes under GNU Emacs too.
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(cl-defstruct shape name)
(cl-defstruct (circle (:include shape)) radius)
(cl-defstruct (rect (:include shape)) width height)

(cl-defgeneric area (s) "Area of shape S.")
(cl-defmethod area ((c circle)) (* float-pi (circle-radius c) (circle-radius c)))
(cl-defmethod area ((r rect)) (* (rect-width r) (rect-height r)))

(defun classify-size (s)
  "Bucket S by area using pcase guards."
  (let ((a (area s)))
    (pcase a
      ((pred (lambda (x) (< x 10))) 'small)
      ((and n (guard (< n 100))) 'medium)
      (_ 'large))))

(ert-deftest data-model-generics-and-inheritance ()
  "cl-defmethod dispatches on struct type; :include shares the `name' slot."
  (let ((c (make-circle :name "c" :radius 2))
        (r (make-rect :name "r" :width 3 :height 4)))
    (should (equal (shape-name c) "c"))         ; inherited accessor
    (should (shape-p c))                          ; subtype satisfies parent pred
    (should (= (area r) 12))
    (should (< (abs (- (area c) 12.566)) 0.01))
    (should (eq (classify-size (make-rect :width 2 :height 2)) 'small))
    (should (eq (classify-size r) 'medium))
    (should (eq (classify-size (make-rect :width 20 :height 20)) 'large))))

(ert-deftest data-model-hash-aggregation ()
  "Tally word lengths into a hash table, then read it back with cl-loop."
  (let ((counts (make-hash-table :test 'eql))
        (words '("a" "bb" "cc" "ddd" "ee")))
    (dolist (w words)
      (cl-incf (gethash (length w) counts 0)))
    (should (= (gethash 2 counts) 3))
    (should (= (gethash 1 counts) 1))
    (let ((pairs (cl-loop for k being the hash-keys of counts using (hash-values v)
                          collect (cons k v))))
      (should (= (seq-reduce #'+ (mapcar #'cdr pairs) 0) 5)))))

(ert-deftest data-model-error-handling ()
  "Custom error hierarchy with condition-case inheritance."
  (define-error 'parse-error "Parse error")
  (define-error 'bad-token "Bad token" 'parse-error)
  (should (eq (condition-case e (signal 'bad-token '("x"))
                (parse-error 'caught-parent))
              'caught-parent))
  (should (equal (condition-case e (signal 'bad-token '("oops"))
                   (error (cadr e)))
                 "oops")))

(ert-run-tests-batch-and-exit)
