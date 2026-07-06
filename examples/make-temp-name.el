;;; make-temp-name.el --- ERT tests for `make-temp-name' -*- lexical-binding: t; -*-

;; Faithful port of Fmake_temp_name (fileio.c), which delegates to
;; make-temp-file-internal / gnulib gen_tempname in GT_NOCREATE mode:
;; PREFIX concatenated with 6 random base62 chars ("a..zA..Z0..9"),
;; retried until the name has no existing file.  The suffix is random, so
;; these assertions are structural (shape/charset/contract), not literal
;; values.  Every `should' below was checked against GNU Emacs 30.2.

(require 'ert)

(ert-deftest make-temp-name-shape ()
  "Result is PREFIX plus a 6-character suffix."
  (let* ((prefix "/tmp/elisprs-mtn-")
         (s (make-temp-name prefix)))
    (should (stringp s))
    (should (string-prefix-p prefix s))
    (should (= (length s) (+ (length prefix) 6)))))

(ert-deftest make-temp-name-charset-base62 ()
  "Every suffix char is a letter or digit (gnulib base62 alphabet)."
  (let* ((s (make-temp-name "X"))
         (suffix (substring s 1)))
    (should (= (length suffix) 6))
    (dotimes (i (length suffix))
      (let ((c (aref suffix i)))
        (should (or (and (>= c ?a) (<= c ?z))
                    (and (>= c ?A) (<= c ?Z))
                    (and (>= c ?0) (<= c ?9))))))))

(ert-deftest make-temp-name-distinct ()
  "Successive calls yield different names (random suffix)."
  (should-not (equal (make-temp-name "P") (make-temp-name "P"))))

(ert-deftest make-temp-name-nonexistent ()
  "The chosen name does not already exist on disk."
  (let ((s (make-temp-name (expand-file-name "elisprs-mtn-" temporary-file-directory))))
    (should-not (file-exists-p s))
    (should-not (file-symlink-p s))))

(ert-deftest make-temp-name-missing-dir-ok ()
  "A prefix in a nonexistent directory still returns a name (no error);
lstat of a name under a missing dir is ENOENT, so the first try wins."
  (let ((s (make-temp-name "/no-such-dir-elisprs-xyz/foo")))
    (should (string-prefix-p "/no-such-dir-elisprs-xyz/foo" s))
    (should (= (length s) (+ (length "/no-such-dir-elisprs-xyz/foo") 6)))))

(ert-deftest make-temp-name-wrong-type ()
  "A non-string PREFIX signals wrong-type-argument stringp."
  (should (equal (condition-case e (make-temp-name 42)
                   (wrong-type-argument (cdr e)))
                 '(stringp 42))))

(ert-run-tests-batch-and-exit)
;;; make-temp-name.el ends here
