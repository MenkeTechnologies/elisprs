;;; cl-sets.el --- cl-lib set ops & seq-group-by, ERT-tested vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; These prelude functions honor cl-lib's keyword protocol (:test defaults to
;; `eql', :key defaults to `identity') and reproduce Emacs' element ordering.
;; Every expected value below was captured from real `emacs -Q --batch' 30.2.
(message "== cl set ops demo ==")

(ert-deftest cl-remove-duplicates-default-eql ()
  "Default comparison is `eql', so distinct string objects are NOT merged;
default keeps the LAST occurrence, :from-end keeps the FIRST."
  (should (equal (cl-remove-duplicates (list "a" "a" "b")) (list "a" "a" "b")))
  (should (equal (cl-remove-duplicates (list "a" "a" "b") :test #'equal) (list "a" "b")))
  (should (equal (cl-remove-duplicates '(1 2 1 3 2)) '(1 3 2)))
  (should (equal (cl-remove-duplicates '(1 2 1 3 2) :from-end t) '(1 2 3)))
  (should (equal (cl-remove-duplicates [1 2 1 3]) [2 1 3]))
  (should (equal (cl-remove-duplicates "abcabc") "abc")))

(ert-deftest cl-remove-duplicates-key ()
  ":key extracts the comparison value; default keeps last per key."
  (should (equal (cl-remove-duplicates '((1 . a) (1 . b) (2 . c)) :key #'car)
                 '((1 . b) (2 . c))))
  (should (equal (cl-remove-duplicates '((1 . a) (1 . b) (2 . c)) :key #'car :from-end t)
                 '((1 . a) (2 . c))))
  (should (equal (cl-remove-duplicates '((1 . a) (2 . b) (1 . c) (2 . d)) :key #'car :test #'=)
                 '((1 . c) (2 . d))))
  ;; cl-delete-duplicates shares the same semantics.
  (should (equal (cl-delete-duplicates (list 1 2 1 3 2)) '(1 3 2))))

(ert-deftest cl-union-order-and-key ()
  "Union prepends non-dup items of the shorter list onto the longer; without
keys non-numbers compare by `eq' (memq), so distinct strings both survive."
  (should (equal (cl-union '(1 2 3) '(3 4 5)) '(5 4 1 2 3)))
  (should (equal (cl-union '("a") '("a" "b")) '("a" "a" "b")))
  (should (equal (cl-union '("a") '("a" "b") :test #'equal) '("a" "b")))
  (should (equal (cl-union '((1 . a)) '((1 . b) (2 . c)) :key #'car) '((1 . b) (2 . c))))
  (should (equal (cl-union nil '(1 2)) '(1 2)))
  (should (equal (cl-union '(1 2) nil) '(1 2)))
  (should (equal (cl-union '(1 2) '(1 2)) '(1 2))))

(ert-deftest cl-intersection-order-and-key ()
  "Intersection returns matching elements from the shorter list, in push order."
  (should (equal (cl-intersection '(1 2 3) '(2 3 4)) '(3 2)))
  (should (equal (cl-intersection '(1 2 3 4 5) '(2 4)) '(4 2)))
  (should (equal (cl-intersection '((1 . a) (3 . x)) '((1 . b) (2 . c)) :key #'car)
                 '((1 . b))))
  (should (equal (cl-intersection '("a" "b") '("b" "c") :test #'equal) '("b")))
  (should-not (cl-intersection nil '(1 2))))

(ert-deftest cl-set-difference-order-and-key ()
  "Set-difference keeps LIST1 elements absent from LIST2, in original order."
  (should (equal (cl-set-difference '(1 2 3 4) '(2 4)) '(1 3)))
  (should (equal (cl-set-difference '(1 2 3 4 5) '(2 4 6)) '(1 3 5)))
  (should (equal (cl-set-difference '((1 . a) (3 . x)) '((1 . b)) :key #'car)
                 '((3 . x))))
  (should (equal (cl-set-difference '("a" "b") '("b") :test #'equal) '("a")))
  (should (equal (cl-set-difference '(1 2 3) nil) '(1 2 3))))

(ert-deftest seq-group-by-order ()
  "Groups appear in reverse first-encounter order with items in forward order,
matching Emacs' fold-over-reversed-sequence implementation."
  (should (equal (seq-group-by (lambda (x) (= 0 (mod x 2))) '(1 2 3 4 5))
                 '((t 2 4) (nil 1 3 5))))
  (should (equal (seq-group-by (lambda (x) (mod x 3)) '(1 2 3 4 5 6 7))
                 '((2 2 5) (0 3 6) (1 1 4 7))))
  (should (equal (seq-group-by #'car '((a . 1) (b . 2) (a . 3)))
                 '((b (b . 2)) (a (a . 1) (a . 3)))))
  (should-not (seq-group-by #'identity '())))

(ert-run-tests-batch-and-exit)
