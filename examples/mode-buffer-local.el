;;; mode-buffer-local.el --- buffer-local vars + mode machinery, ERT-tested  -*- lexical-binding: nil; -*-

;; The buffer-local-variable + major/minor mode subsystem: the buffer.c/data.c
;; local-binding primitives (make-local-variable, make-variable-buffer-local,
;; setq-local, local-variable-p, local-variable-if-set-p, kill-local-variable,
;; kill-all-local-variables, buffer-local-value, default-value/set-default), the
;; subr.el mode plumbing (run-mode-hooks, delay-mode-hooks, derived-mode-p,
;; derived-mode-set-parent), and the mode-definition macros define-derived-mode
;; (derived.el) and define-minor-mode (easy-mmode.el).
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Only ONE implicit buffer is modeled (batch has effectively one), so the tests
;; exercise buffer-local semantics within that buffer.  Real text editing across
;; multiple live buffers is a separate subsystem.  Run through fusevm;
;; `ert-run-tests-batch-and-exit' gates the suite.
(message "== buffer-local + mode machinery ==")

;; ---- make-local-variable: snapshots the default, shadows it ----
(ert-deftest bl-make-local-variable ()
  (defvar bl-a 10)
  (make-local-variable 'bl-a)
  (should (eq t (local-variable-p 'bl-a)))
  ;; Before setting, the local reads the snapshot of the default.
  (should (= bl-a 10))
  (setq bl-a 5)
  ;; emacs -Q: (5 10 t)
  (should (equal (list bl-a (default-value 'bl-a) (local-variable-p 'bl-a))
                 '(5 10 t)))
  ;; The local is a snapshot: a later set-default does not reach it.
  (set-default 'bl-a 20)
  (should (equal (list bl-a (default-value 'bl-a)) '(5 20))))

;; ---- kill-local-variable restores the default ----
(ert-deftest bl-kill-local-variable ()
  (defvar bl-b 10)
  (make-local-variable 'bl-b)
  (setq bl-b 5)
  (kill-local-variable 'bl-b)
  ;; emacs -Q: (10 nil)
  (should (equal (list bl-b (local-variable-p 'bl-b)) '(10 nil))))

;; ---- make-variable-buffer-local: auto-local + special ----
(ert-deftest bl-make-variable-buffer-local ()
  (defvar bl-y 1)
  (make-variable-buffer-local 'bl-y)
  ;; Marked special, not yet local, but would-become-local-if-set.
  ;; emacs -Q: (nil t t)
  (should (equal (list (local-variable-p 'bl-y)
                       (local-variable-if-set-p 'bl-y)
                       (special-variable-p 'bl-y))
                 '(nil t t)))
  ;; A plain set now creates the local, leaving the default alone.
  (set 'bl-y 7)
  ;; emacs -Q: (7 1 t)
  (should (equal (list bl-y (default-value 'bl-y) (local-variable-p 'bl-y))
                 '(7 1 t)))
  ;; A non-auto variable is neither local nor local-if-set.
  (defvar bl-z 3)
  (should (equal (list (local-variable-p 'bl-z) (local-variable-if-set-p 'bl-z))
                 '(nil nil))))

;; ---- setq-local / buffer-local-value ----
(ert-deftest bl-setq-local-and-value ()
  (defvar bl-c 10)
  ;; setq-local returns the value and makes the var local.
  (should (= 42 (setq-local bl-c 42)))
  (should (equal (list bl-c (default-value 'bl-c) (local-variable-p 'bl-c))
                 '(42 10 t)))
  (should (= 42 (buffer-local-value 'bl-c (current-buffer)))))

;; ---- kill-all-local-variables honors permanent-local ----
(ert-deftest bl-kill-all-local-variables ()
  (defvar bl-perm 1)
  (defvar bl-temp 1)
  (put 'bl-perm 'permanent-local t)
  (make-local-variable 'bl-perm)
  (make-local-variable 'bl-temp)
  (setq bl-perm 5 bl-temp 5)
  (kill-all-local-variables)
  ;; The permanent-local survives; the ordinary local is killed.
  ;; emacs -Q: perm (5 t), temp (1 nil)
  (should (equal (list bl-perm (local-variable-p 'bl-perm)) '(5 t)))
  (should (equal (list bl-temp (local-variable-p 'bl-temp)) '(1 nil))))

;; ---- default-value ignores the local binding ----
(ert-deftest bl-default-value-isolated ()
  (defvar bl-d 10)
  (make-local-variable 'bl-d)
  (setq bl-d 5)
  (set-default 'bl-d 99)
  ;; Local unaffected by set-default; default unaffected by the local set.
  ;; emacs -Q: (5 99)
  (should (equal (list bl-d (default-value 'bl-d)) '(5 99))))

;; ---- define-derived-mode: run installs state, keymap, hook ----
(ert-deftest mode-define-derived-mode ()
  ;; :syntax-table/:abbrev-table nil keeps this off the syntax/abbrev subsystems.
  (define-derived-mode dm-base-mode fundamental-mode "DmBase"
    :syntax-table nil :abbrev-table nil
    (setq-local dm-base-ran t))
  (define-derived-mode dm-child-mode dm-base-mode "DmChild"
    :syntax-table nil :abbrev-table nil
    (setq-local dm-child-var 42))
  (defvar dm-hook-ran nil)
  (add-hook 'dm-child-mode-hook (lambda () (setq dm-hook-ran t)))
  (dm-child-mode)
  ;; major-mode/mode-name set; parent body ran; local set; hook ran; keymap in.
  (should (eq major-mode 'dm-child-mode))
  (should (equal mode-name "DmChild"))
  (should (eq t dm-base-ran))
  (should (= 42 dm-child-var))
  (should (eq t (local-variable-p 'dm-child-var)))
  (should (eq t dm-hook-ran))
  (should (eq (current-local-map) dm-child-mode-map))
  ;; derived-mode-p walks the parent chain; a fundamental-mode parent is dropped
  ;; to nil by define-derived-mode, so it is NOT reported as a parent.
  (should (and (derived-mode-p 'dm-base-mode) t))
  (should (eq nil (derived-mode-p 'fundamental-mode)))
  (should (eq nil (derived-mode-p 'text-mode))))

;; ---- define-derived-mode keymap inherits the parent's map ----
(ert-deftest mode-derived-keymap-parent ()
  (define-derived-mode dk-parent-mode fundamental-mode "DkParent"
    :syntax-table nil :abbrev-table nil)
  (define-derived-mode dk-child-mode dk-parent-mode "DkChild"
    :syntax-table nil :abbrev-table nil)
  (dk-child-mode)
  ;; The child map's parent is the parent mode's map (set on first entry).
  (should (eq (keymap-parent dk-child-mode-map) dk-parent-mode-map)))

;; ---- define-minor-mode: toggle semantics + registration ----
(ert-deftest mode-define-minor-mode ()
  (defvar mm-side nil)
  (defvar mm-log nil)
  (define-minor-mode mm-mode "Doc." :lighter " Mm"
    (setq mm-side (if mm-mode 1 0)))
  (add-hook 'mm-mode-hook (lambda () (push mm-mode mm-log)))
  ;; The control variable is buffer-local (default nil).
  (should (eq nil (default-value 'mm-mode)))
  (mm-mode 1)                           ; positive arg -> enable
  (should (eq t mm-mode))
  (should (= 1 mm-side))
  (mm-mode -1)                          ; negative arg -> disable
  (should (eq nil mm-mode))
  (should (= 0 mm-side))
  (mm-mode)                             ; no arg from Lisp -> toggle
  (should (eq t mm-mode))
  ;; Hook fired on every state change, most recent first in the push log.
  (should (equal (reverse mm-log) '(t nil t)))
  (should (eq t (local-variable-p 'mm-mode)))
  ;; Registered in the minor-mode alist with its lighter.
  (should (equal (assq 'mm-mode minor-mode-alist) '(mm-mode " Mm"))))

(ert-run-tests-batch-and-exit)
