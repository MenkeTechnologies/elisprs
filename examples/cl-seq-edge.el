;;; cl-seq-edge.el --- cl-lib seq keyword edge cases, ERT-tested vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; These exercise the cl-lib keyword protocol on the sequence functions where
;; :from-end interacts with :count (remove/substitute the LAST COUNT matches,
;; keeping original order) and where :from-end/:key change the answer of
;; cl-mismatch / cl-search. Also covers subr.el's assoc-default TEST/DEFAULT.
;; Every expected value was captured from real `emacs -Q --batch' 30.2.
(require 'cl-lib)
(message "== cl-seq keyword edge cases ==")

(ert-deftest cl-remove-from-end-count ()
  "Without :from-end the FIRST COUNT matches go; with :from-end the LAST COUNT."
  (should (equal (cl-remove 2 (list 1 2 3 2 1) :count 1) '(1 3 2 1)))
  (should (equal (cl-remove 2 (list 1 2 3 2 1) :count 1 :from-end t) '(1 2 3 1)))
  (should (equal (cl-remove-if #'cl-evenp (list 1 2 3 4 5 6) :count 2) '(1 3 5 6)))
  (should (equal (cl-remove-if #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)
                 '(1 2 3 5)))
  (should (equal (cl-remove-if-not #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)
                 '(1 2 4 6)))
  ;; cl-delete shares cl-remove's semantics.
  (should (equal (cl-delete 2 (list 1 2 3 2 1) :count 1 :from-end t) '(1 2 3 1)))
  ;; :key applies to remove-if's predicate argument.
  (should (equal (cl-remove-if #'cl-evenp (list '(1) '(2) '(3) '(4)) :key #'car)
                 '((1) (3)))))

(ert-deftest cl-substitute-from-end-count ()
  "cl-substitute / cl-substitute-if replace the LAST COUNT matches under :from-end."
  (should (equal (cl-substitute 9 2 (list 1 2 3 2 1) :count 1) '(1 9 3 2 1)))
  (should (equal (cl-substitute 9 2 (list 1 2 3 2 1) :count 1 :from-end t) '(1 2 3 9 1)))
  (should (equal (cl-substitute-if 9 #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)
                 '(1 2 3 9 5 9)))
  (should (equal (cl-nsubstitute 9 2 (list 1 2 3 2 1) :count 1 :from-end t)
                 '(1 2 3 9 1))))

(ert-deftest cl-mismatch-from-end-and-key ()
  "First differing index; :from-end reports the trailing mismatch, :key maps both."
  (should-not (cl-mismatch (list 1 2 3) (list 1 2 3)))
  (should (= (cl-mismatch (list 1 2 3) (list 1 9 3)) 1))
  (should (= (cl-mismatch (list 1 2 3 4) (list 1 2) :from-end t) 3))
  (should (= (cl-mismatch "abcde" "abXde") 2))
  (should (= (cl-mismatch (list '(1) '(2) '(9)) (list '(1) '(2) '(3)) :key #'car) 2)))

(ert-deftest cl-search-from-end ()
  "Leftmost match index by default; rightmost under :from-end; empty-needle rules."
  (should (= (cl-search (list 2 3) (list 1 2 3 2 3)) 1))
  (should (= (cl-search (list 2 3) (list 1 2 3 2 3) :from-end t) 3))
  (should (= (cl-search (list 1 2) (list 1 2 3 1 2) :from-end t) 3))
  (should-not (cl-search (list 9) (list 1 2 3)))
  (should (= (cl-search "bc" "abcbc" :from-end t) 3))
  (should (= (cl-search (list) (list 1 2 3)) 0))
  (should (= (cl-search (list) (list 1 2 3) :from-end t) 3)))

(ert-deftest assoc-default-test-and-default ()
  "TEST is called (ELEM KEY); a matching non-cons element yields DEFAULT."
  (should (equal (assoc-default "b" (list '("a" . 1) '("b" . 2))) 2))
  (should (equal (assoc-default 2 (list '(1 . a) '(2 . b)) #'=) 'b))
  (should-not (assoc-default "x" (list '("a" . 1)) nil 'def))
  (should (equal (assoc-default 5 (list 3 5 7) #'= 'hit) 'hit)))

(ert-run-tests-batch-and-exit)
