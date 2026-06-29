;;; script-demo.el --- end-to-end scripting: file IO + buffers + json + cl/seq  -*- lexical-binding: nil; -*-

;; Exercises the standalone-scripting surface composed together the way a real
;; .el program would: write/read files, scan a buffer with regexps, summarize
;; with seq/cl, round-trip through JSON, and shell out. ERT-tested; the same file
;; passes under GNU Emacs (with cl-lib/json/subr-x/seq).
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'json)

(defvar script-demo--dir "target/script-demo")

(defun script-demo--setup ()
  (make-directory script-demo--dir t)
  (write-region "alice 30\nbob 25\ncarol 41\n" nil
                (file-name-concat script-demo--dir "people.txt")))

(defun script-demo--parse-people (file)
  "Parse \"NAME AGE\" lines into an alist of (NAME . AGE)."
  (with-temp-buffer
    (insert-file-contents file)
    (goto-char (point-min))
    (let (rows)
      (while (re-search-forward "^\\([a-z]+\\) \\([0-9]+\\)$" nil t)
        (push (cons (match-string 1) (string-to-number (match-string 2))) rows))
      (nreverse rows))))

(ert-deftest script-demo-parse-and-summarize ()
  "Read a file, parse with a buffer, summarize with seq/cl."
  (script-demo--setup)
  (let* ((people (script-demo--parse-people
                  (file-name-concat script-demo--dir "people.txt")))
         (ages (mapcar #'cdr people)))
    (should (equal (mapcar #'car people) '("alice" "bob" "carol")))
    (should (= (seq-reduce #'+ ages 0) 96))
    (should (= (seq-max ages) 41))
    (should (equal (cl-remove-if (lambda (p) (< (cdr p) 30)) people)
                   '(("alice" . 30) ("carol" . 41))))))

(ert-deftest script-demo-json-roundtrip ()
  "Serialize the summary to JSON on disk and read it back."
  (script-demo--setup)
  (let* ((people (script-demo--parse-people
                  (file-name-concat script-demo--dir "people.txt")))
         (jf (file-name-concat script-demo--dir "people.json")))
    (write-region (json-encode people) nil jf)
    (let ((back (let ((json-object-type 'alist) (json-key-type 'string))
                  (json-read-from-string
                   (with-temp-buffer (insert-file-contents jf) (buffer-string))))))
      (should (= (cdr (assoc "alice" back)) 30))
      (should (= (cdr (assoc "carol" back)) 41)))))

(ert-deftest script-demo-shell ()
  "Shell out and read the result back."
  (should (equal (string-trim (shell-command-to-string "echo elisprs")) "elisprs"))
  (should (equal (process-lines "printf" "x\ny\n") '("x" "y"))))

(ert-deftest script-demo-cleanup ()
  "Tidy the scratch files."
  (dolist (f '("people.txt" "people.json"))
    (let ((p (file-name-concat script-demo--dir f)))
      (when (file-exists-p p) (delete-file p))))
  (should t))

(ert-run-tests-batch-and-exit)
