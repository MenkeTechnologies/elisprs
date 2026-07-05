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
(defun max (x &rest xs)
  (while xs
    (let ((y (car xs)))
      (if (> y x) (setq x y))
      (if (and (floatp y) (isnan y)) (setq x y)))
    (setq xs (cdr xs)))
  x)
(defun min (x &rest xs)
  (while xs
    (let ((y (car xs)))
      (if (< y x) (setq x y))
      (if (and (floatp y) (isnan y)) (setq x y)))
    (setq xs (cdr xs)))
  x)
;; `mod` is a primitive subr (handles float operands + divisor-sign semantics).
(defun /= (a b) (not (= a b)))
(defun plusp (x) (> x 0))
(defun minusp (x) (< x 0))
(defun cl-plusp (x) (> x 0))
(defun cl-minusp (x) (< x 0))
(defun evenp (x) (zerop (% x 2)))
(defun oddp (x) (not (zerop (% x 2))))
(defun natnump (x) (and (integerp x) (>= x 0)))
(defun fixnump (x) (integerp x))
(defun bignump (_x) nil)
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
  (if (or (null n) (= n 1))
      (progn (while (consp (cdr l)) (setq l (cdr l))) l)
    (nthcdr (max 0 (- (length l) n)) l)))
(defun make-list (n x) (let ((r nil)) (while (> n 0) (setq r (cons x r)) (setq n (1- n))) r))
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
(defun assq (k l) (let ((r nil)) (while (and l (not r)) (if (eq (caar l) k) (setq r (car l)) (setq l (cdr l)))) r))
(defun assoc (k l &optional testfn)
  (let ((r nil))
    (while (and l (not r))
      (if (if testfn (funcall testfn (caar l) k) (equal (caar l) k))
          (setq r (car l))
        (setq l (cdr l))))
    r))
(defun rassq (v l) (let ((r nil)) (while (and l (not r)) (if (eq (cdar l) v) (setq r (car l)) (setq l (cdr l)))) r))
(defun alist-get (k al &optional default _remove testfn)
  ;; Value associated with K in alist AL (DEFAULT if absent); TESTFN overrides eq.
  (let ((p (if testfn (assoc k al testfn) (assq k al))))
    (if p (cdr p) default)))
(defun plist-get (pl k &optional predicate)
  (let ((test (or predicate #'eq)) (r nil))
    (while pl (if (funcall test (car pl) k) (progn (setq r (cadr pl)) (setq pl nil)) (setq pl (cddr pl)))) r))
(defun plist-member (pl k &optional predicate)
  (let ((test (or predicate #'eq)) (r nil))
    (while pl (if (funcall test (car pl) k) (progn (setq r pl) (setq pl nil)) (setq pl (cddr pl)))) r))

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
(defun remq (x l) (seq-filter (lambda (e) (not (eq e x))) l))
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
  (let ((result nil) (tail nil))
    (while lists
      (let ((seg (car lists)))
        (when seg
          (if result (setcdr tail seg) (setq result seg))
          (setq tail seg)
          (while (cdr tail) (setq tail (cdr tail)))))
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
;; No buffer-local bindings in this model, so default-value == symbol-value and
;; set-default / setq-default are just the global setters.
(defun default-value (sym) (symbol-value sym))
(defun set-default (sym val) (set sym val))
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
;; A form that evaluates to V (self-quoting literals as-is, else (quote V)).
(defun macroexp-quote (v)
  (if (and (not (consp v)) (or (not (symbolp v)) (null v) (eq v t) (keywordp v)))
      v
    (list 'quote v)))
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
;; Declaration handler alists (byte-run.el).  elisprs's `declare' is a no-op, so
;; these are not consulted for defun/defmacro expansion; they exist only because
;; libraries (e.g. gv) push their own gv-expander/gv-setter handlers onto them.
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
(defun sequencep (x) (or (listp x) (vectorp x) (stringp x)))
(defun arrayp (x) (or (vectorp x) (stringp x)))
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
(defun string= (a b) (equal (string--name a) (string--name b)))
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
(defun capitalize (s)
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
;; Text properties aren't stored (fusevm strings can't carry them), so this
;; returns the bare string — display-string code runs; property reads still error.
(defun propertize (string &rest _props) string)
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
(defun seq-take (seq n) (seq-into (take n (append seq nil)) (seq--type-of seq)))
(defun seq-drop (seq n) (seq-into (nthcdr n (append seq nil)) (seq--type-of seq)))
(defun seq-subseq (seq start &optional end)
  ;; Sequence-generic, returning SEQ's type; START/END may be negative.
  (let* ((lst (append seq nil)) (len (length lst))
         (s (if (< start 0) (+ len start) start))
         (e (cond ((null end) len) ((< end 0) (+ len end)) (t end)))
         (sub (take (- e s) (nthcdr s lst))))
    (cond ((stringp seq) (apply (function string) sub))
          ((vectorp seq) (vconcat sub))
          (t sub))))
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
  (append (append a nil) (seq-difference b a testfn)))
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
  (let* ((name (if (consp name-spec) (car name-spec) name-spec))
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
    (when parent
      (setq forms (cons `(setq cl-struct--parent
                               (cons (cons ',tag ',(intern (concat "cl-struct-" (symbol-name parent))))
                                     cl-struct--parent))
                        forms)))
    (when copier
      (setq forms (cons `(defun ,(intern copier) (--s--) (vconcat --s--)) forms)))
    (let ((j 1))
      (dolist (sn snames)
        (let ((acc (intern (concat conc (symbol-name sn)))))
          (setq forms (cons `(defun ,acc (--s--) (aref --s-- ,j)) forms))
          (setq forms (cons `(setq cl-struct--slot-index
                                   (cons (cons ',acc ,j) cl-struct--slot-index))
                            forms)))
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
(defmacro cl-flet (bindings &rest body)
  (let* ((gs (mapcar (lambda (b) (make-symbol (symbol-name (car b)))) bindings))
         (alist (cl-mapcar (lambda (b g) (cons (car b) g)) bindings gs)))
    `(let ,(cl-mapcar (lambda (b g) (list g (cl-flet--lambda (cdr b)))) bindings gs)
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
       ,@(cl-mapcar (lambda (b g) (list 'setq g (cl-flet--lambda (cl-flet--walk (cdr b) alist)))) bindings gs)
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
(define-error 'cl-assertion-failed "Assertion failed")
(define-error 'end-of-file "End of file during parsing")
(defun add-to-list (var elt &optional append compare-fn)
  ;; Add ELT to VAR's list unless already present (COMPARE-FN, default `equal');
  ;; prepend by default, or append to the end when APPEND is non-nil.
  (let ((cur (symbol-value var)) (test (or compare-fn #'equal)) (found nil))
    (dolist (x cur) (when (funcall test elt x) (setq found t)))
    (if found cur
      (set var (if append (append cur (list elt)) (cons elt cur))))))
;; No text-property model, so this collapses to plain `equal'.
(defun equal-including-properties (a b) (equal a b))

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
(defun number-or-marker-p (x) (numberp x))
(defun integer-or-marker-p (x) (integerp x))
(defun string-pad (s len &optional padding start)
  ;; Pad S to LENGTH chars with PADDING (default space); pad on the left when
  ;; START is non-nil, otherwise on the right.
  (unless (natnump len) (signal 'wrong-type-argument (list 'natnump len)))
  (let ((pad (or padding 32)) (cur (length s)))
    (if (>= cur len) s
      (let ((fill (make-string (- len cur) pad)))
        (if start (concat fill s) (concat s fill))))))
;; A horizontal rule: LENGTH dashes (default 79, the batch text width) + newline.
;; (Emacs returns a face-propertized string; elisprs can't store props.)
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
(defun string-equal-ignore-case (a b) (string= (downcase a) (downcase b)))
(defun upcase-initials (s)
  ;; Upcase the first letter of every word, leaving the rest unchanged.
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
;; `load' machinery. These are dynamic (special) variables the `load' builtin
;; rebinds around a file's evaluation and restores afterward. At top level
;; `load-file-name'/`load-true-file-name' are nil and `load-in-progress' is nil,
;; matching Emacs. `load-path' is searched for a bare (directory-less) FILE;
;; `load-suffixes' are the extensions tried (elisprs has no bytecode, so `.elc'
;; is never found — only `.el' and the exact name resolve).
(defvar load-path nil)
(defvar load-file-name nil)
(defvar load-true-file-name nil)
(defvar load-in-progress nil)
(defvar load-suffixes '(".elc" ".el"))
(defvar load-file-rep-suffixes '(""))
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
;; with-temp-buffer: run BODY in a fresh editing buffer (text + point only — no
;; markers/narrowing/save-excursion). Returns BODY's value, not the buffer text.
(defmacro with-temp-buffer (&rest body)
  `(progn (--buffer-push--) (unwind-protect (progn ,@body) (--buffer-pop--))))
;; save-excursion: restore point after BODY. (Integer save — no marker, so it
;; doesn't track insertions/deletions before the saved point.)
(defmacro save-excursion (&rest body)
  `(let ((--se-pt-- (point))) (unwind-protect (progn ,@body) (goto-char --se-pt--))))
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
;; No real markers, so this is plain insert.
(defun insert-before-markers (&rest args) (apply #'insert args))
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
;; No buffer-local model: these are no-ops returning the symbol.
(defun make-local-variable (sym) sym)
(defun make-variable-buffer-local (sym) sym)
;; With no buffer-local bindings, no variable is ever locally set.
(defun local-variable-if-set-p (_variable &optional _buffer) nil)
(defun set-default-toplevel-value (sym val) (set sym val))
(defun defalias (symbol definition &optional _docstring) (fset symbol definition) symbol)
(defalias 'string-split 'split-string)
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
       ;; Unknown head: if PLACE is a macro call, expand it once and retry —
       ;; this is how `gv-get' handles macro-defined places such as
       ;; `(cl--generic name)' -> `(get name 'cl--generic)' (gv.el:103).
       (t (let ((me (macroexpand-1 place)))
            (if (eq me place)
                (error "setf: unsupported place %S" place)
              (setf--expand me val))))))))
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
"#;
