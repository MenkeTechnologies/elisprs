;;; text-properties.el --- string & buffer text properties  -*- lexical-binding: nil; -*-

;; Exercises text properties on strings and buffer text: propertize, put/get/set/
;; add/remove-text-property, text-properties-at, next-single-property-change,
;; next-property-change, previous-single-property-change, get-char-property, and
;; buffer-substring/buffer-string carrying (or dropping) properties. Every asserted
;; value was checked against GNU Emacs 30.2 (emacs -Q --batch).
(message "== text-properties demo ==")

(ert-deftest tp-propertize-string ()
  "propertize returns a string carrying the given properties; text-properties-at
returns them in the supplied order."
  (should (equal (let ((s (propertize "hi" 'face 'bold)))
                   (list (get-text-property 0 'face s) (get-text-property 1 'face s)))
                 '(bold bold)))
  (should (equal (let ((s (propertize "hi" 'a 1 'b 2)))
                   (list (text-properties-at 0 s) (get-text-property 0 'b s)))
                 '((a 1 b 2) 2)))
  ;; An index equal to the length reads nil (no char there), not an error.
  (should (equal (get-text-property 2 'face "hi") nil))
  ;; put-text-property mutates a string in place.
  (should (equal (let ((s (copy-sequence "hi")))
                   (put-text-property 0 2 'a 1 s)
                   (list (get-text-property 0 'a s) (get-text-property 1 'a s)))
                 '(1 1))))

(ert-deftest tp-buffer-put-get ()
  "put-text-property on buffer text; get-text-property reads it, with point-max
yielding nil."
  (should (equal (with-temp-buffer
                   (insert "hello world")
                   (put-text-property 1 6 'face 'bold)
                   (list (get-text-property 1 'face) (get-text-property 5 'face)
                         (get-text-property 6 'face) (get-text-property 3 'missing)))
                 '(bold bold nil nil)))
  ;; A new property is prepended; an existing key keeps its position (Emacs order).
  (should (equal (with-temp-buffer
                   (insert "xx")
                   (put-text-property 1 3 'a 1)
                   (put-text-property 1 3 'b 2)
                   (put-text-property 1 3 'a 9)
                   (text-properties-at 1))
                 '(b 2 a 9))))

(ert-deftest tp-set-add-remove ()
  "set-text-properties replaces, add-text-properties merges, remove-text-properties
drops."
  (should (equal (with-temp-buffer
                   (insert "abcde")
                   (put-text-property 2 4 'p 7)
                   (set-text-properties 1 6 nil)
                   (get-text-property 3 'p))
                 nil))
  (should (equal (with-temp-buffer
                   (insert "hello")
                   (put-text-property 1 3 'face 'bold)
                   (add-text-properties 1 3 '(weight heavy))
                   (list (get-text-property 1 'face) (get-text-property 1 'weight)))
                 '(bold heavy)))
  (should (equal (with-temp-buffer
                   (insert "hello")
                   (put-text-property 1 4 'face 'bold)
                   (remove-text-properties 1 4 '(face nil))
                   (get-text-property 1 'face))
                 nil)))

(ert-deftest tp-property-change-scan ()
  "next-single-property-change / next-property-change / previous-single-property-
change locate interval boundaries; LIMIT is honored."
  (should (equal (with-temp-buffer
                   (insert "hello world")
                   (put-text-property 1 6 'face 'bold)
                   (list (next-single-property-change 1 'face)
                         (next-single-property-change 3 'face)
                         (next-single-property-change 6 'face)
                         (next-property-change 1)))
                 '(6 6 nil 6)))
  (should (equal (with-temp-buffer
                   (insert "hello")
                   (put-text-property 1 3 'face 'bold)
                   (list (next-single-property-change 1 'face nil 10)
                         (next-single-property-change 3 'face nil 2)
                         (get-char-property 1 'face)))
                 '(3 2 bold)))
  (should (equal (with-temp-buffer
                   (insert "hello")
                   (put-text-property 1 3 'face 'bold)
                   (previous-single-property-change 5 'face))
                 3))
  ;; No previous change at the very start of the object.
  (should (equal (with-temp-buffer
                   (insert "hi")
                   (put-text-property 1 3 'x 5)
                   (previous-single-property-change 1 'x))
                 nil)))

(ert-deftest tp-buffer-substring-props ()
  "Inserting a propertized string carries its properties into the buffer;
buffer-substring keeps them and -no-properties drops them."
  (should (equal (with-temp-buffer
                   (insert (propertize "AB" 'x 1))
                   (insert "CD")
                   (list (get-text-property 1 'x)
                         (get-text-property 3 'x)
                         (get-text-property 0 'x (buffer-substring 1 3))
                         (get-text-property 0 'x (buffer-substring-no-properties 1 3))))
                 '(1 nil 1 nil))))

(ert-run-tests-batch-and-exit)
