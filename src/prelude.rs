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

;; Regexp matching folds case unless this is let-bound to nil (Emacs default t).
(defvar case-fold-search t)

;;; ---- build/system identity (C-level vars from emacs.c, verified against
;;; ---- GNU Emacs 30.2) ----
;; `emacs-version' is a C variable (Vemacs_version). We track the emulated
;; Emacs release the prelude is faithful to.
(defconst emacs-version "30.2")
;; `emacs-major-version'/`emacs-minor-version' are derived in lisp/version.el
;; exactly like this; kept here because version.el is not preloaded.
(defconst emacs-major-version
  (progn (string-match "^[0-9]+" emacs-version)
         (string-to-number (match-string 0 emacs-version))))
(defconst emacs-minor-version
  (progn (string-match "^[0-9]+\\.\\([0-9]+\\)" emacs-version)
         (string-to-number (match-string 1 emacs-version))))
;; `emacs-build-system'/`emacs-build-time'/`emacs-build-number' are defined in
;; lisp/version.el exactly like this (minus the Android build branches, which
;; never apply here); kept here because version.el is not preloaded. Because
;; `emacs-build-system' is non-nil, `emacs-build-time' evaluates to a
;; `current-time' timestamp, matching a normally-dumped `emacs -Q --batch'.
(defconst emacs-build-system (system-name))
(defconst emacs-build-time (if emacs-build-system (current-time)))
(defconst emacs-build-number 1)
;; `system-type' (Vsystem_type) is platform-derived; the Rust primitive maps the
;; running OS to Emacs's symbol (darwin/gnu-linux/berkeley-unix/windows-nt).
(defvar system-type (--system-type--))
;; No GUI and non-interactive batch execution, matching `emacs -Q --batch'.
(defvar window-system nil)
(defvar noninteractive t)
;; `temporary-file-directory' is defined below, right after
;; `file-name-as-directory', which its initializer depends on.

;;; ---- numeric helpers ----
;; Fixnum bounds match GNU Emacs on a 64-bit build (62-bit tagged integers).
(defconst most-positive-fixnum 2305843009213693951)
(defconst most-negative-fixnum -2305843009213693952)
(defconst float-pi 3.141592653589793)
(defconst float-e 2.718281828459045)
(defconst pi 3.141592653589793)
;; `abs` is a primitive subr (keeps int/float type; (abs -0.0) => 0.0).
;; NaN propagates: any NaN arg makes the result NaN (a NaN never wins `>`/`<`,
;; so once it lands in the accumulator no later value can displace it).
;; `mod` is a primitive subr (handles float operands + divisor-sign semantics).
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
(defun nthcdr (n l)
  ;; Emacs signals on a non-integer index (a float is rejected even when it
  ;; is integer-valued): (nthcdr 1.5 '(a b c)) => wrong-type-argument integerp.
  (unless (integerp n) (signal 'wrong-type-argument (list 'integerp n)))
  (while (and (> n 0) l) (setq l (cdr l)) (setq n (1- n))) l)
(defun last (l &optional n)
  ;; The last N cons cells of L (default 1): (last '(1 2 3) 2) => (2 3).
  ;; Guard on consp so an improper tail stops the walk instead of erroring:
  ;; (last '(1 2 . 3)) => (2 . 3).
  (cond
   ;; A non-list has no cons cells to take: Emacs returns it unchanged --
   ;; (last t) and (last t 0) are both t.
   ((not (consp l)) l)
   ((or (null n) (= n 1))
    (while (consp (cdr l)) (setq l (cdr l)))
    l)
   (t (nthcdr (max 0 (- (length l) n)) l))))
(defun make-list (n x)
  (unless (and (integerp n) (>= n 0) (<= n most-positive-fixnum))
    (signal 'wrong-type-argument (list 'wholenump n)))
  (let ((r nil)) (while (> n 0) (setq r (cons x r)) (setq n (1- n))) r))
(defun number-sequence (from &optional to inc)
  ;; With only FROM, or FROM=TO, the sequence is (FROM); INC defaults to 1.
  (if (or (null to) (= from to))
      (list from)
    (setq inc (or inc 1))
    ;; A zero increment would loop forever; Emacs signals instead.
    (when (= inc 0) (error "The increment can not be zero"))
    (let ((r nil))
      (if (< inc 0)
          (while (>= from to) (setq r (cons from r)) (setq from (+ from inc)))
        (while (<= from to) (setq r (cons from r)) (setq from (+ from inc))))
      (reverse r))))
(defun elt (seq n)
  ;; List path defers to `nth' (signals integerp on a float index); the array
  ;; path signals fixnump like Emacs: (elt [1 2 3] 1.5) => wrong-type-argument
  ;; fixnump, matching aref's own contract rather than nth's integerp.
  (cond ((listp seq) (nth n seq))
        ((arrayp seq)
         (unless (integerp n) (signal 'wrong-type-argument (list 'fixnump n)))
         (aref seq n))
        (t (signal 'wrong-type-argument (list 'sequencep seq)))))
(defun safe-length (l)
  ;; Length of a possibly circular or dotted list, never erroring or looping
  ;; forever.  Faithful to Emacs 30.2's FOR_EACH_TAIL_SAFE (Brent's teleporting
  ;; tortoise/hare, lisp.h): the tortoise jumps to the current tail after
  ;; 2, 4, 8, ... steps and a cycle is reported when the tail meets it, so a
  ;; circular list returns an integer >= the number of distinct cells -- a
  ;; 3-cycle => 5, exactly as Emacs.  (safe-length '(1 2 . 3)) => 2.
  (let ((tail l) (tortoise l) (interval 2) (q 2) (len 0) (done nil))
    (while (and (consp tail) (not done))
      (setq len (1+ len) tail (cdr tail))
      (if (not (consp tail))
          (setq done t)
        (setq q (1- q))
        (if (> q 0)
            (when (eq tail tortoise) (setq done t))
          (setq interval (* interval 2) q interval tortoise tail))))
    len))
(defun length= (seq n) (= (length seq) n))
(defun length< (seq n) (< (length seq) n))
(defun length> (seq n) (> (length seq) n))
(defun car-safe (x) (if (consp x) (car x) nil))
(defun cdr-safe (x) (if (consp x) (cdr x) nil))
(defun caar-safe (x) (if (consp x) (car x) nil))

;;; ---- membership / search ----
(defun memq (x l) (while (and l (not (eq x (car l)))) (setq l (cdr l))) l)
(defun member (x l) (while (and l (not (equal x (car l)))) (setq l (cdr l))) l)
(defun memql (x l) (while (and l (not (eql x (car l)))) (setq l (cdr l))) l)
(defun assoc-string (key alist &optional case-fold)
  ;; First ALIST element equal to KEY as a string (elements may be strings or
  ;; (STRING . VALUE) conses); CASE-FOLD ignores case.
  (let ((k (if (symbolp key) (symbol-name key) key)) (r nil))
    (while (and alist (not r))
      (let* ((el (car alist))
             (raw (if (consp el) (car el) el))
             (s (if (symbolp raw) (symbol-name raw) raw)))
        (if (if case-fold (string-equal-ignore-case k s) (string= k s))
            (setq r el)
          (setq alist (cdr alist)))))
    r))
;; assq/assoc/rassq skip non-cons list elements (Emacs C `FOR_EACH_TAIL' + a
;; `CONSP' guard), so e.g. a docstring string in a body list is ignored rather
;; than signalling `wrong-type-argument listp' — cl-generic relies on this via
;; `(assq 'interactive BODY)'.
(defun assq (k l)
  (let ((r nil))
    (while (and l (not r))
      (if (and (consp (car l)) (eq (car (car l)) k))
          (setq r (car l))
        (setq l (cdr l))))
    r))
(defun assoc (k l &optional testfn)
  (let ((r nil))
    (while (and l (not r))
      (if (and (consp (car l))
               (if testfn (funcall testfn (car (car l)) k) (equal (car (car l)) k)))
          (setq r (car l))
        (setq l (cdr l))))
    r))
(defun rassq (v l)
  (let ((r nil))
    (while (and l (not r))
      (if (and (consp (car l)) (eq (cdr (car l)) v))
          (setq r (car l))
        (setq l (cdr l))))
    r))
(defun alist-get (k al &optional default _remove testfn)
  ;; Value associated with K in alist AL (DEFAULT if absent); TESTFN overrides eq.
  (let ((p (if testfn (assoc k al testfn) (assq k al))))
    (if p (cdr p) default)))
(defun plist-get (pl k &optional predicate)
  ;; Emacs walks cons cells and simply stops at a non-cons -- (plist-get 'sym 1)
  ;; is nil, not an error.
  (let ((test (or predicate #'eq)) (r nil))
    (while (consp pl)
      (if (funcall test (car pl) k) (progn (setq r (cadr pl)) (setq pl nil)) (setq pl (cddr pl))))
    r))
(defun plist-member (pl k &optional predicate)
  (unless (listp pl) (signal 'wrong-type-argument (list 'plistp pl)))
  (let ((test (or predicate #'eq)) (r nil))
    (while (consp pl)
      (if (funcall test (car pl) k) (progn (setq r pl) (setq pl nil)) (setq pl (cddr pl))))
    r))

;;; ---- higher-order / sequence ----
;; seq-* accept any sequence; coerce list/vector/string to a list to iterate.
(defun seq-reduce (f l init) (setq l (append l nil)) (while l (setq init (funcall f init (car l))) (setq l (cdr l))) init)
(defun seq-map (f l) (mapcar f l))
(defun seq-each (f l) (mapc f l))
(defun seq-filter (pred l)
  (setq l (append l nil))
  (let ((r nil)) (while l (if (funcall pred (car l)) (setq r (cons (car l) r))) (setq l (cdr l))) (reverse r)))
(defun seq-remove (pred l) (seq-filter (lambda (e) (not (funcall pred e))) l))
(defun seq-find (pred l &optional default)
  (setq l (append l nil))
  (let ((res default)) (while l (if (funcall pred (car l)) (progn (setq res (car l)) (setq l nil)) (setq l (cdr l)))) res))
(defun seq-some (pred l) (setq l (append l nil)) (let ((r nil)) (while (and l (not r)) (setq r (funcall pred (car l))) (setq l (cdr l))) r))
(defun seq-every-p (pred l) (setq l (append l nil)) (let ((r t)) (while (and l r) (setq r (funcall pred (car l))) (setq l (cdr l))) r))
(defun seq-count (pred l) (setq l (append l nil)) (let ((n 0)) (while l (if (funcall pred (car l)) (setq n (1+ n))) (setq l (cdr l))) n))
(defun seq-empty-p (l) (= 0 (length l)))
(defun seq-length (l) (length l))
(defun seq-elt (l n) (elt l n))
(defun seq-do (f l) (mapc f l))
(defun seqp (object) (sequencep object))
(defun seq-contains-p (seq elt &optional testfn)
  (let ((test (or testfn #'equal)) (l (append seq nil)) (r nil))
    (while (and l (not r)) (when (funcall test elt (car l)) (setq r t)) (setq l (cdr l)))
    r))
(defun seq-reverse (l) (reverse l))
(defun mapconcat (f seq &optional sep)
  ;; SEQ may be any sequence (list/vector/string); coerce to a list of elements.
  (setq sep (or sep ""))
  (let ((l (append seq nil)) (r "") (first t))
    (while l
      (if first (setq first nil) (setq r (concat r sep)))
      (setq r (concat r (funcall f (car l))))
      (setq l (cdr l)))
    r))

;;; ---- set-ish list ops ----
(defun remove (x l)
  (let ((r (seq-filter (lambda (e) (not (equal e x))) (append l nil))))
    (if (vectorp l) (vconcat r) r)))
(defun remq (x l)
  ;; Emacs: (delq X (copy-sequence LIST)) -- a non-list signals `listp', where
  ;; the seq-filter form this used to be signalled `sequencep'.
  (unless (listp l) (signal 'wrong-type-argument (list 'listp l)))
  (seq-filter (lambda (e) (not (eq e x))) l))
(defun string-to-multibyte (s) s)
(defun string-as-multibyte (s) s)
(defun string-to-unibyte (s &rest _) s)
(defun string-as-unibyte (s) s)
(defun multibyte-string-p (s)
  ;; Approximate Emacs: a string is "multibyte" if it has any non-ASCII char.
  (let ((l (and (stringp s) (string-to-list s))) (r nil))
    (while (and l (not r)) (when (>= (car l) 128) (setq r t)) (setq l (cdr l)))
    r))
;; delete/delq remove matching elements destructively (splicing cons cells) for
;; lists; non-lists fall back to the copying remove/remq.
(defun delete (x l)
  (if (not (listp l)) (remove x l)
    (while (and (consp l) (equal (car l) x)) (setq l (cdr l)))
    (let ((tail l))
      (while (and (consp tail) (consp (cdr tail)))
        (if (equal (car (cdr tail)) x) (setcdr tail (cddr tail))
          (setq tail (cdr tail)))))
    l))
(defun delq (x l)
  (unless (or (null l) (proper-list-p l))
    (signal 'wrong-type-argument (list 'listp l)))
  (if (not (listp l)) (remq x l)
    (while (and (consp l) (eq (car l) x)) (setq l (cdr l)))
    (let ((tail l))
      (while (and (consp tail) (consp (cdr tail)))
        (if (eq (car (cdr tail)) x) (setcdr tail (cddr tail))
          (setq tail (cdr tail)))))
    l))
(defun delete-dups (l)
  ;; Destructively remove `equal' duplicates, keeping the first occurrence.
  (let ((tail l))
    (while tail
      (setcdr tail (delete (car tail) (cdr tail)))
      (setq tail (cdr tail))))
  l)
(defun nconc (&rest lists)
  ;; Destructively concatenate LISTS by splicing each onto the previous tail.
  ;; (Uses `while`, not `dolist`, which is defined later in this prelude.)
  ;; Only the arguments BEFORE the last must be lists; the last may be any object
  ;; and becomes the final cdr: (nconc (list 1) (cons 2 "s")) => (1 2 . "s").
  (let ((result nil) (tail nil))
    (while lists
      (let ((seg (car lists)) (last (null (cdr lists))))
        (unless (or last (listp seg))
          (signal 'wrong-type-argument (list 'listp seg)))
        (when seg
          (if result (setcdr tail seg) (setq result seg))
          ;; Walk to the last cons; a non-cons segment (only possible as the last
          ;; argument) has no tail to walk.
          (when (consp seg)
            (setq tail seg)
            (while (consp (cdr tail)) (setq tail (cdr tail))))))
      (setq lists (cdr lists)))
    result))
(defun rassq-delete-all (value alist)
  (seq-filter (lambda (p) (not (and (consp p) (eq (cdr p) value)))) alist))

;;; ---- cl-lib niceties ----
(defun cl-first (l) (car l))
(defun cl-second (l) (cadr l))
(defun cl-third (l) (caddr l))
(defun cl-rest (l) (cdr l))
(defun cl-remove-if (pred seq &rest keys)
  ;; Honors :key, :count, :start, :end and :from-end (the latter removes the
  ;; LAST COUNT matches).
  (let ((count (cl--getkey keys :count nil))
        (key (cl--getkey keys :key 'identity))
        (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil)))
    (cl--remove-by (lambda (x) (funcall pred (funcall key x)))
                   seq start end count from-end)))
(defun cl-remove-if-not (pred seq &rest keys)
  (apply 'cl-remove-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-delete-if (pred seq &rest keys) (apply (function cl-remove-if) pred seq keys))
(defun cl-delete-if-not (pred seq &rest keys) (apply (function cl-remove-if-not) pred seq keys))
(defun cl-find-if (pred seq &rest keys)
  (let ((key (cl--getkey keys :key 'identity))
        (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (r nil) (found nil))
    (while (and lst (or from-end (not found)))
      (when (and (cl--in-bounds i start end) (funcall pred (funcall key (car lst))))
        (setq r (car lst) found t))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-find-if-not (pred seq &rest keys)
  (apply 'cl-find-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-sort (seq pred &rest keys)
  (let ((key (cl--getkey keys :key nil)))
    (if key (sort seq (lambda (a b) (funcall pred (funcall key a) (funcall key b))))
      (sort seq pred))))
(defun commandp (_obj &optional _) nil)
(defun plistp (l)
  (let ((n 0)) (while (consp l) (setq n (1+ n)) (setq l (cdr l))) (and (null l) (= 0 (% n 2)))))
;; Port of cl-some/cl-every from cl-extra.el: with extra SEQs (or a non-list
;; SEQ) map PRED over the sequences in parallel, stopping at the shortest.
(defun cl-some (pred seq &rest rest)
  (if (or rest (nlistp seq))
      (catch 'cl-some
        (apply (function cl-map) nil
               (lambda (&rest x)
                 (let ((res (apply pred x)))
                   (if res (throw 'cl-some res))))
               seq rest)
        nil)
    (let ((x nil))
      (while (and seq (not (setq x (funcall pred (car seq))))) (setq seq (cdr seq)))
      x)))
(defun cl-every (pred seq &rest rest)
  (if (or rest (nlistp seq))
      (catch 'cl-every
        (apply (function cl-map) nil
               (lambda (&rest x) (or (apply pred x) (throw 'cl-every nil)))
               seq rest)
        t)
    (while (and seq (funcall pred (car seq))) (setq seq (cdr seq)))
    (null seq)))
(defun cl-notany (pred seq &rest rest) (not (apply (function cl-some) pred seq rest)))
(defun cl-notevery (pred seq &rest rest) (not (apply (function cl-every) pred seq rest)))
;; cl- aliases for the 3+-level c[ad]r accessors (Emacs only prefixes these;
;; the 2-level caar/cadr/cdar/cddr stay unprefixed).
(defun cl-caddr (x) (caddr x))
(defun cl-cdddr (x) (cdddr x))
(defun cl-cadddr (x) (cadddr x))
;; cl- list utilities.
(defun cl-list-length (l) (length l))
(defun cl-copy-list (l) (copy-sequence l))
(defun cl-revappend (l tail) (append (reverse l) tail))
(defun cl-nreconc (l tail) (nconc (nreverse l) tail))
(defmacro cl-nth-value (n form) (list 'nth n form))
(defun cl-search (seq1 seq2 &rest keys)
  ;; Index in SEQ2 where SEQ1 occurs as a contiguous subsequence, else nil.
  ;; Honors :test, :key and :from-end (rightmost match). Faithful to cl-seq.el.
  (let* ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
         (from-end (cl--getkey keys :from-end nil))
         (l1 (append seq1 nil)) (l2 (append seq2 nil))
         (len1 (length l1)) (len2 (length l2)) (i 0) (res nil))
    (if (= len1 0) (if from-end len2 0)
      (while (<= i (- len2 len1))
        (let ((a l1) (b (nthcdr i l2)) (ok t))
          (while (and ok a)
            (if (funcall test (funcall key (car a)) (funcall key (car b)))
                (setq a (cdr a) b (cdr b)) (setq ok nil)))
          (when ok (setq res i)))
        (if (and res (not from-end)) (setq i (1+ (- len2 len1))) (setq i (1+ i))))
      res)))
(defun cl-mismatch (seq1 seq2 &rest keys)
  ;; Index of first mismatch between SEQ1 and SEQ2, nil if they match.
  ;; Honors :test, :key and :from-end. Faithful to cl-seq.el.
  (let* ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
         (from-end (cl--getkey keys :from-end nil))
         (v1 (vconcat seq1)) (v2 (vconcat seq2))
         (end1 (length v1)) (end2 (length v2)) (start1 0) (start2 0))
    (if from-end
        (progn
          (while (and (< start1 end1) (< start2 end2)
                      (funcall test (funcall key (aref v1 (1- end1)))
                               (funcall key (aref v2 (1- end2)))))
            (setq end1 (1- end1) end2 (1- end2)))
          (and (or (< start1 end1) (< start2 end2)) (1- end1)))
      (while (and (< start1 end1) (< start2 end2)
                  (funcall test (funcall key (aref v1 start1))
                           (funcall key (aref v2 start2))))
        (setq start1 (1+ start1) start2 (1+ start2)))
      (and (or (< start1 end1) (< start2 end2)) start1))))
(defun cl-set-exclusive-or (a b &rest keys)
  (append (apply (function cl-set-difference) a b keys)
          (apply (function cl-set-difference) b a keys)))
(defun cl-nset-exclusive-or (a b &rest keys) (apply (function cl-set-exclusive-or) a b keys))
(defun cl-reduce (f seq &rest keys)
  ;; Supports :initial-value, :key and :from-end. With no initial value and an
  ;; empty SEQ, calls (funcall f) for the identity element.
  (let* ((l (append seq nil))
         (key (plist-get keys :key))
         (has-init (plist-member keys :initial-value))
         (init (plist-get keys :initial-value)))
    (when key (setq l (mapcar (lambda (x) (funcall key x)) l)))
    (if (plist-get keys :from-end)
        (let ((rl (reverse l)) (acc nil))
          (cond (has-init (setq acc init))
                ((null rl) (setq acc (funcall f) rl nil))
                (t (setq acc (car rl) rl (cdr rl))))
          (while rl (setq acc (funcall f (car rl) acc) rl (cdr rl)))
          acc)
      (if has-init (seq-reduce f l init)
        (if (null l) (funcall f) (seq-reduce f (cdr l) (car l)))))))
(defun cl-endp (x) (null x))
(defun cl-subst (new old tree &rest keys)
  ;; With no keywords, substitute OLD (matched by `eql') throughout TREE. When
  ;; :test/:test-not/:key are given, defer to `cl-sublis' exactly like Emacs, so
  ;; the predicate is applied to every node — conses included.
  (if keys
      (apply 'cl-sublis (list (cons old new)) tree keys)
    (cond ((eql tree old) new)
          ((consp tree) (cons (cl-subst new old (car tree)) (cl-subst new old (cdr tree))))
          (t tree))))
(defun cl-sublis (alist tree &rest keys)
  ;; Substitute per ALIST of (OLD . NEW) throughout TREE, honoring :test,
  ;; :test-not and :key against each node (including cons cells).
  (let ((test (cl--getkey keys :test nil))
        (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)))
    (cl--sublis-rec alist tree test test-not key)))
(defun cl--sublis-rec (alist tree test test-not key)
  (let ((keyed (funcall key tree)) (p alist) (hit nil))
    (while (and p (not hit))
      (if (cl--seq-match test test-not (car (car p)) keyed)
          (setq hit p)
        (setq p (cdr p))))
    (if hit (cdr (car hit))
      (if (consp tree)
          (cons (cl--sublis-rec alist (car tree) test test-not key)
                (cl--sublis-rec alist (cdr tree) test test-not key))
        tree))))
;; NOTE: `push'/`dolist' are defined later in this file, so these helpers use
;; explicit `while'/`setq'/`cons' loops to stay valid at load time.
(defun cl-maplist (fn list)
  (let ((r nil))
    (while list (setq r (cons (funcall fn list) r) list (cdr list)))
    (nreverse r)))
(defun cl-stable-sort (seq pred &rest keys) (apply 'cl-sort seq pred keys))
(defun cl-delete-duplicates (seq &rest keys) (apply 'cl-remove-duplicates seq keys))
(defun cl-adjoin (item list &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity)))
    (if (seq-some (lambda (x) (funcall test (funcall key item) (funcall key x))) list)
        list (cons item list))))
;; Faithful ports of Emacs `cl-union'/`cl-intersection'/`cl-set-difference'
;; (cl-seq.el). They honor :test (default `eql') and :key, and reproduce the
;; length-swap + memq/numberp fast path that determines element order. Without
;; keys, non-numeric elements are compared with `memq' (eq), exactly like Emacs.
(defun cl-union (l1 l2 &rest keys)
  (cond ((null l1) l2)
        ((null l2) l1)
        ((and (not keys) (equal l1 l2)) l1)
        (t
         (unless (>= (length l1) (length l2))
           (let ((tmp l1)) (setq l1 l2 l2 tmp)))
         (while l2
           (if (or keys (numberp (car l2)))
               (setq l1 (apply 'cl-adjoin (car l2) l1 keys))
             (or (memq (car l2) l1) (setq l1 (cons (car l2) l1))))
           (setq l2 (cdr l2)))
         l1)))
(defun cl-intersection (l1 l2 &rest keys)
  (and l1 l2
       (if (equal l1 l2) l1
         (let ((key (cl--getkey keys :key 'identity)) (res nil))
           (unless (>= (length l1) (length l2))
             (let ((tmp l1)) (setq l1 l2 l2 tmp)))
           (while l2
             (when (if (or keys (numberp (car l2)))
                       (apply 'cl-member (funcall key (car l2)) l1 keys)
                     (memq (car l2) l1))
               (setq res (cons (car l2) res)))
             (setq l2 (cdr l2)))
           res))))
(defun cl-set-difference (l1 l2 &rest keys)
  (if (or (null l1) (null l2)) l1
    (let ((key (cl--getkey keys :key 'identity)) (res nil))
      (while l1
        (unless (if (or keys (numberp (car l1)))
                    (apply 'cl-member (funcall key (car l1)) l2 keys)
                  (memq (car l1) l2))
          (setq res (cons (car l1) res)))
        (setq l1 (cdr l1)))
      (nreverse res))))
(defun cl-subsetp (l1 l2 &rest keys)
  ;; t when every element of L1 appears in L2 (under :test, with :key applied to
  ;; elements of both lists).
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key nil)) (a l1) (ok t))
    (while (and a ok)
      (let ((x (if key (funcall key (car a)) (car a))))
        (unless (seq-some (lambda (y) (funcall test x (if key (funcall key y) y))) l2)
          (setq ok nil)))
      (setq a (cdr a)))
    ok))
(defun cl-merge (type l1 l2 pred &rest _keys)
  (let ((a (append l1 nil)) (b (append l2 nil)) (r nil))
    (while (and a b)
      (if (funcall pred (car b) (car a)) (setq r (cons (car b) r) b (cdr b))
        (setq r (cons (car a) r) a (cdr a))))
    (setq r (nconc (nreverse r) a b))
    (cond ((eq type 'vector) (vconcat r)) ((eq type 'string) (concat r)) (t r))))

;;; ---- misc functions ----
(defun ignore (&rest _args) nil)
(defun always (&rest _args) t)
;; `default-value'/`set-default' are C primitives (builtins.rs) that read/write
;; the global (default) value cell, bypassing any buffer-local binding.
(defmacro setq-default (&rest args)
  (let ((forms nil))
    (while args
      (setq forms (cons (list 'set-default (list 'quote (car args)) (cadr args)) forms))
      (setq args (cddr args)))
    (cons 'progn (nreverse forms))))
;; Printer limits: special so `(let ((print-length N)) …)` binds them dynamically
;; where the Rust printer can read them.
(defvar print-length nil)
(defvar print-level nil)
(defvar print-escape-newlines nil)
(defvar print-escape-control-characters nil)
(defvar print-quoted t)
(defvar gensym-counter 0)
(defun gensym (&optional prefix)
  (let ((n gensym-counter))
    (setq gensym-counter (1+ gensym-counter))
    (make-symbol (concat (or prefix "g") (number-to-string n)))))
(defun keywordp (x) (and (symbolp x) (let ((n (symbol-name x))) (and (> (length n) 0) (eq (aref n 0) 58)))))

;;; ---- control macros ----
(defmacro prog2 (a b &rest body) (list (quote progn) a (cons (quote prog1) (cons b body))))
(defmacro incf (place &rest amt) `(setf ,place (+ ,place ,(if amt (car amt) 1))))
(defmacro decf (place &rest amt) `(setf ,place (- ,place ,(if amt (car amt) 1))))
;; push: cons X onto PLACE. A plain variable uses setq; a generalized place
;; (e.g. (cdr l), (nth 1 l), (gethash k h)) goes through setf.
(defmacro push (x place)
  (if (symbolp place)
      (list (quote setq) place (list (quote cons) x place))
    (list (quote setf) place (list (quote cons) x place))))
;; setf: assign to a generalized place. Supports a plain variable and the
;; common cons/sequence/hash/symbol places; multiple place/value pairs expand
;; left-to-right.
(defmacro setf (&rest pairs)
  (if (null pairs)
      nil
    (let ((place (car pairs)) (val (cadr pairs)) (rest (cddr pairs)))
      (let ((exp
             (if (consp place)
                 (let ((head (car place)) (args (cdr place)))
                   (cond
                    ((eq head 'car) `(setcar ,(car args) ,val))
                    ((eq head 'cdr) `(setcdr ,(car args) ,val))
                    ((eq head 'caar) `(setcar (car ,(car args)) ,val))
                    ((eq head 'cadr) `(setcar (cdr ,(car args)) ,val))
                    ((eq head 'cddr) `(setcdr (cdr ,(car args)) ,val))
                    ((eq head 'nth) `(setcar (nthcdr ,(car args) ,(cadr args)) ,val))
                    ((eq head 'nthcdr) `(setcdr (nthcdr (1- ,(car args)) ,(cadr args)) ,val))
                    ((eq head 'elt)
                     `(if (listp ,(car args))
                          (setcar (nthcdr ,(cadr args) ,(car args)) ,val)
                        (aset ,(car args) ,(cadr args) ,val)))
                    ((eq head 'aref) `(aset ,(car args) ,(cadr args) ,val))
                    ((eq head 'gethash) `(puthash ,(car args) ,val ,(cadr args)))
                    ((eq head 'symbol-value) `(set ,(car args) ,val))
                    ((eq head 'alist-get) `(setcdr (assq ,(car args) ,(cadr args)) ,val))
                    (t (error "setf: unsupported place: %S" place))))
               `(setq ,place ,val))))
        (if rest `(progn ,exp (setf ,@rest)) exp)))))
(defmacro pop (place)
  (list (quote prog1) (list (quote car) place)
        (if (symbolp place)
            (list (quote setq) place (list (quote cdr) place))
          (list (quote setf) place (list (quote cdr) place)))))
(defmacro dolist (spec &rest body)
  ;; (dolist (VAR LIST [RESULT]) BODY...) — RESULT (with VAR bound to nil) is the
  ;; value of the form; nil if omitted.
  (let ((var (car spec)) (lst (cadr spec)) (result (car (cddr spec))))
    `(let ((,var nil) (--dolist-tail-- ,lst))
       (while --dolist-tail--
         (setq ,var (car --dolist-tail--))
         ,@body
         (setq --dolist-tail-- (cdr --dolist-tail--)))
       (setq ,var nil)
       ,result)))
(defmacro dotimes (spec &rest body)
  ;; (dotimes (VAR COUNT [RESULT]) BODY...) — RESULT (with VAR bound to COUNT) is
  ;; the value of the form; nil if omitted.
  (let ((var (car spec)) (cnt (cadr spec)) (result (car (cddr spec))))
    `(let ((,var 0) (--dotimes-limit-- ,cnt))
       (while (< ,var --dotimes-limit--)
         ,@body
         (setq ,var (1+ ,var)))
       ,result)))

;;; ---- error handling ----
(defmacro ignore-errors (&rest body) `(condition-case nil (progn ,@body) (error nil)))
(defmacro ignore-error (condition &rest body) `(condition-case nil (progn ,@body) (,condition nil)))
(defmacro with-suppressed-warnings (_warnings &rest body) `(progn ,@body))
(defmacro with-no-warnings (&rest body) `(progn ,@body))
;; declare: a no-op at runtime (specs are advisory; defun/lambda ignore them).
(defmacro declare (&rest _specs) nil)
;; declare-function (subr.el:31): a pure byte-compiler hint that FN is defined
;; in FILE; `byte-compile-macroexpand-declare-function' does the real work.  In
;; the interpreter it expands to nil (matching `emacs -Q --batch').
(defmacro declare-function (_fn _file &rest _args)
  "Tell the byte-compiler that function FN is defined, in FILE.
Optional ARGLIST specifies FN's arguments.  Does nothing in the
interpreter; expands to nil."
  (declare (advertised-calling-convention
	    (fn file &optional arglist fileonly) nil))
  nil)
;; Compile-time evaluation: elisprs interprets, so these just run BODY.
(defmacro eval-when-compile (&rest body) (cons 'progn body))
(defmacro eval-and-compile (&rest body) (cons 'progn body))
(defmacro cl-eval-when (situations &rest body)
  ;; Run BODY when a runtime situation (eval/load/:execute/:load-toplevel) applies.
  (if (or (memq 'eval situations) (memq 'load situations)
          (memq :execute situations) (memq :load-toplevel situations))
      (cons 'progn body)
    nil))
(defun byte-code-function-p (_object) nil)
;; compiled-function-p: non-nil for a function whose implementation is compiled.
;; elisprs has no byte-code or native functions, so only primitive subrs qualify
;; (interpreted closures return nil, matching `emacs -Q --batch').
(defun compiled-function-p (object)
  "Return non-nil if OBJECT is a function that is compiled (a primitive subr)."
  (and (subrp object) t))
;; Bound by `macroexpand-all' while it walks a form; libraries (e.g. rx) read it
;; to thread local macro environments.  Defaults to nil outside expansion.
(defvar macroexpand-all-environment nil)
;; macroexp-compiling-p (macroexp.el:141): non-nil only when expanding for the
;; byte-compiler.  elisprs interprets, so `macroexpand-all-environment' never
;; carries the compiler's declare-function cons — this returns nil.
(defun macroexp-compiling-p ()
  "Return non-nil if we're macroexpanding for the compiler."
  (member '(declare-function . byte-compile-macroexpand-declare-function)
          macroexpand-all-environment))
;; cl-lib global declarations (cl-lib.el:252 / cl-macs.el:2645-2686).  Real
;; init files reach these via `(cl-declaim (optimize (speed 3) (safety 0)))'
;; near the top of eieio-core, ede, auth-source, etc.  Faithful port: the
;; `optimize'/`special'/`inline'/`warn' specs all target byte-compiler state
;; that elisprs (an interpreter) never consults, so `cl--do-proclaim' records
;; the optimize levels and otherwise no-ops.
(defvar cl--proclaims-deferred nil)
(defvar cl--proclaim-history t)
(defvar cl--optimize-safety)
(defvar cl--optimize-speed)
(defun cl--do-proclaim (spec hist)
  (and hist (listp cl--proclaim-history) (push spec cl--proclaim-history))
  (cond ((eq (car-safe spec) 'special)
	 (if (boundp 'byte-compile-bound-variables)
	     (setq byte-compile-bound-variables
		   (append (cdr spec) byte-compile-bound-variables))))

	((eq (car-safe spec) 'inline)
	 (while (setq spec (cdr spec))
	   (or (memq (get (car spec) 'byte-optimizer)
		     '(nil byte-compile-inline-expand))
	       (error "%s already has a byte-optimizer, can't make it inline"
		      (car spec)))
	   (put (car spec) 'byte-optimizer #'byte-compile-inline-expand)))

	((eq (car-safe spec) 'notinline)
	 (while (setq spec (cdr spec))
	   (if (eq (get (car spec) 'byte-optimizer)
		   #'byte-compile-inline-expand)
	       (put (car spec) 'byte-optimizer nil))))

	((eq (car-safe spec) 'optimize)
	 (let ((speed (assq (nth 1 (assq 'speed (cdr spec)))
			    '((0 nil) (1 t) (2 t) (3 t))))
	       (safety (assq (nth 1 (assq 'safety (cdr spec)))
			     '((0 t) (1 nil) (2 nil) (3 nil)))))
	   (if speed (setq cl--optimize-speed (car speed)
			   byte-optimize (nth 1 speed)))
	   (if safety (setq cl--optimize-safety (car safety)
			    byte-compile-delete-errors (nth 1 safety)))))

	((and (eq (car-safe spec) 'warn) (boundp 'byte-compile-warnings))
	 (while (setq spec (cdr spec))
	   (if (consp (car spec))
               (if (eq (cadar spec) 0)
                   (byte-compile-disable-warning (caar spec))
                 (byte-compile-enable-warning (caar spec)))))))
  nil)
(defun cl-proclaim (spec)
  "Record a global declaration specified by SPEC."
  (if (fboundp 'cl--do-proclaim) (cl--do-proclaim spec t)
    (push spec cl--proclaims-deferred))
  nil)
(defmacro cl-declaim (&rest specs)
  "Like `cl-proclaim', but takes any number of unevaluated, unquoted arguments.
Puts `(cl-eval-when (compile load eval) ...)' around the declarations
so that they are registered at compile-time as well as run-time."
  (let ((body (mapcar (lambda (x) `(cl-proclaim ',x)) specs)))
    (if (macroexp-compiling-p) `(cl-eval-when (compile load eval) ,@body)
      `(progn ,@body))))
;; A form that evaluates to V (self-quoting literals as-is, else (quote V)).
(defun macroexp-quote (v)
  (if (and (not (consp v)) (or (not (symbolp v)) (null v) (eq v t) (keywordp v)))
      v
    (list 'quote v)))
;; macroexp const-ness predicates (macroexp.el:682).  Used by cl-macs/cl-generic
;; to decide when a subform may be duplicated or optimized away.
(defvar byte-compile-const-variables nil)
(defun macroexp--const-symbol-p (symbol &optional any-value)
  "Non-nil if SYMBOL is constant.
If ANY-VALUE is nil, only return non-nil if the value of the symbol is the
symbol itself."
  (or (memq symbol '(nil t))
      (keywordp symbol)
      (if any-value
          (or (memq symbol byte-compile-const-variables)
              (and (boundp symbol)
                   (condition-case nil
                       (progn (set symbol (symbol-value symbol)) nil)
                     (setting-constant t)))))))
(defun macroexp-const-p (exp)
  "Return non-nil if EXP will always evaluate to the same value."
  (cond ((consp exp) (or (eq (car exp) 'quote)
                         (and (eq (car exp) 'function)
                              (symbolp (cadr exp)))))
        ((symbolp exp) (macroexp--const-symbol-p exp))
        (t t)))
(defun macroexp-copyable-p (exp)
  "Return non-nil if EXP can be copied without extra cost."
  (or (symbolp exp) (macroexp-const-p exp)))
;; defsubst: define an inline function.  In the interpreter this is exactly a
;; `defun'; the `byte-optimizer' property is what the byte-compiler consults to
;; inline calls (byte-run.el:481).  elisprs has no byte-compiler, so the
;; property is set for faithfulness but never acted upon.
(defmacro defsubst (name arglist &rest body)
  "Define an inline function.  The syntax is just like that of `defun'."
  (declare (debug defun) (doc-string 3) (indent 2))
  `(prog1
       (defun ,name ,arglist ,@body)
     (put ',name 'byte-optimizer 'byte-compile-inline-expand)))
;; purecopy: copy an object into pure space.  elisprs has no pure space, so this
;; returns its argument unchanged — observably equal to Emacs's result.
(defun purecopy (object)
  "Return a copy of OBJECT (identity — elisprs has no pure space)."
  object)
;; autoloadp / autoload: mark a symbol to load its defining file on first call
;; (subr.el `autoloadp'; C `Fautoload').  elisprs installs the autoload object
;; faithfully; the on-call load trigger is the autoload subsystem (not wired).
(defun autoloadp (object)
  "Non-nil if OBJECT is an autoload."
  (eq 'autoload (car-safe object)))
(defun autoload (function file &optional docstring interactive type)
  "Define FUNCTION to autoload from FILE.
Does nothing if FUNCTION is already defined as something other than an
autoload.  Returns FUNCTION when it installs the autoload, else nil."
  (if (and (fboundp function)
           (not (autoloadp (symbol-function function))))
      nil
    (fset function (list 'autoload file docstring interactive type))
    function))
;; Advertised calling convention (byte-run.el:498).  Records the SIGNATURE a
;; function advertises to the byte-compiler; cl-generic's `cl-generic-define-method'
;; preserves a generic's advertised signature across redefinition.
(defvar advertised-signature-table (make-hash-table :test 'eq :weakness 'key))
(defun set-advertised-calling-convention (function signature _when)
  "Set the advertised SIGNATURE of FUNCTION."
  (puthash (indirect-function function) signature
           advertised-signature-table))
(defun get-advertised-calling-convention (function)
  "Get the advertised SIGNATURE of FUNCTION.
Return t if there isn't any."
  (gethash function advertised-signature-table t))
;; make-obsolete family (byte-run.el).  These only record properties the
;; byte-compiler reads to emit warnings; in the interpreter they are inert but
;; ported so obsolete declarations in real libraries load cleanly.
(defun byte-run--constant-obsolete-warning (obsolete-name)
  (if (memq obsolete-name '(nil t))
      (error "Can't make `%s' obsolete; did you forget a quote mark?"
             obsolete-name)))
(defun make-obsolete (obsolete-name current-name when)
  "Make the byte-compiler warn that function OBSOLETE-NAME is obsolete."
  (byte-run--constant-obsolete-warning obsolete-name)
  (put obsolete-name 'byte-obsolete-info
       (purecopy (list current-name nil when)))
  obsolete-name)
(defun make-obsolete-variable (obsolete-name current-name when &optional access-type)
  "Make the byte-compiler warn that OBSOLETE-NAME is obsolete."
  (byte-run--constant-obsolete-warning obsolete-name)
  (put obsolete-name 'byte-obsolete-variable
       (purecopy (list current-name access-type when)))
  obsolete-name)
(defmacro define-obsolete-function-alias (obsolete-name current-name when &optional docstring)
  "Set OBSOLETE-NAME's function definition to CURRENT-NAME and mark it obsolete."
  (declare (doc-string 4) (indent defun))
  `(progn
     (defalias ,obsolete-name ,current-name ,docstring)
     (make-obsolete ,obsolete-name ,current-name ,when)))
(defmacro define-obsolete-variable-alias (obsolete-name current-name when &optional docstring)
  "Make OBSOLETE-NAME a variable alias for CURRENT-NAME and mark it obsolete.
Uses `defvaralias' and `make-obsolete-variable' (byte-run.el)."
  (declare (doc-string 4) (indent defun))
  `(progn
     (defvaralias ,obsolete-name ,current-name ,docstring)
     ;; See Bug#4706.
     (dolist (prop '(saved-value saved-variable-comment))
       (and (get ,obsolete-name prop)
            (null (get ,current-name prop))
            (put ,current-name prop (get ,obsolete-name prop))))
     (make-obsolete-variable ,obsolete-name ,current-name ,when)))
;; eval-after-load (subr.el): register FORM to run after FILE loads; run now if
;; FILE (a feature symbol) is already provided.  The string-file regexp path and
;; the fire-on-future-load path are the after-load subsystem (not wired); the
;; feature-symbol registration path used by real libraries is faithful.
(defvar after-load-alist nil)
(defun eval-after-load (file form)
  "Arrange that if FILE is loaded, FORM will be run immediately afterwards."
  (declare (indent 1))
  (let* ((elt (assoc file after-load-alist))
         (func (if (functionp form) form (eval (list 'lambda nil form) t))))
    (unless elt
      (setq elt (list file))
      (push elt after-load-alist))
    (when (and (symbolp file) (featurep file))
      (funcall func))))
(defmacro with-eval-after-load (file &rest body)
  "Execute BODY after FILE is loaded (see `eval-after-load')."
  (declare (indent 1) (debug (form def-body)))
  (list 'eval-after-load file (list 'quote (cons 'progn body))))
;; def-edebug-elem-spec (subr.el): record an Edebug spec element as a property.
;; Edebug is advisory in elisprs, but real libraries register specs at load time.
(defun def-edebug-elem-spec (name spec)
  "Define a new Edebug spec element NAME as shorthand for SPEC."
  (declare (indent 1))
  (when (string-match "\\`[&:]" (symbol-name name))
    (error "Edebug spec name cannot start with '&' or ':'"))
  (unless (consp spec)
    (error "Edebug spec has to be a list: %S" spec))
  (put name 'edebug-elem-spec spec))
;; Declaration handler alists (byte-run.el).  Declared nil here (before the
;; handler functions exist) and populated further below; the host's defun/defmacro
;; expander consults them via `elisprs--expand-defun-declarations' to process
;; `(declare ...)' specs with runtime effect (gv-setter, obsolete, indent, …).
(defvar defun-declarations-alist nil)
(defvar macro-declarations-alist nil)
(defmacro defvar-local (var val &optional doc)
  `(progn (defvar ,var ,val ,doc) (make-variable-buffer-local ',var)))
(defmacro with-demoted-errors (fmt &rest body) `(condition-case --err-- (progn ,@body) (error (message ,fmt --err--) nil)))
;; with-output-to-string: capture princ/prin1/print/terpri output into a string.
;; (No buffer model — standard-output redirection isn't supported; this captures
;; the standard print builtins via an output-capture stack in the host.)
(defmacro with-output-to-string (&rest body)
  `(let ((--wots-- nil))
     (--push-output-capture--)
     (unwind-protect (progn ,@body)
       (setq --wots-- (--pop-output-capture--)))
     --wots--))

;; Evaluate BODY with the regexp match data preserved: any `string-match` inside
;; BODY won't clobber the caller's match state.
(defmacro save-match-data (&rest body)
  `(let ((--save-match-- (match-data)))
     (unwind-protect (progn ,@body)
       (set-match-data --save-match--))))

;;; ====================================================================
;;; Standard library — subr / subr-x / seq / cl-lib written in elisp on
;;; top of the primitives, the way Emacs bootstraps. (Buffer/marker ops,
;;; floats→int, and function-cell introspection need host primitives and
;;; are not included.)
;;; ====================================================================

;;; ---- predicates ----
(defun booleanp (x) (if (or (eq x t) (eq x nil)) t nil))
(defun characterp (x) (and (integerp x) (>= x 0) (<= x #x3FFFFF)))
(defun sequencep (x) (or (listp x) (vectorp x) (stringp x) (char-table-p x)))
(defun arrayp (x) (or (vectorp x) (stringp x) (char-table-p x)))
(defun string-or-null-p (x) (or (null x) (stringp x)))
(defun nlistp (x) (not (listp x)))
;; ROT13: rotate ASCII letters by 13, leaving everything else unchanged.
(defun rot13-string (string)
  (mapconcat (lambda (c)
               (char-to-string
                (cond ((and (>= c ?a) (<= c ?z)) (+ ?a (% (+ (- c ?a) 13) 26)))
                      ((and (>= c ?A) (<= c ?Z)) (+ ?A (% (+ (- c ?A) 13) 26)))
                      (t c))))
             (append string nil) ""))
;; elisprs has no symbols-with-position, so a symbol is always "bare".
(defun symbol-with-pos-p (_x) nil)
(defun bare-symbol (sym) sym)
(defun xor (a b) (cond ((not a) b) ((not b) a) (t nil)))
(defun proper-list-p (x)
  ;; Length if X is a proper (nil-terminated, acyclic) list, else nil. Floyd
  ;; cycle detection so a circular list returns nil instead of looping.
  (let ((slow x) (fast x) (n 0))
    (catch 'proper-list-p--done
      (while t
        (unless (consp fast) (throw 'proper-list-p--done (if (null fast) n nil)))
        (setq fast (cdr fast) n (1+ n))
        (unless (consp fast) (throw 'proper-list-p--done (if (null fast) n nil)))
        (setq fast (cdr fast) n (1+ n) slow (cdr slow))
        (when (eq fast slow) (throw 'proper-list-p--done nil))))))

;;; ---- numbers ----
;; `expt` is a primitive subr (integer power; float for fractional/negative exp).
(defun gcd (a b)
  (setq a (abs a)) (setq b (abs b))
  (while (> b 0) (let ((tmp b)) (setq b (% a b)) (setq a tmp)))
  a)
(defun lcm (a b) (if (or (= a 0) (= b 0)) 0 (/ (abs (* a b)) (gcd a b))))
(defun isqrt (n) (let ((r 0)) (while (<= (* (1+ r) (1+ r)) n) (setq r (1+ r))) r))
(defun cl-signum (x) (cond ((> x 0) 1) ((< x 0) -1) (t 0)))
(defun cl-evenp (n)
  (unless (integerp n) (signal 'wrong-type-argument (list 'integer-or-marker-p n)))
  (= (% n 2) 0))
(defun cl-oddp (n)
  (unless (integerp n) (signal 'wrong-type-argument (list 'integer-or-marker-p n)))
  (/= (% n 2) 0))
;; Two-value division: each returns (QUOTIENT REMAINDER) where the remainder is
;; X - QUOTIENT*Y, matching the single-value floor/ceiling/truncate/round builtins.
(defun cl-floor (x &optional y)
  (let* ((d (or y 1)) (q (floor x d))) (list q (- x (* q d)))))
(defun cl-ceiling (x &optional y)
  (let* ((d (or y 1)) (q (ceiling x d))) (list q (- x (* q d)))))
(defun cl-truncate (x &optional y)
  (let* ((d (or y 1)) (q (truncate x d))) (list q (- x (* q d)))))
(defun cl-round (x &optional y)
  (let* ((d (or y 1)) (q (round x d))) (list q (- x (* q d)))))
(defun cl-mod (x y) (mod x y))
(defun cl-rem (x y) (% x y))
(defun cl-gcd (&rest ns)
  (let ((g 0))
    (dolist (n ns) (setq g (cl--gcd2 g (abs n))))
    g))
(defun cl--gcd2 (a b) (while (/= b 0) (let ((r (% a b))) (setq a b b r))) a)
(defun cl-lcm (&rest ns)
  (let ((l 1))
    (catch 'zero
      (dolist (n ns)
        (if (= n 0) (throw 'zero 0)
          (setq l (/ (* l (abs n)) (cl--gcd2 l (abs n))))))
      l)))
(defun cl-parse-integer (string &rest keys)
  (string-to-number (string-trim string) (or (plist-get keys :radix) 10)))
(defun cl-coerce (object type)
  (cond ((eq type 'list) (append object nil))
        ((memq type '(vector array simple-vector)) (vconcat object))
        ((eq type 'string) (concat object))
        ((eq type 'float) (float object))
        ((eq type 'character) object)
        (t object)))
(defvar cl--gensym-counter 0)
(defun cl-gensym (&optional prefix)
  (prog1 (make-symbol (concat (or prefix "G") (number-to-string cl--gensym-counter)))
    (setq cl--gensym-counter (1+ cl--gensym-counter))))
(defun cl-digit-char-p (char &optional radix)
  (let ((r (or radix 10))
        (v (cond ((and (>= char ?0) (<= char ?9)) (- char ?0))
                 ((and (>= char ?a) (<= char ?z)) (+ 10 (- char ?a)))
                 ((and (>= char ?A) (<= char ?Z)) (+ 10 (- char ?A)))
                 (t nil))))
    (if (and v (< v r)) v nil)))
(defun cl-isqrt (n)
  (if (< n 2) n
    (let ((x n) (y (/ (+ n 1) 2)))
      (while (< y x) (setq x y y (/ (+ x (/ n x)) 2)))
      x)))
;; `string-to-number` is a primitive subr (floats, scientific notation, BASE arg).

;;; ---- key sequences (kbd) ----
;; Control-fold a character: letters/@A-Z[\]^_ map to 0–31, ? to 127, else the
;; control bit (2^26) is set.
(defun kbd--ctrl (c)
  (cond ((and (>= c ?a) (<= c ?z)) (- c 96))
        ((eq c ??) 127)
        ((and (>= c ?@) (<= c ?_)) (- c ?@))
        (t (logior c (ash 1 26)))))
;; Apply modifier flags to a character code (returns an int, possibly with
;; meta/shift/hyper/super/alt bits set).
(defun kbd--char (c ctrl meta shift hyper super alt)
  (when ctrl (setq c (kbd--ctrl c)))
  (when shift (setq c (logior c (ash 1 25))))
  (when hyper (setq c (logior c (ash 1 24))))
  (when super (setq c (logior c (ash 1 23))))
  (when alt (setq c (logior c (ash 1 22))))
  (when meta (setq c (logior c (ash 1 27))))
  c)
;; Build a function-key symbol, prefixing modifiers in Emacs's canonical
;; A-C-H-M-S-s order.
(defun kbd--sym (name ctrl meta shift hyper super alt)
  (when super (setq name (concat "s-" name)))
  (when shift (setq name (concat "S-" name)))
  (when meta (setq name (concat "M-" name)))
  (when hyper (setq name (concat "H-" name)))
  (when ctrl (setq name (concat "C-" name)))
  (when alt (setq name (concat "A-" name)))
  (intern name))
;; Parse one whitespace-delimited key token into a list of key codes/symbols.
(defun kbd--token (tok)
  (let ((ctrl nil) (meta nil) (shift nil) (hyper nil) (super nil) (alt nil)
        (i 0) (n (length tok)) (hadmod nil))
    (while (and (< (1+ i) n) (eq (aref tok (1+ i)) ?-) (memq (aref tok i) '(?C ?M ?S ?H ?s ?A)))
      (setq hadmod t)
      (let ((m (aref tok i)))
        (cond ((eq m ?C) (setq ctrl t)) ((eq m ?M) (setq meta t)) ((eq m ?S) (setq shift t))
              ((eq m ?H) (setq hyper t)) ((eq m ?s) (setq super t)) ((eq m ?A) (setq alt t))))
      (setq i (+ i 2)))
    (let* ((rest (substring tok i)) (rn (length rest))
           (named (cond ((string= rest "RET") 13) ((string= rest "TAB") 9) ((string= rest "SPC") 32)
                        ((string= rest "ESC") 27) ((string= rest "DEL") 127) ((string= rest "LFD") 10)
                        ((string= rest "NUL") 0) (t nil)))
           (anglesym (and (> rn 1) (eq (aref rest 0) ?<) (eq (aref rest (1- rn)) ?>))))
      (cond
       (anglesym (list (kbd--sym (substring rest 1 (1- rn)) ctrl meta shift hyper super alt)))
       (named (list (kbd--char named ctrl meta shift hyper super alt)))
       ;; A plain multi-character token is a sequence of single-character keys.
       ((and (not hadmod) (> rn 1)) (append rest nil))
       ((= rn 1) (list (kbd--char (aref rest 0) ctrl meta shift hyper super alt)))
       (t (list (kbd--sym rest ctrl meta shift hyper super alt)))))))
(defun kbd (keys)
  "Parse a key-description string into a key sequence (string or vector)."
  (let ((codes nil))
    (dolist (tok (split-string keys " " t))
      (setq codes (append codes (kbd--token tok))))
    (if (cl-every (lambda (c) (and (integerp c) (>= c 0) (<= c 127))) codes)
        (apply #'string codes)
      (vconcat codes))))
;; Describe a function-key symbol name ("C-f1" → "C-<f1>"), wrapping the base in
;; angle brackets and keeping any modifier prefixes.
(defun kd--sym (name)
  (let ((i 0) (n (length name)) (mods ""))
    (while (and (< (1+ i) n) (eq (aref name (1+ i)) ?-) (memq (aref name i) '(?C ?M ?S ?H ?s ?A)))
      (setq mods (concat mods (substring name i (+ i 2))))
      (setq i (+ i 2)))
    (concat mods "<" (substring name i) ">")))
(defun single-key-description (key &optional _no-angles)
  "Return a textual description of KEY (an event: integer or symbol)."
  (if (symbolp key)
      (kd--sym (symbol-name key))
    (let* ((alt (/= 0 (logand key (ash 1 22))))
           (super (/= 0 (logand key (ash 1 23))))
           (hyper (/= 0 (logand key (ash 1 24))))
           (shift (/= 0 (logand key (ash 1 25))))
           (meta (/= 0 (logand key (ash 1 27))))
           ;; Strip the non-control modifier bits; what's left holds the base
           ;; char and possibly the explicit control bit (2^26).
           (c (logand key (lognot (+ (ash 1 22) (ash 1 23) (ash 1 24) (ash 1 25) (ash 1 27)))))
           (ctrl nil))
      ;; Control: explicit 2^26 bit, or a folded control char (< 32, not named).
      (cond ((/= 0 (logand c (ash 1 26))) (setq ctrl t c (logand c (lognot (ash 1 26)))))
            ((and (< c 32) (not (memq c '(9 13 27 10))))
             (setq ctrl t c (if (and (>= c 1) (<= c 26)) (+ c 96) (+ c 64)))))
      (let ((base (cond ((eq c 9) "TAB") ((eq c 13) "RET") ((eq c 27) "ESC")
                        ((eq c 10) "LFD") ((eq c 32) "SPC") ((eq c 127) "DEL")
                        (t (char-to-string c)))))
        ;; Modifier prefixes in Emacs's canonical A-C-H-M-S-s order.
        (concat (if alt "A-" "") (if ctrl "C-" "") (if hyper "H-" "") (if meta "M-" "")
                (if shift "S-" "") (if super "s-" "") base)))))
(defun kd--add-meta (e)
  (if (integerp e) (logior e (ash 1 27)) (intern (concat "M-" (symbol-name e)))))
(defun key-description (keys &optional _prefix)
  "Return a textual description of the key sequence KEYS (a string or vector)."
  ;; An ESC (27) prefixing another event collapses into a Meta modifier.
  (let ((evs (append keys nil)) (parts nil))
    (while evs
      (let ((e (car evs)))
        (if (and (eq e 27) (cdr evs))
            (progn (setq evs (cdr evs))
                   (push (single-key-description (kd--add-meta (car evs))) parts))
          (push (single-key-description e) parts)))
      (setq evs (cdr evs)))
    (mapconcat #'identity (nreverse parts) " ")))

;;; ---- strings (ASCII) ----
;; Emacs's string comparators accept symbols too, using their print names.
(defun string--name (x) (if (symbolp x) (symbol-name x) x))
(defun string= (a b)
  ;; Emacs accepts a string or a symbol on either side and signals `stringp' on
  ;; anything else -- it does not quietly answer nil for, say, a float.
  (unless (or (stringp a) (symbolp a)) (signal 'wrong-type-argument (list 'stringp a)))
  (unless (or (stringp b) (symbolp b)) (signal 'wrong-type-argument (list 'stringp b)))
  (equal (string--name a) (string--name b)))
(defun string-equal (a b) (string= a b))
(defun string< (a b)
  (let ((la (string-to-list (string--name a))) (lb (string-to-list (string--name b)))
        (res nil) (done nil))
    (while (not done)
      (cond ((null la) (setq res (not (null lb))) (setq done t))
            ((null lb) (setq res nil) (setq done t))
            ((< (car la) (car lb)) (setq res t) (setq done t))
            ((> (car la) (car lb)) (setq res nil) (setq done t))
            (t (setq la (cdr la)) (setq lb (cdr lb)))))
    res))
(defun string-lessp (a b) (string< a b))
(defun string-greaterp (a b) (string< b a))
(defun string> (a b) (string< b a))
;; Collation: no locale support, so this is ordinary string order (with optional
;; case folding), matching the C-locale default.
(defun string-collate-lessp (s1 s2 &optional _locale ignore-case)
  (if ignore-case (string-lessp (downcase s1) (downcase s2)) (string-lessp s1 s2)))
(defun string-collate-equalp (s1 s2 &optional _locale ignore-case)
  (if ignore-case (string-equal (downcase s1) (downcase s2)) (string-equal s1 s2)))
;; Curly quotes are always displayable here, so the effective style is `curve'.
(defun text-quoting-style () 'curve)
(defun string-reverse (s) (reverse s))
;; `upcase` / `downcase` are primitive subrs (accept a string or a character).
(defun char-or-string--check (s)
  (unless (or (stringp s) (integerp s))
    (signal 'wrong-type-argument (list 'char-or-string-p s)))
  s)
(defun capitalize (s)
  (char-or-string--check s)
  ;; Upcase the first letter of every word (run of alphanumerics), downcase the
  ;; rest: (capitalize "hello world") => "Hello World". A character argument
  ;; capitalizes to its uppercase form (like `upcase').
  (if (integerp s) (upcase s)
  (let ((out nil) (in-word nil))
    (dolist (c (string-to-list s))
      ;; Word constituent: a digit, or a cased letter (upcase ≠ downcase) —
      ;; the latter covers non-ASCII letters like é/ÿ too.
      (let ((wordc (or (and (>= c ?0) (<= c ?9)) (/= (downcase c) (upcase c)))))
        (cond
         ((not wordc) (setq out (cons c out)))
         (in-word (setq out (cons (downcase c) out)))
         (t (setq out (cons (upcase c) out))))
        (setq in-word wordc)))
    (apply (function string) (reverse out)))))
(defun string-trim-left (s &optional regexp)
  ;; Strip a leading match of REGEXP (default whitespace).
  (let ((re (concat "\\`\\(?:" (or regexp "[ \t\n\r]+") "\\)")))
    (if (string-match re s) (substring s (match-end 0)) s)))
(defun string-trim-right (s &optional regexp)
  ;; Strip a trailing match of REGEXP (default whitespace).
  (let ((re (concat "\\(?:" (or regexp "[ \t\n\r]+") "\\)\\'")))
    (if (string-match re s) (substring s 0 (match-beginning 0)) s)))
(defun string-trim (s &optional trim-left trim-right)
  (string-trim-right (string-trim-left s trim-left) trim-right))
(defun string-blank-p (s) (string-match-p "\\`[ \t\n\r]*\\'" s))
(defun string-clean-whitespace (s)
  ;; Collapse internal whitespace runs to a single space and trim the ends.
  (string-trim (replace-regexp-in-string "[ \t\n\r]+" " " s)))
(defun string-truncate-left (string length)
  ;; If STRING is longer than LENGTH, keep the rightmost characters and prepend
  ;; "..." (so the result may exceed the original when LENGTH is <= 3).
  (let ((strlen (length string)))
    (if (<= strlen length)
        string
      (setq length (max 0 (- length 3)))
      (concat "..." (substring string (min (1- strlen)
                                           (max 0 (- strlen length))))))))
(defun string-remove-prefix (prefix s)
  (if (string-prefix-p prefix s) (substring s (length prefix) (length s)) s))
(defun string-remove-suffix (suffix s)
  (if (string-suffix-p suffix s) (substring s 0 (- (length s) (length suffix))) s))

;;; ---- lists ----
(defun butlast (lst &optional n)
  ;; All but the last N elements of LST (default 1): (butlast '(1 2 3) 2) => (1).
  (setq n (or n 1))
  ;; Negative or zero N keeps the whole list; clamp so we never index past the end.
  (let ((keep (min (length lst) (- (length lst) n))) (r nil) (i 0))
    (while (< i keep) (setq r (cons (nth i lst) r)) (setq i (1+ i)))
    (reverse r)))
(defun nbutlast (lst &optional n)
  ;; Destructively drop the last N (default 1) elements.
  (let ((n (or n 1)) (len (length lst)))
    (cond ((<= len n) nil)
          ((<= n 0) lst)
          (t (setcdr (nthcdr (- len n 1) lst) nil) lst))))
(defun take (n lst)
  (let ((out nil))
    (while (and (> n 0) lst) (setq out (cons (car lst) out)) (setq lst (cdr lst)) (setq n (1- n)))
    (reverse out)))
(defun ntake (n lst) (take n lst))
(defun flatten-tree (tree)
  (cond ((null tree) nil)
        ((consp tree) (append (flatten-tree (car tree)) (flatten-tree (cdr tree))))
        (t (list tree))))
(defun flatten-list (tree) (flatten-tree tree))
(defun copy-tree (tree) (if (consp tree) (cons (copy-tree (car tree)) (copy-tree (cdr tree))) tree))
(defun copy-sequence (seq) (if (listp seq) (append seq nil) seq))
(defun ensure-list (x) (if (listp x) x (list x)))
(defun mapcan (fn lst) (apply (function append) (mapcar fn lst)))
(defun assoc-default (key alist &optional test default)
  ;; Faithful port of subr.el: each element (or its car) is compared to KEY via
  ;; (funcall (or TEST 'equal) ELEM KEY); on a hit return its cdr, else DEFAULT.
  (let (found (tail alist) value)
    (while (and tail (not found))
      (let ((elt (car tail)))
        (when (funcall (or test 'equal) (if (consp elt) (car elt) elt) key)
          (setq found t value (if (consp elt) (cdr elt) default))))
      (setq tail (cdr tail)))
    value))
(defun rassoc (val alist)
  (let ((res nil))
    (while (and alist (not res))
      (if (and (consp (car alist)) (equal (cdr (car alist)) val)) (setq res (car alist)))
      (setq alist (cdr alist)))
    res))
(defun assq-delete-all (key alist)
  (let ((out nil))
    (while alist
      (unless (and (consp (car alist)) (eq (car (car alist)) key)) (setq out (cons (car alist) out)))
      (setq alist (cdr alist)))
    (reverse out)))
(defun assoc-delete-all (key alist &optional test)
  ;; Like assq-delete-all but compares with TEST (default `equal').
  (let ((tf (or test #'equal)) (out nil))
    (while alist
      (unless (and (consp (car alist)) (funcall tf (car (car alist)) key))
        (setq out (cons (car alist) out)))
      (setq alist (cdr alist)))
    (reverse out)))
;; Minimal completion API over a list (plain or alist), or a hash-table's keys.
;; (Function collections, obarrays, and completion-ignore-case are unsupported.)
(defun completion--str (e)
  (let ((c (if (consp e) (car e) e)))
    (cond ((symbolp c) (symbol-name c)) ((stringp c) c) (t (format "%s" c)))))
(defun completion--common-prefix (a b)
  (let ((n (min (length a) (length b))) (i 0))
    (while (and (< i n) (eq (aref a i) (aref b i))) (setq i (1+ i)))
    (substring a 0 i)))
(defun completion--elements (collection)
  (if (hash-table-p collection) (hash-table-keys collection) collection))
(defun all-completions (string collection &optional predicate)
  (let ((out nil))
    (dolist (e (completion--elements collection))
      (let ((c (completion--str e)))
        (when (and (string-prefix-p string c) (or (null predicate) (funcall predicate e)))
          (setq out (cons c out)))))
    (nreverse out)))
(defun try-completion (string collection &optional predicate)
  (let ((cands (all-completions string collection predicate)))
    (cond
     ((null cands) nil)
     ((and (null (cdr cands)) (string= (car cands) string)) t)
     (t (let ((common (car cands)))
          (dolist (c (cdr cands)) (setq common (completion--common-prefix common c)))
          common)))))
(defun test-completion (string collection &optional predicate)
  (let ((res nil))
    (dolist (e (completion--elements collection))
      (when (and (string= (completion--str e) string) (or (null predicate) (funcall predicate e)))
        (setq res t)))
    res))
(defun nreverse (seq)
  ;; Reverse a list in place by relinking its cons cells; other sequences copy.
  (if (listp seq)
      (let ((prev nil) (cur seq))
        (while (consp cur)
          (let ((next (cdr cur)))
            (setcdr cur prev)
            (setq prev cur cur next)))
        prev)
    ;; Non-list: only arrays are reversible in place; else signal arrayp (Emacs).
    (if (arrayp seq) (reverse seq)
      (signal 'wrong-type-argument (list 'arrayp seq)))))

;;; ---- sort (stable merge sort, lists) ----
(defun std--merge (a b pred)
  (cond ((null a) b)
        ((null b) a)
        ((funcall pred (car b) (car a)) (cons (car b) (std--merge a (cdr b) pred)))
        (t (cons (car a) (std--merge (cdr a) b pred)))))
(defun std--halves (lst)
  (let ((slow lst) (fast lst) (front nil))
    (while (and fast (cdr fast))
      (setq front (cons (car slow) front)) (setq slow (cdr slow)) (setq fast (cdr (cdr fast))))
    (cons (reverse front) slow)))
(defun sort (lst pred)
  (if (or (null lst) (null (cdr lst))) lst
    (let ((h (std--halves lst)))
      (std--merge (sort (car h) pred) (sort (cdr h) pred) pred))))

;;; ---- seq.el (list-oriented) ----
;; seq-generic: coerce to a list, then restore SEQ's own type (vector/string).
(defun seq-take (seq n)
  (unless (number-or-marker-p n) (signal 'wrong-type-argument (list 'number-or-marker-p n)))
  (seq-into (take n (append seq nil)) (seq--type-of seq)))
(defun seq-drop (seq n)
  (unless (number-or-marker-p n) (signal 'wrong-type-argument (list 'number-or-marker-p n)))
  (seq-into (nthcdr n (append seq nil)) (seq--type-of seq)))
(defun seq-subseq (seq start &optional end)
  ;; Sequence-generic, returning SEQ's type; START/END may be negative.
  ;; Out-of-range is an error, and seq.el reports it differently per type: an
  ;; array signals `args-out-of-range', a list signals a plain `error'.
  (unless (sequencep seq)
    (error "Unsupported sequence: %S" seq))
  (let* ((lst (append seq nil)) (len (length lst))
         (s (if (< start 0) (+ len start) start))
         (e (cond ((null end) len) ((< end 0) (+ len end)) (t end))))
    (when (or (< s 0) (> s len) (< e 0) (> e len) (> s e))
      (if (listp seq)
          (error "Start index out of bounds: %d" start)
        (signal 'args-out-of-range (list seq start end))))
    (let ((sub (take (- e s) (nthcdr s lst))))
      (cond ((stringp seq) (apply (function string) sub))
            ((vectorp seq) (vconcat sub))
            (t sub)))))
(defun seq-uniq (seq &optional testfn)
  (if (null testfn)
      (delete-dups (append seq nil))
    (let ((out nil))
      (dolist (x (append seq nil))
        (unless (let ((f nil)) (dolist (y out) (when (funcall testfn x y) (setq f t))) f)
          (setq out (cons x out))))
      (nreverse out))))
(defun seq-min (seq) (apply (function min) (append seq nil)))
(defun seq-max (seq) (apply (function max) (append seq nil)))
(defun seq-first (seq) (elt seq 0))
(defun seq-rest (seq) (seq-drop seq 1))
(defun seq-position (seq elt &optional testfn)
  (let ((test (or testfn #'equal)) (i 0) (res nil))
    (setq seq (append seq nil))
    (while (and seq (null res)) (if (funcall test (car seq) elt) (setq res i)) (setq seq (cdr seq)) (setq i (1+ i)))
    res))
(defun seq-into (seq type)
  ;; Coerce SEQ (list/vector/string) to a list first so any input type converts.
  (let ((l (append seq nil)))
    (cond ((eq type 'list) l)
          ((eq type 'vector) (apply (function vector) l))
          ((eq type 'string) (apply (function string) l))
          (t seq))))
(defun seq-difference (a b &optional testfn)
  (seq-filter (lambda (x) (not (seq-contains-p b x testfn))) a))
(defun seq-intersection (a b &optional testfn)
  (seq-filter (lambda (x) (seq-contains-p b x testfn)) a))
(defun seq-union (a b &optional testfn)
  ;; Emacs reduces an accumulator over both sequences, skipping an element it
  ;; already holds -- so duplicates *within* A are dropped too:
  ;; (seq-union '(1 1 2) '(2 3)) => (1 2 3), not (1 1 2 3).
  (let ((acc nil))
    (dolist (e (append a nil))
      (unless (seq-contains-p acc e testfn) (setq acc (cons e acc))))
    (dolist (e (append b nil))
      (unless (seq-contains-p acc e testfn) (setq acc (cons e acc))))
    (nreverse acc)))
(defun seq-sort (pred seq) (seq-into (sort (append seq nil) pred) (seq--type-of seq)))
(defun seq-partition (seq n)
  ;; Each chunk keeps SEQ's type (string→strings, vector→vectors, list→lists).
  (let ((out nil) (type (seq--type-of seq)) (l (append seq nil)))
    (while l
      (setq out (cons (seq-into (take n l) type) out))
      (setq l (nthcdr n l)))
    (reverse out)))
(defun seq-split (seq n) (seq-partition seq n))
(defun seq-set-equal-p (seq1 seq2 &optional testfn)
  (and (seq-every-p (lambda (x) (seq-contains-p seq2 x testfn)) seq1)
       (seq-every-p (lambda (x) (seq-contains-p seq1 x testfn)) seq2)))
(defun seq-into-sequence (seq)
  (if (sequencep seq) seq (error "Cannot convert %S into a sequence" seq)))
(defun seq-sort-by (fn pred seq)
  (seq-sort (lambda (a b) (funcall pred (funcall fn a) (funcall fn b))) seq))
(defun seq-positions (seq elt &optional testfn)
  (let ((test (or testfn 'equal)) (i 0) (out nil) (l (append seq nil)))
    (while l
      (when (funcall test (car l) elt) (setq out (cons i out)))
      (setq i (1+ i) l (cdr l)))
    (nreverse out)))
(defun seq-remove-at-position (seq n)
  (let ((i 0) (out nil) (l (append seq nil)))
    (while l (unless (= i n) (setq out (cons (car l) out))) (setq i (1+ i) l (cdr l)))
    (let ((r (nreverse out))) (if (vectorp seq) (vconcat r) r))))
(defun seq--type-of (seq)
  (cond ((listp seq) 'list) ((vectorp seq) 'vector) ((stringp seq) 'string) (t 'list)))
(defun seq-take-while (pred seq)
  ;; Leading run of SEQ for which PRED holds, returned as SEQ's own type.
  (let ((lst (append seq nil)) (acc nil) (go t))
    (while (and lst go)
      (if (funcall pred (car lst))
          (setq acc (cons (car lst) acc) lst (cdr lst))
        (setq go nil)))
    (seq-into (nreverse acc) (seq--type-of seq))))
(defun seq-drop-while (pred seq)
  ;; SEQ with its leading PRED-satisfying run removed, in SEQ's own type.
  (let ((lst (append seq nil)) (go t))
    (while (and lst go)
      (if (funcall pred (car lst)) (setq lst (cdr lst)) (setq go nil)))
    (seq-into lst (seq--type-of seq))))
(defun seq-contains (seq elt &optional testfn)
  ;; Deprecated in Emacs but still provided: returns the matching element.
  (let ((test (or testfn #'equal)) (l (append seq nil)) (res nil))
    (while (and l (null res))
      (when (funcall test elt (car l)) (setq res (car l)))
      (setq l (cdr l)))
    res))

;;; ---- cl-lib (subset) ----
(defun cl-mapcar (fn &rest seqs)
  (apply (function seq-mapn) fn (mapcar (lambda (s) (append s nil)) seqs)))
(defun cl-map (type fn &rest seqs)
  "Map FN over SEQS in parallel, collecting results into a sequence of TYPE.
TYPE nil maps for side effects only and returns nil."
  (let ((result (apply (function cl-mapcar) fn seqs)))
    (if (null type) nil (cl-coerce result type))))
(defun cl-subseq (seq start &optional end) (seq-subseq seq start end))
(defun cl--in-bounds (i start end) (and (>= i start) (or (null end) (< i end))))
(defun cl-position (item seq &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (from-end (cl--getkey keys :from-end nil))
        (lst (append seq nil)) (i 0) (r nil))
    ;; With :from-end, keep scanning so R ends up the last match.
    (while (and lst (or from-end (not r)))
      (when (and (cl--in-bounds i start end)
                 (cl--seq-match test test-not item (funcall key (car lst))))
        (setq r i))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-count (item seq &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (n 0))
    (while lst
      (when (and (cl--in-bounds i start end)
                 (cl--seq-match test test-not item (funcall key (car lst))))
        (setq n (1+ n)))
      (setq i (1+ i) lst (cdr lst)))
    n))
(defun cl-count-if (pred seq &rest keys)
  (let ((key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (n 0))
    (while lst
      (when (and (cl--in-bounds i start end) (funcall pred (funcall key (car lst))))
        (setq n (1+ n)))
      (setq i (1+ i) lst (cdr lst)))
    n))
(defun cl-count-if-not (pred seq &rest keys)
  (apply 'cl-count-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-position-if (pred seq &rest keys)
  (let ((key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (from-end (cl--getkey keys :from-end nil))
        (lst (append seq nil)) (i 0) (r nil))
    (while (and lst (or from-end (not r)))
      (when (and (cl--in-bounds i start end) (funcall pred (funcall key (car lst))))
        (setq r i))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-position-if-not (pred seq &rest keys)
  (apply 'cl-position-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-find (item seq &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (r nil) (found nil))
    (while (and lst (or from-end (not found)))
      (when (and (cl--in-bounds i start end)
                 (cl--seq-match test test-not item (funcall key (car lst))))
        (setq r (car lst) found t))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-remove-duplicates (seq &rest keys)
  ;; Return a copy of SEQ with duplicates removed. Elements are compared with
  ;; :test (default `eql') applied to the value returned by :key (default
  ;; `identity'). Default keeps the LAST occurrence of each element; with
  ;; :from-end non-nil, keeps the FIRST. Keeping the LAST occurrence in order is
  ;; the same as keeping the FIRST occurrence of the reversed list.
  (let* ((test (cl--getkey keys :test 'eql))
         (key (cl--getkey keys :key 'identity))
         (from-end (cl--getkey keys :from-end nil))
         (items (if from-end (append seq nil) (reverse (append seq nil))))
         (seen nil) (res nil))
    (dolist (x items)
      (let ((k (funcall key x)) (dup nil) (s seen))
        (while (and s (not dup))
          (when (funcall test k (car s)) (setq dup t))
          (setq s (cdr s)))
        (unless dup (setq seen (cons k seen) res (cons x res)))))
    (cl--like (if from-end (nreverse res) res) seq)))
(defun cl-pairlis (the-keys the-values &optional alist)
  ;; Pair KEYS with VALUES (stopping at the shorter list), prepended to ALIST.
  (nconc (cl-mapcar 'cons the-keys the-values) alist))
(defun cl-remprop (symbol propname)
  ;; Remove the first PROPNAME/value pair from SYMBOL's plist; return t if one
  ;; was removed, else nil. Rebuilds the plist rather than splicing in place.
  (let ((plist (symbol-plist symbol)) (out nil) (found nil))
    (while plist
      (if (and (not found) (eq (car plist) propname))
          (setq found t)
        (setq out (append out (list (car plist) (car (cdr plist))))))
      (setq plist (cdr (cdr plist))))
    (when found (setplist symbol out))
    found))
(defun lax-plist-get (plist prop)
  (let ((res nil) (found nil))
    (while (and plist (not found))
      (if (equal (car plist) prop) (setq res (car (cdr plist)) found t)
        (setq plist (cdr (cdr plist)))))
    res))
(defun cl-tailp (sublist list)
  (let ((res nil))
    (while (and (consp list) (not res))
      (if (eq list sublist) (setq res t) (setq list (cdr list))))
    (or res (eq list sublist))))
(defun cl-ldiff (list sublist)
  (let ((res nil))
    (while (and (consp list) (not (eq list sublist)))
      (setq res (cons (car list) res) list (cdr list)))
    (nreverse res)))
(defun cl-getf (plist key &optional default)
  (let ((m (plist-member plist key))) (if m (cadr m) default)))
;; Remove the FIRST KEY/value pair from PLIST (eq comparison), like Emacs.
(defun cl--plist-remove-first (plist tag)
  (let ((out nil) (removed nil))
    (while plist
      (if (and (not removed) (eq (car plist) tag))
          (setq removed t)
        (setq out (cons (cadr plist) (cons (car plist) out))))
      (setq plist (cddr plist)))
    (nreverse out)))
(defmacro cl-remf (place tag)
  "Remove the property TAG from the plist stored in PLACE; return t if present."
  `(let ((--cl-remf-- (plist-member ,place ,tag)))
     (when --cl-remf-- (setf ,place (cl--plist-remove-first ,place ,tag)))
     (and --cl-remf-- t)))

;;; ---- subr-x macros ----
;; Build nested `(let ((VAR VAL)) (if VAR <inner> ELSE))` for a list of BINDINGS,
;; short-circuiting to ELSE the first time a bound value is nil.
(defun if-let--chain (bindings then else)
  (if (null bindings) then
    (let* ((b (car bindings))
           ;; Clause forms: SYMBOL (test its value), (SYMBOL VALUE) (bind+test),
           ;; or (VALUE) — a single-element list that is only tested, not bound.
           (test-only (and (consp b) (null (cdr b))))
           (var (cond ((not (consp b)) b)
                      (test-only (make-symbol "if-let"))
                      (t (car b))))
           (val (cond ((not (consp b)) b)
                      (test-only (car b))
                      (t (car (cdr b))))))
      (list 'let (list (list var val))
            (list 'if var (if-let--chain (cdr bindings) then else) else)))))
;; Accept the old single-binding spelling `(VAR VAL)` as well as a binding list.
(defun if-let--norm (spec)
  (if (and (consp spec) (symbolp (car spec))) (list spec) spec))
(defmacro if-let* (bindings then &rest else)
  (if-let--chain bindings then (cons 'progn else)))
(defmacro when-let* (bindings &rest body)
  (if-let--chain bindings (cons 'progn body) nil))
(defmacro if-let (bindings then &rest else)
  (if-let--chain (if-let--norm bindings) then (cons 'progn else)))
(defmacro when-let (bindings &rest body)
  (if-let--chain (if-let--norm bindings) (cons 'progn body) nil))
(defmacro named-let (name bindings &rest body)
  ;; A self-recursive local loop: (named-let f ((i 0)) (if … (f (1+ i)) i)).
  (let ((vars (mapcar (function car) bindings))
        (vals (mapcar (lambda (b) (car (cdr b))) bindings)))
    `(progn (defun ,name ,vars ,@body) (,name ,@vals))))
;; cl-block / cl-return-from: a named escape, implemented with catch/throw on a
;; per-name tag symbol. cl-return / cl-dolist / cl-dotimes use the nil block.
(defmacro cl-block (name &rest body)
  `(catch (quote ,(intern (concat "--cl-block-" (symbol-name name) "--"))) ,@body))
(defmacro cl-return-from (name &optional value)
  `(throw (quote ,(intern (concat "--cl-block-" (symbol-name name) "--"))) ,value))
(defmacro cl-return (&optional value) `(cl-return-from nil ,value))
(defmacro cl-dolist (spec &rest body) `(cl-block nil (dolist ,spec ,@body)))
(defmacro cl-dotimes (spec &rest body) `(cl-block nil (dotimes ,spec ,@body)))
(defmacro cl-pushnew (x place &rest _keys)
  ;; Add X to the front of PLACE unless already present (by eql).
  `(if (memql ,x ,place) ,place (setf ,place (cons ,x ,place))))
;;; ---- help.el usage/docstring helpers (help-add-fundoc-usage &c) ----
;; Faithful ports from lisp/help.el (30.2). cl-defgeneric/cl-defmethod expansion
;; in cl-generic.el calls `help-add-fundoc-usage' to append the "(fn ARGS)" line
;; to a method's docstring; these are the (formerly help-fns.el) helpers it needs.
(defun help--docstring-quote (string)
  "Return a doc string that represents STRING.
The result, when formatted by `substitute-command-keys', should equal STRING."
  (replace-regexp-in-string "['\\`‘’]" "\\\\=\\&" string))
(defun help--make-usage (function arglist)
  (cons (if (symbolp function) function 'anonymous)
        (mapcar (lambda (arg)
                  (cond
                   ;; Parameter name.
                   ((symbolp arg)
                    (let ((name (symbol-name arg)))
                      (cond
                       ((string-match "\\`&" name) (bare-symbol arg))
                       ((string-match "\\`_." name)
                        (intern (upcase (substring name 1))))
                       (t (intern (upcase name))))))
                   ;; Parameter with a default value (from cl-defgeneric etc).
                   ((and (consp arg)
                         (symbolp (car arg)))
                    (cons (intern (upcase (symbol-name (car arg)))) (cdr arg)))
                   ;; Something else.
                   (t arg)))
                arglist)))
;; (define-obsolete-function-alias 'help-make-usage ...) lives after `defalias'
;; is defined below — the alias body calls `defalias' at load time.
(defun help--make-usage-docstring (fn arglist)
  (let ((print-escape-newlines t))
    (help--docstring-quote (format "%S" (help--make-usage fn arglist)))))
;; `help-split-fundoc' is defined after `pcase' (below) since its body uses it
;; and elisprs expands macros at `defun'-definition time.
(defun help-add-fundoc-usage (docstring arglist)
  "Add the usage info to DOCSTRING.
If DOCSTRING already has a usage info, then just return it unchanged.
The usage info is built from ARGLIST.  DOCSTRING can be nil.
ARGLIST can also be t or a string of the form \"(FUN ARG1 ARG2 ...)\"."
  (unless (stringp docstring) (setq docstring ""))
  (if (or (string-match "\n\n(fn\\(\\( .*\\)?)\\)\\'" docstring)
          (eq arglist t))
      docstring
    (concat docstring
            (if (string-match "\n?\n\\'" docstring)
                (if (< (- (match-end 0) (match-beginning 0)) 2) "\n" "")
              "\n\n")
            (if (stringp arglist)
                (if (string-match "\\`[^ ]+\\(.*\\))\\'" arglist)
                    (concat "(fn" (match-string 1 arglist) ")")
                  (error "Unrecognized usage format"))
              (help--make-usage-docstring 'fn arglist)))))

;; cl-defstruct registries: --slots maps NAME -> full slot specs (read at
;; macroexpansion to inherit via :include); --parent maps child-tag -> parent-tag
;; (walked at runtime so a subtype satisfies a parent's predicate).
(defvar cl-struct--slots nil)
(defvar cl-struct--parent nil)
(defun cl-struct--is-a (tag target)
  (let ((res nil))
    (while (and tag (not res))
      (if (eq tag target) (setq res t) (setq tag (cdr (assq tag cl-struct--parent)))))
    res))
;; cl-defstruct: a struct is a vector [cl-struct-NAME slot1 slot2 ...]. Generates
;; the `make-NAME' keyword constructor (with per-slot defaults), `NAME-p'
;; predicate, `NAME-SLOT' accessors (setf-able), and `copy-NAME' copier. (Printing
;; and `type-of' differ from Emacs records — these are plain vectors.)
(defmacro cl-defstruct (name-spec &rest slots)
  ;; An optional docstring may precede the slot specs (cl-macs.el pops it via
  ;; `(if (stringp (car descs)) (pop descs))').  Record it on the struct's plist
  ;; and drop it so it is not mistaken for a slot.
  (let* ((--doc-- (and (stringp (car slots)) (car slots)))
         (slots (if --doc-- (cdr slots) slots))
         (name (if (consp name-spec) (car name-spec) name-spec))
         (sname (symbol-name name))
         (tag (intern (concat "cl-struct-" sname)))
         ;; Options: (:constructor NAME) renames the make-NAME constructor;
         ;; (:conc-name P) overrides the NAME- accessor prefix.
         ;; Constructors: each (:constructor NAME) is a keyword ctor; (:constructor
         ;; NAME ARGLIST) is a BOA (positional) ctor; default is make-NAME. A bare
         ;; (:constructor nil) suppresses the default without adding one.
         (constructors
          (let ((cs nil) (saw nil))
            (when (consp name-spec)
              (dolist (opt (cdr name-spec))
                (when (and (consp opt) (eq (car opt) :constructor))
                  (setq saw t)
                  (let ((nm (car (cdr opt))) (al (cdr (cdr opt))))
                    (when nm
                      (setq cs (cons (cons nm (if al (list 'boa (car al)) 'kw)) cs)))))))
            (if saw (reverse cs) (list (cons (intern (concat "make-" sname)) 'kw)))))
         (conc (let ((p (concat sname "-")))
                 (when (consp name-spec)
                   (dolist (opt (cdr name-spec))
                     (when (and (consp opt) (eq (car opt) :conc-name))
                       (let ((v (car (cdr opt))))
                         (setq p (cond ((null v) "") ((symbolp v) (symbol-name v)) (t v)))))))
                 p))
         ;; (:copier NAME) renames the copy-NAME copier; (:copier nil) suppresses it.
         (copier (let ((c (concat "copy-" sname)))
                   (when (consp name-spec)
                     (dolist (opt (cdr name-spec))
                       (when (and (consp opt) (eq (car opt) :copier))
                         (setq c (and (car (cdr opt)) (symbol-name (car (cdr opt))))))))
                   c))
         ;; (:predicate NAME) renames the NAME-p predicate.
         (pred (let ((c (concat sname "-p")))
                 (when (consp name-spec)
                   (dolist (opt (cdr name-spec))
                     (when (and (consp opt) (eq (car opt) :predicate) (car (cdr opt)))
                       (setq c (symbol-name (car (cdr opt)))))))
                 c))
         ;; (:include PARENT) inherits PARENT's slots (prepended, same order) and
         ;; records the tag parentage so the predicate accepts subtypes.
         (parent (let ((p nil))
                   (when (consp name-spec)
                     (dolist (opt (cdr name-spec))
                       (when (and (consp opt) (eq (car opt) :include))
                         (setq p (car (cdr opt))))))
                   p))
         (parent-slots (and parent (boundp 'cl-struct--slots)
                            (cdr (assq parent cl-struct--slots))))
         (all-slots (append parent-slots slots))
         (snames (mapcar (lambda (s) (if (consp s) (car s) s)) all-slots))
         (defaults (mapcar (lambda (s) (if (consp s) (car (cdr s)) nil)) all-slots))
         ;; Per-slot options plist is (cddr SLOT): (:type T :read-only R :documentation D).
         ;; `:type'/`:documentation' are parsed and ignored for storage (Emacs does
         ;; not enforce :type at runtime); `:read-only' non-nil makes the slot's
         ;; setf-expander error, matching cl-macs.el:3257.
         (readonly (mapcar (lambda (s) (and (consp s) (plist-get (cdr (cdr s)) :read-only)))
                           all-slots))
         (forms nil)
         ;; constructor: vector of defaults, then apply :keyword overrides.
         (kw-clauses (let ((j 1) (cs nil))
                       (dolist (sn snames)
                         (setq cs (cons (list (list 'eq '--k-- (intern (concat ":" (symbol-name sn))))
                                              (list 'aset '--v-- j '--val--))
                                        cs))
                         (setq j (1+ j)))
                       (reverse cs))))
    ;; Record this struct's full (inherited + own) slot specs at macroexpansion
    ;; time so a later (:include this) can read them.
    (setq cl-struct--slots (cons (cons name all-slots) cl-struct--slots))
    (dolist (cspec constructors)
      (let ((cname (car cspec)) (ckind (cdr cspec)))
        (if (eq ckind 'kw)
            (setq forms
                  (cons `(defun ,cname (&rest --args--)
                           (let ((--v-- (vector ',tag ,@defaults)) (--a-- --args--))
                             (while --a--
                               (let ((--k-- (car --a--)) (--val-- (car (cdr --a--))))
                                 (cond ,@kw-clauses))
                               (setq --a-- (cdr (cdr --a--))))
                             --v--))
                        forms))
          ;; BOA: ckind = (boa ARGLIST). Each slot takes the like-named arg if it
          ;; appears in ARGLIST, else its default. Per-arg defaults like (y 10)
          ;; are reduced to plain params (the var binds to nil if unsupplied).
          (let* ((arglist (car (cdr ckind)))
                 (avars (let ((r nil))
                          (dolist (p arglist)
                            (cond ((and (symbolp p) (> (length (symbol-name p)) 0)
                                        (eq (aref (symbol-name p) 0) ?&)) nil)
                                  ((consp p) (setq r (cons (car p) r)))
                                  (t (setq r (cons p r)))))
                          (reverse r)))
                 (defargs (mapcar (lambda (p) (if (consp p) (car p) p)) arglist))
                 (vals (let ((vs nil) (sn snames) (df defaults))
                         (while sn
                           (setq vs (cons (if (memq (car sn) avars) (car sn) (car df)) vs))
                           (setq sn (cdr sn) df (cdr df)))
                         (reverse vs))))
            (setq forms (cons `(defun ,cname ,defargs (vector ',tag ,@vals)) forms))))))
    (setq forms (cons `(defun ,(intern pred) (--o--)
                         (and (vectorp --o--) (> (length --o--) 0)
                              (cl-struct--is-a (aref --o-- 0) ',tag)))
                      forms))
    ;; Record the tag's parent so the predicate accepts subtypes.  With an
    ;; explicit `:include' the parent is that struct; otherwise every "normal"
    ;; struct implicitly roots at `cl-structure-object' (cl-macs.el defaults
    ;; `parent-type' to it), which is what makes `cl-struct-p' — the predicate
    ;; for `cl-structure-object' — accept any struct.  The root itself is skipped
    ;; so it has no self-parent.
    (let ((ptag (cond (parent (intern (concat "cl-struct-" (symbol-name parent))))
                      ((not (eq name 'cl-structure-object)) 'cl-struct-cl-structure-object))))
      (when ptag
        (setq forms (cons `(setq cl-struct--parent
                                 (cons (cons ',tag ',ptag) cl-struct--parent))
                          forms))))
    (when copier
      (setq forms (cons `(defun ,(intern copier) (--s--) (vconcat --s--)) forms)))
    ;; Register a `cl-structure-class' so cl-generic's typeof generalizer can
    ;; dispatch on this struct type (cl-preloaded.el:205).  Guarded so the many
    ;; structs defined before the class registry is bootstrapped are skipped.
    (setq forms
          (cons `(when (and (fboundp 'cl--struct-new-class)
                            (cl--find-class 'cl-structure-object))
                   (put ',name 'cl--class
                        (cl--struct-new-class
                         ',name ,--doc--
                         (list (or ,(if parent `(cl--find-class ',parent))
                                   (cl--find-class 'cl-structure-object)))
                         nil nil nil nil nil nil nil)))
                forms))
    (let ((j 1) (ros readonly))
      (dolist (sn snames)
        (let ((acc (intern (concat conc (symbol-name sn)))))
          (setq forms (cons `(defun ,acc (--s--) (aref --s-- ,j)) forms))
          (setq forms (cons `(setq cl-struct--slot-index
                                   (cons (cons ',acc ,j) cl-struct--slot-index))
                            forms))
          (when (car ros)
            (setq forms (cons `(setq cl-struct--slot-readonly
                                     (cons ',acc cl-struct--slot-readonly))
                              forms))))
        (setq ros (cdr ros))
        (setq j (1+ j))))
    `(progn ,@(reverse forms) ',name)))
;; Generic struct-slot introspection. Slots are stored as bare symbols (no
;; default) or `(NAME DEFAULT)' pairs; slot 0 of the vector is the type tag.
(defun cl-struct-slot-offset (struct-type slot-name)
  (let ((slots (cdr (assq struct-type cl-struct--slots))) (i 1) (idx nil))
    (dolist (s slots)
      (when (eq (if (consp s) (car s) s) slot-name) (setq idx i))
      (setq i (1+ i)))
    (or idx (error "Invalid slot name: %S, %S" struct-type slot-name))))
(defun cl-struct-slot-value (struct-type slot-name inst)
  (aref inst (cl-struct-slot-offset struct-type slot-name)))
(defun cl-struct-slot-info (struct-type)
  (cons '(cl-tag-slot)
        (mapcar (lambda (s) (if (consp s) s (list s nil)))
                (cdr (assq struct-type cl-struct--slots)))))
(defmacro and-let* (bindings &rest body)
  ;; Like when-let* but with no body returns the last bound value (SRFI-2).
  (if-let--chain bindings
                 (if body (cons 'progn body)
                   (let ((lastb (car (last bindings))))
                     (if (consp lastb) (car lastb) lastb)))
                 nil))
;; let-alist: bind every `.KEY' symbol in BODY to (cdr (assq 'KEY ALIST)).
(defun let-alist--dots (form acc)
  (cond
   ((and (symbolp form) form)
    (let ((n (symbol-name form)))
      (if (and (> (length n) 1) (eq (aref n 0) ?.) (not (memq form acc)))
          (cons form acc) acc)))
   ((consp form)
    (if (eq (car form) 'quote) acc
      (let-alist--dots (cdr form) (let-alist--dots (car form) acc))))
   (t acc)))
(defmacro let-alist (alist &rest body)
  (let ((dots (let-alist--dots body nil)))
    `(let ((--let-alist-- ,alist))
       (let ,(mapcar (lambda (d)
                       (list d (list 'cdr (list 'assq
                                                (list 'quote (intern (substring (symbol-name d) 1)))
                                                '--let-alist--))))
                     dots)
         ,@body))))
;; cl-flet / cl-labels: lexical local functions. Rewrite calls to a NAME and
;; #'NAME in BODY into `funcall'/refs of a let-bound lambda. cl-labels also walks
;; the function bodies (so they can recurse / call each other) and binds via
;; setq so the lambdas capture the (by-reference) gensym vars.
(defun cl-flet--walk (form alist)
  (cond
   ((not (consp form)) form)
   ((eq (car form) 'quote) form)
   ((eq (car form) 'function)
    (let ((a (assq (car (cdr form)) alist)))
      (if a (cdr a) form)))
   ((and (symbolp (car form)) (assq (car form) alist))
    (cons 'funcall (cons (cdr (assq (car form) alist)) (cl-flet--walk (cdr form) alist))))
   (t (cons (cl-flet--walk (car form) alist) (cl-flet--walk (cdr form) alist)))))
(defun cl-flet--cl-argp (arglist)
  ;; Non-nil when ARGLIST uses cl-lambda-list features a plain `lambda' can't
  ;; take: per-arg defaults (cons elements) or &key/&aux/&whole/&allow-other-keys.
  (let ((cl nil))
    (while (consp arglist)
      (when (or (consp (car arglist))
                (memq (car arglist) '(&key &aux &whole &allow-other-keys)))
        (setq cl t))
      (setq arglist (cdr arglist)))
    cl))
(defun cl-flet--lambda (spec)
  ;; SPEC is (ARGLIST . BODY). Build the local-function lambda, routing through
  ;; `cl-destructuring-bind' when ARGLIST needs full cl-lambda-list handling
  ;; (matching Emacs cl-flet/cl-labels, which accept cl-lambda-lists).
  (if (cl-flet--cl-argp (car spec))
      (list 'lambda '(&rest --cl-flet-args--)
            (cons 'cl-destructuring-bind
                  (cons (car spec) (cons '--cl-flet-args-- (cdr spec)))))
    (cons 'lambda spec)))
(defun cl-flet--binding-value (spec)
  ;; SPEC is (cdr BINDING).  Per cl-macs.el:2098, a length-1 SPEC is the
  ;; `(FUNC EXP)' form: EXP is an expression returning the function value to
  ;; bind (used by cl-generic's `(cl-flet ((cl-call-next-method CNM)) ...)').
  ;; Otherwise SPEC is `(ARGLIST BODY...)', shorthand for a local lambda.
  (if (= (length spec) 1)
      (car spec)
    (cl-flet--lambda spec)))
(defmacro cl-flet (bindings &rest body)
  (let* ((gs (mapcar (lambda (b) (make-symbol (symbol-name (car b)))) bindings))
         (alist (cl-mapcar (lambda (b g) (cons (car b) g)) bindings gs)))
    `(let ,(cl-mapcar (lambda (b g) (list g (cl-flet--binding-value (cdr b)))) bindings gs)
       ,@(cl-flet--walk body alist))))
(defmacro cl-flet* (bindings &rest body)
  ;; Sequential cl-flet: each local function sees the earlier ones.
  (if (null bindings)
      (cons 'progn body)
    `(cl-flet (,(car bindings)) (cl-flet* ,(cdr bindings) ,@body))))
(defmacro cl-labels (bindings &rest body)
  (let* ((gs (mapcar (lambda (b) (make-symbol (symbol-name (car b)))) bindings))
         (alist (cl-mapcar (lambda (b g) (cons (car b) g)) bindings gs)))
    `(let ,(mapcar (lambda (g) (list g nil)) gs)
       ,@(cl-mapcar (lambda (b g) (list 'setq g (cl-flet--binding-value (cl-flet--walk (cdr b) alist)))) bindings gs)
       ,@(cl-flet--walk body alist))))
;; cl-macrolet: local macros. Expand calls to them throughout BODY at expansion
;; time (re-walking each expansion), then leave the rest for the normal pass.
(defun cl-macrolet--walk (form expanders)
  (cond
   ((not (consp form)) form)
   ((eq (car form) 'quote) form)
   ((and (symbolp (car form)) (assq (car form) expanders))
    (cl-macrolet--walk (apply (cdr (assq (car form) expanders)) (cdr form)) expanders))
   (t (cons (cl-macrolet--walk (car form) expanders)
            (cl-macrolet--walk (cdr form) expanders)))))
(defmacro cl-macrolet (bindings &rest body)
  ;; eval each local-macro lambda into a real closure (apply needs a closure,
  ;; not a raw lambda list), then expand calls to them in BODY.
  (let ((expanders (mapcar (lambda (b) (cons (car b) (eval (cons 'lambda (cdr b)) t))) bindings)))
    (cons 'progn (cl-macrolet--walk body expanders))))
;; cl-symbol-macrolet: substitute each NAME with its EXPANSION in value position
;; throughout BODY; `(setq NAME v)` becomes `(setf EXPANSION v)`. (No shadowing
;; handling — don't rebind a symbol-macro name with let inside the body.)
(defun cl-symbol-macrolet--setq (pairs alist)
  (when pairs
    (let* ((sym (car pairs)) (a (assq sym alist)) (place (if a (car (cdr a)) sym)))
      (cons place (cons (cl-symbol-macrolet--walk (car (cdr pairs)) alist)
                        (cl-symbol-macrolet--setq (cdr (cdr pairs)) alist))))))
(defun cl-symbol-macrolet--walk (form alist)
  (cond
   ((symbolp form) (let ((a (assq form alist))) (if a (car (cdr a)) form)))
   ((not (consp form)) form)
   ((eq (car form) 'quote) form)
   ((eq (car form) 'setq) (cons 'setf (cl-symbol-macrolet--setq (cdr form) alist)))
   (t (cons (cl-symbol-macrolet--walk (car form) alist)
            (cl-symbol-macrolet--walk (cdr form) alist)))))
(defmacro cl-symbol-macrolet (bindings &rest body)
  (cons 'progn (cl-symbol-macrolet--walk body bindings)))
(defmacro letrec (bindings &rest body)
  ;; Bind to nil, then assign — so the (by-reference) closures can recurse / refer
  ;; to each other.
  `(let ,(mapcar (lambda (b) (list (car b) nil)) bindings)
     ,@(mapcar (lambda (b) (list 'setq (car b) (car (cdr b)))) bindings)
     ,@body))
(defmacro dlet (bindings &rest body)
  ;; Dynamic let. (No buffer-local distinction here, so a plain `let'.)
  `(let ,bindings ,@body))
(defmacro cl-letf (bindings &rest body)
  ;; Temporarily set generalized places, restoring them on exit. Each binding is
  ;; (PLACE [VALUE]); with no VALUE the place is just saved and restored.
  (let ((olds (mapcar (lambda (_b) (make-symbol "old")) bindings)) (saves nil) (sets nil) (restores nil))
    (cl-mapcar (lambda (b o)
                 (setq saves (cons (list o (car b)) saves))
                 (when (cdr b) (setq sets (cons (list 'setf (car b) (car (cdr b))) sets)))
                 (setq restores (cons (list 'setf (car b) o) restores)))
               bindings olds)
    `(let ,(reverse saves)
       (unwind-protect (progn ,@(reverse sets) ,@body)
         ,@(reverse restores)))))
(defmacro cl-letf* (bindings &rest body)
  ;; Sequential cl-letf: each binding's place is set before the next is saved.
  (if (null bindings)
      `(progn ,@body)
    `(cl-letf (,(car bindings)) (cl-letf* ,(cdr bindings) ,@body))))
(defmacro thread-first (x &rest forms)
  (let ((acc x))
    (while forms
      (let ((form (car forms)))
        (setq acc (if (consp form) (cons (car form) (cons acc (cdr form))) (list form acc))))
      (setq forms (cdr forms)))
    acc))
(defmacro thread-last (x &rest forms)
  (let ((acc x))
    (while forms
      (let ((form (car forms)))
        (setq acc (if (consp form) (append form (list acc)) (list form acc))))
      (setq forms (cdr forms)))
    acc))

;;; ---- more cl-lib / seq / subr-x / functional ----
(defmacro cl-incf (place &rest amt) `(setf ,place (+ ,place ,(if amt (car amt) 1))))
(defmacro cl-decf (place &rest amt) `(setf ,place (- ,place ,(if amt (car amt) 1))))
;; cl-callf: (cl-callf FUNC PLACE ARGS...) -> (setf PLACE (FUNC PLACE ARGS...)).
;; cl-callf2 puts a fixed ARG1 before PLACE in the call.
(defmacro cl-callf (func place &rest args)
  (list 'setf place (cons func (cons place args))))
(defmacro cl-callf2 (func arg1 place &rest args)
  (list 'setf place (cons func (cons arg1 (cons place args)))))
(defmacro cl-locally (&rest body) (cons 'progn body))
;; cl-psetf: evaluate all values first, then assign all places (parallel setf).
(defmacro cl-psetf (&rest pairs)
  (let ((binds nil) (sets nil) (ps pairs))
    (while ps
      (let ((tv (make-symbol "ps")))
        (setq binds (cons (list tv (cadr ps)) binds))
        (setq sets (cons (list 'setf (car ps) tv) sets))
        (setq ps (cddr ps))))
    `(let ,(reverse binds) ,@(reverse sets) nil)))
;; cl-rotatef: rotate place values left (last gets the first's old value).
(defmacro cl-rotatef (&rest places)
  (if (cdr places)
      (let ((tv (make-symbol "rot")) (forms nil) (ps places))
        (while (cdr ps)
          (setq forms (cons (list 'setf (car ps) (cadr ps)) forms))
          (setq ps (cdr ps)))
        `(let ((,tv ,(car places)))
           ,@(reverse forms)
           (setf ,(car ps) ,tv)
           nil))
    nil))
;; cl-shiftf: shift place values left, the last place taking NEWVAL (the final
;; argument); returns the first place's old value.
(defmacro cl-shiftf (&rest args)
  (let* ((places (butlast args)) (newval (car (last args)))
         (tv (make-symbol "shf")) (forms nil) (ps places))
    (while (cdr ps)
      (setq forms (cons (list 'setf (car ps) (cadr ps)) forms))
      (setq ps (cdr ps)))
    `(let ((,tv ,(car places)))
       ,@(reverse forms)
       (setf ,(car ps) ,newval)
       ,tv)))
(defun cl-first (x) (car x))
(defun cl-fourth (x) (nth 3 x))
(defun cl-fifth (x) (nth 4 x))
(defun cl-sixth (x) (nth 5 x))
(defun cl-seventh (x) (nth 6 x))
(defun cl-eighth (x) (nth 7 x))
(defun cl-ninth (x) (nth 8 x))
(defun cl-tenth (x) (nth 9 x))
;; cl-lib sequence functions with :test / :key / :count keyword args.
(defun cl--getkey (keys kw default)
  (let ((m (plist-member keys kw))) (if m (car (cdr m)) default)))
(defun cl--like (lst seq)
  ;; Coerce a result list back to SEQ's type (string / vector / list).
  (cond ((stringp seq) (apply (function string) lst))
        ((vectorp seq) (vconcat lst))
        (t lst)))
(defun cl-member (item lst &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (r nil))
    (while (and lst (not r))
      (if (cl--seq-match test test-not item (funcall key (car lst)))
          (setq r lst) (setq lst (cdr lst))))
    r))
(defun cl-assoc (item alist &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (r nil))
    (while (and alist (not r))
      (let ((pair (car alist)))
        (if (and (consp pair) (cl--seq-match test test-not item (funcall key (car pair))))
            (setq r pair) (setq alist (cdr alist)))))
    r))
(defun cl-member-if (pred lst &rest keys)
  (let ((key (cl--getkey keys :key 'identity)) (r nil))
    (while (and lst (not r))
      (if (funcall pred (funcall key (car lst))) (setq r lst) (setq lst (cdr lst))))
    r))
(defun cl-member-if-not (pred lst &rest keys)
  (apply 'cl-member-if (lambda (x) (not (funcall pred x))) lst keys))
(defun cl-assoc-if (pred alist &rest keys)
  (let ((key (cl--getkey keys :key 'identity)) (r nil))
    (while (and alist (not r))
      (let ((pair (car alist)))
        (if (and (consp pair) (funcall pred (funcall key (car pair))))
            (setq r pair) (setq alist (cdr alist)))))
    r))
(defun cl-assoc-if-not (pred alist &rest keys)
  (apply 'cl-assoc-if (lambda (x) (not (funcall pred x))) alist keys))
(defun cl-rassoc (item alist &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (r nil))
    (while (and alist (not r))
      (let ((pair (car alist)))
        (if (and (consp pair) (cl--seq-match test test-not item (funcall key (cdr pair))))
            (setq r pair) (setq alist (cdr alist)))))
    r))
(defun cl-rassoc-if (pred alist &rest keys)
  (let ((key (cl--getkey keys :key 'identity)) (r nil))
    (while (and alist (not r))
      (let ((pair (car alist)))
        (if (and (consp pair) (funcall pred (funcall key (cdr pair))))
            (setq r pair) (setq alist (cdr alist)))))
    r))
(defun cl-rassoc-if-not (pred alist &rest keys)
  (apply 'cl-rassoc-if (lambda (x) (not (funcall pred x))) alist keys))
;; Element-match predicate honoring :test / :test-not (default `eql'), mirroring
;; cl--check-test-nokey in cl-seq.el. X is the already-:key-extracted value; the
;; test is called (funcall TEST item x) — item first, element second.
(defun cl--seq-match (test test-not item x)
  (cond (test (funcall test item x))
        (test-not (not (funcall test-not item x)))
        (t (eql item x))))
;; Ascending list of indices in LST to act on: those within [START,END) for
;; which (funcall matchp ELT) is non-nil, limited to COUNT taken from the front,
;; or from the back when FROM-END is non-nil.
(defun cl--act-indices (matchp lst start end count from-end)
  (let ((hits nil) (i 0))
    (dolist (x lst)
      (when (and (cl--in-bounds i start end) (funcall matchp x)) (setq hits (cons i hits)))
      (setq i (1+ i)))
    (setq hits (nreverse hits))
    (if (null count) hits
      (if from-end (nthcdr (max 0 (- (length hits) count)) hits) (take count hits)))))
;; Rebuild SEQ dropping the elements whose index is selected by cl--act-indices.
(defun cl--remove-by (matchp seq start end count from-end)
  (let ((lst (append seq nil)) (acts nil) (out nil) (i 0))
    (setq acts (cl--act-indices matchp lst start end count from-end))
    (dolist (x lst)
      (unless (memql i acts) (setq out (cons x out)))
      (setq i (1+ i)))
    (cl--like (nreverse out) seq)))
;; Rebuild SEQ replacing the selected elements with NEW (others unchanged).
(defun cl--subst-by (matchp new seq start end count from-end)
  (let ((lst (append seq nil)) (acts nil) (out nil) (i 0))
    (setq acts (cl--act-indices matchp lst start end count from-end))
    (dolist (x lst)
      (setq out (cons (if (memql i acts) new x) out))
      (setq i (1+ i)))
    (cl--like (nreverse out) seq)))
(defun cl-remove (item seq &rest keys)
  ;; :test/:test-not/:key/:count/:start/:end/:from-end (cl-seq.el semantics).
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (count (cl--getkey keys :count nil))
        (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil)))
    (cl--remove-by (lambda (x) (cl--seq-match test test-not item (funcall key x)))
                   seq start end count from-end)))
(defun cl-delete (item seq &rest keys) (apply (function cl-remove) item seq keys))
(defun cl-substitute (new old seq &rest keys)
  (let ((test (cl--getkey keys :test nil)) (test-not (cl--getkey keys :test-not nil))
        (key (cl--getkey keys :key 'identity)) (count (cl--getkey keys :count nil))
        (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil)))
    (cl--subst-by (lambda (x) (cl--seq-match test test-not old (funcall key x)))
                  new seq start end count from-end)))
(defun cl-substitute-if (new pred seq &rest keys)
  (let ((key (cl--getkey keys :key 'identity)) (count (cl--getkey keys :count nil))
        (from-end (cl--getkey keys :from-end nil))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil)))
    (cl--subst-by (lambda (x) (funcall pred (funcall key x)))
                  new seq start end count from-end)))
(defun cl-substitute-if-not (new pred seq &rest keys)
  (apply 'cl-substitute-if new (lambda (x) (not (funcall pred x))) seq keys))
;; Destructive substitution: elisprs rebuilds the sequence, so these match the
;; non-destructive forms (the return value is what callers rely on).
(defun cl-nsubstitute (new old seq &rest keys) (apply 'cl-substitute new old seq keys))
(defun cl-nsubstitute-if (new pred seq &rest keys) (apply 'cl-substitute-if new pred seq keys))
(defun cl-nsubstitute-if-not (new pred seq &rest keys)
  (apply 'cl-substitute-if-not new pred seq keys))
(defun cl-fill (seq item &rest keys)
  "Fill the elements of SEQ with ITEM, destructively; keywords :start :end.
Port of cl-fill from cl-seq.el (mutates SEQ in place, returns SEQ)."
  (let ((start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil)))
    (if (listp seq)
        (let ((p (nthcdr start seq)) (n (and end (- end start))))
          (while (and p (or (null n) (>= (cl-decf n) 0)))
            (setcar p item)
            (setq p (cdr p))))
      (or end (setq end (length seq)))
      (if (and (= start 0) (= end (length seq)))
          (fillarray seq item)
        (while (< start end)
          (aset seq start item)
          (setq start (1+ start)))))
    seq))
(defun cl-replace (seq1 seq2 &rest keys)
  "Replace the elements of SEQ1 with those of SEQ2, destructively; returns SEQ1.
Port of cl-replace from cl-seq.el; keywords :start1 :end1 :start2 :end2."
  (if (stringp seq1)
      ;; Immutable-string model: return a fresh modified string. The return value
      ;; is faithful to emacs cl-replace; in-place string aliasing is unsupported.
      (let* ((start1 (cl--getkey keys :start1 0)) (end1 (cl--getkey keys :end1 nil))
             (start2 (cl--getkey keys :start2 0)) (end2 (cl--getkey keys :end2 nil))
             (l1 (append seq1 nil)) (l2 (append seq2 nil))
             (e1 (min (or end1 (length l1)) (length l1)))
             (e2 (min (or end2 (length l2)) (length l2)))
             (p1 (nthcdr start1 l1)) (p2 (nthcdr start2 l2)))
        (while (and p1 p2 (< start1 e1) (< start2 e2))
          (setcar p1 (car p2))
          (setq p1 (cdr p1) p2 (cdr p2) start1 (1+ start1) start2 (1+ start2)))
        (apply #'string l1))
    (let ((start1 (cl--getkey keys :start1 0)) (end1 (cl--getkey keys :end1 nil))
        (start2 (cl--getkey keys :start2 0)) (end2 (cl--getkey keys :end2 nil)))
    (if (and (eq seq1 seq2) (<= start2 start1))
        (or (= start1 start2)
            (let* ((len (length seq1))
                   (n (min (- (or end1 len) start1) (- (or end2 len) start2))))
              (while (>= (setq n (1- n)) 0)
                (if (listp seq1)
                    (setcar (nthcdr (+ start1 n) seq1) (elt seq2 (+ start2 n)))
                  (aset seq1 (+ start1 n) (elt seq2 (+ start2 n)))))))
      (if (listp seq1)
          (let ((p1 (nthcdr start1 seq1)) (n1 (and end1 (- end1 start1))))
            (if (listp seq2)
                (let ((p2 (nthcdr start2 seq2))
                      (n (cond ((and n1 end2) (min n1 (- end2 start2)))
                               ((and n1 (null end2)) n1)
                               ((and (null n1) end2) (- end2 start2)))))
                  (while (and p1 p2 (or (null n) (>= (cl-decf n) 0)))
                    (setcar p1 (car p2))
                    (setq p1 (cdr p1) p2 (cdr p2))))
              (setq end2 (if (null n1)
                             (or end2 (length seq2))
                           (min (or end2 (length seq2)) (+ start2 n1))))
              (while (and p1 (< start2 end2))
                (setcar p1 (aref seq2 start2))
                (setq p1 (cdr p1) start2 (1+ start2)))))
        (setq end1 (min (or end1 (length seq1))
                        (+ start1 (- (or end2 (length seq2)) start2))))
        (if (listp seq2)
            (let ((p2 (nthcdr start2 seq2)))
              (while (< start1 end1)
                (aset seq1 start1 (car p2))
                (setq p2 (cdr p2) start1 (1+ start1))))
          (while (< start1 end1)
            (aset seq1 start1 (aref seq2 start2))
            (setq start2 (1+ start2) start1 (1+ start1))))))
    seq1)))
(defun cl-mapcan (fn &rest seqs) (apply 'nconc (apply 'cl-mapcar fn seqs)))
(defun cl-acons (key val alist) (cons (cons key val) alist))
(defun cl-list* (&rest args)
  (if (null (cdr args)) (car args) (cons (car args) (apply (function cl-list*) (cdr args)))))

(defun seq-map-indexed (fn seq)
  (let ((i 0) (out nil) (l (append seq nil)))
    (while l (setq out (cons (funcall fn (car l) i) out)) (setq l (cdr l)) (setq i (1+ i)))
    (reverse out)))
(defun seq-do-indexed (fn seq)
  (let ((i 0) (l (append seq nil)))
    (while l (funcall fn (car l) i) (setq l (cdr l)) (setq i (1+ i))))
  nil)
(defun seq-keep (fn seq) (seq-filter (function identity) (mapcar fn seq)))
(defun seq-mapcat (fn seq &optional type)
  (apply 'seq-concatenate (or type 'list) (mapcar fn (append seq nil))))
(defun seq-mapn (fn &rest seqs)
  ;; Apply FN across N sequences in parallel, stopping at the shortest:
  ;; (seq-mapn #'+ '(1 2) '(3 4)) => (4 6). Accepts any sequence types.
  (let ((r nil))
    (setq seqs (mapcar (lambda (s) (append s nil)) seqs))
    (while (not (memq nil seqs))
      (setq r (cons (apply fn (mapcar (function car) seqs)) r))
      (setq seqs (mapcar (function cdr) seqs)))
    (reverse r)))
;; Like `format' (we don't translate `...' to curved quotes).
(defun format-message (fmt &rest args)
  ;; Curve-quote the format string (default `text-quoting-style' is 'curve):
  ;; grave accent => left single quote, apostrophe => right single quote.
  (apply (function format) (string-replace "'" "’" (string-replace "`" "‘" fmt)) args))
(defun cl--digitp (c) (and (>= c ?0) (<= c ?9)))
;; value<: a canonical total order within a type (numbers/strings/symbols/lists/
;; vectors); cross-type comparison signals an error, like Emacs 30.
(defun value<--class (x)
  (cond ((numberp x) 'number) ((stringp x) 'string)
        ((or (consp x) (null x)) 'list) ((symbolp x) 'symbol)
        ((vectorp x) 'vector) (t 'other)))
(defun value< (a b)
  (let ((ca (value<--class a)) (cb (value<--class b)))
    (unless (eq ca cb) (signal (quote type-mismatch) (list a b)))
    (cond
     ((eq ca 'number) (< a b))
     ((eq ca 'string) (string< a b))
     ((eq ca 'symbol) (string< (symbol-name a) (symbol-name b)))
     ((eq ca 'list)
      (cond ((null a) (not (null b)))
            ((null b) nil)
            ((value< (car a) (car b)) t)
            ((value< (car b) (car a)) nil)
            (t (value< (cdr a) (cdr b)))))
     ((eq ca 'vector)
      (let ((n (min (length a) (length b))) (i 0) (res 'eq))
        (while (and (< i n) (eq res 'eq))
          (cond ((value< (aref a i) (aref b i)) (setq res t))
                ((value< (aref b i) (aref a i)) (setq res nil)))
          (setq i (1+ i)))
        (if (eq res 'eq) (< (length a) (length b)) res)))
     (t (signal (quote type-mismatch) (list a b))))))
(defun string-version-lessp (s1 s2)
  ;; Like `string-lessp' but compare embedded decimal runs numerically.
  (let ((i 0) (j 0) (n1 (length s1)) (n2 (length s2)) (res nil) (done nil))
    (while (not done)
      (cond
       ((and (>= i n1) (>= j n2)) (setq done t res nil))
       ((>= i n1) (setq done t res t))
       ((>= j n2) (setq done t res nil))
       (t (let ((c1 (aref s1 i)) (c2 (aref s2 j)))
            (if (and (cl--digitp c1) (cl--digitp c2))
                (let ((a i) (b j))
                  (while (and (< i n1) (cl--digitp (aref s1 i))) (setq i (1+ i)))
                  (while (and (< j n2) (cl--digitp (aref s2 j))) (setq j (1+ j)))
                  (let ((v1 (string-to-number (substring s1 a i)))
                        (v2 (string-to-number (substring s2 b j))))
                    (cond ((< v1 v2) (setq done t res t))
                          ((> v1 v2) (setq done t res nil)))))
              (cond ((< c1 c2) (setq done t res t))
                    ((> c1 c2) (setq done t res nil))
                    (t (setq i (1+ i) j (1+ j)))))))))
    res))
(defun seq-concatenate (type &rest seqs)
  ;; Concatenate SEQS into one sequence of TYPE (`list', `vector', or `string').
  (let ((all nil))
    (dolist (s seqs) (setq all (append all (append s nil))))
    (cond ((eq type 'list) all)
          ((eq type 'vector) (vconcat all))
          ((eq type 'string) (apply (function string) all))
          (t (error "Not a sequence type name: %S" type)))))
(defun cl-concatenate (type &rest seqs) (apply #'seq-concatenate type seqs))
;; cl-equalp: like equal but numbers compare with `=' (1 = 1.0) and strings
;; case-insensitively; recurses through conses and vectors. (Characters are
;; integers in elisp, so they compare numerically, like Emacs.)
(defun cl-equalp (x y)
  (cond
   ((and (numberp x) (numberp y)) (= x y))
   ((and (stringp x) (stringp y)) (string-equal-ignore-case x y))
   ((and (consp x) (consp y)) (and (cl-equalp (car x) (car y)) (cl-equalp (cdr x) (cdr y))))
   ((and (vectorp x) (vectorp y))
    (and (= (length x) (length y)) (cl-equalp (append x nil) (append y nil))))
   (t (equal x y))))
(defun copy-alist (al)
  (mapcar (lambda (p) (if (consp p) (cons (car p) (cdr p)) p)) al))
(defun substring-no-properties (s &optional from to)
  (if to (substring s (or from 0) to) (substring s (or from 0))))
;; The human-readable message for an error object (ERROR-SYMBOL . DATA).
(defun error-message-string (err)
  (let* ((sym (car err)) (data (cdr err)) (msg (get sym 'error-message))
         ;; Data is printed readably (prin1), except the file-error family,
         ;; which shows its strings verbatim.
         (filey (memq 'file-error (get sym 'error-conditions)))
         (render (lambda (x) (if (and filey (stringp x)) x (format "%S" x)))))
    (cond
     ;; `error'/`user-error' carry their message string as the sole datum.
     ((and (memq sym '(error user-error)) (stringp (car data))) (car data))
     (msg (if data (concat msg ": " (mapconcat render data ", ")) msg))
     ((null data) (symbol-name sym))
     (t (concat (symbol-name sym) ": " (mapconcat render data ", "))))))
(defun seq-group-by (fn seq)
  ;; Faithful port of Emacs `seq-group-by' (seq.el): fold over the REVERSED
  ;; sequence, prepending newly-seen keys and pushing each element onto its
  ;; group's cdr. This yields groups in reverse first-encounter order with
  ;; items in forward order.
  (seq-reduce
   (lambda (acc elt)
     (let* ((key (funcall fn elt)) (cell (assoc key acc)))
       (if cell (setcdr cell (cons elt (cdr cell)))
         (setq acc (cons (list key elt) acc)))
       acc))
   (reverse (append seq nil))
   nil))

(defun plist-put (plist prop val &optional predicate)
  ;; Mutate PLIST in place: overwrite an existing PROP, or append (PROP VAL) to
  ;; the tail via setcdr. Only a nil PLIST yields a fresh list (can't mutate nil).
  ;; A non-list PLIST is `plistp' -- its own predicate, not `listp'.
  (unless (listp plist) (signal 'wrong-type-argument (list 'plistp plist)))
  (let ((test (or predicate #'eq)))
   (if (null plist)
      (list prop val)
    (let ((p plist) (done nil))
      (while (not done)
        (cond
         ((funcall test (car p) prop) (setcar (cdr p) val) (setq done t))
         ((cddr p) (setq p (cddr p)))
         (t (setcdr (cdr p) (list prop val)) (setq done t))))
      plist))))
;; Symbol property lists, backed by a hash table (we have no per-symbol plist
;; slot). Enough for `get`/`put`/`define-error` and the error-condition system.
(defvar symbol-plist--table (make-hash-table :test 'eq))
(defun put (sym prop val)
  (puthash sym (plist-put (gethash sym symbol-plist--table) prop val) symbol-plist--table)
  val)
(defun get (sym prop) (plist-get (gethash sym symbol-plist--table) prop))
(defun symbol-plist (sym) (gethash sym symbol-plist--table))
(defun setplist (sym plist) (puthash sym plist symbol-plist--table) plist)
;; Function properties share the symbol plist table.
(defun function-get (f prop &optional _autoload) (get f prop))
(defun function-put (f prop value) (put f prop value))
(defun define-symbol-prop (symbol prop value) (put symbol prop value))
;; Records: elisprs models them (and cl-defstruct instances) as vectors tagged
;; `cl-struct-TYPE' in slot 0, so `type-of'/printing report the bare TYPE. (aref
;; slot 0 yields the tag rather than the bare type, a known representation quirk.)
(defun record (type &rest slots)
  (apply #'vector (intern (concat "cl-struct-" (symbol-name type))) slots))
(defun make-record (type slots init)
  ;; Emacs's `Fmake_record' checks SLOTS is a wholenum, then caps the record at
  ;; PSEUDOVECTOR_SIZE_MASK (4095) slots; anything larger signals a plain error
  ;; rather than attempting an impossible allocation.
  (unless (and (integerp slots) (>= slots 0) (<= slots most-positive-fixnum))
    (signal 'wrong-type-argument (list 'wholenump slots)))
  (when (> (1+ slots) 4095)
    (error "Attempt to allocate a record of %d slots; max is %d" (1+ slots) 4095))
  (let ((v (make-vector (1+ slots) init)))
    (aset v 0 (intern (concat "cl-struct-" (symbol-name type))))
    v))
(defun define-error (name message &optional parent)
  ;; Register NAME as an error condition: its conditions are NAME plus PARENT's.
  (let* ((parent (or parent 'error))
         (parents (if (listp parent) parent (list parent)))
         (conds (cons name (apply (function append)
                                  (mapcar (lambda (p) (get p 'error-conditions)) parents)))))
    (put name 'error-conditions conds)
    (put name 'error-message message)
    name))
;; Seed the standard error symbols (so `error-message-string' / `get' match Emacs).
(define-error 'error "error" nil)
(put 'error 'error-conditions '(error))
(define-error 'user-error "")
(define-error 'quit "Quit" nil)
(define-error 'args-out-of-range "Args out of range")
(define-error 'arith-error "Arithmetic error")
(define-error 'type-mismatch "Types do not match")
(define-error 'wrong-type-argument "Wrong type argument")
(define-error 'wrong-number-of-arguments "Wrong number of arguments")
(define-error 'void-variable "Symbol's value as variable is void")
(define-error 'void-function "Symbol's function definition is void")
(define-error 'invalid-function "Invalid function")
(define-error 'wrong-length-argument "Wrong length argument")
(define-error 'invalid-regexp "Invalid regexp")
(define-error 'cyclic-variable-indirection "Cyclic variable indirection")
(define-error 'cl-assertion-failed "Assertion failed")
(define-error 'end-of-file "End of file during parsing")
(defun add-to-list (var elt &optional append compare-fn)
  ;; Add ELT to VAR's list unless already present (COMPARE-FN, default `equal');
  ;; prepend by default, or append to the end when APPEND is non-nil.
  (let ((cur (symbol-value var)) (test (or compare-fn #'equal)) (found nil))
    (dolist (x cur) (when (funcall test elt x) (setq found t)))
    (if found cur
      (set var (if append (append cur (list elt)) (cons elt cur))))))
;; equal-including-properties compares text as `equal' does, and additionally
;; requires matching text properties on strings.
(defun equal-including-properties (a b)
  (and (equal a b)
       (or (not (stringp a))
           (let ((i 0) (n (length a)) (ok t))
             (while (and ok (< i n))
               (unless (--plists-equal (text-properties-at i a)
                                       (text-properties-at i b))
                 (setq ok nil))
               (setq i (1+ i)))
             ok))))

;; ── text-property scanning (built on the primitive get/put/at) ──
;; Structural plist equality: same key -> `eq' value set (a nil value = absent).
(defun --plist-subset (a b)
  (let ((ok t) (p a))
    (while (and ok p)
      (unless (eq (cadr p) (plist-get b (car p))) (setq ok nil))
      (setq p (cddr p)))
    ok))
(defun --plists-equal (a b) (and (--plist-subset a b) (--plist-subset b a)))
;; Overlays are not modeled, so get-char-property falls straight through to the
;; text properties (NAMED boundary: no overlay lookup layer).
(defun get-char-property (pos prop &optional object)
  (get-text-property pos prop object))
(defun next-single-property-change (pos prop &optional object limit)
  (let* ((end (if (stringp object) (length object) (point-max)))
         (lim (if limit (min limit end) end))
         (val (get-text-property pos prop object))
         (p (1+ pos)) (res nil))
    (while (and (null res) (< p lim))
      (if (not (eq val (get-text-property p prop object)))
          (setq res p)
        (setq p (1+ p))))
    (or res (if limit lim nil))))
(defun next-property-change (pos &optional object limit)
  (let* ((end (if (stringp object) (length object) (point-max)))
         (lim (if limit (min limit end) end))
         (val (text-properties-at pos object))
         (p (1+ pos)) (res nil))
    (while (and (null res) (< p lim))
      (if (not (--plists-equal val (text-properties-at p object)))
          (setq res p)
        (setq p (1+ p))))
    (or res (if limit lim nil))))
(defun previous-single-property-change (pos prop &optional object limit)
  (let ((start (if (stringp object) 0 (point-min))))
    (if (<= pos start) nil
      (let* ((lim (if limit (max limit start) start))
             (val (get-text-property (1- pos) prop object))
             (p (1- pos)) (res nil))
        (while (and (null res) (> p lim))
          (if (not (eq val (get-text-property (1- p) prop object)))
              (setq res p)
            (setq p (1- p))))
        (or res (if limit lim nil))))))

(defun apply-partially (fn &rest args) (lambda (&rest more) (apply fn (append args more))))
(defun complement (fn) (lambda (&rest args) (not (apply fn args))))
(defun cl-constantly (x) (lambda (&rest --ignore--) x))

(defun string-chop-newline (s) (if (string-suffix-p "\n" s) (substring s 0 (- (length s) 1)) s))
(defun pp-to-string (object) (concat (prin1-to-string object) "\n"))
(defun pp (object &optional _stream) (princ (pp-to-string object)) nil)
;; cl-print: elisprs has no special struct/closure rendering, so these fall back
;; to the ordinary printer.
(defun cl-prin1-to-string (object) (prin1-to-string object))
(defun cl-prin1 (object &optional stream) (prin1 object stream))
(defun format-spec (format specification &rest _)
  ;; Replace each %X in FORMAT with the cdr of (X . VALUE) in SPECIFICATION; %% => %.
  (let ((i 0) (n (length format)) (out nil))
    (while (< i n)
      (let ((c (aref format i)))
        (if (and (eq c ?%) (< (1+ i) n))
            (let ((nc (aref format (1+ i))))
              (if (eq nc ?%) (setq out (cons "%" out))
                (let ((cell (assq nc specification)))
                  (setq out (cons (if cell (format "%s" (cdr cell)) "") out))))
              (setq i (+ i 2)))
          (setq out (cons (char-to-string c) out))
          (setq i (1+ i)))))
    (apply 'concat (nreverse out))))
(defun subst-char-in-string (from to string &optional _inplace)
  (concat (mapcar (lambda (c) (if (eq c from) to c)) string)))
(defun string-bytes (s)
  ;; Number of bytes in the UTF-8 encoding of S.
  (let ((n 0) (l (string-to-list s)))
    (while l
      (let ((c (car l)))
        (setq n (+ n (cond ((< c 128) 1) ((< c 2048) 2) ((< c 65536) 3) (t 4)))))
      (setq l (cdr l)))
    n))
(defun char-width (c)
  ;; 0 for combining marks, 2 for East-Asian wide/fullwidth, else 1.
  (cond
   ;; Control chars: newline 0, tab tab-width (8), others display as `^X' (2).
   ((eq c ?\n) 0)
   ((eq c ?\t) 8)
   ((or (< c 32) (eq c 127)) 2)
   ((or (and (>= c #x0300) (<= c #x036F)) (and (>= c #x200B) (<= c #x200F))) 0)
   ((or (and (>= c #x1100) (<= c #x115F))   ; Hangul Jamo
        (and (>= c #x2E80) (<= c #x303E))   ; CJK radicals … symbols
        (and (>= c #x3041) (<= c #x33FF))   ; Hiragana … CJK compat
        (and (>= c #x3400) (<= c #x4DBF))   ; CJK ext A
        (and (>= c #x4E00) (<= c #x9FFF))   ; CJK unified
        (and (>= c #xA000) (<= c #xA4CF))   ; Yi
        (and (>= c #xAC00) (<= c #xD7A3))   ; Hangul syllables
        (and (>= c #xF900) (<= c #xFAFF))   ; CJK compat ideographs
        (and (>= c #xFF00) (<= c #xFF60))   ; Fullwidth forms
        (and (>= c #xFFE0) (<= c #xFFE6))
        (and (>= c #x1F300) (<= c #x1F64F)) ; Misc symbols & pictographs, emoticons
        (and (>= c #x1F900) (<= c #x1F9FF)) ; Supplemental symbols & pictographs
        (and (>= c #x1FA70) (<= c #x1FAFF)) ; Symbols & pictographs ext A
        (and (>= c #x20000) (<= c #x3FFFD))) ; CJK ext B+
    2)
   (t 1)))
(defun string-width (s &optional _from _to)
  (let ((w 0) (l (string-to-list s)))
    (while l (setq w (+ w (char-width (car l))) l (cdr l)))
    w))
(defun truncate-string-to-width (str end-column &optional start-column padding ellipsis)
  ;; Truncate STR so its display width is at most END-COLUMN (from START-COLUMN).
  ;; When ELLIPSIS is non-nil and STR is truncated, append it (t means "…"),
  ;; keeping the total width within END-COLUMN.
  (let* ((chars (string-to-list str))
         (total (let ((w 0)) (dolist (c chars) (setq w (+ w (char-width c)))) w))
         (truncated (> total end-column))
         (ell (cond ((null ellipsis) "") ((eq ellipsis t) "…") (t ellipsis)))
         (limit (if truncated (- end-column (string-width ell)) end-column))
         (col 0) (start (or start-column 0)) (out nil) (l chars) (stop nil))
    (while (and l (not stop))
      (let* ((c (car l)) (cw (char-width c)))
        (if (> (+ col cw) limit) (setq stop t)
          (when (>= col start) (setq out (cons c out)))
          (setq col (+ col cw))))
      (setq l (cdr l)))
    (setq out (nreverse out))
    (when truncated (setq out (append out (string-to-list ell)) col (+ col (string-width ell))))
    (when (and padding (< col end-column))
      (let ((pad nil))
        (while (< col end-column) (setq pad (cons padding pad) col (1+ col)))
        (setq out (append out pad))))
    (concat out)))
(defun cl-type-of (obj)
  (cond ((null obj) 'null)
        ((integerp obj) 'fixnum)
        ((floatp obj) 'float)
        ((symbolp obj) 'symbol)
        ((stringp obj) 'string)
        ((consp obj) 'cons)
        (t (type-of obj))))
(defun number-or-marker-p (x) (or (numberp x) (markerp x)))
(defun integer-or-marker-p (x) (or (integerp x) (markerp x)))
(defun string-pad (s len &optional padding start)
  ;; Pad S to LENGTH chars with PADDING (default space); pad on the left when
  ;; START is non-nil, otherwise on the right.
  (unless (natnump len) (signal 'wrong-type-argument (list 'natnump len)))
  (let ((pad (or padding 32)) (cur (length s)))
    (if (>= cur len) s
      (let ((fill (make-string (- len cur) pad)))
        (if start (concat fill s) (concat s fill))))))
;; A horizontal rule: LENGTH dashes (default 79, the batch text width) + newline.
;; (Emacs returns a string with a face applied; the plain text is returned here.)
(defun make-separator-line (&optional length)
  (concat (make-string (or length 79) ?-) "\n"))
(defun string-lines (string &optional omit-nulls keep-newlines)
  ;; Split STRING into a list of lines on newline.  When OMIT-NULLS, drop empty
  ;; lines; when KEEP-NEWLINES, retain the trailing newline on each line.
  (if (equal string "")
      (list "")
    (let ((lines nil) (start 0) (len (length string)))
      (while (< start len)
        (let ((nl (string-search "\n" string start)))
          (if nl
              (progn
                (let ((line (substring string start (if keep-newlines (1+ nl) nl))))
                  (when (or (not omit-nulls) (not (string= line "")))
                    (push line lines)))
                (setq start (1+ nl)))
            (let ((line (substring string start)))
              (when (or (not omit-nulls) (not (string= line "")))
                (push line lines))
              (setq start len)))))
      (nreverse lines))))
(defun string-glyph-split (string)
  ;; Split STRING into its glyphs.  Without composition tables this is a
  ;; per-character split, which matches Emacs for non-composed text.
  (mapcar #'char-to-string (string-to-list string)))
(defun string-limit (string length &optional end _coding-system)
  ;; First LENGTH chars of STRING (or last LENGTH when END is non-nil); the whole
  ;; string if it is already short enough. (Byte-based CODING-SYSTEM unsupported.)
  (let ((len (length string)))
    (if (<= len length)
        string
      (if end
          (substring string (- len length) len)
        (substring string 0 length)))))
(defun string-fill (s len)
  ;; Greedily wrap S so no line exceeds LEN columns, breaking only at spaces.
  (let ((words (split-string s)) (lines nil) (cur ""))
    (dolist (w words)
      (cond ((string= cur "") (setq cur w))
            ((<= (+ (length cur) 1 (length w)) len) (setq cur (concat cur " " w)))
            (t (push cur lines) (setq cur w))))
    (unless (string= cur "") (push cur lines))
    (mapconcat 'identity (reverse lines) "\n")))
(defun string-equal-ignore-case (a b)
  (unless (or (stringp a) (symbolp a)) (signal 'wrong-type-argument (list 'stringp a)))
  (unless (or (stringp b) (symbolp b)) (signal 'wrong-type-argument (list 'stringp b)))
  (string= (downcase (string--name a)) (downcase (string--name b))))
(defun upcase-initials (s)
  ;; Upcase the first letter of every word, leaving the rest unchanged.
  (char-or-string--check s)
  (let ((out nil) (in-word nil))
    (dolist (c (string-to-list s))
      (let* ((lower (and (>= c ?a) (<= c ?z)))
             (alnum (or lower (and (>= c ?A) (<= c ?Z)) (and (>= c ?0) (<= c ?9)))))
        (cond ((not alnum) (setq out (cons c out)))
              (in-word (setq out (cons c out)))
              (t (setq out (cons (if lower (- c 32) c) out))))
        (setq in-word alnum)))
    (apply (function string) (reverse out))))
(defun string-replace (from to s)
  ;; Emacs signals on an empty FROMSTRING rather than looping forever.
  (if (string-empty-p from) (signal 'wrong-length-argument (list 0))
    (let ((out "") (pos (string-search from s)))
      (while pos
        (setq out (concat out (substring s 0 pos) to))
        (setq s (substring s (+ pos (length from)) (length s)))
        (setq pos (string-search from s)))
      (concat out s))))
;; Shell-like quoting (subr.el): combine strings into one quoted command line and
;; split one back, respecting double-quoted segments (read as elisp string literals).
(defun combine-and-quote-strings (strings &optional separator)
  (let* ((sep (or separator " "))
         (re (concat "[\\\"]\\|" (regexp-quote sep))))
    (mapconcat
     (lambda (str)
       (if (or (= (length str) 0) (string-match re str))
           (concat "\"" (replace-regexp-in-string "[\\\"]" "\\\\\\&" str) "\"")
         str))
     strings sep)))
(defun split-string-and-unquote (string &optional separator)
  (let ((sep (or separator "[ \t\n\r\f]+"))
        (i (string-search "\"" string)))
    (if (null i)
        (split-string string sep t)
      (append (unless (= i 0) (split-string (substring string 0 i) sep t))
              (let ((rfs (read-from-string string i)))
                (cons (car rfs)
                      (split-string-and-unquote (substring string (cdr rfs)) sep)))))))
;;; ---- json.el (encode subset) ----
;; Encode elisp data to a JSON string (the forgiving json.el `json-encode' API,
;; not the strict native `json-serialize'). Lists are objects when they look like
;; an alist/plist, else arrays; nil encodes as null; symbols as their name.
(defun json-encode-string (s)
  (let ((out "\""))
    (dolist (c (string-to-list s))
      (setq out (concat out
                        (cond ((eq c 34) "\\\"")
                              ((eq c 92) "\\\\")
                              ((eq c 8) "\\b")
                              ((eq c 9) "\\t")
                              ((eq c 10) "\\n")
                              ((eq c 12) "\\f")
                              ((eq c 13) "\\r")
                              ((< c 32) (format "\\u%04x" c))
                              (t (char-to-string c))))))
    (concat out "\"")))
(defun json-alist-p (l)
  (and (consp l) (let ((ok t)) (dolist (e l) (unless (consp e) (setq ok nil))) ok)))
(defun json-plist-p (l) (and (consp l) (keywordp (car l))))
(defun json--encode-key (k)
  (json-encode-string (cond ((stringp k) k)
                            ((keywordp k) (substring (symbol-name k) 1))
                            ((symbolp k) (symbol-name k))
                            (t (format "%s" k)))))
(defun json--encode-kv (k v) (concat (json--encode-key k) ":" (json-encode v)))
(defun json-encode (object)
  (cond
   ((eq object t) "true")
   ((eq object json-false) "false")
   ((eq object json-null) "null")
   ((null object) "null")
   ((stringp object) (json-encode-string object))
   ((keywordp object) (json-encode-string (substring (symbol-name object) 1)))
   ((numberp object) (number-to-string object))
   ((hash-table-p object)
    (let ((parts nil))
      (maphash (lambda (k v) (setq parts (cons (json--encode-kv k v) parts))) object)
      (concat "{" (mapconcat (function identity) (nreverse parts) ",") "}")))
   ((vectorp object)
    (concat "[" (mapconcat (function json-encode) (append object nil) ",") "]"))
   ((json-alist-p object)
    (concat "{" (mapconcat (lambda (p) (json--encode-kv (car p) (cdr p))) object ",") "}"))
   ((json-plist-p object)
    (let ((parts nil) (l object))
      (while l (setq parts (cons (json--encode-kv (car l) (car (cdr l))) parts)) (setq l (cddr l)))
      (concat "{" (mapconcat (function identity) (nreverse parts) ",") "}")))
   ((listp object) (concat "[" (mapconcat (function json-encode) object ",") "]"))
   ((symbolp object) (json-encode-string (symbol-name object)))
   (t (error "Unknown JSON object: %S" object))))
;; json.el reading (json-read-from-string). Defaults: objects → alists with
;; symbol keys, arrays → vectors, true→t, false→json-false, null→json-null.
(defvar json-object-type 'alist)
(defvar json-array-type 'vector)
(defvar json-key-type nil)
(defvar json-null nil)
(defvar json-false :json-false)
(defun json--skip-ws (s i)
  (let ((n (length s)))
    (while (and (< i n) (memq (aref s i) '(32 9 10 13))) (setq i (1+ i)))
    i))
(defun json--lookahead (s i word)
  (and (<= (+ i (length word)) (length s)) (string= (substring s i (+ i (length word))) word)))
(defun json--read-string (s i)
  (setq i (1+ i))
  (let ((out "") (done nil))
    (while (not done)
      (let ((c (aref s i)))
        (cond
         ((eq c 34) (setq i (1+ i) done t))
         ((eq c 92)
          (setq i (1+ i))
          (let ((e (aref s i)))
            (if (eq e ?u)
                (setq out (concat out (char-to-string (string-to-number (substring s (1+ i) (+ i 5)) 16)))
                      i (+ i 5))
              (setq out (concat out (char-to-string
                                     (cond ((eq e ?n) 10) ((eq e ?t) 9) ((eq e ?r) 13)
                                           ((eq e ?b) 8) ((eq e ?f) 12) ((eq e ?/) 47)
                                           (t e))))
                    i (1+ i)))))
         (t (setq out (concat out (char-to-string c)) i (1+ i))))))
    (cons out i)))
(defun json--read-number (s i)
  (let ((start i) (n (length s)))
    (while (and (< i n) (memq (aref s i) '(?- ?+ ?0 ?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9 ?. ?e ?E)))
      (setq i (1+ i)))
    (cons (string-to-number (substring s start i)) i)))
(defun json--read-array (s i)
  (setq i (json--skip-ws s (1+ i)))
  (let ((items nil))
    (if (eq (aref s i) ?\])
        (setq i (1+ i))
      (let ((done nil))
        (while (not done)
          (let ((r (json--read s i)))
            (setq items (cons (car r) items) i (json--skip-ws s (cdr r))))
          (cond ((eq (aref s i) ?,) (setq i (json--skip-ws s (1+ i))))
                ((eq (aref s i) ?\]) (setq i (1+ i) done t))
                (t (error "json-read: malformed array"))))))
    (setq items (nreverse items))
    (cons (if (eq json-array-type 'list) items (vconcat items)) i)))
(defun json--object-key (k)
  (cond ((eq json-key-type 'string) k)
        ((eq json-key-type 'keyword) (intern (concat ":" k)))
        (t (intern k))))
(defun json--build-object (pairs)
  (cond
   ((eq json-object-type 'hash-table)
    (let ((h (make-hash-table :test 'equal))) (dolist (p pairs) (puthash (car p) (cdr p) h)) h))
   ((eq json-object-type 'plist)
    (let ((out nil)) (dolist (p pairs) (setq out (cons (cdr p) (cons (intern (concat ":" (car p))) out)))) (nreverse out)))
   (t (mapcar (lambda (p) (cons (json--object-key (car p)) (cdr p))) pairs))))
(defun json--read-object (s i)
  (setq i (json--skip-ws s (1+ i)))
  (let ((pairs nil))
    (if (eq (aref s i) ?})
        (setq i (1+ i))
      (let ((done nil))
        (while (not done)
          (let ((kr (json--read-string s i)))
            (setq i (json--skip-ws s (cdr kr)))
            (setq i (json--skip-ws s (1+ i)))  ; skip the ':'
            (let ((vr (json--read s i)))
              (setq pairs (cons (cons (car kr) (car vr)) pairs) i (json--skip-ws s (cdr vr)))))
          (cond ((eq (aref s i) ?,) (setq i (json--skip-ws s (1+ i))))
                ((eq (aref s i) ?}) (setq i (1+ i) done t))
                (t (error "json-read: malformed object"))))))
    (cons (json--build-object (nreverse pairs)) i)))
(defun json--read (s i)
  (setq i (json--skip-ws s i))
  (let ((c (aref s i)))
    (cond
     ((eq c ?{) (json--read-object s i))
     ((eq c ?\[) (json--read-array s i))
     ((eq c 34) (json--read-string s i))
     ((or (eq c ?-) (and (>= c ?0) (<= c ?9))) (json--read-number s i))
     ((json--lookahead s i "true") (cons t (+ i 4)))
     ((json--lookahead s i "false") (cons json-false (+ i 5)))
     ((json--lookahead s i "null") (cons json-null (+ i 4)))
     (t (error "json-read: unexpected character")))))
(defun json-read-from-string (s) (car (json--read s 0)))
;; Native JSON API (Emacs 27+): keyword args, hash-table/string-key defaults,
;; :null / :false objects — built on the json.el machinery via dynamic binding.
(defun json-parse-string (string &rest args)
  (let ((json-object-type (or (plist-get args :object-type) 'hash-table))
        (json-array-type (or (plist-get args :array-type) 'vector))
        (json-null (if (plist-member args :null-object) (plist-get args :null-object) :null))
        (json-false (if (plist-member args :false-object) (plist-get args :false-object) :false)))
    (json-read-from-string string)))
(defun json-serialize (object &rest args)
  (let ((json-null (if (plist-member args :null-object) (plist-get args :null-object) :null))
        (json-false (if (plist-member args :false-object) (plist-get args :false-object) :false)))
    (json-encode object)))
;;; ---- file-name path manipulation (pure, no filesystem) ----
(defun file-name--last (f ch)
  (let ((i (length f)) (res nil))
    (while (and (> i 0) (null res))
      (setq i (1- i))
      (when (eq (aref f i) ch) (setq res i)))
    res))
(defun file-name-directory (f)
  (let ((i (file-name--last f ?/))) (and i (substring f 0 (1+ i)))))
(defun file-name-nondirectory (f)
  (let ((i (file-name--last f ?/))) (if i (substring f (1+ i)) f)))
(defun file-name-extension (f &optional period)
  (let* ((nd (file-name-nondirectory f)) (i (file-name--last nd ?.)))
    (and i (> i 0) (if period (substring nd i) (substring nd (1+ i))))))
(defun file-name-sans-extension (f)
  (let* ((nd (file-name-nondirectory f)) (i (file-name--last nd ?.)))
    (if (and i (> i 0)) (concat (or (file-name-directory f) "") (substring nd 0 i)) f)))
(defun file-name-base (f) (file-name-sans-extension (file-name-nondirectory f)))
(defun file-name-as-directory (f)
  (cond ((string= f "") "./")
        ((eq (aref f (1- (length f))) ?/) f)
        (t (concat f "/"))))
;; `temporary-file-directory' (Vtemporary_file_directory, callproc.c
;; `init_callproc') is a C variable read at load time by stock lisp (files.el,
;; jka-compr.el, url-*.el, ...). The Rust primitive returns the raw dir
;; ($TMPDIR if present -- even empty -- else the macOS Darwin per-user temp dir
;; from confstr(_CS_DARWIN_USER_TEMP_DIR), else "/tmp/"); `file-name-as-directory'
;; adds the trailing slash exactly as the C `Ffile_name_as_directory' call does.
;; Values verified against GNU Emacs 30.2 for TMPDIR set/unset/empty.
(defvar temporary-file-directory
  (file-name-as-directory (--temp-directory--)))
;; No separate small-file temp dir by default, matching `emacs -Q'.
(defvar small-temporary-file-directory nil)
;; `make-temp-name' (Fmake_temp_name, fileio.c) delegates to
;; `make-temp-file-internal' with DIR-FLAG 0, which calls the gnulib
;; `gen_tempname' (tempname.c) in its GT_NOCREATE mode: it replaces a run of
;; 6 `X's with random characters drawn from the 62-char base
;; "a..zA..Z0..9", and loops (up to 62^3 = 238328 attempts) until the name
;; belongs to no existing file (glibc `try_nocreate' uses lstat, so a broken
;; symlink also counts as existing). The random suffix makes the result
;; non-deterministic, so this matches Emacs's observable contract -- PREFIX +
;; 6 base62 chars naming a nonexistent file -- rather than a fixed value.
(defvar make-temp-name--letters
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
  "The 62-char base gnulib `gen_tempname' draws temp-name suffix chars from.")
(defun make-temp-name (prefix)
  "Generate temporary file name (string) starting with PREFIX (a string).

This function tries to choose a name that has no existing file.
For this to work, PREFIX should be an absolute file name, and PREFIX
and the returned string should both be non-magic."
  (unless (stringp prefix)
    (signal 'wrong-type-argument (list 'stringp prefix)))
  (let ((attempts 238328)          ; 62 * 62 * 62, gnulib ATTEMPTS_MIN
        (result nil))
    (while (and (null result) (> attempts 0))
      (setq attempts (1- attempts))
      (let ((candidate prefix)
            (i 0))
        (while (< i 6)
          (setq candidate
                (concat candidate
                        (char-to-string
                         (aref make-temp-name--letters (random 62)))))
          (setq i (1+ i)))
        ;; lstat semantics: a real file OR a (possibly broken) symlink counts.
        (unless (or (file-exists-p candidate) (file-symlink-p candidate))
          (setq result candidate))))
    (unless result
      (signal 'file-error (list "Creating file name with prefix" prefix)))
    result))
(defun directory-file-name (f)
  (if (and (> (length f) 1) (eq (aref f (1- (length f))) ?/))
      (substring f 0 (1- (length f)))
    f))
(defun file-name-concat (&rest parts)
  (let ((out "") (first t))
    (dolist (p parts)
      (unless (string= p "")
        (if first (setq out p first nil)
          (setq out (concat out (if (eq (aref out (1- (length out))) ?/) "" "/") p)))))
    out))
(defun file-name-absolute-p (f)
  (and (> (length f) 0) (or (eq (aref f 0) ?/) (eq (aref f 0) ?~)) t))
(defun file-name-split (f) (split-string f "/"))
(defun directory-name-p (name)
  (and (> (length name) 0) (eq (aref name (1- (length name))) ?/)))
(defun file-name-with-extension (filename extension)
  (let ((extn (string-trim-left extension "[.]")))
    (cond ((string-empty-p filename) (error "Empty filename"))
          ((string-empty-p extn) (error "Malformed extension: %s" extension))
          ((directory-name-p filename) (error "Filename is a directory: %s" filename))
          (t (concat (file-name-sans-extension filename) "." extn)))))
(defun file-name-parent-directory (filename)
  (let* ((expanded (expand-file-name filename))
         (parent (file-name-directory (directory-file-name expanded))))
    (cond ((or (null parent) (equal parent expanded)) nil)
          ((not (file-name-absolute-p filename)) (file-relative-name parent))
          (t parent))))
;; "/:" quoting marks a name as literal (no remote/wildcard expansion).
(defun file-name-quote (name &optional _top)
  (if (string-prefix-p "/:" name) name (concat "/:" name)))
(defun file-name-unquote (name &optional _top)
  (if (string-prefix-p "/:" name) (substring name 2) name))
(defun file-name-quoted-p (name &optional _top) (string-prefix-p "/:" name))
;; POSIX: standard file names need no conversion.
(defun convert-standard-filename (filename) filename)
(defvar default-directory (file-name-as-directory (--current-directory--)))
;; `file-name-handler-alist' (C `Vfile_name_handler_alist', fileio.c): alist of
;; (REGEXP . HANDLER) pairs consulted by file-name operations to dispatch magic
;; file names. The C variable is initialized to nil; the batch default below is
;; the post-loadup set Emacs 30.2 reports under `emacs -Q --batch' — the epa,
;; jka-compr, tramp, and `file-name-non-special' handlers installed at startup.
;; elisprs handles compressed loads in Rust (no dispatch through this alist), so
;; this is data other code reads/rebinds, matching the oracle value-for-value.
(defvar file-name-handler-alist
  '(("\\.gpg\\(~\\|\\.~[0-9]+~\\)?\\'" . epa-file-handler)
    ("\\(?:\\.tzst\\|\\.zst\\|\\.dz\\|\\.txz\\|\\.xz\\|\\.lzma\\|\\.lz\\|\\.g?z\\|\\.\\(?:tgz\\|svgz\\|sifz\\)\\|\\.tbz2?\\|\\.bz2\\|\\.Z\\)\\(?:~\\|\\.~[-[:alnum:]:#@^._]+\\(?:~[[:digit:]]+\\)?~\\)?\\'" . jka-compr-handler)
    ("\\`/\\(?:-\\|[^/:|]\\{2,\\}\\):" . tramp-autoload-file-name-handler)
    ("\\`/:" . file-name-non-special)))
;; `load' machinery. These are dynamic (special) variables the `load' builtin
;; rebinds around a file's evaluation and restores afterward. At top level
;; `load-file-name'/`load-true-file-name' are nil and `load-in-progress' is nil,
;; matching Emacs. `load-path' is searched for a bare (directory-less) FILE;
;; `load-suffixes' are the extensions tried (elisprs has no bytecode, so `.elc'
;; is never found); crossed with `load-file-rep-suffixes' = `("" ".gz")', so
;; `.el', `.el.gz', the exact name, and its `.gz' variant all resolve.
(defvar load-path nil)
(defvar load-file-name nil)
(defvar load-true-file-name nil)
(defvar load-in-progress nil)
(defvar load-suffixes '(".elc" ".el"))
;; jka-compr (auto-compression-mode, on by default) sets this to `("" ".gz")' so
;; `load' transparently finds and gunzips compressed libraries — the stock Emacs
;; lisp tree ships as `*.el.gz'. The `load' builtin honors the `.gz' rep-suffix.
(defvar load-file-rep-suffixes '("" ".gz"))
;; get-load-suffixes (C `Fget_load_suffixes', lread.c): the cross product of
;; `load-suffixes' with `load-file-rep-suffixes', in the same order the C loop
;; conses then nreverses — suffix0+rep0, suffix0+rep1, suffix1+rep0, ...
(defun get-load-suffixes ()
  "Return the list of suffixes that `load' should try, in order."
  (let ((lst nil))
    (dolist (suffix load-suffixes)
      (dolist (ext load-file-rep-suffixes)
        (setq lst (cons (concat suffix ext) lst))))
    (nreverse lst)))
;; locate-library (subr.el:3153): resolve LIBRARY to the file `load' would pick,
;; searching `load-path' (or PATH) with the load + rep suffixes.
(defun locate-library (library &optional nosuffix path interactive-call)
  "Show the precise file name of Emacs library LIBRARY.
LIBRARY should be a relative file name of the library, a string.
Optional second arg NOSUFFIX non-nil means don't add suffixes.
Optional third arg PATH is a list of directories to search instead
of `load-path'."
  (let ((file (locate-file library
			   (or path load-path)
			   (append (unless nosuffix (get-load-suffixes))
				   load-file-rep-suffixes))))
    (if interactive-call
	(if file
	    (message "Library is file %s" (abbreviate-file-name file))
	  (message "No library %s in search path" library)))
    file))
;; Emacs startup variables (startup.el). `init-file-debug' gates verbose init
;; error reporting; nil by default. `user-init-file'/`user-emacs-directory' are
;; set by real Emacs before init loads — nil here until a caller binds them.
(defvar init-file-debug nil)
(defvar user-init-file nil)
;; custom.el:1216 — searched by `load-theme'/`customize-themes'. Faithful port of
;; the preloaded defvar; value is the literal list `(custom-theme-directory t)'
;; (the two symbols are quoted, not evaluated), matching the real Emacs default.
(defvar custom-theme-load-path (list 'custom-theme-directory t)
  "List of directories to search for custom theme files.")
(defun expand-file-name (name &optional dir)
  ;; Expand NAME against DIR (default `default-directory'), `~/' via $HOME, and
  ;; collapse `.', `..' and `//'. (No remote/`~user' handling.)
  (setq dir (or dir default-directory))
  (cond ((string-prefix-p "~/" name) (setq name (concat (or (getenv "HOME") "~") (substring name 1))))
        ((string= name "~") (setq name (or (getenv "HOME") "~"))))
  (let* ((combined (if (file-name-absolute-p name) name (concat (file-name-as-directory dir) name)))
         (abs (string-prefix-p "/" combined))
         (trail (and (> (length combined) 1) (string-suffix-p "/" combined)))
         (out nil))
    (dolist (c (split-string combined "/"))
      (cond ((or (string= c "") (string= c ".")) nil)
            ((string= c "..") (when out (setq out (cdr out))))
            (t (setq out (cons c out)))))
    (let ((res (concat (if abs "/" "") (mapconcat (function identity) (nreverse out) "/"))))
      (when (and trail (not (string-suffix-p "/" res))) (setq res (concat res "/")))
      (if (string= res "") (if abs "/" ".") res))))
(defun file-relative-name (filename &optional dir)
  ;; Common-prefix relativization (no `../' climb when FILENAME isn't under DIR).
  (let ((d (file-name-as-directory (expand-file-name (or dir default-directory))))
        (f (expand-file-name filename)))
    (if (string-prefix-p d f) (substring f (length d)) f)))
(defun abbreviate-file-name (filename)
  (let ((home (getenv "HOME")))
    (if (and home (string-prefix-p (file-name-as-directory home) filename))
        (concat "~/" (substring filename (length (file-name-as-directory home))))
      filename)))
(defun directory-files (dir &optional full match nosort)
  ;; Names in DIR (incl. "." ".."), sorted unless NOSORT, filtered by MATCH regexp;
  ;; with FULL, each name is prefixed with DIR's expanded path (uncollapsed).
  (let ((names (--directory-files-- dir match nosort)))
    (if full
        (let ((d (file-name-as-directory (expand-file-name dir))))
          (mapcar (lambda (n) (concat d n)) names))
      names)))

;; ── exec-path / file-location subsystem ──────────────────────────────────
;; Faithful port of the process/file-search machinery Emacs builds on:
;; `path-separator', `exec-suffixes', `exec-directory', `exec-path',
;; `invocation-name'/`invocation-directory', `executable-find', `locate-file'
;; (the last two are verbatim from files.el; `locate-file-internal' reimplements
;; the C `openp' search from lread.c). Values match `emacs -Q --batch' on unix
;; for the parts that are OS-defined (path-separator, exec-suffixes) and mirror
;; Emacs's structure for the rest.
(defvar path-separator ":"
  "The directory separator in search paths, as a string.")
(defvar exec-suffixes '("")
  "List of suffixes to try to find executable file names (empty on POSIX).")
;; Emacs's `exec-directory' is the libexec dir holding its C helper programs;
;; elisprs has none, so it points at the directory of the running `elisp'
;; binary — a sensible, real directory that keeps `exec-path's structure
;; (PATH dirs + exec-directory) intact. `invocation-name'/`invocation-directory'
;; are the binary's basename and directory, mirroring Emacs's C-level values.
(defvar invocation-name (file-name-nondirectory (--invocation-file--))
  "The program name that was used to run this instance of elisprs.")
(defvar invocation-directory (file-name-directory (--invocation-file--))
  "The directory in which the elisprs executable was found, to run subprocesses.")
(defvar exec-directory (file-name-as-directory (file-name-directory (--invocation-file--)))
  "Directory of architecture-dependent files that come with elisprs.")
;; Emacs's `data-directory' is the `etc/' dir of the installation (a string,
;; used by libraries such as `shadow' via `expand-file-name'); elisprs ships no
;; `etc/', so — like `exec-directory' — it points at the running binary's
;; directory, keeping it a real, absolute string.
(defvar data-directory (file-name-as-directory (file-name-directory (--invocation-file--)))
  "Directory of machine-independent files that come with elisprs.")
;; Built exactly like Emacs's `init_callproc' (callproc.c): `decode_env_path'
;; on $PATH (splitting on `path-separator', empty elements → ".") followed by
;; `exec-directory' with its trailing slash removed (`directory-file-name').
(defvar exec-path
  (append (mapcar (lambda (d) (if (string= d "") "." d))
                  (split-string (or (getenv "PATH") "") path-separator))
          (list (directory-file-name exec-directory)))
  "List of directories to search programs to run in subprocesses.")
;; elisprs models no remote/TRAMP files, so every file name is local: this
;; returns nil for all inputs (Emacs returns nil for local names too), which
;; makes `executable-find's remote branch dead code (as intended here).
(defun file-remote-p (file &optional _identification _connected)
  "Test whether FILE specifies a location on a remote system.
elisprs has no remote-file support, so this is always nil."
  (ignore file) nil)
(defun locate-file-internal (filename path &optional suffixes predicate)
  "Faithful reimplementation of the C `openp' search backing `locate-file'.
Search each directory in PATH, trying each suffix in SUFFIXES (defaulting to
`(\"\")'), for FILENAME.  Return the first absolute candidate satisfying
PREDICATE, else nil.  PREDICATE nil means `file-readable-p'; an integer is a
POSIX access(2) mode bitmask (1=X_OK 2=W_OK 4=R_OK, 0=existence); a function is
called on the candidate.  Directories are skipped unless PREDICATE is a function
that returns the symbol `dir-ok'.  An absolute or `~'-prefixed FILENAME is tried
once regardless of PATH."
  (let ((suffixes (or suffixes '("")))
        (absolute (and (> (length filename) 0) (memq (aref filename 0) '(?/ ?~)))))
    (catch 'found
      (dolist (dir (if absolute '(nil) path))
        (dolist (suffix suffixes)
          (let* ((candidate (expand-file-name (concat filename suffix) dir))
                 (isdir (file-directory-p candidate)))
            (cond
             ((integerp predicate)
              (when (and (not isdir) (locate-file--access-ok candidate predicate))
                (throw 'found candidate)))
             ((functionp predicate)
              (let ((res (funcall predicate candidate)))
                (when (and res (or (not isdir) (eq res 'dir-ok)))
                  (throw 'found candidate))))
             (t
              (when (and (not isdir) (file-readable-p candidate))
                (throw 'found candidate)))))))
      nil)))
(defun locate-file--access-ok (file mode)
  "Non-nil if FILE passes the POSIX access(2) MODE bitmask (see `locate-file-internal')."
  (and (or (zerop (logand mode 1)) (file-executable-p file))
       (or (zerop (logand mode 2)) (file-writable-p file))
       (or (zerop (logand mode 4)) (file-readable-p file))
       (file-exists-p file)))
(defun locate-file (filename path &optional suffixes predicate)
  "Search for FILENAME through PATH.
If found, return the absolute file name of FILENAME; otherwise return nil.
PATH should be a list of directories to look in, like the lists in `exec-path'
or `load-path'.  If SUFFIXES is non-nil, it should be a list of suffixes to
append to file name when searching.  If SUFFIXES is nil, it is equivalent to
`(\"\")'.  If non-nil, PREDICATE is used instead of `file-readable-p'.
PREDICATE can also be an integer access(2) mode, or one of the symbols
`executable', `readable', `writable', `exists', or a list of them."
  (if (and predicate (symbolp predicate) (not (functionp predicate)))
      (setq predicate (list predicate)))
  (when (and (consp predicate) (not (functionp predicate)))
    (setq predicate
          (logior (if (memq 'executable predicate) 1 0)
                  (if (memq 'writable predicate) 2 0)
                  (if (memq 'readable predicate) 4 0))))
  (locate-file-internal filename path suffixes predicate))
(defun executable-find (command &optional remote)
  "Search for COMMAND in `exec-path' and return the absolute file name.
Return nil if COMMAND is not found anywhere in `exec-path'.
If REMOTE is non-nil, search on a remote host if `default-directory' is
remote, otherwise search locally."
  (if (and remote (file-remote-p default-directory))
      (let ((res (locate-file
                  command
                  (mapcar
                   (lambda (x) (concat (file-remote-p default-directory) x))
                   (exec-path))
                  exec-suffixes 'file-executable-p)))
        (when (stringp res) (file-local-name res)))
    ;; Use 1 rather than file-executable-p to better match the
    ;; behavior of call-process.
    (let ((default-directory (file-name-quote default-directory 'top)))
      (locate-file command exec-path exec-suffixes 1))))

;; save-current-buffer: restore the current buffer after BODY (if it is still live).
(defmacro save-current-buffer (&rest body)
  `(let ((--scb-- (current-buffer)))
     (unwind-protect (progn ,@body)
       (when (buffer-live-p --scb--) (set-buffer --scb--)))))
;; with-current-buffer: evaluate BODY with BUFFER-OR-NAME current, restoring after.
(defmacro with-current-buffer (buffer-or-name &rest body)
  `(save-current-buffer (set-buffer ,buffer-or-name) ,@body))
;; with-temp-buffer: run BODY in a fresh temporary buffer, killing it afterward.
;; Returns BODY's value, not the buffer text.
(defmacro with-temp-buffer (&rest body)
  `(let ((--tb-- (generate-new-buffer " *temp*")))
     (unwind-protect
         (with-current-buffer --tb-- ,@body)
       (kill-buffer --tb--))))
;; save-excursion: restore the current buffer AND point after BODY. Point is saved
;; as a marker-like position that tracks insertions/deletions during BODY.
(defmacro save-excursion (&rest body)
  `(let ((--se-b-- (current-buffer)))
     (--se-push--)
     (unwind-protect (progn ,@body)
       (when (buffer-live-p --se-b--) (set-buffer --se-b--))
       (--se-pop--))))
;; save-restriction: restore the buffer's narrowing after BODY (the saved bounds
;; track edits made in BODY, like Emacs markers).
(defmacro save-restriction (&rest body)
  `(progn (--save-restriction--)
     (unwind-protect (progn ,@body) (--restore-restriction--))))
;; Region case conversion: rewrite [BEG,END) through FN (length-preserving, so
;; save-excursion's integer restore stays accurate).
(defun buffer--map-region (beg end fn)
  (let ((lo (min beg end)) (hi (max beg end)))
    (save-excursion
      (let ((s (buffer-substring lo hi)))
        (delete-region lo hi)
        (goto-char lo)
        (insert (funcall fn s))))))
(defun upcase-region (beg end &optional _region) (buffer--map-region beg end #'upcase) nil)
(defun downcase-region (beg end &optional _region) (buffer--map-region beg end #'downcase) nil)
(defun capitalize-region (beg end &optional _region) (buffer--map-region beg end #'capitalize) nil)
(defun subst-char-in-region (start end fromchar tochar &optional _noundo)
  (buffer--map-region start end
    (lambda (s)
      (mapconcat (lambda (c) (char-to-string (if (eq c fromchar) tochar c))) (append s nil) "")))
  nil)
;; Case-convert ARG words forward from point, leaving point after them.
(defun buffer--case-word (arg fn)
  (let ((beg (point)))
    (forward-word arg)
    (let* ((end (point)) (lo (min beg end)) (hi (max beg end)) (s (buffer-substring lo hi)))
      (delete-region lo hi)
      (goto-char lo)
      (insert (funcall fn s))
      (goto-char (+ lo (length s))))))
(defun upcase-word (arg) (buffer--case-word arg #'upcase) nil)
(defun downcase-word (arg) (buffer--case-word arg #'downcase) nil)
(defun capitalize-word (arg) (buffer--case-word arg #'capitalize) nil)
;; kill-line: delete to end of line; at end-of-line delete the newline instead.
;; (No kill ring in elisprs, so this only deletes — it does not save the text.)
(defun kill-line (&optional _arg)
  (let ((beg (point))
        (end (if (and (eolp) (not (eobp))) (1+ (point)) (line-end-position))))
    (delete-region beg end)
    nil))
;; transpose-chars: swap the characters around point and advance; at end of line
;; swap the two preceding characters (the common Emacs behavior).
(defun transpose-chars (_arg)
  (when (and (eolp) (> (point) (1+ (point-min)))) (forward-char -1))
  (when (and (> (point) (point-min)) (< (point) (point-max)))
    (let ((a (char-before)) (b (char-after)))
      (delete-region (1- (point)) (1+ (point)))
      (insert b a)))
  nil)
;; buffer-hash: a SHA-1 of the buffer contents (BUFFER arg ignored — current only).
(defun buffer-hash (&optional _buffer) (sha1 (buffer-string)))
;; Character at/before point (0 at the buffer's end/start), like Emacs.
(defun following-char () (or (char-after) 0))
(defun preceding-char () (or (char-before) 0))
;; Insert N newlines at point (default 1); like insert, no kill ring.
(defun newline (&optional n _interactive) (insert (make-string (or n 1) ?\n)) nil)
;; Insert N newlines after point, leaving point before them.
(defun open-line (n) (save-excursion (insert (make-string n ?\n))) nil)
;; insert-before-markers: like insert, but markers exactly at point advance past
;; the new text (handled by the primitive --insert-before-markers--).
(defun insert-before-markers (&rest args)
  (dolist (a args) (--insert-before-markers-- a))
  nil)
(defun count-words (start end)
  (length (split-string (buffer-substring start end) "[^[:alnum:]]+" t)))
(defun how-many (regexp &optional start end)
  ;; Count non-overlapping matches of REGEXP from START (or point) to END.
  (save-excursion
    (when start (goto-char start))
    (let ((count 0) (limit (or end (point-max))))
      (while (re-search-forward regexp limit t)
        (when (= (match-beginning 0) (match-end 0))
          (if (>= (point) limit) (goto-char (1+ limit)) (forward-char 1)))
        (setq count (1+ count)))
      count)))
;;; ---- misc small subr.el helpers ----
(defun ngettext (singular plural n) (if (= n 1) singular plural))

;;; ---- version comparison (faithful port of lisp/subr.el, Emacs 30.2) ----
;; version-to-list parses a version string into a list of integers; the
;; version-list-* comparators do the significant-trailing-zero comparison; and
;; version< / version<= / version= wrap them over two version strings. The
;; version-regexp-alist data is copied value-for-value from subr.el so that
;; non-numeric qualifiers (snapshot/alpha/beta/pre/rc) sort exactly as Emacs.
(defconst version-separator "."
  "Specify the string used to separate the version elements.")

(defconst version-regexp-alist
  '(("^[-._+ ]?snapshot$"                                 . -4)
    ("^[-._+]$"                                           . -4)
    ("^[-._+ ]?\\(cvs\\|git\\|bzr\\|svn\\|hg\\|darcs\\)$" . -4)
    ("^[-._+ ]?unknown$"                                  . -4)
    ("^[-._+ ]?alpha$"                                    . -3)
    ("^[-._+ ]?beta$"                                     . -2)
    ("^[-._+ ]?\\(pre\\|rc\\)$"                           . -1))
  "Specify association between non-numeric version and its priority.")

(defun version-to-list (ver)
  "Convert version string VER into a list of integers."
  (declare (side-effect-free t))
  (unless (stringp ver)
    (error "Version must be a string"))
  ;; Change .x.y to 0.x.y
  (if (and (>= (length ver) (length version-separator))
	   (string-equal (substring ver 0 (length version-separator))
			 version-separator))
      (setq ver (concat "0" ver)))
  (unless (string-match-p "^[0-9]" ver)
    (error "Invalid version syntax: `%s' (must start with a number)" ver))
  (save-match-data
    (let ((i 0)
	  (case-fold-search t)		; ignore case in matching
	  lst s al)
      ;; Parse the version-string up to a separator until there are none left
      (while (and (setq s (string-match "[0-9]+" ver i))
		  (= s i))
        ;; Add the numeric part to the beginning of the version list;
        ;; lst gets reversed at the end
	(setq lst (cons (string-to-number (substring ver i (match-end 0)))
			lst)
	      i   (match-end 0))
	;; handle non-numeric part
	(when (and (setq s (string-match "[^0-9]+" ver i))
		   (= s i))
	  (setq s (substring ver i (match-end 0))
		i (match-end 0))
	  ;; handle alpha, beta, pre, etc. separator
	  (unless (string= s version-separator)
	    (setq al version-regexp-alist)
	    (while (and al (not (string-match (caar al) s)))
	      (setq al (cdr al)))
	    (cond (al
		   (push (cdar al) lst))
        ;; Convert 22.3a to 22.3.1, 22.3b to 22.3.2, etc., but only if
        ;; the letter is the end of the version-string, to avoid
        ;; 22.8X3 being valid
        ((and (string-match "^[-._+ ]?\\([a-zA-Z]\\)$" s)
           (= i (length ver)))
		   (push (- (aref (downcase (match-string 1 s)) 0) ?a -1)
			 lst))
		  (t (error "Invalid version syntax: `%s'" ver))))))
    (nreverse lst))))

(defun version-list-< (l1 l2)
  "Return t if L1, a list specification of a version, is lower than L2."
  (declare (pure t) (side-effect-free t))
  (while (and l1 l2 (= (car l1) (car l2)))
    (setq l1 (cdr l1)
	  l2 (cdr l2)))
  (cond
   ((and l1 l2) (< (car l1) (car l2)))
   ((and (null l1) (null l2)) nil)
   (l1 (< (version-list-not-zero l1) 0))
   (t  (< 0 (version-list-not-zero l2)))))

(defun version-list-= (l1 l2)
  "Return t if L1, a list specification of a version, is equal to L2."
  (declare (pure t) (side-effect-free t))
  (while (and l1 l2 (= (car l1) (car l2)))
    (setq l1 (cdr l1)
	  l2 (cdr l2)))
  (cond
   ((and l1 l2) nil)
   ((and (null l1) (null l2)))
   (l1 (zerop (version-list-not-zero l1)))
   (t  (zerop (version-list-not-zero l2)))))

(defun version-list-<= (l1 l2)
  "Return t if L1, a list specification of a version, is lower or equal to L2."
  (declare (pure t) (side-effect-free t))
  (while (and l1 l2 (= (car l1) (car l2)))
    (setq l1 (cdr l1)
	  l2 (cdr l2)))
  (cond
   ((and l1 l2) (< (car l1) (car l2)))
   ((and (null l1) (null l2)))
   (l1 (<= (version-list-not-zero l1) 0))
   (t  (<= 0 (version-list-not-zero l2)))))

(defun version-list-not-zero (lst)
  "Return the first non-zero element of LST, which is a list of integers.
If all LST elements are zeros or LST is nil, return zero."
  (declare (pure t) (side-effect-free t))
  (while (and lst (zerop (car lst)))
    (setq lst (cdr lst)))
  (if lst
      (car lst)
    0))

(defun version< (v1 v2)
  "Return t if version V1 is lower (older) than V2."
  (declare (side-effect-free t))
  (version-list-< (version-to-list v1) (version-to-list v2)))

(defun version<= (v1 v2)
  "Return t if version V1 is lower (older) than or equal to V2."
  (declare (side-effect-free t))
  (version-list-<= (version-to-list v1) (version-to-list v2)))

(defun version= (v1 v2)
  "Return t if version V1 is equal to V2."
  (declare (side-effect-free t))
  (version-list-= (version-to-list v1) (version-to-list v2)))
;; format-seconds: ported from time-date.el. %y/%d/%h/%m/%s units (upper-case adds
;; ---- time arithmetic (seconds-based) ----
;; elisprs represents time values as plain seconds (integer or float) — a valid
;; Emacs time value. (Emacs's high-precision ticks/list forms for fractional
;; results aren't reproduced, but float-time/time-less-p etc. interoperate.)
(defun time-to-seconds (time) (float-time time))
(defun time--norm (s) (if (= s (truncate s)) (truncate s) s))
(defun time-add (a b) (time--norm (+ (float-time a) (float-time b))))
(defun time-subtract (a b) (time--norm (- (float-time a) (float-time b))))
(defun time-less-p (a b) (< (float-time a) (float-time b)))
(defun time-equal-p (a b) (= (float-time a) (float-time b)))
(defun time-convert (time &optional form)
  ;; FORM t asks for the highest-resolution (TICKS . HZ) pair.  Ported from
  ;; timefns.c `time_convert' / `decode_float_time': an integer stays (N . 1);
  ;; a float f is decomposed exactly as emacs does — scale = DBL_MANT_DIG-1-ilogb(f)
  ;; (ilogb(f) = (cdr (frexp f)) - 1, so scale = 53 - (cdr (frexp f))), giving
  ;; ticks = f*2^scale (an exact integer) over hz = 2^scale.  Zero maps to (0 . 1).
  ;; A numeric FORM is an explicit HZ.
  (cond ((eq form 'integer) (truncate (float-time time)))
        ((eq form 'list) (let ((s (truncate (float-time time))))
                           (list (ash s -16) (logand s #xffff))))
        ((eq form t)
         (if (integerp time)
             (cons time 1)
           (let ((f (float-time time)))
             (if (= f 0.0)
                 (cons 0 1)
               (let* ((scale (- 53 (cdr (frexp f))))
                      (hz (expt 2 scale)))
                 (cons (round (* f hz)) hz))))))
        ((integerp form)
         (cons (round (* (float-time time) form)) form))
        (t (time--norm (float-time time)))))
(defun current-time-zone (&optional time zone)
  ;; (OFFSET NAME). The local zone's offset comes from decode-time's zone field;
  ;; the abbreviated NAME is unavailable here, so it is nil.
  (cond ((eq zone t) (list 0 "UTC"))
        ((integerp zone) (list zone nil))
        (t (list (or (nth 8 (decode-time (or time (current-time)))) 0) nil))))
;; decoded-time accessors: (sec min hour day month year weekday dst zone).
(defun decoded-time-second (dt) (nth 0 dt))
(defun decoded-time-minute (dt) (nth 1 dt))
(defun decoded-time-hour (dt) (nth 2 dt))
(defun decoded-time-day (dt) (nth 3 dt))
(defun decoded-time-month (dt) (nth 4 dt))
(defun decoded-time-year (dt) (nth 5 dt))
(defun decoded-time-weekday (dt) (nth 6 dt))
(defun decoded-time-dst (dt) (nth 7 dt))
(defun decoded-time-zone (dt) (nth 8 dt))
;; ---- format-seconds ----
;; the unit name), %z chops leading zero units, %x chops trailing zero units,
;; %,Ns gives N decimals on seconds, width/zero-pad via %N / %.N.
(defun format-seconds (string seconds)
  "Use format control STRING to format the number SECONDS."
  (let ((start 0)
        (units '(("y" "year" 31536000) ("d" "day" 86400) ("h" "hour" 3600)
                 ("m" "minute" 60) ("s" "second" 1) ("z") ("x")))
        (case-fold-search t)
        spec match usedunits zeroflag larger prev name unit num
        leading-zeropos trailing-zeropos fraction chop-leading chop-trailing)
    (while (string-match "%\\.?[0-9]*\\(,[0-9]\\)?\\(.\\)" string start)
      (setq start (match-end 0) spec (match-string 2 string))
      (unless (string-equal spec "%")
        (or (setq match (assoc (downcase spec) units))
            (error "Bad format specifier: `%s'" spec))
        (if (assoc (downcase spec) usedunits)
            (error "Multiple instances of specifier: `%s'" spec))
        (if (or (string-equal (car match) "z") (string-equal (car match) "x"))
            (setq zeroflag t)
          (unless larger
            (setq unit (nth 2 match) larger (and prev (> unit prev)) prev unit)))
        (push match usedunits)))
    (when (and zeroflag larger) (error "Units are not in decreasing order of size"))
    (unless (numberp seconds) (setq seconds (float-time seconds)))
    (setq fraction (mod seconds 1) seconds (round seconds))
    (dolist (u units)
      (setq spec (car u) name (cadr u) unit (nth 2 u))
      (when (string-match
             (format "%%\\(\\.?[0-9]+\\)?\\(,[0-9]+\\)?\\(%s\\)" spec) string)
        (cond
         ((string-equal spec "z")
          (setq chop-leading (if leading-zeropos
                                 (min leading-zeropos (match-beginning 0))
                               (+ 2 (match-beginning 0)))))
         ((string-equal spec "x") (setq chop-trailing t))
         (t
          (setq num (floor seconds unit) seconds (- seconds (* num unit)))
          (let ((is-zero (zerop (if (= unit 1) (+ num fraction) num))))
            (when (and (not leading-zeropos) (not is-zero))
              (setq leading-zeropos (match-beginning 0)))
            (unless is-zero (setq trailing-zeropos nil))
            (when (and (not trailing-zeropos) is-zero)
              (setq trailing-zeropos (match-beginning 0))))
          (setq string
                (replace-match
                 (format (if (match-string 2 string)
                             (concat "%"
                                     (and (match-string 1 string)
                                          (if (= (elt (match-string 1 string) 0) ?.)
                                              (concat "0" (substring (match-string 1 string) 1))
                                            (match-string 1 string)))
                                     (concat "." (substring (match-string 2 string) 1)) "f%s")
                           (concat "%" (match-string 1 string) "d%s"))
                         (if (= unit 1) (+ num fraction) num)
                         (if (string-equal (match-string 3 string) spec)
                             ""
                           (format " %s%s" name (if (= num 1) "" "s"))))
                 t t string))))))
    (let ((pre string))
      (when (and chop-trailing trailing-zeropos)
        (setq string (substring string 0 trailing-zeropos)))
      (when chop-leading (setq string (substring string chop-leading)))
      (when (equal string "") (setq string pre)))
    (setq string (replace-regexp-in-string "%[zx]" "" string)))
  (string-trim (string-replace "%%" "%" string)))
(defun char-displayable-p (_char &optional _display) t)
;; Feature/module system: the libraries elisprs bundles are always "provided",
;; so (require 'cl-lib) etc. are no-ops — `require' checks `features' and never
;; loads a file. (Arbitrary .el files ARE loadable via the `load' builtin.)
(defvar features '(emacs cl-lib cl-macs cl-seq cl-extra seq subr-x map rx pcase json gv ert))
(defun provide (feature &optional _subfeatures)
  (unless (memq feature features) (setq features (cons feature features)))
  feature)
(defun featurep (feature &optional _subfeature) (and (memq feature features) t))
;; Faithful port of C `Frequire' (fns.c): if FEATURE is already provided, return
;; it; otherwise `load' its file (FILENAME, else the feature's name) with a
;; recursion guard, then verify the load actually provided FEATURE. Loading uses
;; MUST-SUFFIX when FILENAME is nil (require a `.el'/`.elc', never a bare name),
;; matching `Fload (…, Qnil, Qt, Qnil, NILP (filename) ? Qt : Qnil)'.
(defvar require--nesting-list nil
  "Features whose `require' is currently in progress (recursion guard).")
(defun require (feature &optional filename noerror)
  (if (featurep feature)
      feature
    (let ((lisp-file (if filename filename (symbol-name feature))))
      (when (member lisp-file require--nesting-list)
        (error "Recursive `require' for feature `%s'" feature))
      (let ((require--nesting-list (cons lisp-file require--nesting-list)))
        (if (load lisp-file noerror t nil (if filename nil t))
            (if (featurep feature)
                feature
              (unless noerror
                (error "Loading file %s failed to provide feature `%s'"
                       lisp-file feature)))
          nil)))))
(defmacro bound-and-true-p (var) (list 'and (list 'boundp (list 'quote var)) var))
;; `make-local-variable', `make-variable-buffer-local', `local-variable-p',
;; `local-variable-if-set-p', `kill-local-variable' and `buffer-local-value' are
;; C primitives (builtins.rs) backed by the current buffer's local-binding table.
;; Faithful to the C subr `Fset_default_toplevel_value' (data.c): it runs
;; `set_default_internal' and returns nil, NOT the value.
(defun set-default-toplevel-value (sym val) (set-default sym val) nil)
(defun defalias (symbol definition &optional _docstring) (fset symbol definition) symbol)
(defalias 'string-split 'split-string)
;; help.el: obsolete alias for `help--make-usage' (help usage helpers ported above).
(define-obsolete-function-alias 'help-make-usage #'help--make-usage "25.1")
;; with-memoization: cache BODY's value in PLACE; reuse it on later calls.
(defmacro with-memoization (place &rest body) `(or ,place (setf ,place (progn ,@body))))
;; regexp-opt: a regexp matching any of STRINGS (sorted, regexp-quoted alternation).
;; NOTE: this does NOT replicate Emacs's trie/shared-prefix optimization, so the
;; output string differs for prefix-overlapping inputs — but it matches the same
;; set of strings. Special cases (empty, single, PAREN variants) match Emacs.
(defun match-string-no-properties (n &optional string)
  ;; elisprs strings carry no text properties, so this is just match-string.
  (match-string n string))
(defun regexp-opt-depth (regexp)
  ;; Count capturing groups: bare `\\(' (not the `\\(?…' shy/numbered forms).
  (let ((count 0) (i 0) (n (length regexp)))
    (while (< i n)
      (if (and (eq (aref regexp i) ?\\) (< (1+ i) n) (eq (aref regexp (1+ i)) ?\())
          (progn
            (unless (and (< (+ i 2) n) (eq (aref regexp (+ i 2)) ??))
              (setq count (1+ count)))
            (setq i (+ i 2)))
        (setq i (1+ i))))
    count))
(defun regexp-opt (strings &optional paren)
  (let ((open (cond ((stringp paren) paren)
                    ((eq paren 'words) "\\<\\(")
                    ((eq paren 'symbols) "\\_<\\(")
                    (paren "\\(")
                    (t "\\(?:")))
        (close (cond ((eq paren 'words) "\\)\\>")
                     ((eq paren 'symbols) "\\)\\_>")
                     (t "\\)"))))
    (cond
     ((null strings) "\\(?:\\`a\\`\\)")
     ;; A single one-character string needs no group.
     ((and (null paren) (null (cdr strings)) (= (length (car strings)) 1))
      (regexp-quote (car strings)))
     (t (let ((sorted (sort (copy-sequence strings) #'string<)))
          (concat open (mapconcat #'regexp-quote sorted "\\|") close))))))

(defmacro while-let (binding &rest body)
  (let ((var (car (car binding))) (val (car (cdr (car binding)))))
    `(let ((,var ,val)) (while ,var ,@body (setq ,var ,val)))))

(defmacro cl-case (expr &rest clauses)
  `(let ((--cl-case-v-- ,expr))
     (cond ,@(mapcar
              (lambda (clause)
                (let ((key (car clause)) (body (cdr clause)))
                  (cond ((memq key '(t otherwise)) (cons t body))
                        ((listp key) (cons (list 'memq '--cl-case-v-- (list 'quote key)) body))
                        (t (cons (list 'eql '--cl-case-v-- (list 'quote key)) body)))))
              clauses))))
;; ---- cl-loop (common subset) ----
;; Supported: `for V from A to/below/downto/above B [by S]`, `for V in LIST`,
;; `for V on LIST`, `repeat N`, `while`/`until COND`; accumulation `collect`,
;; `append`, `nconc`, `sum`, `count`, `maximize`, `minimize`; side effects `do
;; FORMS`; and `finally [return EXPR | do FORMS]` / `return EXPR`. Not supported
;; yet: parallel `for` clauses, `across`, `with`, `into`, `when`/`unless`/`if`
;; conditionals, destructuring.
(defun cl-loop--kw (x) (and (symbolp x) (symbol-name x)))
(defun cl-loop--clause-p (x)
  (member (cl-loop--kw x)
          '("for" "as" "repeat" "while" "until" "with" "collect" "collecting"
            "append" "appending" "nconc" "nconcing" "sum" "summing" "count"
            "counting" "maximize" "maximizing" "minimize" "minimizing" "do"
            "doing" "finally" "return" "when" "unless" "if" "else" "end"
            "always" "never" "thereis" "and" "into")))
;; True when X names a clause `cl-loop--accum' can parse (accumulators + do/return).
(defun cl-loop--accum-kw-p (x)
  (member (cl-loop--kw x)
          '("collect" "collecting" "append" "appending" "nconc" "nconcing"
            "sum" "summing" "count" "counting" "maximize" "maximizing"
            "minimize" "minimizing" "vconcat" "vconcating" "concat" "concating"
            "do" "doing" "return")))
;; Parse ONE accumulation or `do' clause at C. Return (FORM REST KIND VAR INIT):
;; FORM targets VAR (or `--clacc--' when VAR is nil), KIND is the accumulator
;; kind (nil for `do'), INIT its initial value.
(defun cl-loop--accum (c)
  (let* ((kw (cl-loop--kw (car c))) (expr (nth 1 c)) (rr (nthcdr 2 c))
         (var nil) (kind nil) (init nil) (form nil))
    (cond
     ((member kw '("collect" "collecting" "append" "appending" "nconc" "nconcing"))
      (when (equal (cl-loop--kw (car rr)) "into") (setq var (nth 1 rr) rr (nthcdr 2 rr)))
      (setq kind 'list)
      (let ((tgt (or var '--clacc--)))
        (setq form (cond ((member kw '("collect" "collecting")) (list 'setq tgt (list 'nconc tgt (list 'list expr))))
                         ((member kw '("append" "appending")) (list 'setq tgt (list 'append tgt expr)))
                         (t (list 'setq tgt (list 'nconc tgt expr)))))))
     ((member kw '("sum" "summing" "count" "counting"))
      (when (equal (cl-loop--kw (car rr)) "into") (setq var (nth 1 rr) rr (nthcdr 2 rr)))
      (setq kind 'num init 0)
      (let ((tgt (or var '--clacc--)) (d (if (member kw '("count" "counting")) (list 'if expr 1 0) expr)))
        (setq form (list 'setq tgt (list '+ tgt d)))))
     ((member kw '("maximize" "maximizing" "minimize" "minimizing"))
      (when (equal (cl-loop--kw (car rr)) "into") (setq var (nth 1 rr) rr (nthcdr 2 rr)))
      (setq kind 'ext)
      (let ((tgt (or var '--clacc--)) (fn (if (member kw '("maximize" "maximizing")) 'max 'min)))
        (setq form (list 'setq tgt (list 'if tgt (list fn tgt expr) expr)))))
     ((member kw '("vconcat" "vconcating" "concat" "concating"))
      (when (equal (cl-loop--kw (car rr)) "into") (setq var (nth 1 rr) rr (nthcdr 2 rr)))
      (let ((tgt (or var '--clacc--)))
        (if (member kw '("concat" "concating"))
            (setq kind 'str init "" form (list 'setq tgt (list 'concat tgt expr)))
          (setq kind 'vec init [] form (list 'setq tgt (list 'vconcat tgt expr))))))
     ((member kw '("do" "doing"))
      (let ((forms nil) (r (cdr c)))
        (while (and r (not (cl-loop--clause-p (car r)))) (setq forms (cons (car r) forms) r (cdr r)))
        (setq form (cons 'progn (reverse forms)) rr r)))
     ((equal kw "return")
      (setq form (list 'throw (list 'quote '--cl-loop--) expr) rr (nthcdr 2 c)))
     (t (error "cl-loop: expected an accumulation clause, got %S" (car c))))
    (list form rr kind var init)))
(defmacro cl-loop (&rest clauses)
  (let ((binds nil) (test t) (pre nil) (steps nil) (body nil)
        (acc-kind nil) (bool-result nil) (finally nil) (initial nil) (loop-name nil) (c clauses))
    (while c
      (let ((kw (cl-loop--kw (car c))))
        (cond
         ;; named NAME — wrap the loop in `(cl-block NAME …)` so `cl-return-from
         ;; NAME` can exit it (Emacs names the implicit block with this name).
         ((equal kw "named") (setq loop-name (nth 1 c) c (nthcdr 2 c)))
         ;; `and' joins the next clause into the SAME iteration step. When it
         ;; joins an accumulation / conditional / do clause, drop `and' and let
         ;; that clause parse normally (its body already runs each pass). When it
         ;; joins a binding clause the leading `for' is omitted, so re-insert it.
         ((equal kw "and")
          (if (member (cl-loop--kw (nth 1 c))
                      '("collect" "collecting" "append" "appending" "nconc" "nconcing"
                        "sum" "summing" "count" "counting" "maximize" "maximizing"
                        "minimize" "minimizing" "vconcat" "vconcating" "concat" "concating"
                        "do" "doing" "when" "unless" "if" "return"))
              (setq c (cdr c))
            (setq c (cons 'for (cdr c)))))
         ;; for V across SEQ — iterate the elements of a string/vector/list.
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "across"))
          (let ((var (nth 1 c)) (tv (make-symbol "tail")))
            (setq binds (cons (list tv (list 'append (nth 3 c) nil)) (cons (list var nil) binds)))
            (setq test (if (eq test t) tv (list 'and test tv)))
            (setq pre (cons (list 'setq var (list 'car tv)) pre))
            (setq steps (cons (list 'setq tv (list 'cdr tv)) steps))
            (setq c (nthcdr 4 c))))
         ;; for V being [the|each] KIND of SOURCE  (elements / hash-keys / hash-values)
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "being"))
          (let ((var (nth 1 c)) (tv (make-symbol "tail")) (ht (make-symbol "ht")) (r (nthcdr 3 c)))
            (when (member (cl-loop--kw (car r)) '("the" "each")) (setq r (cdr r)))
            (let ((kind (cl-loop--kw (car r))))
              (setq r (cdr r))
              (when (member (cl-loop--kw (car r)) '("of" "in")) (setq r (cdr r)))
              (let* ((source (car r))
                     (hashp (member kind '("hash-keys" "hash-key" "hash-values" "hash-value")))
                     (keysp (member kind '("hash-keys" "hash-key")))
                     (listform (cond (keysp (list 'hash-table-keys ht))
                                     ((member kind '("hash-values" "hash-value")) (list 'hash-table-values ht))
                                     (t (list 'append source nil))))
                     (usevar nil) (usekind nil))
                (setq r (cdr r))
                ;; using (hash-values V) / (hash-keys V): bind the companion var.
                (when (equal (cl-loop--kw (car r)) "using")
                  (let ((u (nth 1 r))) (setq usekind (cl-loop--kw (car u)) usevar (nth 1 u)))
                  (setq r (nthcdr 2 r)))
                (when hashp (setq binds (cons (list ht source) binds)))
                (setq binds (cons (list tv listform) (cons (list var nil) binds)))
                (when usevar
                  (setq binds (cons (list usevar (if (equal usekind "index") 0 nil)) binds)))
                (setq test (if (eq test t) tv (list 'and test tv)))
                (setq pre (cons (list 'setq var (list 'car tv)) pre))
                ;; companion: value for a key-iteration (the common form).
                (when (and usevar keysp (member usekind '("hash-values" "hash-value")))
                  (setq pre (cons (list 'setq usevar (list 'gethash var ht)) pre)))
                ;; using (index V): V counts iterations from 0.
                (when (and usevar (equal usekind "index"))
                  (setq steps (cons (list 'setq usevar (list '1+ usevar)) steps)))
                (setq steps (cons (list 'setq tv (list 'cdr tv)) steps))
                (setq c r)))))
         ;; for V = INIT [then STEP] — bind to INIT, step at end of each iteration
         ;; (so V is current when later until/while tests run); no `then' re-evaluates
         ;; INIT each pass.
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "="))
          (let ((var (nth 1 c)) (initv (nth 3 c)) (r (nthcdr 4 c)) (stepv nil))
            (if (equal (cl-loop--kw (car r)) "then")
                (setq stepv (nth 1 r) r (nthcdr 2 r))
              (setq stepv initv))
            (setq binds (cons (list var initv) binds))
            (setq steps (cons (list 'setq var stepv) steps))
            (setq c r)))
         ;; for V [from A] [to/below/downto/above B] [by S]  (from defaults to 0)
         ((and (member kw '("for" "as"))
               (member (cl-loop--kw (nth 2 c))
                       '("from" "upfrom" "downfrom" "to" "upto" "below" "downto" "above")))
          (let* ((var (nth 1 c)) (has-from (member (cl-loop--kw (nth 2 c)) '("from" "upfrom" "downfrom")))
                 (sub (if has-from (cl-loop--kw (nth 2 c)) "from"))
                 (start (if has-from (nth 3 c) 0)) (r (if has-from (nthcdr 4 c) (nthcdr 2 c)))
                 (limk nil) (lim nil) (step 1) (down nil))
            (while (member (cl-loop--kw (car r))
                           '("to" "upto" "below" "downto" "above" "by"))
              (if (equal (cl-loop--kw (car r)) "by")
                  (setq step (nth 1 r) r (nthcdr 2 r))
                (setq limk (cl-loop--kw (car r)) lim (nth 1 r) r (nthcdr 2 r))))
            ;; Count down for `downfrom', or when the limit is `downto'/`above'.
            (setq down (or (equal sub "downfrom") (member limk '("downto" "above"))))
            (setq binds (cons (list var start) binds))
            (when limk
              (let ((cnd (cond ((member limk '("to" "upto")) (list '<= var lim))
                               ((equal limk "below") (list '< var lim))
                               ((equal limk "downto") (list '>= var lim))
                               ((equal limk "above") (list '> var lim)))))
                (setq test (if (eq test t) cnd (list 'and test cnd)))))
            (setq steps (cons (list 'setq var (list (if down '- '+) var step)) steps))
            (setq c r)))
         ;; for (A B ...) in LIST  — destructure each element
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "in")
               (consp (nth 1 c)))
          (let* ((pat (nth 1 c)) (tv (make-symbol "tail")) (ev (make-symbol "el"))
                 (dbs (cl-db--binds pat ev)) (setqs nil) (r (nthcdr 4 c)) (stepfn nil))
            (when (equal (cl-loop--kw (car r)) "by")
              (setq stepfn (nth 1 r) r (nthcdr 2 r)))
            (setq binds (cons (list tv (nth 3 c)) (cons (list ev nil) binds)))
            (dolist (b dbs) (setq binds (cons (list (car b) nil) binds)))
            (setq test (if (eq test t) tv (list 'and test tv)))
            (setq setqs (cons (list 'setq ev (list 'car tv)) nil))
            (dolist (b dbs) (setq setqs (cons (list 'setq (car b) (car (cdr b))) setqs)))
            (setq pre (cons (cons 'progn (reverse setqs)) pre))
            (setq steps (cons (list 'setq tv (if stepfn (list 'funcall stepfn tv) (list 'cdr tv))) steps))
            (setq c r)))
         ;; for V in LIST [by STEP-FN]
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "in"))
          (let ((var (nth 1 c)) (tv (make-symbol "tail")) (r (nthcdr 4 c)) (stepfn nil))
            (when (equal (cl-loop--kw (car r)) "by")
              (setq stepfn (nth 1 r) r (nthcdr 2 r)))
            (setq binds (cons (list tv (nth 3 c)) (cons (list var nil) binds)))
            (setq test (if (eq test t) tv (list 'and test tv)))
            (setq pre (cons (list 'setq var (list 'car tv)) pre))
            (setq steps (cons (list 'setq tv (if stepfn (list 'funcall stepfn tv) (list 'cdr tv))) steps))
            (setq c r)))
         ;; for V on LIST
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "on"))
          (let ((pat (nth 1 c)) (r (nthcdr 4 c)) (stepfn nil))
            (when (equal (cl-loop--kw (car r)) "by")
              (setq stepfn (nth 1 r) r (nthcdr 2 r)))
            (if (consp pat)
                ;; Destructure the current tail's leading elements each iteration.
                (let* ((tv (make-symbol "tail")) (dbs (cl-db--binds pat tv)) (setqs nil))
                  (setq binds (cons (list tv (nth 3 c)) binds))
                  (dolist (b dbs) (setq binds (cons (list (car b) nil) binds)))
                  (setq test (if (eq test t) tv (list 'and test tv)))
                  (dolist (b dbs) (setq setqs (cons (list 'setq (car b) (car (cdr b))) setqs)))
                  (setq pre (cons (cons 'progn (reverse setqs)) pre))
                  (setq steps (cons (list 'setq tv (if stepfn (list 'funcall stepfn tv) (list 'cdr tv))) steps)))
              (setq binds (cons (list pat (nth 3 c)) binds))
              (setq test (if (eq test t) pat (list 'and test pat)))
              (setq steps (cons (list 'setq pat (if stepfn (list 'funcall stepfn pat) (list 'cdr pat))) steps)))
            (setq c r)))
         ;; repeat N
         ((equal kw "repeat")
          (let ((rv (make-symbol "n")))
            (setq binds (cons (list rv (nth 1 c)) binds))
            (let ((cnd (list '> rv 0)))
              (setq test (if (eq test t) cnd (list 'and test cnd))))
            (setq steps (cons (list 'setq rv (list '1- rv)) steps))
            (setq c (nthcdr 2 c))))
         ((equal kw "while")
          (setq test (if (eq test t) (nth 1 c) (list 'and test (nth 1 c))))
          (setq c (nthcdr 2 c)))
         ((equal kw "until")
          (let ((cnd (list 'not (nth 1 c))))
            (setq test (if (eq test t) cnd (list 'and test cnd))))
          (setq c (nthcdr 2 c)))
         ;; with VAR = VAL [and VAR2 = VAL2 ...]
         ((equal kw "with")
          (let ((r (cdr c)) (more t))
            (while more
              (let ((wv (car r)))
                (if (equal (cl-loop--kw (nth 1 r)) "=")
                    (setq binds (cons (list wv (nth 2 r)) binds) r (nthcdr 3 r))
                  (setq binds (cons (list wv nil) binds) r (cdr r)))
                (if (equal (cl-loop--kw (car r)) "and") (setq r (cdr r)) (setq more nil))))
            (setq c r)))
         ;; when/unless/if COND <accum> [and <accum>...] [else <accum> [and ...]] [end]
         ;; `and'-joined accumulators all share the branch condition (emacs 30.2:
         ;; `when C collect X and collect Y' gates BOTH collects on C).
         ((member kw '("when" "unless" "if"))
          (let* ((cnd (nth 1 c)) (r (nthcdr 2 c)) (neg (equal kw "unless"))
                 (a (cl-loop--accum r)) (cforms (list (nth 0 a))) (aforms nil))
            (setq r (nth 1 a))
            (if (nth 3 a) (setq binds (cons (list (nth 3 a) (nth 4 a)) binds))
              (when (nth 2 a) (setq acc-kind (nth 2 a))))
            (while (and (equal (cl-loop--kw (car r)) "and") (cl-loop--accum-kw-p (nth 1 r)))
              (let ((a2 (cl-loop--accum (cdr r))))
                (setq cforms (cons (nth 0 a2) cforms) r (nth 1 a2))
                (if (nth 3 a2) (setq binds (cons (list (nth 3 a2) (nth 4 a2)) binds))
                  (when (nth 2 a2) (setq acc-kind (nth 2 a2))))))
            (when (equal (cl-loop--kw (car r)) "else")
              (let ((b (cl-loop--accum (cdr r))))
                (setq aforms (list (nth 0 b)) r (nth 1 b))
                (if (nth 3 b) (setq binds (cons (list (nth 3 b) (nth 4 b)) binds))
                  (when (nth 2 b) (setq acc-kind (nth 2 b))))
                (while (and (equal (cl-loop--kw (car r)) "and") (cl-loop--accum-kw-p (nth 1 r)))
                  (let ((b2 (cl-loop--accum (cdr r))))
                    (setq aforms (cons (nth 0 b2) aforms) r (nth 1 b2))
                    (if (nth 3 b2) (setq binds (cons (list (nth 3 b2) (nth 4 b2)) binds))
                      (when (nth 2 b2) (setq acc-kind (nth 2 b2))))))))
            (when (equal (cl-loop--kw (car r)) "end") (setq r (cdr r)))
            (let ((cform (if (cdr cforms) (cons 'progn (reverse cforms)) (car cforms)))
                  (aform (cond ((null aforms) nil)
                               ((cdr aforms) (cons 'progn (reverse aforms)))
                               (t (car aforms)))))
              (setq body (cons (if neg (list 'if cnd aform cform) (list 'if cnd cform aform)) body)))
            (setq c r)))
         ;; boolean termination clauses
         ((equal kw "always")
          (setq bool-result t)
          (setq body (cons (list 'unless (nth 1 c) (list 'throw ''--cl-loop-- nil)) body))
          (setq c (nthcdr 2 c)))
         ((equal kw "never")
          (setq bool-result t)
          (setq body (cons (list 'when (nth 1 c) (list 'throw ''--cl-loop-- nil)) body))
          (setq c (nthcdr 2 c)))
         ((equal kw "thereis")
          (let ((tv (make-symbol "v")))
            (setq body (cons (list 'let (list (list tv (nth 1 c)))
                                   (list 'when tv (list 'throw ''--cl-loop-- tv))) body)))
          (setq c (nthcdr 2 c)))
         ;; direct accumulation / do
         ((member kw '("collect" "collecting" "append" "appending" "nconc" "nconcing"
                       "sum" "summing" "count" "counting" "maximize" "maximizing"
                       "minimize" "minimizing" "vconcat" "vconcating" "concat" "concating"
                       "do" "doing"))
          (let ((a (cl-loop--accum c)))
            (if (nth 3 a) (setq binds (cons (list (nth 3 a) (nth 4 a)) binds))
              (when (nth 2 a) (setq acc-kind (nth 2 a))))
            (setq body (cons (nth 0 a) body))
            (setq c (nth 1 a))))
         ((equal kw "return")
          (setq body (cons (list 'throw ''--cl-loop-- (nth 1 c)) body))
          (setq c (nthcdr 2 c)))
         ((equal kw "initially")
          (setq c (cdr c))
          (when (member (cl-loop--kw (car c)) '("do" "doing")) (setq c (cdr c)))
          (let ((fs nil))
            (while (and c (not (cl-loop--clause-p (car c)))) (setq fs (cons (car c) fs)) (setq c (cdr c)))
            (setq initial (append initial (reverse fs)))))
         ((equal kw "finally")
          (setq c (cdr c))
          (if (equal (cl-loop--kw (car c)) "return")
              (progn (setq finally (list (nth 1 c))) (setq c (nthcdr 2 c)))
            (when (member (cl-loop--kw (car c)) '("do" "doing")) (setq c (cdr c)))
            (let ((fs nil))
              (while (and c (not (cl-loop--clause-p (car c)))) (setq fs (cons (car c) fs)) (setq c (cdr c)))
              (setq finally (reverse fs)))))
         (t (error "cl-loop: unsupported clause %S" (car c))))))
    (let ((init (cond ((eq acc-kind 'num) 0) ((eq acc-kind 'str) "") ((eq acc-kind 'vec) []) (t nil)))
          (result (cond (finally (cons 'progn finally))
                        (acc-kind '--clacc--)
                        (bool-result t)
                        (t nil))))
      ;; Wrap in `(cl-block NAME …)` (NAME defaults to nil) so `cl-return`/
      ;; `cl-return-from NAME` exit the loop.
      `(cl-block ,loop-name
         (let* (,@(reverse binds) (--clacc-- ,init))
           ,@initial
           (catch '--cl-loop--
             (while ,test ,@(reverse pre) ,@(reverse body) ,@(reverse steps))
             ,result))))))

;; The predicate symbol for a `cl-typecase' type name (integer->integerp, etc.).
(defmacro cl-deftype (name arglist &rest body)
  ;; Register a type alias: NAME (with ARGLIST) expands to the type spec BODY
  ;; returns, consulted by cl-typep.
  (list 'progn
        (list 'put (list 'quote name) (list 'quote 'cl-deftype-handler)
              (cons 'lambda (cons arglist body)))
        (list 'quote name)))
(defun cl--type-bound (n lo hi)
  ;; Range check for (TYPE LO HI); bound `*'/nil/absent means unbounded.
  (and (or (null lo) (eq lo '*) (>= n lo))
       (or (null hi) (eq hi '*) (<= n hi))))
(defun cl-typep (obj type)
  ;; Simple type names plus compound specifiers: (integer LO HI), (or …),
  ;; (and …), (not T), (member …), (eql V), (satisfies PRED).
  (cond
   ((eq type t) t)
   ((eq type nil) nil)
   ((consp type)
    (let ((head (car type)) (args (cdr type)))
      (cond
       ((eq head 'or) (let ((r nil)) (dolist (tp args) (when (cl-typep obj tp) (setq r t))) r))
       ((eq head 'and) (let ((r t)) (dolist (tp args) (unless (cl-typep obj tp) (setq r nil))) r))
       ((eq head 'not) (not (cl-typep obj (car args))))
       ((eq head 'member) (and (memql obj args) t))
       ((eq head 'eql) (eql obj (car args)))
       ((eq head 'satisfies) (and (funcall (car args) obj) t))
       ((memq head '(integer fixnum bignum signed-byte unsigned-byte))
        (and (integerp obj) (cl--type-bound obj (car args) (car (cdr args)))))
       ((eq head 'float) (and (floatp obj) (cl--type-bound obj (car args) (car (cdr args)))))
       ((memq head '(number real)) (and (numberp obj) (cl--type-bound obj (car args) (car (cdr args)))))
       ;; A cl-deftype alias applied to arguments.
       ((and (symbolp head) (get head 'cl-deftype-handler))
        (cl-typep obj (apply (get head 'cl-deftype-handler) args)))
       (t nil))))
   ;; A cl-deftype alias used bare.
   ((and (symbolp type) (get type 'cl-deftype-handler))
    (cl-typep obj (funcall (get type 'cl-deftype-handler))))
   (t (funcall (cl-typecase--pred type) obj))))
(defun cl-typecase--pred (type)
  (cond ((eq type 'list) 'listp)
        ((eq type 'null) 'null)
        ((eq type 'atom) 'atom)
        ((eq type 'number) 'numberp)
        ;; Builtins use TYPEp; cl-defstruct types use the hyphenated TYPE-p.
        (t (let ((p (intern (concat (symbol-name type) "p")))
                 (sp (intern (concat (symbol-name type) "-p"))))
             (if (and (not (fboundp p)) (fboundp sp)) sp p)))))

;; ---- cl-defgeneric / cl-defmethod: single/multi type-dispatch with CLOS
;; method combination — primary plus :before/:after/:around qualifiers and
;; `cl-call-next-method'/`cl-next-method-p'. Methods are kept per generic name
;; in `cl--generic-table' as (QUALIFIER SPECS . FN); a call selects the
;; applicable methods, orders them by specificity, and runs the effective method.
(defvar cl--generic-table nil)
(defvar cl--cnm-args nil)
(defvar cl--cnm-next nil)
(define-error 'cl-no-applicable-method "No applicable method")
(define-error 'cl-no-next-method "No next method")
(defun cl--spec-match (sp arg)
  (cond ((eq sp t) t)
        ((and (consp sp) (eq (car sp) 'eql)) (eql arg (car (cdr sp))))
        ((and (consp sp) (eq (car sp) 'head)) (and (consp arg) (eq (car arg) (car (cdr sp)))))
        (t (cl-typep arg sp))))
(defun cl--type-rank (sp)
  ;; Higher = more specific, so it wins ties between applicable methods.
  (cond ((eq sp t) 0)
        ((and (consp sp) (memq (car sp) '(eql head))) 100)
        ((memq sp '(number list sequence array atom integer-or-marker)) 2)
        (t 3)))
(defun cl--add-method (name qualifier specs fn)
  (let ((entry (assq name cl--generic-table)))
    (unless entry
      (setq entry (cons name nil) cl--generic-table (cons entry cl--generic-table)))
    (setcdr entry (cons (cons qualifier (cons specs fn))
                        (cl-remove-if (lambda (m) (and (eq (car m) qualifier)
                                                       (equal (car (cdr m)) specs)))
                                      (cdr entry))))))
(defun cl--sortq (app qual desc)
  ;; The FNs in APP (each (SCORE QUALIFIER FN)) whose qualifier is QUAL, ordered
  ;; by SCORE (descending when DESC).
  (let ((sel (cl-remove-if-not (lambda (e) (eq (car (cdr e)) qual)) app)))
    (setq sel (sort sel (lambda (a b) (if desc (> (car a) (car b)) (< (car a) (car b))))))
    (mapcar (lambda (e) (car (cdr (cdr e)))) sel)))
(defun cl-call-next-method (&rest newargs)
  (let ((args (if newargs newargs cl--cnm-args)))
    (if cl--cnm-next (funcall cl--cnm-next args) (signal 'cl-no-next-method nil))))
(defun cl-next-method-p () (and cl--cnm-next t))
(defun cl--chain (methods final args)
  ;; Run METHODS[0] with `cl-call-next-method' wired to the rest, ending in FINAL
  ;; (a one-arg thunk) or signaling when exhausted.
  (if (null methods)
      (if final (funcall final args) (signal 'cl-no-next-method nil))
    (let ((cl--cnm-args args)
          (cl--cnm-next (if (or (cdr methods) final)
                            (let ((rest (cdr methods)))
                              (lambda (a) (cl--chain rest final a)))
                          nil)))
      (apply (car methods) args))))
(defun cl--generic-dispatch (name args)
  (let ((methods (cdr (assq name cl--generic-table))) (app nil))
    (dolist (m methods)
      ;; m = (QUALIFIER SPECS . FN)
      (let ((specs (car (cdr m))) (score 0) (ok t) (a args))
        (while (and specs ok)
          (let ((sp (car specs)))
            (if (cl--spec-match sp (car a)) (setq score (+ score (cl--type-rank sp)))
              (setq ok nil)))
          (setq specs (cdr specs) a (cdr a)))
        (when ok (setq app (cons (list score (car m) (cdr (cdr m))) app)))))
    (let ((arounds (cl--sortq app :around t))
          (befores (cl--sortq app :before t))
          (primaries (cl--sortq app nil t))
          (afters (cl--sortq app :after nil)))
      (if (and (null primaries) (null arounds))
          (signal 'cl-no-applicable-method (list name))
        (let ((core (lambda (a)
                      (dolist (b befores) (apply b a))
                      (let ((result (cl--chain primaries nil a)))
                        (dolist (af afters) (apply af a))
                        result))))
          (if arounds (cl--chain arounds core args) (funcall core args)))))))
(defun cl--method-spec (sp)
  ;; Build a FORM that evaluates to the runtime specializer for arglist entry SP.
  (cond ((and (consp sp) (memq (car sp) '(eql head)))
         (list 'list (list 'quote (car sp)) (car (cdr sp))))
        (t (list 'quote sp))))
(defmacro cl-defmethod (name &rest body)
  ;; (cl-defmethod NAME [QUALIFIER] ARGLIST BODY...) — QUALIFIER is an optional
  ;; :before/:after/:around keyword.
  (let ((qualifier nil) (arglist nil) (plain nil) (specs nil) (mode 'req))
    (when (keywordp (car body)) (setq qualifier (car body) body (cdr body)))
    (setq arglist (car body) body (cdr body))
    (dolist (a arglist)
      (cond
       ((memq a '(&optional &rest &key)) (setq mode a) (setq plain (cons a plain)))
       ((and (eq mode 'req) (consp a))
        (setq plain (cons (car a) plain) specs (cons (cl--method-spec (car (cdr a))) specs)))
       ((eq mode 'req) (setq plain (cons a plain) specs (cons t specs)))
       (t (setq plain (cons a plain)))))
    (setq plain (nreverse plain) specs (nreverse specs))
    (list 'progn
          (list 'cl--add-method (list 'quote name) qualifier (cons 'list specs)
                (cons 'lambda (cons plain body)))
          (list 'defun name '(&rest --args--)
                (list 'cl--generic-dispatch (list 'quote name) '--args--))
          (list 'quote name))))
(defmacro cl-defgeneric (name arglist &rest body)
  ;; Establish the dispatcher; real body forms become an unspecialized default.
  (let ((real (cl-remove-if
               (lambda (f) (or (stringp f)
                               (and (consp f) (memq (car f) '(declare :documentation)))))
               body)))
    (append
     (list 'progn
           (list 'unless (list 'assq (list 'quote name) 'cl--generic-table)
                 (list 'setq 'cl--generic-table
                       (list 'cons (list 'cons (list 'quote name) nil) 'cl--generic-table))))
     (when real
       (list (list 'cl--add-method (list 'quote name) nil nil
                   (cons 'lambda (cons arglist real)))))
     (list (list 'defun name '(&rest --args--)
                 (list 'cl--generic-dispatch (list 'quote name) '--args--))
           (list 'quote name)))))
(defmacro cl-typecase (expr &rest clauses)
  `(let ((--ct-v-- ,expr))
     (cond ,@(mapcar
              (lambda (clause)
                (let ((type (car clause)) (body (cdr clause)))
                  (if (memq type '(t otherwise))
                      (cons t body)
                    ;; Route through cl-typep so compound type specs work too.
                    (cons (list 'cl-typep '--ct-v-- (list 'quote type)) body))))
              clauses))))
(defmacro cl-the (_type form) form)
(defmacro cl-assert (form &optional _show-args string &rest args)
  ;; With a STRING, signal a plain `error' with the formatted message; otherwise
  ;; `cl-assertion-failed' carrying the failed FORM.
  (if string
      (list 'if form nil (cons 'error (cons string args)))
    (list 'if form nil (list 'signal (list 'quote 'cl-assertion-failed) (list 'list (list 'quote form))))))
(defmacro cl-check-type (form type &rest _)
  (list 'if (list 'cl-typep form (list 'quote type)) nil
        (list 'signal (list 'quote 'wrong-type-argument) (list 'list (list 'quote type) form))))
(defmacro cl-etypecase (expr &rest clauses)
  `(cl-typecase ,expr ,@clauses (t (error "cl-etypecase failed"))))
(defmacro cl-ecase (expr &rest clauses)
  `(cl-case ,expr ,@clauses (t (error "cl-ecase failed"))))
;; (cl-do ((VAR INIT [STEP])...) (END RESULT...) BODY...): like CL `do', with
;; the steps computed from the previous iteration's values (parallel assignment).
(defmacro cl-do (specs endclause &rest body)
  (let ((inits nil) (steps nil) (tmps nil))
    (dolist (s specs)
      (let ((var (if (consp s) (car s) s))
            (init (if (consp s) (car (cdr s)) nil))
            (has-step (and (consp s) (cdr (cdr s)))))
        (setq inits (cons (list var init) inits))
        (when has-step
          (let ((tv (make-symbol (symbol-name var))))
            (setq tmps (cons (list tv (car (cdr (cdr s)))) tmps))
            (setq steps (cons (list var tv) steps))))))
    (setq inits (nreverse inits) tmps (nreverse tmps) steps (nreverse steps))
    (let ((setqargs nil))
      (dolist (p steps)
        (setq setqargs (cons (car p) setqargs))
        (setq setqargs (cons (car (cdr p)) setqargs)))
      (setq setqargs (nreverse setqargs))
      `(let ,inits
         (while (not ,(car endclause))
           ,@body
           (let ,tmps (setq ,@setqargs)))
         ,@(cdr endclause)))))
;; cl-do*: like cl-do but bindings and step assignments are sequential (let* and
;; an in-order setq), so each spec sees the updated earlier ones.
(defmacro cl-do* (specs endclause &rest body)
  (let ((inits nil) (steps nil))
    (dolist (s specs)
      (let ((var (if (consp s) (car s) s))
            (init (if (consp s) (car (cdr s)) nil))
            (has-step (and (consp s) (cdr (cdr s)))))
        (setq inits (cons (list var init) inits))
        (when has-step
          (setq steps (cons (car (cdr (cdr s))) (cons var steps))))))
    (setq inits (nreverse inits) steps (nreverse steps))
    `(let* ,inits
       (while (not ,(car endclause))
         ,@body
         (setq ,@steps))
       ,@(cdr endclause))))
;; cl-progv: dynamically bind the runtime lists SYMBOLS to VALUES around BODY,
;; restoring (or unbinding) each afterwards.
(defmacro cl-progv (symbols values &rest body)
  `(let* ((--pv-syms-- ,symbols)
          (--pv-saved-- (mapcar (lambda (s) (list s (boundp s) (and (boundp s) (symbol-value s))))
                                --pv-syms--)))
     (unwind-protect
         (progn
           (let ((ss --pv-syms--) (vv ,values))
             (while ss (set (car ss) (car vv)) (setq ss (cdr ss) vv (cdr vv))))
           ,@body)
       (dolist (e --pv-saved--)
         (if (nth 1 e) (set (car e) (nth 2 e)) (makunbound (car e)))))))
;; Build let* bindings that positionally destructure VALEXPR (a symbol holding a
;; list) against a flat ARGLIST, honoring &optional and &rest.
(defun cl-db--plist-get (plist key default)
  ;; plist-get with a fallback when KEY is absent (for &key defaults).
  (let ((m (plist-member plist key))) (if m (car (cdr m)) default)))
(defun cl-db--arity (arglist)
  ;; (MIN . MAX) element counts a destructuring ARGLIST accepts; MAX is nil for
  ;; no upper bound (&rest / &body / &key or a dotted tail allow extra elements).
  ;; &aux vars consume nothing, so they neither raise MIN nor lift the bound.
  (let ((min 0) (max 0) (mode 'req) (unbounded nil))
    (when (eq (car arglist) '&whole) (setq arglist (cddr arglist)))
    (while (consp arglist)
      (let ((a (car arglist)))
        (cond
         ((eq a '&optional) (setq mode 'opt))
         ((memq a '(&rest &body &key)) (setq mode 'done unbounded t))
         ((eq a '&aux) (setq mode 'done))
         ((eq mode 'done) nil)
         ((eq mode 'opt) (setq max (1+ max)))
         (t (setq min (1+ min) max (1+ max)))))
      (setq arglist (cdr arglist)))
    (when (and arglist (symbolp arglist)) (setq unbounded t))
    (cons min (if unbounded nil max))))
(defun cl-db--check (val min max arglist)
  ;; Signal `wrong-number-of-arguments' when VAL's length is outside [MIN,MAX]
  ;; (MAX nil = unbounded), reporting ARGLIST like Emacs' cl-destructuring-bind.
  (let ((n (length val)))
    (when (or (< n min) (and max (> n max)))
      (signal 'wrong-number-of-arguments (list arglist n))))
  nil)
(defun cl-db--binds (arglist v &optional check)
  ;; Supports &whole, &optional / &rest / &key with per-arg defaults `(VAR DEFAULT)`
  ;; and nested patterns in required position. &key reads the plist tail at pos i.
  ;; When CHECK (the top-level arglist to report) is non-nil, an arity guard is
  ;; emitted so cl-destructuring-bind errors on length mismatch; cl-loop passes
  ;; no CHECK and stays lenient, matching Emacs in both cases.
  (let ((binds nil) (i 0) (mode 'req)
        (haskey nil) (allow nil) (keystart 0) (keywords nil))
    ;; Leading (&whole VAR …): bind VAR to the whole value, then continue.
    (when (eq (car arglist) '&whole)
      (setq binds (cons (list (car (cdr arglist)) v) binds))
      (setq arglist (cddr arglist)))
    (when check
      (let ((ar (cl-db--arity arglist)))
        (setq binds (cons (list (make-symbol "--cl-db-chk--")
                                (list 'cl-db--check v (car ar) (cdr ar)
                                      (list 'quote check)))
                          binds))))
    (while (consp arglist)
      (let ((a (car arglist)))
        (cond
         ((eq a '&optional) (setq mode 'opt))
         ((eq a '&rest) (setq mode 'rest))
         ((eq a '&key) (setq mode 'key haskey t keystart i))
         ((eq a '&allow-other-keys) (setq allow t))
         ((eq a '&aux) (setq mode 'aux))
         ((eq mode 'rest) (setq binds (cons (list a (list 'nthcdr i v)) binds)))
         ((eq mode 'aux)
          (let ((var (if (consp a) (car a) a)) (def (and (consp a) (car (cdr a)))))
            (setq binds (cons (list var def) binds))))
         ((eq mode 'key)
          (let* ((var (if (consp a) (car a) a))
                 (def (and (consp a) (car (cdr a))))
                 (kw (intern (concat ":" (symbol-name var)))))
            (setq keywords (cons kw keywords))
            (setq binds (cons (list var (list 'cl-db--plist-get (list 'nthcdr i v)
                                              (list 'quote kw) def))
                              binds))))
         ((eq mode 'opt)
          (let ((var (if (consp a) (car a) a)) (def (and (consp a) (car (cdr a)))))
            (setq binds (cons (list var (list 'if (list 'nthcdr i v) (list 'nth i v) def))
                              binds))
            (setq i (1+ i))))
         ((consp a)
          ;; Nested pattern: bind a temp to the element, then destructure it.
          (let ((tv (make-symbol "db")))
            (setq binds (cons (list tv (list 'nth i v)) binds))
            (dolist (rb (cl-db--binds a tv check)) (setq binds (cons rb binds))))
          (setq i (1+ i)))
         (t (setq binds (cons (list a (list 'nth i v)) binds)) (setq i (1+ i)))))
      (setq arglist (cdr arglist)))
    ;; &key keyword validation (only under CHECK, i.e. cl-destructuring-bind).
    ;; Faithful to cl-macs.el: reject any keyword not among the &key names unless
    ;; the lambda-list has &allow-other-keys or the plist carries :allow-other-keys t.
    (when (and check haskey (not allow))
      (let ((ks (make-symbol "--cl-keys--"))
            (tail (list 'nthcdr keystart v))
            (allowed (reverse keywords)))
        (setq binds
              (cons
               (list (make-symbol "--cl-db-keychk--")
                     (list 'let (list (list ks tail))
                           (list 'while ks
                                 (list 'cond
                                       (list (list 'memq (list 'car ks)
                                                   (list 'quote (append allowed '(:allow-other-keys))))
                                             (list 'if (list 'cdr ks) nil
                                                   (list 'error "Missing argument for %s" (list 'car ks)))
                                             (list 'setq ks (list 'cddr ks)))
                                       (list (list 'car (list 'cdr (list 'memq '':allow-other-keys tail)))
                                             (list 'setq ks nil))
                                       (list t (list 'error "Keyword argument %s not one of %s"
                                                     (list 'car ks) (list 'quote allowed)))))))
               binds))))
    ;; A dotted tail — e.g. (K . V) — binds the trailing symbol to the rest.
    (when (and arglist (symbolp arglist))
      (setq binds (cons (list arglist (list 'nthcdr i v)) binds)))
    (reverse binds)))
(defmacro cl-destructuring-bind (arglist expr &rest body)
  `(let ((--cl-db-v-- ,expr))
     (let* ,(cl-db--binds arglist '--cl-db-v-- arglist) ,@body)))
;; cl-defun/cl-defmacro: defun/defmacro accepting a full cl-lambda-list
;; (&optional/&key/&rest/&aux with per-arg defaults), via cl-destructuring-bind.
(defmacro cl-defun (name arglist &rest body)
  `(defun ,name (&rest --cl-args--)
     (cl-destructuring-bind ,arglist --cl-args-- ,@body)))
(defmacro cl-defmacro (name arglist &rest body)
  `(defmacro ,name (&rest --cl-args--)
     (cl-destructuring-bind ,arglist --cl-args-- ,@body)))
;; cl multiple values are just lists in this model.
(defun cl-values (&rest vals) vals)
(defun cl-values-list (l) l)
(defmacro cl-multiple-value-bind (vars form &rest body)
  `(cl-destructuring-bind ,vars ,form ,@body))
(defmacro cl-multiple-value-setq (vars form)
  (let ((tmp (make-symbol "mv")))
    `(let ((,tmp ,form))
       ,@(let ((sets nil) (i 0) (vs vars))
           (while vs
             (setq sets (cons (list 'setq (car vs) (list 'nth i tmp)) sets) i (1+ i) vs (cdr vs)))
           (reverse sets))
       ,tmp)))

;; ---- setf: generalized-variable assignment ----
;; Expands (setf PLACE VALUE) to the right mutator for PLACE. Supported places
;; (those whose setter primitives exist): a plain variable, car/cdr and the
;; two-level c[ad][ad]r accessors, nth, elt, aref, gethash, and symbol-value.
;; Each setter returns VALUE, so (setf …) yields the last assigned value, as in
;; Emacs. Backquote-pattern places (cl-struct slots, alist-get) wait on more
;; setter primitives / lazy backquote.
;; Maps a cl-defstruct accessor symbol to its slot index (populated by
;; `cl-defstruct' when it runs, consulted by `setf--expand' when expanding later
;; top-level forms — which works because forms are processed in order).
(defvar cl-struct--slot-index nil)
;; Accessors of `:read-only' cl-defstruct slots. `setf--expand' signals the same
;; "ACCESSOR is a read-only slot" error cl-macs.el:3260 raises via gv-define-expander.
(defvar cl-struct--slot-readonly nil)
(defun setf--expand (place val)
  (if (symbolp place)
      (list 'setq place val)
    (let ((head (car place)) (args (cdr place)))
      (cond
       ((eq head 'car) (list 'setcar (car args) val))
       ((eq head 'cdr) (list 'setcdr (car args) val))
       ((eq head 'caar) (list 'setcar (list 'car (car args)) val))
       ((eq head 'cadr) (list 'setcar (list 'cdr (car args)) val))
       ((eq head 'cdar) (list 'setcdr (list 'car (car args)) val))
       ((eq head 'cddr) (list 'setcdr (list 'cdr (car args)) val))
       ;; Triple combinators: set{car,cdr} of the inner two-step accessor.
       ((eq head 'caaar) (list 'setcar (list 'caar (car args)) val))
       ((eq head 'caadr) (list 'setcar (list 'cadr (car args)) val))
       ((eq head 'cadar) (list 'setcar (list 'cdar (car args)) val))
       ((eq head 'caddr) (list 'setcar (list 'cddr (car args)) val))
       ((eq head 'cdaar) (list 'setcdr (list 'caar (car args)) val))
       ((eq head 'cdadr) (list 'setcdr (list 'cadr (car args)) val))
       ((eq head 'cddar) (list 'setcdr (list 'cdar (car args)) val))
       ((eq head 'cdddr) (list 'setcdr (list 'cddr (car args)) val))
       ((eq head 'nth) (list 'setcar (list 'nthcdr (car args) (car (cdr args))) val))
       ;; (setf (nthcdr N L) V): N=0 replaces L; else setcdr the (N-1)th cell.
       ((eq head 'nthcdr)
        (list 'let (list (list '--setf-nc-- (car args)))
              (list 'if (list '<= '--setf-nc-- 0)
                    (setf--expand (car (cdr args)) val)
                    (list 'setcdr (list 'nthcdr (list '1- '--setf-nc--) (car (cdr args))) val))))
       ((memq head '(elt seq-elt))
        ;; Bind the sequence + index once: list → setcar, array → aset.
        (list 'let (list (list '--setf-s-- (car args)) (list '--setf-n-- (car (cdr args))))
              (list 'if (list 'listp '--setf-s--)
                    (list 'setcar (list 'nthcdr '--setf-n-- '--setf-s--) val)
                    (list 'aset '--setf-s-- '--setf-n-- val))))
       ((eq head 'aref) (list 'aset (car args) (car (cdr args)) val))
       ((eq head 'gethash) (list 'puthash (car args) val (car (cdr args))))
       ((eq head 'symbol-value) (list 'set (car args) val))
       ;; (setf (default-value SYM) V) -> (set-default SYM V) (gv.el simple setter).
       ((eq head 'default-value) (list 'set-default (car args) val))
       ((eq head 'symbol-function) (list 'fset (car args) val))
       ;; (setf (get SYM PROP) V) -> (put SYM PROP V).
       ((eq head 'get) (list 'put (car args) (car (cdr args)) val))
       ;; (setf (cl--find-class NAME) CLASS) -> (put NAME 'cl--class CLASS).
       ;; `cl--find-class' stores class descriptors on the symbol's plist, exactly
       ;; as cl-preloaded.el does; this is its gv setter.
       ((eq head 'cl--find-class)
        (list 'put (car args) (list 'quote 'cl--class) val))
       ;; cl-first..cl-tenth name list positions; cl-rest is the cdr.
       ((eq head 'cl-rest) (list 'setcdr (car args) val))
       ((memq head '(cl-first cl-second cl-third cl-fourth cl-fifth
                     cl-sixth cl-seventh cl-eighth cl-ninth cl-tenth))
        (let ((idx (cdr (assq head '((cl-first . 0) (cl-second . 1) (cl-third . 2)
                                     (cl-fourth . 3) (cl-fifth . 4) (cl-sixth . 5)
                                     (cl-seventh . 6) (cl-eighth . 7) (cl-ninth . 8)
                                     (cl-tenth . 9))))))
          (list 'setcar (list 'nthcdr idx (car args)) val)))
       ;; `decoded-time' is `(cl-defstruct (decoded-time (:type list)) ...)' in
       ;; Emacs (simple.el:11111), so its accessors are list-position places whose
       ;; setter is `(setcar (nthcdr INDEX TIME) VAL)' — the same expansion a
       ;; `:type list' struct generates.  time-date.el relies on it (e.g.
       ;; `(setf (decoded-time-month time) ...)', `(cl-incf (decoded-time-year time) ...)').
       ((assq head '((decoded-time-second . 0) (decoded-time-minute . 1)
                     (decoded-time-hour . 2) (decoded-time-day . 3)
                     (decoded-time-month . 4) (decoded-time-year . 5)
                     (decoded-time-weekday . 6) (decoded-time-dst . 7)
                     (decoded-time-zone . 8)))
        (list 'setcar
              (list 'nthcdr
                    (cdr (assq head '((decoded-time-second . 0) (decoded-time-minute . 1)
                                      (decoded-time-hour . 2) (decoded-time-day . 3)
                                      (decoded-time-month . 4) (decoded-time-year . 5)
                                      (decoded-time-weekday . 6) (decoded-time-dst . 7)
                                      (decoded-time-zone . 8))))
                    (car args))
              val))
       ;; (setf (alist-get K AL &optional DEFAULT REMOVE TESTFN) V): setcdr an
       ;; existing pair (found via TESTFN or eq), else prepend. With REMOVE, a V
       ;; equal to DEFAULT deletes the entry instead.
       ((eq head 'alist-get)
        (let* ((key (car args)) (al (nth 1 args)) (default (nth 2 args))
               (remove (nth 3 args)) (testfn (nth 4 args))
               (getter (if testfn (list 'assoc key al testfn) (list 'assq key al)))
               (eq-default (if testfn (list testfn '--ag-v-- default) (list 'eql '--ag-v-- default))))
          (if remove
              (list 'let (list (list '--ag-v-- val) (list '--ag-p-- getter))
                    (list 'if eq-default
                          (list 'when '--ag-p-- (setf--expand al (list 'delq '--ag-p-- al)))
                          (list 'if '--ag-p--
                                (list 'setcdr '--ag-p-- '--ag-v--)
                                (setf--expand al (list 'cons (list 'cons key '--ag-v--) al)))))
            (list 'let (list (list '--ag-p-- getter))
                  (list 'if '--ag-p--
                        (list 'setcdr '--ag-p-- val)
                        (setf--expand al (list 'cons (list 'cons key val) al)))))))
       ;; (setf (plist-get P K) V) / (setf (cl-getf P K) V): set an existing value
       ;; cell, else prepend (K V) to P — matching Emacs's order for a new key.
       ((memq head '(plist-get cl-getf))
        (list 'let (list (list '--pg-m-- (list 'plist-member (car args) (car (cdr args)))))
              (list 'if '--pg-m--
                    (list 'setcar (list 'cdr '--pg-m--) val)
                    (setf--expand (car args)
                                  (list 'cons (car (cdr args)) (list 'cons val (car args)))))))
       ;; A read-only cl-defstruct slot: setf is an error (cl-macs.el:3260).
       ((memq head cl-struct--slot-readonly)
        (error "%s is a read-only slot" head))
       ;; A cl-defstruct accessor: (setf (NAME-SLOT s) v) -> (aset s INDEX v).
       ((assq head cl-struct--slot-index)
        (list 'aset (car args) (cdr (assq head cl-struct--slot-index)) val))
       ;; (setf (map-elt MAP KEY) V): rebind MAP to a copy with KEY updated/added
       ;; (hash-tables/arrays mutate in place; alists may grow at the head).
       ((eq head 'map-elt)
        (setf--expand (car args)
                      (list 'map--put (car args) (car (cdr args)) val)))
       ;; (setf (cl-subseq SEQ START &optional END) V): destructively copy V into
       ;; SEQ[START..END) via cl-replace, then yield V — matching cl-lib's setter.
       ((eq head 'cl-subseq)
        (list 'let (list (list '--ss-v-- val))
              (list 'cl-replace (car args) '--ss-v--
                    :start1 (car (cdr args)) :end1 (car (cddr args)))
              '--ss-v--))
       ;; Control-flow places (gv.el): setf threads into the value position of
       ;; each branch.  Only one branch runs, so VAL may appear textually more
       ;; than once without being evaluated twice.
       ;; (setf (if C THEN ELSE...) V) -> (if C (setf THEN V) (setf (progn ELSE...) V))
       ((eq head 'if)
        (list 'if (car args)
              (setf--expand (car (cdr args)) val)
              (setf--expand (cons 'progn (cdr (cdr args))) val)))
       ;; (setf (progn A B C) V) -> (progn A B (setf C V))
       ((eq head 'progn)
        (append (list 'progn) (butlast args)
                (list (setf--expand (car (last args)) val))))
       ;; (setf (cond (T A B) ...) V) -> (cond (T A (setf B V)) ...)
       ((eq head 'cond)
        (cons 'cond
              (mapcar (lambda (clause)
                        (append (list (car clause)) (butlast (cdr clause))
                                (list (setf--expand (car (last clause)) val))))
                      args)))
       ;; Unknown head: consult the gv.el registry first (gv.el:98 checks the
       ;; `gv-expander' property before anything else), so any place registered
       ;; via `gv-define-setter'/`gv-define-expander' (e.g. `gv-deref', user
       ;; places) works.  `setf' is exactly `(gv-letplace (_getter setter) place
       ;; (funcall setter val))' (gv.el:288).
       (t (let ((gf (function-get head 'gv-expander 'autoload)))
            (if gf
                (apply gf (lambda (_getter setter) (funcall setter val)) args)
              ;; No gv-expander: if PLACE is a macro call, expand it once and
              ;; retry (gv.el:103), else use the `(setf HEAD)' function setter
              ;; (installed via `(defalias (gv-setter aname) ...)').
              (let ((me (macroexpand-1 place)))
                (if (eq me place)
                    (let ((setter (intern (format "(setf %s)" head))))
                      (if (fboundp setter)
                          (cons 'funcall (cons (list 'function setter) (cons val args)))
                        (error "setf: unsupported place %S" place)))
                  (setf--expand me val))))))))))
(defmacro setf (&rest pairs)
  (let ((forms nil))
    (while pairs
      (setq forms (cons (setf--expand (car pairs) (car (cdr pairs))) forms))
      (setq pairs (cdr (cdr pairs))))
    (cons 'progn (reverse forms))))

;; ---- hooks (subr.el `add-hook'; C `run-hooks'/`run-hook-with-args') ----
;; Defined after the full `setf' (add-hook's depth-alist branch uses a `get'
;; place).  add-hook is ported verbatim; the depth-alist / buffer-local branches
;; are dead in elisprs's global-only, no-buffer-local model but kept faithful.
(defun add-hook (hook function &optional depth local)
  "Add to the value of HOOK the function FUNCTION.
FUNCTION is not added if already present."
  (or (boundp hook) (set hook nil))
  (or (default-boundp hook) (set-default hook nil))
  (unless (numberp depth) (setq depth (if depth 90 0)))
  (if local (unless (local-variable-if-set-p hook)
	      (set (make-local-variable hook) (list t)))
    (when (and (local-variable-if-set-p hook)
               (not (and (consp (symbol-value hook))
                         (memq t (symbol-value hook)))))
      (setq local t)))
  (let ((hook-value (if local (symbol-value hook) (default-value hook))))
    (when (or (not (listp hook-value)) (functionp hook-value))
      (setq hook-value (list hook-value)))
    (unless (member function hook-value)
      (let ((depth-sym (get hook 'hook--depth-alist)))
        (unless (zerop depth)
          (unless depth-sym
            (setq depth-sym (make-symbol "depth-alist"))
            (set depth-sym nil)
            (setf (get hook 'hook--depth-alist) depth-sym))
          (if local (make-local-variable depth-sym))
          (setf (alist-get function
                           (if local (symbol-value depth-sym)
                             (default-value depth-sym))
                           0)
                depth))
        (setq hook-value
	      (if (< 0 depth)
		  (append hook-value (list function))
		(cons function hook-value)))
        (when depth-sym
          (let ((depth-alist (if local (symbol-value depth-sym)
                               (default-value depth-sym))))
            (when depth-alist
              (setq hook-value
                    (sort (if (< 0 depth) hook-value (copy-sequence hook-value))
                          (lambda (f1 f2)
                            (< (alist-get f1 depth-alist 0 nil #'eq)
                               (alist-get f2 depth-alist 0 nil #'eq))))))))))
    (if local
	(progn
	  (and (symbolp function)
	       (get function 'permanent-local-hook)
	       (not (get hook 'permanent-local))
	       (put hook 'permanent-local 'permanent-local-hook))
	  (set hook hook-value))
      (set-default hook hook-value))))
(defun run-hook-with-args (hook &rest args)
  "Run HOOK with the specified arguments ARGS.
HOOK should be a symbol; its value is a function or a list of functions."
  (when (boundp hook)
    (let ((value (symbol-value hook)))
      (if (functionp value)
          (apply value args)
        (dolist (f value)
          (unless (eq f t)
            (apply f args)))
        nil))))
(defun run-hooks (&rest hooks)
  "Run each hook in HOOKS, which are symbols whose values are lists of functions."
  (dolist (hook hooks)
    (run-hook-with-args hook))
  nil)

;; ---- pcase: structural `cond` (non-backquote subset) ----
;; Supported patterns (compiled to tests + bindings at macroexpansion time):
;;   _              wildcard — always matches
;;   nil / t / :kw  self-quoting literals (matched with `equal`)
;;   NUMBER STRING  self-quoting literals
;;   SYMBOL         binds SYMBOL to the value (anything not a literal above)
;;   'X / (quote X) literal X
;;   (pred FN)      matches when (FN VALUE) is non-nil; (pred (FN ARGS...)) →
;;                  (FN ARGS... VALUE), as in Emacs
;;   (guard EXPR)   matches when EXPR — which can read earlier bindings — is non-nil
;;   (and PAT...)   matches when every PAT matches (bindings accumulate)
;;   (or PAT...)    matches when any PAT matches
;; Backquote patterns (`(,a ,b)) are NOT supported here: this reader expands
;; backquote eagerly at read time, so no `\`' form survives for pcase to
;; destructure. They need lazy backquote first.
(defun pcase--list->cons (pats)
  ;; (P1 P2 ...) -> (cons P1 (cons P2 ... nil)) so a `list' pattern reuses the
  ;; `cons' structural matcher.
  (if (null pats) nil (list 'cons (car pats) (pcase--list->cons (cdr pats)))))
;; How `pred'/`app' call FN on the value: a lambda or symbol gets VAL as its one
;; argument; a partial application (F ARGS…) appends VAL.
(defun pcase--apply (fn val)
  (cond ((and (consp fn) (eq (car fn) 'lambda)) (list 'funcall fn val))
        ((consp fn) (append fn (list val)))
        (t (list fn val))))
;; ---- rx: compile an `rx' S-expression form to a regexp string (a useful
;; subset of rx.el — string/char literals, the named character classes and
;; anchors, group/or/seq, the quantifiers, char sets `(any …)' and `(not …)').
(defun rx--symbol (s)
  (cond
   ((memq s '(bol line-start)) "^")
   ((memq s '(eol line-end)) "$")
   ((memq s '(bos string-start buffer-start bot)) "\\`")
   ((memq s '(eos string-end buffer-end eot)) "\\'")
   ((eq s 'point) "\\=")
   ((memq s '(word-start bow)) "\\<")
   ((memq s '(word-end eow)) "\\>")
   ((eq s 'word-boundary) "\\b")
   ((eq s 'not-word-boundary) "\\B")
   ((eq s 'symbol-start) "\\_<")
   ((eq s 'symbol-end) "\\_>")
   ((memq s '(digit numeric num)) "[[:digit:]]")
   ((memq s '(alpha alphabetic letter)) "[[:alpha:]]")
   ((memq s '(alnum alphanumeric)) "[[:alnum:]]")
   ((memq s '(space whitespace white)) "[[:space:]]")
   ((memq s '(upper upper-case)) "[[:upper:]]")
   ((memq s '(lower lower-case)) "[[:lower:]]")
   ((memq s '(punct punctuation)) "[[:punct:]]")
   ((eq s 'blank) "[[:blank:]]")
   ((memq s '(cntrl control)) "[[:cntrl:]]")
   ((memq s '(hex hex-digit xdigit)) "[[:xdigit:]]")
   ((memq s '(graph graphic)) "[[:graph:]]")
   ((memq s '(print printing)) "[[:print:]]")
   ((memq s '(word wordchar)) "\\w")
   ((eq s 'not-wordchar) "\\W")
   ((memq s '(nonl not-newline any)) ".")
   ;; anychar/anything match ANY char incl. newline (empty negated class).
   ((memq s '(anychar anything)) "[^z-a]")
   ((eq s 'unmatchable) "\\`a\\`")
   (t (error "rx: unknown symbol %S" s))))
(defun rx--atom-p (s)
  ;; A regexp that a quantifier can suffix without a shy group.
  (let ((n (length s)))
    (cond ((= n 1) t)
          ((and (eq (aref s 0) ?\[) (eq (aref s (1- n)) ?\])) t)
          ((and (= n 2) (eq (aref s 0) ?\\)) t)
          ;; `\sC` / `\SC` syntax-class matchers are single-char atoms.
          ((and (= n 3) (eq (aref s 0) ?\\) (memq (aref s 1) '(?s ?S))) t)
          ((and (>= n 4) (eq (aref s 0) ?\\) (eq (aref s 1) ?\()) t)
          (t nil))))
(defun rx--quant-body (args)
  (let ((s (rx--seq args))) (if (rx--atom-p s) s (concat "\\(?:" s "\\)"))))
(defun rx--class-in (s)
  (substring (rx--symbol s) 1 (1- (length (rx--symbol s)))))
(defun rx--charset (args)
  (mapconcat (lambda (a) (cond ((stringp a) a)
                               ((integerp a) (char-to-string a))
                               ((symbolp a) (rx--class-in a))
                               (t "")))
             args ""))
(defun rx--not (arg)
  (cond
   ((and (consp arg) (memq (car arg) '(any in char)))
    (concat "[^" (rx--charset (cdr arg)) "]"))
   ((eq arg 'word-boundary) "\\B")
   ((symbolp arg)
    (let ((s (rx--symbol arg)))
      (if (and (> (length s) 1) (eq (aref s 0) ?\[))
          (concat "[^" (substring s 1 (1- (length s))) "]")
        (concat "[^" s "]"))))
   (t (error "rx: bad (not ...) %S" arg))))
(defun rx--form (form)
  (cond
   ((stringp form) (regexp-quote form))
   ((integerp form) (regexp-quote (char-to-string form)))
   ((symbolp form) (rx--symbol form))
   ((consp form) (rx--list form))
   (t (error "rx: bad form %S" form))))
(defun rx--seq (forms) (mapconcat 'rx--form forms ""))
(defun rx--1char (a) (cond ((integerp a) (char-to-string a)) ((stringp a) a) (t "")))
(defun rx--all-1char-p (args)
  (let ((ok t))
    (while args
      (let ((a (car args)))
        (unless (or (integerp a) (and (stringp a) (= (length a) 1))) (setq ok nil)))
      (setq args (cdr args)))
    ok))
(defun rx--syntax-code (sym)
  ;; Map an rx `(syntax CLASS)` name to its `\sC` syntax-class code.
  (cond ((eq sym 'whitespace) "-")
        ((eq sym 'word) "w")
        ((eq sym 'symbol) "_")
        ((eq sym 'punctuation) ".")
        ((eq sym 'open-parenthesis) "(")
        ((eq sym 'close-parenthesis) ")")
        ((eq sym 'string-quote) "\"")
        ((eq sym 'comment-start) "<")
        ((eq sym 'comment-end) ">")
        ((eq sym 'escape) "\\\\")
        (t (error "rx: unknown syntax class %S" sym))))
(defun rx--list (form)
  (let ((head (car form)) (args (cdr form)))
    (cond
     ((memq head '(seq sequence : and)) (rx--seq args))
     ((memq head '(or |))
      ;; Emacs folds all-single-character alternatives into a char class.
      (if (and args (rx--all-1char-p args))
          (concat "[" (mapconcat 'rx--1char args "") "]")
        (concat "\\(?:" (mapconcat 'rx--form args "\\|") "\\)")))
     ((memq head '(group submatch)) (concat "\\(" (rx--seq args) "\\)"))
     ((memq head '(group-n submatch-n))
      (concat "\\(?" (number-to-string (car args)) ":" (rx--seq (cdr args)) "\\)"))
     ((memq head '(zero-or-more * 0+)) (concat (rx--quant-body args) "*"))
     ((memq head '(one-or-more + 1+)) (concat (rx--quant-body args) "+"))
     ((memq head '(zero-or-one opt optional)) (concat (rx--quant-body args) "?"))
     ;; `?` reads as the space char (32) and `??` as the `?` char (63); Emacs's
     ;; rx treats them as the greedy/non-greedy optional operators.
     ((eql head 32) (concat (rx--quant-body args) "?"))
     ((eql head 63) (concat (rx--quant-body args) "??"))
     ((eq head 'syntax)
      ;; Emacs emits the `\w` shorthand for word syntax, `\sC` for the rest.
      (if (eq (car args) 'word) "\\w" (concat "\\s" (rx--syntax-code (car args)))))
     ((eq head '=) (concat (rx--quant-body (cdr args)) "\\{" (number-to-string (car args)) "\\}"))
     ((eq head '>=) (concat (rx--quant-body (cdr args)) "\\{" (number-to-string (car args)) ",\\}"))
     ((memq head '(** repeat))
      (if (and (integerp (car args)) (integerp (nth 1 args)))
          (concat (rx--quant-body (nthcdr 2 args)) "\\{" (number-to-string (car args)) ","
                  (number-to-string (nth 1 args)) "\\}")
        (concat (rx--quant-body (cdr args)) "\\{" (number-to-string (car args)) "\\}")))
     ((memq head '(any in char)) (concat "[" (rx--charset args) "]"))
     ((eq head 'not) (rx--not (car args)))
     ((memq head '(regexp regex)) (car args))
     ((eq head 'literal) (regexp-quote (car args)))
     ((eq head 'backref) (concat "\\" (number-to-string (car args))))
     ;; minimal-match/maximal-match: render the inner form (greediness control
     ;; isn't modeled separately here).
     ((memq head '(minimal-match maximal-match)) (rx--form (car args)))
     (t (error "rx: unknown form %S" head)))))
(defmacro rx (&rest forms) (rx--seq forms))
(defun rx-to-string (form &optional no-group)
  "Translate the rx FORM to a regexp string; wrap in a shy group unless NO-GROUP
or the result is already atomic/grouped."
  (let ((s (rx--form form)))
    (if (or no-group (rx--atom-p s)) s (concat "\\(?:" s "\\)"))))

(defun pcase--literal-p (pat)
  (or (numberp pat) (stringp pat) (keywordp pat) (eq pat t) (null pat)))
(defun pcase--compile (pat val)
  ;; Return (TESTS . BINDS): TESTS a list of boolean forms over VAL, BINDS a
  ;; list of (SYM ACCESSOR) let*-bindings. In this subset every binder captures
  ;; VAL whole, so accessors never car/cdr an atom.
  (cond
   ((eq pat '_) (cons nil nil))
   ((pcase--literal-p pat) (cons (list (list 'equal val (list 'quote pat))) nil))
   ((symbolp pat) (cons nil (list (list pat val))))
   ((consp pat)
    (let ((head (car pat)))
      (cond
       ((eq head 'quote) (cons (list (list 'equal val pat)) nil))
       ((eq head 'pred) (cons (list (pcase--apply (car (cdr pat)) val)) nil))
       ((eq head 'guard) (cons (list (car (cdr pat))) nil))
       ;; (rx …): match VAL (a string) against the compiled regexp.
       ((eq head 'rx)
        (cons (list (list 'and (list 'stringp val) (list 'string-match (rx--seq (cdr pat)) val))) nil))
       ((eq head 'cl-type) (cons (list (list (cl-typecase--pred (car (cdr pat))) val)) nil))
       ;; (app FN PAT): match PAT against (FN VAL).
       ((eq head 'app)
        (let* ((tv (make-symbol "app"))
               (r (pcase--compile (nth 2 pat) tv)))
          (cons (car r) (cons (list tv (pcase--apply (nth 1 pat) val)) (cdr r)))))
       ;; (let PAT EXPR): match PAT against EXPR's value (EXPR ignores VAL).
       ((eq head 'let)
        (let* ((tv (make-symbol "let"))
               (r (pcase--compile (nth 1 pat) tv)))
          (cons (car r) (cons (list tv (nth 2 pat)) (cdr r)))))
       ((eq head 'and)
        (let ((tests nil) (binds nil))
          (dolist (p (cdr pat))
            (let ((r (pcase--compile p val)))
              (setq tests (append tests (car r)))
              (setq binds (append binds (cdr r)))))
          (cons tests binds)))
       ((eq head 'or)
        (let ((alts nil) (binds nil))
          (dolist (p (cdr pat))
            (let ((r (pcase--compile p val)))
              (setq alts (append alts (list (cons 'and (car r)))))
              (setq binds (append binds (cdr r)))))
          (cons (list (cons 'or alts)) binds)))
       ;; Backquote patterns: this reader expands `(,a ,b) to (cons a (cons b
       ;; nil)) at read time, so a `cons' form here is a structural cons pattern.
       ;; Sub-accessors use car-safe/cdr-safe and are gated by a `consp' test.
       ((eq head 'cons)
        (let ((cr (pcase--compile (nth 1 pat) (list 'car-safe val)))
              (cd (pcase--compile (nth 2 pat) (list 'cdr-safe val))))
          (cons (cons (list 'consp val) (append (car cr) (car cd)))
                (append (cdr cr) (cdr cd)))))
       ;; `(a b) with no unquotes expands to (list 'a 'b); treat as a cons chain.
       ((eq head 'list)
        (pcase--compile (pcase--list->cons (cdr pat)) val))
       ;; Backquoted vector pattern `[,a ,b]: the reader expands it to
       ;; (vconcat (cons a (cons b nil))). Require a vector, then match the
       ;; cons-pattern against its elements as a list. The `lv' binding is
       ;; guarded so a non-vector VAL just fails the match (never errors).
       ((eq head 'vconcat)
        (let* ((lv (make-symbol "vl"))
               (r (pcase--compile (nth 1 pat) lv)))
          (cons (cons (list 'vectorp val) (car r))
                (cons (list lv (list 'if (list 'vectorp val) (list 'append val nil) nil))
                      (cdr r)))))
       ;; (seq P0 P1 ...): match each subpattern against (nth i SV), where SV is
       ;; the elements as a list (or nil if VAL is not a sequence).
       ((eq head 'seq)
        (let ((sv (make-symbol "sv")) (tests (list (list 'sequencep val))) (binds nil) (i 0))
          (setq binds (list (list sv (list 'if (list 'sequencep val) (list 'append val nil) nil))))
          (dolist (p (cdr pat))
            (let ((r (pcase--compile p (list 'nth i sv))))
              (setq tests (append tests (car r)) binds (append binds (cdr r))))
            (setq i (1+ i)))
          (cons tests binds)))
       (t (error "pcase: unsupported pattern %S" pat)))))
   (t (error "pcase: unsupported pattern %S" pat))))
(defun pcase--clause (clause)
  ;; Build one `cond' clause (TEST BODY) from a pcase clause (PATTERN BODY...).
  ;; Bindings wrap BOTH the test and the body via `let*' so a `guard' in TEST
  ;; sees the binders. This `cond' shape (like `cl-case') expands cleanly when
  ;; nested in a macro-produced `defun' (e.g. an ERT `should'); a `catch'/`throw'
  ;; shape does not — it miscompiles to a "void variable" error there.
  (let* ((r (pcase--compile (car clause) '--pcase-v--))
         (tests (car r))
         (binds (cdr r))
         (conj (if tests (cons 'and tests) t))
         (test (if binds (list 'let* binds conj) conj))
         (body (cons 'progn (cdr clause))))
    (list test (if binds (list 'let* binds body) body))))
(defmacro pcase (expr &rest clauses)
  `(let ((--pcase-v-- ,expr))
     (cond ,@(mapcar (function pcase--clause) clauses))))
(defmacro pcase-exhaustive (expr &rest clauses)
  ;; Like pcase, but error if no clause matches.
  `(let ((--pcase-v-- ,expr))
     (cond ,@(mapcar (function pcase--clause) clauses)
           (t (error "No clause matching %S" --pcase-v--)))))
(defmacro pcase-let (bindings &rest body)
  ;; Each binding is (PATTERN VALUE); destructure VALUE against PATTERN (reusing
  ;; `pcase--compile'), binding the pattern variables for BODY.
  (let ((lets nil) (i 0))
    (dolist (b bindings)
      (let* ((tv (intern (concat "--pl-" (number-to-string i) "--")))
             (r (pcase--compile (car b) tv)))
        (setq lets (cons (list tv (car (cdr b))) lets))
        (dolist (bind (cdr r)) (setq lets (cons bind lets)))
        (setq i (1+ i))))
    `(let* ,(reverse lets) ,@body)))
(defmacro pcase-let* (bindings &rest body)
  ;; Sequential pcase-let; our `let*' expansion already binds in order.
  (cons 'pcase-let (cons bindings body)))
(defmacro pcase-setq (&rest args)
  ;; Pairs of PATTERN VALUE: destructure each VALUE and `setq' the pattern's
  ;; variables (the existing bindings, not new ones).
  (let ((forms nil) (i 0))
    (while args
      (let* ((pat (car args)) (val (car (cdr args)))
             (tv (intern (concat "--ps-" (number-to-string i) "--")))
             (r (pcase--compile pat tv)))
        (setq args (cdr (cdr args)) i (1+ i))
        (setq forms
              (cons (list 'let (list (list tv val))
                          (cons 'progn
                                (mapcar (lambda (b) (list 'setq (car b) (car (cdr b))))
                                        (cdr r))))
                    forms))))
    (cons 'progn (reverse forms))))
(defmacro pcase-dolist (spec &rest body)
  ;; Iterate (cadr SPEC), destructuring each element against (car SPEC).
  (let ((ev (make-symbol "e")))
    (list 'dolist (list ev (car (cdr spec)))
          (cons 'pcase-let (cons (list (list (car spec) ev)) body)))))
;; help.el:2302 — defined here (after `pcase') because its body uses `pcase' and
;; elisprs expands macros when the `defun' is read, not when it is called.
(defun help-split-fundoc (docstring def &optional section)
  "Split a function DOCSTRING into the actual doc and the usage info.
Return (USAGE . DOC), where USAGE is a string describing the argument
list of DEF, such as \"(apply FUNCTION &rest ARGUMENTS)\".
DEF is the function whose usage we're looking for in DOCSTRING.
With SECTION nil, return nil if there is no usage info; conversely,
SECTION t means to return (USAGE . DOC) even if there's no usage info.
When SECTION is \\='usage or \\='doc, return only that part."
  (let* ((found (and docstring
                     (string-match "\n\n(fn\\(\\( .*\\)?)\\)\\'" docstring)))
         (doc (if found
                  (and (memq section '(t nil doc))
                       (not (zerop (match-beginning 0)))
                       (substring docstring 0 (match-beginning 0)))
                docstring))
         (usage (and found
                     (memq section '(t nil usage))
                     (let ((tail (match-string 1 docstring)))
                       (format "(%s%s"
                               (if (and (symbolp def) def)
                                   (help--docstring-quote (format "%S" def))
                                 'anonymous)
                               tail)))))
    (pcase section
      (`nil (and usage (cons usage doc)))
      (`t (cons usage doc))
      (`usage usage)
      (`doc doc))))
(defmacro seq-let (args seq &rest body)
  ;; Positionally bind ARGS to the elements of SEQ for BODY; `&rest` binds the
  ;; tail. ARGS may be a list or a vector pattern.
  (when (vectorp args) (setq args (append args nil)))
  (let ((s (make-symbol "seq")) (binds nil) (i 0) (more t))
    (while (and args more)
      (let ((a (car args)))
        (if (eq a '&rest)
            (setq binds (cons (list (car (cdr args)) (list 'seq-drop s i)) binds) more nil)
          (setq binds (cons (list a (list 'elt s i)) binds) i (1+ i))))
      (setq args (cdr args)))
    `(let* ((,s ,seq) ,@(reverse binds)) ,@body)))
(defmacro seq-setq (args seq)
  ;; Like `seq-let` but assigns to existing places with `setq` (positional, plus
  ;; `&rest` for the tail).
  (let ((s (make-symbol "seq")) (sets nil) (i 0) (more t) (rest args))
    (while (and rest more)
      (let ((a (car rest)))
        (if (eq a '&rest)
            (setq sets (cons (list 'setq (car (cdr rest)) (list 'seq-drop s i)) sets) more nil)
          (setq sets (cons (list 'setq a (list 'elt s i)) sets) i (1+ i))))
      (setq rest (cdr rest)))
    `(let ((,s ,seq)) ,@(reverse sets))))
(defmacro seq-doseq (spec &rest body)
  ;; (seq-doseq (VAR SEQUENCE) BODY...) — iterate VAR over any sequence's
  ;; elements. Returns the sequence (like Emacs, via seq-do).
  (let ((var (car spec)) (seq (car (cdr spec))) (sv (make-symbol "seq")))
    `(let* ((,sv ,seq) (--seq-doseq-tail-- (append ,sv nil)) (,var nil))
       (while --seq-doseq-tail--
         (setq ,var (car --seq-doseq-tail--))
         ,@body
         (setq --seq-doseq-tail-- (cdr --seq-doseq-tail--)))
       ,sv)))
(defun macroexp-progn (forms) (if (cdr forms) (cons 'progn forms) (car forms)))

;; macroexp-unprogn / macroexp-let* / macroexp-let2 (macroexp.el:560,565,601).
;; `macroexp-let2' evaluates BODY with SYM bound to an expression for EXP: if
;; EXP is copyable per TEST, SYM is EXP itself; else SYM is a fresh symbol and
;; the result wraps a `let*' that evaluates EXP once.  gv's place-expanders use
;; it to guarantee each place subform is evaluated exactly once.
(defun macroexp-unprogn (exp)
  (if (eq (car-safe exp) 'progn) (or (cdr exp) '(nil)) (list exp)))
(defun macroexp-let* (bindings exp)
  (cond ((null bindings) exp)
        ((eq 'let* (car-safe exp))
         (append (list 'let* (append bindings (car (cdr exp)))) (cdr (cdr exp))))
        (t (list 'let* bindings exp))))
(defmacro macroexp-let2 (test sym exp &rest body)
  (declare (indent 3))
  (let ((bodysym (make-symbol "body"))
        (expsym (make-symbol "exp")))
    `(let* ((,expsym ,exp)
            (,sym (if (funcall #',(or test 'macroexp-const-p) ,expsym)
                      ,expsym (make-symbol ,(symbol-name sym))))
            (,bodysym ,(macroexp-progn body)))
       (if (eq ,sym ,expsym) ,bodysym
         (macroexp-let* (list (list ,sym ,expsym)) ,bodysym)))))
;; macroexp-small-p (macroexp.el:678): is EXP small enough to duplicate?  gv's
;; `if'/`cond' expanders consult it (after `lexical-binding') to decide between
;; duplicating the DO code into each branch and building a runtime closure pair.
(defun macroexp--maxsize (exp size)
  (cond ((< size 0) size)
        ((symbolp exp) (1- size))
        ((stringp exp) (- size (/ (length exp) 16)))
        ((vectorp exp)
         (dotimes (i (length exp))
           (setq size (macroexp--maxsize (aref exp i) size)))
         (1- size))
        ((consp exp)
         (dolist (e exp)
           (setq size (macroexp--maxsize e size)))
         (1- size))
        (t -1)))
(defun macroexp-small-p (exp)
  (> (macroexp--maxsize exp 10) 0))

;;; ---- gv.el: generalized-variable place expanders ----
;; Faithful port of emacs-lisp/gv.el's higher-order place model.  A place-
;; expander is a function (do -> code) where DO is (getter setter -> code); it is
;; stored on the head symbol's `gv-expander' property (gv.el:71).  `gv-get' turns
;; a PLACE form into that call, `gv-letplace' is its macro sugar, `gv-ref'/
;; `gv-deref' build first-class references, and `setf' (via `setf--expand's'
;; fallback) consults this registry for any place its own cond does not handle.
(define-error 'gv-invalid-place "Invalid place expression")

(defun gv-get (place do)
  "Build the code that applies DO to PLACE (gv.el:80)."
  (cond
   ((symbolp place)
    (let ((me (macroexpand-1 place)))
      (if (eq me place)
          (funcall do place (lambda (v) (list 'setq place v)))
        (gv-get me do))))
   ((not (consp place)) (signal 'gv-invalid-place (list place)))
   (t
    (let* ((head (car place))
           (gf (function-get head 'gv-expander 'autoload)))
      (if gf (apply gf do (cdr place))
        (let ((me (macroexpand-1 place)))
          (if (and (eq me place) (get head 'compiler-macro))
              (setq me (apply (get head 'compiler-macro) place (cdr place))))
          (if (and (eq me place) (fboundp head)
                   (symbolp (symbol-function head)))
              (setq me (cons (symbol-function head) (cdr place))))
          (if (eq me place)
              (if (and (symbolp head) (get head 'setf-method))
                  (error "Incompatible place needs recompilation: %S" head)
                (let ((setter (gv-setter head)))
                  (gv--defsetter head (lambda (&rest args) (cons setter args))
                                 do (cdr place))))
            (gv-get me do))))))))

(defmacro gv-letplace (vars place &rest body)
  "Build the code manipulating the generalized variable PLACE (gv.el:133)."
  (declare (indent 2))
  `(gv-get ,place (lambda ,vars ,@body)))

(defmacro gv-define-expander (name handler)
  "Use HANDLER to handle NAME as a generalized var (gv.el:150)."
  (declare (indent 1))
  `(function-put ',name 'gv-expander ,handler))

(defun gv--defsetter (name setter do args &optional vars)
  "Helper used by code generated by `gv-define-setter' (gv.el:227)."
  (if (null args)
      (let ((vars (nreverse vars)))
        (funcall do (cons name vars) (lambda (v) (apply setter v vars))))
    (macroexp-let2 nil v (car args)
      (gv--defsetter name setter do (cdr args) (cons v vars)))))

(defmacro gv-define-setter (name arglist &rest body)
  "Define a setter method for generalized variable NAME (gv.el:242)."
  (declare (indent 2))
  `(gv-define-expander ,name
     (lambda (do &rest args)
       (gv--defsetter ',name (lambda ,arglist ,@body) do args))))

(defmacro gv-define-simple-setter (name setter &optional fix-return)
  "Define a simple setter method for generalized variable NAME (gv.el:262).
Assignments of VAL to (NAME ARGS...) become (SETTER ARGS... VAL)."
  `(gv-define-setter ,name (val &rest args)
     ,(if fix-return
          `(macroexp-let2 nil v val
             `(progn
                (,',setter ,@args ,v)
                ,v))
        ``(,',setter ,@args ,val))))

;;; The common generalized variables (gv.el:348).
(gv-define-simple-setter aref aset)
(gv-define-simple-setter char-table-range set-char-table-range)
(gv-define-simple-setter car setcar)
(gv-define-simple-setter cdr setcdr)
(gv-define-setter caar (val x) `(setcar (car ,x) ,val))
(gv-define-setter cadr (val x) `(setcar (cdr ,x) ,val))
(gv-define-setter cdar (val x) `(setcdr (car ,x) ,val))
(gv-define-setter cddr (val x) `(setcdr (cdr ,x) ,val))
(gv-define-setter elt (store seq n)
  `(if (listp ,seq) (setcar (nthcdr ,n ,seq) ,store)
     (aset ,seq ,n ,store)))
(gv-define-simple-setter get put)
(gv-define-setter gethash (val k h &optional _d) `(puthash ,k ,val ,h))
(put 'nth 'gv-expander
     (lambda (do idx list)
       (macroexp-let2 nil c `(nthcdr ,idx ,list)
         (funcall do `(car ,c) (lambda (v) `(setcar ,c ,v))))))
(gv-define-simple-setter symbol-function fset)
(gv-define-simple-setter symbol-plist setplist)
(gv-define-simple-setter symbol-value set)
(put 'nthcdr 'gv-expander
     (lambda (do n place)
       (macroexp-let2 nil idx n
         (gv-letplace (getter setter) place
           (funcall do `(nthcdr ,idx ,getter)
                    (lambda (v) `(if (<= ,idx 0) ,(funcall setter v)
                              (setcdr (nthcdr (1- ,idx) ,getter) ,v))))))))
(gv-define-simple-setter default-value set-default)

;;; Control-flow and "occasionally handy" place expanders (gv.el:459).  These
;; show up as the output of macroexpanding real places (e.g. struct accessors)
;; and, in nadvice, as literal `(gv-ref (cond ...))'.  Since elisprs's
;; `lexical-binding' is nil, the `(not lexical-binding)' branch always fires:
;; the DO code is duplicated into each branch rather than closed over at runtime.
(put 'progn 'gv-expander
     (lambda (do &rest exps)
       (let ((start (butlast exps))
             (end (car (last exps))))
         (if (null start) (gv-get end do)
           `(progn ,@start ,(gv-get end do))))))
(let ((let-expander
       (lambda (letsym)
         (lambda (do bindings &rest body)
           `(,letsym ,bindings
                     ,@(macroexp-unprogn
                        (gv-get (macroexp-progn body) do)))))))
  (put 'let 'gv-expander (funcall let-expander 'let))
  (put 'let* 'gv-expander (funcall let-expander 'let*)))
(put 'if 'gv-expander
     (lambda (do test then &rest else)
       (if (or (not lexical-binding)
               (macroexp-small-p (funcall do 'dummy (lambda (_) 'dummy))))
           `(if ,test ,(gv-get then do)
              ,@(macroexp-unprogn (gv-get (macroexp-progn else) do)))
         (let ((v (gensym "v")))
           (macroexp-let2 nil
               gv `(if ,test ,(gv-letplace (getter setter) then
                                `(cons (lambda () ,getter)
                                       (lambda (,v) ,(funcall setter v))))
                     ,(gv-letplace (getter setter) (macroexp-progn else)
                        `(cons (lambda () ,getter)
                               (lambda (,v) ,(funcall setter v)))))
             (funcall do `(funcall (car ,gv))
                      (lambda (v) `(funcall (cdr ,gv) ,v))))))))
(put 'cond 'gv-expander
     (lambda (do &rest branches)
       (if (or (not lexical-binding)
               (macroexp-small-p (funcall do 'dummy (lambda (_) 'dummy))))
           `(cond
             ,@(mapcar (lambda (branch)
                         (if (cdr branch)
                             (cons (car branch)
                                   (macroexp-unprogn
                                    (gv-get (macroexp-progn (cdr branch)) do)))
                           (gv-get (car branch) do)))
                       branches))
         (let ((v (gensym "v")))
           (macroexp-let2 nil
               gv `(cond
                    ,@(mapcar
                       (lambda (branch)
                         (if (cdr branch)
                             `(,(car branch)
                               ,@(macroexp-unprogn
                                  (gv-letplace (getter setter)
                                      (macroexp-progn (cdr branch))
                                    `(cons (lambda () ,getter)
                                           (lambda (,v) ,(funcall setter v))))))
                           (gv-letplace (getter setter)
                               (car branch)
                             `(cons (lambda () ,getter)
                                    (lambda (,v) ,(funcall setter v))))))
                       branches))
             (funcall do `(funcall (car ,gv))
                      (lambda (v) `(funcall (cdr ,gv) ,v))))))))
(put 'cons 'gv-expander
     (lambda (do a d)
       (gv-letplace (agetter asetter) a
         (gv-letplace (dgetter dsetter) d
           (funcall do
                    `(cons ,agetter ,dgetter)
                    (lambda (v)
                      (macroexp-let2 nil v v
                        `(progn
                           ,(funcall asetter `(car ,v))
                           ,(funcall dsetter `(cdr ,v))
                           ,v))))))))
(put 'logand 'gv-expander
     (lambda (do place &rest masks)
       (gv-letplace (getter setter) place
         (macroexp-let2 macroexp-copyable-p
             mask (if (cdr masks) `(logand ,@masks) (car masks))
           (funcall
            do `(logand ,getter ,mask)
            (lambda (v)
              (macroexp-let2 nil v v
                `(progn
                   ,(funcall setter
                             `(logior (logand ,v ,mask)
                                      (logand ,getter (lognot ,mask))))
                   ,v))))))))
;; (setf (error ...) ..) appears naturally from macroexpanding places like
;; (setf (pcase-exhaustive ...)) (gv.el:646).
(gv-define-expander error (lambda (_do &rest args) `(error . ,args)))

;;; References (gv.el:597).
(defmacro gv-ref (place)
  "Return a reference to PLACE (gv.el:600).
This is like the `&' operator of the C language."
  (gv-letplace (getter setter) place
    `(cons (lambda () ,getter)
           (lambda (gv--val) ,(funcall setter 'gv--val)))))
(defsubst gv-deref (ref)
  "Dereference REF, returning the referenced value (gv.el:621)."
  (funcall (car ref)))
(gv-define-setter gv-deref (v ref) `(funcall (cdr ,ref) ,v))

;; (setf (eq a 7) B) means (setq a 7) or (setq a nil) per B's truthiness — used
;; by define-minor-mode's :variable (gv.el:813).
(gv-define-expander eq
  (lambda (do place val)
    (gv-letplace (getter setter) place
      (macroexp-let2 nil val val
        (funcall do `(eq ,getter ,val)
                 (lambda (v)
                   `(cond
                     (,v ,(funcall setter val))
                     ((eq ,getter ,val) ,(funcall setter `(not ,val))))))))))

(defmacro cl-function (f) (list 'function f))

(defun hash-table-empty-p (h) (= 0 (hash-table-count h)))

;;; ---- map.el (subset) ----
;; A generic key/value interface over alists, hash-tables and arrays. A list
;; whose first element is an atom is treated as a plist (KEY VALUE KEY VALUE...),
;; exactly like Emacs map.el; otherwise it is an alist. Alist lookups default to
;; `equal`, plist lookups default to `eq` (plist-member's default).
(defun map--plist-p (list)
  "Return non-nil if LIST is the start of a nonempty plist map."
  (and (consp list) (atom (car list))))
(defun map-elt (map key &optional default testfn)
  (cond
   ((hash-table-p map) (gethash key map default))
   ((listp map)
    (if (map--plist-p map)
        (let ((res (plist-member map key testfn)))
          (if res (cadr res) default))
      (let ((entry (assoc key map (or testfn #'equal))))
        (if entry (cdr entry) default))))
   ((arrayp map)
    (if (and (integerp key) (>= key 0) (< key (length map)))
        (aref map key)
      default))
   (t default)))
(defun map-contains-key (map key &optional testfn)
  (cond
   ((hash-table-p map)
    (let ((sentinel (list 'map--miss)))
      (not (eq sentinel (gethash key map sentinel)))))
   ((listp map)
    (if (map--plist-p map)
        (plist-member map key testfn)
      (and (assoc key map (or testfn #'equal)) t)))
   ((arrayp map) (and (integerp key) (>= key 0) (< key (length map))))
   (t nil)))
(defun map-keys (map) (map-apply (lambda (k _v) k) map))
(defun map-values (map) (map-apply (lambda (_k v) v) map))
(defun map-pairs (map) (map-apply #'cons map))
(defun map-length (map)
  (cond
   ((hash-table-p map) (hash-table-count map))
   ((listp map) (if (map--plist-p map) (/ (length map) 2) (length map)))
   ((arrayp map) (length map))
   (t 0)))
(defun map-empty-p (map) (= 0 (map-length map)))
(defun map-do (function map)
  (cond
   ((hash-table-p map) (maphash function map) nil)
   ((listp map)
    (if (map--plist-p map)
        (while map
          (funcall function (car map) (cadr map))
          (setq map (cddr map)))
      (dolist (pair map) (funcall function (car pair) (cdr pair))))
    nil)
   ((arrayp map)
    (dotimes (i (length map)) (funcall function i (aref map i)))
    nil)))
(defun map-apply (function map)
  (let ((acc nil))
    (map-do (lambda (k v) (setq acc (cons (funcall function k v) acc))) map)
    (nreverse acc)))
(defun map-filter (pred map)
  (let ((acc nil))
    (map-do (lambda (k v) (when (funcall pred k v) (setq acc (cons (cons k v) acc)))) map)
    (nreverse acc)))
(defun map-remove (pred map)
  (map-filter (lambda (k v) (not (funcall pred k v))) map))
(defun map-some (pred map)
  (catch 'map--some
    (map-do (lambda (k v) (let ((r (funcall pred k v))) (when r (throw 'map--some r)))) map)
    nil))
(defun map-every-p (pred map)
  (catch 'map--every
    (map-do (lambda (k v) (unless (funcall pred k v) (throw 'map--every nil))) map)
    t))
(defun map-nested-elt (map keys &optional default)
  (let ((m map))
    (while (and keys m)
      (setq m (map-elt m (car keys)) keys (cdr keys)))
    (if keys default (or m default))))
(defun map-delete (map key)
  (cond
   ((hash-table-p map) (remhash key map) map)
   ((listp map)
    (if (map--plist-p map)
        (let ((res nil))
          (while map
            (unless (eq (car map) key) (setq res (cons (cadr map) (cons (car map) res))))
            (setq map (cddr map)))
          (nreverse res))
      (let ((res nil))
        (dolist (pair map) (unless (equal (car pair) key) (setq res (cons pair res))))
        (nreverse res))))
   (t map)))
;; Internal: return MAP updated so KEY maps to VALUE (used by setf map-elt).
(defun map--put (map key value)
  (cond
   ((hash-table-p map) (puthash key value map) map)
   ((listp map)
    (if (map--plist-p map)
        (plist-put map key value)
      (let ((entry (assoc key map #'equal)))
        (if entry (progn (setcdr entry value) map)
          (cons (cons key value) map)))))
   ((arrayp map) (aset map key value) map)
   (t (error "map--put: unsupported map type"))))
(defun map--into (pairs type)
  (cond
   ((eq type 'list) (let ((acc nil)) (dolist (p pairs) (setq acc (map--put acc (car p) (cdr p)))) (nreverse acc)))
   ((eq type 'alist) (let ((acc nil)) (dolist (p pairs) (setq acc (map--put acc (car p) (cdr p)))) (nreverse acc)))
   ((eq type 'hash-table)
    (let ((h (make-hash-table :test 'equal)))
      (dolist (p pairs) (puthash (car p) (cdr p) h)) h))
   ;; (hash-table :test TEST …) — the keyword-spec form.
   ((and (consp type) (eq (car type) 'hash-table))
    (let ((h (make-hash-table :test (or (plist-get (cdr type) :test) 'eql))))
      (dolist (p pairs) (puthash (car p) (cdr p) h)) h))
   (t (error "map-into: unsupported type %S" type))))
(defun map-into (map type) (map--into (map-pairs map) type))
(defun map-insert (map key value)
  "Return a new map like MAP with KEY mapped to VALUE (MAP is unchanged)."
  (cond ((listp map)
         (if (map--plist-p map) (cons key (cons value map)) (cons (cons key value) map)))
        ((hash-table-p map) (let ((h (copy-hash-table map))) (puthash key value h) h))
        (t (error "map-insert: unsupported map type"))))
(defun map-put! (map key value &optional testfn)
  "Set KEY to VALUE in MAP in place; error if an alist must grow."
  (cond
   ((hash-table-p map) (puthash key value map) map)
   ((listp map)
    (if (map--plist-p map)
        (progn (plist-put map key value) value)
      (let ((entry (assoc key map (or testfn #'equal))))
        (if entry (progn (setcdr entry value) map)
          (error "Cannot modify map in-place: %S" map)))))
   ((arrayp map) (aset map key value) map)
   (t (error "map-put!: unsupported map type"))))
(defun map-values-apply (function map) (map-apply (lambda (_k v) (funcall function v)) map))
(defun map-keys-apply (function map) (map-apply (lambda (k _v) (funcall function k)) map))
(defun map-merge (type &rest maps)
  (let ((pairs nil))
    (dolist (m maps) (setq pairs (append pairs (map-pairs m))))
    (map--into pairs type)))
(defun map-merge-with (type function &rest maps)
  ;; Combine values for duplicate keys with FUNCTION, preserving first-seen order.
  (let ((result nil))
    (dolist (m maps)
      (map-do (lambda (k v)
                (let ((entry (assoc k result #'equal)))
                  (if entry
                      (setcdr entry (funcall function (cdr entry) v))
                    (setq result (append result (list (cons k v)))))))
              m))
    (map--into result type)))


;;; ---- Customize declaration machinery (custom.el / cus-face.el) ----
;; Faithful port of the DECLARATION half of custom.el: `defgroup', `defcustom',
;; `defface' and the `custom-declare-*' functions they expand into, storing the
;; same observable symbol properties real Emacs stores at declaration time
;; (standard-value, custom-type, custom-requests, custom-group,
;; group-documentation, face-defface-spec, ...). This lets libraries that
;; declare options (sort.el, ansi-color.el, ...) load. The Customize *UI*
;; (widget.el, cus-edit.el, `custom-set-variables' persistence) and live face
;; objects/frames are out of scope; where a facet needs them the boundary is
;; named below.
;;
;; elisprs binds dynamically this milestone, so `lexical-binding' is nil; the
;; `defcustom' macro (custom.el:249) branches on it to decide how STANDARD is
;; stashed. With nil it quotes STANDARD directly, so `standard-value' is
;; (list DEFAULT) — exactly what GNU Emacs leaves for a lexical-binding:nil file.
(defvar lexical-binding nil)
(defvar purify-flag nil)
(defvar custom-define-hook nil)          ; custom.el:38
(defvar custom-dont-initialize nil)      ; custom.el:42
(defvar custom-current-group-alist nil)  ; custom.el:47
;; C `current-load-list' (lread.c): `load' accumulates definitions on it. A
;; plain global is enough for `custom-declare-face' to push onto here.
(defvar current-load-list nil)

;; custom.el:52 — set SYMBOL to EXP only if it has no default binding yet.
(defun custom-initialize-default (symbol exp)
  (condition-case nil
      (default-toplevel-value symbol)
    (void-variable
     (set-default-toplevel-value
      symbol (eval (let ((sv (get symbol 'saved-value)))
                     (if sv (car sv) exp))
                   t)))))

;; custom.el:68
(defun custom-initialize-set (symbol exp)
  (condition-case nil
      (default-toplevel-value symbol)
    (error
     (funcall (or (get symbol 'custom-set) #'set-default-toplevel-value)
              symbol
              (eval (let ((sv (get symbol 'saved-value)))
                      (if sv (car sv) exp)))))))

;; custom.el:84 — the default `:initialize' every plain `defcustom' uses.
(defun custom-initialize-reset (symbol exp)
  ;; The `custom-check-value'/widget branch only fires for options previously
  ;; set via `setopt'; it never runs at declaration (custom-check-value is nil),
  ;; so `widget-convert'/`widget-apply' — part of the out-of-scope widget UI —
  ;; are never called here.
  (let ((value (get symbol 'custom-check-value)))
    (when value
      (let ((type (get symbol 'custom-type)))
        (when (and type
                   (boundp symbol)
                   (eq (car value) (symbol-value symbol))
                   (not (widget-apply (widget-convert type)
                                      :match (car value))))
          (warn "Value `%S' for `%s' does not match type %s"
                value symbol type)))))
  (funcall (or (get symbol 'custom-set) #'set-default-toplevel-value)
           symbol
           (condition-case nil
               (let ((def (default-toplevel-value symbol))
                     (getter (get symbol 'custom-get)))
                 (if getter (funcall getter symbol) def))
             (error
              (eval (let ((sv (get symbol 'saved-value)))
                      (if sv (car sv) exp)))))))

;; custom.el:117
(defun custom-initialize-changed (symbol exp)
  (condition-case nil
      (let ((def (default-toplevel-value symbol)))
        (funcall (or (get symbol 'custom-set) #'set-default-toplevel-value)
                 symbol
                 (let ((getter (get symbol 'custom-get)))
                   (if getter (funcall getter symbol) def))))
    (error
     (cond
      ((get symbol 'saved-value)
       (funcall (or (get symbol 'custom-set) #'set-default-toplevel-value)
                symbol
                (eval (car (get symbol 'saved-value)))))
      (t
       (set-default-toplevel-value symbol (eval exp)))))))

;; custom.el:137
(defvar custom-delayed-init-variables nil
  "List of variables whose initialization is pending until startup.
Once this list has been processed, this var is set to a non-list value.")

;; custom.el:141
(defun custom-initialize-delay (symbol value)
  "Delay initialization of SYMBOL to the next Emacs start.
This is used in files that are preloaded (or for autoloaded
variables), so that the initialization is done in the run-time
context rather than the build-time context."
  ;; Defvar it so as to mark it special, etc (bug#25770).
  (internal--define-uninitialized-variable symbol)
  ;; Until the var is actually initialized, it is kept unbound.
  (if (listp custom-delayed-init-variables)
      (push symbol custom-delayed-init-variables)
    ;; In case this is called after startup, there is no "later" to which to
    ;; delay it, so initialize it "normally" (bug#47072).
    (custom-initialize-reset symbol value)))

;; C subr (eval.c): mark SYMBOL special (dynamically bound) and record DOC,
;; without touching its value. Bare `(defvar SYMBOL)' marks special without
;; binding; the docstring lives on the `variable-documentation' property.
(defun internal--define-uninitialized-variable (symbol &optional doc)
  (eval (list 'defvar symbol))
  (when doc (put symbol 'variable-documentation doc))
  nil)

;; custom.el:479
(defun custom-current-group ()
  (cdr (assoc load-file-name custom-current-group-alist)))

;; custom.el:545
(defun custom-add-to-group (group option widget)
  (let ((members (get group 'custom-group))
        (entry (list option widget)))
    (unless (member entry members)
      (put group 'custom-group (nconc members (list entry))))))

;; custom.el:626
(defun custom-add-option (symbol option)
  (let ((options (get symbol 'custom-options)))
    (unless (member option options)
      (put symbol 'custom-options (cons option options)))))

;; custom.el:652
(defun custom-add-load (symbol load)
  (let ((loads (get symbol 'custom-loads)))
    (unless (member load loads)
      (put symbol 'custom-loads (cons (purecopy load) loads)))))

;; custom.el:659
(defun custom-autoload (symbol load &optional noset)
  "Mark SYMBOL as autoloaded custom variable and add dependency LOAD.
If NOSET is non-nil, don't bother autoloading LOAD when setting the variable."
  (put symbol 'custom-autoload (if noset 'noset t))
  (custom-add-load symbol load))

;; custom.el:638
(defun custom-add-link (symbol widget)
  (let ((links (get symbol 'custom-links)))
    (unless (member widget links)
      (put symbol 'custom-links (cons (purecopy widget) links)))))

;; custom.el:644
(defun custom-add-version (symbol version)
  (put symbol 'custom-version (purecopy version)))

;; custom.el:648
(defun custom-add-package-version (symbol version)
  (put symbol 'custom-package-version (purecopy version)))

;; custom.el:607
(defun custom-add-dependencies (symbol value)
  (unless (listp value)
    (error "Invalid custom dependency `%s'" value))
  (let* ((deps (get symbol 'custom-dependencies))
         (new-deps deps))
    (while value
      (let ((dep (car value)))
        (unless (symbolp dep)
          (error "Invalid custom dependency `%s'" dep))
        (unless (memq dep new-deps)
          (setq new-deps (cons dep new-deps)))
        (setq value (cdr value))))
    (unless (eq deps new-deps)
      (put symbol 'custom-dependencies new-deps))))

;; custom.el:585
(defun custom-handle-keyword (symbol keyword value type)
  (if purify-flag
      (setq value (purecopy value)))
  (cond ((eq keyword :group)
         (custom-add-to-group value symbol type))
        ((eq keyword :version)
         (custom-add-version symbol value))
        ((eq keyword :package-version)
         (custom-add-package-version symbol value))
        ((eq keyword :link)
         (custom-add-link symbol value))
        ((eq keyword :load)
         (custom-add-load symbol value))
        ((eq keyword :tag)
         (put symbol 'custom-tag value))
        ((eq keyword :set-after)
         (custom-add-dependencies symbol value))
        (t
         (error "Unknown keyword %s" keyword))))

;; custom.el:566
(defun custom-handle-all-keywords (symbol args type)
  (unless (memq :group args)
    (let ((cg (custom-current-group)))
      (when cg
        (custom-add-to-group cg symbol type))))
  (while args
    (let ((arg (car args)))
      (setq args (cdr args))
      (unless (symbolp arg)
        (error "Junk in args %S" args))
      (let ((keyword arg)
            (value (car args)))
        (unless args
          (error "Keyword %s is missing an argument" keyword))
        (setq args (cdr args))
        (custom-handle-keyword symbol keyword value type)))))

;; custom.el:161
(defun custom-declare-variable (symbol default doc &rest args)
  (put symbol 'standard-value (purecopy (list default)))
  (when (get symbol 'force-value)
    (put symbol 'force-value nil))
  (if (keywordp doc)
      (error "Doc string is missing"))
  (let ((initialize #'custom-initialize-reset)
        (requests nil)
        buffer-local)
    (unless (memq :group args)
      (let ((cg (custom-current-group)))
        (when cg
          (custom-add-to-group cg symbol 'custom-variable))))
    (while args
      (let ((keyword (pop args)))
        (unless (symbolp keyword)
          (error "Junk in args %S" args))
        (unless args
          (error "Keyword %s is missing an argument" keyword))
        (let ((value (pop args)))
          (cond ((eq keyword :initialize)
                 (setq initialize value))
                ((eq keyword :set)
                 (put symbol 'custom-set value))
                ((eq keyword :get)
                 (put symbol 'custom-get value))
                ((eq keyword :require)
                 (push value requests))
                ((eq keyword :risky)
                 (put symbol 'risky-local-variable value))
                ((eq keyword :safe)
                 (put symbol 'safe-local-variable value))
                ((eq keyword :local)
                 (when (memq value '(t permanent))
                   (setq buffer-local t))
                 (when (eq value 'permanent)
                   (put symbol 'permanent-local t)))
                ((eq keyword :type)
                 (put symbol 'custom-type (purecopy value)))
                ((eq keyword :options)
                 (if (get symbol 'custom-options)
                     (mapc (lambda (option)
                             (custom-add-option symbol option))
                           value)
                   (put symbol 'custom-options (copy-sequence value))))
                (t
                 (custom-handle-keyword symbol keyword value
                                        'custom-variable))))))
    (internal--define-uninitialized-variable symbol doc)
    (put symbol 'custom-requests requests)
    (unless custom-dont-initialize
      (funcall initialize symbol default)
      (let ((theme (caar (get symbol 'theme-value))))
        (when (and theme (not (eq theme 'user)) (get symbol 'saved-value))
          (put symbol 'saved-value nil))))
    (when buffer-local
      (make-variable-buffer-local symbol)))
  (run-hooks 'custom-define-hook)
  symbol)

;; custom.el:482
(defun custom-declare-group (symbol members doc &rest args)
  (while members
    (apply #'custom-add-to-group symbol (car members))
    (setq members (cdr members)))
  (when doc
    (put symbol 'group-documentation (purecopy doc)))
  (while args
    (let ((arg (car args)))
      (setq args (cdr args))
      (unless (symbolp arg)
        (error "Junk in args %S" args))
      (let ((keyword arg)
            (value (car args)))
        (unless args
          (error "Keyword %s is missing an argument" keyword))
        (setq args (cdr args))
        (cond ((eq keyword :prefix)
               (put symbol 'custom-prefix (purecopy value)))
              (t
               (custom-handle-keyword symbol keyword value
                                      'custom-group))))))
  (let ((elt (assoc load-file-name custom-current-group-alist)))
    (if elt (setcdr elt symbol)
      (push (cons load-file-name symbol) custom-current-group-alist)))
  (run-hooks 'custom-define-hook)
  symbol)

;; C subr: t if OBJECT can serve as a documentation string — a string, an
;; integer DOC-file offset, or a (string . offset) cons.
(defun documentation-stringp (object)
  (or (stringp object)
      (integerp object)
      (and (consp object) (stringp (car object)) (integerp (cdr object)))))

;; faces.el:662
(defun set-face-documentation (face string)
  (put face 'face-documentation (purecopy string)))

;; faces.el:1685 — declaration-scope port: store the face spec under the
;; requested property (this is the observable half). BOUNDARY: real
;; `face-spec-set' then calls `make-empty-face' and recalculates the face on
;; every frame (`face-spec-recalc'). elisprs has no display subsystem — no face
;; objects, no frames — so those are omitted; no face object is created.
(defun face-spec-set (face spec &optional spec-type)
  (if (get face 'face-alias)
      (setq face (get face 'face-alias)))
  (unless spec-type
    (setq spec-type 'face-override-spec))
  (if (memq spec-type '(face-defface-spec face-override-spec
                        customized-face saved-face))
      (put face spec-type spec))
  (if (memq spec-type '(reset saved-face))
      (put face 'customized-face nil))
  (if (memq spec-type '(customized-face saved-face reset))
      (put face 'face-override-spec nil))
  (unless (eq face 'face-override-spec)
    (put face 'face-modified nil)))

;; cus-face.el:32
(defun custom-declare-face (face spec doc &rest args)
  (when (and doc
             (not (documentation-stringp doc)))
    (error "Invalid (or missing) doc string %S" doc))
  (unless (get face 'face-defface-spec)
    (face-spec-set face (purecopy spec) 'face-defface-spec)
    (push (cons 'defface face) current-load-list)
    (when doc
      (set-face-documentation face (purecopy doc)))
    (custom-handle-all-keywords face args 'custom-face)
    (run-hooks 'custom-define-hook))
  face)

;; custom.el:512 — no backquote here, matching the upstream bootstrap note.
(defmacro defgroup (symbol members doc &rest args)
  (nconc (list 'custom-declare-group (list 'quote symbol) members doc) args))

;; custom.el:249
(defmacro defcustom (symbol standard doc &rest args)
  `(custom-declare-variable
    ',symbol
    ,(if lexical-binding
         ``(funcall #',(lambda () "" ,standard))
       `',standard)
    ,doc
    ,@args))

;; custom.el:409
(defmacro defface (face spec doc &rest args)
  (nconc (list 'custom-declare-face (list 'quote face) spec doc) args))

;;; ---- password-prompt recognition data (international/mule-conf.el) ----
;; mule-conf.el is preloaded in Emacs, so these are always bound. Both are
;; fixed, build-independent i18n data lists — captured value-for-value from GNU
;; Emacs 30.2 mule-conf.el:1681/1739. Used by comint.el/shell.el and friends to
;; build `comint-password-prompt-regexp'. Placed after `defcustom' is defined.
;; mule-conf.el:1681
(defcustom password-word-equivalents
  '("password" "passcode" "passphrase" "pass phrase" "pin"
    "decryption key" "encryption key" ; From ccrypt.
    ; These are sorted according to the GNU en_US locale.
    "암호"		; ko
    "パスワード"	; ja
    "ପ୍ରବେଶ ସଙ୍କେତ"	; or
    "ពាក្យសម្ងាត់"		; km
    "adgangskode"	; da
    "contraseña"	; es
    "contrasenya"	; ca
    "geslo"		; sl
    "hasło"		; pl
    "heslo"		; cs, sk
    "iphasiwedi"	; zu
    "jelszó"		; hu
    "lösenord"		; sv
    "lozinka"		; hr, sr
    "mật khẩu"		; vi
    "mot de passe"	; fr
    "parola"		; tr
    "pasahitza"		; eu
    "passord"		; nb
    "passwort"		; de
    "pasvorto"		; eo
    "salasana"		; fi
    "senha"		; pt
    "slaptažodis"	; lt
    "wachtwoord"	; nl
    "كلمة السر"		; ar
    "ססמה"		; he
    "лозинка"		; sr
    "пароль"		; kk, ru, uk
    "गुप्तशब्द"		; mr
    "शब्दकूट"		; hi
    "પાસવર્ડ"		; gu
    "సంకేతపదము"		; te
    "ਪਾਸਵਰਡ"		; pa
    "ಗುಪ್ತಪದ"		; kn
    "கடவுச்சொல்"		; ta
    "അടയാളവാക്ക്"		; ml
    "গুপ্তশব্দ"		; as
    "পাসওয়ার্ড"		; bn_IN
    "රහස්පදය"		; si
    "密码"		; zh_CN
    "密碼"		; zh_TW
    )
  "List of words equivalent to \"password\".
This is used by Shell mode and other parts of Emacs to recognize
password prompts, including prompts in languages other than
English.  Different case choices should not be assumed to be
included; callers should bind `case-fold-search' to t."
  :type '(repeat string)
  :version "27.1"
  :group 'processes)
;; mule-conf.el:1739
(defcustom password-colon-equivalents
  '(?: ; ?\N{COLON}
    ?： ; ?\N{FULLWIDTH COLON}
    ?﹕ ; ?\N{SMALL COLON}
    ?︓ ; ?\N{PRESENTATION FORM FOR VERTICAL COLON}
    ?៖ ; ?\N{KHMER SIGN CAMNUC PII KUUH}
    )
  "List of characters equivalent to trailing colon in \"password\" prompts."
  :type '(repeat character)
  :version "30.1"
  :group 'processes)

;;; ---- files.el pure-data regexps ----
;; files.el:1180 `locate-dominating-stop-dir-regexp': a plain `defvar' whose
;; value is a fixed, build-independent regexp string (purecopy'd in the C tree).
;; Read by `locate-dominating-file' to decide which directory names terminate an
;; upward search. Ported value-for-value from GNU Emacs 30.2 files.el.
(defvar locate-dominating-stop-dir-regexp
  (purecopy "\\`\\(?:[\\/][\\/][^\\/]+[\\/]\\|/\\(?:net\\|afs\\|\\.\\.\\.\\)/\\)\\'")
  "Regexp of directory names that stop the search in `locate-dominating-file'.
Any directory whose name matches this regexp will be treated like
a kind of root directory by `locate-dominating-file', which will stop its
search when it bumps into it.
The default regexp prevents fruitless and time-consuming attempts to find
special files in directories in which file names are interpreted as host names,
or mount points potentially requiring authentication as a different user.")
;; files.el:1722 `mounted-file-systems': a `defcustom' regexp. The standard
;; value is platform-conditional; the `if' is kept intact so the value is
;; correct on any `system-type' (Windows/Cygwin get "^//[^/]+/", the rest get
;; the regexp-opt-precomputed alternation). Ported value-for-value; oracle on
;; darwin yields the else branch. `:require 'regexp-opt' is honored by
;; `custom-declare-variable' (pushed onto its requests list).
(defcustom mounted-file-systems
  (if (memq system-type '(windows-nt cygwin))
      "^//[^/]+/"
    "^\\(?:/\\(?:afs/\\|m\\(?:edia/\\|nt\\)\\|\\(?:ne\\|tmp_mn\\)t/\\)\\)")
  "File systems that ought to be mounted."
  :group 'files
  :version "26.1"
  :require 'regexp-opt
  :type 'regexp)

;;; ---- per-user Emacs file location (files.el / startup.el) ----
;; startup.el:597 sets `user-emacs-directory' at startup; the defvar itself is
;; nil ("The value does not matter since Emacs sets this at startup").  elisprs
;; does not run the C/startup init path, so we seed the documented runtime value
;; "~/.emacs.d/" -- oracle-confirmed against emacs -Q --batch (30.2).
;; startup.el
(defvar init-file-user nil
  "Identity of user whose init file is or was read.")
;; startup.el (effective runtime value; Emacs sets this at startup)
(defvar user-emacs-directory "~/.emacs.d/"
  "Directory beneath which additional per-user Emacs-specific files are placed.")
;; emacs.c C variable (nil outside the dump/pdump build path)
(defvar dump-mode nil
  "Non-nil means Emacs is dumping or bootstrapping.")
;; files.el
(defcustom user-emacs-directory-warning t
  "Non-nil means warn if unable to access or create `user-emacs-directory'."
  :type 'boolean
  :group 'initialization
  :version "24.4")

;; files.el: faithful port of `locate-user-emacs-file'.
(defun locate-user-emacs-file (new-name &optional old-name)
  "Return an absolute per-user Emacs-specific file name.
If NEW-NAME exists in `user-emacs-directory', return it.
Else if OLD-NAME is non-nil and ~/OLD-NAME exists, return ~/OLD-NAME.
Else return NEW-NAME in `user-emacs-directory', creating the
directory if it does not exist."
  (convert-standard-filename
   (let* ((home (concat "~" (or init-file-user "")))
	  (at-home (and old-name (expand-file-name old-name home)))
          (bestname (abbreviate-file-name
                     (expand-file-name new-name user-emacs-directory))))
     (if (and at-home (not (file-readable-p bestname))
              (file-readable-p at-home))
	 at-home
       ;; Make sure `user-emacs-directory' exists,
       ;; unless we're in batch mode or dumping Emacs.
       (or noninteractive
           dump-mode
	   (let (errtype)
	     (if (file-directory-p user-emacs-directory)
		 (or (file-accessible-directory-p user-emacs-directory)
		     (setq errtype "access"))
               ;; We don't want to create HOME if it doesn't exist.
               (if (and (not (file-exists-p "~"))
                        (string-prefix-p
                         (expand-file-name "~")
                         (expand-file-name user-emacs-directory)))
                   (setq errtype "create")
                 ;; Create `user-emacs-directory'.
	         (with-file-modes ?\700
		   (condition-case nil
		       (make-directory user-emacs-directory t)
		     (error (setq errtype "create"))))))
	     (when (and errtype
			user-emacs-directory-warning
			(not (get 'user-emacs-directory-warning 'this-session)))
	       ;; Warn only once per Emacs session.
	       (put 'user-emacs-directory-warning 'this-session t)
	       (display-warning 'initialization
				(format "\
Unable to %s `user-emacs-directory' (%s).
Any data that would normally be written there may be lost!
If you never want to see this message again,
customize the variable `user-emacs-directory-warning'."
					errtype user-emacs-directory)))))
       bestname))))

;;; ---- keymaps (data subsystem: keymap.c primitives + keymap.el string API) ----
;; A keymap is a list whose car is the symbol `keymap'.  Bindings follow as
;; (EVENT . DEFINITION) conses; a bare `keymap' element in the tail begins the
;; PARENT keymap (shared structure).  These are faithful ports of the DOCUMENTED
;; behavior of the C primitives in keymap.c, verified value-for-value against the
;; Emacs 30.2 binary.  make-keymap (full char-table keymap) is NOT implemented:
;; it needs a char-table Value type elisprs lacks -- see report.  command-remapping
;; returns nil (correct with no active global/local remap keymaps).

;; With no buffer-local/global remap keymaps in effect, no command is remapped.
(defun command-remapping (_command &optional _position _keymaps) nil)

(defun make-sparse-keymap (&optional string)
  "Construct and return a new sparse keymap.
Optional STRING is a menu name for the keymap."
  (if string (list 'keymap string) (list 'keymap)))

(defun keymapp (object)
  "Return t if OBJECT is a keymap.
A keymap is a list (keymap . ALIST), or a symbol whose function
definition is itself a keymap."
  (if (symbolp object)
      (and object (keymapp (symbol-function object)))
    (and (consp object) (eq (car object) 'keymap))))

;; Resolve a keymap that may be a symbol standing for a keymap (a prefix command).
(defun keymap--get (object)
  (if (symbolp object) (symbol-function object) object))

(defun make-composed-keymap (maps &optional parent)
  "Construct a new keymap composed of MAPS and inheriting from PARENT.
MAPS can be a single keymap or a list of keymaps.  PARENT, if non-nil,
should be a keymap."
  (cons 'keymap (append (if (keymapp maps) (list maps) maps) parent)))

;; Return the (EVENT . DEF) cons in KM's own bindings (before the parent), or nil.
(defun keymap--own-binding (km event)
  (let ((tail (cdr km)) (res nil))
    (while (and (consp tail) (not res))
      (let ((el (car tail)))
        (cond ((eq el 'keymap) (setq tail nil))
              ((and (consp el) (equal (car el) event)) (setq res el))
              (t (setq tail (cdr tail))))))
    res))

;; Replace or prepend a binding for EVENT in KM's own bindings.
(defun keymap--set-binding (km event def)
  (let ((cell (keymap--own-binding km event)))
    (if cell
        (setcdr cell def)
      (setcdr km (cons (cons event def) (cdr km))))))

;; Delete EVENT's own binding from KM.
(defun keymap--remove-binding (km event)
  (let ((cell (keymap--own-binding km event)))
    (when cell (setcdr km (delq cell (cdr km))))))

;; Expand meta-modified integer events (bit 2^27) into ESC (27) + base char,
;; matching how keymap.c stores/looks up meta bindings.  Symbol events (e.g.
;; `M-left') are stored verbatim and are not expanded.
(defun keymap--expand-meta (events)
  (let ((res nil))
    (dolist (e events)
      (if (and (integerp e) (/= 0 (logand e (ash 1 27))))
          (progn (push 27 res) (push (logand e (lognot (ash 1 27))) res))
        (push e res)))
    (nreverse res)))

(defun define-key (keymap key def &optional remove)
  "In KEYMAP, define key sequence KEY as DEF.
KEY is a string or vector of events.  DEF is anything that can be a
key definition (a command symbol, a keymap for a prefix key, nil, etc.).
If optional REMOVE is non-nil, remove the binding instead.
Returns DEF."
  (let ((events (keymap--expand-meta (append key nil)))
        (km keymap))
    (while (cdr events)
      (let ((sub (keymap--get (cdr (or (keymap--own-binding km (car events))
                                       (cons nil nil))))))
        (if (keymapp sub)
            (setq km (keymap--get sub))
          (let ((new (make-sparse-keymap)))
            (keymap--set-binding km (car events) new)
            (setq km new))))
      (setq events (cdr events)))
    (if remove
        (keymap--remove-binding km (car events))
      (keymap--set-binding km (car events) def))
    def))

;; Look up EVENT in KM's own bindings, then (transparently) its parent chain.
;; ACCEPT-DEFAULT recognizes a (t . DEF) default binding.
(defun lookup-key--event (km event accept-default)
  (let ((tail (cdr km)) (res nil) (deflt nil) (done nil))
    (while (and (consp tail) (not done))
      (let ((el (car tail)))
        (cond
         ((eq el 'keymap)
          (setq res (lookup-key--event tail event accept-default) done t))
         ((and (consp el) (eq (car el) 'keymap))
          (let ((r (lookup-key--event el event accept-default)))
            (when r (setq res r done t))))
         ((and (consp el) (equal (car el) event))
          (setq res (cdr el) done t))
         ((and (consp el) (eq (car el) t))
          (setq deflt (cdr el)))))
      (unless done (setq tail (cdr tail))))
    (or res (and accept-default deflt))))

(defun lookup-key (keymap key &optional accept-default)
  "Look up key sequence KEY in KEYMAP; return the definition.
KEY is a string or vector.  Returns nil if undefined.  If KEY is longer
than needed to reach a non-prefix binding, returns the number of events
at the front of KEY that were used.  ACCEPT-DEFAULT recognizes default
(t) bindings."
  (let ((events (keymap--expand-meta (append key nil)))
        (km (keymap--get keymap)) (i 0) (res nil) (done nil))
    (if (null events)
        keymap
      (while (and events (not done))
        (setq res (lookup-key--event km (car events) accept-default))
        (setq events (cdr events))
        (setq i (1+ i))
        (cond
         ((null res) (setq done t))
         ((null events) (setq done t))
         ((keymapp res) (setq km (keymap--get res)))
         (t (setq res i done t))))
      res)))

(defun keymap-parent (keymap)
  "Return the parent keymap of KEYMAP, or nil if it has none."
  (let ((tail (cdr keymap)) (res nil))
    (while (and (consp tail) (not res))
      (if (eq (car tail) 'keymap)
          (setq res tail)
        (setq tail (cdr tail))))
    res))

(defun set-keymap-parent (keymap parent)
  "Modify KEYMAP to set its parent keymap to PARENT.  Return PARENT."
  (let ((prev keymap) (tail (cdr keymap)))
    (while (and (consp tail) (not (eq (car tail) 'keymap)))
      (setq prev tail tail (cdr tail)))
    (setcdr prev parent)
    parent))

(defun define-prefix-command (command &optional mapvar _name)
  "Define COMMAND as a prefix command with a new sparse keymap.
Set COMMAND's function cell to the keymap, and its value cell (or MAPVAR
if given and not t) to the same keymap.  Return COMMAND."
  (let ((map (make-sparse-keymap)))
    (fset command map)
    (if (and mapvar (not (eq mapvar t)))
        (set mapvar map)
      (set command map))
    command))

(defun suppress-keymap (map &optional nodigits)
  "Make MAP override all normally self-inserting keys to be undefined.
Normally, as an exception, digits and minus-sign are set to make prefix
args, but optional second arg NODIGITS non-nil treats them like other chars."
  (define-key map [remap self-insert-command] #'undefined)
  (or nodigits
      (let (loop)
        (define-key map "-" #'negative-argument)
        (setq loop ?0)
        (while (<= loop ?9)
          (define-key map (char-to-string loop) #'digit-argument)
          (setq loop (1+ loop))))))

;; ---- keymap.el: the string-based key API (faithful ports) ----

(defun key-parse (keys)
  "Convert KEYS to the internal Emacs key representation.
KEYS should be a string describing a key sequence in the format
returned by \\[describe-key] (`describe-key')."
  (save-match-data
    (let ((case-fold-search nil)
          (len (length keys))
          (pos 0)
          (res []))
      (while (and (< pos len)
                  (string-match "[^ \t\n\f]+" keys pos))
        (let* ((word-beg (match-beginning 0))
               (word-end (match-end 0))
               (word (substring keys word-beg len))
               (times 1)
               key)
          (if (string-match "\\`<[^ <>\t\n\f][^>\t\n\f]*>" word)
              (setq word (match-string 0 word)
                    pos (+ word-beg (match-end 0)))
            (setq word (substring keys word-beg word-end)
                  pos word-end))
          (when (string-match "\\([0-9]+\\)\\*." word)
            (setq times (string-to-number (substring word 0 (match-end 1))))
            (setq word (substring word (1+ (match-end 1)))))
          (cond ((string-match "^<<.+>>$" word)
                 (setq key (vconcat (if (eq (key-binding [?\M-x])
                                            'execute-extended-command)
                                        [?\M-x]
                                      (or (car (where-is-internal
                                                'execute-extended-command))
                                          [?\M-x]))
                                    (substring word 2 -2) "\r")))
                ((and (string-match "^\\(\\([ACHMsS]-\\)*\\)<\\(.+\\)>$" word)
                      (progn
                        (setq word (concat (match-string 1 word)
                                           (match-string 3 word)))
                        (not (string-match
                              "\\<\\(NUL\\|RET\\|LFD\\|TAB\\|ESC\\|SPC\\|DEL\\)$"
                              word))))
                 (setq key (list (intern word))))
                ((or (equal word "REM") (string-match "^;;" word))
                 (setq pos (string-match "$" keys pos)))
                (t
                 (let ((orig-word word) (prefix 0) (bits 0))
                   (while (string-match "^[ACHMsS]-." word)
                     (setq bits (+ bits
                                   (cdr
                                    (assq (aref word 0)
                                          '((?A . ?\A-\0) (?C . ?\C-\0)
                                            (?H . ?\H-\0) (?M . ?\M-\0)
                                            (?s . ?\s-\0) (?S . ?\S-\0))))))
                     (setq prefix (+ prefix 2))
                     (setq word (substring word 2)))
                   (when (string-match "^\\^.$" word)
                     (setq bits (+ bits ?\C-\0))
                     (setq prefix (1+ prefix))
                     (setq word (substring word 1)))
                   (let ((found (assoc word '(("NUL" . "\0") ("RET" . "\r")
                                              ("LFD" . "\n") ("TAB" . "\t")
                                              ("ESC" . "\e") ("SPC" . " ")
                                              ("DEL" . "\177")))))
                     (when found (setq word (cdr found))))
                   (when (string-match "^\\\\[0-7]+$" word)
                     (let ((n 0))
                       (dolist (ch (cdr (string-to-list word)))
                         (setq n (+ (* n 8) ch -48)))
                       (setq word (vector n))))
                   (cond ((= bits 0)
                          (setq key word))
                         ((and (= bits ?\M-\0) (stringp word)
                               (string-match "^-?[0-9]+$" word))
                          (setq key (mapcar (lambda (x) (+ x bits))
                                            (append word nil))))
                         ((/= (length word) 1)
                          (error "%s must prefix a single character, not %s"
                                 (substring orig-word 0 prefix) word))
                         ((and (/= (logand bits ?\C-\0) 0) (stringp word)
                               (string-match "[@-_a-z]" word))
                          (setq key (list (+ bits (- ?\C-\0)
                                             (logand (aref word 0) 31)))))
                         (t
                          (setq key (list (+ bits (aref word 0)))))))))
          (when key
            (dolist (_ (number-sequence 1 times))
              (setq res (vconcat res key))))))
      res)))

(defun key-valid-p (keys)
  "Return non-nil if KEYS, a string, is a valid key sequence.
KEYS should be a string consisting of one or more key strokes, with a
single space character separating one key stroke from another."
  (let ((case-fold-search nil))
    (and
     (stringp keys)
     (string-match-p "\\`[^ ]+\\( [^ ]+\\)*\\'" keys)
     (save-match-data
       (catch 'exit
         (let ((prefixes
                "\\(A-\\)?\\(C-\\)?\\(H-\\)?\\(M-\\)?\\(S-\\)?\\(s-\\)?"))
           (dolist (key (split-string keys " "))
             (when (string-match (concat "\\`" prefixes) key)
               (setq key (substring key (match-end 0))))
             (unless (or (and (= (length key) 1)
                              (not (< (aref key 0) ?\s))
                              (or (multibyte-string-p key)
                                  (not (<= 127 (aref key 0) 255))))
                         (and (string-match-p "\\`<[-_A-Za-z0-9]+>\\'" key)
                              (= (progn
                                   (string-match
                                    (concat "\\`<" prefixes) key)
                                   (match-end 0))
                                 1))
                         (string-match-p
                          "\\`\\(NUL\\|RET\\|TAB\\|LFD\\|ESC\\|SPC\\|DEL\\)\\'"
                          key))
               (throw 'exit nil)))
           t))))))

(defun keymap--check (key)
  "Signal an error if KEY doesn't have a valid syntax."
  (unless (key-valid-p key)
    (error "%S is not a valid key definition; see `key-valid-p'" key)))

(defun keymap-set (keymap key definition)
  "Set KEY to DEFINITION in KEYMAP.
KEY is a string that satisfies `key-valid-p'."
  (keymap--check key)
  (when (stringp definition)
    (keymap--check definition)
    (setq definition (key-parse definition)))
  (define-key keymap (key-parse key) definition))

(defun keymap-unset (keymap key &optional remove)
  "Remove KEY from KEYMAP.
If REMOVE is non-nil, remove the binding instead of setting it to nil."
  (keymap--check key)
  (define-key keymap (key-parse key) nil remove))

(defun keymap-lookup (keymap key &optional accept-default no-remap position)
  "Return the binding for command KEY in KEYMAP.
KEY is a string that satisfies `key-valid-p'.  KEYMAP must be non-nil
here: looking up in the current buffer-local/global keymaps (KEYMAP nil)
needs live buffer state that elisprs does not provide."
  (keymap--check key)
  (when (and keymap position)
    (error "Can't pass in both keymap and position"))
  (if keymap
      (let ((value (lookup-key keymap (key-parse key) accept-default)))
        (if (and (not no-remap)
                 (symbolp value))
            (or (command-remapping value) value)
          value))
    (error "keymap-lookup with no keymap needs buffer-local current keymaps (unsupported)")))

(defun define-keymap (&rest definitions)
  "Create a new keymap and define KEY/DEFINITION pairs as key bindings.
Return the new keymap.  Options may be given as keywords before the
pairs: :full :suppress :parent :keymap :name :prefix."
  (let (full suppress parent name prefix keymap)
    (while (and definitions
                (keywordp (car definitions))
                (not (eq (car definitions) :menu)))
      (let ((keyword (pop definitions)))
        (unless definitions
          (error "Missing keyword value for %s" keyword))
        (let ((value (pop definitions)))
          (pcase keyword
            (:full (setq full value))
            (:keymap (setq keymap value))
            (:parent (setq parent value))
            (:suppress (setq suppress value))
            (:name (setq name value))
            (:prefix (setq prefix value))
            (_ (error "Invalid keyword: %s" keyword))))))

    (when (and prefix
               (or full parent suppress keymap))
      (error "A prefix keymap can't be defined with :full/:parent/:suppress/:keymap keywords"))

    (when (and keymap full)
      (error "Invalid combination: :keymap with :full"))

    (let ((keymap (cond
                   (keymap keymap)
                   (prefix (define-prefix-command prefix nil name))
                   (full (make-keymap name))
                   (t (make-sparse-keymap name))))
          seen-keys)
      (when suppress
        (suppress-keymap keymap (eq suppress 'nodigits)))
      (when parent
        (set-keymap-parent keymap parent))

      (while definitions
        (let ((key (pop definitions)))
          (unless definitions
            (error "Uneven number of key/definition pairs"))
          (let ((def (pop definitions)))
            (if (eq key :menu)
                (easy-menu-define nil keymap "" def)
              (when (member key seen-keys)
                (message "Duplicate definition for key: %S %s" key keymap))
              (push key seen-keys)
              (keymap-set keymap key def)))))
      keymap)))

(defmacro defvar-keymap (variable-name &rest defs)
  "Define VARIABLE-NAME as a variable with a keymap definition.
See `define-keymap' for an explanation of the keywords and KEY/DEFINITION.
Also accepts a `:doc' keyword for the variable documentation string, and
a `:repeat' keyword controlling `repeat-mode' behavior."
  (declare (indent 1))
  (let ((opts nil)
        doc repeat props)
    (while (and defs
                (keywordp (car defs))
                (not (eq (car defs) :menu)))
      (let ((keyword (pop defs)))
        (unless defs
          (error "Uneven number of keywords"))
        (cond
         ((eq keyword :doc) (setq doc (pop defs)))
         ((eq keyword :repeat) (setq repeat (pop defs)))
         (t (push keyword opts)
            (push (pop defs) opts)))))
    (unless (zerop (% (length defs) 2))
      (error "Uneven number of key/definition pairs: %s" defs))

    (let ((defs defs)
          key seen-keys)
      (while defs
        (setq key (pop defs))
        (pop defs)
        (when (not (eq key :menu))
          (if (member key seen-keys)
              (error "Duplicate definition for key '%s' in keymap '%s'"
                     key variable-name)
            (push key seen-keys)))))

    (when repeat
      (let ((defs defs)
            def)
        (dolist (def (plist-get repeat :enter))
          (push (list 'put (list 'quote def) ''repeat-map (list 'quote variable-name)) props))
        (while defs
          (pop defs)
          (setq def (pop defs))
          (when (and (memq (car def) '(function quote))
                     (not (memq (cadr def) (plist-get repeat :exit))))
            (push (list 'put def ''repeat-map (list 'quote variable-name)) props)))
        (dolist (def (plist-get repeat :hints))
          (push (list 'put (list 'quote (car def)) ''repeat-hint (list 'quote (cdr def))) props))))

    (let ((defvar-form
           (append (list 'defvar variable-name
                         (append (list 'define-keymap) (nreverse opts) defs))
                   (and doc (list doc)))))
      (if props
          (append (list 'progn defvar-form) (nreverse props))
        defvar-form))))

;; button-buffer-map (button.el) and special-mode-map (simple.el) are pure
;; keymap data that tabulated-list-mode-map inherits from via :parent.  Ported
;; from their upstream defvar-keymap forms.
(defvar-keymap button-buffer-map
  :doc "Keymap useful for buffers containing buttons.
Mode-specific keymaps may want to use this as their parent keymap."
  "TAB" #'forward-button
  "ESC TAB" #'backward-button
  "<backtab>" #'backward-button)

;;; ---- button types (button.el) ----
;; Faithful port of button.el's button-type system (define-button-type and the
;; button-type-{put,get,subtype-p} accessors).  Button types hold default
;; properties for buttons; each type NAME stores them on a separate uninterned
;; `-button' symbol (its `button-category-symbol'), so `category' text/overlay
;; properties can point at it without name clashes.  Overlay/text-property
;; button placement (make-button, insert-button, push-button navigation) is not
;; modeled here — only the type registry, which is what init files exercise at
;; load time.  Value-for-value against Emacs 30.2 button.el.

(defface button '((t :inherit link))
  "Default face used for buttons."
  :group 'basic-faces)

(defvar-keymap button-map
  :doc "Keymap used by buttons."
  :parent button-buffer-map
  "RET" #'push-button
  "<mouse-2>" #'push-button
  "<follow-link>" 'mouse-face
  ;; FIXME: You'd think that for keymaps coming from text-properties on the
  ;; mode-line or header-line, the `mode-line' or `header-line' prefix
  ;; shouldn't be necessary!
  "<mode-line> <mouse-2>" #'push-button
  "<header-line> <mouse-2>" #'push-button
  ;; `push-button' will automatically dispatch to
  ;; `touch-screen-track-tap'.
  "<mode-line> <touchscreen-down>" #'push-button
  "<header-line> <touchscreen-down>" #'push-button
  "<touchscreen-down>" #'push-button)

;; `button-mode' (the TAB-navigation minor mode) is intentionally omitted: it is
;; button UI, not part of the type registry, and the prelude already omits it
;; while keeping button-buffer-map.  The type machinery below is self-contained.

;; Default properties for buttons.
(put 'default-button 'face 'button)
(put 'default-button 'mouse-face 'highlight)
(put 'default-button 'keymap button-map)
(put 'default-button 'type 'button)
;; `action' may be either a function to call, or a marker to go to.
(put 'default-button 'action #'ignore)
(put 'default-button 'help-echo (purecopy "mouse-2, RET: Push this button"))
;; Make overlay buttons go away if their underlying text is deleted.
(put 'default-button 'evaporate t)
;; Prevent insertions adjacent to text-property buttons from
;; inheriting their properties.
(put 'default-button 'rear-nonsticky t)

;; A `category-symbol' property for the default button type.
(put 'button 'button-category-symbol 'default-button)

;; [this is an internal function]
(defsubst button-category-symbol (type)
  "Return the symbol used by `button-type' TYPE to store properties.
Buttons inherit them by setting their `category' property to that symbol."
  (or (get type 'button-category-symbol)
      (error "Unknown button type `%s'" type)))

(defun define-button-type (name &rest properties)
  "Define a `button type' called NAME (a symbol).
The remaining PROPERTIES arguments form a plist of PROPERTY VALUE
pairs, specifying properties to use as defaults for buttons with
this type (a button's type may be set by giving it a `type'
property when creating the button, using the :type keyword
argument).

In addition, the keyword argument :supertype may be used to specify a
`button-type' from which NAME inherits its default property values
\(however, the inheritance happens only when NAME is defined; subsequent
changes to a supertype are not reflected in its subtypes)."
  (declare (indent defun))
  (let ((catsym (make-symbol (concat (symbol-name name) "-button")))
	(super-catsym
	 (button-category-symbol
	  (or (plist-get properties 'supertype)
	      (plist-get properties :supertype)
	      'button))))
    ;; Provide a link so that it's easy to find the real symbol.
    (put name 'button-category-symbol catsym)
    ;; Initialize NAME's properties using the global defaults.
    (let ((default-props (symbol-plist super-catsym)))
      (while default-props
	(put catsym (pop default-props) (pop default-props))))
    ;; Add NAME as the `type' property, which will then be returned as
    ;; the type property of individual buttons.
    (put catsym 'type name)
    ;; Add the properties in PROPERTIES to the real symbol.
    (while properties
      (let ((prop (pop properties)))
	(when (eq prop :supertype)
	  (setq prop 'supertype))
	(put catsym prop (pop properties))))
    ;; Make sure there's a `supertype' property.
    (unless (get catsym 'supertype)
      (put catsym 'supertype 'button))
    name))

(defun button-type-put (type prop val)
  "Set the `button-type' TYPE's PROP property to VAL."
  (put (button-category-symbol type) prop val))

(defun button-type-get (type prop)
  "Get the property of `button-type' TYPE named PROP."
  (get (button-category-symbol type) prop))

(defun button-type-subtype-p (type supertype)
  "Return non-nil if `button-type' TYPE is a subtype of SUPERTYPE."
  (or (eq type supertype)
      (and type
	   (button-type-subtype-p (button-type-get type 'supertype)
				  supertype))))

;; Mark `button' as provided so `(require 'button)' in init files no-ops onto
;; the preloaded type machinery instead of trying to open a file (button.el:672).
(provide 'button)

;;; ---- widget types (widget.el) ----
;; Faithful port of widget.el, the bootstrap half of the widget system that only
;; defines new widget types (everything else is autoloaded from wid-edit.el).
;; `define-widget' registers a type by storing (CLASS . ARGS) on the NAME
;; symbol's `widget-type' property and the doc string on `widget-documentation';
;; that registry is what init files touch at load time. The widget UI itself
;; (widget-create/widget-convert/widget-apply, the wid-edit.el layer) is not
;; modeled here. Value-for-value against Emacs 30.2 widget.el.

;; `define-widget-keywords' is a dummy kept only so external libraries that still
;; call it don't error; it expands to nil (obsolete since 27.1).
(defmacro define-widget-keywords (&rest _keys)
  (declare (obsolete nil "27.1") (indent defun))
  nil)

(defun define-widget (name class doc &rest args)
  "Define a new widget type named NAME from CLASS.

NAME and CLASS should both be symbols, CLASS should be one of the
existing widget types, or nil to create the widget from scratch.

After the new widget has been defined, the following two calls will
create identical widgets:

* (widget-create NAME)

* (apply #\\='widget-create CLASS ARGS)

The third argument DOC is a documentation string for the widget."
  (declare (doc-string 3) (indent defun))
  ;;
  (unless (or (null doc) (stringp doc))
    (error "Widget documentation must be nil or a string"))
  (put name 'widget-type (cons class args))
  (put name 'widget-documentation (purecopy doc))
  name)

(define-obsolete-function-alias 'widget-plist-member #'plist-member "26.1")

;; Mark `widget' as provided so `(require 'widget)' no-ops onto the preloaded
;; type registry instead of trying to open a file (widget.el:139).
(provide 'widget)

(defvar-keymap special-mode-map
  :suppress t
  "q" #'quit-window
  "SPC" #'scroll-up-command
  "S-SPC" #'scroll-down-command
  "DEL" #'scroll-down-command
  "?" #'describe-mode
  "h" #'describe-mode
  ">" #'end-of-buffer
  "<" #'beginning-of-buffer
  "g" #'revert-buffer)

;;; ---- major/minor mode machinery (derived.el, easy-mmode.el, subr.el) ----
;; Real text editing (point/insert, multiple named live buffers, narrowing,
;; marker-based save-excursion, first-class markers, and string/buffer text
;; properties) is modeled by the text-editing subsystem (builtins.rs). Overlays
;; and redisplay are not. Syntax tables and abbrev tables are separate subsystems:
;; the constructors below
;; are placeholders sufficient for `define-derived-mode' to expand and load; they
;; do not model syntax/abbrev semantics.

;; `interactive' is only meaningful as the first body form of a command; when a
;; command is called from Lisp (as in batch), it is a no-op. Modeling it as a
;; macro that expands to nil drops its (unevaluated) interactive spec at compile
;; time, matching the non-interactive runtime behavior.
(defmacro interactive (&rest _) nil)

;; `current-buffer'/`set-buffer'/`get-buffer-create' and the rest of the buffer
;; registry are C-level primitives (see builtins.rs). Buffers are not associated
;; with files in this model, so `buffer-file-name' is always nil.
(defun buffer-file-name (&optional _buffer) nil)

;;; ---- char-tables (public make-char-table over the make-char-table--new subr) ----
;; make-char-table reads SUBTYPE's `char-table-extra-slots' property (0..10) to
;; size the extra slots, then calls the low-level allocator.  Faithful to
;; chartab.c `Fmake_char_table'.
(defun make-char-table (subtype &optional init)
  "Return a newly created char-table, with purpose SUBTYPE.
Each element is initialized to INIT, which defaults to nil.

The property `char-table-extra-slots' of SUBTYPE controls the number of
extra slots to reserve in this char-table.  This slot number is an
integer between 0 and 10, or nil, meaning 0."
  (let ((n (get subtype 'char-table-extra-slots)))
    (setq n (cond ((null n) 0)
                  ((and (integerp n) (>= n 0) (<= n 10)) n)
                  (t (error "Invalid number of extra slots"))))
    (make-char-table--new subtype init n)))

;;; ---- syntax tables (syntax.c documented behavior, over char-tables) ----
;; A syntax table is a char-table with subtype `syntax-table'.  Each element is
;; a cons (SYNTAX-CODE . MATCHING-CHAR) where SYNTAX-CODE encodes the syntax
;; class in its low 16 bits plus flag bits; MATCHING-CHAR is the paired paren
;; (nil if none).  Verified value-for-value against the Emacs 30.2 binary.

;; syntax_code_spec (syntax.c): class index -> designator character.
(defconst --syntax-code-spec-- " .w_()'\"$\\/<>@!|"
  "Char at index N is the designator for syntax class N (see `string-to-syntax').")

(defun syntax-class-to-char (syntax-class)
  "Return the designator character for SYNTAX-CLASS (an integer 0..15)."
  (aref --syntax-code-spec-- syntax-class))

;; The flag descriptor characters and the bit each sets in the syntax code.
(defconst --syntax-flag-alist--
  '((?1 . 16) (?2 . 17) (?3 . 18) (?4 . 19) (?p . 20) (?b . 21) (?n . 22))
  "Maps a `modify-syntax-entry' flag char to the bit it sets in the syntax code.")

(defun string-to-syntax (descriptor)
  "Convert syntax DESCRIPTOR string into the internal (CODE . MATCH) form.
DESCRIPTOR's first char names the class, the second the matching char
\(a space or missing means none), and any remaining chars are flags."
  (when (or (null descriptor) (= (length descriptor) 0))
    (error "Invalid syntax description string: %S" descriptor))
  (let* ((c (aref descriptor 0))
         ;; `-' is an alias for whitespace; otherwise look C up in the spec.
         (class (if (eq c ?-) 0
                  (let ((i 0) (found nil) (len (length --syntax-code-spec--)))
                    (while (and (not found) (< i len))
                      (when (eq (aref --syntax-code-spec-- i) c) (setq found i))
                      (setq i (1+ i)))
                    (or found (error "Invalid syntax description string: %S" descriptor)))))
         (match (and (> (length descriptor) 1)
                     (let ((m (aref descriptor 1)))
                       (unless (eq m ?\s) m))))
         (code class)
         (i 2) (len (length descriptor)))
    (while (< i len)
      (let ((bit (cdr (assq (aref descriptor i) --syntax-flag-alist--))))
        (when bit (setq code (logior code (ash 1 bit)))))
      (setq i (1+ i)))
    (cons code match)))

(defun syntax-class (syntax)
  "Return the syntax class part of the syntax code SYNTAX (a (CODE . MATCH) cons).
Return nil if SYNTAX is nil."
  (and syntax (logand (car syntax) 65535)))

;; The standard syntax table, built once to mirror syntax.c `init_syntax_once'.
;; ASCII entries reproduce the binary's `standard-syntax-table' exactly; all
;; chars >= 128 default to word class (Emacs's dominant default), with U+00A0
;; (no-break space) set to whitespace.  Full Unicode punctuation/whitespace
;; categorization beyond that is not modeled (NAMED boundary).
(defvar --standard-syntax-table-- nil
  "The standard syntax table (see `standard-syntax-table').")

(defun --init-standard-syntax-table-- ()
  (let ((tbl (make-char-table 'syntax-table))
        (word (string-to-syntax "w"))
        (space (string-to-syntax " "))
        (punct (string-to-syntax "."))
        (symbol (string-to-syntax "_")))
    ;; Word everywhere by default (letters, digits, most non-ASCII).
    (set-char-table-range tbl t word)
    ;; Control chars 0..31 are punctuation ...
    (set-char-table-range tbl '(0 . 31) punct)
    ;; ... except the whitespace ones, plus space and U+00A0.
    (dolist (c '(?\t ?\n ?\f ?\r ?\s 160)) (aset tbl c space))
    ;; ASCII punctuation.
    (dolist (c '(?! ?# ?' ?, ?. ?: ?\; ?? ?@ ?^ ?` ?~ 127)) (aset tbl c punct))
    ;; ASCII symbol constituents.
    (dolist (c '(?& ?* ?+ ?- ?/ ?< ?= ?> ?_ ?|)) (aset tbl c symbol))
    ;; String quote and escape.
    (aset tbl ?\" (string-to-syntax "\""))
    (aset tbl ?\\ (string-to-syntax "\\"))
    ;; Balanced pairs: open matches close and vice versa.
    (aset tbl ?\( (string-to-syntax "()"))
    (aset tbl ?\) (string-to-syntax ")("))
    (aset tbl ?\[ (string-to-syntax "(]"))
    (aset tbl ?\] (string-to-syntax ")["))
    (aset tbl ?\{ (string-to-syntax "(}"))
    (aset tbl ?\} (string-to-syntax "){"))
    tbl))

(setq --standard-syntax-table-- (--init-standard-syntax-table--))

(defun standard-syntax-table ()
  "Return the standard syntax table.
This is the one used for new buffers."
  --standard-syntax-table--)

;; The current buffer's syntax table is a buffer-local slot defaulting to the
;; standard table (Emacs keeps it in the buffer object; buffer-local var here).
(defvar-local --current-syntax-table-- nil
  "The current buffer's syntax table (see `syntax-table').")

(defun syntax-table ()
  "Return the current syntax table.
This is the one specified by the current buffer."
  (or --current-syntax-table-- --standard-syntax-table--))

(defun set-syntax-table (table)
  "Select TABLE as the syntax table for the current buffer."
  (setq --current-syntax-table-- table)
  table)

(defun copy-syntax-table (&optional table)
  "Construct a new syntax table and return it.
It is a copy of TABLE, which defaults to the standard syntax table."
  (let* ((src (or table --standard-syntax-table--))
         (new (make-char-table 'syntax-table)))
    (set-char-table-parent new (char-table-parent src))
    ;; Copy every set char over the char range.  Non-ASCII beyond the modeled
    ;; range shares the source's word default via the range copy of ascii+.
    (dotimes (c 128) (aset new c (char-table-range src c)))
    (set-char-table-range new '(128 . #x3FFFFF) (char-table-range src 128))
    new))

(defun make-syntax-table (&optional parent)
  "Return a new syntax table.
Create a syntax table that inherits from PARENT (which defaults to the
standard syntax table)."
  (let ((table (make-char-table 'syntax-table)))
    (set-char-table-parent table (or parent --standard-syntax-table--))
    table))

(defun syntax-table-p (object)
  "Return t if OBJECT is a syntax table."
  (and (char-table-p object) (eq (char-table-subtype object) 'syntax-table)))

(defun modify-syntax-entry (char newentry &optional syntax-table)
  "Set syntax for the characters CHAR to the string NEWENTRY.
CHAR may be a cons (MIN . MAX), in which case, syntaxes of all characters
in the range between MIN and MAX, inclusive, are set.  SYNTAX-TABLE
defaults to the current buffer's syntax table."
  (set-char-table-range (or syntax-table (syntax-table))
                        char (string-to-syntax newentry))
  nil)

(defun char-syntax (character)
  "Return the syntax class of CHARACTER, described by a character.
For example, if CHARACTER is a word constituent, the character `?w' is
returned.  The characters that correspond to various syntax codes are
listed in the documentation of `modify-syntax-entry'."
  (syntax-class-to-char (syntax-class (aref (syntax-table) character))))

(defmacro with-syntax-table (table &rest body)
  "Evaluate BODY with syntax table TABLE as the current syntax table.
The syntax table of the current buffer is saved, BODY is evaluated, and
the saved table is restored.  (Single-buffer model: the save/restore is
over the one current buffer's syntax-table slot.)"
  (declare (indent 1) (debug t))
  (let ((old-table (make-symbol "table")))
    `(let ((,old-table (syntax-table)))
       (unwind-protect
           (progn
             (set-syntax-table ,table)
             ,@body)
         (set-syntax-table ,old-table)))))

;; ---- Lisp-mode syntax tables (lisp-mode.el / elisp-mode.el) ----
;; Pure make-syntax-table + modify-syntax-entry constructions; ported verbatim so
;; libraries that base their own parse tables on them (ietf-drums, rfc2047, the
;; url-* parsers, mail-parse) load with the exact upstream syntax classes.
(defvar lisp-data-mode-syntax-table
  (let ((table (make-syntax-table))
        (i 0))
    (while (< i ?0)
      (modify-syntax-entry i "_   " table)
      (setq i (1+ i)))
    (setq i (1+ ?9))
    (while (< i ?A)
      (modify-syntax-entry i "_   " table)
      (setq i (1+ i)))
    (setq i (1+ ?Z))
    (while (< i ?a)
      (modify-syntax-entry i "_   " table)
      (setq i (1+ i)))
    (setq i (1+ ?z))
    (while (< i 128)
      (modify-syntax-entry i "_   " table)
      (setq i (1+ i)))
    (modify-syntax-entry ?\s "    " table)
    ;; Non-break space acts as whitespace.
    (modify-syntax-entry ?\xa0 "    " table)
    (modify-syntax-entry ?\t "    " table)
    (modify-syntax-entry ?\f "    " table)
    (modify-syntax-entry ?\n ">   " table)
    (modify-syntax-entry ?\; "<   " table)
    (modify-syntax-entry ?` "'   " table)
    (modify-syntax-entry ?' "'   " table)
    (modify-syntax-entry ?, "'   " table)
    (modify-syntax-entry ?@ "_ p" table)
    ;; Used to be singlequote; changed for flonums.
    (modify-syntax-entry ?. "_   " table)
    (modify-syntax-entry ?# "'   " table)
    (modify-syntax-entry ?\" "\"    " table)
    (modify-syntax-entry ?\\ "\\   " table)
    (modify-syntax-entry ?\( "()  " table)
    (modify-syntax-entry ?\) ")(  " table)
    (modify-syntax-entry ?\[ "(]" table)
    (modify-syntax-entry ?\] ")[" table)
    table)
  "Parent syntax table used in Lisp modes.")

(defvar lisp-mode-syntax-table
  (let ((table (make-syntax-table lisp-data-mode-syntax-table)))
    (modify-syntax-entry ?\[ "_   " table)
    (modify-syntax-entry ?\] "_   " table)
    (modify-syntax-entry ?# "' 14" table)
    (modify-syntax-entry ?| "\" 23bn" table)
    table)
  "Syntax table used in `lisp-mode'.")

(defvar emacs-lisp-mode-syntax-table
  (let ((table (make-syntax-table lisp-data-mode-syntax-table)))
    ;; Remove the "p" flag from the entry of `@' because we use instead
    ;; `syntax-propertize' to take care of `,@', which is more precise.
    (modify-syntax-entry ?@ "_" table)
    table)
  "Syntax table used in `emacs-lisp-mode'.")

;; Abbrev table placeholder (boundary: the abbrev/obarray subsystem is not
;; modeled).  An abbrev table is a plain vector here rather than a real obarray.
;; `abbrev-table-get'/`abbrev-table-put' are ported for their observable
;; behavior (get returns what put stored; unset props are nil) using a property
;; side-table keyed by table identity, since the real accessors store props in
;; the table's own "" symbol, which needs the obarray subsystem elisprs lacks.
(defun make-abbrev-table (&optional props)
  "Create a new, empty abbrev table object.
PROPS is a list of properties."
  (let ((table (make-vector 59 0)))
    (while (consp props)
      (abbrev-table-put table (car props) (cadr props))
      (setq props (cddr props)))
    table))
(defvar --abbrev-table-props-- (make-hash-table :test 'eq)
  "Maps an abbrev table object to its property plist (see `abbrev-table-get').")
(defun abbrev-table-get (table prop)
  "Get the PROP property of abbrev table TABLE."
  (plist-get (gethash table --abbrev-table-props--) prop))
(defun abbrev-table-put (table prop val)
  "Set the PROP property of abbrev table TABLE to VAL."
  (puthash table
           (plist-put (gethash table --abbrev-table-props--) prop val)
           --abbrev-table-props--)
  val)
(defun define-abbrev-table (tablename &rest _)
  (unless (and (boundp tablename) (vectorp (symbol-value tablename)))
    (set tablename (make-abbrev-table)))
  tablename)

;; merge-ordered-lists (subr.el): merge LISTS into one, removing duplicates and
;; obeying each list's relative order (C3-style). Used by derived-mode-all-parents.
(defun merge-ordered-lists (lists &optional error-function)
  "Merge LISTS in a consistent order.
LISTS is a list of lists of elements."
  (let ((result '()))
    (setq lists (remq nil lists))
    (while (cdr (setq lists (delq nil lists)))
      (let* ((next nil)
	     (tail lists))
	(while tail
	  (let ((candidate (caar tail))
	        (other-lists lists))
	    (while other-lists
	      (if (not (memql candidate (cdr (car other-lists))))
	          (setq other-lists (cdr other-lists))
	        (setq candidate nil)
	        (setq other-lists nil)))
	    (if (not candidate)
	        (setq tail (cdr tail))
	      (setq next candidate)
	      (setq tail nil))))
	(unless next
	  (setq next (funcall (or error-function #'caar) lists))
	  (unless (assoc next lists #'eql)
	    (error "Invalid candidate returned by error-function: %S" next)))
	(push next result)
	(setq lists
	      (mapcar (lambda (l) (if (eql (car l) next) (cdr l) l))
		      lists))))
    (if (null result) (car lists)
      (append (nreverse result) (car lists)))))

;; Mode-line/hook variables (subr.el, buffer.c defaults).
(defvar-local major-mode 'fundamental-mode
  "Symbol for current buffer's major mode.")
(defvar-local mode-name nil
  "Pretty name of current buffer's major mode.")
(defvar delay-mode-hooks nil
  "If non-nil, `run-mode-hooks' should delay running the hooks.")
(defvar-local delayed-mode-hooks nil
  "List of delayed mode hooks waiting to be run.")
(put 'delay-mode-hooks 'permanent-local t)
(defvar-local delayed-after-hook-functions nil
  "List of delayed :after-hook forms waiting to be run.")
(defvar change-major-mode-after-body-hook nil
  "Normal hook run in major mode functions, before the mode hooks.")
(defvar after-change-major-mode-hook nil
  "Normal hook run at the very end of major mode functions.")
(defvar change-major-mode-hook nil
  "Normal hook run by `kill-all-local-variables' before it kills locals.")
(defvar-local local-abbrev-table nil
  "Local (mode-specific) abbrev table of current buffer.")

;; kill-all-local-variables (buffer.c documented behavior): run
;; `change-major-mode-hook', then eliminate the current buffer's local variables
;; except those with a non-nil `permanent-local' property, and reset the local
;; keymap. (Syntax-table/abbrev-table resets belong to subsystems not modeled.)
(defun kill-all-local-variables (&optional kill-permanent)
  "Switch to Fundamental mode by killing current buffer's local variables."
  (run-hooks 'change-major-mode-hook)
  (dolist (sym (--buffer-local-symbols--))
    (unless (and (not kill-permanent) (get sym 'permanent-local))
      (kill-local-variable sym)))
  (use-local-map nil)
  nil)

;; run-mode-hooks / delay-mode-hooks (subr.el, ported faithfully). The
;; `hack-local-variables' branch is inert here — `buffer-file-name' is always nil
;; (no file-visiting buffers).
(defun run-mode-hooks (&rest hooks)
  "Run mode hooks `delayed-mode-hooks' and HOOKS, or delay HOOKS."
  (if delay-mode-hooks
      (dolist (hook hooks)
	(push hook delayed-mode-hooks))
    (setq hooks (nconc (nreverse delayed-mode-hooks) hooks))
    (and (bound-and-true-p syntax-propertize-function)
         (not (local-variable-p 'parse-sexp-lookup-properties))
         (setq-local parse-sexp-lookup-properties t))
    (setq delayed-mode-hooks nil)
    (apply #'run-hooks (cons 'change-major-mode-after-body-hook hooks))
    (if (buffer-file-name)
        (with-demoted-errors "File local-variables error: %s"
          (hack-local-variables 'no-mode)))
    (run-hooks 'after-change-major-mode-hook)
    (dolist (fun (prog1 (nreverse delayed-after-hook-functions)
                    (setq delayed-after-hook-functions nil)))
      (funcall fun))))
(defmacro delay-mode-hooks (&rest body)
  "Execute BODY, but delay any `run-mode-hooks'."
  (declare (debug t) (indent 0))
  `(progn
     (make-local-variable 'delay-mode-hooks)
     (let ((delay-mode-hooks t))
       ,@body)))

;; setq-local (subr.el): make each SYM buffer-local and set it.
(defmacro setq-local (&rest pairs)
  "Make each SYMBOL local to the current buffer and set it to its VALUE."
  (let ((expansion nil) (sym nil))
    (unless (zerop (mod (length pairs) 2))
      (error "PAIRS must have an even number of variable/value members"))
    (while pairs
      (setq sym (car pairs))
      (push `(set (make-local-variable ',sym) ,(cadr pairs)) expansion)
      (setq pairs (cddr pairs)))
    (cons 'progn (nreverse expansion))))

;; Derived-mode parent tracking (subr.el).
(defun derived-mode--flush (mode)
  (put mode 'derived-mode--all-parents nil)
  (let ((followers (get mode 'derived-mode--followers)))
    (when followers
      (put mode 'derived-mode--followers nil)
      (mapc #'derived-mode--flush followers))))
(defun derived-mode-set-parent (mode parent)
  "Declare PARENT to be the parent of MODE."
  (put mode 'derived-mode-parent parent)
  (derived-mode--flush mode))
(defun derived-mode-add-parents (mode extra-parents)
  "Add EXTRA-PARENTS to the parents of MODE."
  (put mode 'derived-mode-extra-parents extra-parents)
  (derived-mode--flush mode))
(defun derived-mode-all-parents (mode &optional known-children)
  "Return all the parents of MODE, starting with MODE."
  (let ((ps (get mode 'derived-mode--all-parents)))
    (cond
     (ps ps)
     ((memq mode known-children)
      (memq mode (reverse known-children)))
     (t
      (let* ((new-children (cons mode known-children))
             (get-all-parents
              (lambda (parent)
                (let ((followers (get parent 'derived-mode--followers)))
                  (unless (memq mode followers)
                    (put parent 'derived-mode--followers
                         (cons mode followers))))
                (derived-mode-all-parents parent new-children)))
             (parent (or (get mode 'derived-mode-parent)
                         (let ((alias (symbol-function mode)))
                           (and (symbolp alias) alias))))
             (extras (get mode 'derived-mode-extra-parents))
             (all-parents
              (merge-ordered-lists
               (cons (if (and parent (not (memq parent extras)))
                         (funcall get-all-parents parent))
                     (mapcar get-all-parents extras)))))
        (if (and (memq mode all-parents) known-children)
            (cons mode (remq mode all-parents))
          (put mode 'derived-mode--all-parents (cons mode all-parents))))))))
(defun provided-mode-derived-p (mode &optional modes &rest old-modes)
  "Non-nil if MODE is derived from a mode that is a member of the list MODES."
  (cond
   (old-modes (setq modes (cons modes old-modes)))
   ((not (listp modes)) (setq modes (list modes))))
  (let ((ps (derived-mode-all-parents mode)))
    (while (and modes (not (memq (car modes) ps)))
      (setq modes (cdr modes)))
    (car modes)))
(defun derived-mode-p (&optional modes &rest old-modes)
  "Return non-nil if the current major mode is derived from one of MODES."
  (provided-mode-derived-p major-mode (if old-modes (cons modes old-modes)
                                        modes)))

;; use-local-map / current-local-map are C primitives (builtins.rs) backed by the
;; current buffer's local keymap slot.

;; Docstring helpers (subr.el / derived.el). internal--format-docstring-line does
;; not fill/wrap here (fill is cosmetic; the string content matches Emacs).
(defun internal--format-docstring-line (string &rest objects)
  "Format a single documentation line from STRING and OBJECTS."
  (when (string-match "\n" string)
    (error "Unable to fill string containing newline: %S" string))
  (apply #'format string objects))
(defsubst derived-mode-hook-name (mode)
  "Construct a mode-hook name based on the symbol MODE."
  (intern (concat (symbol-name mode) "-hook")))
(defsubst derived-mode-map-name (mode)
  "Construct a map name based on the symbol MODE."
  (intern (concat (symbol-name mode) "-map")))
(defsubst derived-mode-syntax-table-name (mode)
  "Construct a syntax-table name based on the symbol MODE."
  (intern (concat (symbol-name mode) "-syntax-table")))
(defsubst derived-mode-abbrev-table-name (mode)
  "Construct an abbrev-table name based on the symbol MODE."
  (intern (concat (symbol-name mode) "-abbrev-table")))
(defun derived-mode-make-docstring (parent child &optional
					   docstring syntax abbrev)
  "Construct a docstring for a new mode if none is provided."
  (let ((map (derived-mode-map-name child))
	(hook (derived-mode-hook-name child)))
    (unless (stringp docstring)
      (setq docstring
	    (if (null parent)
                (concat
                 "Major-mode.\n"
                 (internal--format-docstring-line
                  "Uses keymap `%s'%s%s." map
                  (if abbrev (format "%s abbrev table `%s'"
                                     (if syntax "," " and") abbrev) "")
                  (if syntax (format " and syntax-table `%s'" syntax) "")))
	      (format "Major mode derived from `%s' by `define-derived-mode'.
It inherits all of the parent's attributes, but has its own keymap%s:

%s

which more-or-less shadow%s %s's corresponding table%s."
		      parent
		      (cond ((and abbrev syntax)
			     ",\nabbrev table and syntax table")
			    (abbrev "\nand abbrev table")
			    (syntax "\nand syntax table")
			    (t ""))
                      (internal--format-docstring-line
                       "  `%s'%s"
                       map
                       (cond ((and abbrev syntax)
                              (format ", `%s' and `%s'" abbrev syntax))
                             ((or abbrev syntax)
                              (format " and `%s'" (or abbrev syntax)))
                             (t "")))
		      (if (or abbrev syntax) "" "s")
		      parent
		      (if (or abbrev syntax) "s" "")))))
    (unless (string-match (regexp-quote (symbol-name hook)) docstring)
      (setq docstring
            (concat docstring "\n\n"
                    (internal--format-docstring-line
                     "%s%s%s"
                     (if (null parent)
                         "This mode "
                       (concat
                        "In addition to any hooks its parent mode "
                        (if (string-match (format "[`‘]%s['’]"
                                                  (regexp-quote
                                                   (symbol-name parent)))
                                          docstring)
                            nil
                          (format "`%s' " parent))
                        "might have run, this mode "))
                     (format "runs the hook `%s'" hook)
                     ", as the final or penultimate step during initialization."))))
    (unless (string-match "\\\\[{[]" docstring)
      (setq docstring (concat docstring "\n\n\\{" (symbol-name map) "}")))
    docstring))

;; define-derived-mode (derived.el), ported faithfully.
(defmacro define-derived-mode (child parent name &optional docstring &rest body)
  "Create a new mode CHILD which is a variant of an existing mode PARENT."
  (declare (debug (&define name symbolp sexp [&optional stringp]
			   [&rest keywordp sexp] def-body))
	   (doc-string 4)
	   (indent defun))
  (when (and docstring (not (stringp docstring)))
    (push docstring body)
    (setq docstring nil))
  (when (eq parent 'fundamental-mode) (setq parent nil))
  (let ((map (derived-mode-map-name child))
	(syntax (derived-mode-syntax-table-name child))
	(abbrev (derived-mode-abbrev-table-name child))
	(declare-abbrev t)
	(declare-syntax t)
	(hook (derived-mode-hook-name child))
	(group nil)
        (interactive t)
        (after-hook nil))
    (while (keywordp (car body))
      (pcase (pop body)
	(:group (setq group (pop body)))
	(:abbrev-table (setq abbrev (pop body)) (setq declare-abbrev nil))
	(:syntax-table (setq syntax (pop body)) (setq declare-syntax nil))
        (:after-hook (setq after-hook (pop body)))
        (:interactive (setq interactive (pop body)))
	(_ (pop body))))
    (setq docstring (derived-mode-make-docstring
		     parent child docstring syntax abbrev))
    `(progn
       (defvar ,hook nil)
       (unless (get ',hook 'variable-documentation)
         (put ',hook 'variable-documentation
              ,(format "Hook run after entering `%S'.
No problems result if this variable is not bound.
`add-hook' automatically binds it.  (This is true for all hook variables.)"
                       child)))
       ;; Mark the map special (bare `defvar', no value) always, but only
       ;; INITIALIZE it when unbound -- mirroring Emacs's no-clobber `defvar'
       ;; and the syntax/abbrev guards below.  A mode whose map is preloaded
       ;; (e.g. `special-mode-map' via `defvar-keymap') keeps its bindings.
       (with-no-warnings (defvar ,map))
       (unless (boundp ',map)
	 (put ',map 'definition-name ',child)
	 (defvar ,map (make-sparse-keymap)))
       (unless (get ',map 'variable-documentation)
	 (put ',map 'variable-documentation
	      (purecopy ,(format "Keymap for `%s'." child))))
       ,(if declare-syntax
	    `(progn
               (defvar ,syntax)
	       (unless (boundp ',syntax)
		 (put ',syntax 'definition-name ',child)
		 (defvar ,syntax (make-syntax-table)))
	       (unless (get ',syntax 'variable-documentation)
		 (put ',syntax 'variable-documentation
		      (purecopy ,(format "Syntax table for `%s'." child))))))
       ,(if declare-abbrev
	    `(progn
               (defvar ,abbrev)
	       (unless (boundp ',abbrev)
		 (put ',abbrev 'definition-name ',child)
		 (defvar ,abbrev
		   (progn (define-abbrev-table ',abbrev nil) ,abbrev)))
	       (unless (get ',abbrev 'variable-documentation)
		 (put ',abbrev 'variable-documentation
		      (purecopy ,(format "Abbrev table for `%s'." child))))))
       (if (fboundp 'derived-mode-set-parent)
           (derived-mode-set-parent ',child ',parent)
         (put ',child 'derived-mode-parent ',parent))
       ,(if group `(put ',child 'custom-mode-group ,group))
       (defun ,child ()
	 ,docstring
	 ,(and interactive '(interactive))
	 (delay-mode-hooks
	  (,(or parent 'kill-all-local-variables))
	  (setq major-mode (quote ,child))
	  (setq mode-name ,name)
	  ,(when parent
	     `(progn
		(if (get (quote ,parent) 'mode-class)
		    (put (quote ,child) 'mode-class
			 (get (quote ,parent) 'mode-class)))
		(unless (keymap-parent ,map)
		  (set-keymap-parent ,map (current-local-map)))
		,(when declare-syntax
		   `(let ((parent (char-table-parent ,syntax)))
		      (unless (and parent
				   (not (eq parent (standard-syntax-table))))
			(set-char-table-parent ,syntax (syntax-table)))))
                ,(when declare-abbrev
                   `(unless (or (abbrev-table-get ,abbrev :parents)
                                (eq ,abbrev local-abbrev-table))
                      (abbrev-table-put ,abbrev :parents
                                        (list local-abbrev-table))))))
	  (use-local-map ,map)
	  ,(when syntax `(set-syntax-table ,syntax))
	  ,(when abbrev `(setq local-abbrev-table ,abbrev))
	  ,@body)
	 ,@(when after-hook
	     `((push (lambda () ,after-hook) delayed-after-hook-functions)))
	 (run-mode-hooks ',hook)))))

;; ---- fundamental-mode + special-mode (simple.el) ----
;; Buffer-local state slots these modes touch (buffer.c C variables in Emacs;
;; buffer-local Lisp variables here).
(defvar-local buffer-read-only nil
  "Non-nil if this buffer is read-only.")
(defvar-local buffer-undo-list nil
  "List of undo entries in current buffer.
A value of t means undo information is not being recorded.")

;; buffer-disable-undo (simple.el): stop keeping undo information.
(defun buffer-disable-undo (&optional _buffer)
  "Make current buffer stop keeping undo information."
  (setq buffer-undo-list t))
(defun buffer-enable-undo (&optional _buffer)
  "Start keeping undo information for the current buffer."
  (when (eq buffer-undo-list t)
    (setq buffer-undo-list nil)))

;; fundamental-mode (simple.el): the root major mode.  Body is empty; it just
;; resets local variables and installs itself as `major-mode'.
(defun fundamental-mode ()
  "Major mode not specialized for anything in particular.
Other major modes are defined by comparison with this one."
  (interactive)
  (kill-all-local-variables)
  (setq major-mode 'fundamental-mode)
  (setq mode-name "Fundamental")
  (run-mode-hooks))

;; Display-engine state variables (C `character.c'/`xdisp.c' + `simple.el').
;; These are the real variable declarations, not stubs; the redisplay engine
;; that acts on them is a separate subsystem and is not modeled here.
;; `glyphless-char-display' is itself a char-table (subtype glyphless-char-display,
;; one extra slot), so it also exercises the char-table type.
(put 'glyphless-char-display 'char-table-extra-slots 1)
(defvar glyphless-char-display (make-char-table 'glyphless-char-display)
  "Char-table defining glyphless characters.")
(defvar-local truncate-lines nil
  "Non-nil means do not display continuation lines.")
(defvar-local bidi-paragraph-direction nil
  "If non-nil, forces a paragraph direction in the current buffer.")
(defvar-local text-scale-remap-header-line nil
  "If non-nil, text scaling may change the height of the header line.")
(defvar-local revert-buffer-function nil
  "Function to use to revert this buffer.")

;; special-mode (simple.el): parent mode for buffers that view formatted data.
(put 'special-mode 'mode-class 'special)
(define-derived-mode special-mode nil "Special"
  "Parent major mode from which special major modes should inherit.

A special major mode is intended to view specially formatted data
rather than files.  These modes usually use read-only buffers."
  (setq buffer-read-only t))

;; ---- define-minor-mode (easy-mmode.el) + supporting subr.el pieces ----

;; prefix-numeric-value (C callint.c documented behavior): the numeric value of a
;; raw prefix argument. nil -> 1, `-' -> -1, (N) -> N, a number -> itself.
(defun prefix-numeric-value (arg)
  "Return numeric meaning of raw prefix argument ARG."
  (cond ((null arg) 1)
        ((eq arg '-) -1)
        ((consp arg) (car arg))
        ((integerp arg) arg)
        (t 1)))
(defvar current-prefix-arg nil
  "The raw prefix argument for the next command.")
;; Interactive invocation is not modeled (no `call-interactively'); a command
;; called from Lisp — the only path here — is never an interactive call.
(defun called-interactively-p (&optional _kind) nil)
;; Batch has no echo area / mode line.
(defun current-message () nil)
(defun force-mode-line-update (&optional _all) nil)
;; Warnings are not modeled; return the form unchanged.
(defun macroexp-warn-and-return (_msg form &rest _) form)

;;; ---- declaration handlers (byte-run.el) -----------------------------------
;; `declare' specs that have a *runtime* effect (register a gv-setter, set the
;; obsolete/indent/doc-string properties, …) are processed at defun/defmacro
;; macroexpansion time.  In Emacs `defun'/`defmacro' are themselves macros that
;; do this; in elisprs they are compiler special forms, so the host's
;; defun/defmacro expander (macroexpand_all) delegates to
;; `elisprs--expand-defun-declarations' below once these handlers are defined.
;; Each `defun-declarations-alist' entry is (PROP FUN [DOC]); FUN is applied to
;; (NAME ARGLIST . VALUES) and returns a form to eval after the definition.
;; Ported faithfully from byte-run.el.
(defalias 'byte-run--set-advertised-calling-convention
  (lambda (f _args arglist when)
    (list 'set-advertised-calling-convention
          (list 'quote f) (list 'quote arglist) (list 'quote when))))
(defalias 'byte-run--set-obsolete
  (lambda (f _args new-name when)
    (list 'make-obsolete
          (list 'quote f) (list 'quote new-name) when)))
(defalias 'byte-run--set-interactive-only
  (lambda (f _args instead)
    (list 'function-put (list 'quote f)
          ''interactive-only (list 'quote instead))))
(defalias 'byte-run--set-pure
  (lambda (f _args val)
    (list 'function-put (list 'quote f)
          ''pure (list 'quote val))))
(defalias 'byte-run--set-side-effect-free
  (lambda (f _args val)
    (list 'function-put (list 'quote f)
          ''side-effect-free (list 'quote val))))
(defalias 'byte-run--set-important-return-value
  (lambda (f _args val)
    (list 'function-put (list 'quote f)
          ''important-return-value (list 'quote val))))
(defalias 'byte-run--set-compiler-macro
  (lambda (f args compiler-function)
    (if (not (eq (car-safe compiler-function) 'lambda))
        `(eval-and-compile
           (function-put ',f 'compiler-macro #',compiler-function))
      (let ((cfname (intern (concat (symbol-name f) "--anon-cmacro")))
            (data (cdr compiler-function)))
        `(progn
           (eval-and-compile
             (function-put ',f 'compiler-macro #',cfname))
           :autoload-end
           (eval-and-compile
             (defun ,cfname (,@(car data) ,@args)
               (ignore ,@(delq '&rest (delq '&optional (copy-sequence args))))
               ,@(cdr data))))))))
(defalias 'byte-run--set-doc-string
  (lambda (f _args pos)
    (list 'function-put (list 'quote f)
          ''doc-string-elt (if (numberp pos) pos (list 'quote pos)))))
(defalias 'byte-run--set-indent
  (lambda (f _args val)
    (list 'function-put (list 'quote f)
          ''lisp-indent-function (if (numberp val) val (list 'quote val)))))
(defalias 'byte-run--set-speed
  (lambda (f _args val)
    (list 'function-put (list 'quote f) ''speed (list 'quote val))))
(defalias 'byte-run--set-safety
  (lambda (f _args val)
    (list 'function-put (list 'quote f) ''safety (list 'quote val))))
(defalias 'byte-run--set-completion
  (lambda (f _args val)
    (list 'function-put (list 'quote f)
          ''completion-predicate (list 'function val))))
(defalias 'byte-run--set-modes
  (lambda (f _args &rest val)
    (list 'function-put (list 'quote f) ''command-modes (list 'quote val))))
(defalias 'byte-run--set-interactive-args
  (lambda (f args &rest val)
    (setq args (remove '&optional (remove '&rest args)))
    (list 'function-put (list 'quote f)
          ''interactive-args
          (list 'quote
                (mapcar (lambda (elem)
                          (cons (seq-position args (car elem)) (cadr elem)))
                        val)))))
(defalias 'byte-run--set-function-type
  (lambda (f _args val &optional f2)
    (when (and f2 (not (eq f2 f)))
      (error "`%s' does not match top level function `%s' inside function type declaration"
             f2 f))
    (list 'function-put (list 'quote f) ''function-type (list 'quote val))))
(defalias 'byte-run--set-debug
  (lambda (name _args spec)
    (list 'progn :autoload-end
          (list 'put (list 'quote name) ''edebug-form-spec (list 'quote spec)))))
(defalias 'byte-run--set-no-font-lock-keyword
  (lambda (name _args val)
    (list 'function-put (list 'quote name) ''no-font-lock-keyword (list 'quote val))))

;; Populate the alists declared earlier as nil (byte-run.el:235,349).
(setq defun-declarations-alist
      (list
       (list 'advertised-calling-convention
             #'byte-run--set-advertised-calling-convention)
       (list 'obsolete #'byte-run--set-obsolete)
       (list 'interactive-only #'byte-run--set-interactive-only)
       (list 'pure #'byte-run--set-pure)
       (list 'side-effect-free #'byte-run--set-side-effect-free)
       (list 'important-return-value #'byte-run--set-important-return-value)
       (list 'compiler-macro #'byte-run--set-compiler-macro)
       (list 'doc-string #'byte-run--set-doc-string)
       (list 'indent #'byte-run--set-indent)
       (list 'speed #'byte-run--set-speed)
       (list 'safety #'byte-run--set-safety)
       (list 'completion #'byte-run--set-completion)
       (list 'modes #'byte-run--set-modes)
       (list 'interactive-args #'byte-run--set-interactive-args)
       (list 'ftype #'byte-run--set-function-type)))
(setq macro-declarations-alist
      (cons (list 'debug #'byte-run--set-debug)
            (cons (list 'no-font-lock-keyword #'byte-run--set-no-font-lock-keyword)
                  defun-declarations-alist)))

;; byte-run.el:279 — split BODY into (DOCSTRING DECLARE INTERACTIVE REST WARNINGS).
(defun byte-run--parse-body (body allow-interactive)
  "Decompose BODY into (DOCSTRING DECLARE INTERACTIVE BODY-REST WARNINGS)."
  (let* ((top body)
         (docstring nil)
         (declare-form nil)
         (interactive-form nil)
         (warnings nil)
         (warn (lambda (msg form)
                 (push (macroexp-warn-and-return
                        (format-message msg) nil nil t form)
                       warnings))))
    (while
        (and body
             (let* ((form (car body))
                    (head (car-safe form)))
               (cond
                ((or (and (stringp form) (cdr body))
                     (eq head :documentation))
                 (cond
                  (docstring (funcall warn "More than one doc string" top))
                  (declare-form
                   (funcall warn "Doc string after `declare'" declare-form))
                  (interactive-form
                   (funcall warn "Doc string after `interactive'" interactive-form))
                  (t (setq docstring form)))
                 t)
                ((eq head 'declare)
                 (cond
                  (declare-form
                   (funcall warn "More than one `declare' form" form))
                  (interactive-form
                   (funcall warn "`declare' after `interactive'" form))
                  (t (setq declare-form form)))
                 t)
                ((eq head 'interactive)
                 (cond
                  ((not allow-interactive)
                   (funcall warn "No `interactive' form allowed here" form))
                  (interactive-form
                   (funcall warn "More than one `interactive' form" form))
                  (t (setq interactive-form form)))
                 t))))
      (setq body (cdr body)))
    (list docstring declare-form interactive-form body warnings)))

;; byte-run.el:326 — map each declaration clause through DECLARATIONS-ALIST.
(defun byte-run--parse-declarations (name arglist clauses construct declarations-alist)
  (let* ((cl-decls nil)
         (actions
          (mapcar
           (lambda (x)
             (let ((f (cdr (assq (car x) declarations-alist))))
               (cond
                (f (apply (car f) name arglist (cdr x)))
                ((and (featurep 'cl)
                      (memq (car x)
                            '(special inline notinline optimize warn)))
                 (push (list 'declare x) cl-decls)
                 nil)
                (t
                 (macroexp-warn-and-return
                  (format-message "Unknown %s property `%S'"
                                  construct (car x))
                  nil nil nil (car x))))))
           clauses)))
    (cons actions cl-decls)))

;; gv.el:159 — turn a (gv-setter …)/(gv-expander …) declaration into the form
;; that registers the corresponding generalized-variable handler.
(defun gv--defun-declaration (symbol name args handler &optional fix)
  `(progn
     :autoload-end
     ,(pcase (cons symbol handler)
        (`(gv-expander . (lambda (,do) . ,body))
         `(gv-define-expander ,name (lambda (,do ,@args) ,@body)))
        (`(gv-expander . ,(pred symbolp))
         `(gv-define-expander ,name #',handler))
        (`(gv-setter . (lambda (,store) . ,body))
         `(gv-define-setter ,name (,store ,@args) ,@body))
        (`(gv-setter . ,(pred symbolp))
         `(gv-define-simple-setter ,name ,handler ,fix))
        (_ (message "Unknown %s declaration %S" symbol handler) nil))))
(defsubst gv--expander-defun-declaration (&rest args)
  (apply #'gv--defun-declaration 'gv-expander args))
(defsubst gv--setter-defun-declaration (&rest args)
  (apply #'gv--defun-declaration 'gv-setter args))
(or (assq 'gv-expander defun-declarations-alist)
    (let ((x (list 'gv-expander #'gv--expander-defun-declaration)))
      (push x macro-declarations-alist)
      (push x defun-declarations-alist)))
(or (assq 'gv-setter defun-declarations-alist)
    (push (list 'gv-setter #'gv--setter-defun-declaration)
          defun-declarations-alist))

;; Bridge invoked by the host's defun/defmacro expander (macroexpand_all).
;; CONSTRUCT is `defun' or `defmacro'; NAME/ARGLIST/BODY are the raw pieces.
;; Returns the replacement form threading each declaration's runtime side-effect
;; form after the (unchanged-shape) definition, mirroring byte-run.el's `defun'
;; and `defmacro' macros — or nil when BODY has no `declare' (host falls through
;; to plain body expansion).
(defun elisprs--expand-defun-declarations (construct name arglist body)
  (let* ((allow-interactive (eq construct 'defun))
         (parse (byte-run--parse-body body allow-interactive))
         (docstring (nth 0 parse))
         (declare-form (nth 1 parse))
         (interactive-form (nth 2 parse))
         (rest (nth 3 parse))
         (warnings (nth 4 parse))
         (alist (if (eq construct 'defmacro)
                    macro-declarations-alist
                  defun-declarations-alist))
         (declarations
          (and declare-form
               (byte-run--parse-declarations
                name arglist (cdr declare-form) construct alist))))
    (when declare-form
      (setq rest (nconc warnings rest))
      (setq rest (nconc (cdr declarations) rest))
      (when interactive-form (setq rest (cons interactive-form rest)))
      (when docstring (setq rest (cons docstring rest)))
      (when (null rest) (setq rest '(nil)))
      (let ((def (cons construct (cons name (cons arglist rest)))))
        (if declarations
            (cons 'prog1 (cons def (car declarations)))
          def)))))
;; custom-local-buffer (custom.el): when non-nil in a Customization buffer,
;; :set functions target that buffer's local binding instead of the default.
(defvar custom-local-buffer nil
  "Non-nil, in a Customization buffer, means customize a specific buffer.
If this variable is non-nil, it should be a buffer,
and it means customize the local bindings of that buffer.
This variable is a permanent local, and it normally has a local binding
in every Customization buffer.")
(put 'custom-local-buffer 'permanent-local t)

;; custom-set-default (custom.el): the default :set function for a customizable
;; variable.  Sets the default value of VARIABLE to VALUE, unless a
;; `custom-local-buffer' is active (Customize buffer), in which case the local
;; binding in that buffer is set instead.
(defun custom-set-default (variable value)
  "Default :set function for a customizable variable.
Normally, this sets the default value of VARIABLE to VALUE,
but if `custom-local-buffer' is non-nil,
this sets the local binding in that buffer instead."
  (if custom-local-buffer
      (with-current-buffer custom-local-buffer
	(set variable value))
    (set-default-toplevel-value variable value)))

;; ---------------------------------------------------------------------------
;; custom-set-variables / custom-theme-set-variables (custom.el).
;;
;; This is the machinery a user's Customize block generates.  Ported faithfully
;; from custom.el (Emacs 30.2).  It is pure Lisp: no widgets, keymaps, eieio, or
;; face machinery on this path -- only theme-value/saved-value bookkeeping via
;; get/put plus the default :set (custom-set-default).
;; ---------------------------------------------------------------------------

;; load-history (lread.c global): normally a list of loaded files and the
;; symbols they defined.  elisprs does not track it; nil means "nothing known
;; to be loaded", which only affects the autoload branch of custom-load-symbol.
(defvar load-history nil
  "Alist mapping loaded file names to symbols and features.")

;; custom.el:690
(defvar custom-load-recursion nil
  "Hack to avoid recursive dependencies.")

;; custom.el:884
(defvar custom-known-themes '(user changed)
  "Themes that have been defined with `deftheme'.")

;; custom.el:892
(defsubst custom-theme-p (theme)
  "Non-nil when THEME has been defined."
  (memq theme custom-known-themes))

;; custom.el:895
(defsubst custom-check-theme (theme)
  "Check whether THEME is valid, and signal an error if it is not."
  (unless (custom-theme-p theme)
    (error "Unknown theme `%s'" theme)))

;; custom.el:753
(defun custom-quote (sexp)
  "Quote SEXP if it is not self quoting."
  (if (and (not (consp sexp))
           (or (keywordp sexp)
               (not (symbolp sexp))
               (booleanp sexp)))
      sexp
    (list 'quote sexp)))

;; custom.el:1234
(defvar custom--inhibit-theme-enable 'apply-only-user
  "Whether the custom-theme-set-* functions act immediately.
If nil, `custom-theme-set-variables' and `custom-theme-set-faces'
change the current values of the given variable or face.  If t,
they just make a record of the theme's settings.  If the value is
`apply-only-user', then only the `user' theme is allowed to
change the current values.")

;; custom.el:901
(defun custom--should-apply-setting (theme)
  (or (null custom--inhibit-theme-enable)
      (and (eq custom--inhibit-theme-enable 'apply-only-user)
           (eq theme 'user))))

;; custom.el:906
(defun custom-push-theme (prop symbol theme mode &optional value)
  "Record VALUE for face or variable SYMBOL in custom theme THEME.
PROP is `theme-face' for a face, `theme-value' for a variable.
MODE can be either the symbol `set' or the symbol `reset'."
  (unless (memq prop '(theme-value theme-face theme-icon))
    (error "Unknown theme property"))
  (let* ((old (get symbol prop))
	 (setting (assq theme old))
	 (theme-settings
	  (get theme 'theme-settings)))
    (cond
     ((eq mode 'reset)
      (when setting
	(let (res)
	  (dolist (theme-setting theme-settings)
	    (if (and (eq (car  theme-setting) prop)
		     (eq (cadr theme-setting) symbol))
		(setq res theme-setting)))
	  (put theme 'theme-settings (delq res theme-settings)))
	(put symbol prop (delq setting old))))
     (setting
      (let (res)
	(dolist (theme-setting theme-settings)
	  (if (and (eq (car  theme-setting) prop)
		   (eq (cadr theme-setting) symbol))
	      (setq res theme-setting)))
	(put theme 'theme-settings
	     (cons (list prop symbol theme value)
		   (delq res theme-settings)))
        (put symbol prop (cons (list theme value) (delq setting old)))))
     (t
      (when (custom--should-apply-setting theme)
	(unless old
	  (when (and (eq prop 'theme-value)
		     (boundp symbol))
	    (let ((sv  (get symbol 'standard-value))
		  (val (symbol-value symbol)))
	      (unless (or
                       (and sv (equal (eval (car sv)) val))
                       (and (eq theme 'user) (equal (custom-quote val) value)))
		(setq old `((changed ,(custom-quote val))))))))
	(put symbol prop (cons (list theme value) old)))
      (put theme 'theme-settings
	   (cons (list prop symbol theme value) theme-settings))))))

;; custom.el:693
(defun custom-load-symbol (symbol)
  "Load all dependencies for SYMBOL."
  (unless custom-load-recursion
    (let ((custom-load-recursion t))
      (ignore-errors
        (require 'cus-load))
      (ignore-errors
        (require 'cus-start))
      (dolist (load (get symbol 'custom-loads))
        (cond ((symbolp load) (ignore-errors (require load)))
	      ((assoc load load-history))
	      ((let ((regexp (concat "\\(\\`\\|/\\)" (regexp-quote load)
				     "\\(\\'\\|\\.\\)"))
		     (found nil))
		 (dolist (loaded load-history)
		   (and (stringp (car loaded))
			(string-match-p regexp (car loaded))
			(setq found t)))
		 found))
	      ((equal load "cus-edit"))
              (t (ignore-errors (load load))))))))

;; custom.el:1085 (defvars for the topological sort)
(defvar custom--sort-vars-table)
(defvar custom--sort-vars-result)

;; custom.el:1123
(defun custom--sort-vars-1 (sym &optional _ignored)
  (let ((elt (gethash sym custom--sort-vars-table)))
    (when elt
      (cond
       ((eq (car elt) 'dependant)
	(error "Circular custom dependency on `%s'" sym))
       ((car elt)
	(setcar elt 'dependant)
	(dolist (dep (get sym 'custom-dependencies))
	  (custom--sort-vars-1 dep))
	(setcar elt nil)
	(push (cdr elt) custom--sort-vars-result))))))

;; custom.el:1088
(defun custom--sort-vars (vars)
  "Sort VARS based on custom dependencies."
  (let ((custom--sort-vars-table (make-hash-table))
	(dependants (make-hash-table))
	(custom--sort-vars-result nil)
	last)
    (dolist (var vars)
      (puthash (car var) (cons t var) custom--sort-vars-table)
      (puthash (car var) var dependants))
    (dolist (var vars)
      (dolist (dep (get (car var) 'custom-dependencies))
	(remhash dep dependants)))
    (maphash (lambda (sym var)
	       (when (and (null (get sym 'custom-dependencies))
			  (or (nth 3 var)
			      (eq (get sym 'custom-set)
				  'custom-set-minor-mode)))
		 (remhash sym dependants)
		 (push var last)))
	     dependants)
    (maphash #'custom--sort-vars-1 dependants)
    (nconc (nreverse custom--sort-vars-result) last)))

;; custom.el:1017
(defun custom-theme-set-variables (theme &rest args)
  "Initialize variables for theme THEME according to settings in ARGS.
Each of the arguments in ARGS should be a list of this form:

  (SYMBOL EXP [NOW [REQUEST [COMMENT]]])"
  (custom-check-theme theme)
  (dolist (entry args)
    (let* ((symbol (indirect-variable (nth 0 entry))))
      (unless (or (get symbol 'standard-value)
                  (memq (get symbol 'custom-autoload) '(nil noset)))
        (custom-load-symbol symbol))))
  (setq args (custom--sort-vars args))
  (dolist (entry args)
    (unless (listp entry)
      (error "Incompatible Custom theme spec"))
    (let* ((symbol (indirect-variable (nth 0 entry)))
	   (value (nth 1 entry)))
      (custom-push-theme 'theme-value symbol theme 'set value)
      (when (custom--should-apply-setting theme)
	(let* ((now (nth 2 entry))
	       (requests (nth 3 entry))
	       (comment (nth 4 entry))
	       set)
	  (when requests
	    (put symbol 'custom-requests requests)
            (mapc (lambda (lib) (require lib nil t)) requests))
          (setq set (or (get symbol 'custom-set) #'custom-set-default))
	  (put symbol 'saved-value (list value))
	  (put symbol 'saved-variable-comment comment)
	  (condition-case data
	      (cond (now
		     (put symbol 'force-value t)
		     (funcall set symbol (eval value)))
		    ((default-boundp symbol)
		     (funcall set symbol (eval value))))
	    (error
	     (message "Error setting %s: %s" symbol data)))
	  (and (or now (default-boundp symbol))
	       (put symbol 'variable-comment comment)))))))

;; custom.el:1001
(defun custom-set-variables (&rest args)
  "Install user customizations of variable values specified in ARGS.
These settings are registered as theme `user'.
The arguments should each be a list of the form:

  (SYMBOL EXP [NOW [REQUEST [COMMENT]]])"
  (apply #'custom-theme-set-variables 'user args))

;; custom-set-minor-mode (custom.el): a defcustom :set that toggles the mode.
;; (Reached only via Customize, which is out of the mode-run path.)
(defun custom-set-minor-mode (variable value)
  (funcall variable (if value 1 0)))

;; minor-mode registries (subr.el defvars).
(defvar minor-mode-list nil
  "List of all minor mode functions.")
(defvar minor-mode-alist nil
  "Alist saying how to show minor modes in the mode line.")
(defvar minor-mode-map-alist nil
  "Alist of keymaps to use for minor modes.")
(defvar global-minor-modes nil
  "A list of the currently enabled global minor modes.")
(defvar-local local-minor-modes nil
  "A list of the currently enabled minor modes in the current buffer.")
(defvar mode-line-mode-menu (make-sparse-keymap "Minor Modes")
  "Menu of mode operations in the mode line.")

;; add-minor-mode (subr.el), ported faithfully.
(defun add-minor-mode (toggle name &optional keymap after toggle-fun)
  "Register a new minor mode."
  (unless (memq toggle minor-mode-list)
    (push toggle minor-mode-list))
  (unless toggle-fun (setq toggle-fun toggle))
  (unless (eq toggle-fun toggle)
    (put toggle :minor-mode-function toggle-fun))
  (when name
    (let ((existing (assq toggle minor-mode-alist)))
      (if existing
	  (setcdr existing (list name))
	(let ((tail minor-mode-alist) found)
	  (while (and tail (not found))
	    (if (eq after (caar tail))
		(setq found tail)
	      (setq tail (cdr tail))))
	  (if found
	      (let ((rest (cdr found)))
		(setcdr found nil)
		(nconc found (list (list toggle name)) rest))
	    (push (list toggle name) minor-mode-alist))))))
  (when (get toggle :included)
    (define-key mode-line-mode-menu
      (vector toggle)
      (list 'menu-item
	    (concat
	     (or (get toggle :menu-tag)
		 (if (stringp name) name (symbol-name toggle)))
	     (let ((mode-name (if (symbolp name) (symbol-value name))))
	       (if (and (stringp mode-name) (string-match "[^ ]+" mode-name))
		   (concat " (" (match-string 0 mode-name) ")"))))
	    toggle-fun
	    :button (cons :toggle toggle))))
  (when keymap
    (let ((existing (assq toggle minor-mode-map-alist)))
      (if existing
	  (setcdr existing keymap)
	(let ((tail minor-mode-map-alist) found)
	  (while (and tail (not found))
	    (if (eq after (caar tail))
		(setq found tail)
	      (setq tail (cdr tail))))
	  (if found
	      (let ((rest (cdr found)))
		(setcdr found nil)
		(nconc found (list (cons toggle keymap)) rest))
	    (push (cons toggle keymap) minor-mode-map-alist)))))))

;; easy-mmode-pretty-mode-name (easy-mmode.el).
(defun easy-mmode-pretty-mode-name (mode &optional lighter)
  "Turn the symbol MODE into a string intended for the user."
  (let* ((case-fold-search t)
	 (name (concat (replace-regexp-in-string
			"-Minor" " minor"
			(capitalize (replace-regexp-in-string
				     "toggle-\\|-mode\\'" ""
                                     (symbol-name mode))))
		       " mode")))
    (setq name (replace-regexp-in-string "\\`Global-" "Global " name))
    (if (not (stringp lighter)) name
      (setq lighter (replace-regexp-in-string "\\`\\s-+\\|\\s-+\\'" ""
					      lighter))
      (replace-regexp-in-string (regexp-quote lighter) lighter name t t))))

;; ensure-empty-lines (subr.el): ensure LINES empty lines before point.
(defun ensure-empty-lines (&optional lines)
  "Ensure that there are LINES number of empty lines before point."
  (unless (bolp)
    (insert "\n"))
  (let ((lines (or lines 1))
        (start (save-excursion
                 (if (re-search-backward "[^\n]" nil t)
                     (+ (point) 2)
                   (point-min)))))
    (cond
     ((> (- (point) start) lines)
      (delete-region (point) (- (point) (- (point) start lines))))
     ((< (- (point) start) lines)
      (insert (make-string (- lines (- (point) start)) ?\n))))))

(defconst easy-mmode--arg-docstring
  "This is a %sminor mode.  If called interactively, toggle the
`%s' mode.  If the prefix argument is positive, enable the mode,
and if it is zero or negative, disable the mode.

If called from Lisp, toggle the mode if ARG is `toggle'.
Enable the mode if ARG is nil, omitted, or is a positive number.
Disable the mode if ARG is a negative number.

To check whether the minor mode is enabled in the current buffer,
evaluate %s.

The mode's hook is called both when the mode is enabled and when
it is disabled.")
;; easy-mmode--mode-docstring (easy-mmode.el). Uses a temp buffer to compose the
;; docstring; `fill-region' is skipped when unbound (no line-wrap; cosmetic).
(defun easy-mmode--mode-docstring (doc mode-pretty-name keymap-sym getter global)
  (if (and doc (string-match-p "\\bARG\\b" doc))
      doc
    (with-temp-buffer
      (let ((lines (if doc
                       (string-lines doc)
                     (list (format "Toggle %s on or off." mode-pretty-name)))))
        (insert (pop lines))
        (ensure-empty-lines)
        (while (and lines (equal (car lines) ""))
          (pop lines))
        (dolist (line lines)
          (insert line "\n"))
        (ensure-empty-lines)
        (let* ((fill-prefix nil)
               (docs-fc (bound-and-true-p emacs-lisp-docstring-fill-column))
               (fill-column (if (integerp docs-fc) docs-fc 65))
               (argdoc (format
                        easy-mmode--arg-docstring
                        (if global "global " "")
                        mode-pretty-name
                        (concat
                         (if (symbolp getter) "the variable ")
                         (format "`%s'"
                                 (string-replace "'" "\\='" (format "%S" getter)))))))
          (let ((start (point)))
            (insert argdoc)
            (when (fboundp 'fill-region)
              (fill-region start (point) 'left t))))
        (when (and (boundp keymap-sym)
                   (or (not doc)
                       (not (string-search "\\{" doc))))
          (ensure-empty-lines)
          (insert (format "\\{%s}" keymap-sym)))
        (buffer-string)))))

;; define-minor-mode (easy-mmode.el), ported faithfully.
(defmacro define-minor-mode (mode doc &rest body)
  "Define a new minor mode MODE."
  (declare (doc-string 2) (indent defun))
  (let* ((last-message (make-symbol "last-message"))
         (mode-name (symbol-name mode))
         (init-value nil)
         (keymap nil)
         (lighter nil)
	 (pretty-name nil)
	 (globalp nil)
	 (set nil)
	 (initialize nil)
	 (type nil)
	 (extra-args nil)
	 (extra-keywords nil)
         (variable nil)
         (setter `(setq ,mode))
         (getter mode)
         (modefun mode)
	 (after-hook nil)
	 (hook (intern (concat mode-name "-hook")))
	 (hook-on (intern (concat mode-name "-on-hook")))
	 (hook-off (intern (concat mode-name "-off-hook")))
         (interactive t)
         (warnwrap (if (or (null body) (keywordp (car body))) #'identity
                     (lambda (exp)
                       (macroexp-warn-and-return
                        (format-message
                         "Use keywords rather than deprecated positional arguments to `define-minor-mode'")
                        exp))))
	 keyw keymap-sym tmp)
    (unless (keywordp (car body))
      (setq init-value (pop body))
      (unless (keywordp (car body))
        (setq lighter (pop body))
        (unless (keywordp (car body))
          (setq keymap (pop body)))))
    (while (keywordp (setq keyw (car body)))
      (setq body (cdr body))
      (pcase keyw
	(:init-value (setq init-value (pop body)))
	(:lighter (setq lighter (purecopy (pop body))))
	(:global (setq globalp (pop body))
                 (when (and globalp (symbolp mode))
                   (setq setter `(setq-default ,mode))
                   (setq getter `(default-value ',mode))))
	(:extra-args (setq extra-args (pop body)))
	(:set (setq set (list :set (pop body))))
	(:initialize (setq initialize (list :initialize (pop body))))
	(:type (setq type (list :type (pop body))))
	(:keymap (setq keymap (pop body)))
	(:interactive (setq interactive (pop body)))
        (:variable (setq variable (pop body))
                   (if (not (and (setq tmp (cdr-safe variable))
                                 (or (symbolp tmp)
                                     (functionp tmp))))
                       (progn
                         (setq setter `(setf ,variable))
                         (setq getter variable))
                     (setq getter (car variable))
                     (setq setter `(funcall #',(cdr variable)))))
	(:after-hook (setq after-hook (pop body)))
	(_ (push keyw extra-keywords) (push (pop body) extra-keywords))))
    (setq pretty-name (easy-mmode-pretty-mode-name mode lighter))
    (setq keymap-sym (if (and keymap (symbolp keymap)) keymap
		       (intern (concat mode-name "-map"))))
    (unless set (setq set '(:set #'custom-set-minor-mode)))
    (unless initialize
      (setq initialize '(:initialize #'custom-initialize-default)))
    (unless type (setq type '(:type 'boolean)))
    `(progn
       ,(cond
         (variable nil)
         ((not globalp)
          `(progn
             :autoload-end
             (defvar-local ,mode ,init-value
               ,(concat (format "Non-nil if %s is enabled.\n" pretty-name)
                        (internal--format-docstring-line
                         "Use the command `%s' to change this variable." mode)))))
         (t
	  (let ((base-doc-string
                 (concat "Non-nil if %s is enabled.
See the `%s' command
for a description of this minor mode."
                         (if body "
Setting this variable directly does not take effect;
either customize it (see the info node `Easy Customization')
or call the function `%s'."))))
	    `(defcustom ,mode ,init-value
	       ,(format base-doc-string pretty-name mode mode)
	       ,@set
	       ,@initialize
	       ,@type
               ,@(nreverse extra-keywords)))))
       ,(funcall
         warnwrap
         `(defun ,modefun (&optional arg ,@extra-args)
            ,(easy-mmode--mode-docstring doc pretty-name keymap-sym
                                         getter globalp)
            ,(when interactive
               (if (consp interactive)
                   `(interactive
                     (list (if current-prefix-arg
                               (prefix-numeric-value current-prefix-arg)
                             'toggle))
                     ,@interactive)
		 '(interactive
                   (list (if current-prefix-arg
                             (prefix-numeric-value current-prefix-arg)
                           'toggle)))))
	    (let ((,last-message (current-message)))
              (,@setter
               (cond ((eq arg 'toggle)
                      (not ,getter))
                     ((and (numberp arg)
                           (< arg 1))
                      nil)
                     (t
                      t)))
              ,@(if globalp
                    `((when (boundp 'global-minor-modes)
                        (setq global-minor-modes
                              (delq ',modefun global-minor-modes))
                        (when ,getter
                          (push ',modefun global-minor-modes))))
                  `((when (boundp 'local-minor-modes)
                      (setq local-minor-modes
                            (delq ',modefun local-minor-modes))
                      (when ,getter
                        (push ',modefun local-minor-modes)))))
              ,@body
              (run-hooks ',hook (if ,getter ',hook-on ',hook-off))
              (if (called-interactively-p 'any)
                  (progn
                    ,(if (and globalp (not variable))
                         `(customize-mark-as-set ',mode))
                    (unless (and (current-message)
                                 (not (equal ,last-message
                                             (current-message))))
                      (let ((local ,(if globalp "" " in current buffer")))
			(message "%s %sabled%s" ,pretty-name
			         (if ,getter "en" "dis") local)))))
	      ,@(when after-hook `(,after-hook)))
	    (force-mode-line-update)
	    ,getter))
       :autoload-end
       (defvar ,hook nil)
       (unless (get ',hook 'variable-documentation)
         (put ',hook 'variable-documentation
              ,(format "Hook run after entering or leaving `%s'.
No problems result if this variable is not bound.
`add-hook' automatically binds it.  (This is true for all hook variables.)"
                       modefun)))
       (put ',hook 'custom-type 'hook)
       (put ',hook 'standard-value (list nil))
       ,(unless (symbolp keymap)
	  `(defvar ,keymap-sym
	     (let ((m ,keymap))
	       (cond ((keymapp m) m)
		     ((listp m)
                      (easy-mmode-define-keymap m))
		     (t (error "Invalid keymap %S" m))))
	     ,(format "Keymap for `%s'." mode-name)))
       ,(let ((modevar (pcase getter (`(default-value ',v) v) (_ getter))))
          (if (not (symbolp modevar))
              (if (or lighter keymap)
                  (error ":lighter and :keymap unsupported with mode expression %S" getter))
            `(with-no-warnings
               (add-minor-mode ',modevar ',lighter
                               ,(if keymap keymap-sym
                                  `(if (boundp ',keymap-sym) ,keymap-sym))
                               nil
                               ,(unless (eq mode modefun) `',modefun))))))))

;;; ---- ERT: Emacs Lisp Regression Testing (subset) ----
;; Ported from ERT: `should` / `should-not` / `should-error` / `skip-unless`
;; assertions, `ert-fail` / `ert-pass`, and `ert-deftest` with an optional
;; docstring plus `:expected-result` and `:tags` keyword args. Assertions signal
;; `ert-test-failed`; `skip-unless` signals `ert-test-skipped`. The runner
;; classifies each test as pass / fail / skip / expected-fail (XFAIL) /
;; unexpected-pass and returns the number of UNEXPECTED results (0 = all as
;; expected). `ert-run-tests-batch-and-exit` errors out on any unexpected one.

(defvar ert--tests nil)   ; alist of (name . expected-result), :passed | :failed

;; should-failure explanation: for an assertion `(PRED ARG...)`, each ARG is
;; evaluated once and its value reported next to the form on failure — the way
;; ERT explains a failing `should`. Limited to known pure predicates so eager
;; argument evaluation can't change semantics; anything else falls back to a
;; plain truthiness check.
(defvar ert--explain-fns
  '(= /= < > <= >= eq eql equal not null member memq assoc assq
    stringp numberp integerp consp listp atom symbolp zerop))

(defun ert--let-binds (syms args)
  (if (null syms) nil
    (cons (list (car syms) (car args)) (ert--let-binds (cdr syms) (cdr args)))))

(defun ert--value-pairs (args syms)
  (if (null args) nil
    (cons (list 'cons (list 'quote (car args)) (car syms))
          (ert--value-pairs (cdr args) (cdr syms)))))

(defun ert--argsyms (n)
  (let ((i 0) (out nil))
    (while (< i n)
      (setq out (cons (intern (concat "--ert-a" (number-to-string i) "--")) out))
      (setq i (1+ i)))
    (reverse out)))

(defmacro should (form)
  (if (and (consp form) (memq (car form) ert--explain-fns))
      (let* ((fn (car form))
             (args (cdr form))
             (syms (ert--argsyms (length args))))
        `(let ,(ert--let-binds syms args)
           (if (,fn ,@syms) t
             (signal 'ert-test-failed
                     (list 'should ',form :values (list ,@(ert--value-pairs args syms)))))))
    `(if ,form t (signal 'ert-test-failed (list 'should ',form)))))

(defmacro should-not (form)
  `(if ,form (signal 'ert-test-failed (list 'should-not ',form)) t))

(defmacro ert-info (_spec &rest body)
  ;; Compatibility shim: real ERT attaches the info string to failure reports
  ;; (which needs the ERT UI); here `ert-info` simply evaluates its BODY.
  `(progn ,@body))

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

;;; ---- oclosure (Open Closures) ----
;; Faithful port of emacs-lisp/oclosure.el (Emacs 30.2).  An OClosure is a
;; closure that also carries a type (for cl-generic dispatch) and named,
;; optionally-mutable slots reachable from outside via generated accessors.
;;
;; elisprs divergences from oclosure.el (NAMED — the observable API matches the
;; Emacs binary exactly; only the host-specific internals differ):
;;  * The C primitives `oclosure--get/--set/--copy/--fix-type', `oclosure-type'
;;    and `closurep' are Rust builtins (src/builtins.rs), because elisprs closures
;;    are compiled (a fusevm Chunk + captured env), not aref-indexable
;;    interpreted-functions.  Slot values live in the closure's captured lexical
;;    env (the same storage the body reads), so `oclosure--set' and an in-body
;;    `setq' stay mutually visible — exactly as Emacs stores slots in the env.
;;  * `oclosure-define' installs the class AND the accessors/copiers in one
;;    runtime call (`oclosure--define'); Emacs splits this across an
;;    eval-and-compile class registration and a compile-time
;;    `oclosure--define-functions' macro.  Merging is safe because top-level forms
;;    are evaluated in order, so the class is registered before the next form's
;;    macroexpansion (which `oclosure-lambda' needs).
;;  * The cl--class / cl-slot-descriptor substrate (from cl-preloaded.el) is
;;    provided here minimally: the `closure' parent is a plain class object rather
;;    than the full built-in-type hierarchy.
;;  * The cl-print pretty-printer methods for accessors are omitted (cosmetic).

;; -- helpers not yet in the prelude --
(defun macroexp-parse-body (body)
  "Parse a function BODY into (DECLARATIONS . EXPS)."
  (let ((decls ()))
    (while
        (and body
             (let ((e (car body)))
               (or (and (stringp e) (cdr body))
                   (memq (car-safe e)
                         '(:documentation declare interactive cl-declare)))))
      (push (pop body) decls))
    (cons (nreverse decls) body)))

;; macroexp--fgrep (macroexp.el:724): return those of BINDINGS whose var/func
;; symbol appears anywhere in SEXP — a poor-man's free-variable test used by
;; cl-generic to decide whether a method body references `cl-call-next-method'.
;; Faithful port, including the tortoise/hare cycle guard for cyclic data.
(defun macroexp--fgrep (bindings sexp)
  (let ((res '())
        (seen (make-hash-table :test #'eq))
        (sexpss (list (list sexp))))
    (while (and sexpss bindings)
      (let ((sexps (pop sexpss)))
        (unless (gethash sexps seen)
          (puthash sexps t seen)
          (if (vectorp sexps) (setq sexps (mapcar #'identity sexps)))
          (let ((tortoise sexps) (skip t))
            (while sexps
              (let ((sexp (if (consp sexps) (pop sexps)
                            (prog1 sexps (setq sexps nil)))))
                (if skip
                    (setq skip nil)
                  (setq tortoise (cdr tortoise))
                  (if (eq tortoise sexps)
                      (setq sexps nil)
                    (setq skip t)))
                (cond
                 ((or (consp sexp) (vectorp sexp)) (push sexp sexpss))
                 (t
                  (let ((tmp (assq sexp bindings)))
                    (when tmp
                      (push tmp res)
                      (setq bindings (remove tmp bindings))))))))))))
    res))

(defconst cl--lambda-list-keywords
  '(&optional &rest &key &allow-other-keys &aux &whole &body &environment))

(defun cl--arglist-args (args)
  (if (not (listp args)) (list args)
    (let ((res nil) (kind nil) arg)
      (while (consp args)
        (setq arg (pop args))
        (if (memq arg cl--lambda-list-keywords) (setq kind arg)
          (if (eq arg '&cl-defs) (pop args)
            (and (consp arg) kind (setq arg (car arg)))
            (and (consp arg) (cdr arg) (eq kind '&key) (setq arg (cadr arg)))
            (setq res (nconc res (cl--arglist-args arg))))))
      (nconc res (and args (list args))))))

(defun gv-setter (name)
  "Return the symbol where the (setf NAME) function should be placed."
  (intern (format "(setf %s)" name)))

;; -- cl--class / cl-slot-descriptor substrate (subset of cl-preloaded.el) --
(defun cl--find-class (name) (get name 'cl--class))

(cl-defstruct (cl-slot-descriptor
               (:conc-name cl--slot-descriptor-)
               (:constructor nil)
               (:constructor cl--make-slot-descriptor (name &optional initform type props)))
  name initform type props)

(cl-defstruct (cl--class (:constructor nil))
  name docstring parents slots index-table)

(cl-defstruct (oclosure--class (:include cl--class))
  allparents)

;; -- built-in type class registry (faithful port of cl-preloaded.el) --
;; cl-generic's typeof generalizer dispatches on the class DAG registered here:
;; `cl--generic-type-specializers' looks up the class of `cl-type-of' and walks
;; `cl--class-allparents'.  Without this, cl-generic-generalizers signals
;; "Unknown specializer integer" on the built-in-type prefill in cl-generic.el.

;; cl-preloaded.el:299 — the linearised parent list, most specific first.
(defun cl--class-allparents (class)
  (cons (cl--class-name class)
        (merge-ordered-lists (mapcar #'cl--class-allparents
                                     (cl--class-parents class)))))

;; cl-preloaded.el:304 — type descriptors for built-in types.
(cl-defstruct (built-in-class
               (:include cl--class)
               (:noinline t)
               (:constructor nil)
               (:constructor built-in-class--make (name docstring parents))
               (:copier nil))
  "Type descriptors for built-in types.
The `slots' (and hence `index-table') are currently unused."
  )

;; cl-preloaded.el:314 — register a built-in type NAME with PARENTS.
(defmacro cl--define-built-in-type (name parents &optional docstring &rest slots)
  (declare (indent 2) (doc-string 3))
  (unless (listp parents) (setq parents (list parents)))
  (unless (or parents (eq name t))
    (error "Missing parents for %S: %S" name parents))
  (let ((predicate (intern-soft (format
                                 (if (string-match "-" (symbol-name name))
                                     "%s-p" "%sp")
                                 name))))
    (unless (fboundp predicate) (setq predicate nil))
    (while (keywordp (car slots))
      (let ((kw (pop slots)) (val (pop slots)))
        (pcase kw
          (:predicate (setq predicate val))
          (_ (error "Unknown keyword arg: %S" kw)))))
    `(progn
       ,(if predicate `(put ',name 'cl-deftype-satisfies #',predicate)
          nil)
       (put ',name 'cl--class
            (built-in-class--make ',name ,docstring
                                  (mapcar (lambda (type)
                                            (let ((class (get type 'cl--class)))
                                              (unless class
                                                (error "Unknown type: %S" type))
                                              class))
                                          ',parents))))))

;; cl-preloaded.el:353 — like `functionp' but nil for lists and symbols.
(defun cl-functionp (object)
  "Return non-nil if OBJECT is a member of type `function'.
This is like `functionp' except that it returns nil for all lists and symbols,
regardless if `funcall' would accept to call them."
  (memq (cl-type-of object)
        '(primitive-function native-comp-function module-function
          interpreted-function byte-code-function)))

;; cl-preloaded.el:361-472 — the built-in type DAG, parents before children.
(cl--define-built-in-type t nil "Abstract supertype of everything.")
(cl--define-built-in-type atom t "Abstract supertype of anything but cons cells."
                          :predicate atom)
(cl--define-built-in-type tree-sitter-compiled-query atom)
(cl--define-built-in-type tree-sitter-node atom)
(cl--define-built-in-type tree-sitter-parser atom)
(when (fboundp 'user-ptrp)
  (cl--define-built-in-type user-ptr atom nil
                            :predicate user-ptrp))
(cl--define-built-in-type font-object atom)
(cl--define-built-in-type font-entity atom)
(cl--define-built-in-type font-spec atom)
(cl--define-built-in-type condvar atom)
(cl--define-built-in-type mutex atom)
(cl--define-built-in-type thread atom)
(cl--define-built-in-type terminal atom)
(cl--define-built-in-type hash-table atom)
(cl--define-built-in-type frame atom)
(cl--define-built-in-type buffer atom)
(cl--define-built-in-type window atom)
(cl--define-built-in-type process atom)
(cl--define-built-in-type finalizer atom)
(cl--define-built-in-type window-configuration atom)
(cl--define-built-in-type overlay atom)
(cl--define-built-in-type number-or-marker atom
  "Abstract supertype of both `number's and `marker's.")
(cl--define-built-in-type symbol atom
  "Type of symbols."
  (name     symbol-name)
  (value    symbol-value)
  (function symbol-function)
  (plist    symbol-plist))
(cl--define-built-in-type obarray atom)
(cl--define-built-in-type native-comp-unit atom)
(cl--define-built-in-type sequence t "Abstract supertype of sequences.")
(cl--define-built-in-type list sequence)
(cl--define-built-in-type array (sequence atom) "Abstract supertype of arrays.")
(cl--define-built-in-type number (number-or-marker)
  "Abstract supertype of numbers.")
(cl--define-built-in-type float (number))
(cl--define-built-in-type integer-or-marker (number-or-marker)
  "Abstract supertype of both `integer's and `marker's.")
(cl--define-built-in-type integer (number integer-or-marker))
(cl--define-built-in-type marker (integer-or-marker))
(cl--define-built-in-type bignum (integer)
  "Type of those integers too large to fit in a `fixnum'.")
(cl--define-built-in-type fixnum (integer)
  "Type of small (fixed-size) integers.")
(cl--define-built-in-type boolean (symbol)
  "Type of the canonical boolean values, i.e. either nil or t.")
(cl--define-built-in-type symbol-with-pos (symbol)
  "Type of symbols augmented with source-position information.")
(cl--define-built-in-type vector (array))
(cl--define-built-in-type record (atom)
  "Abstract type of objects with slots.")
(cl--define-built-in-type bool-vector (array) "Type of bitvectors.")
(cl--define-built-in-type char-table (array)
  "Type of special arrays that are indexed by characters.")
(cl--define-built-in-type string (array))
(cl--define-built-in-type null (boolean list)
  "Type of the nil value."
  :predicate null)
(cl--define-built-in-type cons (list)
  "Type of cons cells."
  (car car) (cdr cdr))
(cl--define-built-in-type function (atom)
  "Abstract supertype of function values.")
(cl--define-built-in-type compiled-function (function)
  "Abstract type of functions that have been compiled.")
(cl--define-built-in-type closure (function)
  "Abstract type of functions represented by a vector-like object.")
(cl--define-built-in-type byte-code-function (compiled-function closure)
  "Type of functions that have been byte-compiled.")
(cl--define-built-in-type subr (atom)
  "Abstract type of functions compiled to machine code.")
(cl--define-built-in-type module-function (function)
  "Type of functions provided via the module API.")
(cl--define-built-in-type interpreted-function (closure)
  "Type of functions that have not been compiled.")
(cl--define-built-in-type special-form (subr)
  "Type of the core syntactic elements of the Emacs Lisp language.")
(cl--define-built-in-type native-comp-function (subr compiled-function)
  "Type of functions that have been compiled by the native compiler.")
(cl--define-built-in-type primitive-function (subr compiled-function)
  "Type of functions hand written in C.")

;; -- cl-defstruct class registry (faithful subset of cl-preloaded.el) --
;; cl-generic's typeof generalizer also dispatches on cl-defstruct types: it
;; needs `cl--find-class NAME' to return a `cl-structure-class' object.  Every
;; `cl-defstruct' now registers one (see the guarded form emitted by the macro);
;; the two metaclasses below and their parent chain (record → cl-structure-object
;; → cl--class → cl-structure-class) are bootstrapped by hand, matching the DAG
;; `cl--class-allparents' walks (binary-verified against GNU Emacs 30.2).

;; cl-preloaded.el:207 — the type of CL struct descriptors.  Extra slots beyond
;; those inherited from cl--class are kept for layout fidelity though the
;; cluster's load path only reads name/parents.
(cl-defstruct (cl-structure-class
               (:include cl--class)
               (:conc-name cl--struct-class-)
               (:predicate cl--struct-class-p)
               (:constructor nil)
               (:constructor cl--struct-new-class
                (name docstring parents type named slots index-table
                      children-sym tag print))
               (:copier nil))
  "The type of CL structs descriptors."
  (tag nil) (type nil) (named nil) (print nil) (children-sym nil))

;; cl-preloaded.el:232 — the root parent of all "normal" CL structs.
(cl-defstruct (cl-structure-object
               (:predicate cl-struct-p)
               (:constructor nil)
               (:copier nil))
  "The root parent of all \"normal\" CL structs")

;; Bootstrap the metaclass chain by hand (the `cl-defstruct' auto-registration
;; above short-circuits until cl-structure-object is registered).
(put 'cl-structure-object 'cl--class
     (cl--struct-new-class 'cl-structure-object nil (list (cl--find-class 'record))
                           nil nil nil nil nil nil nil))
(put 'cl--class 'cl--class
     (cl--struct-new-class 'cl--class nil (list (cl--find-class 'cl-structure-object))
                           nil nil nil nil nil nil nil))
(put 'cl-structure-class 'cl--class
     (cl--struct-new-class 'cl-structure-class nil (list (cl--find-class 'cl--class))
                           nil nil nil nil nil nil nil))

(defun oclosure--index-table (slotdescs)
  (let ((i -1)
        (it (make-hash-table :test 'eq)))
    (dolist (desc slotdescs)
      (let ((slot (cl--slot-descriptor-name desc)))
        (cl-incf i)
        (when (gethash slot it)
          (error "Duplicate slot name: %S" slot))
        (setf (gethash slot it) i)))
    it))

(defun oclosure--class-make (name docstring slots parents allparents)
  (make-oclosure--class :name name :docstring docstring :parents parents
                        :slots slots :index-table (oclosure--index-table slots)
                        :allparents allparents))

;; The `closure' parent class (minimal), then the `oclosure' root.
(setf (cl--find-class 'closure)
      (make-oclosure--class :name 'closure :allparents '(closure)))
(setf (cl--find-class 'oclosure)
      (oclosure--class-make 'oclosure
                            "The root parent of all OClosure types"
                            nil (list (cl--find-class 'closure))
                            '(oclosure)))

(defun oclosure--p (oclosure)
  (not (not (oclosure-type oclosure))))
(cl-deftype oclosure () '(satisfies oclosure--p))

(defun oclosure--slot-mutable-p (slotdesc)
  (not (alist-get :read-only (cl--slot-descriptor-props slotdesc))))

(defun oclosure--build-class (name docstring parent-names slots)
  (cl-assert (null (cdr parent-names)))
  (let* ((parent-class (let ((pname (or (car parent-names) 'oclosure)))
                         (or (cl--find-class pname)
                             (error "Unknown class: %S" pname))))
         (slotdescs
          (append
           (oclosure--class-slots parent-class)
           (mapcar (lambda (field)
                     (if (not (consp field))
                         (cl--make-slot-descriptor field nil nil
                                                   '((:read-only . t)))
                       (let ((sname (pop field))
                             (type nil)
                             (read-only t)
                             (props '()))
                         (while field
                           (pcase (pop field)
                             (:mutable (setq read-only (not (car field))))
                             (:type (setq type (car field)))
                             (p (message "Unknown property: %S" p)
                                (push (cons p (car field)) props)))
                           (setq field (cdr field)))
                         (cl--make-slot-descriptor sname nil type
                                                   `((:read-only . ,read-only)
                                                     ,@props)))))
                   slots))))
    (oclosure--class-make name docstring slotdescs
                          (if (cdr parent-names)
                              (oclosure--class-parents parent-class)
                            (list parent-class))
                          (cons name (oclosure--class-allparents
                                      parent-class)))))

(defun oclosure--defstruct-make-copiers (copiers slotdescs name)
  (let* ((mutables '())
         (slots (mapcar
                 (lambda (desc)
                   (let ((sname (cl--slot-descriptor-name desc)))
                     (when (oclosure--slot-mutable-p desc)
                       (push sname mutables))
                     sname))
                 slotdescs)))
    (mapcar
     (lambda (copier)
       (pcase-let*
           ((cname (pop copier))
            (args (or (pop copier) `(&key ,@slots)))
            (inline (and (eq :inline (car copier)) (pop copier)))
            (doc (or (pop copier)
                     (format "Copier for objects of type `%s'." name)))
            (obj (make-symbol "obj"))
            (absent (make-symbol "absent"))
            (anames (cl--arglist-args args))
            (mnames
             (let ((res '()) (tmp args))
               (while (and tmp (not (memq (car tmp) cl--lambda-list-keywords)))
                 (push (pop tmp) res))
               res))
            (has-kw (let ((k nil))
                      (dolist (a args)
                        (when (memq a cl--lambda-list-keywords) (setq k t)))
                      k))
            (index -1)
            (mutlist '())
            (argvals
             (mapcar
              (lambda (slot)
                (setq index (1+ index))
                (let* ((mutable (memq slot mutables))
                       (get `(oclosure--get ,obj ,index ,(not (not mutable)))))
                  (push mutable mutlist)
                  (cond
                   ((not (memq slot anames)) get)
                   ((memq slot mnames) slot)
                   (t `(if (eq ',absent ,slot) ,get ,slot)))))
              slots)))
         `(,(if inline 'cl-defsubst 'cl-defun) ,cname
           ,(if has-kw `(&cl-defs (',absent) ,obj ,@args) `(,obj ,@args))
           ,doc
           (oclosure--copy ,obj ',(if (remq nil mutlist) (nreverse mutlist))
                           ,@argvals))))
     copiers)))

(defun oclosure--install-functions (name copiers)
  (let* ((class (cl--find-class name))
         (slotdescs (oclosure--class-slots class))
         (i -1))
    (dolist (desc slotdescs)
      (setq i (1+ i))
      (let* ((slot (cl--slot-descriptor-name desc))
             (mutable (oclosure--slot-mutable-p desc))
             (aname (intern (format "%S--%S" name slot))))
        (if (not mutable)
            (defalias aname
              (oclosure--copy oclosure--accessor-prototype nil name slot i))
          (defalias aname
            (oclosure--accessor-copy oclosure--mut-getter-prototype name slot i))
          (defalias (gv-setter aname)
            (oclosure--accessor-copy oclosure--mut-setter-prototype name slot i)))))
    (dolist (form (oclosure--defstruct-make-copiers copiers slotdescs name))
      (eval form t))))

(defun oclosure--define (name docstring parent-names slots &rest props)
  (let* ((class (oclosure--build-class name docstring parent-names slots))
         (pred (lambda (oclosure)
                 (let ((type (oclosure-type oclosure)))
                   (when type
                     (memq name (oclosure--class-allparents
                                 (cl--find-class type)))))))
         (predname (or (plist-get props :predicate)
                       (intern (format "%s--internal-p" name)))))
    (setf (cl--find-class name) class)
    (dolist (slot (oclosure--class-slots class))
      (put (cl--slot-descriptor-name slot) 'slot-name t))
    (defalias predname pred)
    (put name 'cl-deftype-satisfies predname)
    (oclosure--install-functions name (plist-get props :copiers))
    name))

(defmacro oclosure-define (name &optional docstring &rest slots)
  "Define a new OClosure type."
  (declare (doc-string 2) (indent 1))
  (unless (or (stringp docstring) (null docstring))
    (push docstring slots)
    (setq docstring nil))
  (let* ((options (when (consp name)
                    (prog1 (copy-sequence (cdr name))
                      (setq name (car name)))))
         (get-opt (lambda (opt &optional all)
                    (let ((val (assq opt options))
                          tmp)
                      (when val (setq options (delq val options)))
                      (if (not all)
                          (cdr val)
                        (when val
                          (setq val (list (cdr val)))
                          (while (setq tmp (assq opt options))
                            (push (cdr tmp) val)
                            (setq options (delq tmp options)))
                          (nreverse val))))))
         (predicate (car (funcall get-opt :predicate)))
         (parent-names (or (funcall get-opt :parent)
                           (funcall get-opt :include)))
         (copiers (funcall get-opt :copier 'all)))
    `(oclosure--define ',name ,docstring ',parent-names ',slots
                       ,@(when predicate `(:predicate ',predicate))
                       :copiers ',copiers)))

(defmacro oclosure--lambda (type bindings mutables args &rest body)
  "Low level construction of an OClosure object."
  (let* ((parsed (macroexp-parse-body body))
         (prebody (car parsed))
         (realbody (cdr parsed))
         (slotnames (mapcar #'car bindings)))
    `(let ,(mapcar (lambda (bind)
                     (if (cdr bind) bind
                       `(,(car bind) (progn nil))))
                   (reverse bindings))
       (oclosure--fix-type ,type ',slotnames ',mutables
         (lambda ,args
           ,@prebody
           (if t nil ,@slotnames
               ,@(mapcar (lambda (m) `(setq ,m ,m)) mutables))
           ,@realbody)))))

(defmacro oclosure-lambda (type-and-slots args &rest body)
  "Define anonymous OClosure function."
  (declare (indent 2))
  (let* ((type (car type-and-slots))
         (fields (cdr type-and-slots))
         (class (or (cl--find-class type)
                    (error "Unknown class: %S" type)))
         (slots (oclosure--class-slots class))
         (mutables '())
         (slotbinds (mapcar (lambda (slot)
                              (let ((sname (cl--slot-descriptor-name slot)))
                                (when (oclosure--slot-mutable-p slot)
                                  (push sname mutables))
                                (list sname)))
                            slots))
         (tempbinds (mapcar
                     (lambda (field)
                       (let* ((fname (car field))
                              (bind (assq fname slotbinds)))
                         (cond
                          ((not bind) (error "Unknown slot: %S" fname))
                          ((cdr bind) (error "Duplicate slot: %S" fname))
                          (t (let ((temp (gensym "temp")))
                               (setcdr bind (list temp))
                               (cons temp (cdr field)))))))
                     fields)))
    `(let ,tempbinds
       (oclosure--lambda ',type ,slotbinds ,mutables ,args ,@body))))

(defun oclosure--slot-index (oclosure slotname)
  (gethash slotname
           (oclosure--class-index-table
            (cl--find-class (oclosure-type oclosure)))))

(defun oclosure--slot-value (oclosure slotname)
  (let ((class (cl--find-class (oclosure-type oclosure)))
        (index (oclosure--slot-index oclosure slotname)))
    (oclosure--get oclosure index
                   (oclosure--slot-mutable-p
                    (nth index (oclosure--class-slots class))))))

(defun oclosure--set-slot-value (oclosure slotname value)
  (let ((class (cl--find-class (oclosure-type oclosure)))
        (index (oclosure--slot-index oclosure slotname)))
    (unless (oclosure--slot-mutable-p
             (nth index (oclosure--class-slots class)))
      (signal 'setting-constant (list oclosure slotname)))
    (oclosure--set value oclosure index)))

;; Accessor prototype + the `accessor'/`oclosure-accessor' types (bootstrapped
;; exactly like oclosure.el).
(defconst oclosure--accessor-prototype
  (oclosure--lambda 'oclosure-accessor ((type) (slot) (index)) nil
    (oclosure) (oclosure--get oclosure index nil)))

(oclosure-define accessor
  "OClosure function to access a specific slot of an object."
  type slot)

(oclosure-define (oclosure-accessor
                  (:parent accessor)
                  (:copier oclosure--accessor-copy (type slot index)))
  "OClosure function to access a specific slot of an OClosure function."
  index)

(defconst oclosure--mut-getter-prototype
  (oclosure-lambda (oclosure-accessor (type) (slot) (index)) (oclosure)
    (oclosure--get oclosure index t)))
(defconst oclosure--mut-setter-prototype
  (oclosure-lambda (oclosure-accessor (type) (slot) (index)) (val oclosure)
    (oclosure--set val oclosure index)))

(oclosure-define (save-some-buffers-function
                  (:predicate save-some-buffers-function--p)))

(oclosure-define (cconv--interactive-helper) fun if)
(defun cconv--interactive-helper (fun if)
  "Add interactive \"form\" IF to FUN."
  (oclosure-lambda (cconv--interactive-helper (fun fun) (if if))
      (&rest args)
    (apply (if (called-interactively-p 'any)
               #'funcall-interactively #'funcall)
           fun args)))

(provide 'oclosure)

;;; ---- coding systems (predicate/registry subset) ----
;; Faithful to GNU Emacs 30.2's built-in coding-system registry. Only the
;; registration/predicate surface is ported: `coding-system-p', `coding-system-base',
;; `coding-system-list', `check-coding-system'.  The actual encode/decode machinery
;; (define-coding-system-internal and the charset codecs) is not implemented here.
;; Registry data captured value-for-value from `emacs -Q --batch' via
;; (coding-system-list) and (coding-system-list t).
(define-error 'coding-system-error "Invalid coding system")

(defconst coding-system--all
  '(
    binary no-conversion undecided prefer-utf-8 raw-text no-conversion-multibyte latin-1 iso-8859-1
    iso-latin-1 emacs-mule cp65001 mule-utf-8 utf-8 utf-8-with-signature utf-8-auto utf-8-emacs
    utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16 iso-2022-7bit
    iso-2022-7bit-ss2 iso-2022-int-1 iso-2022-7bit-lock iso-2022-cjk iso-2022-7bit-lock-ss2 iso-2022-8bit-ss2 ctext x-ctext
    compound-text ctext-no-compositions ctext-with-extensions x-ctext-with-extensions compound-text-with-extensions ascii iso-safe us-ascii
    utf-7 utf-7-imap chinese-iso-7bit iso-2022-cn iso-2022-cn-ext gb2312 cn-gb euc-cn
    euc-china cn-gb-2312 chinese-iso-8bit hz hz-gb-2312 chinese-hz cp950 cn-big5
    big5 chinese-big5 cn-big5-hkscs big5-hkscs chinese-big5-hkscs euc-taiwan euc-tw windows-936
    cp936 gbk chinese-gbk gb18030 chinese-gb18030 iso-8859-5 cyrillic-iso-8bit cp878
    koi8 koi8-r cyrillic-koi8 koi8-u alternativnyj cyrillic-alternativnyj cp866 koi8-t
    cp1251 windows-1251 cp866u ruscii cp1125 ibm855 cp855 mik
    pt154 devanagari in-is13194-devanagari ebcdic-us ebcdic-uk cp1047 ibm1047 cp038
    ebcdic-int ibm038 latin-2 iso-8859-2 iso-latin-2 latin-3 iso-8859-3 iso-latin-3
    latin-4 iso-8859-4 iso-latin-4 latin-5 iso-8859-9 iso-latin-5 latin-6 iso-8859-10
    iso-latin-6 latin-7 iso-8859-13 iso-latin-7 latin-8 iso-8859-14 iso-latin-8 latin-0
    latin-9 iso-8859-15 iso-latin-9 cp1250 windows-1250 cp1252 windows-1252 cp1254
    windows-1254 cp1257 windows-1257 cp256 ebcdic-int1 ibm256 cp273 ibm273
    cp274 ebcdic-be ibm274 cp275 ebcdic-br ibm275 cp277 ebcdic-cp-no
    ebcdic-cp-dk ibm277 cp278 ebcdic-cp-se ebcdic-cp-fi ibm278 cp280 ebcdic-cp-it
    ibm280 cp284 ebcdic-cp-es ibm284 cp285 ebcdic-cp-gb ibm285 cp297
    ebcdic-cp-fr ibm297 ibm775 cp775 ibm850 cp850 ibm852 cp852
    ibm857 cp857 cp858 ibm860 cp860 ibm861 cp861 ibm863
    cp863 ibm865 cp865 ibm437 cp437 macintosh mac-roman next
    roman8 hp-roman8 adobe-standard-encoding latin-10 iso-8859-16 iso-latin-10 iso-8859-7 greek-iso-8bit
    cp1253 windows-1253 cp737 ibm851 cp851 ibm869 cp869 iso-8859-8-i
    iso-8859-8-e iso-8859-8 hebrew-iso-8bit cp1255 windows-1255 ibm862 cp862 junet
    iso-2022-jp iso-2022-jp-2 sjis shift_jis japanese-shift-jis cp932 japanese-cp932 old-jis
    iso-2022-jp-1978-irv japanese-iso-7bit-1978-irv euc-jp euc-japan euc-japan-1990 japanese-iso-8bit eucjp-ms iso-2022-jp-3
    iso-2022-jp-2004 euc-jisx0213 euc-jis-2004 shift_jis-2004 japanese-shift-jis-2004 cp281 ebcdic-jp-e ibm281
    cp290 ebcdic-jp-kana ibm290 ks_c_5601-1987 euc-korea euc-kr korean-iso-8bit korean-iso-7bit-lock
    iso-2022-kr cp949 korean-cp949 lao tis-620 tis620 th-tis620 thai-tis620
    ibm874 cp874 iso-8859-11 tibetan tibetan-iso-8bit viscii vietnamese-viscii tcvn-5712
    tcvn vietnamese-tcvn vscii vietnamese-vscii viqr vietnamese-viqr cp1258 windows-1258
    iso-8859-6 cp1256 windows-1256 georgian-ps georgian-academy
    )
  "All built-in coding-system names (base systems and aliases), GNU Emacs 30.2.")

(defconst coding-system--alias-alist
  '(
    (binary . no-conversion) (latin-1 . iso-latin-1) (iso-8859-1 . iso-latin-1) (cp65001 . utf-8)
    (mule-utf-8 . utf-8) (utf-16-le . utf-16le-with-signature) (utf-16-be . utf-16be-with-signature) (iso-2022-int-1 . iso-2022-7bit-lock)
    (iso-2022-cjk . iso-2022-7bit-lock-ss2) (ctext . compound-text) (x-ctext . compound-text) (ctext-with-extensions . compound-text-with-extensions)
    (x-ctext-with-extensions . compound-text-with-extensions) (ascii . us-ascii) (iso-safe . us-ascii) (chinese-iso-7bit . iso-2022-cn)
    (gb2312 . chinese-iso-8bit) (cn-gb . chinese-iso-8bit) (euc-cn . chinese-iso-8bit) (euc-china . chinese-iso-8bit)
    (cn-gb-2312 . chinese-iso-8bit) (hz . chinese-hz) (hz-gb-2312 . chinese-hz) (cp950 . chinese-big5)
    (cn-big5 . chinese-big5) (big5 . chinese-big5) (cn-big5-hkscs . chinese-big5-hkscs) (big5-hkscs . chinese-big5-hkscs)
    (euc-taiwan . euc-tw) (windows-936 . chinese-gbk) (cp936 . chinese-gbk) (gbk . chinese-gbk)
    (gb18030 . chinese-gb18030) (iso-8859-5 . cyrillic-iso-8bit) (cp878 . cyrillic-koi8) (koi8 . cyrillic-koi8)
    (koi8-r . cyrillic-koi8) (alternativnyj . cyrillic-alternativnyj) (cp1251 . windows-1251) (cp866u . cp1125)
    (ruscii . cp1125) (ibm855 . cp855) (devanagari . in-is13194-devanagari) (cp1047 . ibm1047)
    (cp038 . ibm038) (ebcdic-int . ibm038) (latin-2 . iso-latin-2) (iso-8859-2 . iso-latin-2)
    (latin-3 . iso-latin-3) (iso-8859-3 . iso-latin-3) (latin-4 . iso-latin-4) (iso-8859-4 . iso-latin-4)
    (latin-5 . iso-latin-5) (iso-8859-9 . iso-latin-5) (latin-6 . iso-latin-6) (iso-8859-10 . iso-latin-6)
    (latin-7 . iso-latin-7) (iso-8859-13 . iso-latin-7) (latin-8 . iso-latin-8) (iso-8859-14 . iso-latin-8)
    (latin-0 . iso-latin-9) (latin-9 . iso-latin-9) (iso-8859-15 . iso-latin-9) (cp1250 . windows-1250)
    (cp1252 . windows-1252) (cp1254 . windows-1254) (cp1257 . windows-1257) (cp256 . ibm256)
    (ebcdic-int1 . ibm256) (cp273 . ibm273) (cp274 . ibm274) (ebcdic-be . ibm274)
    (cp275 . ibm275) (ebcdic-br . ibm275) (cp277 . ibm277) (ebcdic-cp-no . ibm277)
    (ebcdic-cp-dk . ibm277) (cp278 . ibm278) (ebcdic-cp-se . ibm278) (ebcdic-cp-fi . ibm278)
    (cp280 . ibm280) (ebcdic-cp-it . ibm280) (cp284 . ibm284) (ebcdic-cp-es . ibm284)
    (cp285 . ibm285) (ebcdic-cp-gb . ibm285) (cp297 . ibm297) (ebcdic-cp-fr . ibm297)
    (ibm775 . cp775) (ibm850 . cp850) (ibm852 . cp852) (ibm857 . cp857)
    (ibm860 . cp860) (ibm861 . cp861) (ibm863 . cp863) (ibm865 . cp865)
    (ibm437 . cp437) (macintosh . mac-roman) (roman8 . hp-roman8) (latin-10 . iso-latin-10)
    (iso-8859-16 . iso-latin-10) (iso-8859-7 . greek-iso-8bit) (cp1253 . windows-1253) (ibm851 . cp851)
    (ibm869 . cp869) (iso-8859-8-i . hebrew-iso-8bit) (iso-8859-8-e . hebrew-iso-8bit) (iso-8859-8 . hebrew-iso-8bit)
    (cp1255 . windows-1255) (ibm862 . cp862) (junet . iso-2022-jp) (sjis . japanese-shift-jis)
    (shift_jis . japanese-shift-jis) (cp932 . japanese-cp932) (old-jis . japanese-iso-7bit-1978-irv) (iso-2022-jp-1978-irv . japanese-iso-7bit-1978-irv)
    (euc-jp . japanese-iso-8bit) (euc-japan . japanese-iso-8bit) (euc-japan-1990 . japanese-iso-8bit) (iso-2022-jp-3 . iso-2022-jp-2004)
    (euc-jisx0213 . euc-jis-2004) (shift_jis-2004 . japanese-shift-jis-2004) (cp281 . ibm281) (ebcdic-jp-e . ibm281)
    (cp290 . ibm290) (ebcdic-jp-kana . ibm290) (ks_c_5601-1987 . korean-iso-8bit) (euc-korea . korean-iso-8bit)
    (euc-kr . korean-iso-8bit) (korean-iso-7bit-lock . iso-2022-kr) (cp949 . korean-cp949) (tis-620 . thai-tis620)
    (tis620 . thai-tis620) (th-tis620 . thai-tis620) (ibm874 . cp874) (tibetan . tibetan-iso-8bit)
    (viscii . vietnamese-viscii) (tcvn-5712 . vietnamese-vscii) (tcvn . vietnamese-vscii) (vietnamese-tcvn . vietnamese-vscii)
    (vscii . vietnamese-vscii) (viqr . vietnamese-viqr) (cp1258 . windows-1258) (cp1256 . windows-1256)
    )
  "Alist mapping each alias coding-system to its base coding-system, GNU Emacs 30.2.")

(defun coding-system--strip-eol (name)
  "Strip one trailing -unix/-dos/-mac EOL suffix from string NAME."
  (cond ((string-suffix-p "-unix" name) (substring name 0 -5))
        ((string-suffix-p "-dos" name) (substring name 0 -4))
        ((string-suffix-p "-mac" name) (substring name 0 -4))
        (t name)))

(defun coding-system-p (object)
  "Return t if OBJECT is nil or a coding system.
See the documentation of `define-coding-system' for information
about coding-system objects."
  (or (null object)
      (and (symbolp object)
           (and (memq (intern (coding-system--strip-eol (symbol-name object)))
                      coding-system--all)
                t))))

(defun coding-system-base (coding-system)
  "Return the base of CODING-SYSTEM.
Any alias or end-of-line variant resolves to its base coding system.
If CODING-SYSTEM is invalid, signal a `coding-system-error'."
  (if (null coding-system)
      'no-conversion
    (progn
      (check-coding-system coding-system)
      (let* ((base (intern (coding-system--strip-eol (symbol-name coding-system))))
             (resolved (assq base coding-system--alias-alist)))
        (if resolved (cdr resolved) base)))))

(defun coding-system-list (&optional base-only)
  "Return a list of all existing non-subsidiary coding systems.
If optional arg BASE-ONLY is non-nil, only base coding systems are listed."
  (if base-only
      (let ((result nil))
        (dolist (cs coding-system--all)
          (unless (assq cs coding-system--alias-alist)
            (push cs result)))
        (nreverse result))
    (copy-sequence coding-system--all)))

(defun check-coding-system (coding-system)
  "Check validity of CODING-SYSTEM.
If valid, return CODING-SYSTEM, else signal a `coding-system-error'."
  (if (coding-system-p coding-system)
      coding-system
    (signal 'coding-system-error (list coding-system))))

(defconst coding-system--eol-fixed
  '(no-conversion no-conversion-multibyte binary)
  "Registered coding systems whose EOL type is fixed to `unix' (0), GNU Emacs 30.2.
These have no -dos/-mac subsidiary variants, so `coding-system-eol-type'
returns the integer 0 for them rather than a vector, and no subsidiary
NAME-dos/NAME-mac coding system exists.")

(defun coding-system-eol-type (coding-system)
  "Return the EOL (end-of-line) conversion type of CODING-SYSTEM.
The value is an integer 0, 1, or 2 when CODING-SYSTEM has a fixed EOL
type (`unix', `dos', or `mac' respectively), or a vector
[BASE-unix BASE-dos BASE-mac] of the three subsidiary coding systems
when the EOL type is undecided.  BASE is the base of CODING-SYSTEM.
CODING-SYSTEM nil is treated as `no-conversion'.
Return nil if CODING-SYSTEM is not a valid coding system."
  (when (null coding-system)
    (setq coding-system 'no-conversion))
  (and (symbolp coding-system)
       (let ((name (symbol-name coding-system)))
         (cond
          ;; A directly registered base or alias (no EOL suffix to strip).
          ((memq coding-system coding-system--all)
           (if (memq coding-system coding-system--eol-fixed)
               0
             (let ((base (symbol-name (coding-system-base coding-system))))
               (vector (intern (concat base "-unix"))
                       (intern (concat base "-dos"))
                       (intern (concat base "-mac"))))))
          ;; A subsidiary NAME-unix/-dos/-mac of an EOL-undecided system.
          (t
           (let ((stem (coding-system--strip-eol name)))
             (and (not (string= stem name))
                  (let ((base (intern stem)))
                    (and (memq base coding-system--all)
                         (not (memq base coding-system--eol-fixed))
                         (cond ((string-suffix-p "-unix" name) 0)
                               ((string-suffix-p "-dos" name) 1)
                               (t 2)))))))))))

(defun coding-system-change-eol-conversion (coding-system eol-type)
  "Return a coding system which differs from CODING-SYSTEM in EOL conversion.
The returned coding system converts end-of-line by EOL-TYPE
but text as the same way as CODING-SYSTEM.
EOL-TYPE should be `unix', `dos', `mac', or nil.
If EOL-TYPE is nil, the returned coding system detects
how end-of-line is formatted automatically while decoding.

EOL-TYPE can be specified by an integer 0, 1, or 2.
They means `unix', `dos', and `mac' respectively."
  (if (symbolp eol-type)
      (setq eol-type (cond ((eq eol-type 'unix) 0)
                           ((eq eol-type 'dos) 1)
                           ((eq eol-type 'mac) 2)
                           (t eol-type))))
  ;; We call `coding-system-base' before `coding-system-eol-type',
  ;; because the coding-system may not be initialized until then.
  (let* ((base (coding-system-base coding-system))
         (orig-eol-type (coding-system-eol-type coding-system)))
    (cond ((vectorp orig-eol-type)
           (if (not eol-type)
               coding-system
             (aref orig-eol-type eol-type)))
          ((not eol-type)
           base)
          ((= eol-type orig-eol-type)
           coding-system)
          ((progn (setq orig-eol-type (coding-system-eol-type base))
                  (vectorp orig-eol-type))
           (aref orig-eol-type eol-type)))))

;;; ---- language environments (mule-cmds.el data surface) ----
;; Faithful port of GNU Emacs 30.2's `language-info-alist' machinery: the
;; registry that `set-language-info-alist' populates when a language/*.el file
;; declares a language environment.  `bindings--define-key' (bindings.el) and
;; `define-key-after' (subr.el) are the two supporting keymap helpers these
;; functions need for their menu-map bookkeeping; both are copied verbatim.
;; The heavy `set-language-environment*' switch functions are NOT ported here —
;; they only fire when a definition targets the CURRENT environment; declaring a
;; new environment (the load-time case) never touches them.  Bodies are verbatim
;; from mule-cmds.el / subr.el / bindings.el.

(defun bindings--define-key (map key item)
  "Define KEY in keymap MAP according to ITEM from a menu.
This is like `define-key', but it takes the definition from the
specified menu item, and makes pure copies of as much as possible
of the menu's data."
  (declare (indent 2))
  (define-key map key
    (cond
     ((not (consp item)) item)
     ((keymapp item) item)
     ((stringp (car item))
      (if (keymapp (cdr item))
          (cons (purecopy (car item)) (cdr item))
        (purecopy item)))
     ((eq 'menu-item (car item))
      (if (keymapp (nth 2 item))
          `(menu-item ,(purecopy (nth 1 item)) ,(nth 2 item)
                      ,@(purecopy (nthcdr 3 item)))
        (purecopy item)))
     (t (message "non-menu-item: %S" item) item))))

(defun define-key-after (keymap key definition &optional after)
  "Add binding in KEYMAP for KEY => DEFINITION, right after AFTER's binding.
This is like `define-key' except that the binding for KEY is placed
just after the binding for the event AFTER, instead of at the beginning
of the map.  Note that AFTER must be an event type (like KEY), NOT a command
\(like DEFINITION).

If AFTER is t or omitted, the new binding goes at the end of the keymap.
AFTER should be a single event type--a symbol or a character, not a sequence.

Bindings are always added before any inherited map.

The order of bindings in a keymap matters only when it is used as
a menu, so this function is not useful for non-menu keymaps."
  (declare (indent defun))
  (unless after (setq after t))
  (or (keymapp keymap)
      (signal 'wrong-type-argument (list 'keymapp keymap)))
  (setq key
	(if (<= (length key) 1) (aref key 0)
	  (setq keymap (lookup-key keymap
				   (apply #'vector
					  (butlast (mapcar #'identity key)))))
	  (aref key (1- (length key)))))
  (let ((tail keymap) done inserted)
    (while (and (not done) tail)
      ;; Delete any earlier bindings for the same key.
      (if (eq (car-safe (car (cdr tail))) key)
	  (setcdr tail (cdr (cdr tail))))
      ;; If we hit an included map, go down that one.
      (if (keymapp (car tail)) (setq tail (car tail)))
      ;; When we reach AFTER's binding, insert the new binding after.
      ;; If we reach an inherited keymap, insert just before that.
      ;; If we reach the end of this keymap, insert at the end.
      (if (or (and (eq (car-safe (car tail)) after)
		   (not (eq after t)))
	      (eq (car (cdr tail)) 'keymap)
	      (null (cdr tail)))
	  (progn
	    ;; Stop the scan only if we find a parent keymap.
	    ;; Keep going past the inserted element
	    ;; so we can delete any duplications that come later.
	    (if (eq (car (cdr tail)) 'keymap)
		(setq done t))
	    ;; Don't insert more than once.
	    (or inserted
		(setcdr tail (cons (cons key definition) (cdr tail))))
	    (setq inserted t)))
      (setq tail (cdr tail)))))

(defvar language-info-alist nil
  "Alist of language environment definitions.
Each element looks like:
	(LANGUAGE-NAME . ((KEY . INFO) ...))
where LANGUAGE-NAME is a string, the name of the language environment,
KEY is a symbol denoting the kind of information, and
INFO is the data associated with KEY.")

(defvar current-language-environment "English"
  "The last language environment specified with `set-language-environment'.")

(defvar describe-language-environment-map
  (let ((map (make-sparse-keymap "Describe Language Environment")))
    (bindings--define-key map
      [Default] '(menu-item "Default" describe-specified-language-support))
    map))

(defvar setup-language-environment-map
  (let ((map (make-sparse-keymap "Set Language Environment")))
    (bindings--define-key map
      [Default] '(menu-item "Default" setup-specified-language-environment))
    map))

(defun get-language-info (lang-env key)
  "Return information listed under KEY for language environment LANG-ENV.
KEY is a symbol denoting the kind of information.
For a list of useful values for KEY and their meanings,
see `language-info-alist'."
  (if (symbolp lang-env)
      (setq lang-env (symbol-name lang-env)))
  (let ((lang-slot (assoc-string lang-env language-info-alist t)))
    (if lang-slot
	(cdr (assq key (cdr lang-slot))))))

(defun set-language-info (lang-env key info)
  "Modify part of the definition of language environment LANG-ENV.
Specifically, this stores the information INFO under KEY
in the definition of this language environment.
KEY is a symbol denoting the kind of information.
INFO is the value for that information.

For a list of useful values for KEY and their meanings,
see `language-info-alist'."
  (if (symbolp lang-env)
      (setq lang-env (symbol-name lang-env)))
  (set-language-info-internal lang-env key info)
  (if (equal lang-env current-language-environment)
      (cond ((eq key 'coding-priority)
	     (set-language-environment-coding-systems lang-env)
	     (set-language-environment-charset lang-env))
	    ((eq key 'input-method)
	     (set-language-environment-input-method lang-env))
	    ((eq key 'nonascii-translation)
	     (set-language-environment-nonascii-translation lang-env))
	    ((eq key 'charset)
	     (set-language-environment-charset lang-env)))))

(defun set-language-info-internal (lang-env key info)
  "Internal use only.
Arguments are the same as `set-language-info'."
  (let (lang-slot key-slot)
    (setq lang-slot (assoc lang-env language-info-alist))
    (if (null lang-slot)		; If no slot for the language, add it.
	(setq lang-slot (list lang-env)
	      language-info-alist (cons lang-slot language-info-alist)))
    (setq key-slot (assq key lang-slot))
    (if (null key-slot)			; If no slot for the key, add it.
	(progn
	  (setq key-slot (list key))
	  (setcdr lang-slot (cons key-slot (cdr lang-slot)))))
    (setcdr key-slot (purecopy info))
    ;; Update the custom-type of `current-language-environment'.
    (put 'current-language-environment 'custom-type
	 (cons 'choice (mapcar
			(lambda (lang)
			  (list 'const lang))
			(sort (mapcar 'car language-info-alist) 'string<))))))

(defun set-language-info-setup-keymap (lang-env alist describe-map setup-map)
  "Setup menu items for LANG-ENV.
See `set-language-info-alist' for details of other arguments."
  (let ((doc (assq 'documentation alist)))
    (when doc
      (define-key-after describe-map (vector (intern lang-env))
	(cons lang-env 'describe-specified-language-support))))
  (define-key-after setup-map (vector (intern lang-env))
    (cons lang-env 'setup-specified-language-environment)))

(defun set-language-info-alist (lang-env alist &optional parents)
  "Store ALIST as the definition of language environment LANG-ENV.
ALIST is an alist of KEY and INFO values.  See the documentation of
`language-info-alist' for the meanings of KEY and INFO.

Optional arg PARENTS is a list of parent menu names; it specifies
where to put this language environment in the
Describe Language Environment and Set Language Environment menus.
For example, (\"European\") means to put this language environment
in the European submenu in each of those two menus."
  (cond ((symbolp lang-env)
	 (setq lang-env (symbol-name lang-env)))
	((stringp lang-env)
	 (setq lang-env (purecopy lang-env))))
  (if parents
      (while parents
	(let (describe-map setup-map parent-symbol parent prompt)
	  (if (symbolp (setq parent-symbol (car parents)))
	      (setq parent (symbol-name parent))
	    (setq parent parent-symbol parent-symbol (intern parent)))
	  (setq describe-map (lookup-key describe-language-environment-map
                                         (vector parent-symbol)))
	  ;; This prompt string is for define-prefix-command, so
	  ;; that the map it creates will be suitable for a menu.
	  (or describe-map (setq prompt (format "%s Environment" parent)))
	  (unless describe-map
	    (setq describe-map (intern (format "describe-%s-environment-map"
					       (downcase parent))))
	    (define-prefix-command describe-map nil prompt)
	    (define-key-after
              describe-language-environment-map
              (vector parent-symbol) (cons parent describe-map)))
	  (setq setup-map (lookup-key setup-language-environment-map
                                      (vector parent-symbol)))
	  (unless setup-map
	    (setq setup-map (intern (format "setup-%s-environment-map"
                                            (downcase parent))))
	    (define-prefix-command setup-map nil prompt)
	    (define-key-after
              setup-language-environment-map
              (vector parent-symbol) (cons parent setup-map)))
	  (setq parents (cdr parents))
          (set-language-info-setup-keymap
           lang-env alist
           (symbol-value describe-map) (symbol-value setup-map))))
    (set-language-info-setup-keymap
     lang-env alist
     describe-language-environment-map setup-language-environment-map))
  (dolist (elt alist)
    (set-language-info-internal lang-env (car elt) (cdr elt)))
  (if (equal lang-env current-language-environment)
      (set-language-environment lang-env)))
"#;
