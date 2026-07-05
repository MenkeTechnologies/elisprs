;;; byte-run-defs.el --- byte-run.el/subr.el/gv.el preloaded defs  -*- lexical-binding: nil; -*-

;; Pins the foundational definitions that real Emacs preloads (from byte-run.el,
;; subr.el and gv.el) and that self-contained pure-lisp libraries (subr-x, seq,
;; gv, rx, cl-lib, cl-seq, macroexp, generator, cl-generic) rely on at load time.
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Run through fusevm; `ert-run-tests-batch-and-exit' gates the suite.

(message "== byte-run defs demo ==")

;; ---- defsubst (byte-run.el:481): an inline function == defun in the interpreter,
;; plus the `byte-optimizer' property the byte-compiler consults. ----
(ert-deftest byte-run-defsubst ()
  (defsubst brd-empty-p (h) (zerop (hash-table-count h)))
  ;; Callable exactly like a defun.
  (should (eq t (brd-empty-p (make-hash-table))))
  (should (functionp 'brd-empty-p))
  ;; emacs -Q: (get 'brd-empty-p 'byte-optimizer) => byte-compile-inline-expand
  (should (eq 'byte-compile-inline-expand (get 'brd-empty-p 'byte-optimizer))))

;; ---- declare-function (subr.el:31): a pure byte-compiler hint; the
;; interpreter expands it to nil regardless of arity. ----
(ert-deftest byte-run-declare-function ()
  ;; emacs -Q: (macroexpand '(declare-function foo "bar")) => nil
  (should (eq nil (macroexpand '(declare-function foo "bar"))))
  ;; emacs -Q: (macroexpand '(declare-function foo "bar" (a b) t)) => nil
  (should (eq nil (macroexpand '(declare-function foo "bar" (a b) t))))
  ;; Evaluating a declaration is a no-op returning nil, and does NOT define FN.
  (should (eq nil (declare-function brd-undefined "nowhere" (x))))
  (should (eq nil (fboundp 'brd-undefined))))

;; ---- make-obsolete / make-obsolete-variable (byte-run.el) ----
(ert-deftest byte-run-make-obsolete ()
  (defun brd-oldf () 1)
  (make-obsolete 'brd-oldf 'brd-newf "30.1")
  ;; emacs -Q: (get 'brd-oldf 'byte-obsolete-info) => (brd-newf nil "30.1")
  (should (equal '(brd-newf nil "30.1") (get 'brd-oldf 'byte-obsolete-info)))
  (make-obsolete-variable 'brd-oldv 'brd-newv "30.1" 'set)
  ;; emacs -Q: (get 'brd-oldv 'byte-obsolete-variable) => (brd-newv set "30.1")
  (should (equal '(brd-newv set "30.1") (get 'brd-oldv 'byte-obsolete-variable)))
  ;; A quote-mark slip (nil/t as name) is an error, not a silent no-op.
  (should-error (make-obsolete nil 'x "30.1")))

;; ---- define-obsolete-function-alias (byte-run.el): defalias + make-obsolete ----
(ert-deftest byte-run-define-obsolete-function-alias ()
  (defun brd-newfn (x) (* x 2))
  (define-obsolete-function-alias 'brd-oldfn 'brd-newfn "30.1")
  ;; emacs -Q: (list (brd-oldfn 5) (get 'brd-oldfn 'byte-obsolete-info))
  ;;           => (10 (brd-newfn nil "30.1"))
  (should (= 10 (brd-oldfn 5)))
  (should (equal '(brd-newfn nil "30.1") (get 'brd-oldfn 'byte-obsolete-info))))

;; ---- autoload / autoloadp (C `Fautoload'; subr.el `autoloadp') ----
(ert-deftest byte-run-autoload ()
  (autoload 'brd-af "afile" "d")
  ;; emacs -Q: (symbol-function 'brd-af) => (autoload "afile" "d" nil nil)
  (should (equal '(autoload "afile" "d" nil nil) (symbol-function 'brd-af)))
  (should (autoloadp (symbol-function 'brd-af)))
  (should (autoloadp '(autoload "f")))
  (should-not (autoloadp '(lambda () 1)))
  ;; Does not clobber an already-defined non-autoload function.
  (defun brd-defined () 42)
  (autoload 'brd-defined "other")
  (should (= 42 (brd-defined))))

;; ---- purecopy: identity in elisprs (no pure space) ----
(ert-deftest byte-run-purecopy ()
  (should (equal "hi" (purecopy "hi")))
  (should (equal '(1 2 3) (purecopy '(1 2 3)))))

;; ---- compiled-function-p (C): t only for primitive subrs in elisprs ----
(ert-deftest byte-run-compiled-function-p ()
  ;; emacs -Q: (list (compiled-function-p (symbol-function 'car))
  ;;                 (compiled-function-p (lambda (x) x))
  ;;                 (compiled-function-p 5)) => (t nil nil)
  (should (eq t (and (compiled-function-p (symbol-function 'car)) t)))
  (should-not (compiled-function-p (lambda (x) x)))
  (should-not (compiled-function-p 5)))

;; ---- add-hook / run-hooks / run-hook-with-args (subr.el / C) ----
;; Accumulators are top-level special vars so the hook lambdas mutate them
;; through `run-hooks' regardless of the enclosing binding mode.
(defvar brd-acc nil)
(defvar brd-count 0)
(ert-deftest byte-run-hooks ()
  (setq brd-acc nil)
  (add-hook 'brd-hook (lambda () (setq brd-acc (cons 'a brd-acc))))
  (add-hook 'brd-hook (lambda () (setq brd-acc (cons 'b brd-acc))))
  (run-hooks 'brd-hook)
  ;; emacs -Q: two lambdas, depth 0 => second one prepended => (a b)
  (should (equal '(a b) brd-acc))
  ;; add-hook does not re-add an identical function.
  (setq brd-count 0)
  (let ((fn (lambda () (setq brd-count (1+ brd-count)))))
    (add-hook 'brd-hook2 fn)
    (add-hook 'brd-hook2 fn)
    (run-hooks 'brd-hook2)
    (should (= 1 brd-count)))
  ;; run-hooks on an unbound hook is a silent no-op.
  (should-not (run-hooks 'brd-never-bound))
  ;; run-hook-with-args threads arguments through.
  (setq brd-acc nil)
  (add-hook 'brd-hook3 (lambda (x) (setq brd-acc (cons x brd-acc))))
  (run-hook-with-args 'brd-hook3 7)
  (should (equal '(7) brd-acc)))

;; ---- eval-after-load / with-eval-after-load (subr.el) ----
(ert-deftest byte-run-eval-after-load ()
  ;; Feature already provided => FORM runs immediately.
  (let ((r nil))
    (provide 'brd-feat)
    (eval-after-load 'brd-feat (lambda () (setq r 'ran)))
    (should (eq 'ran r)))
  ;; Feature not provided => registered, not run.
  (let ((r 'untouched))
    (eval-after-load 'brd-absent-feat (lambda () (setq r 'ran)))
    (should (eq 'untouched r))))

;; ---- def-edebug-elem-spec (subr.el): records the spec property ----
(ert-deftest byte-run-def-edebug-elem-spec ()
  (def-edebug-elem-spec 'brd-spec '(sexp form))
  (should (equal '(sexp form) (get 'brd-spec 'edebug-elem-spec)))
  ;; A `&'/`:'-prefixed name is rejected; a non-list spec is rejected.
  (should-error (def-edebug-elem-spec '&bad '(x)))
  (should-error (def-edebug-elem-spec 'brd-notalist 'x)))

;; ---- setf: macro-defined places expand and retry (gv.el:103) ----
;; (cl--generic name) style: a macro place expanding to (get name 'slot).
;; Defined at top level so it exists when the deftest body is macroexpanded.
(defmacro brd-slot (n) (list 'get n ''brd-slot))
(ert-deftest setf-macro-place ()
  (setf (brd-slot 'obj) 42)
  ;; emacs -Q: (get 'obj 'brd-slot) => 42
  (should (= 42 (get 'obj 'brd-slot))))

;; ---- setf: control-flow places if/cond/progn (gv.el) ----
(ert-deftest setf-control-flow-places ()
  ;; (setf (car (if FLAG A B)) V) mutates the selected branch only.
  (let ((a (list 1 2)) (b (list 3 4)) (flag nil))
    (setf (car (if flag a b)) 9)
    ;; emacs -Q: (list a b) => ((1 2) (9 4))
    (should (equal '((1 2) (9 4)) (list a b))))
  ;; (setf (progn ... PLACE) V) targets the last form.
  (let ((c (list 5 6)))
    (setf (car (progn 'ignored c)) 99)
    (should (equal '(99 6) c)))
  ;; (setf (cond (COND PLACE)) V) targets the chosen clause's last form.
  (let ((d (list 7 8)) (pick t))
    (setf (car (cond (pick d) (t nil))) 11)
    (should (equal '(11 8) d))))

;; ---- setf: default-value place (gv simple setter) ----
(ert-deftest setf-default-value-place ()
  (defvar brd-dv 1)
  (setf (default-value 'brd-dv) 5)
  ;; No buffer-local model, so default-value == symbol-value.
  (should (= 5 (default-value 'brd-dv)))
  (should (= 5 brd-dv)))

(ert-run-tests-batch-and-exit)
;;; byte-run-defs.el ends here
