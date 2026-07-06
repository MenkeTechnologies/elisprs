;;; custom-initialize-delay.el --- custom-initialize-delay port, self-checked  -*- lexical-binding: nil; -*-
;; Run me:  elisp examples/custom-initialize-delay.el
;;
;; Regression gate for the faithful port of `custom-initialize-delay'
;; (custom.el:141) and its companion `defvar custom-delayed-init-variables'
;; (custom.el:137). This is the `:initialize' function preloaded/autoloaded
;; options use so their init runs in the run-time (not build-time) context.
;;
;; Two code paths, both captured from the real Emacs 30.2 binary
;; (lexical-binding nil, matching elisprs's dynamic-binding milestone):
;;
;;   PATH A (`custom-delayed-init-variables' is a list): the symbol is defvar'd
;;   (marked special) but LEFT UNBOUND, and pushed onto the delayed list.
;;     emacs -Q --batch --eval '(let ((custom-delayed-init-variables (list)))
;;       (custom-initialize-delay (quote v) 42)
;;       (princ (list (memq (quote v) custom-delayed-init-variables)
;;                    (boundp (quote v)) (special-variable-p (quote v)))))'
;;       =>  ((v) nil t)
;;
;;   PATH B (`custom-delayed-init-variables' is a non-list, e.g. t — this is the
;;   post-startup state, bug#47072): there is no "later" to delay to, so it
;;   initializes "normally" via `custom-initialize-reset', binding the value.
;;     emacs -Q --batch --eval '(progn (setq custom-delayed-init-variables t)
;;       (custom-initialize-delay (quote w) 99) (princ (symbol-value (quote w))))'
;;       =>  99
;;
;; ERT batch runner errors (→ non-zero exit) on any failure, so this doubles as
;; the CI self-test.
(message "== custom-initialize-delay demo ==")

(ert-deftest cid-delayed-var-defaults-to-list ()
  "`custom-delayed-init-variables' is defined and defaults to the empty list.
Emacs: (boundp 'custom-delayed-init-variables) => t, value => nil,
       (special-variable-p 'custom-delayed-init-variables) => t."
  (should (boundp 'custom-delayed-init-variables))
  (should (special-variable-p 'custom-delayed-init-variables)))

(ert-deftest cid-list-path-pushes-and-leaves-unbound ()
  "PATH A: with a live list, delay pushes the symbol and leaves it unbound.
Emacs: after (custom-initialize-delay 'cid-a 42) under a list binding,
       (memq 'cid-a custom-delayed-init-variables) => (cid-a ...),
       (boundp 'cid-a) => nil, (special-variable-p 'cid-a) => t."
  (let ((custom-delayed-init-variables (list 'cid-pre)))
    (custom-initialize-delay 'cid-a 42)
    (should (memq 'cid-a custom-delayed-init-variables))
    ;; The pre-existing member is preserved (push, not replace).
    (should (memq 'cid-pre custom-delayed-init-variables))
    (should-not (boundp 'cid-a))
    ;; Still marked special (defvar'd) even though unbound.
    (should (eq (special-variable-p 'cid-a) t))))

(ert-deftest cid-nonlist-path-initializes-normally ()
  "PATH B: with a non-list value, delay falls back to `custom-initialize-reset'.
Emacs: after (setq custom-delayed-init-variables t) then
       (custom-initialize-delay 'cid-b 99), (boundp 'cid-b) => t,
       cid-b => 99, (special-variable-p 'cid-b) => t."
  (let ((custom-delayed-init-variables t))
    (custom-initialize-delay 'cid-b 99)
    (should (boundp 'cid-b))
    (should (= (symbol-value 'cid-b) 99))
    (should (eq (special-variable-p 'cid-b) t))))

(ert-deftest cid-defcustom-uses-delay-initialize ()
  "A `defcustom' with `:initialize #'custom-initialize-delay' obeys PATH A:
the option records `standard-value' but stays unbound while the list is live.
Emacs: (custom-declare-variable 'cid-opt 8 \"d\" :type 'integer
          :initialize #'custom-initialize-delay) under a list binding =>
       (boundp 'cid-opt) => nil, (get 'cid-opt 'standard-value) => (8),
       (memq 'cid-opt custom-delayed-init-variables) => (cid-opt)."
  (let ((custom-delayed-init-variables (list)))
    (custom-declare-variable 'cid-opt 8 "An integer option."
                             :type 'integer
                             :initialize #'custom-initialize-delay)
    (should-not (boundp 'cid-opt))
    (should (equal (get 'cid-opt 'standard-value) '(8)))
    (should (memq 'cid-opt custom-delayed-init-variables))))

(ert-run-tests-batch-and-exit)

;;; custom-initialize-delay.el ends here
