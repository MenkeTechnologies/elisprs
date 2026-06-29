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
;; `abs` is a primitive subr (keeps int/float type; (abs -0.0) => 0.0).
(defun max (x &rest xs) (while xs (if (> (car xs) x) (setq x (car xs))) (setq xs (cdr xs))) x)
(defun min (x &rest xs) (while xs (if (< (car xs) x) (setq x (car xs))) (setq xs (cdr xs))) x)
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
(defun nthcdr (n l) (while (and (> n 0) l) (setq l (cdr l)) (setq n (1- n))) l)
(defun last (l &optional n)
  ;; The last N cons cells of L (default 1): (last '(1 2 3) 2) => (2 3).
  ;; Guard on consp so an improper tail stops the walk instead of erroring:
  ;; (last '(1 2 . 3)) => (2 . 3).
  (if (or (null n) (= n 1))
      (progn (while (consp (cdr l)) (setq l (cdr l))) l)
    (nthcdr (max 0 (- (length l) n)) l)))
(defun make-list (n x) (let ((r nil)) (while (> n 0) (setq r (cons x r)) (setq n (1- n))) r))
(defun number-sequence (from to &optional inc)
  (setq inc (or inc 1))
  (let ((r nil))
    (if (< inc 0)
        (while (>= from to) (setq r (cons from r)) (setq from (+ from inc)))
      (while (<= from to) (setq r (cons from r)) (setq from (+ from inc))))
    (reverse r)))
(defun elt (seq n) (if (listp seq) (nth n seq) (aref seq n)))
(defun safe-length (l)
  ;; Count cons cells in the spine, stopping at any non-cons tail (so improper
  ;; lists don't error): (safe-length '(1 2 . 3)) => 2.
  (let ((n 0)) (while (consp l) (setq n (1+ n)) (setq l (cdr l))) n))
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
             (s (if (consp el) (car el) el))
             (s (if (symbolp s) (symbol-name s) s)))
        (if (if case-fold (string-equal-ignore-case k s) (string= k s))
            (setq r el)
          (setq alist (cdr alist)))))
    r))
(defun assq (k l) (let ((r nil)) (while (and l (not r)) (if (eq (caar l) k) (setq r (car l)) (setq l (cdr l)))) r))
(defun assoc (k l &optional testfn)
  (let ((r nil))
    (while (and l (not r))
      (if (if testfn (funcall testfn k (caar l)) (equal (caar l) k))
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
(defun seq-contains-p (l x) (if (member x (append l nil)) t nil))
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
(defun delete (x l) (remove x l))
(defun delq (x l) (remq x l))
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
  (let ((count (cl--getkey keys :count nil)) (lst (append seq nil)) (r nil) (n 0))
    (while lst
      (if (and (funcall pred (car lst)) (or (null count) (< n count)))
          (setq n (1+ n))
        (setq r (cons (car lst) r)))
      (setq lst (cdr lst)))
    (cl--like (nreverse r) seq)))
(defun cl-remove-if-not (pred seq &rest keys)
  (apply 'cl-remove-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-delete-if (pred seq &rest keys) (apply (function cl-remove-if) pred seq keys))
(defun cl-delete-if-not (pred seq &rest keys) (apply (function cl-remove-if-not) pred seq keys))
(defun cl-find-if (pred l) (seq-find pred l))
(defun cl-find-if-not (pred l) (seq-find (lambda (x) (not (funcall pred x))) l))
(defun cl-sort (seq pred &rest keys)
  (let ((key (cl--getkey keys :key nil)))
    (if key (sort seq (lambda (a b) (funcall pred (funcall key a) (funcall key b))))
      (sort seq pred))))
(defun commandp (_obj &optional _) nil)
(defun plistp (l)
  (let ((n 0)) (while (consp l) (setq n (1+ n)) (setq l (cdr l))) (and (null l) (= 0 (% n 2)))))
(defun cl-some (pred l) (seq-some pred l))
(defun cl-every (pred l) (seq-every-p pred l))
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
(defun cl-subst (new old tree &rest _keys)
  (cond ((eql tree old) new)
        ((consp tree) (cons (cl-subst new old (car tree)) (cl-subst new old (cdr tree))))
        (t tree)))
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
(defun cl-union (l1 l2 &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (r (append l1 nil)) (b l2))
    (while b
      (let ((x (car b)))
        (unless (seq-some (lambda (y) (funcall test x y)) r) (setq r (cons x r))))
      (setq b (cdr b)))
    r))
(defun cl-intersection (l1 l2 &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (r nil) (a l1))
    (while a
      (let ((x (car a)))
        (when (seq-some (lambda (y) (funcall test x y)) l2) (setq r (cons x r))))
      (setq a (cdr a)))
    r))
(defun cl-set-difference (l1 l2 &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (r nil) (a l1))
    (while a
      (let ((x (car a)))
        (unless (seq-some (lambda (y) (funcall test x y)) l2) (setq r (cons x r))))
      (setq a (cdr a)))
    (nreverse r)))
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
(defmacro push (x place) (list (quote setq) place (list (quote cons) x place)))
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
(defmacro pop (place) (list (quote prog1) (list (quote car) place) (list (quote setq) place (list (quote cdr) place))))
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
(defun characterp (x) (and (integerp x) (>= x 0)))
(defun sequencep (x) (or (listp x) (vectorp x) (stringp x)))
(defun arrayp (x) (or (vectorp x) (stringp x)))
(defun string-or-null-p (x) (or (null x) (stringp x)))
(defun xor (a b) (cond ((not a) b) ((not b) a) (t nil)))
(defun proper-list-p (x)
  (let ((n 0))
    (while (consp x) (setq n (1+ n)) (setq x (cdr x)))
    (if (null x) n nil)))

;;; ---- numbers ----
;; `expt` is a primitive subr (integer power; float for fractional/negative exp).
(defun gcd (a b)
  (setq a (abs a)) (setq b (abs b))
  (while (> b 0) (let ((tmp b)) (setq b (% a b)) (setq a tmp)))
  a)
(defun lcm (a b) (if (or (= a 0) (= b 0)) 0 (/ (abs (* a b)) (gcd a b))))
(defun isqrt (n) (let ((r 0)) (while (<= (* (1+ r) (1+ r)) n) (setq r (1+ r))) r))
(defun cl-signum (x) (cond ((> x 0) 1) ((< x 0) -1) (t 0)))
(defun cl-evenp (n) (= (% n 2) 0))
(defun cl-oddp (n) (/= (% n 2) 0))
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

;;; ---- strings (ASCII) ----
(defun string= (a b) (equal a b))
(defun string-equal (a b) (equal a b))
(defun string< (a b)
  (let ((la (string-to-list a)) (lb (string-to-list b)) (res nil) (done nil))
    (while (not done)
      (cond ((null la) (setq res (not (null lb))) (setq done t))
            ((null lb) (setq res nil) (setq done t))
            ((< (car la) (car lb)) (setq res t) (setq done t))
            ((> (car la) (car lb)) (setq res nil) (setq done t))
            (t (setq la (cdr la)) (setq lb (cdr lb)))))
    res))
(defun string-lessp (a b) (string< a b))
(defun string-greaterp (a b) (string< b a))
(defun string-reverse (s) (reverse s))
;; `upcase` / `downcase` are primitive subrs (accept a string or a character).
(defun capitalize (s)
  ;; Upcase the first letter of every word (run of alphanumerics), downcase the
  ;; rest: (capitalize "hello world") => "Hello World".
  (let ((out nil) (in-word nil))
    (dolist (c (string-to-list s))
      (let* ((lower (and (>= c ?a) (<= c ?z)))
             (upper (and (>= c ?A) (<= c ?Z)))
             (alnum (or lower upper (and (>= c ?0) (<= c ?9)))))
        (cond
         ((not alnum) (setq out (cons c out)))
         (in-word (setq out (cons (if upper (+ c 32) c) out)))
         (t (setq out (cons (if lower (- c 32) c) out))))
        (setq in-word alnum)))
    (apply (function string) (reverse out))))
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
(defun nbutlast (lst &optional n) (butlast lst n))
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
(defun assoc-default (key alist) (let ((cell (assoc key alist))) (if cell (cdr cell) nil)))
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
(defun nreverse (lst) (reverse lst))

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
(defun seq-uniq (seq) (delete-dups (append seq nil)))
(defun seq-min (seq) (apply (function min) (append seq nil)))
(defun seq-max (seq) (apply (function max) (append seq nil)))
(defun seq-first (seq) (elt seq 0))
(defun seq-rest (seq) (seq-drop seq 1))
(defun seq-position (seq elt)
  (let ((i 0) (res nil))
    (setq seq (append seq nil))
    (while (and seq (null res)) (if (equal (car seq) elt) (setq res i)) (setq seq (cdr seq)) (setq i (1+ i)))
    res))
(defun seq-into (seq type)
  (cond ((eq type 'list) (append seq nil))
        ((eq type 'vector) (apply (function vector) seq))
        ((eq type 'string) (apply (function string) seq))
        (t seq)))
(defun seq-difference (a b) (seq-filter (lambda (x) (not (seq-contains-p b x))) a))
(defun seq-intersection (a b) (seq-filter (lambda (x) (seq-contains-p b x)) a))
(defun seq-union (a b) (append a (seq-difference b a)))
(defun seq-sort (pred seq) (seq-into (sort (append seq nil) pred) (seq--type-of seq)))
(defun seq-partition (seq n)
  (let ((out nil) (vec (vectorp seq)) (l (append seq nil)))
    (while l
      (let ((chunk (take n l)))
        (setq out (cons (if vec (vconcat chunk) chunk) out)))
      (setq l (nthcdr n l)))
    (reverse out)))
(defun seq-split (seq n) (seq-partition seq n))
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
(defun cl-subseq (seq start &optional end) (seq-subseq seq start end))
(defun cl--in-bounds (i start end) (and (>= i start) (or (null end) (< i end))))
(defun cl-position (item seq &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (r nil))
    (while (and lst (not r))
      (when (and (cl--in-bounds i start end) (funcall test item (funcall key (car lst))))
        (setq r i))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-count (item seq &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
        (start (cl--getkey keys :start 0)) (end (cl--getkey keys :end nil))
        (lst (append seq nil)) (i 0) (n 0))
    (while lst
      (when (and (cl--in-bounds i start end) (funcall test item (funcall key (car lst))))
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
        (lst (append seq nil)) (i 0) (r nil))
    (while (and lst (not r))
      (when (and (cl--in-bounds i start end) (funcall pred (funcall key (car lst))))
        (setq r i))
      (setq i (1+ i) lst (cdr lst)))
    r))
(defun cl-position-if-not (pred seq &rest keys)
  (apply 'cl-position-if (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-find (item seq &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
        (lst (append seq nil)) (r nil) (found nil))
    (while (and lst (not found))
      (if (funcall test item (funcall key (car lst))) (setq r (car lst) found t)
        (setq lst (cdr lst))))
    r))
(defun cl-remove-duplicates (seq &rest keys)
  ;; Default keeps the LAST occurrence of each `equal' element; with :from-end
  ;; non-nil, keeps the FIRST.
  (if (plist-get keys :from-end)
      (cl--like (delete-dups (append seq nil)) seq)
    (cl--like (reverse (delete-dups (reverse (append seq nil)))) seq)))
(defun cl-pairlis (the-keys the-values &optional alist)
  (let ((ks (reverse the-keys)) (vs (reverse the-values)) (res alist))
    (while ks
      (setq res (cons (cons (car ks) (car vs)) res) ks (cdr ks) vs (cdr vs)))
    res))
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

;;; ---- subr-x macros ----
;; Build nested `(let ((VAR VAL)) (if VAR <inner> ELSE))` for a list of BINDINGS,
;; short-circuiting to ELSE the first time a bound value is nil.
(defun if-let--chain (bindings then else)
  (if (null bindings) then
    (let* ((b (car bindings))
           (var (if (consp b) (car b) b))
           (val (if (consp b) (car (cdr b)) b)))
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
(defmacro cl-flet (bindings &rest body)
  (let* ((gs (mapcar (lambda (b) (make-symbol (symbol-name (car b)))) bindings))
         (alist (cl-mapcar (lambda (b g) (cons (car b) g)) bindings gs)))
    `(let ,(cl-mapcar (lambda (b g) (list g (cons 'lambda (cdr b)))) bindings gs)
       ,@(cl-flet--walk body alist))))
(defmacro cl-labels (bindings &rest body)
  (let* ((gs (mapcar (lambda (b) (make-symbol (symbol-name (car b)))) bindings))
         (alist (cl-mapcar (lambda (b g) (cons (car b) g)) bindings gs)))
    `(let ,(mapcar (lambda (g) (list g nil)) gs)
       ,@(cl-mapcar (lambda (b g) (list 'setq g (cl-flet--walk (cons 'lambda (cdr b)) alist))) bindings gs)
       ,@(cl-flet--walk body alist))))
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
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity)) (r nil))
    (while (and lst (not r))
      (if (funcall test item (funcall key (car lst))) (setq r lst) (setq lst (cdr lst))))
    r))
(defun cl-assoc (item alist &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity)) (r nil))
    (while (and alist (not r))
      (let ((pair (car alist)))
        (if (and (consp pair) (funcall test item (funcall key (car pair))))
            (setq r pair) (setq alist (cdr alist)))))
    r))
(defun cl-remove (item seq &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
        (count (cl--getkey keys :count nil)) (lst (append seq nil)) (out nil) (removed 0))
    (while lst
      (if (and (funcall test item (funcall key (car lst))) (or (null count) (< removed count)))
          (setq removed (1+ removed))
        (setq out (cons (car lst) out)))
      (setq lst (cdr lst)))
    (cl--like (reverse out) seq)))
(defun cl-delete (item seq &rest keys) (apply (function cl-remove) item seq keys))
(defun cl-substitute (new old seq &rest keys)
  (let ((test (cl--getkey keys :test 'eql)) (key (cl--getkey keys :key 'identity))
        (count (cl--getkey keys :count nil)) (lst (append seq nil)) (out nil) (done 0))
    (while lst
      (if (and (funcall test old (funcall key (car lst))) (or (null count) (< done count)))
          (progn (setq out (cons new out)) (setq done (1+ done)))
        (setq out (cons (car lst) out)))
      (setq lst (cdr lst)))
    (cl--like (reverse out) seq)))
(defun cl-substitute-if (new pred seq &rest keys)
  (let ((key (cl--getkey keys :key 'identity)) (count (cl--getkey keys :count nil))
        (lst (append seq nil)) (out nil) (done 0))
    (while lst
      (if (and (funcall pred (funcall key (car lst))) (or (null count) (< done count)))
          (setq out (cons new out) done (1+ done))
        (setq out (cons (car lst) out)))
      (setq lst (cdr lst)))
    (cl--like (reverse out) seq)))
(defun cl-substitute-if-not (new pred seq &rest keys)
  (apply 'cl-substitute-if new (lambda (x) (not (funcall pred x))) seq keys))
(defun cl-mapcan (fn &rest seqs) (apply 'nconc (apply 'cl-mapcar fn seqs)))
(defun cl-acons (key val alist) (cons (cons key val) alist))
(defun cl-list* (&rest args)
  (if (null (cdr args)) (car args) (cons (car args) (apply (function cl-list*) (cdr args)))))

(defun seq-map-indexed (fn seq)
  (let ((i 0) (out nil))
    (while seq (setq out (cons (funcall fn (car seq) i) out)) (setq seq (cdr seq)) (setq i (1+ i)))
    (reverse out)))
(defun seq-do-indexed (fn seq)
  (let ((i 0)) (while seq (funcall fn (car seq) i) (setq seq (cdr seq)) (setq i (1+ i)))) nil)
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
(defun copy-alist (al)
  (mapcar (lambda (p) (if (consp p) (cons (car p) (cdr p)) p)) al))
(defun substring-no-properties (s &optional from to)
  (if to (substring s (or from 0) to) (substring s (or from 0))))
;; The human-readable message for an error object (ERROR-SYMBOL . DATA).
(defun error-message-string (err)
  (let* ((sym (car err)) (data (cdr err)) (msg (get sym 'error-message)))
    (cond
     ;; `error'/`user-error' carry their message string as the sole datum.
     ((and (memq sym '(error user-error)) (stringp (car data))) (car data))
     (msg (if data
              (concat msg ": " (mapconcat (lambda (x) (if (stringp x) x (format "%S" x))) data ", "))
            msg))
     ((null data) (symbol-name sym))
     (t (concat (symbol-name sym) ": "
                (mapconcat (lambda (x) (if (stringp x) x (format "%S" x))) data ", "))))))
(defun seq-group-by (fn seq)
  (let ((result nil))
    (dolist (x (append seq nil))
      (let* ((key (funcall fn x)) (cell (assoc key result)))
        (if cell (setcdr cell (cons x (cdr cell)))
          (setq result (cons (cons key (list x)) result)))))
    ;; Reverse so groups appear in first-encounter order, items in order.
    (nreverse (mapcar (lambda (c) (cons (car c) (reverse (cdr c)))) result))))

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
(define-error 'wrong-type-argument "Wrong type argument")
(define-error 'wrong-number-of-arguments "Wrong number of arguments")
(define-error 'void-variable "Symbol's value as variable is void")
(define-error 'void-function "Symbol's function definition is void")
(define-error 'invalid-function "Invalid function")
(define-error 'wrong-length-argument "Wrong length argument")
(define-error 'invalid-regexp "Invalid regexp")
(define-error 'cl-assertion-failed "Assertion failed")
(define-error 'end-of-file "End of file during parsing")
(defun add-to-list (var elt)
  (let ((cur (symbol-value var)))
    (if (member elt cur) cur (set var (cons elt cur)))))

(defun apply-partially (fn &rest args) (lambda (&rest more) (apply fn (append args more))))
(defun complement (fn) (lambda (&rest args) (not (apply fn args))))
(defun cl-constantly (x) (lambda (&rest --ignore--) x))

(defun string-chop-newline (s) (if (string-suffix-p "\n" s) (substring s 0 (- (length s) 1)) s))
(defun pp-to-string (object) (concat (prin1-to-string object) "\n"))
(defun pp (object &optional _stream) (princ (pp-to-string object)) nil)
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
        (and (>= c #x20000) (<= c #x3FFFD))) ; CJK ext B+
    2)
   (t 1)))
(defun string-width (s &optional _from _to)
  (let ((w 0) (l (string-to-list s)))
    (while l (setq w (+ w (char-width (car l))) l (cdr l)))
    w))
(defun truncate-string-to-width (str end-column &optional start-column padding _ellipsis)
  ;; Truncate STR so its display width is at most END-COLUMN (from START-COLUMN).
  (let ((col 0) (start (or start-column 0)) (out nil) (l (string-to-list str)) (stop nil))
    (while (and l (not stop))
      (let* ((c (car l)) (cw (char-width c)))
        (if (> (+ col cw) end-column) (setq stop t)
          (when (>= col start) (setq out (cons c out)))
          (setq col (+ col cw))))
      (setq l (cdr l)))
    (when (and padding (< col end-column))
      (while (< col end-column) (setq out (cons padding out) col (1+ col))))
    (concat (nreverse out))))
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
  (let ((pad (or padding 32)) (cur (length s)))
    (if (>= cur len) s
      (let ((fill (make-string (- len cur) pad)))
        (if start (concat fill s) (concat s fill))))))
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
        (acc-kind nil) (bool-result nil) (finally nil) (c clauses))
    (while c
      (let ((kw (cl-loop--kw (car c))))
        (cond
         ;; named NAME — accepted (enables cl-return-from); no extra setup needed.
         ((equal kw "named") (setq c (nthcdr 2 c)))
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
          (let ((var (nth 1 c)) (tv (make-symbol "tail")) (r (nthcdr 3 c)))
            (when (member (cl-loop--kw (car r)) '("the" "each")) (setq r (cdr r)))
            (let ((kind (cl-loop--kw (car r))))
              (setq r (cdr r))
              (when (member (cl-loop--kw (car r)) '("of" "in")) (setq r (cdr r)))
              (let ((listform (cond ((member kind '("hash-keys" "hash-key")) (list 'hash-table-keys (car r)))
                                    ((member kind '("hash-values" "hash-value")) (list 'hash-table-values (car r)))
                                    (t (list 'append (car r) nil)))))
                (setq r (cdr r))
                (when (equal (cl-loop--kw (car r)) "using") (setq r (nthcdr 2 r)))
                (setq binds (cons (list tv listform) (cons (list var nil) binds)))
                (setq test (if (eq test t) tv (list 'and test tv)))
                (setq pre (cons (list 'setq var (list 'car tv)) pre))
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
                 (dbs (cl-db--binds pat ev)) (setqs nil))
            (setq binds (cons (list tv (nth 3 c)) (cons (list ev nil) binds)))
            (dolist (b dbs) (setq binds (cons (list (car b) nil) binds)))
            (setq test (if (eq test t) tv (list 'and test tv)))
            (setq setqs (cons (list 'setq ev (list 'car tv)) nil))
            (dolist (b dbs) (setq setqs (cons (list 'setq (car b) (car (cdr b))) setqs)))
            (setq pre (cons (cons 'progn (reverse setqs)) pre))
            (setq steps (cons (list 'setq tv (list 'cdr tv)) steps))
            (setq c (nthcdr 4 c))))
         ;; for V in LIST
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "in"))
          (let ((var (nth 1 c)) (tv (make-symbol "tail")))
            (setq binds (cons (list tv (nth 3 c)) (cons (list var nil) binds)))
            (setq test (if (eq test t) tv (list 'and test tv)))
            (setq pre (cons (list 'setq var (list 'car tv)) pre))
            (setq steps (cons (list 'setq tv (list 'cdr tv)) steps))
            (setq c (nthcdr 4 c))))
         ;; for V on LIST
         ((and (member kw '("for" "as")) (equal (cl-loop--kw (nth 2 c)) "on"))
          (let ((var (nth 1 c)))
            (setq binds (cons (list var (nth 3 c)) binds))
            (setq test (if (eq test t) var (list 'and test var)))
            (setq steps (cons (list 'setq var (list 'cdr var)) steps))
            (setq c (nthcdr 4 c))))
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
         ;; when/unless/if COND <accum> [else <accum>] [end]
         ((member kw '("when" "unless" "if"))
          (let* ((cnd (nth 1 c)) (r (nthcdr 2 c)) (neg (equal kw "unless"))
                 (a (cl-loop--accum r)) (cform (nth 0 a)) (aform nil))
            (setq r (nth 1 a))
            (if (nth 3 a) (setq binds (cons (list (nth 3 a) (nth 4 a)) binds))
              (when (nth 2 a) (setq acc-kind (nth 2 a))))
            (when (equal (cl-loop--kw (car r)) "else")
              (let ((b (cl-loop--accum (cdr r))))
                (setq aform (nth 0 b) r (nth 1 b))
                (if (nth 3 b) (setq binds (cons (list (nth 3 b) (nth 4 b)) binds))
                  (when (nth 2 b) (setq acc-kind (nth 2 b))))))
            (when (equal (cl-loop--kw (car r)) "end") (setq r (cdr r)))
            (setq body (cons (if neg (list 'if cnd aform cform) (list 'if cnd cform aform)) body))
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
      `(let* (,@(reverse binds) (--clacc-- ,init))
         (catch '--cl-loop--
           (while ,test ,@(reverse pre) ,@(reverse body) ,@(reverse steps))
           ,result)))))

;; The predicate symbol for a `cl-typecase' type name (integer->integerp, etc.).
(defun cl-typep (obj type)
  ;; Simple type names only: (cl-typep 5 'integer) => t.
  (cond ((eq type t) t)
        ((eq type nil) nil)
        (t (funcall (cl-typecase--pred type) obj))))
(defun cl-typecase--pred (type)
  (cond ((eq type 'list) 'listp)
        ((eq type 'null) 'null)
        ((eq type 'atom) 'atom)
        ((eq type 'number) 'numberp)
        (t (intern (concat (symbol-name type) "p")))))

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
                    (cons (list (cl-typecase--pred type) '--ct-v--) body))))
              clauses))))
(defmacro cl-the (_type form) form)
(defmacro cl-assert (form &rest _)
  (list 'if form nil (list 'signal (list 'quote 'cl-assertion-failed) (list 'list (list 'quote form)))))
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
;; Build let* bindings that positionally destructure VALEXPR (a symbol holding a
;; list) against a flat ARGLIST, honoring &optional and &rest.
(defun cl-db--plist-get (plist key default)
  ;; plist-get with a fallback when KEY is absent (for &key defaults).
  (let ((m (plist-member plist key))) (if m (car (cdr m)) default)))
(defun cl-db--binds (arglist v)
  ;; Supports &optional / &rest / &key with per-arg defaults `(VAR DEFAULT)` and
  ;; nested patterns in required position. &key reads the plist tail at position i.
  (let ((binds nil) (i 0) (mode 'req))
    (while (consp arglist)
      (let ((a (car arglist)))
        (cond
         ((eq a '&optional) (setq mode 'opt))
         ((eq a '&rest) (setq mode 'rest))
         ((eq a '&key) (setq mode 'key))
         ((eq a '&aux) (setq mode 'aux))
         ((eq mode 'rest) (setq binds (cons (list a (list 'nthcdr i v)) binds)))
         ((eq mode 'aux)
          (let ((var (if (consp a) (car a) a)) (def (and (consp a) (car (cdr a)))))
            (setq binds (cons (list var def) binds))))
         ((eq mode 'key)
          (let* ((var (if (consp a) (car a) a))
                 (def (and (consp a) (car (cdr a))))
                 (kw (intern (concat ":" (symbol-name var)))))
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
            (dolist (rb (cl-db--binds a tv)) (setq binds (cons rb binds))))
          (setq i (1+ i)))
         (t (setq binds (cons (list a (list 'nth i v)) binds)) (setq i (1+ i)))))
      (setq arglist (cdr arglist)))
    ;; A dotted tail — e.g. (K . V) — binds the trailing symbol to the rest.
    (when (and arglist (symbolp arglist))
      (setq binds (cons (list arglist (list 'nthcdr i v)) binds)))
    (reverse binds)))
(defmacro cl-destructuring-bind (arglist expr &rest body)
  `(let ((--cl-db-v-- ,expr))
     (let* ,(cl-db--binds arglist '--cl-db-v--) ,@body)))
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
       ((eq head 'nth) (list 'setcar (list 'nthcdr (car args) (car (cdr args))) val))
       ((eq head 'elt)
        ;; Bind the sequence + index once: list → setcar, array → aset.
        (list 'let (list (list '--setf-s-- (car args)) (list '--setf-n-- (car (cdr args))))
              (list 'if (list 'listp '--setf-s--)
                    (list 'setcar (list 'nthcdr '--setf-n-- '--setf-s--) val)
                    (list 'aset '--setf-s-- '--setf-n-- val))))
       ((eq head 'aref) (list 'aset (car args) (car (cdr args)) val))
       ((eq head 'gethash) (list 'puthash (car args) val (car (cdr args))))
       ((eq head 'symbol-value) (list 'set (car args) val))
       ((eq head 'symbol-function) (list 'fset (car args) val))
       ;; (setf (alist-get K AL) V): setcdr an existing pair, else prepend.
       ((eq head 'alist-get)
        (list 'let (list (list '--ag-p-- (list 'assq (car args) (car (cdr args)))))
              (list 'if '--ag-p--
                    (list 'setcdr '--ag-p-- val)
                    (setf--expand (car (cdr args))
                                  (list 'cons (list 'cons (car args) val) (car (cdr args)))))))
       ;; (setf (plist-get P K) V): set an existing value cell, else prepend
       ;; (K V) to P — matching Emacs's order for a new key.
       ((eq head 'plist-get)
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
       (t (error "setf: unsupported place %S" place))))))
(defmacro setf (&rest pairs)
  (let ((forms nil))
    (while pairs
      (setq forms (cons (setf--expand (car pairs) (car (cdr pairs))) forms))
      (setq pairs (cdr (cdr pairs))))
    (cons 'progn (reverse forms))))

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
   ((memq s '(nonl not-newline any anychar anything)) ".")
   ((eq s 'unmatchable) "\\`a\\`")
   (t (error "rx: unknown symbol %S" s))))
(defun rx--atom-p (s)
  ;; A regexp that a quantifier can suffix without a shy group.
  (let ((n (length s)))
    (cond ((= n 1) t)
          ((and (eq (aref s 0) ?\[) (eq (aref s (1- n)) ?\])) t)
          ((and (= n 2) (eq (aref s 0) ?\\)) t)
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
     (t (error "rx: unknown form %S" head)))))
(defmacro rx (&rest forms) (rx--seq forms))

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
(defmacro pcase-dolist (spec &rest body)
  ;; Iterate (cadr SPEC), destructuring each element against (car SPEC).
  (let ((ev (make-symbol "e")))
    (list 'dolist (list ev (car (cdr spec)))
          (cons 'pcase-let (cons (list (list (car spec) ev)) body)))))
(defmacro seq-let (args seq &rest body)
  ;; Positionally bind ARGS to the elements of SEQ for BODY; `&rest` binds the tail.
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
  ;; (seq-doseq (VAR SEQUENCE) BODY...) — iterate VAR over any sequence's elements.
  (let ((var (car spec)) (seq (car (cdr spec))))
    `(let ((--seq-doseq-tail-- (append ,seq nil)) (,var nil))
       (while --seq-doseq-tail--
         (setq ,var (car --seq-doseq-tail--))
         ,@body
         (setq --seq-doseq-tail-- (cdr --seq-doseq-tail--)))
       nil)))
(defun macroexp-progn (forms) (if (cdr forms) (cons 'progn forms) (car forms)))
(defmacro cl-function (f) (list 'function f))

(defun hash-table-empty-p (h) (= 0 (hash-table-count h)))

;;; ---- map.el (subset) ----
;; A generic key/value interface over alists, hash-tables and arrays. Lists are
;; treated strictly as alists, and (like Emacs map.el) the default test for list
;; lookups is `equal` (not `eq`).
(defun map-elt (map key &optional default testfn)
  (cond
   ((hash-table-p map) (gethash key map default))
   ((listp map)
    (let ((entry (assoc key map (or testfn #'equal))))
      (if entry (cdr entry) default)))
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
   ((listp map) (and (assoc key map (or testfn #'equal)) t))
   ((arrayp map) (and (integerp key) (>= key 0) (< key (length map))))
   (t nil)))
(defun map-keys (map) (map-apply (lambda (k _v) k) map))
(defun map-values (map) (map-apply (lambda (_k v) v) map))
(defun map-pairs (map) (map-apply #'cons map))
(defun map-length (map)
  (cond
   ((hash-table-p map) (hash-table-count map))
   ((listp map) (length map))
   ((arrayp map) (length map))
   (t 0)))
(defun map-empty-p (map) (= 0 (map-length map)))
(defun map-do (function map)
  (cond
   ((hash-table-p map) (maphash function map) nil)
   ((listp map)
    (dolist (pair map) (funcall function (car pair) (cdr pair)))
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
    (let ((res nil))
      (dolist (pair map) (unless (equal (car pair) key) (setq res (cons pair res))))
      (nreverse res)))
   (t map)))
;; Internal: return MAP updated so KEY maps to VALUE (used by setf map-elt).
(defun map--put (map key value)
  (cond
   ((hash-table-p map) (puthash key value map) map)
   ((listp map)
    (let ((entry (assoc key map #'equal)))
      (if entry (progn (setcdr entry value) map)
        (cons (cons key value) map))))
   ((arrayp map) (aset map key value) map)
   (t (error "map--put: unsupported map type"))))
(defun map--into (pairs type)
  (cond
   ((eq type 'list) (let ((acc nil)) (dolist (p pairs) (setq acc (map--put acc (car p) (cdr p)))) (nreverse acc)))
   ((eq type 'alist) (let ((acc nil)) (dolist (p pairs) (setq acc (map--put acc (car p) (cdr p)))) (nreverse acc)))
   ((eq type 'hash-table)
    (let ((h (make-hash-table :test 'equal)))
      (dolist (p pairs) (puthash (car p) (cdr p) h)) h))
   (t (error "map-into: unsupported type %S" type))))
(defun map-into (map type) (map--into (map-pairs map) type))
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
