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
;; The large magnitudes matter: a float whose integer part does not fit an i64 is
;; how `(floor 1e30 3)' told apart an exact bignum quotient from a saturated
;; `i64::MAX'. The NaNs are spelled with different mantissas on purpose — Emacs
;; stores a token's leading integer in the NaN's significand and prints it back,
;; so `3.0e+NaN' and `0.0e+NaN' are distinguishable values.
(defvar fz-floats
  '(0.0 -0.0 1.0 -1.0 0.5 -1.5 3.14 2.5 0.1 1.0e+INF -1.0e+INF 0.0e+NaN 3.0e+NaN
    -0.0e+NaN 1e10 1e-10 1.5e300 1e30 -1e30 1e19 9.3e18 -9.3e18))
(defvar fz-strings
  '("" "a" "ab" "abc" "Hello, World" "hello world" "  padded  " "a,b,,c" "line\nbreak"
    "tab\there" "quote\"d" "back\\slash" "123" "-4.5" "ÜñîçøðÉ" "αβγ" "aAbB"
    ;; Version-ish and filename-ish spellings: `string-version-lessp' is gnulib
    ;; filevercmp, where a leading ".", a "~", a file suffix and a digit run each
    ;; take a different branch. Plain words never reach any of them.
    "foo2" "foo10" "foo01" "1.2" "1.10" "." ".." ".hidden" "~" "a~1" "x-1"
    "foo2.png" "foo12.png" "a.tar.gz" "0" "00" "_" "!" "a1b2"))
;; Non-finite and payload-carrying float *tokens*, as strings, for the
;; read/print/`string-to-number' round trip.
(defvar fz-float-tokens
  '("1.0e+INF" "-1.0e+INF" "0.0e+NaN" "-0.0e+NaN" "3.7e+NaN" "123.0e+NaN" ".5e+NaN"
    "1e+INF" "1.e+INF" ".5e+INF" "1.0e-INF" "1.0eINF" "1.0E+INF" "1.0e+inf"
    "1.0e+NAN" "e+INF" "+1.0e+INF" "1.0e+INFx" "0e+NaN" "1." ".5" "1e5"))
(defvar fz-symbols '(foo bar baz nil t car - + a))
;; Symbols in *function* position for the introspection subrs (`func-arity',
;; `fboundp', `symbol-function', `indirect-function', `special-form-p',
;; `macrop'). A subr, a special form, an alias, a name nothing defines, and the
;; two symbols that are symbols in Emacs but not heap symbols here (`t'/`nil')
;; each take a different branch — and elisprs lowers every special form in the
;; compiler, which is exactly where it had no function cell at all.
;;
;; Deliberately all C-level: `symbol-function' of anything Emacs defines in Lisp
;; answers a *byte-code object*, which elisprs (having no byte compiler) prints
;; as its source closure. That difference is real but it is not what these calls
;; are here to measure, and it would make every run report it.
(defvar fz-fnames
  '(car cons list + not if let progn quote while catch unwind-protect
    no-such-function-xyz nil t))
;; Pure-ASCII strings for the base64 encoders. Their contract for a character in
;; 128-255 turns on whether the string is unibyte, which elisprs does not track:
;; Emacs reads "\303\251" as two raw bytes (encodable) and "\351" as one
;; multibyte character (rejected), and both are two-character/one-character
;; strings of the same codes here. Above 255 there is no ambiguity and elisprs
;; signals with Emacs, so the pool stays below 128 rather than measuring a gap
;; the fuzzer cannot act on.
(defvar fz-ascii-strings
  '("" "a" "ab" "abc" "abcd" "Hello, World" "hello world" "  padded  " "a,b,,c"
    "line\nbreak" "tab\there" "quote\"d" "back\\slash" "123" "-4.5" "aAbB" "a1b2"))
;; `secure-hash' algorithm names, plus one it does not have: the rejection
;; message is as much of the contract as the digest.
(defvar fz-algos '(md5 sha1 sha224 sha256 sha384 sha512 bogus))
;; base64 tokens. The malformed ones are the point: Emacs's decoder is strict
;; about quadruple length and about where `=' may appear, and a decoder written
;; as a loose bit stream accepts every one of them silently.
;; Every token here decodes to ASCII, because a decoded byte above 127 lands in
;; the unibyte-string gap: the value is right (`string-to-list' agrees) but Emacs
;; prints such a string as octal escapes and elisprs prints the characters.
;; `"-_-_"' is still present for the alphabet check — in the padded mode it is an
;; error, which is ASCII either way.
(defvar fz-b64
  '("" "YWJj" "YQ==" "YWI=" "YWJjZA==" "AAAA" "YQ==YQ==" "  YWJj  " "YWJj\n"
    "YWJ" "YWJj=" "=" "====" "A===" "AB=C" "!!!!" "SGVsbG8=" "MTIz"))
;; `format-time-string' formats. Every call passes an explicit time and a
;; non-nil ZONE (UTC), so nothing here reads the clock or the local zone.
(defvar fz-tfmts
  '("%Y-%m-%d" "%H:%M:%S" "%Y-%m-%dT%H:%M:%S" "%j" "%A %B" "%s" "%%" "%F" "%T"
    "%y" "%m/%d" "%e" "%p" "%Z" "%a %b %e %H:%M:%S %Y"))
;; Bounded absolute times (seconds since the epoch) for the time formatters.
(defvar fz-times '(0 1 86399 86400 1000000000 -1 951782400 2147483647))
;; Emacs regexp syntax, not the `regex`-crate dialect: grouping and alternation
;; are backslashed. A few are deliberately malformed — an invalid regexp is a
;; parity case of its own (`invalid-regexp` and its message text).
(defvar fz-regexps
  '("a" "a+" "a*b" "[a-z]+" "[^a-z]" "\\(a\\)\\1" "\\(a+\\)\\(b\\)?" "^a" "b$" "\\`a" "a\\'"
    "\\<a\\>" "\\_<a\\_>" "\\w+" "\\W" "\\s-" "[[:alpha:]]+" "[[:digit:]]" "a\\{2,3\\}"
    "\\(?:ab\\|cd\\)" "a\\|b" "." "\\." "" "[" "\\(" "a\\{" "*a" "[z-a]"))

;; `format' control strings: each pairs with the argument kinds it consumes.
(defvar fz-formats
  '(("%s" any) ("%S" any) ("%d" num) ("%d%%" num) ("%x" int) ("%X" int) ("%o" int)
    ("%c" char) ("%e" num) ("%f" num) ("%g" num) ("%5d" num) ("%-8s" any) ("%+d" num)
    ("%#x" int) ("%#o" int) ("%.3s" str) ("%5.2f" num) ("%08.3f" num) ("%2$s %1$s" any any)
    ("[%s]" any) ("%s-%s" any any) ("%d/%d" num num)))

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
   ((eq kind 're) (fz-pick fz-regexps))
   ((eq kind 'ftok) (fz-pick fz-float-tokens))
   ((eq kind 'fname) (list 'quote (fz-pick fz-fnames)))
   ((eq kind 'algo) (list 'quote (fz-pick fz-algos)))
   ((eq kind 'b64) (fz-pick fz-b64))
   ((eq kind 'astr) (fz-pick fz-ascii-strings))
   ((eq kind 'bstr) (fz-pick fz-strings))
   ((eq kind 'tfmt) (fz-pick fz-tfmts))
   ((eq kind 'time) (fz-pick fz-times))
   ;; A bool-vector, built inline so the corpus stays a single expression.
   ((eq kind 'bv)
    (cons 'bool-vector
          (let ((n (fz-int 4)) (acc nil) (i 0))
            (while (< i n) (push (fz-pick '(t nil)) acc) (setq i (1+ i)))
            acc)))
   ;; A freshly-allocated vector: `aset' and `fillarray' mutate their argument,
   ;; so they must never be handed a literal shared with a later form.
   ((eq kind 'freshvec)
    (cons 'vector
          (let ((n (1+ (fz-int 4))) (acc nil) (i 0))
            (while (< i n)
              (push (fz-atom (fz-pick '(int str sym bool))) acc)
              (setq i (1+ i)))
            acc)))
   ;; An improper list. A search that finds its item before the tail must return
   ;; normally; one that runs off the end must signal `(wrong-type-argument listp
   ;; TAIL)'. Both halves of that are parity surface, and a proper list tests
   ;; neither.
   ((eq kind 'dotted)
    (let ((head (fz-atom (fz-pick '(int str sym bool))))
          (tail (fz-pick '(t 'sym 5 "s"))))
      (if (fz-chance 50)
          (list 'cons head tail)
        (list 'cons (fz-atom 'int) (list 'cons head tail)))))
   ((eq kind 'ht)
    ;; A hash table built inline: `(let ((h (make-hash-table …))) (puthash …) h)`.
    (let ((test (fz-pick '(eq eql equal)))
          (k (fz-atom (fz-pick '(int str sym))))
          (v (fz-atom (fz-pick '(int str bool)))))
      (list 'let (list (list 'h (list 'make-hash-table :test (list 'quote test))))
            (list 'puthash k v 'h)
            'h)))
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
    ;; One-argument `+'/`*' still type-check their operand: `(+ t)' signals, it
    ;; does not answer `t'. Four operands fold left with every argument
    ;; evaluated first, which a chain of binary opcodes does not do.
    (+ num) (* num) (- num num num) (+ num num num num) (* num num num num)
    (/ num num) (% int int) (mod num num) (max num num) (min num num) (abs num)
    (1+ num) (1- num) (expt num small) (truncate num) (floor num) (ceiling num)
    ;; The two-argument rounding forms take a completely different path from the
    ;; one-argument ones: with a float operand Emacs divides *exactly*, so the
    ;; quotient can be a bignum no `f64' could hold.
    (truncate num num) (floor num num) (ceiling num num) (round num num)
    (cl-truncate num num) (cl-ceiling num num)
    (round num) (float num) (ffloor float) (fceiling float) (ftruncate float)
    (fround float) (sqrt num) (exp num) (log num) (sin num) (cos num) (isnan float)
    (cl-evenp int) (cl-oddp int) (zerop num) (natnump any)
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
    ;; regexp
    (string-match re str) (string-match-p re str) (string-match re str small)
    (replace-regexp-in-string re str str) (replace-regexp-in-string re str str bool)
    (regexp-quote str) (split-string str re) (split-string str re bool)
    (string-trim str re re)
    ;; hash tables (built and read in one form so the corpus stays pure)
    (hash-table-count ht) (hash-table-test ht) (hash-table-p any) (hash-table-keys ht)
    (hash-table-values ht) (gethash any ht) (gethash any ht any)
    ;; plists / alists
    (plist-get list any) (plist-member list any) (plist-put freshlist any any)
    (alist-get any list) (alist-get any list any) (assoc-default any list)
    (assq-delete-all any list) (rassq-delete-all any list) (assoc-string any list)
    ;; text properties
    (propertize str sym any) (substring-no-properties str)
    (get-text-property small sym str) (text-properties-at small str)
    (equal-including-properties any any)
    ;; symbols / obarray
    (intern str) (intern-soft str) (make-symbol str) (symbol-name sym) (symbolp any)
    (type-of any) (subrp any) (functionp any) (macrop any) (special-form-p any)
    ;; cl-lib
    (cl-remove-duplicates list) (cl-position any list) (cl-count any list)
    (cl-find any list) (cl-some fn list) (cl-every fn list) (cl-remove-if fn list)
    (cl-subseq list small) (cl-reduce fn list) (cl-sort freshlist fn)
    (cl-evenp int) (cl-oddp int) (cl-plusp num) (cl-minusp num)
    ;; case / chars
    (capitalize any) (upcase-initials str) (char-equal char char) (string-to-char str)
    (char-to-string char) (string-width str) (string-reverse str)
    ;; printing
    (prin1-to-string any) (prin1-to-string any bool) (format-message str any)
    ;; error objects: the symbol, its condition chain and its rendered message are
    ;; as much of the contract as any return value
    (error-message-string list) (type-of any)
    ;; sequence/list corners the call table did not reach
    (last list small) (nthcdr int list) (take small list) (ntake small freshlist)
    (seq-take-while fn seq) (seq-drop-while fn seq) (seq-map-indexed fn seq)
    (seq-mapn fn seq seq) (seq-split list small) (seq-keep fn list)
    (seq-positions list any) (seq-into seq sym) (seq-first seq) (seq-rest seq)
    (cl-remove-if-not fn list) (cl-set-difference list list) (cl-union list list)
    (cl-intersection list list) (cl-list* any any list) (cl-ldiff list list)
    (cl-signum num) (cl-gcd int int) (cl-lcm int int) (cl-floor num num)
    (cl-round num num) (cl-mod num num) (cl-rem num num) (cl-adjoin any list)
    ;; Short-circuiting cl-seq searches: these are the ones that may stop before
    ;; an improper tail, so they must walk the list in place rather than
    ;; normalizing it through `append' (which signals `listp' up front).
    (cl-position any seq) (cl-find any seq) (cl-position-if fn seq) (cl-find-if fn seq)
    (cl-position any dotted) (cl-find any dotted)
    (cl-position-if fn dotted) (cl-find-if fn dotted)
    (cl-count any seq) (cl-some fn dotted) (cl-every fn dotted)
    ;; Non-finite / payload-carrying float tokens through read and print.
    (string-to-number ftok) (read ftok) (number-to-string float) (prin1-to-string float)
    ;; string corners
    (compare-strings str any any str any any) (string-version-lessp str str)
    (string-pad str small char) (string-pad str small char bool)
    (assoc-string any list bool) (split-string str re bool str)
    ;; hashing / encoding. Pure and deterministic, and the whole area was
    ;; unreached by the call table: the digests, the strict base64 reader, and
    ;; the byte-per-character contract both encoders and `url-unhex-string' owe.
    ;; The digest object slot is bounded rather than chaos-filled: Emacs's
    ;; `Invalid object argument' data for a *list* object splices oddly (a proper
    ;; list is spliced, an improper one is not, and `nil' arrives as the string
    ;; "nil"), which is recorded in BUGS.md as an unclosed gap. The digests, the
    ;; algorithm rejection and the START/END range check are what these measure.
    (md5 bstr) (sha1 bstr) (secure-hash algo bstr)
    (secure-hash algo astr small small) (secure-hash algo astr small any)
    (md5 astr small small) (sha1 astr small small)
    (base64-encode-string astr) (base64-encode-string astr bool)
    (base64-decode-string b64) (base64-decode-string b64 bool)
    (base64url-encode-string astr) (base64url-encode-string astr bool)
    ;; `url-hexify-string' is a Lisp `mapconcat' over `url-unreserved-chars', so
    ;; a bad element reports that bool-vector's own `aref' error (`fixnump' /
    ;; `args-out-of-range' naming the table). That surface is the library's
    ;; internals rather than the function's contract, so the slot stays a string.
    (url-hexify-string bstr) (url-unhex-string bstr)
    ;; The float library beyond the elementary functions. `copysign' type-checks
    ;; with `floatp', not the number check every neighbour uses.
    (frexp float) (ldexp float small) (ldexp num small) (logb num)
    (copysign float float) (copysign num num) (tan num) (asin num) (acos num)
    (atan num) (atan num num) (lsh int small) (byteorder)
    ;; Function introspection. A special form, a compiler-lowered macro and an
    ;; undefined name each answer differently, and elisprs lowers the first two.
    (func-arity fname) (fboundp fname) (subrp fname) (macrop fname)
    (special-form-p fname) (indirect-function fname) (symbol-function fname)
    (functionp fname) (keywordp any) (bare-symbol-p any) (subr-name any)
    ;; Characters and byte strings.
    (char-width char) (char-uppercase-p char) (text-char-description char)
    (string char) (string char char) (char-or-string-p any) (max-char)
    ;; Mutable arrays: `aset'/`fillarray' return and mutate, so both the value
    ;; and the mutated array are compared.
    (make-vector small any) (aset freshvec small any) (fillarray freshvec any)
    (bool-vector-not bv) (bool-vector-subsetp bv bv) (bool-vector-p any)
    (make-bool-vector small bool) (length bv) (append bv list)
    ;; Time formatting, always with an explicit time and UTC so nothing here
    ;; reads the clock or the ambient zone.
    (format-time-string tfmt time bool) (decode-time time bool) (float-time time)
    ;; The reader as a function, including its `end-of-file' data.
    (read-from-string str) (read-from-string str small)
    (string-to-number str small) (member-ignore-case any list)
    ;; predicates
    (consp any) (listp any) (atom any) (null any) (not any) (stringp any) (symbolp any)
    (vectorp any) (arrayp any) (sequencep any) (functionp any) (booleanp any)
    (integerp any) (floatp any) (numberp any) (fixnump any) (bignump any)))

;; Slots whose value must stay small for the form to stay bounded, no matter what
;; the table says: a chaos int in `make-string' would allocate gigabytes.
;; `dotted' and `ftok' are here for a different reason than the others: not size,
;; but that chaos-filling or nesting them would destroy the only thing they test.
;; A `dotted' slot holding a proper list, or an `ftok' slot holding "abc", is a
;; slot that has stopped covering improper tails and non-finite float syntax.
;; `fname', `algo', `b64', `tfmt', `time', `bv' and `freshvec' join for the same
;; reason `dotted' and `ftok' did: chaos-filling or nesting them destroys the only
;; thing they test (a function designator, a digest name, a base64 token, a time
;; format, a bounded epoch second, a bool-vector, an unshared vector).
(defvar fz-bounded
  '(small char re ht dotted ftok fname algo b64 astr bstr tfmt time bv freshvec))

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
   ((eq kind 'freshlist) (fz-leaf 'list))
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

(defun fz-format-form (depth)
  "A `format' call whose control string matches its argument kinds."
  (let* ((spec (fz-pick fz-formats))
         (ctl (car spec))
         (kinds (cdr spec))
         (args (mapcar (lambda (k) (fz-of-kind k (1- depth))) kinds)))
    (cons 'format (cons ctl args))))

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
     ;; The printer's dynamic variables change what `prin1' emits, which is a
     ;; parity surface of its own.
     ((fz-chance 14)
      (list 'let (list (list (fz-pick '(print-length print-level))
                            (fz-atom 'small)))
            (list 'prin1-to-string (fz-expr d))))
     ((fz-chance 14)
      (list 'let (list (list (fz-pick '(print-escape-newlines
                                        print-escape-control-characters
                                        print-quoted print-circle))
                            t))
            (list 'prin1-to-string (fz-expr d))))
     ;; Shared and circular structure under `print-circle'. Built by mutation
     ;; (`setcdr'/`setcar' on a freshly consed list) because that is the only way
     ;; to make a cycle, and printed rather than returned — `drive.el' would
     ;; itself have to print a cycle otherwise. Bounded: the list is 2 conses.
     ((fz-chance 10)
      (list 'let (list (list 'print-circle (fz-pick '(t nil))))
            (list 'prin1-to-string
                  (fz-pick
                   (list
                    ;; circular through the cdr / through the car
                    '(let ((x (list 1 2))) (setcdr (cdr x) x) x)
                    '(let ((x (list 1 2))) (setcar x x) x)
                    ;; shared but acyclic, in a list / in a vector
                    '(let ((y (list 1 2))) (list y y))
                    '(let ((y (list 1 2))) (vector y y))
                    '(let ((y (list 1))) (list y (list y) y)))))))
     ;; Error handling: the signalled symbol, its condition chain, the handler
     ;; that catches it, and the interaction with `unwind-protect' are all part of
     ;; the contract. A `t' handler keeps every generated form catchable, so an
     ;; uncaught non-`error' signal can never kill the driver mid-corpus.
     ((fz-chance 14)
      (let ((sig (fz-pick '(error quit arith-error wrong-type-argument
                            args-out-of-range void-function end-of-file
                            user-error cl-assertion-failed)))
            (handler (fz-pick '(error quit arith-error wrong-type-argument t))))
        (list 'condition-case 'e
              (list 'signal (list 'quote sig) (list 'quote (fz-pick '(nil (1) ("x" 2)))))
              (list handler (fz-pick '(e '(caught) (car e) (cdr e))))
              (list t ''fellthrough))))
     ((fz-chance 12)
      ;; unwind-protect: does the cleanup run, and does a cleanup that itself
      ;; signals supersede the body?
      (list 'let (list (list 'n 0))
            (list 'list
                  (list 'condition-case 'e
                        (list 'unwind-protect (fz-expr d)
                              (fz-pick (list '(setq n 1) '(error "cleanup") '(setq n (1+ n)))))
                        (list 'error '(cadr e))
                        (list t ''other))
                  'n)))
     ((fz-chance 12)
      ;; catch/throw crossing a loop and an unwind-protect.
      (list 'catch (list 'quote 'tag)
            (list (fz-pick '(dolist dotimes))
                  (if (fz-chance 50) (list 'i (fz-atom 'small)) (list 'i ''(1 2 3)))
                  (list 'unwind-protect
                        (list 'when (list 'equal 'i (fz-atom 'small))
                              (list 'throw (list 'quote 'tag) 'i))
                        nil))))
     ;; cl-loop clause shapes. `while'/`until'/`downfrom' and the accumulator vs
     ;; abnormal-exit distinction are their own parity surface.
     ((fz-chance 14)
      (let ((lim (fz-atom 'small)))
        (fz-pick
         (list
          (list 'cl-loop 'for 'i 'from 1 'to 4 'collect 'i)
          (list 'cl-loop 'for 'i 'downfrom 4 'to lim 'collect 'i)
          (list 'cl-loop 'for 'i 'downfrom 4 'above lim 'collect 'i)
          (list 'cl-loop 'for 'i 'in ''(1 2 3 4) 'while (list '< 'i lim) 'collect 'i)
          (list 'cl-loop 'for 'i 'in ''(1 2 3 4) 'until (list '> 'i lim) 'collect 'i)
          (list 'cl-loop 'for 'i 'in ''(1 2 3 4) 'while (list '< 'i lim)
                'collect 'i 'finally 'return ''fin)
          (list 'cl-loop 'for 'i 'from 1 'to 4 'do (list 'when (list '> 'i lim)
                                                         (list 'cl-return 'i)))
          (list 'cl-loop 'for 'i 'from 1 'to 4 'when (list '= 'i lim) 'return 'i)
          (list 'cl-loop 'for 'i 'from 1 'to 4 'always (list '< 'i lim))
          (list 'cl-loop 'for 'i 'from 1 'to 4 'never (list '> 'i lim))
          (list 'cl-loop 'for 'i 'from 1 'to 4 'thereis (list 'and (list '= 'i lim) ''yes))
          (list 'cl-loop 'for 'i 'from 1 'to 4 'sum 'i)))))
     ;; A macro defined and used in the same enclosing form: the expander has to
     ;; install it before it reaches the use site.
     ((fz-chance 8)
      (list 'progn
            (list 'defmacro 'fzgm '(a) (list 'list '(quote list) 'a 'a))
            (list 'fzgm (fz-atom (fz-pick '(int str sym))))))
     ((fz-chance 30) (list 'dotimes (list 'i (fz-atom 'small)) (fz-expr d)))
     (t (list 'progn (fz-expr d) (fz-expr d))))))

(defun fz-arity-form ()
  "A wrong-arity call whose surplus (or sole) argument has a side effect.

WHEN the arity is checked is a parity surface of its own, and it is invisible
in both the value and the signalled error -- those already agree.  Emacs checks
a SUBR's arity in `eval_sub''s argument-count switch, before any argument form
runs, so `n' stays 0; a CLOSURE's arguments are evaluated into a vector before
`funcall_lambda' compares counts, so `n' becomes 9.  Only reading `n' afterwards
separates the two.

Reaching the callee through `fset'/`defalias' covers the case where it is not
statically visible at the call site at all -- which is exactly where a
compile-time-only arity check silently does nothing.  The indirection always
goes on `fzwa', never on the real subr: `(fset 'car ...)' would leak into every
later form in the corpus."
  (let* ((spec (fz-pick '((car 2) (cdr 2) (cons 1) (cons 3) (nth 1)
                          (point 1) (length 2) (symbol-name 2) (1+ 2))))
         (fn (nth 0 spec))
         (argc (nth 1 spec))
         (args (let ((acc (list (list 'setq 'n 9))) (i 1))
                 (while (< i argc) (push 1 acc) (setq i (1+ i)))
                 acc)))
    (cond
     ((fz-chance 40)
      (list 'let '((n 0)) (list 'ignore-errors (cons fn args)) 'n))
     ((fz-chance 25)
      (list 'let '((n 0))
            (list 'defalias ''fzwa (list 'quote fn))
            (list 'ignore-errors (cons 'fzwa args))
            'n))
     ((fz-chance 25)
      (list 'let '((n 0))
            (list 'fset ''fzwa (list 'symbol-function (list 'quote fn)))
            (list 'ignore-errors (cons 'fzwa args))
            'n))
     ;; The closure control: the same shape must still evaluate its arguments,
     ;; so a check that over-fires shows up here as `0'.
     (t
      (list 'let '((n 0))
            (list 'defun 'fzwc '(a) 'a)
            (list 'ignore-errors (cons 'fzwc args))
            'n)))))

(defun fz-expr (depth)
  "A random expression with at most DEPTH levels of nesting."
  (cond
   ((<= depth 0) (fz-leaf 'any))
   ((fz-chance 22) (fz-leaf (fz-pick '(any any list vec str))))
   ((fz-chance 12) (fz-control depth))
   ((fz-chance 8) (fz-format-form depth))
   ((fz-chance 6) (fz-arity-form))
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
