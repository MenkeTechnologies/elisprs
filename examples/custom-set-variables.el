;;; custom-set-variables.el --- faithful `custom-set-variables' (custom.el:1001)  -*- lexical-binding: nil; -*-

;; Regression gate for `custom-set-variables' / `custom-theme-set-variables'
;; (custom.el, Emacs 30.2). This is the form a user's Customize block generates
;; and writes verbatim into an init file, e.g.
;;   (custom-set-variables
;;    '(inhibit-startup-screen t)
;;    '(tab-width 8))
;; Before this port, loading any such init file failed with
;; "Symbol's function definition is void: custom-set-variables".
;;
;; `custom-set-variables' delegates to `(custom-theme-set-variables 'user ...)',
;; which for each (SYMBOL EXP [NOW [REQUEST [COMMENT]]]) entry:
;;   * records EXP unevaluated in SYMBOL's `saved-value' property,
;;   * registers it under theme `user' via `custom-push-theme'
;;     (SYMBOL's `theme-value' + `user's `theme-settings'),
;;   * calls the `custom-set' setter (default `custom-set-default') with the
;;     evaluated EXP, but only if NOW is set or the var already has a default.
;;
;; Every asserted value below is what the real Emacs 30.2 binary reports
;; (emacs -Q --batch). ERT batch runner exits non-zero on any failure.
(message "== custom-set-variables demo ==")

(ert-deftest csv-sets-bound-variable-and-records-theme ()
  "A pre-bound variable is set to the evaluated EXP, and the setting is
registered under theme `user'.
Real Emacs: after (custom-set-variables '(csv-a 8)) with csv-a defvar'd to 0,
csv-a => 8, (get 'csv-a 'saved-value) => (8),
(get 'csv-a 'theme-value) => ((user 8) (changed 0)) -- the pre-existing value
is stashed under a fake `changed' theme so it can be restored on theme disable."
  (defvar csv-a 0)
  (custom-set-variables '(csv-a 8))
  (should (eq (default-value 'csv-a) 8))
  (should (equal (get 'csv-a 'saved-value) '(8)))
  (should (equal (get 'csv-a 'theme-value) '((user 8) (changed 0)))))

(ert-deftest csv-boolean-t-literal-stays-t ()
  "The most common Customize form sets a flag to the constant `t'. The
evaluated value must be `t' (eq to t), not a truthy stand-in.
Real Emacs: after (custom-set-variables '(csv-flag t)), csv-flag => t."
  (defvar csv-flag nil)
  (custom-set-variables '(csv-flag t))
  (should (eq (default-value 'csv-flag) t))
  (should (equal (get 'csv-flag 'saved-value) '(t)))
  (should (equal (get 'csv-flag 'theme-value) '((user t) (changed nil)))))

(ert-deftest csv-skips-unbound-without-now ()
  "Without a NOW flag, an entry whose variable has no default binding is
recorded but NOT set (custom.el only funcalls the setter when
`(default-boundp symbol)' or NOW).
Real Emacs: csv-unbound stays void as a value, but its theme-value is recorded."
  (should-not (default-boundp 'csv-unbound))
  (custom-set-variables '(csv-unbound 5))
  (should-not (default-boundp 'csv-unbound))
  (should (equal (get 'csv-unbound 'theme-value) '((user 5)))))

(ert-deftest csv-now-flag-forces-rogue-variable ()
  "With NOW non-nil the variable is force-set even without a prior default,
and `force-value' is stamped.
Real Emacs: after (custom-set-variables '(csv-rogue 42 t)),
csv-rogue => 42 and (get 'csv-rogue 'force-value) => t."
  (custom-set-variables '(csv-rogue 42 t))
  (should (eq (default-value 'csv-rogue) 42))
  (should (eq (get 'csv-rogue 'force-value) t)))

(ert-deftest csv-honors-per-symbol-custom-set ()
  "A per-variable `custom-set' property overrides the default setter.
Real Emacs: the custom setter is invoked with (SYMBOL EVALUATED-VALUE)."
  (defvar csv-setter-log nil)
  (defun csv-my-setter (sym val)
    (push (cons sym val) csv-setter-log)
    (set-default-toplevel-value sym val))
  (defvar csv-hooked nil)
  (put 'csv-hooked 'custom-set 'csv-my-setter)
  (custom-set-variables '(csv-hooked 99))
  (should (equal csv-setter-log '((csv-hooked . 99))))
  (should (eq (default-value 'csv-hooked) 99)))

(ert-deftest csv-registers-under-user-theme-settings ()
  "Each set variable is pushed onto theme `user's `theme-settings' as a
(theme-value SYMBOL user VALUE) entry.
Real Emacs: after setting csv-ts, `user's theme-settings contains an entry
whose head is `theme-value' and whose symbol is csv-ts."
  (defvar csv-ts 0)
  (custom-set-variables '(csv-ts 3))
  (let ((entry (assq 'csv-ts
                     (mapcar (lambda (s) (cons (nth 1 s) s))
                             (get 'user 'theme-settings)))))
    (should entry)
    (should (eq (nth 1 (cdr entry)) 'csv-ts))
    (should (eq (car (cdr entry)) 'theme-value))))

(ert-run-tests-batch-and-exit)
;;; custom-set-variables.el ends here
