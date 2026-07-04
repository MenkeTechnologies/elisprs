;;; cl-seq-destructive.el --- cl-fill/cl-replace/cl-subseq-setf + multi-seq cl-some/every, ERT vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; cl-fill and cl-replace DESTRUCTIVELY modify their target sequence in place
;; (per cl-seq.el), returning it; the prior prelude built a fresh copy and left
;; the original untouched. (setf (cl-subseq SEQ START END) V) routes through
;; cl-replace. cl-some/cl-every/cl-notany/cl-notevery accept multiple sequences
;; and iterate in parallel, stopping at the shortest (per cl-extra.el).
;; Every expected value captured from real `emacs -Q --batch' 30.2.
(require 'cl-lib)
(message "== cl-seq destructive + multi-seq ==")

(ert-deftest cl-fill-mutates-in-place ()
  "cl-fill overwrites the ORIGINAL list/vector between :start and :end."
  (let ((l (list 1 2 3 4 5)))
    (should (equal (cl-fill l 'x :start 1 :end 3) '(1 x x 4 5)))
    (should (equal l '(1 x x 4 5))))                 ; same object was mutated
  (let ((v (vector 0 1 2 3 4)))
    (cl-fill v 9 :start 1 :end 3)
    (should (equal v [0 9 9 3 4])))
  (let ((v (vector 1 2 3)))
    (cl-fill v 7)
    (should (equal v [7 7 7]))))

(ert-deftest cl-replace-mutates-in-place ()
  "cl-replace copies SEQ2 into SEQ1, mutating SEQ1, honoring the :startN/:endN."
  (let ((l (list 1 2 3 4 5)))
    (should (equal (cl-replace l '(a b) :start1 1) '(1 a b 4 5)))
    (should (equal l '(1 a b 4 5))))
  (let ((v (vector 0 0 0 0)))
    (cl-replace v [7 8] :start1 2)
    (should (equal v [0 0 7 8])))
  ;; :start2/:end2 select the source slice; list target stays the same object.
  (let ((l (list 1 2 3)))
    (cl-replace l '(a b c d) :start2 1 :end2 3)
    (should (equal l '(b c 3))))
  ;; list source into a vector target copies element-wise.
  (let ((v (vector 1 2 3 4)))
    (cl-replace v '(a b) :start1 1)
    (should (equal v [1 a b 4]))))

(ert-deftest cl-subseq-setf ()
  "(setf (cl-subseq SEQ START [END]) V) writes V into the slice destructively."
  (let ((l (list 1 2 3 4 5)))
    (setf (cl-subseq l 1 3) '(20 30))
    (should (equal l '(1 20 30 4 5))))
  (let ((v (vector 1 2 3 4)))
    (setf (cl-subseq v 1 3) [20 30])
    (should (equal v [1 20 30 4])))
  ;; No END: replace from START to the end of SEQ.
  (let ((l (list 1 2 3 4 5)))
    (setf (cl-subseq l 2) '(a b c))
    (should (equal l '(1 2 a b c)))))

(ert-deftest cl-some-every-multi-seq ()
  "With extra sequences PRED is applied across them in parallel, shortest wins."
  (should (eq (cl-some #'= '(1 2 3) '(3 2 1)) t))
  (should (eq (cl-some #'> '(1 2 3) '(5 5 5)) nil))
  (should (eq (cl-every #'< '(1 2 3) '(2 3 4)) t))
  ;; stops at the shorter sequence => the third pair is never compared
  (should (eq (cl-every #'< '(1 2 3) '(2 3)) t))
  (should (eq (cl-notany #'= '(1 2) '(3 4)) t))
  (should (eq (cl-notevery #'= '(1 2) '(1 3)) t))
  ;; cl-some returns the predicate's first non-nil VALUE, not just t
  (should (eq (cl-some #'identity [nil nil 3]) 3))
  ;; three-way parallel map
  (should (eq (cl-some (lambda (a b c) (> (+ a b c) 10)) '(1 2 3) '(4 5 6) '(1 1 5)) t)))

(ert-run-tests-batch-and-exit)
