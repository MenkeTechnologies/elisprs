;;; gen.el --- seeded generator for the elisprs differential fuzz corpus  -*- lexical-binding: t; -*-

;; Emits FUZZ_N random elisp forms, one per line, to stdout. The forms are the
;; corpus that `scripts/fuzz_parity.sh' feeds to BOTH `emacs -Q --batch' (the
;; ground truth) and `elisp' (the subject) through `drive.el'; any line whose
;; two results differ is a parity gap.
;;
;;   FUZZ_SEED   PRNG seed (default 1). Same seed => byte-identical corpus.
;;   FUZZ_N      number of forms (default 200).
;;   FUZZ_DEPTH  max expression nesting (default 3).
;;
;; The PRNG is a 32-bit xorshift built only from `logand'/`logxor'/`ash', so it
;; never leaves fixnum range: the corpus is identical whether the generator is
;; run under Emacs or under elisprs itself.
;;
;; Every generated form must be PURE and BOUNDED — the two engines are separate
;; processes, so anything reading the clock, the filesystem, a buffer, `random',
;; or a gensym counter would diverge without being a bug. Sizes (`make-string',
;; `number-sequence', `expt' exponents) are drawn from small pools so a form
;; cannot allocate its way out of the process.

;;; ── PRNG ─────────────────────────────────────────────────────────────────────

(defvar fz-state 1)

(defun fz-next ()
  "Next 32-bit xorshift word."
  (let ((s fz-state))
    (setq s (logand (logxor s (ash s 13)) #xFFFFFFFF))
    (setq s (logxor s (ash s -17)))
    (setq s (logand (logxor s (ash s 5)) #xFFFFFFFF))
    (setq fz-state s)
    s))

(defun fz-int (n)
  "Uniform-ish integer in [0, N)."
  (if (<= n 1) 0 (% (fz-next) n)))

(defun fz-pick (seq)
  "A random element of SEQ."
  (elt seq (fz-int (length seq))))

(defun fz-chance (percent)
  "Non-nil PERCENT of the time."
  (< (fz-int 100) percent))

;;; ── atom pools ───────────────────────────────────────────────────────────────

;; `most-positive-fixnum'/`most-negative-fixnum' are in the int pool on purpose:
;; they are where Emacs switches to bignums and where a 64-bit host wraps.
(defvar fz-ints
  '(0 1 2 3 -1 -2 5 7 8 10 16 42 -7 -42 100 255 256 1000 65535 -65536 123456789
    2305843009213693951 -2305843009213693952 4611686018427387903))
(defvar fz-small '(0 1 2 3 4 5 6 7))
(defvar fz-floats
  '(0.0 -0.0 1.0 -1.0 0.5 -1.5 3.14 2.5 0.1 1.0e+INF -1.0e+INF 0.0e+NaN 1e10 1e-10 1.5e300))
(defvar fz-strings
  '("" "a" "ab" "abc" "Hello, World" "hello world" "  padded  " "a,b,,c" "line\nbreak"
    "tab\there" "quote\"d" "back\\slash" "123" "-4.5" "ÜñîçøðÉ" "αβγ" "aAbB"))
(defvar fz-symbols '(foo bar baz nil t car - + a))
(defvar fz-fns
  '(car cdr 1+ 1- abs identity not null length upcase downcase symbol-name
    number-to-string string-to-number integerp stringp consp listp nlistp
    string-to-char char-to-string reverse cl-evenp cl-oddp zerop natnump))
;; Wrong-type arguments are a parity dimension of their own (Emacs's error data
;; is part of the contract), so a fraction of arguments are drawn from here
;; regardless of the slot's declared kind. Everything in the pool is cheap: a
;; chaos value can never make a form allocate.
(defvar fz-chaos '(nil t 'sym "str" "" 1.5 -1 0 [1 2] (list 1 2) 'car ?a))

(defun fz-atom (kind)
  "A literal of KIND."
  (cond
   ((eq kind 'int) (fz-pick fz-ints))
   ((eq kind 'small) (fz-pick fz-small))
   ((eq kind 'float) (fz-pick fz-floats))
   ((eq kind 'num) (if (fz-chance 60) (fz-pick fz-ints) (fz-pick fz-floats)))
   ((eq kind 'str) (fz-pick fz-strings))
   ((eq kind 'sym) (list 'quote (fz-pick fz-symbols)))
   ((eq kind 'bool) (fz-pick '(t nil)))
   ((eq kind 'char) (fz-pick '(?a ?z ?A ?0 ?\s ?\n ?\t ?é)))
   ((eq kind 'fn) (let ((f (fz-pick fz-fns)))
                    (if (fz-chance 20)
                        (list 'lambda '(x) (list (fz-pick '(list cons)) 'x 'x))
                      (list 'function f))))
   (t (fz-pick fz-chaos))))

;;; ── call table ───────────────────────────────────────────────────────────────

;; (NAME KIND...) — one KIND per argument slot. Slots are filled by `fz-of-kind',
;; which returns either a literal of that kind or a nested call whose result has
;; that kind, so the corpus is mostly type-correct and the interesting divergence
;; is in the *semantics*, not in the error path. `fz-chaos-rate' then breaks the
;; types back open on a fraction of slots to fuzz the error path too.
(defvar fz-calls
  '(;; arithmetic
    (+ num num) (+ num num num) (- num num) (- num) (* num num) (* num num num)
    (/ num num) (% int int) (mod num num) (max num num) (min num num) (abs num)
    (1+ num) (1- num) (expt num small) (truncate num) (floor num) (ceiling num)
    (round num) (float num) (ffloor float) (fceiling float) (ftruncate float)
    (fround float) (sqrt num) (exp num) (log num) (sin num) (cos num) (isnan float)
    (cl-evenp int) (cl-oddp int) (zerop num) (natnump any) (float-to-string float)
    ;; bits
    (logand int int) (logior int int) (logxor int int) (lognot int) (ash int small)
    (ash int int) (logcount int)
    ;; comparison / equality
    (= num num) (< num num) (> num num) (<= num num) (>= num num) (/= num num)
    (eq any any) (eql any any) (equal any any)
    (string= str str) (string< str str) (string> str str) (string-lessp str str)
    (string-equal-ignore-case str str) (string-prefix-p str str) (string-suffix-p str str)
    ;; lists
    (car list) (cdr list) (caar list) (cadr list) (cddr list) (cdar list)
    (cons any any) (list any any) (list any any any) (append list list)
    (reverse seq) (nth int list) (nthcdr int list) (last list) (last list small)
    (butlast list) (butlast list small) (length seq) (safe-length any) (elt seq int)
    (member any list) (memq any list) (memql any list) (assq any list) (assoc any list)
    (rassq any list) (rassoc any list) (delete any list) (delq any list) (remove any list)
    (remq any list) (flatten-tree list) (number-sequence small small)
    (number-sequence small small small) (make-list small any) (copy-sequence seq)
    (proper-list-p any) (take small list) (nreverse list) (nconc list list)
    (alist-get any list) (plist-get list any) (plist-member list any)
    (assq-delete-all any list) (delete-dups list)
    ;; higher order
    (mapcar fn seq) (mapconcat fn seq) (mapconcat fn seq str) (mapcan fn list)
    (apply fn list) (funcall fn any) (sort list fn) (sort seq fn)
    (seq-filter fn seq) (seq-remove fn seq) (seq-map fn seq) (seq-elt seq int)
    (seq-take seq int) (seq-drop seq int) (seq-uniq seq) (seq-contains-p seq any)
    (seq-position seq any) (seq-min seq) (seq-max seq) (seq-find fn seq)
    (seq-count fn seq) (seq-partition seq small) (seq-reverse seq) (seq-empty-p seq)
    (seq-difference seq seq) (seq-intersection seq seq) (seq-union seq seq)
    (seq-subseq seq int) (seq-sort fn seq) (seq-length seq) (seq-reduce fn seq any)
    ;; strings
    (concat str str) (concat str str str) (substring str int) (substring str int int)
    (string-to-number str) (number-to-string num) (char-to-string char)
    (string-to-char str) (upcase any) (downcase any) (capitalize any)
    (upcase-initials str) (string-trim str) (string-trim-left str) (string-trim-right str)
    (split-string str) (split-string str str) (split-string str str bool)
    (string-join list) (string-join list str) (string-search str str)
    (string-replace str str str) (string-to-list str) (string-to-vector str)
    (make-string small char) (string-pad str small) (string-distance str str)
    (string-reverse str) (string-empty-p str) (string-width str) (regexp-quote str)
    (string-match str str) (replace-regexp-in-string str str str) (string-remove-prefix str str)
    (string-remove-suffix str str) (format str any) (format str any any)
    (prin1-to-string any) (intern str) (symbol-name sym) (type-of any)
    ;; vectors / sequences
    (aref vec int) (vconcat seq seq) (vector any any) (append vec list)
    ;; predicates
    (consp any) (listp any) (atom any) (null any) (not any) (stringp any) (symbolp any)
    (vectorp any) (arrayp any) (sequencep any) (functionp any) (booleanp any)
    (integerp any) (floatp any) (numberp any) (fixnump any) (bignump any)))

;; Slots whose value must stay small for the form to stay bounded, no matter what
;; the table says: a chaos int in `make-string' would allocate gigabytes.
(defvar fz-bounded '(small char))

(defvar fz-chaos-rate 12
  "Percent of argument slots filled with a deliberately wrong-typed value.")

;;; ── expression builder ───────────────────────────────────────────────────────

(defun fz-callp (kind)
  "Call specs whose result plausibly has KIND — nil means \"any spec\"."
  (cond
   ((eq kind 'int) '((length seq) (string-to-char str) (logand int int) (logxor int int)
                     (ash int small) (logcount int) (1+ int) (1- int) (abs int)
                     (truncate num) (floor num) (round num) (% int int)
                     (string-distance str str) (string-width str)))
   ((eq kind 'num) '((+ num num) (- num num) (* num num) (max num num) (min num num)
                     (abs num) (float num) (sqrt num) (expt num small) (mod num num)))
   ((eq kind 'str) '((concat str str) (upcase str) (downcase str) (capitalize str)
                     (number-to-string num) (substring str int) (string-trim str)
                     (symbol-name sym) (prin1-to-string any) (format str any)
                     (char-to-string char) (make-string small char) (string-join list str)))
   ((eq kind 'list) '((list any any) (cons any any) (append list list) (reverse list)
                      (number-sequence small small) (make-list small any) (cdr list)
                      (mapcar fn seq) (seq-filter fn seq) (string-to-list str)
                      (split-string str str)))
   ((eq kind 'vec) '((vector any any) (vconcat seq seq) (string-to-vector str)))
   (t nil)))

(defun fz-of-kind (kind depth)
  "An expression of KIND with at most DEPTH more levels of nesting."
  (cond
   ;; Bounded slots are never chaos-filled and never nested: they are the reason
   ;; a fuzzed form cannot allocate without limit.
   ((memq kind fz-bounded) (fz-atom kind))
   ((fz-chance fz-chaos-rate) (fz-pick fz-chaos))
   ((<= depth 0) (fz-leaf kind))
   ((eq kind 'any) (fz-expr depth))
   ((eq kind 'seq)
    (fz-of-kind (fz-pick '(list str vec)) depth))
   ((fz-chance 45)
    (let ((specs (fz-callp kind)))
      (if specs (fz-build (fz-pick specs) (1- depth)) (fz-leaf kind))))
   (t (fz-leaf kind))))

(defun fz-leaf (kind)
  "A literal (never a call) of KIND."
  (cond
   ((eq kind 'list)
    (let ((n (fz-int 4)) (acc nil) (i 0))
      (while (< i n)
        (push (fz-atom (fz-pick '(int str sym float bool))) acc)
        (setq i (1+ i)))
      ;; Half quoted literal, half freshly consed: destructive builtins
      ;; (`nreverse', `nconc', `delete-dups') must not chew on a literal.
      (if (fz-chance 50)
          (cons 'list acc)
        (list 'quote (mapcar (lambda (x) (if (and (consp x) (eq (car x) 'quote)) (cadr x) x))
                             acc)))))
   ((eq kind 'vec)
    (let ((n (fz-int 4)) (acc nil) (i 0))
      (while (< i n)
        (push (fz-atom (fz-pick '(int str sym bool))) acc)
        (setq i (1+ i)))
      (cons 'vector acc)))
   ((eq kind 'seq) (fz-leaf (fz-pick '(list str vec))))
   ((eq kind 'any) (fz-atom (fz-pick '(int float str sym bool char))))
   (t (fz-atom kind))))

(defun fz-build (spec depth)
  "Build a call from SPEC = (NAME KIND...)."
  (cons (car spec)
        (mapcar (lambda (k) (fz-of-kind k depth)) (cdr spec))))

(defun fz-control (depth)
  "A random control-flow / binding form."
  (let ((d (1- depth)))
    (cond
     ((fz-chance 14) (list 'if (fz-expr d) (fz-expr d) (fz-expr d)))
     ((fz-chance 12) (list 'and (fz-expr d) (fz-expr d)))
     ((fz-chance 12) (list 'or (fz-expr d) (fz-expr d)))
     ((fz-chance 12) (list 'let (list (list 'x (fz-expr d)))
                           (list (fz-pick '(list cons)) 'x (fz-expr d))))
     ((fz-chance 12) (list 'let* (list (list 'x (fz-expr d)) (list 'y (list 'list 'x 'x)))
                           (list 'cons 'y (fz-expr d))))
     ((fz-chance 12) (list 'cond (list (fz-expr d) (fz-expr d)) (list t (fz-expr d))))
     ((fz-chance 12) (list 'when (fz-expr d) (fz-expr d)))
     ((fz-chance 12) (list 'catch (list 'quote 'tag)
                           (list 'throw (list 'quote 'tag) (fz-expr d))))
     ((fz-chance 20) (list 'ignore-errors (fz-expr d)))
     ((fz-chance 30) (list 'dotimes (list 'i (fz-atom 'small)) (fz-expr d)))
     (t (list 'progn (fz-expr d) (fz-expr d))))))

(defun fz-expr (depth)
  "A random expression with at most DEPTH levels of nesting."
  (cond
   ((<= depth 0) (fz-leaf 'any))
   ((fz-chance 22) (fz-leaf (fz-pick '(any any list vec str))))
   ((fz-chance 12) (fz-control depth))
   (t (fz-build (fz-pick fz-calls) (1- depth)))))

;;; ── main ─────────────────────────────────────────────────────────────────────

(let* ((seed (string-to-number (or (getenv "FUZZ_SEED") "1")))
       (n (string-to-number (or (getenv "FUZZ_N") "200")))
       (depth (string-to-number (or (getenv "FUZZ_DEPTH") "3")))
       (i 0))
  (setq fz-state (if (zerop seed) 1 (logand seed #xFFFFFFFF)))
  ;; One form per line is the corpus contract, and `prin1' prints a newline
  ;; inside a string literally unless this is set.
  (setq print-escape-newlines t)
  ;; Discard the first few words: a small seed's first xorshift outputs are
  ;; poorly mixed, which would make low seeds generate near-identical corpora.
  (dotimes (_ 8) (fz-next))
  (while (< i n)
    ;; One form per line — `drive.el' reads the corpus line by line, so a form
    ;; must never contain a raw newline. Strings with \n print escaped, so
    ;; `prin1' output is always single-line.
    (princ (prin1-to-string (fz-expr depth)))
    (terpri)
    (setq i (1+ i))))

;;; gen.el ends here
