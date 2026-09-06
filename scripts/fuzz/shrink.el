;;; shrink.el --- emit smaller candidates for one diverging fuzz form  -*- lexical-binding: t; -*-

;; A raw fuzz hit is a depth-3 tree with three unrelated distractions bolted
;; onto the one call that actually diverges, which is why they take longer to
;; diagnose than to fix.  This is the candidate generator for the delta-debugger
;; in `fuzz_parity.sh': given ONE form on stdin (or in FUZZ_FORM), it prints a
;; batch of strictly-smaller forms, one per line, in roughly
;; most-aggressive-first order.  The orchestrator evaluates the whole batch
;; under both engines in one process pair, keeps the first candidate that still
;; diverges, and calls back in with that.  Iterating to a fixpoint is what turns
;;
;;   (equal (split-string 1.5) (let ((x (and 97 97))) (cons x (let ((n 0))
;;     (defalias 'fzwa '1+) (ignore-errors (fzwa 1 (setq n 9))) n))))
;;
;; into `(split-string 1.5)'.
;;
;; The generator is deliberately dumb and syntactic: it knows nothing about
;; which head symbols matter, so it can never shrink "towards" a bug it already
;; believes in.  The only oracle is the differential one, applied by the caller.
;;
;;   FUZZ_FORM   the form to shrink, as text.  Required.
;;
;; Every candidate is printed with `prin1' under `print-escape-newlines', the
;; same one-form-per-line contract `drive.el' reads, so a candidate can be fed
;; straight back through the normal corpus path with no special casing.

(require 'cl-lib)

;;; ── tree addressing ──────────────────────────────────────────────────────────
;; A "path" is a list of indices: (2 0) is `(nth 0 (nth 2 form))'.  Only proper
;; list structure is walked — a dotted tail or a vector is a leaf, because
;; rebuilding those positions would change the form's shape rather than shrink
;; it, and a shrinker that alters shape reports a different bug than the one it
;; was handed.

(defun fzs-paths (form &optional prefix depth)
  "All paths into FORM's proper-list structure, deepest-last.
PREFIX is the path of FORM itself; DEPTH bounds the walk so a pathological
corpus line cannot make the generator itself the slow part."
  (let ((depth (or depth 0)))
    (when (and (proper-list-p form) form (< depth 8))
      (let ((out nil) (i 0))
        (dolist (child form)
          (let ((p (append prefix (list i))))
            (push p out)
            (setq out (nconc (nreverse (fzs-paths child p (1+ depth))) out)))
          (setq i (1+ i)))
        (nreverse out)))))

(defun fzs-get (form path)
  "The subterm of FORM at PATH."
  (if (null path) form (fzs-get (nth (car path) form) (cdr path))))

(defun fzs-put (form path new)
  "A copy of FORM with the subterm at PATH replaced by NEW."
  (if (null path)
      new
    (let* ((copy (copy-sequence form))
           (i (car path)))
      (setcar (nthcdr i copy) (fzs-put (nth i copy) (cdr path) new))
      copy)))

(defun fzs-drop (form path)
  "A copy of FORM with the element at PATH removed from its parent list."
  (let ((parent-path (butlast path))
        (i (car (last path))))
    (fzs-put form parent-path
             (let ((parent (copy-sequence (fzs-get form parent-path))))
               (append (seq-take parent i) (nthcdr (1+ i) parent))))))

;;; ── per-subterm reductions ───────────────────────────────────────────────────
;; What a single subterm can collapse to.  `nil' is first because it is the most
;; aggressive and the most often accepted; the literals after it exist so a
;; divergence that needs *some* value of the right rough type does not block the
;; walk.  A call's own arguments come last: promoting `(f (g x))' to `(g x)' is
;; the reduction that peels wrappers off, and it is worth trying every argument
;; because the diverging call is rarely in argument 0.

(defun fzs-reductions (sub)
  (append
   (unless (null sub) '(nil))
   (cond
    ((and (numberp sub) (not (equal sub 0))) '(0))
    ((and (stringp sub) (not (equal sub ""))) '("")))
   ;; `(quote X)' collapses to X only when X is self-evaluating; otherwise the
   ;; candidate would evaluate a symbol or a call that the original never ran.
   (when (and (consp sub) (eq (car sub) 'quote)
              (let ((d (cadr sub))) (or (numberp d) (stringp d) (keywordp d)
                                        (memq d '(nil t)))))
     (list (cadr sub)))
   (when (and (consp sub) (proper-list-p sub))
     (cdr sub))))

(defun fzs-candidates (form)
  "Smaller forms to try in place of FORM, most-aggressive-first, deduplicated."
  (let ((seen (make-hash-table :test #'equal))
        (out nil)
        (paths (fzs-paths form)))
    (cl-flet ((offer (c)
                (unless (or (equal c form) (gethash c seen))
                  (puthash c t seen)
                  (push c out))))
      ;; 1. Replace the whole form by one of its own subterms.  This is the big
      ;;    win on the wrapper-heavy shapes `gen.el' produces, so it goes first.
      (dolist (p paths)
        (let ((sub (fzs-get form p)))
          (when (consp sub) (offer sub))))
      ;; 2. Reduce one subterm in place.
      (dolist (p paths)
        (dolist (r (fzs-reductions (fzs-get form p)))
          (offer (fzs-put form p r))))
      ;; 3. Drop one element outright — the only reduction that can shorten an
      ;;    argument list, which is what exposes arity-dependent divergences.
      (dolist (p paths)
        ;; Never drop a head symbol: `(f a b)' -> `(a b)' calls something else
        ;; entirely, which is a different form, not a smaller one.
        (unless (eq (car (last p)) 0)
          (offer (fzs-drop form p)))))
    ;; Smallest first: fewer conses is a better candidate to accept, and the
    ;; caller takes the first that still diverges.
    (sort (nreverse out)
          (lambda (a b) (< (length (prin1-to-string a))
                           (length (prin1-to-string b)))))))

;;; ── main ─────────────────────────────────────────────────────────────────────

(setq print-escape-newlines t)

(let* ((text (or (getenv "FUZZ_FORM") (error "FUZZ_FORM unset")))
       (form (car (read-from-string text))))
  (dolist (c (fzs-candidates form))
    (princ (prin1-to-string c))
    (terpri)))

;;; shrink.el ends here
