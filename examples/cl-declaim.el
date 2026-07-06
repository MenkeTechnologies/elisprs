;;; cl-declaim.el --- cl-lib global declarations, ERT-tested  -*- lexical-binding: t; -*-

;; A faithful port of the cl-lib global-declaration machinery:
;;   `cl-proclaim'    (cl-lib.el:254) records a declaration spec, dispatching to
;;                    `cl--do-proclaim' when it is defined (else deferring).
;;   `cl-declaim'     (cl-lib.el:260) wraps unquoted specs in `(cl-proclaim '..)'
;;                    forms; outside the byte-compiler it expands to a `progn'.
;;   `cl--do-proclaim'(cl-macs.el:2648) interprets `optimize'/`special'/`inline'/
;;                    `notinline'/`warn' specs.  elisprs has no byte-compiler, so
;;                    only the `optimize' levels (`cl--optimize-speed'/`-safety')
;;                    and the `inline' byte-optimizer property have runtime effect.
;;   `macroexp-compiling-p' (macroexp.el:141) is nil in the interpreter.
;;
;; Real init files reach this via `(cl-declaim (optimize (speed 3) (safety 0)))'
;; near the top of eieio-core, ede, auth-source, gnus, and many others; before
;; this port `(cl-declaim ...)' fell through to a function call and errored with
;; "Symbol's function definition is void: speed".  cl-macs is always present in
;; elisprs (`features' lists it), so `cl--do-proclaim' is fbound and the specs
;; take effect immediately, matching `emacs -Q --batch' with cl-macs loaded.
;; Every value asserted here was checked against `emacs -Q --batch' (GNU Emacs 30.2).

(message "== cl-declaim demo ==")

(ert-deftest cl-declaim-optimize-no-error ()
  "The canonical init-file form loads without error and returns nil."
  (should (eq (cl-declaim (optimize (speed 3) (safety 0))) nil)))

(ert-deftest cl-proclaim-optimize-sets-levels ()
  "`(optimize (speed N) (safety M))' records N/M in the cl--optimize-* vars,
and mirrors the byte-compiler flags `byte-optimize'/`byte-compile-delete-errors'."
  (cl-proclaim '(optimize (speed 2) (safety 3)))
  (should (= cl--optimize-speed 2))
  (should (= cl--optimize-safety 3))
  (should (eq byte-optimize t))
  (should (eq byte-compile-delete-errors nil))
  (cl-proclaim '(optimize (speed 0) (safety 0)))
  (should (= cl--optimize-speed 0))
  (should (= cl--optimize-safety 0))
  (should (eq byte-optimize nil))
  (should (eq byte-compile-delete-errors t)))

(ert-deftest cl-proclaim-returns-nil ()
  "`cl-proclaim' always returns nil, whatever the spec."
  (should (eq (cl-proclaim '(optimize (speed 1))) nil))
  (should (eq (cl-proclaim '(special cl-declaim-test--dynvar)) nil)))

(ert-deftest cl-proclaim-inline-notinline-property ()
  "`(inline F)' installs the inline byte-optimizer property; `(notinline F)'
removes it again."
  (let ((sym (make-symbol "cl-declaim-test--fn")))
    (should (eq (get sym 'byte-optimizer) nil))
    (cl-proclaim (list 'inline sym))
    (should (eq (get sym 'byte-optimizer) 'byte-compile-inline-expand))
    (cl-proclaim (list 'notinline sym))
    (should (eq (get sym 'byte-optimizer) nil))))

(ert-deftest cl-declaim-expands-to-progn ()
  "Outside the compiler, `cl-declaim' wraps each spec in a quoted `cl-proclaim'
call inside a `progn' (cl-lib.el:264)."
  (should (equal (macroexpand-1 '(cl-declaim (optimize (speed 3)) (special v1 v2)))
                 '(progn (cl-proclaim '(optimize (speed 3)))
                         (cl-proclaim '(special v1 v2)))))
  (should (equal (macroexpand-1 '(cl-declaim)) '(progn))))

(ert-deftest macroexp-compiling-p-nil-in-interpreter ()
  "`macroexp-compiling-p' is nil when not expanding for the byte-compiler."
  (should (eq (macroexp-compiling-p) nil)))

(ert-deftest cl-declaim-multiple-specs-run ()
  "A multi-spec `cl-declaim' runs every `cl-proclaim' in order."
  (cl-declaim (optimize (speed 1) (safety 1)) (optimize (speed 3) (safety 0)))
  ;; Last optimize spec wins.
  (should (= cl--optimize-speed 3))
  (should (= cl--optimize-safety 0)))

(ert-run-tests-batch-and-exit)
