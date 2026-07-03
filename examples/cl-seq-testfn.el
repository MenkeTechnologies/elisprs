;;; cl-seq-testfn.el --- assoc TESTFN + cl-lib :test-not/:start/:end parity  -*- lexical-binding: nil; -*-

;; Pins the keyword protocol where elisprs previously diverged from Emacs 30.2:
;;   * `assoc' with a TESTFN calls (funcall TEST (car ELEMENT) KEY) — element
;;     first, key second (NOT reversed).
;;   * :test-not selects an element when (funcall TEST-NOT item elt) is nil,
;;     across cl-member / cl-assoc / cl-rassoc / cl-find / cl-remove /
;;     cl-position / cl-count.
;;   * :start/:end bound the active window on cl-remove / cl-substitute /
;;     cl-find / cl-find-if / cl-remove-if (which previously ignored them),
;;     interacting correctly with :count and :from-end.
;; Every expected value was captured from real `emacs -Q --batch' 30.2.
(require 'cl-lib)
(message "== assoc TESTFN + cl-lib :test-not/:start/:end ==")

(ert-deftest assoc-testfn-arg-order ()
  "TESTFN receives (car ELEMENT) first, KEY second — like real `assoc'."
  ;; (> elem-car key): first element whose car exceeds nothing... here KEY=3,
  ;; (> 1 3) nil, so with reversed args the old code found nothing; correct
  ;; order asks (> 1 3)=nil too — pick a predicate that distinguishes the order.
  (should (equal (assoc 3 '((1 . a) (2 . b)) (lambda (elem key) (< elem key)))
                 '(1 . a)))
  (should (equal (assoc 3 '((4 . a) (2 . b)) (lambda (elem key) (< elem key)))
                 '(2 . b)))
  ;; Default (no TESTFN) still uses `equal'.
  (should (equal (assoc "b" '(("a" . 1) ("b" . 2))) '("b" . 2))))

(ert-deftest cl-seq-test-not ()
  ":test-not matches an element when (TEST-NOT item elt) returns nil."
  (should (equal (cl-member 2 '(1 2 3) :test-not #'eql) '(1 2 3)))
  (should (equal (cl-assoc 2 '((1 . a) (2 . b) (3 . c)) :test-not #'eql) '(1 . a)))
  (should (equal (cl-rassoc 2 '((a . 1) (b . 2)) :test-not #'eql) '(a . 1)))
  (should (equal (cl-find 2 '(1 2 3) :test-not #'eql) 1))
  (should (equal (cl-remove 2 '(1 2 3 2) :test-not #'eql) '(2 2)))
  (should (equal (cl-count 5 '(5 1 5 2 5) :test-not #'eql) 2))
  (should (equal (cl-position 2 '(1 2 3) :test-not #'eql) 0))
  ;; :test-not with :key selects the first list whose car is NOT < item.
  (should (equal (cl-member 4 '((1) (2) (3)) :key #'car :test-not #'<)
                 '((1) (2) (3)))))

(ert-deftest cl-seq-start-end ()
  ":start/:end confine the active window; outside elements pass through."
  (should (equal (cl-remove 2 '(2 1 2 3 2) :start 1 :end 4) '(2 1 3 2)))
  (should (equal (cl-remove 5 '(5 1 5 2 5) :test-not #'eql :start 1 :end 4) '(5 5 5)))
  (should (equal (cl-substitute 9 2 '(2 1 2 3 2) :start 1 :end 4) '(2 1 9 3 2)))
  (should (equal (cl-find 2 '(2 1 2 3) :start 1) 2))
  (should-not (cl-find 2 '(2 1 2 3) :start 1 :end 2))
  (should (equal (cl-find-if #'cl-evenp '(1 2 3 4) :start 2) 4))
  (should-not (cl-find-if #'cl-evenp '(2 1 2 3) :start 1 :end 2))
  (should (equal (cl-remove-if #'cl-evenp '(2 1 2 3 2) :start 1 :end 4) '(2 1 3 2)))
  ;; cl-delete shares cl-remove's window semantics.
  (should (equal (cl-delete 2 (list 2 1 2 3 2) :start 1 :end 4) '(2 1 3 2))))

(ert-deftest cl-seq-start-end-count-from-end ()
  ":start/:end combine with :count and :from-end (last COUNT within window)."
  (should (equal (cl-remove 2 '(2 1 2 3 2) :start 1 :end 4 :from-end t :count 1)
                 '(2 1 3 2)))
  (should (equal (cl-substitute 9 5 '(5 5 5 5 5) :start 1 :end 4 :count 1 :from-end t)
                 '(5 5 5 9 5)))
  (should (equal (cl-substitute 9 2 '(2 1 2 3 2) :count 1 :from-end t :start 1)
                 '(2 1 2 3 9)))
  (should (equal (cl-position 2 '(2 1 2 3 2) :from-end t :start 1 :end 4) 2)))

(ert-run-tests-batch-and-exit)
