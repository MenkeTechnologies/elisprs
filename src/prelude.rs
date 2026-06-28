//! The elisp prelude: the large `[DERIVED]` surface (per the research
//! inventory) written in elisp on top of the Rust primitives. This is how the
//! port gets breadth without hand-coding a thousand subrs — exactly how Emacs
//! bootstraps `subr.el`.
//!
//! Loaded once per host (see `lib::load_prelude`), form by form, best-effort.
//! Everything here uses only primitives or earlier prelude definitions, and
//! every macro is defined before its first use.

pub const PRELUDE: &str = r#"
;;; ---- c[ad]+r family ----
(defun caar (x) (car (car x)))
(defun cadr (x) (car (cdr x)))
(defun cdar (x) (cdr (car x)))
(defun cddr (x) (cdr (cdr x)))
(defun caaar (x) (car (caar x)))
(defun caddr (x) (car (cddr x)))
(defun cdddr (x) (cdr (cddr x)))
(defun cadddr (x) (car (cdddr x)))
(defun cddddr (x) (cdr (cdddr x)))

;;; ---- numeric helpers ----
(defun abs (x) (if (< x 0) (- x) x))
(defun max (x &rest xs) (while xs (if (> (car xs) x) (setq x (car xs))) (setq xs (cdr xs))) x)
(defun min (x &rest xs) (while xs (if (< (car xs) x) (setq x (car xs))) (setq xs (cdr xs))) x)
(defun mod (x y) (let ((r (% x y))) (if (if (< r 0) (> y 0) (< y 0)) (+ r y) r)))
(defun /= (a b) (not (= a b)))
(defun plusp (x) (> x 0))
(defun minusp (x) (< x 0))
(defun cl-plusp (x) (> x 0))
(defun cl-minusp (x) (< x 0))
(defun evenp (x) (zerop (% x 2)))
(defun oddp (x) (not (zerop (% x 2))))
(defun natnump (x) (and (integerp x) (>= x 0)))
(defun wholenump (x) (natnump x))

;;; ---- list construction / access ----
(defun nthcdr (n l) (while (and (> n 0) l) (setq l (cdr l)) (setq n (1- n))) l)
(defun last (l) (while (cdr l) (setq l (cdr l))) l)
(defun make-list (n x) (let ((r nil)) (while (> n 0) (setq r (cons x r)) (setq n (1- n))) r))
(defun number-sequence (from to &optional inc)
  (setq inc (or inc 1))
  (let ((r nil)) (while (<= from to) (setq r (cons from r)) (setq from (+ from inc))) (reverse r)))
(defun elt (seq n) (if (listp seq) (nth n seq) (aref seq n)))
(defun safe-length (l) (length l))
(defun caar-safe (x) (if (consp x) (car x) nil))

;;; ---- membership / search ----
(defun memq (x l) (while (and l (not (eq x (car l)))) (setq l (cdr l))) l)
(defun member (x l) (while (and l (not (equal x (car l)))) (setq l (cdr l))) l)
(defun assq (k l) (let ((r nil)) (while (and l (not r)) (if (eq (caar l) k) (setq r (car l)) (setq l (cdr l)))) r))
(defun assoc (k l) (let ((r nil)) (while (and l (not r)) (if (equal (caar l) k) (setq r (car l)) (setq l (cdr l)))) r))
(defun rassq (v l) (let ((r nil)) (while (and l (not r)) (if (eq (cdar l) v) (setq r (car l)) (setq l (cdr l)))) r))
(defun alist-get (k al) (let ((p (assq k al))) (if p (cdr p) nil)))
(defun plist-get (pl k)
  (let ((r nil)) (while pl (if (eq (car pl) k) (progn (setq r (cadr pl)) (setq pl nil)) (setq pl (cddr pl)))) r))
(defun plist-member (pl k)
  (let ((r nil)) (while pl (if (eq (car pl) k) (progn (setq r pl) (setq pl nil)) (setq pl (cddr pl)))) r))

;;; ---- higher-order / sequence ----
(defun seq-reduce (f l init) (while l (setq init (funcall f init (car l))) (setq l (cdr l))) init)
(defun seq-map (f l) (mapcar f l))
(defun seq-each (f l) (mapc f l))
(defun seq-filter (pred l)
  (let ((r nil)) (while l (if (funcall pred (car l)) (setq r (cons (car l) r))) (setq l (cdr l))) (reverse r)))
(defun seq-remove (pred l) (seq-filter (lambda (e) (not (funcall pred e))) l))
(defun seq-find (pred l &optional default)
  (let ((res default)) (while l (if (funcall pred (car l)) (progn (setq res (car l)) (setq l nil)) (setq l (cdr l)))) res))
(defun seq-some (pred l) (let ((r nil)) (while (and l (not r)) (setq r (funcall pred (car l))) (setq l (cdr l))) r))
(defun seq-every-p (pred l) (let ((r t)) (while (and l r) (setq r (funcall pred (car l))) (setq l (cdr l))) r))
(defun seq-count (pred l) (let ((n 0)) (while l (if (funcall pred (car l)) (setq n (1+ n))) (setq l (cdr l))) n))
(defun seq-empty-p (l) (null l))
(defun seq-length (l) (length l))
(defun seq-elt (l n) (elt l n))
(defun seq-do (f l) (mapc f l))
(defun seq-contains-p (l x) (if (member x l) t nil))
(defun seq-reverse (l) (reverse l))
(defun mapconcat (f l &optional sep)
  (setq sep (or sep ""))
  (let ((r "") (first t))
    (while l
      (if first (setq first nil) (setq r (concat r sep)))
      (setq r (concat r (funcall f (car l))))
      (setq l (cdr l)))
    r))

;;; ---- set-ish list ops ----
(defun remove (x l) (seq-filter (lambda (e) (not (equal e x))) l))
(defun remq (x l) (seq-filter (lambda (e) (not (eq e x))) l))
(defun delete (x l) (remove x l))
(defun delq (x l) (remq x l))
(defun delete-dups (l)
  (let ((seen nil) (r nil))
    (while l
      (if (not (member (car l) seen)) (progn (setq seen (cons (car l) seen)) (setq r (cons (car l) r))))
      (setq l (cdr l)))
    (reverse r)))

;;; ---- cl-lib niceties ----
(defun cl-first (l) (car l))
(defun cl-second (l) (cadr l))
(defun cl-third (l) (caddr l))
(defun cl-rest (l) (cdr l))
(defun cl-remove-if (pred l) (seq-remove pred l))
(defun cl-remove-if-not (pred l) (seq-filter pred l))
(defun cl-find-if (pred l) (seq-find pred l))
(defun cl-some (pred l) (seq-some pred l))
(defun cl-every (pred l) (seq-every-p pred l))
(defun cl-reduce (f l) (if (null l) nil (seq-reduce f (cdr l) (car l))))

;;; ---- misc functions ----
(defun ignore (&rest _args) nil)
(defun always (&rest _args) t)
(defun keywordp (x) (and (symbolp x) (let ((n (symbol-name x))) (and (> (length n) 0) (eq (aref n 0) 58)))))

;;; ---- control macros ----
(defmacro prog2 (a b &rest body) (list (quote progn) a (cons (quote prog1) (cons b body))))
(defmacro incf (place) (list (quote setq) place (list (quote 1+) place)))
(defmacro decf (place) (list (quote setq) place (list (quote 1-) place)))
(defmacro push (x place) (list (quote setq) place (list (quote cons) x place)))
(defmacro pop (place) (list (quote prog1) (list (quote car) place) (list (quote setq) place (list (quote cdr) place))))
(defmacro dolist (spec &rest body)
  (let ((var (car spec)) (lst (cadr spec)))
    `(let ((,var nil) (--dolist-tail-- ,lst))
       (while --dolist-tail--
         (setq ,var (car --dolist-tail--))
         ,@body
         (setq --dolist-tail-- (cdr --dolist-tail--))))))
(defmacro dotimes (spec &rest body)
  (let ((var (car spec)) (cnt (cadr spec)))
    `(let ((,var 0) (--dotimes-limit-- ,cnt))
       (while (< ,var --dotimes-limit--)
         ,@body
         (setq ,var (1+ ,var))))))

;;; ---- error handling ----
(defmacro ignore-errors (&rest body) `(condition-case nil (progn ,@body) (error nil)))
(defmacro with-demoted-errors (fmt &rest body) `(condition-case --err-- (progn ,@body) (error (message ,fmt --err--) nil)))

;;; ---- ERT: Emacs Lisp Regression Testing (subset) ----
;; Ported from ERT: `should` / `should-not` / `should-error` / `skip-unless`
;; assertions, `ert-fail` / `ert-pass`, and `ert-deftest` with an optional
;; docstring plus `:expected-result` and `:tags` keyword args. Assertions signal
;; `ert-test-failed`; `skip-unless` signals `ert-test-skipped`. The runner
;; classifies each test as pass / fail / skip / expected-fail (XFAIL) /
;; unexpected-pass and returns the number of UNEXPECTED results (0 = all as
;; expected). `ert-run-tests-batch-and-exit` errors out on any unexpected one.

(defvar ert--tests nil)   ; alist of (name . expected-result), :passed | :failed

(defmacro should (form)
  `(if ,form t (signal 'ert-test-failed (list 'should ',form))))

(defmacro should-not (form)
  `(if ,form (signal 'ert-test-failed (list 'should-not ',form)) t))

(defmacro should-error (form &rest keys)
  (let* ((type (plist-get keys :type))
         (check (if type (list 'not (list 'eq (list 'car '--ert-c--) type)) nil)))
    `(let ((--ert-c-- (condition-case --ert-e-- (progn ,form 'ert--no-error)
                        (error --ert-e--))))
       (cond
        ((eq --ert-c-- 'ert--no-error)
         (signal 'ert-test-failed (list 'should-error :no-error ',form)))
        (,check
         (signal 'ert-test-failed (list 'should-error :wrong-type --ert-c--)))
        (t --ert-c--)))))

(defmacro skip-unless (form)
  `(unless ,form (signal 'ert-test-skipped (list 'skip-unless ',form))))

(defun ert-fail (data) (signal 'ert-test-failed data))
(defun ert-pass () t)

(defmacro ert-deftest (name arglist &rest body)
  ;; Strip an optional docstring, then leading :expected-result / :tags args.
  (if (stringp (car body)) (setq body (cdr body)))
  (let ((expected :passed))
    (while (memq (car body) '(:expected-result :tags))
      (if (eq (car body) :expected-result) (setq expected (car (cdr body))))
      (setq body (cdr (cdr body))))
    `(progn
       (defun ,name ,arglist ,@body)
       (setq ert--tests (cons (cons ',name ,expected) ert--tests))
       ',name)))

(defun ert-run-tests-batch ()
  (let ((tests (reverse ert--tests)) (total 0) (unexpected 0) (skipped 0))
    (while tests
      (let* ((entry (car tests)) (name (car entry)) (expected (cdr entry)))
        (setq total (1+ total))
        (condition-case --ert-err--
            (progn
              (funcall name)
              (if (eq expected :failed)
                  (progn (setq unexpected (1+ unexpected))
                         (message "  UNEXPECTED-OK  %s" name))
                (message "  PASS   %s" name)))
          (ert-test-skipped
           (setq skipped (1+ skipped))
           (message "  SKIP   %s" name))
          (error
           (if (eq expected :failed)
               (message "  XFAIL  %s" name)
             (progn (setq unexpected (1+ unexpected))
                    (message "  FAIL   %s -- %S" name --ert-err--))))))
      (setq tests (cdr tests)))
    (message "Ran %d tests: %d unexpected, %d skipped." total unexpected skipped)
    unexpected))

(defun ert-run-tests-batch-and-exit ()
  "Run all tests; raise an error (→ non-zero exit) on any unexpected result."
  (let ((bad (ert-run-tests-batch)))
    (if (> bad 0) (error "%d unexpected ERT result(s)" bad) t)))
"#;
