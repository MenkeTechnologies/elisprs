;;; drive.el --- evaluate a fuzz corpus and print one result line per form  -*- lexical-binding: t; -*-

;; Run identically under `emacs -Q --batch -l drive.el' (ground truth) and
;; `elisp drive.el' (subject). For each corpus line it prints
;;
;;   INDEX<TAB>=VALUE      ; `prin1' of the value the form evaluated to
;;   INDEX<TAB>!ERROR      ; `prin1' of the error object the form signalled
;;
;; so `fuzz_parity.sh' can compare the two engines line by line. Errors are part
;; of the comparison on purpose: Emacs's error symbol and error data are as much
;; of the contract as the return value.
;;
;;   FUZZ_CORPUS  path to the corpus (one form per line). Required.
;;   FUZZ_START   first index to evaluate (default 0).
;;   FUZZ_COUNT   how many to evaluate, 0 = to end (default 0).
;;
;; START/COUNT exist so the orchestrator can re-run a single form in its own
;; process after a crash or a hang killed the batch run.

;; elisprs preloads its prelude (cl-lib / seq / subr-x live in src/prelude.rs), so
;; a bare `emacs -Q --batch' would answer `void-function' to every `cl-*' form and
;; drown the report in fake divergences. Load them on the Emacs side to compare
;; like with like.
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defvar fz-max-print 400
  "Result strings longer than this are clipped — a diff nobody can read is a
diff nobody fixes, and the first 400 characters always contain the divergence
for the shapes this corpus generates.")

(defun fz-clip (s)
  (if (> (length s) fz-max-print)
      (concat (substring s 0 fz-max-print) "...<clipped>")
    s))

(defun fz-lines (path)
  (with-temp-buffer
    (insert-file-contents path)
    (split-string (buffer-string) "\n" t)))

;; One result per line is the comparison contract: a string value containing a
;; newline would otherwise print literally and desynchronize the two outputs.
;; Both engines honour this, so it changes the expected value on both sides
;; equally — it hides no divergence.
(setq print-escape-newlines t)

(let* ((corpus (or (getenv "FUZZ_CORPUS") (error "FUZZ_CORPUS unset")))
       (start (string-to-number (or (getenv "FUZZ_START") "0")))
       (count (string-to-number (or (getenv "FUZZ_COUNT") "0")))
       (end (if (> count 0) (+ start count) most-positive-fixnum))
       (i 0))
  (dolist (line (fz-lines corpus))
    (when (and (>= i start) (< i end))
      (let ((out (condition-case e
                     (concat "=" (prin1-to-string (eval (car (read-from-string line)) t)))
                   (error (concat "!" (prin1-to-string e))))))
        (princ (format "%d\t%s\n" i (fz-clip out)))))
    (setq i (1+ i))))

;;; drive.el ends here
