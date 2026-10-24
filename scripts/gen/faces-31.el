;;; faces-31.el --- print src/lisp/faces-31.el from the oracle  -*- lexical-binding: t -*-

;; emacs -Q --batch -l scripts/gen/faces-31.el > src/lisp/faces-31.el
;;
;; For each face of `(reverse (face-list))' (face-id order), copies its
;; `defface' form verbatim from the source of `(symbol-file FACE 'defface)'.

(let ((prev nil) (missing nil))
  (princ ";;; faces-31.el --- the faces of batch Emacs, in face-id order  -*- lexical-binding: t -*-

;; Verbatim `defface' forms from GNU Emacs 31.1 lisp/faces.el and the other
;; preloaded libraries that define faces, ordered as `(face-list)' of
;; `emacs -Q --batch' orders them (each library's in file order, libraries
;; in loadup.el's order), so `face-list' and face IDs match.  Generated with
;; the oracle: for each face of `(reverse (face-list))', its `defface' form
;; from the source of `(symbol-file FACE 'defface)'.
")
  (dolist (face (reverse (face-list)))
    (let* ((elc (symbol-file face 'defface))
           (el (and elc (concat (file-name-sans-extension elc) ".el.gz")))
           (text nil))
      (when (and el (file-exists-p el))
        (with-temp-buffer
          (insert-file-contents el)
          (goto-char (point-min))
          (when (re-search-forward
                 (format "^(defface %s\\_>" (regexp-quote (symbol-name face)))
                 nil t)
            (goto-char (match-beginning 0))
            (let ((start (point)))
              (forward-sexp)
              (setq text (buffer-substring start (point)))))))
      (if (not text)
          (push face missing)
        (let ((file (file-name-nondirectory (file-name-sans-extension elc))))
          (unless (equal file prev)
            (princ (format "\n;;;; %s.el\n" file))
            (setq prev file))
          (princ (format "\n%s\n" text))))))
  (when missing (princ (format "\n;; MISSING %S\n" (nreverse missing)))))
