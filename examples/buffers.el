;;; buffers.el --- temp-buffer text processing + file I/O, ERT-tested  -*- lexical-binding: nil; -*-

;; A minimal editing-buffer model (text + point) backs with-temp-buffer, insert,
;; point motion, search/replace, and insert-file-contents/write-region. This is
;; the read -> process -> write pipeline a standalone .el script actually uses.
(message "== buffers demo ==")

(defun wc-lines (text)
  "Count lines in TEXT using a temp buffer."
  (with-temp-buffer
    (insert text)
    (count-lines (point-min) (point-max))))

(defun upcase-words-via-buffer (text)
  "Uppercase every run of letters in TEXT via re-search-forward/replace-match."
  (with-temp-buffer
    (insert text)
    (goto-char (point-min))
    (while (re-search-forward "[a-z]+" nil t)
      (replace-match (upcase (match-string 0)) t))
    (buffer-string)))

(defun collect-numbers (text)
  "Return all integers appearing in TEXT, in order."
  (with-temp-buffer
    (insert text)
    (goto-char (point-min))
    (let (nums)
      (while (re-search-forward "[0-9]+" nil t)
        (push (string-to-number (match-string 0)) nums))
      (nreverse nums))))

(ert-deftest buffers-count-and-transform ()
  "Line counting, regexp transform, and number extraction over a buffer."
  (should (= (wc-lines "a\nb\nc") 3))
  (should (= (wc-lines "a\nb\n") 2))
  (should (equal (upcase-words-via-buffer "foo 12 bar") "FOO 12 BAR"))
  (should (equal (collect-numbers "x1 y22 z333") '(1 22 333))))

(ert-deftest buffers-motion ()
  "Point motion: forward-line, beginning/end-of-line, word motion."
  (with-temp-buffer
    (insert "alpha\nbeta\ngamma")
    (goto-char (point-min))
    (forward-line 1)
    (should (= (point) 7))
    (should (looking-at "beta"))
    (end-of-line)
    (should (= (char-before) ?a))
    (beginning-of-line)
    (forward-word)
    (should (= (point) 11))))

(ert-deftest buffers-file-roundtrip ()
  "write-region then insert-file-contents reproduces the text."
  (let ((f "target/elp-buffers-example.txt")
        (payload "line one\nline two\n"))
    (write-region payload nil f)
    (unwind-protect
        (should (equal (with-temp-buffer (insert-file-contents f) (buffer-string))
                       payload))
      (delete-file f))))

(ert-deftest buffers-region-and-word-case ()
  "Region/word case conversion and subst-char-in-region edit in place."
  (should (equal (with-temp-buffer (insert "Hello World")
                   (upcase-region 1 6) (buffer-string))
                 "HELLO World"))
  (should (equal (with-temp-buffer (insert "hello world")
                   (capitalize-region (point-min) (point-max)) (buffer-string))
                 "Hello World"))
  (should (equal (with-temp-buffer (insert "FOO bar")
                   (goto-char 1) (downcase-word 1) (buffer-string))
                 "foo bar"))
  (should (equal (with-temp-buffer (insert "a.b.c")
                   (subst-char-in-region 1 6 ?. ?-) (buffer-string))
                 "a-b-c"))
  (should (equal (with-temp-buffer (insert "abc") (goto-char 2)
                   (transpose-chars 1) (buffer-string))
                 "bac")))

(ert-run-tests-batch-and-exit)
