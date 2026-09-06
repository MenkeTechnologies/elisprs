;;; entry.el --- report the entry-point state a fuzz run will observe  -*- lexical-binding: t; -*-

;; The ORACLE IS THE ENTRY POINT, not the binary.  `emacs -Q --batch -l FILE'
;; and `emacs --script FILE' are the same executable and answer differently:
;;
;;   emacs -Q --batch -l probe.el  => buffer "*scratch*", (char-syntax ?.) = 95
;;   emacs --script    probe.el    => buffer " *load*",   (char-syntax ?.) = 46
;;
;; Every `char-syntax' / `\sC' / `skip-syntax-*' / `forward-sexp' answer in the
;; corpus comes from the current buffer's syntax table, so a run whose two
;; columns entered through different doors reports a wall of syntax divergences
;; that are not bugs — or, worse, reports parity because two different-but-wrong
;; things happened to agree.  `fuzz_parity.sh' runs this file through the exact
;; argv it will use for the corpus, on both engines, and refuses to start if the
;; two do not match.
;;
;; Line 1 is the gated state, printed with `prin1' so the comparison is plain
;; string equality.  Line 2 is unGATED context — things that legitimately differ
;; and are printed so a human reading the header can see them.

(setq print-escape-newlines t)

(prin1 (list (cons 'buffer (buffer-name))
             ;; A spread of classes rather than one character: `?.' alone
             ;; separates the two Emacs entry points, but `?$' and `?{' catch a
             ;; table that is neither (e.g. a cache hit that skipped the
             ;; prelude's `set-syntax-table' and left `standard-syntax-table').
             (cons 'char-syntax (mapcar #'char-syntax '(?. ?\; ?$ ?{ ?@ ?- ?_ ?a)))
             (cons 'case-fold case-fold-search)
             (cons 'noninteractive (and noninteractive t))))
(terpri)

;; Context, never gated: `major-mode' is not part of the syntax contract, and
;; elisprs does not port the `prog-mode' -> `lisp-data-mode' -> `emacs-lisp-mode'
;; -> `lisp-interaction-mode' chain, only the syntax table that chain installs.
(princ (format "%S %S %s\n" major-mode (default-value 'major-mode) emacs-version))

;;; entry.el ends here
