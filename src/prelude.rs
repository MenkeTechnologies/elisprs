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
  (if (or (null n) (= n 1))
      (progn (while (cdr l) (setq l (cdr l))) l)
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
(defun safe-length (l) (length l))
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
  ;; Destructively remove `equal' duplicates, keeping the first occurrence.
  (let ((tail l))
    (while tail
      (setcdr tail (delete (car tail) (cdr tail)))
      (setq tail (cdr tail))))
  l)
(defun nconc (&rest lists)
  ;; Destructively concatenate LISTS by splicing each onto the previous tail.
  (let ((result nil) (tail nil))
    (dolist (l lists)
      (when l
        (if result (setcdr tail l) (setq result l))
        (setq tail l)
        (while (cdr tail) (setq tail (cdr tail)))))
    result))
(defun rassq-delete-all (value alist)
  (seq-filter (lambda (p) (not (and (consp p) (eq (cdr p) value)))) alist))

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
(defmacro with-demoted-errors (fmt &rest body) `(condition-case --err-- (progn ,@body) (error (message ,fmt --err--) nil)))

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
(defun cl-oddp (n) (= (% n 2) 1))
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
(defun string-blank-p (s) (string-empty-p (string-trim s)))
(defun string-remove-prefix (prefix s)
  (if (string-prefix-p prefix s) (substring s (length prefix) (length s)) s))
(defun string-remove-suffix (suffix s)
  (if (string-suffix-p suffix s) (substring s 0 (- (length s) (length suffix))) s))

;;; ---- lists ----
(defun butlast (lst &optional n)
  ;; All but the last N elements of LST (default 1): (butlast '(1 2 3) 2) => (1).
  (setq n (or n 1))
  (let ((keep (- (length lst) n)) (r nil) (i 0))
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
(defun seq-take (seq n) (take n seq))
(defun seq-drop (seq n) (nthcdr n seq))
(defun seq-subseq (seq start end) (take (- end start) (nthcdr start seq)))
(defun seq-uniq (seq) (delete-dups (append seq nil)))
(defun seq-min (seq) (apply (function min) seq))
(defun seq-max (seq) (apply (function max) seq))
(defun seq-first (seq) (car seq))
(defun seq-rest (seq) (cdr seq))
(defun seq-position (seq elt)
  (let ((i 0) (res nil))
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
(defun seq-sort (pred seq) (sort (append seq nil) pred))
(defun seq-partition (seq n)
  (let ((out nil))
    (while seq (setq out (cons (take n seq) out)) (setq seq (nthcdr n seq)))
    (reverse out)))

;;; ---- cl-lib (subset) ----
(defun cl-mapcar (fn lst) (mapcar fn lst))
(defun cl-subseq (seq start end) (seq-subseq seq start end))
(defun cl-position (item lst) (seq-position lst item))
(defun cl-count (item lst) (let ((n 0)) (dolist (x lst) (if (equal x item) (setq n (1+ n)))) n))
(defun cl-find (item lst) (if (member item lst) item nil))
(defun cl-remove-duplicates (lst) (delete-dups (append lst nil)))
(defun cl-getf (plist key) (plist-get plist key))

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
(defun cl-member (item lst) (member item lst))
(defun cl-assoc (item alist) (assoc item alist))
(defun cl-remove (item lst) (seq-remove (lambda (x) (equal x item)) lst))
(defun cl-delete (item lst) (cl-remove item lst))
(defun cl-substitute (new old lst) (mapcar (lambda (x) (if (equal x old) new x)) lst))
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
(defun seq-mapcat (fn seq) (apply (function append) (mapcar fn seq)))
(defun seq-mapn (fn &rest seqs)
  ;; Apply FN across N sequences in parallel, stopping at the shortest:
  ;; (seq-mapn #'+ '(1 2) '(3 4)) => (4 6).
  (let ((r nil))
    (while (not (memq nil seqs))
      (setq r (cons (apply fn (mapcar (function car) seqs)) r))
      (setq seqs (mapcar (function cdr) seqs)))
    (reverse r)))
;; Like `format' (we don't translate `...' to curved quotes).
(defun format-message (fmt &rest args) (apply (function format) fmt args))
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
  (let ((sym (car err)) (data (cdr err)))
    (cond
     ((and (eq sym 'error) (stringp (car data))) (car data))
     ((null data) (symbol-name sym))
     (t (concat (symbol-name sym) ": "
                (mapconcat (lambda (x) (if (stringp x) x (format "%S" x))) data ", "))))))
(defun seq-group-by (fn seq)
  (let ((result nil))
    (dolist (x seq)
      (let* ((key (funcall fn x)) (cell (assoc key result)))
        (if cell (setcdr cell (cons x (cdr cell)))
          (setq result (cons (cons key (list x)) result)))))
    (mapcar (lambda (c) (cons (car c) (reverse (cdr c)))) result)))

(defun plist-put (plist prop val)
  ;; Mutate PLIST in place: overwrite an existing PROP, or append (PROP VAL) to
  ;; the tail via setcdr. Only a nil PLIST yields a fresh list (can't mutate nil).
  (if (null plist)
      (list prop val)
    (let ((p plist) (done nil))
      (while (not done)
        (cond
         ((eq (car p) prop) (setcar (cdr p) val) (setq done t))
         ((cddr p) (setq p (cddr p)))
         (t (setcdr (cdr p) (list prop val)) (setq done t))))
      plist)))
(defun add-to-list (var elt)
  (let ((cur (symbol-value var)))
    (if (member elt cur) cur (set var (cons elt cur)))))

(defun apply-partially (fn &rest args) (lambda (&rest more) (apply fn (append args more))))
(defun complement (fn) (lambda (&rest args) (not (apply fn args))))
(defun cl-constantly (x) (lambda (&rest --ignore--) x))

(defun string-chop-newline (s) (if (string-suffix-p "\n" s) (substring s 0 (- (length s) 1)) s))
(defun string-pad (s len &optional padding start)
  ;; Pad S to LENGTH chars with PADDING (default space); pad on the left when
  ;; START is non-nil, otherwise on the right.
  (let ((pad (or padding 32)) (cur (length s)))
    (if (>= cur len) s
      (let ((fill (make-string (- len cur) pad)))
        (if start (concat fill s) (concat s fill))))))
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
  (if (string-empty-p from) s
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

;; ---- setf: generalized-variable assignment ----
;; Expands (setf PLACE VALUE) to the right mutator for PLACE. Supported places
;; (those whose setter primitives exist): a plain variable, car/cdr and the
;; two-level c[ad][ad]r accessors, nth, elt, aref, gethash, and symbol-value.
;; Each setter returns VALUE, so (setf …) yields the last assigned value, as in
;; Emacs. Backquote-pattern places (cl-struct slots, alist-get) wait on more
;; setter primitives / lazy backquote.
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
       ((eq head 'pred)
        (let ((fn (car (cdr pat))))
          (cons (list (if (consp fn) (append fn (list val)) (list fn val))) nil)))
       ((eq head 'guard) (cons (list (car (cdr pat))) nil))
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
(defmacro pcase-let (bindings &rest body)
  ;; Only the simple `(SYM VALUE)` binding form is supported (full pcase-let
  ;; destructuring needs backquote patterns).
  `(let ,bindings ,@body))

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
