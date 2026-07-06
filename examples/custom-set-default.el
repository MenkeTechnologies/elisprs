;;; custom-set-default.el --- faithful `custom-set-default' (custom.el:732)  -*- lexical-binding: nil; -*-

;; Regression gate for `custom-set-default', the default `:set' function for a
;; customizable variable. `custom-set-variables' and Customize call it (or the
;; per-variable `custom-set' property) to write a variable's value at load time,
;; so any init file that runs `custom-set-variables' hits it. Before this port,
;; loading such a file failed with
;; "Symbol's function definition is void: custom-set-default".
;;
;; Faithful body (custom.el:732):
;;   (if custom-local-buffer
;;       (with-current-buffer custom-local-buffer
;;         (set variable value))
;;     (set-default-toplevel-value variable value))
;;
;; Every asserted value below is what the real Emacs 30.2 binary reports
;; (emacs -Q --batch). ERT batch runner exits non-zero on any failure.
(message "== custom-set-default demo ==")

(ert-deftest custom-set-default-sets-default-value ()
  "With `custom-local-buffer' nil, the top-level default value is written.
Real Emacs: after (custom-set-default 'v 42), (default-value 'v) => 42."
  (defvar csd-plain 0)
  (custom-set-default 'csd-plain 42)
  (should (eq (default-value 'csd-plain) 42))
  (should (eq (symbol-value 'csd-plain) 42)))

(ert-deftest custom-set-default-returns-nil-via-toplevel ()
  "`custom-set-default' tail-calls the C subr `set-default-toplevel-value',
which returns nil (not the value it stored).
Real Emacs: (custom-set-default 'v 7) => nil, and (default-value 'v) => 7."
  (defvar csd-ret 0)
  (should (eq (custom-set-default 'csd-ret 7) nil))
  (should (eq (default-value 'csd-ret) 7)))

(ert-deftest custom-set-default-honors-custom-set-property ()
  "`custom-set-variables' dispatches through the per-symbol `custom-set'
property, defaulting to `custom-set-default'; verify the default path writes
the default value the way `custom-set-variables' relies on.
Real Emacs: (get 'v 'custom-set) => nil, so the default `#'custom-set-default'
is used and (default-value 'v) => 5."
  (defvar csd-prop 0)
  (should (eq (get 'csd-prop 'custom-set) nil))
  (funcall (or (get 'csd-prop 'custom-set) #'custom-set-default) 'csd-prop 5)
  (should (eq (default-value 'csd-prop) 5)))

(ert-deftest custom-set-default-local-buffer-branch ()
  "With `custom-local-buffer' bound to a buffer, the write happens via
`with-current-buffer' + `set' rather than the top-level default. For a
non-buffer-local variable `set' still updates the global cell, so both the
buffer view and the default reflect the new value.
Real Emacs: default and in-buffer value both => 99."
  (defvar csd-lb 1)
  (let ((buf (get-buffer-create "csd-cb")))
    (let ((custom-local-buffer buf))
      (custom-set-default 'csd-lb 99))
    (should (eq (default-value 'csd-lb) 99))
    (should (eq (with-current-buffer buf csd-lb) 99))))

(ert-deftest custom-set-default-local-buffer-flag-is-permanent-local ()
  "`custom-local-buffer' carries the `permanent-local' property (custom.el).
Real Emacs: (get 'custom-local-buffer 'permanent-local) => t; default nil."
  (should (eq (get 'custom-local-buffer 'permanent-local) t))
  (should (eq (default-value 'custom-local-buffer) nil)))

(ert-run-tests-batch-and-exit)
;;; custom-set-default.el ends here
