;;; buffer-editing.el --- text buffers, point, narrowing, markers  -*- lexical-binding: nil; -*-

;; Exercises the text-editing subsystem: a global registry of named live buffers,
;; each with its own text, 1-based point, mark, and narrowing bounds; insertion and
;; deletion that shift later positions; save-excursion (marker-based point restore
;; that tracks intervening edits); and save-restriction/narrow-to-region/widen.
;; Every asserted value was checked against GNU Emacs 30.2 (emacs -Q --batch).
(message "== buffer-editing demo ==")

(ert-deftest be-point-and-text ()
  "Point positions are 1-based; insert advances point and shifts later text."
  (should (equal (with-temp-buffer
                   (insert "hello world") (goto-char 1)
                   (list (point) (char-after) (buffer-string) (buffer-size)))
                 '(1 104 "hello world" 11)))
  (should (equal (with-temp-buffer
                   (insert "hello") (goto-char 2) (delete-char 2) (buffer-string))
                 "hlo"))
  (should (equal (with-temp-buffer
                   (insert "hello") (goto-char 3) (insert-char ?x 2)
                   (list (point) (buffer-string)))
                 '(5 "hexxllo")))
  (should (equal (with-temp-buffer
                   (insert "hello") (goto-char 3) (delete-char -2)
                   (list (point) (buffer-string)))
                 '(1 "llo")))
  (should (equal (with-temp-buffer (insert 65 66) (buffer-string)) "AB")))

(ert-deftest be-line-motion ()
  "forward-line / line-beginning-position / line-end-position over newlines."
  (should (equal (with-temp-buffer
                   (insert "abc\ndef\n") (goto-char (point-min)) (forward-line 1)
                   (list (point) (line-beginning-position) (line-end-position)))
                 '(5 5 8)))
  (should (equal (with-temp-buffer
                   (insert "line1\nline2\nline3") (goto-char 1) (forward-line 2)
                   (list (point) (bolp)))
                 '(13 t))))

(ert-deftest be-save-excursion-marker ()
  "save-excursion restores point via a marker that tracks edits made in the body."
  ;; Insert before the saved point: the restored point shifts by the inserted length.
  (should (equal (with-temp-buffer
                   (insert "abcdef")
                   (save-excursion (goto-char 1) (insert "X"))
                   (list (point) (buffer-string)))
                 '(8 "Xabcdef")))
  ;; Insert exactly at the saved point (insertion-type nil): point does not advance.
  (should (= (with-temp-buffer
               (insert "abc") (goto-char 2)
               (save-excursion (insert "XY")) (point))
             2))
  ;; Delete spanning the saved point: it collapses to the deletion start.
  (should (= (with-temp-buffer
               (insert "abcdef") (goto-char 4)
               (save-excursion (goto-char 2) (delete-region 2 5)) (point))
             2))
  ;; save-excursion returns the body's value.
  (should (= (with-temp-buffer (insert "abc") (save-excursion 42)) 42)))

(ert-deftest be-narrowing ()
  "narrow-to-region / widen / save-restriction and point-min/max under narrowing."
  (should (equal (with-temp-buffer
                   (insert "0123456789") (narrow-to-region 3 6)
                   (list (point-min) (point-max) (buffer-string) (point)))
                 '(3 6 "234" 6)))
  ;; buffer-size ignores narrowing; buffer-string honors it.
  (should (= (with-temp-buffer (insert "0123456789") (narrow-to-region 3 6)
                               (buffer-size))
             10))
  ;; Inserting inside the region extends point-max (zv is insertion-type t).
  (should (equal (with-temp-buffer
                   (insert "0123456789") (narrow-to-region 3 6)
                   (goto-char 4) (insert "XX")
                   (list (point-min) (point-max) (buffer-string)))
                 '(3 8 "2XX34")))
  ;; widen restores the full accessible range but leaves point where it was.
  (should (equal (with-temp-buffer
                   (insert "0123456789") (narrow-to-region 3 6)
                   (goto-char (point-max)) (widen)
                   (list (point-min) (point-max) (point)))
                 '(1 11 6)))
  ;; save-restriction restores narrowing (bounds track edits inside the body).
  (should (equal (with-temp-buffer
                   (insert "12345") (narrow-to-region 2 4)
                   (save-restriction (widen))
                   (list (point-min) (point-max)))
                 '(2 4)))
  (should (equal (with-temp-buffer
                   (insert "0123456789") (narrow-to-region 2 8)
                   (save-restriction (narrow-to-region 3 6) (goto-char 4) (insert "QQ"))
                   (list (point-min) (point-max) (buffer-string)))
                 '(2 10 "12QQ3456")))
  ;; erase-buffer deletes everything and removes the restriction.
  (should (equal (with-temp-buffer
                   (insert "hello") (narrow-to-region 2 4) (erase-buffer)
                   (list (buffer-string) (point-min) (point-max)))
                 '("" 1 1))))

(ert-deftest be-buffer-registry ()
  "Named live buffers: create, switch, rename, kill, and list."
  (should (equal (let ((b (generate-new-buffer "be-foo")))
                   (prog1 (list (bufferp b) (buffer-name b) (buffer-live-p b))
                     (kill-buffer b)))
                 '(t "be-foo" t)))
  ;; with-current-buffer switches the current buffer for its body.
  (should (equal (let ((b (get-buffer-create "be-bar")))
                   (with-current-buffer b (erase-buffer) (insert "hi"))
                   (prog1 (with-current-buffer b (buffer-string))
                     (kill-buffer b)))
                 "hi"))
  ;; kill-buffer makes the object non-live and drops it from get-buffer.
  (should (equal (let ((b (get-buffer-create "be-z")))
                   (kill-buffer b)
                   (list (buffer-live-p b) (get-buffer "be-z")))
                 '(nil nil)))
  ;; generate-new-buffer-name uniquifies with <N>.
  (should (equal (let ((b (get-buffer-create "be-dup")))
                   (prog1 (generate-new-buffer-name "be-dup") (kill-buffer b)))
                 "be-dup<2>"))
  ;; Each buffer keeps its own text and point.
  (should (equal (let ((a (get-buffer-create "be-A"))
                       (b (get-buffer-create "be-B")))
                   (with-current-buffer a (erase-buffer) (insert "aaa"))
                   (with-current-buffer b (erase-buffer) (insert "bbbbb"))
                   (prog1 (list (with-current-buffer a (buffer-size))
                                (with-current-buffer b (buffer-size)))
                     (kill-buffer a) (kill-buffer b)))
                 '(3 5)))
  ;; buffer-list contains the live buffers we created (order-independent check).
  (should (let ((b (get-buffer-create "be-listed")))
            (prog1 (member "be-listed" (mapcar #'buffer-name (buffer-list)))
              (kill-buffer b)))))

(ert-deftest be-mark-and-region ()
  "The mark and region-beginning/region-end."
  (should (equal (with-temp-buffer
                   (insert "abc") (set-mark 2)
                   (list (mark) (region-beginning) (region-end)))
                 '(2 2 4)))
  (should (null (with-temp-buffer (insert "abc") (mark)))))

(ert-run-tests-batch-and-exit)
