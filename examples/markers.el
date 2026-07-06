;;; markers.el --- general marker objects  -*- lexical-binding: nil; -*-

;; Exercises first-class marker objects: make-marker / point-marker / copy-marker /
;; set-marker / marker-position / marker-buffer / markerp / marker-insertion-type.
;; A marker auto-adjusts when text is inserted or deleted before/at it, honoring its
;; insertion type; a marker also serves as a buffer position (goto-char) and coerces
;; to its position in host-dispatched arithmetic. Every asserted value was checked
;; against GNU Emacs 30.2 (emacs -Q --batch).
(message "== markers demo ==")

(ert-deftest mk-basic-predicates ()
  "A point-marker reports its position, buffer, insertion type, and markerp."
  (should (equal (with-temp-buffer
                   (insert "abc")
                   (let ((m (point-marker)))
                     (list (markerp m) (marker-position m)
                           (marker-insertion-type m) (bufferp (marker-buffer m)))))
                 '(t 4 nil t)))
  ;; A fresh make-marker points nowhere: nil position and nil buffer.
  (should (equal (let ((m (make-marker)))
                   (list (markerp m) (marker-position m) (marker-buffer m)))
                 '(t nil nil)))
  (should (equal (with-temp-buffer (insert "hi") (markerp (point-marker))) t))
  (should (equal (markerp 5) nil)))

(ert-deftest mk-auto-adjust-insert ()
  "Inserting before a marker shifts it by the inserted length."
  (should (equal (with-temp-buffer
                   (insert "abc")
                   (let ((m (point-marker)))   ; m at 4
                     (goto-char 1) (insert "XY") ; 2 chars before m
                     (marker-position m)))
                 6)))

(ert-deftest mk-insertion-type ()
  "At the insertion point, a nil-type marker stays before the text, a t-type
marker advances past it."
  (should (equal (with-temp-buffer
                   (insert "abc") (goto-char 2)
                   (let ((m (copy-marker (point) nil))
                         (n (copy-marker (point) t)))
                     (insert "XY")
                     (list (marker-position m) (marker-position n))))
                 '(2 4)))
  ;; set-marker-insertion-type flips the behavior on an existing marker.
  (should (equal (with-temp-buffer
                   (insert "abc") (goto-char 2)
                   (let ((m (copy-marker (point) nil)))
                     (set-marker-insertion-type m t)
                     (insert "XY")
                     (marker-position m)))
                 4)))

(ert-deftest mk-auto-adjust-delete ()
  "Deletion before a marker shifts it left; deletion spanning it clamps it to the
start of the deleted region."
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (let ((m (copy-marker 5)))
                     (delete-region 2 4)        ; remove 2 chars before m
                     (marker-position m)))
                 3))
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (let ((m (copy-marker 3)))
                     (delete-region 2 5)        ; m is inside the deleted span
                     (marker-position m)))
                 2)))

(ert-deftest mk-copy-and-set ()
  "copy-marker clamps an integer position to the buffer; set-marker retargets or
detaches a marker."
  (should (equal (with-temp-buffer
                   (insert "abc")
                   (list (marker-position (copy-marker))     ; nil arg -> detached
                         (marker-position (copy-marker 99))  ; clamp high -> point-max
                         (marker-position (copy-marker 0)))) ; clamp low -> point-min
                 '(nil 4 1)))
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (let ((m (copy-marker 3)))
                     (set-marker m 99)          ; clamp to point-max
                     (marker-position m)))
                 7))
  (should (equal (with-temp-buffer
                   (insert "abc")
                   (let ((m (make-marker)))
                     (set-marker m 2)
                     (set-marker m nil)         ; detach
                     (list (marker-position m) (marker-buffer m))))
                 '(nil nil))))

(ert-deftest mk-as-position-and-equal ()
  "A marker is accepted as a buffer position; two markers are `equal' when they
share a buffer and position but never `eq'."
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (let ((m (copy-marker 3)))
                     (goto-char m) (point)))
                 3))
  ;; Host-dispatched arithmetic coerces a marker to its position.
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (let ((m (copy-marker 3)))
                     (funcall #'+ m 2)))
                 5))
  (should (equal (with-temp-buffer
                   (insert "abc")
                   (let ((m (copy-marker 2)) (n (copy-marker 2)))
                     (list (eq m n) (equal m n) (type-of m))))
                 '(nil t marker))))

(ert-deftest mk-insert-before-markers ()
  "insert-before-markers relocates a marker sitting at the insertion point past
the newly inserted text."
  (should (equal (with-temp-buffer
                   (insert "abcdef") (goto-char 3)
                   (let ((m (point-marker)))
                     (insert-before-markers "XY")
                     (marker-position m)))
                 5)))

(ert-run-tests-batch-and-exit)
