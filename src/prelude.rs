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
(defun expt (base e) (let ((r 1)) (while (> e 0) (setq r (* r base)) (setq e (1- e))) r))
(defun gcd (a b)
  (setq a (abs a)) (setq b (abs b))
  (while (> b 0) (let ((tmp b)) (setq b (% a b)) (setq a tmp)))
  a)
(defun lcm (a b) (if (or (= a 0) (= b 0)) 0 (/ (abs (* a b)) (gcd a b))))
(defun isqrt (n) (let ((r 0)) (while (<= (* (1+ r) (1+ r)) n) (setq r (1+ r))) r))
(defun cl-signum (x) (cond ((> x 0) 1) ((< x 0) -1) (t 0)))
(defun cl-evenp (n) (= (% n 2) 0))
(defun cl-oddp (n) (= (% n 2) 1))
(defun string-to-number (s)
  (let ((chars (string-to-list s)) (sign 1) (n 0) (seen nil))
    (while (and chars (or (= (car chars) 32) (= (car chars) 9))) (setq chars (cdr chars)))
    (when (and chars (= (car chars) ?-)) (setq sign -1) (setq chars (cdr chars)))
    (when (and chars (= (car chars) ?+)) (setq chars (cdr chars)))
    (while (and chars (>= (car chars) ?0) (<= (car chars) ?9))
      (setq n (+ (* n 10) (- (car chars) ?0))) (setq seen t) (setq chars (cdr chars)))
    (if seen (* sign n) 0)))

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
(defun string-reverse (s) (apply (function string) (reverse (string-to-list s))))
(defun upcase (s)
  (apply (function string)
         (mapcar (lambda (c) (if (and (>= c ?a) (<= c ?z)) (- c 32) c)) (string-to-list s))))
(defun downcase (s)
  (apply (function string)
         (mapcar (lambda (c) (if (and (>= c ?A) (<= c ?Z)) (+ c 32) c)) (string-to-list s))))
(defun capitalize (s)
  (if (string-empty-p s) s
    (let* ((chars (string-to-list (downcase s))) (c (car chars)))
      (apply (function string)
             (cons (if (and (>= c ?a) (<= c ?z)) (- c 32) c) (cdr chars))))))
(defun string-trim-left (s)
  (let ((chars (string-to-list s)))
    (while (and chars (or (= (car chars) 32) (= (car chars) 9) (= (car chars) 10) (= (car chars) 13)))
      (setq chars (cdr chars)))
    (apply (function string) chars)))
(defun string-trim-right (s) (string-reverse (string-trim-left (string-reverse s))))
(defun string-trim (s) (string-trim-right (string-trim-left s)))
(defun string-blank-p (s) (string-empty-p (string-trim s)))
(defun string-remove-prefix (prefix s)
  (if (string-prefix-p prefix s) (substring s (length prefix) (length s)) s))
(defun string-remove-suffix (suffix s)
  (if (string-suffix-p suffix s) (substring s 0 (- (length s) (length suffix))) s))

;;; ---- lists ----
(defun butlast (lst) (reverse (cdr (reverse lst))))
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
(defmacro when-let (binding &rest body)
  (let ((var (car (car binding))) (val (car (cdr (car binding)))))
    `(let ((,var ,val)) (when ,var ,@body))))
(defmacro if-let (binding then &rest else)
  (let ((var (car (car binding))) (val (car (cdr (car binding)))))
    `(let ((,var ,val)) (if ,var ,then ,@else))))
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
(defmacro cl-incf (place &rest amt) (if amt `(setq ,place (+ ,place ,(car amt))) `(setq ,place (1+ ,place))))
(defmacro cl-decf (place &rest amt) (if amt `(setq ,place (- ,place ,(car amt))) `(setq ,place (1- ,place))))
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
(defun seq-group-by (fn seq)
  (let ((result nil))
    (dolist (x seq)
      (let* ((key (funcall fn x)) (cell (assoc key result)))
        (if cell (setcdr cell (cons x (cdr cell)))
          (setq result (cons (cons key (list x)) result)))))
    (mapcar (lambda (c) (cons (car c) (reverse (cdr c)))) result)))

(defun plist-put (plist prop val)
  (let ((p plist) (done nil))
    (while (and p (not done))
      (if (eq (car p) prop) (progn (setcar (cdr p) val) (setq done t)) (setq p (cddr p))))
    (if done plist (append plist (list prop val)))))
(defun add-to-list (var elt)
  (let ((cur (symbol-value var)))
    (if (member elt cur) cur (set var (cons elt cur)))))

(defun apply-partially (fn &rest args) (lambda (&rest more) (apply fn (append args more))))
(defun complement (fn) (lambda (&rest args) (not (apply fn args))))
(defun cl-constantly (x) (lambda (&rest --ignore--) x))

(defun string-chop-newline (s) (if (string-suffix-p "\n" s) (substring s 0 (- (length s) 1)) s))
(defun string-pad (s len) (let ((cur (length s))) (if (>= cur len) s (concat s (make-string (- len cur) 32)))))
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
