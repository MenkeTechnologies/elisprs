;;; map-plist.el --- map.el plist-shaped lists + string-truncate-left, ERT-tested vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; Emacs map.el treats a list whose first element is an atom as a plist
;; (KEY VALUE KEY VALUE...), not an alist -- see `map--plist-p'. Plist lookups
;; default to `eq' (plist-member's default) while alist lookups default to
;; `equal'. Every expected value below was captured from real
;; `emacs -Q --batch' 30.2.
(message "== map plist + string-truncate-left demo ==")

(ert-deftest map-plist-read-ops ()
  "map-elt/keys/values/pairs/length/contains-key over a plist-shaped list."
  ;; A plain list of atoms is a plist: integer key 1 -> value 2.
  (should (equal (map-elt '(1 2 3) 1) 2))
  (should (equal (map-elt '(:a 1 :b 2) :b) 2))
  (should (equal (map-elt '(:a 1 :b 2) :z 'nf) 'nf))
  (should (equal (map-keys '(:a 1 :b 2)) '(:a :b)))
  (should (equal (map-values '(:a 1 :b 2)) '(1 2)))
  (should (equal (map-pairs '(:a 1 :b 2)) '((:a . 1) (:b . 2))))
  ;; Length counts pairs, not cells.
  (should (equal (map-length '(:a 1 :b 2)) 2))
  ;; map-contains-key returns the plist tail (truthy), not `t'.
  (should (equal (map-contains-key '(:a 1 :b 2) :b) '(:b 2)))
  (should-not (map-contains-key '(:a 1 :b 2) :z)))

(ert-deftest map-plist-higher-order ()
  "map-apply/filter/some/every-p/do/nested-elt fold over pairs."
  (should (equal (map-apply #'cons '(:a 1 :b 2)) '((:a . 1) (:b . 2))))
  (should (equal (map-filter (lambda (_k v) (> v 1)) '(:a 1 :b 2 :c 3))
                 '((:b . 2) (:c . 3))))
  (should (equal (map-some (lambda (_k v) (and (> v 1) v)) '(:a 1 :b 2)) 2))
  (should (map-every-p (lambda (_k v) (> v 0)) '(:a 1 :b 2)))
  (should (equal (map-nested-elt '(:a (:b (:c 42))) '(:a :b :c)) 42))
  (should-not (map-do #'ignore '(:a 1 :b 2))))

(ert-deftest map-plist-write-ops ()
  "map-delete/map-insert plus setf on map-elt over plists."
  (should (equal (map-delete '(:a 1 :b 2 :c 3) :b) '(:a 1 :c 3)))
  (should (equal (map-insert '(:a 1) :b 2) '(:b 2 :a 1)))
  ;; setf on an existing key mutates in place; a new key appends at the tail.
  (should (equal (let ((m (list :a 1 :b 2))) (setf (map-elt m :b) 99) m)
                 '(:a 1 :b 99)))
  (should (equal (let ((m (list :a 1 :b 2))) (setf (map-elt m :c) 3) m)
                 '(:a 1 :b 2 :c 3))))

(ert-deftest map-alist-still-alist ()
  "Lists whose car is a cons keep alist semantics (default test `equal')."
  (should (equal (map-elt '((a . 1) (b . 2)) 'b) 2))
  (should (equal (map-keys '((a . 1) (b . 2))) '(a b)))
  (should (equal (map-length '((a . 1) (b . 2))) 2))
  (should (equal (map-contains-key '((a . 1)) 'a) t)))

(ert-deftest string-truncate-left-ellipsis ()
  "\"...\" is always prepended when truncating, keeping the rightmost chars;
the result may exceed LENGTH when LENGTH is 3 or smaller."
  (should (equal (string-truncate-left "hello world" 5) "...ld"))
  (should (equal (string-truncate-left "hello world" 8) "...world"))
  (should (equal (string-truncate-left "hello" 10) "hello"))
  (should (equal (string-truncate-left "abcdef" 3) "...f"))
  (should (equal (string-truncate-left "ab" 0) "...b")))

(ert-run-tests-batch-and-exit)
