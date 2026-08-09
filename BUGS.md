# elisprs — known parity bugs vs Emacs Lisp

Goal: behavioral parity with real Emacs Lisp. Each entry below is a **reproduced
divergence** between `elisp -e EXPR` and `emacs -Q --batch --eval '(prin1 EXPR)'`,
checked against **GNU Emacs 30.2**.

Repro helpers:

```sh
E=./target/debug/elisp
elref() { emacs -Q --batch --eval "(prin1 $1)" 2>&1; }   # ground truth
```

---

## Differential fuzzing vs Emacs 30.2 — ✅ FIXED

`scripts/fuzz_parity.sh` generates a seeded corpus of random elisp forms and runs
every one under both `emacs -Q --batch` (ground truth) and `elisp`, comparing the
value *and* the signalled error. It reduced a 2,000-form corpus from 334 diverging
forms to 119, and the residue is error-data shape in corners (a closure's printed
form inside `wrong-number-of-arguments`, `invalid-function` data, `args-out-of-range`
on some `seq-*` paths). What it found and what was fixed:

- **Integer overflow wrapped instead of promoting.** Emacs's integers are
  unbounded; `(expt 2 70)` answered `0`, `(* 1000000000000 1000000000000)` wrapped.
  elisprs now has bignums (`Obj::Bignum`), and integers promote everywhere: the
  arithmetic ops, `expt`/`ash`/`lsh`/`abs`/bitwise, `/` `%` `mod`, rounding, the
  reader (a literal too big for an `i64` silently became a *float*), the printer,
  `eq`/`eql`/`equal`, `sxhash`, `format`, `number-to-string`, `string-to-number`.
- **Arithmetic coerced non-numbers.** `(+ 1 "a")` answered `1.0` and
  `(min "str" 1)` answered `"str"` — fusevm's ops are awk-flavoured, and the
  compiler lowers elisp's `+`/`-`/`*`/comparisons straight to them. fusevm 0.14.6
  added a numeric hook so those ops delegate the cases they cannot compute; elisp
  now signals `(wrong-type-argument number-or-marker-p "a")`.
- **`(eval 2.5 t)` answered `2` the second time.** fusevm's block JIT returned its
  result register as `Value::Int` unconditionally, truncating any float chunk
  result once the cache was warm.
- **A warm script cache shadowed the `exp` builtin.** The heap image re-interned
  uninterned symbols; the prelude binds a local named `exp`. Cold run: `0.367…`.
  Warm run: `void-function exp`.
- **Comparison was lossy** above 2^53 (`=` compared `f64`s).
- **Float printing** did not follow Emacs's shortest-round-trip `%g`.
- **Error data** leaked raw heap handles (`(obj:154357)`), omitted the offending
  value, and used one predicate where Emacs uses four (`numberp` /
  `number-or-marker-p` / `integerp` / `integer-or-marker-p`).
- **Regexp errors** leaked `fancy-regex`'s parser text; Emacs's own messages and
  tolerances (`"*x"`, `[z-a]`) are reproduced now.
- `match-data` trailing unmatched groups, `print-escape-control-characters`,
  `seq-union` dedup, and the `string-suffix-p` / `nconc` / `string-to-vector` /
  `concat` / `remq` / `delq` / `string=` tolerances.

A second pass over the same corpus took it from 119 to 56 diverging forms and
found two more semantic bugs (not just error shapes):

- **`(eval FORM t)` evaluated FORM in the caller's lexical scope.**
  `(let ((x 5)) (eval 'x t))` returned 5; Emacs signals `(void-variable x)`. Any
  closure FORM created also captured — and printed — the caller's bindings.
- **A closure printed as `#<closure>`** rather than as its source
  (`#[(x) ((list x x)) (t)]`), which also made every `wrong-number-of-arguments`
  involving a closure or subr diverge.

Plus: argument checking order (`(max t 'foo)` names `t`), `seq-subseq` bounds,
`split-string` with an empty separator, `ash` with a bignum count, and the
`capitalize` / `sort` / `mapcar` / `format` / `intern` / `last` / `plist-get` type
contracts.

A third pass took the same corpus from 56 to 46 diverging forms, and found the
worst bug yet — one that has nothing to do with Emacs parity per se:

- **A handled error poisoned the identity of the next one.** The error message and
  the structured error object travel on separate channels and were never paired, so
  an object left behind by an already-caught error was picked up by the next error
  that only produced a message. `(condition-case e (progn (ignore-errors (error
  "boom")) (car 1)) (error e))` answered `(error "boom")` — the wrong condition
  entirely, which any `condition-case` dispatching on the symbol would then
  mis-handle.
- **A speculative function lookup left an error behind**: `macroexpand-1` probes a
  form's head to see whether it names a macro, and a failed probe registered an
  `invalid-function` object that then replaced the real error.

Plus: `min`/`max` as subrs, improper lists naming their tail, `substring` index
types, `seq-take`/`seq-drop` argument checks.

A fourth pass (three fresh 3,000-form corpora, seeds 20260716 / 424242 / 987654)
cleared the numeric/format/symbol-identity cluster:

- **`%d` erred on non-finite floats.** Emacs renders the word:
  `(format "%d%%" 0.0e+NaN)` → `"nan%"`, `(format "%+d" 1.0e+INF)` → `"+inf"` —
  space-padded to width (the `0` flag is ignored), the sign flags applying to
  infinities but never to NaN. `%o`/`%x`/`%X` instead signal `(overflow-error)`,
  and `%c` rejects any float (even integral) as a type mismatch.
- **`fround`/`ffloor`/`fceiling`/`ftruncate` accepted any number.** They are
  `CHECK_FLOAT` subrs: `(fround 0)` is `(wrong-type-argument floatp 0)`, and the
  error names the offender (`floatp`, not `numberp`).
- **`make-symbol`/`intern-soft` dropped the offender** from their `stringp` error
  data: `(make-symbol 1.5)` said `(wrong-type-argument stringp)` with no datum.
- **`(symbolp (= 1 2))` answered nil.** A computed nil traveled as a raw boolean
  that `symbolp` didn't recognize as the symbol nil.
- **Number-lookalike symbol names printed unescaped.** `(intern "0.0e+NaN")`
  printed `0.0e+NaN` (which reads back as a float); the printer's numeric-token
  check missed the non-finite float syntax, bignum-sized integers, and `1.`.
- **Error-object identity of the predicate pool.** `natnump`/`nlistp` were
  prelude lambdas, so `(sort '(…) #'natnump)` reported a closure where Emacs
  says `#<subr natnump>` — both are C subrs now. `not` was its own subr where
  Emacs has `(defalias 'not #'null)`, so `#'not` misprinted as `#<subr not>`.
  Byte-compiled `cl-evenp`/`cl-oddp` report a wrong argument count as
  exec_byte_code's `((MANDATORY . NONREST) NARGS)` cons — `(1 . 1)`, never a
  printed closure. `substring-no-properties` likewise became the C subr it is
  in Emacs (with its `stringp`-before-indices check).
- **Text-property fns validated OBJECT with `bufferp`.** Emacs uses
  `buffer-or-string-p` (`(get-text-property 1 't 97)`), and one `propertize`
  call covering a range is ONE printed interval — per-char plists sharing an
  object no longer split into `0 1 … 1 2 …` runs even when a plist key is a
  string (which the structural merge couldn't compare).
- **`(- -0.0)`** is float negation → `0.0` (verified already correct; pinned by
  a regression test alongside `(+ -0.0)` / `(- -0.0 0)` staying `-0.0`).

Reproduce any of it:

```sh
bash scripts/fuzz_parity.sh -n 2000 -s 1      # seeded: same corpus every run
bash scripts/fuzz_parity.sh -c corpus.el      # re-check a saved corpus
```

A fourth pass (three 3,000-form seeded corpora) targeted the string functions,
and its through-line was that **Emacs defines most of them in Lisp, and the Lisp
definition — check order included — is the contract**:

- **`replace-regexp-in-string` is now the subr.el Lisp definition** (ported into
  the prelude, replacing a Rust reimplementation). That one move fixed five
  distinct gaps: `(length string)` rejects a non-sequence first (`sequencep 97`,
  not `stringp 97`), a nil STRING flows to `(substring nil 0 0)` (`arrayp nil` —
  even when the regexp is invalid, since l = 0 never compiles it), REGEXP is
  type-checked by `string-match` only when the loop runs (`stringp sym`), REP is
  not examined until a match happens (`(replace-regexp-in-string "123" [1 2]
  "line\nbreak")` returns the string), and `(< start l)` stops the empty-regexp
  loop from appending one more replacement at end-of-string
  (`(replace-regexp-in-string "" "t" "-4.5")` => `"t-t4t.t5"`, no trailing `t`).
- **`string-prefix-p` / `string-suffix-p` / `string-remove-prefix` /
  `string-remove-suffix`** now length-test exactly like subr.el / subr-x.el:
  both `length`s come before any string check (so `(string-prefix-p 97 "ab")` is
  `sequencep 97` and `(string-suffix-p 97 0)` names `0`, the STRING, whose
  length is taken first), a too-long prefix answers nil even for a non-string
  STRING (`(string-prefix-p "hello world" [1 2])` => nil, and
  `(string-remove-prefix "Hello, World" nil)` => nil), and only then does
  `compare-strings` signal `stringp`.
- **`string-join` is `mapconcat`**, whose up-front `length` names an improper
  list's tail: `(string-join (cons '- 9) 1.5)` => `(wrong-type-argument listp 9)`.
- **`string-empty-p` is `(string= STRING "")`** — `(string-empty-p nil)` is nil
  (symbol name), not a `stringp` error; `string-equal-ignore-case` is
  `compare-strings` and takes ONLY strings (`(string-equal-ignore-case nil "x")`
  signals `stringp nil`); `string-lessp` accepts a string or symbol and checks
  its first argument first (`(string< 97 "aa")` => `stringp 97`, and vectors are
  rejected, not silently compared).
- **`case-fold-search` reaches everything `string-match` reaches**:
  `split-string` compiled its separator case-sensitively
  (`(split-string "HELLO" "hello")` => `("" "")` in batch Emacs), and a
  backreference under folding must match cross-case
  (`(string-trim "aAbB" "\\(a\\)\\1" "a+")` => `"bB"` — fancy-regex 0.14
  compared backrefs case-sensitively under `(?i)`; 0.16 is the new floor).
- **`upcase`/`downcase`/`capitalize` on a character**: a negative integer is not
  a character (`char-or-string-p -1`), but one above the character range comes
  back unchanged (`(upcase 4194304)` => `4194304` — Emacs treats high bits as
  event modifiers). `upcase-initials` treated only ASCII as word constituents,
  leaving `"αβγ"` untouched; word chars are digits or cased letters, so it is
  `"Αβγ"`.
- **Check order elsewhere**: `substring` checks the array before the indices
  (`(substring -1 1.5)` => `arrayp -1`), `substring-no-properties` insists on a
  string (`stringp 97`, not `arrayp`), `split-string` type-checks its separator
  regexp before the string (`(split-string [1 2] 97)` => `stringp 97`).
- **GNU regex's two `\{` diagnostics**: content that can never appear in an
  interval is `Invalid content of \{\}` even when the pattern then ends
  (`"a\{x"`, `"a\{2\)"`), while running out of pattern with the interval still
  well-formed is `Unmatched \{` (`"a\{2,"`). An empty interval is valid:
  `(string-match "a\\{\\}" "b")` => 0.

The same pass's list/sequence cluster — and one **process-killing crash**:

- **`assoc-string` with a nil key spun forever.** A matched element may itself
  be nil (`(assoc-string nil '(… nil))`: key nil and element nil both compare
  as `"nil"` via `symbol-name`), and the walk looped on "result still nil",
  never advancing — 100% CPU, unbounded allocation, dead process (fuzz form
  #2745, seed 424242). The walk now carries a DONE flag like C `Fassoc_string`.
- **`nconc` is Fnconc, not an append-alike.** Every argument but the last must
  be a *cons* — `(nconc 0 '(1))` is `(wrong-type-argument consp 0)`, not
  `listp` — and each argument's spine is walked to its LAST CONS, so a dotted
  tail is overwritten, never an error: `(nconc '(1) '(2 . 3) '(4))` => `(1 2 4)`.
  `mapcan` is mapcar + `nconc` (it used `append`), so dotted per-element
  results splice: `(mapcan (lambda (x) (cons x x)) (list -0.0 t))` =>
  `(-0.0 t . t)`.
- **`mapconcat` mapped and concatenated incrementally.** Fmapconcat length-checks
  SEQ first (dotted SEQ names its tail), runs FUNCTION over *every* element,
  and only then concatenates — `(mapconcat #'zerop (vector 0 'nil))` fails
  inside `zerop` on the second element, never on concatenating the first `t`.
  `concat` itself now enforces Fconcat's contract: an argument is a string,
  nil, or a list/vector of *characters*; a list argument's structure is checked
  before its elements (`(concat '(a . 2))` => `listp 2`, `(concat '(a a))` =>
  `characterp a`); elisprs silently skipped non-characters, so
  `(mapconcat (lambda (x) (list x x)) (vector 'car t 1000) "z")` answered
  `"zzϨϨ"`. `vconcat` shares the list walk (`(vconcat '(t . 9))` => `listp 9`).
- **`cl-sort` rejected strings and mis-named its error.** cl-seq.el coerces any
  non-list with `(append SEQ nil)` — whose failure is `sequencep`, not bare
  `sort`'s `list-or-vector-p` — sorts a string via vconcat/concat round-trip,
  and writes a vector's sorted elements back in place (`:key` honored).
- **Which end of an improper list an error names.** `assq`/`assoc`/`rassoc`/
  `alist-get`, and `nth`/`nthcdr` mid-walk, are `CHECK_LIST_END`: they name the
  WHOLE list (`(assq 'a (cons -1 10))` => `listp (-1 . 10)`); `nth`'s *final*
  `car` names the tail value (`(nth 1 '(1 . 2))` => `listp 2`); `nreverse`
  relinks first and signals after, so it names the now-mutated head cons
  (`(nreverse (cons "p" 32))` => `listp ("p")`); `delete-dups` sizes with
  `length` first (`sequencep sym` / `listp 2.5`).
- **seq.el/subr.el guards were missing.** `seq-partition` with N < 1 is nil
  without touching SEQ — elisprs looped forever on zero progress
  (`(seq-partition "ab" 0)` hung). `butlast` with N ≤ 0 returns LIST
  unvalidated (`(butlast 0 0)` => `0`). `seq-drop` on a list is `(nthcdr n
  list)` (`integerp` on a bad N) while other sequences hit the generic
  `(<= n 0)` (`number-or-marker-p`); `seq-subseq` defers strings/vectors to
  `substring` (`integerp`, `args-out-of-range`) and keeps seq.el's plain
  `error` for list indices. `rassq-delete-all`/`assq-delete-all` are the
  destructive subr.el loops, whose `(car alist)` names a non-list with `listp`.
- **`aref` dispatches on the array's type before any bounds check** —
  `(aref 97 -7)` is `(wrong-type-argument arrayp 97)`, not `args-out-of-range`.

A fifth pass (fresh 3,000-form corpus, seed 555001) cleared the plist /
delete-family / sort-order cluster — its through-line is that the C walks'
exact break points and CHECK_LIST_END data are the contract:

- **`plist-get` never signals.** Fplist_get walks with `FOR_EACH_TAIL_SAFE`
  and breaks on a non-cons cdr BEFORE testing the key: `(plist-get '(1 . 2) 1)`,
  `(plist-get '(1) 1)`, `(plist-get 5 1)` are all nil (elisprs signalled
  `listp` on the dotted cdr). `plist-member` tests the key FIRST — so
  `(plist-member '(1 . 2) 1)` => `(1 . 2)` — but ends with CHECK_TYPE naming
  the WHOLE plist under `plistp` (`(plist-member '(1 2 . 3) 99)` =>
  `plistp (1 2 . 3)`, and a non-list is `plistp`, never `listp`). `plist-put`
  breaks BEFORE its key test, so `(plist-put '(1 . 2) 1 9)` and
  `(plist-put '(1) 1 9)` are `plistp` errors even though the key is present.
- **Empty strings are `eq`.** Emacs keeps ONE shared `empty_unibyte_string`
  object (alloc.c), so every 0-length construction — `""`, `(make-string 0 C)`,
  `(substring s 0 0)`, `(copy-sequence "")` — is the same object, and
  plist-put's default `eq` test REPLACES under an equal-but-differently-written
  `""` key: `(plist-put '("" -65536) "" V)` => `("" V)`, where elisprs
  appended a second pair.
- **`delq`/`delete` error data is the list AFTER head removals.** Fdelq/Fdelete
  rebind LIST past deleted head cells before `CHECK_LIST_END`, so
  `(delq 'a (cons 'a 5))` => `listp 5` while `(delq 'x (cons 'a 5))` names the
  whole `(a . 5)`. `delete` on a dotted list SIGNALED nothing at all in elisprs
  (`(delete "str" '("a" . [1 2]))` returned the list); it now walks like Fdelete
  and filters vectors/strings into fresh copies (`(delete ?a "abca")` =>
  `"bc"`, previously a char list). `remq` is subr.el's: cdr past head matches,
  then `memq` (names the WHOLE list) and only on a hit
  `(delq ELT (copy-sequence LIST))` (names the TAIL) — so
  `(remq "x" '("αβγ" . N))` => `listp ("αβγ" . N)` but `(remq 'a '(x a . 3))`
  => `listp 3`. `remove` is delete-over-copy-sequence, and a non-sequence is
  `sequencep` through `delete`/`remove` but `listp` through `delq`/`remq`.
- **`sort`'s comparison ORDER is tim_sort's.** Emacs 30 sorts with src/sort.c
  (the CPython listsort port); under MAX_MINRUN (64) that is exactly count_run
  + binary insertion, whose FIRST predicate call is `pred(a[1], a[0])` — a
  throwing predicate therefore names a[1]: `(cl-sort '(t 1.0 nil)
  #'string-to-number)` => `stringp 1.0` (elisprs's merge sort first compared
  the nil). The host sort now reproduces count_run + binarysort verbatim below
  64 elements, logging-predicate-identical to Emacs.
- **`zerop` is not a subr.** It is a byte-compiled defsubst from subr.el
  (`(subrp (symbol-function 'zerop))` => nil), so a wrong argument count is
  exec_byte_code's `((MANDATORY . NONREST) NARGS)`: `(seq-reduce #'zerop "ab"
  0)` => `(wrong-number-of-arguments (1 . 1) 2)`, never `#<subr zerop>`. The
  Rust subr is gone; the prelude defines it (which also survives the
  bytecode-cache heap image, where a shadowing defun over a registered subr
  was silently lost on cache HIT).
- **`string-trim` trims the RIGHT side first** — subr-x's
  `(string-trim-left (string-trim-right S TRIM-RIGHT) TRIM-LEFT)` — so with two
  bad regexps the right one's compile error wins:
  `(string-trim "" "\\(" "[")` => `(invalid-regexp "Unmatched [ or [^")`.
- **`apply`'s improper spread names the TAIL.** Fapply sizes SPREAD with
  `list_length`, whose CHECK_LIST_END names the loop variable:
  `(apply #'1- (cons 2.5 -2))` => `listp -2` (elisprs named the whole cons).
- **`cl-remove-if` with a nil predicate removes nils.** cl-seq.el routes
  through `(cl-remove nil SEQ :if PRED …)` and a nil `:if` falls back to
  cl--check-test's item-`eql` matching — `(cl-remove-if nil '(nil 2 nil))` =>
  `(2)`, `(cl-remove-if nil '(2 +))` => `(2 +)` (elisprs funcalled nil). The
  NOT is dropped in the fallback, so `cl-remove-if-not` removes nils too.

Still open from this corpus: interpreted closures capture (and print) the whole
enclosing lexical environment where Emacs 30's cconv captures only free
variables — `(let ((x 1.5e300)) (sort '(42 t) (lambda (x) (cons x x))))` errors
naming `#[(x) ((cons x x)) ((x . 1.5e+300))]` where Emacs prints
`#[(x) ((cons x x)) (t)]`.

---

## Additional parity gaps found via sweep — ✅ FIXED

Beyond the numbered entries below, fresh `elisp` vs `emacs -Q` sweeps surfaced
and fixed: `vconcat`, `string-to-vector`, `logcount`, `string-equal-ignore-case`,
`upcase-initials`, `most-positive-fixnum` / `most-negative-fixnum`; `abs` keeping
int/float type and normalizing `-0.0`; `string-prefix-p` / `string-suffix-p`
IGNORE-CASE; `assoc` TESTFN; `string-pad` PADDING/START. Introspection: added
`symbol-function`, `intern-soft`, `subrp`, `macrop`, `special-form-p`,
`char-uppercase-p`, `string-distance`, `fixnump`, `bignump` (with `macrop` /
`special-form-p` matching Emacs's classification). Sequences: `seq-concatenate`,
`copy-alist`, `substring-no-properties`; `alist-get` DEFAULT/TESTFN; `string-trim`
regexp arguments. Format: `+`/space sign flags and C-style `%e`; added
`hash-table-test`, `nbutlast`, `memql`, `assoc-string`; `string-search` START arg.
`format` `%x`/`%o` sign+magnitude for negatives and `#` flag. `case-fold-search`
(default t) honored by `string-match`/`replace-regexp-in-string`; `incf`/`decf`/
`cl-incf`/`cl-decf` on generalized places; multi-binding `when-let*`/`if-let*` and
`named-let`.

`replace-regexp-in-string` with a *function* REP now works (handled in the
re-entrant `call_function` path). Destructive `plist-put` / `delete-dups`; added
`nconc`, `rassq-delete-all`, `fillarray`; `number-sequence` negative step;
`case-fold-search`; generalized-place `incf`/`decf`; `when-let*`/`if-let*`/`named-let`.
cl-lib/seq parity (verified with libs loaded): `cl-reduce :initial-value`,
`cl-mapcar` N-seq, `cl-remove-duplicates` keep-last, `seq-group-by` group order.
Added `length=`/`length<`/`length>`, `cl-typecase`, `cl-destructuring-bind`,
`string-clean-whitespace`; `cl-getf` DEFAULT.

A broad `cl-loop` subset is now implemented: numeric/`in`/`on`/`repeat`/`while`/
`until` drivers; `collect`/`append`/`nconc`/`sum`/`count`/`maximize`/`minimize`
accumulators (with `into VAR`); `with VAR = VAL`; `when`/`unless`/`if`…`else`
conditionals; `always`/`never`/`thereis`; `do`; `finally`. Not yet: parallel
`for`, `across`, destructuring.

Fixed a lexical-scope leak: a `throw`/`error` out of an inner `let` left the
scope open (`run_closure` now unwinds to its entry depth), which was the real
cause of the "void variable" failures when an ERT `should` wrapped a
`catch`/nested-`let`-emitting macro — previously worked around per-feature.
`mapcar`/`mapc`/`seq-*` accept any sequence (vector/string); added `boundp`,
`gensym`, `default-value`; hash tables print in Emacs-30 `#s(hash-table …)` syntax.
Added `cl-flet`/`cl-labels` (lexical local fns via call-rewriting; mutual/self
recursion), `let-alist`, `and-let*`, `cl-dolist`/`cl-dotimes`, `fset`/`fboundp`.
`pcase` backquote patterns (incl. dotted) now work; fixed dotted backquote reader.
Added `cl-block`/`cl-return-from`/`cl-return`, `cl-pushnew`, `cl-find-if-not`;
`cl-subseq`/`seq-subseq` are sequence-generic (optional/negative END). `cl-defstruct`
(constructor/accessors/predicate/copier, setf-able slots; instances print as
`#s(NAME …)`, `type-of`/`recordp`/`cl-struct-p` recognize them — at the time of this
round `vectorp` was still t since they were vectors underneath; superseded by R5-U,
which gave records a real `Obj::Record` type so `vectorp` is now nil). `cl-member`/`cl-assoc`/`cl-find`/
`cl-position`/`cl-count`/`cl-remove`/`cl-delete`/`cl-substitute` take `:test`/`:key`/
`:count` keyword args. Fixed `condition-case` to bind the real `(SYMBOL . DATA)`
error object (data list preserved, not stringified); added `ignore-error`,
`with-suppressed-warnings`. Fixed `#'(lambda …)` to compile to a closure.
`user-error` signals `user-error`; added `get`/`put`/`symbol-plist`,
`define-error` + seeded error conditions (so `error-message-string` matches Emacs),
`seq-let`, `macroexp-progn`, `cl-function`, and `pcase-let` destructuring.
Added `cl-letf`, `letrec`, `dlet`; nested `cl-destructuring-bind`; `seq-let` `&rest`.
`cl-defstruct` instances print as `#s(NAME …)` (type-of/recordp/cl-struct-p too).
Added `eval`; `cl-loop` numeric `for` accepts an implicit `from 0`. Exposed
`macroexpand`/`-1`/`-all` (user/prelude macros; intrinsic `when`/`unless` pass
through), `indirect-function`, `cl-sort`, `commandp`, `plistp`. Float printing now
matches Emacs (shortest form, exponential for extreme magnitudes); added the pcase
`(cl-type …)` pattern and `pcase-exhaustive`.

**Notable still-missing:** `string-fill` (word-wrapping); the `cl-loop` clauses
above.

**Still divergent (harder):** ~~pattern backreferences in regexps (the backing
engine doesn't backtrack)~~ — ✅ FIXED: swapped the matching engine to
`fancy-regex`, whose backtracking handles `\1`..`\9` while keeping the linear
`regex` fast path for backref-free patterns.

---

## Core semantics — wrong values (highest severity)

### 1. ✅ FIXED — No bignum support — silent integer overflow
- `(expt 2 100)` → `1267650600228229401496703205376`; `(* 1000000000000 1000000000000)`
  → `1000000000000000000000000`; `#xFFFFFFFFFFFFFFFF` reads as a bignum.
- Integers promote out of `i64` rather than wrapping. The feasibility note below is
  kept because it is the reason the native-op fast path and the promotion coexist
  the way they do.
- **Feasibility (investigated):** fusevm executes `+`/`-`/`*` via native ops that
  **wrap silently** — `Op::Mul => self.arith_int_fast(i64::wrapping_mul, …)`
  (`fusevm vm.rs:1104`; the Cranelift JIT path does the same). elisprs lowers hot
  arithmetic to exactly these ops on purpose (the JIT/AOT story in the README), so
  a bignum value type cannot promote on overflow without **either** (a) removing
  the native-op lowering for `+`/`-`/`*` and routing all integer arithmetic
  through host builtins with `checked_*` + promotion to a host-side
  `Obj::Bignum(num_bigint::BigInt)` — sacrificing the native fast path — **or**
  (b) changing fusevm itself (add an overflow-trap/host-fallback to the int ops).
  Both are real architecture decisions (and touch equality/printing/comparison
  too), so this is intentionally left for an explicit owner decision rather than a
  silent half-fix that only works on the interpreted path.

### 2. ✅ FIXED — `(lambda …)` in operator (head) position fails
- `((lambda (x) x) 5)` → Emacs `5`, elisprs `error: invalid-function`
- `((lambda (x &optional y) (list x y)) 1)` → Emacs `(1 nil)`, elisprs `error: invalid-function`
- `(funcall (lambda …) …)` works, so only *direct* application of a lambda form is
  broken. Very common idiom.

### 3. ✅ FIXED — `eq` on floats returns `t` (must be object identity → `nil`)
- `(eq 1.0 1.0)` → Emacs `nil`, elisprs `t`
- `el_eq` compares floats by bit pattern. `src/builtins.rs:163`
  (`Value::Float(x), Value::Float(y) => x.to_bits() == y.to_bits()`).
  `eql`/`equal` are correct; `eq` must not equate distinct float objects.

### 4. ✅ FIXED — `round` uses round-half-away-from-zero, not banker's rounding
- `(round 2.5)` → Emacs `2`, elisprs `3`
- `(round 0.5)` → Emacs `0`, elisprs `1`
- `(round -2.5)` → Emacs `-2`, elisprs `-3`
- Emacs rounds half to even.

### 5. ✅ FIXED — Float contagion lost in inlined `1+` / `1-`
- `(1+ 1.0)` → Emacs `2.0`, elisprs `2`
- `(1- 1.0)` → Emacs `1.0`, elisprs `0`
- The compiler inlines to integer opcodes `Op::Inc`/`Op::Dec`
  (`src/compiler.rs:170-176`), bypassing the correct `one_plus` builtin
  (`src/builtins.rs:115`). `(funcall #'1+ 1.0)` is correct.

### 6. ✅ FIXED — `mod` truncates a float operand
- `(mod 13.5 4)` → Emacs `1.5`, elisprs `1`

### 7. ✅ FIXED — `expt` mishandles float / negative exponents
- `(expt 2.0 0.5)` → Emacs `1.4142135623730951`, elisprs `2.0` (fractional exponent ignored)
- `(expt 2 -1)` → Emacs `0.5`, elisprs `1` (negative exponent should yield float)
- `(expt 0.0 0)` → Emacs `1.0`, elisprs `1` (result should be float)

### 8. ✅ FIXED — `string-to-number` can't parse floats / scientific notation / base arg
- `(string-to-number "1.5e3")` → Emacs `1500.0`, elisprs `1`
- `(string-to-number "ff" 16)` → Emacs `255`, elisprs `error: wrong-number-of-arguments`

### 9. ✅ FIXED — `split-string` ignores OMIT-NULLS
- `(split-string "a,b,,c" "," t)` → Emacs `("a" "b" "c")`, elisprs `("a" "b" "" "c")`

### 10. ✅ FIXED — `dotimes` / `dolist` ignore the RESULT form (3rd spec element)
- `(dotimes (i 3 i) i)` → Emacs `3`, elisprs `nil`
- `(let ((s nil)) (dolist (x '(1 2 3) s) (push x s)))` → Emacs `(3 2 1)`, elisprs `nil`
- Macros in `src/prelude.rs:122-134` never emit the result form `(caddr spec)`.

### 11. ✅ FIXED — `capitalize` only capitalizes the first word
- `(capitalize "hello world")` → Emacs `"Hello World"`, elisprs `"Hello world"`

---

## Reader — read syntax not supported

### 12. ✅ FIXED — Vector literals `[…]` not read
- `[1 2 3]` → Emacs `[1 2 3]`, elisprs `error: Symbol's value as variable is void: [1`
- `(vector 1 2 3)` works and prints `[1 2 3]`; only the literal reader is missing.
  Cascades to `(aref [10 20 30] 1)`, `(vconcat [1 2] [3 4])`, `(equal [1 2] [1 2])`, …

### 13. ✅ FIXED — Radix literals `#x` `#b` `#o` not read
- `#x1f` → Emacs `31`, elisprs `error: …void: #x1f` (same for `#b101`→5, `#o17`→15)

### 14. ✅ FIXED — Char modifier syntax `?\C-` / `?\M-` not read
- `?\C-a` → Emacs `1`, elisprs `error: …void: -a`
- `?\M-a` → Emacs `134217825`, elisprs `error: …void: -a`
- Plain `?A`→65 and `?\n`→10 work; only modifier escapes fail. `src/reader.rs:156`
  (`read_char_literal`).

### 15. ✅ FIXED — Float special-value read syntax not supported
- `1.0e+INF` → Emacs `1.0e+INF`, elisprs `error: …void: 1.0e+INF`

---

## `format` directives

### 16. ✅ FIXED — Width / precision / flags silently ignored (returned literally)
- `(format "%5d" 42)` → Emacs `"   42"`, elisprs `"%5d"`
- `(format "%-5d|" 42)` → Emacs `"42   |"`, elisprs `"%-5d|"`
- `(format "%05d" 42)` → Emacs `"00042"`, elisprs `"%05d"`
- `(format "%.2f" 3.14159)` → Emacs `"3.14"`, elisprs `"%.2f"`
- `(format "%3.1f" 3.14159)` → Emacs `"3.1"`, elisprs `"%3.1f"`
- `(format "%+d" 5)` → Emacs `"+5"`, elisprs `"%+d"`
- `(format "% d" 5)` → Emacs `" 5"`, elisprs `"% d"`
- Also `(format "%f" 3.14159)` → Emacs `"3.141590"` (6 digits), elisprs `"3.14159"`.

### 17. ✅ FIXED — Conversions `%X` `%o` `%e` `%g` unsupported (returned literally)
- `(format "%X" 255)` → Emacs `"FF"`, elisprs `"%X"`
- `(format "%o" 8)` → Emacs `"10"`, elisprs `"%o"`
- `(format "%e" 31415.9)` → Emacs `"3.141590e+04"`, elisprs `"%e"`
- `(format "%g" 100000.0)` → Emacs `"100000"`, elisprs `"%g"`

### 18. ✅ FIXED — Argument field numbers `%N$` unsupported
- `(format "%2$s %1$s" "a" "b")` → Emacs `"b a"`, elisprs `"%2$s %1$s"`

---

## Float printing

### 19. ✅ FIXED — Infinity prints `inf`, should be `1.0e+INF`
- `(/ 1.0 0)` → Emacs `1.0e+INF`, elisprs `inf` (same for `(/ 1 0.0)`)

### 20. ✅ FIXED — NaN prints `NaN`, should be `0.0e+NaN`
- `(/ 0.0 0.0)` → Emacs `0.0e+NaN`, elisprs `NaN`

---

## Missing optional args / sequence coercion

### 21. ✅ FIXED — Optional second arg unsupported on several builtins
- `(floor 7 2)` → Emacs `3`, elisprs `7` (divisor arg ignored)
- `(last '(1 2 3) 2)` → Emacs `(2 3)`, elisprs `error: wrong-number-of-arguments`
- `(butlast '(1 2 3) 2)` → Emacs `(1)`, elisprs `error: wrong-number-of-arguments`

### 22. ✅ FIXED — Sequence coercion missing (strings/chars as sequences)
- `(reverse "abc")` → Emacs `"cba"`, elisprs `error: wrong-type-argument: listp`
- `(append "ab" nil)` → Emacs `(97 98)`, elisprs `error: wrong-type-argument: listp`
- `(append '(1 2) '(3 4) 5)` → Emacs `(1 2 3 4 . 5)` (dotted tail), elisprs `error: wrong-type-argument: listp`
- `(downcase ?A)` → Emacs `97`, elisprs `error: wrong-type-argument: stringp 65` (also `(upcase ?a)`)

### 23. ✅ FIXED — Core functions present in `emacs -Q` but void in elisprs
Added: `type-of`, `functionp`, `char-or-string-p`, `sqrt`, `fround`, `ffloor`,
`fceiling`, `ftruncate`, `isnan`, `char-equal`, `logb`, `read`, `compare-strings`,
`error-message-string`, `seq-mapn` (`prin1-to-string` already present).
`format-message` is an alias of `format` here (no curved-quote translation), so it
is provided as such.

---

## Coverage — verified at parity (no bug)

Integer `/` truncation toward zero, `%`/`mod` integer sign rules, float contagion in
`+ - * = < min max`, `(/ 1 0)` arith-error, `ash`/`logand`/`logior`/`logxor`/`lognot`,
`?A`/`?\n`, dotted-pair printing, `nthcdr`/`nreverse`/`assoc`/`assq`/`alist-get`/
`member`/`memq`/`setcar`, `mapcar`/`mapconcat`/`sort`, `elt`/`aref`(via `vector`)/
`concat`/`substring` (incl. negative indices), `string-match`/`replace-regexp-in-string`,
basic `format` (`%d %s %S %c %%`), plist-get/put, `eql`/`equal`, `cond`/`and`/`or`/
`when`/`unless`/`while`/`catch-throw`/`condition-case` (incl. `arith-error`,
`wrong-type-argument`), `unwind-protect`, `let`/`let*`, lexical closures,
`funcall`/`apply`, `&optional`/`&rest` via funcall, hash tables, `car`/`cdr` of nil,
`(nth 99 …)`→nil, `number-sequence`, string utils, `intern`/`eq` on symbols, keywords,
backquote/unquote/splice.

---

# Round 2 — additional confirmed divergences (vs `emacs -Q` 30.2)

Found in a deeper second pass; all reproduced against the current binary. **Ground
truth is bare `emacs -Q --batch`** — `cl-lib` macros that are `void-function` there
(`cl-loop`, `cl-flet`, `cl-labels`, `cl-typecase`, `cl-destructuring-bind`,
`cl-reduce`/`cl-find`/`cl-position`/`cl-mapcar`/`cl-getf` with keywords,
`cl-remove-duplicates`) are **not** listed: Emacs errors too, so they aren't `-Q`
parity bugs (they'd need `(require 'cl-lib)`).

## Critical — wrong values / silent miscomputation

### R2-A. ✅ FIXED — Arithmetic silently coerces non-numbers instead of signaling
- `(+ 1 "a")` → `(wrong-type-argument number-or-marker-p "a")`, as Emacs.

### R2-B. ✅ FIXED — `wrong-type-argument` error data is one string, not separate elements
(host.rs `make_error_object`: for `wrong-type-argument`/`args-out-of-range` it re-reads the rendered
message into separate value elements via `read_all_forms`+reader — `(car 5)` → `(wrong-type-argument
listp 5)`, `(caddr e)` → `5`. Works for awkward values (strings with spaces re-read as one form).
Fixed `substring` to render its array readably (`h.print` not `as_str_cow`).
Known residual: the host-less coercion helpers `as_num`/`as_int`/`as_string` still render their bad
value via `as_str_cow`, so e.g. `(aref "abc" 'x)` yields `(wrong-type-argument numberp (obj:N))`
instead of `(… fixnump x)`; fixing needs threading `h` into those helpers — separate sweep.)

### R2-C. ✅ FIXED — `user-error` signals the `error` symbol, not `user-error`
(now signals the `user-error` symbol; verified `(condition-case e (user-error "nope") (error e))` → `(user-error "nope")`)
- `(condition-case e (user-error "nope") (error e))` → Emacs `(user-error "nope")`,
  elisprs `(error "nope")` — the two conditions can't be distinguished.

### R2-D. ✅ FIXED — Float printer doesn'	 use exponent form for large / small magnitudes
- `(prin1-to-string 1e20)` → `"1e+20"`; `1.5e-10` → `"1.5e-10"`.

## Macros / special forms

### R2-E. ✅ FIXED — `cl-incf` / `cl-decf` only accept a bare symbol, not a generalized place
(verified `(cl-incf (car l))`, `(cl-incf (aref v 1) 10)`, `(cl-incf (gethash …))` all work via setf)
- `(let ((l (list 1 2))) (cl-incf (car l)) l)` → Emacs `(2 2)`, elisprs `error: setq: expected a symbol`
- `setf` itself works on places, so the cl-incf/decf macros just don't expand through it.

### R2-F. ✅ FIXED — `setq-default` is broken
(prelude: added `set-default` + `setq-default` macro; no buffer-local model so both are global sets)
- `(setq-default x 5)` → Emacs `5`, elisprs `error: Symbol's value as variable is void: x`

### R2-G. ✅ FIXED — `pcase` backquote patterns
- `` (pcase (list 1 2) (`(,a ,b) (+ a b))) `` → `3`.
- Supported by teaching `pcase--compile` to read the reader's eager backquote
  expansion (`cons`/`quote`/literals) as structural patterns — incl. nested and
  dotted `` `(,a . ,rest) ``. Also fixed a reader bug where dotted backquote
  `` `(,a . ,b) `` mis-expanded the unquoted tail.

## Sequence / string semantics

### R2-H. ✅ FIXED — `mapcar` (and `seq-map`) reject vector/string sequences
- `(mapcar #'+ [1 2 3])` → `(2 3 4)`; `(mapcar #'+ "abc")` → `(98 99 100)`. Both match.

### R2-I. ✅ FIXED — `seq-empty-p` wrong on the empty string
(prelude: `(= 0 (length l))` so vectors/strings count too)
- `(seq-empty-p "")` → Emacs `t`, elisprs `nil`

### R2-J. ✅ FIXED — `string-blank-p` returns `t` instead of the match position
(prelude: `(string-match-p "\\`[ \t\n\r]*\\'" s)`)
- `(string-blank-p "  ")` → Emacs `0`, elisprs `t`

### R2-K. ✅ FIXED — `string-pad` 4-arg form (PADDING + START)
- `(string-pad "ab" 5 ?* t)` → `"***ab"`.

### R2-L. ✅ FIXED — `make-hash-table` print format diverges from Emacs 30
- `(make-hash-table)` → Emacs `#s(hash-table)`, elisprs `#s(hash-table size 0)`

### R2-M. ✅ FIXED — `assoc` TESTFN (3rd arg)
- `(assoc 2 '((1 . 10) (2 . 20)) #'=)` → `(2 . 20)`.

### R2-N. ✅ FIXED — Emacs-30 `sort` keyword API (`:key`/`:lessp`)
- `(sort '(3 1 2) :key #'- :lessp #'<)` → `(3 2 1)`.

## Missing builtins / constants (present in bare `emacs -Q`, void in elisprs)

All confirmed `void-function`/void-variable in elisprs while `emacs -Q` returns a value:

- **Symbols/eval:** `boundp` (`t`), `fboundp` (`t`), `gensym` (`g0`), `macrop` (`t`),
  `special-variable-p` (`nil`), `func-arity` (`(1 . 1)`), `indirect-function`,
  `featurep`/`provide`/`require`, `named-let` (`6`)
- **Constants:** `most-positive-fixnum` (`2305843009213693951`), `most-negative-fixnum`
- **Sequences:** `vconcat` (`[1 2 3 4]`), `copy-alist`, `length=`/`length<`/`length>` (`t`),
  `string-to-vector` (`[97 98 99]`), `seq-mapn` (`(4 6)`)
- **Strings/props:** `propertize` (`#("hi" 0 2 (face bold))`) + text properties,
  `string-width` (`3`), `string-distance` (`3`), `string-equal-ignore-case` (`t`)
- **Math:** `frexp` (`(0.5 . 4)`), `ldexp` (`8.0`), `copysign` (`-3.0`)

Areas probed in round 2 that PASSED (now match Emacs): `floor`/`ceiling`/`truncate`/
`round` with a divisor arg (the #21 follow-ups — fixed), `seq-reverse`,
`string-search`/`string-replace`, `cl-case`, `when-let`/`if-let`, `pcase`
`pred`/`or`/`and`/`guard`, the autoloaded `seq-` family on lists, `type-of`/`functionp`,
`prin1-to-string` (the #23 entry — now present).

---

# Round 3 — additional confirmed divergences (vs `emacs -Q` 30.2)

Third deep pass against the current binary. Ground truth = bare `emacs -Q --batch`;
`cl-*` symbols void there are excluded. None of these overlap rounds 1–2.

## Behavioral — wrong values / wrong errors

### R3-A. ✅ FIXED — String/char `\` escapes: named-control, hex, and octal all wrong
`?\a` → 7, `?\x41` → 65, `"\x41"` → `"A"`, `(string-to-list "\x41\x42")` → `(65 66)`,
`?\N{LATIN SMALL LETTER A}` → 97 — all match emacs 30.2. The original report follows.
- `?\a`→Emacs `7`, elisprs `97`; likewise `?\b`/`?\f`/`?\v`/`?\s`/`?\d` give the ASCII
  of the letter instead of the control code
- `?\x41`→Emacs `65`, elisprs `41`; `?\101` (octal)→`65` vs `1`
- `"\x41"`→Emacs `"A"`, elisprs `"x41"`; `"\101"`→`"A"` vs `"101"`; `"\C-a"`→ctrl-char vs `"C-a"`
- `(string-to-list "\x41\x42")`→Emacs `(65 66)`, elisprs `(120 52 49 120 52 50)`
- `?\N{LATIN SMALL LETTER A}`→Emacs `97`, elisprs `error: void: {LATIN`
- `src/reader.rs` `unescape` (~401-410), shared by string (~169) and char (~229) paths.
  Round-1 #14 covered only `\C-`/`\M-` modifiers — this is the rest.

### R3-B. ✅ FIXED — Symbol read-escape (`\`) unsupported
(reader.rs `read_atom`: `\` escapes the next char into the symbol name and forces a symbol — never a number/nil/t)
- `'foo\ bar` → Emacs symbol `foo bar`, elisprs `error: …void: bar`

### R3-C. ✅ FIXED — Symbol printing doesn't escape; empty symbol mis-prints
(host.rs `print_symbol_readable`: prin1 escapes special chars/control/space, leading `?`/`.`/number;
empty name => `##`; princ stays raw. Round-trips with R3-B.)
- `(prin1-to-string (intern "a b"))` → Emacs `"a\\ b"`, elisprs `"a b"` (round-trips wrong)
- `(prin1-to-string (intern ""))` → Emacs `"##"`, elisprs `""`

### R3-D. ✅ FIXED — `print-length` / `print-level` ignored
(prelude `defvar`s them special; host.rs printer threads depth + reads the limits via `print_limit`,
truncating lists/vectors with `...` for length and over-deep nesting for level)
- `(let ((print-length 3)) (prin1-to-string '(1 2 3 4 5)))` → Emacs `"(1 2 3 ...)"`, elisprs `"(1 2 3 4 5)"`
- `(let ((print-level 2)) (prin1-to-string '(1 (2 (3)))))` → Emacs `"(1 (2 ...))"`, elisprs full

### R3-E. ✅ FIXED — `format` `%x`/`%o` on negatives print two's-complement, not signed
- `(format "%x" -1)` → Emacs `"-1"`, elisprs `"ffffffffffffffff"`
- `(format "%o" -8)` → Emacs `"-10"`, elisprs `"1777777777777777777770"`

### R3-F. ✅ FIXED — `format` `#` flag unsupported (returned literally)
(verified `(format "%#x" 255)` → `"0xff"`, `(format "%#o" 8)` → `"010"`)
- `(format "%#x" 255)` → Emacs `"0xff"`, elisprs `"%#x"`

### R3-G. ✅ FIXED — `substring` doesn't bounds-check END
(builtins.rs: adjust negatives, then signal `args-out-of-range` outside `[0,len]`)
- `(substring "abc" 1 10)` → Emacs signals `args-out-of-range ("abc" 1 10)`, elisprs `"bc"`
  (round 1 checked negative indices, not over-range)

### R3-H. ✅ FIXED — `nth` on a vector returns nil instead of signaling
(builtins.rs: `nth` now walks the cons spine — improper lists work, non-cons signals listp)
- `(nth 1 [1 2 3])` → Emacs signals `wrong-type-argument listp [1 2 3]`, elisprs `nil`

### R3-I. ✅ FIXED — `last` on an improper (dotted) list errors instead of returning
(prelude: walk while `(consp (cdr l))` so the dotted tail stops the loop)
- `(last '(1 2 . 3))` → Emacs `(2 . 3)`, elisprs `error: wrong-type-argument: listp 3`

### R3-J. ✅ FIXED — `char-equal` ignores `case-fold-search`
- `(char-equal ?a ?A)` → Emacs `t` (case-fold defaults t in batch), elisprs `nil`

### R3-K. ✅ MOSTLY FIXED — `signal`/`condition-case` stringify the entire error DATA list
- `(condition-case e (signal 'my-err '(a b)) (t (cdr e)))` → `(a b)`, as Emacs.
- **Residual:** the data a *builtin* signals is rendered to text at signal time, so a
  printer variable bound around the failing call is baked into it:
  `(let ((print-length 6)) (condition-case e (elt [1 2 3 4 5 6 7 8] 99) (error e)))`
  keeps the abbreviated `[1 2 3 4 5 6 \...]` where Emacs holds the vector itself and
  prints it later, unabbreviated. Errors would have to carry values, not strings.

### R3-L. ✅ FIXED — Hex reader rejects values above i64 range (hard error)
(reader.rs `read_radix`: parses into a `BigInt` and hands it to `make_integer`, which
picks fixnum or bignum — `#xFFFFFFFFFFFFFFFF` → `18446744073709551615`, and the
negative and `#NNr` forms with it)

## Missing builtins — confirmed `emacs -Q` returns a value, void in elisprs

- **Eval/macros (high impact):** `eval` (`(eval '(+ 1 2))`→3), `macroexpand`,
  `macroexpand-1`, `macroexpand-all`, `special-form-p`, `byte-code-function-p`,
  `interactive-form`, `documentation`, `make-closure`
- **Symbols/functions:** `fset`, `defalias`, `symbol-function` (`#<subr car>`), `put`/`get`,
  `symbol-plist`, `setplist`, `fmakunbound`, `function-get`, `intern-soft` (→nil)
- **Predicates/numbers:** `fixnump` (t), `bignump`, `log` (`(log 0)`→`-1.0e+INF`),
  `logcount` (`(logcount 7)`→3)
- **Lists/cons:** `nconc`, `member-ignore-case`, `rassq-delete-all`, `car-safe`, `cdr-safe`;
  the c[ad]+r gaps `caadr`/`cadar`/`cdaar`/`cdadr`/`cddar` (void while `caaar`/`caddr`/`cdddr` exist)
- **Strings:** `substring-no-properties`, `upcase-initials`, `string-fill`,
  `string-clean-whitespace`, `string-bytes` (`"λ"`→2), `multibyte-string-p`, `char-width`,
  `string>`, `string-version-lessp`, `value<` (Emacs-30 generic `<`)
- **Records/bool-vectors:** `record` (`#s(foo 1 2)`), `recordp`, `make-bool-vector`, `bool-vector`
- **Hash/equality:** `sxhash-equal`, `sxhash-eq`, `equal-including-properties`
- **Reader/regexp/macros:** `read-from-string`, `regexp-opt`, `let-alist`, `dlet`

Areas probed in round 3 that PASSED: radix literals `#16r`/`#2r`/`#36r`/`#x`/`#b`/`#o`,
`?λ`/`"λ"` unicode, `?\C-\M-a` nesting, `?\^?`, `'()`, normal-magnitude float printing,
`(expt 0 0)`, `(sqrt -1)`→NaN, `mod`/`%` signs, `ash`/`lsh`/`logand`, `ffloor`/`fround`,
`flatten-tree`/`ensure-list`/`take`/`ntake`/`proper-list-p`/`delete`/`remq`/`delete-dups`/
`assq-delete-all`/`safe-length`, `seq-*` on lists, `format` `%c`/`%.Nf`/`%-N.Ms`/`%g`,
`string-pad`(2/3-arg)/`split-string`/`string-trim`/`mapconcat`/`read`, `pcase`
pred/and/guard, `apply-partially`, hash put/get/maphash, `string<`/`string-greaterp`,
`aref`/`elt`/`copy-sequence`/`reverse`/`sort` on vectors.

---

# Round 4 — additional confirmed divergences (vs `emacs -Q` 30.2)

Fourth pass against the current binary. Ground truth = bare `emacs -Q --batch`;
`cl-*`/`subr-x`-only symbols void there are excluded. No overlap with rounds 1–3.

## Behavioral — wrong values / wrong errors

### R4-A. ✅ FIXED — `letrec` is broken
- `(letrec ((a 1) (b (+ a 1))) (list a b))` → Emacs `(1 2)`, elisprs `error: …void: a`
- `(letrec ((f (lambda (n) (if (= n 0) 1 (* n (funcall f (1- n))))))) (funcall f 5))` →
  Emacs `120`, elisprs `error: invalid-function`. Forward/self references don't resolve
  (`letrec` not in `src/prelude.rs`).

### R4-B. ✅ FIXED — `if-let` / `when-let` only bind the FIRST clause of a multi-binding list
- `(if-let ((a 1) (b 2)) (+ a b) 'no)` → Emacs `3`, elisprs `error: …void: b`
- `(if-let (a 1) a)` (single var-form) → Emacs `1`, elisprs `error: wrong-type-argument: listp a`
- Macros at `src/prelude.rs:368-373` use only `(car binding)`. (Round 2's single nested-binding
  case passes; the multi-binding and short forms don't.)

### R4-C. ✅ FIXED — `if-let*` / `when-let*` / `and-let*` undefined
- `(when-let* ((a 1) (b 2)) (+ a b))` → Emacs `3`, elisprs `error: void-function: b`
  (the `*` variants aren't defined, so `b` evaluates as a call)

### R4-D. ✅ FIXED — `seq-let` is broken
- `(seq-let (a b) (list 1 2 3) (list a b))` → Emacs `(1 2)`, elisprs `error: …void: b`
  (also the vector-pattern form). Destructuring binder not implemented.

### R4-E. ✅ FIXED — `condition-case` ignores the `:success` handler
(host.rs `intrinsic_condition_case` Ok-branch: run a `:success` handler with VAR bound to the value)
- `(condition-case x 5 (:success (* x 2)))` → Emacs `10`, elisprs `5`
- The `:success` clause (run when BODY returns normally, VAR bound to the result) is dropped.
  `src/compiler.rs:257` / `src/host.rs:1126`.

### R4-F. ✅ FIXED — `butlast` with negative N appends a spurious `nil`
(prelude: clamp `keep` to `(min (length lst) (- (length lst) n))`)
- `(butlast '(1 2 3) -1)` → Emacs `(1 2 3)`, elisprs `(1 2 3 nil)`
- `src/prelude.rs:283` computes `keep = len - n` = 4 for n=-1 and walks `(nth 3 …)`→nil.
  Emacs returns a full copy for any N ≤ 0. (N=0 happens to work.)

### R4-G. ✅ FIXED — Printer doesn't abbreviate `quote` / `function`
(host.rs `print_list`: two-element `(quote X)`/`(function X)`/`` (` X) `` print as `'X`/`#'X`/`` `X ``)
- `(prin1-to-string '(quote a))` → Emacs `"'a"`, elisprs `"(quote a)"`
- `'(function f)` → Emacs `"#'f"`, elisprs `"(function f)"`; same under `princ`/`format "%S"`.
  `print-quoted` defaults non-nil; two-element quote/function/backquote/unquote lists should
  print with reader sugar.

### R4-H. ✅ FIXED — `format` `%e` uses wrong exponent format and drops default precision
- `(format "%e" 31415.9)` → Emacs `"3.141590e+04"`, elisprs `"3.14159e4"`
- `(format "%e" 1.0)` → Emacs `"1.000000e+00"`, elisprs `"1e0"`
- Exponent lacks sign + 2-digit zero-pad; default 6-digit precision not applied. (Round-1 #17
  had `%e` returned-literally; now implemented but mis-formatted.)

### R4-I. `format` `%g` ignores precision and the exponent-switch threshold — ✅ FIXED (R5-H)
- `(format "%.3g" 3.14159)` → Emacs `"3.14"`, elisprs `"3.14159"`
- `(format "%g" 1000000.0)` → Emacs `"1e+06"`, elisprs `"1000000"`
- Fixed in R5-H: `format_g` now implements C-printf `%g` — exponent form when the decimal
  exponent is `>= precision` (default 6) or `< -4`, precision counts significant digits,
  trailing zeros trimmed (kept with `#`), width/sign flags honored.

## Missing builtins / macros — `emacs -Q` returns a value, void in elisprs

- **Macros:** `pcase-exhaustive` (`two`), `with-suppressed-warnings`
- **Completion:** `try-completion` (`"foo"`), `all-completions` (`("foo")`), `test-completion`
  (`t`), `assoc-string` (`(assoc-string "A" '("a") t)`→`"a"`)
- **Lists/plists:** `lax-plist-get`
- **Seq:** `seq-set-equal-p` (`t`), `seq-sort-by` (`(3 2 1)`)
- **Hash tables:** `hash-table-test` (→`eql`), `hash-table-size` (→`4`)
- **Printing:** `pp-to-string` (`"(1 2)\n"`)

Areas probed in round 4 that PASSED: `while-let`, `dlet` (was R3-missing — now present),
`named-let`; `mapc`/`mapcan`/`mapconcat`(1-arg)/`assoc-default`/`plist-member`/`plist-put`/
`alist-get` DEFAULT **and** the 5-arg TESTFN form (just fixed); `take`/`ntake`/`butlast 0`/
`flatten-tree` dotted/`number-sequence` float step; the full `seq-` family on lists;
`floor`/`ceiling`/`truncate`/`round`/`ffloor`/`fround`/`fceiling`/`ftruncate` on negatives,
`natnump`/`zerop`/`logand`/`logior`/`logxor` identities, `abs`/`number-to-string`/`logb`;
`keywordp`/`symbol-name :x`/`make-symbol`/`apply #'max`; `make-vector`/`make-list`/
`make-string`/`string`/`char-to-string`/`string-to-char`/`vconcat`/`append` vector; printer
`-0.0`/dotted-cons/`%S` vector/`%s nil`; `format` `%-10s`/`%010.3f`/`%5c`/`%x`/`%d` of char;
`ignore-errors`/`ignore`/`always`/`xor`/`prog1`/`prog2`.

### R5-A. `pcase (app FN PAT)` / `(pred LAMBDA)` / `setf` places — ✅ FIXED
- `(pcase 5 ((app 1+ 6) 'yes))` → Emacs `yes`, was void
- `(pcase 3 ((pred (lambda (n) (> n 1))) 'big))` → Emacs `big`, was void (lambda as FN)
- `(let ((a (list (cons 1 2)))) (setf (alist-get 1 a) 99) a)` → Emacs `((1 . 99))`, was unsupported place
- `(let ((p (list :a 1))) (setf (plist-get p :b) 2) p)` → Emacs `(:b 2 :a 1)` (prepends new key)
- `(cl-typep 5 'integer)` → Emacs `t`, was void
- Fixed: added `pcase--apply` (handles lambda / named / curried FN), the `app` arm and a
  lambda-aware `pred` arm in `pcase--compile`; `setf--expand` places for `alist-get`/`plist-get`;
  `cl-typep`.

### R5-B. Missing `cl-*-if` count/position + `string-fill` — ✅ FIXED
- `(cl-count-if #'cl-oddp '(1 2 3 4 5))` → Emacs `3`, was void
- `(cl-count-if-not …)`, `(cl-position-if …)`, `(cl-position-if-not …)` likewise void
- `(string-fill "a b c d" 3)` → Emacs `"a b\nc d"`, was void
- Fixed: added the four `cl-*-if` predicates (honoring `:key`) next to `cl-count`/`cl-position`,
  and `string-fill` (greedy wrap at spaces).

### R5-C. Missing `cl-` integer math + `cl-oddp` negative bug — ✅ FIXED
- `(cl-floor 7 2)` → Emacs `(3 1)`; `cl-ceiling`/`cl-truncate`/`cl-round` likewise void
- `(cl-mod 7 3)`/`(cl-rem -7 3)`/`(cl-gcd 12 18 8)`→`2`/`(cl-lcm 4 6 10)`→`60`/`(cl-isqrt 17)`→`4`: all void
- `(cl-oddp -3)` → Emacs `t`, elisprs `nil` (used `(= (% n 2) 1)`, wrong for negatives)
- Fixed: added the two-value `cl-floor`/`cl-ceiling`/`cl-truncate`/`cl-round` (on the existing
  2-arg builtins), `cl-mod`/`cl-rem`, variadic `cl-gcd`/`cl-lcm`, `cl-isqrt`; `cl-oddp` now uses
  `/=`.

### R5-D. Missing `cl-` set/seq family + `cl-reduce :from-end` — ✅ FIXED
- `cl-union`/`cl-intersection`/`cl-set-difference`/`cl-adjoin`/`cl-subst`/`cl-maplist`/`cl-merge`/
  `cl-stable-sort`/`cl-delete-duplicates`/`cl-endp`: all void
- `(cl-reduce #'- '(1 2 3) :from-end t)` → Emacs `2`, elisprs `-4` (folded left, ignored `:from-end`)
- Fixed: added the listed functions (set ops honor `:test`, result orders match Emacs —
  union/intersection reversed-scan, set-difference forward); `cl-reduce` now does a right fold
  for `:from-end` and applies `:key`. (NOTE: still no `:count`/`:start`/`:end` bounding keywords
  on the `cl-remove`/`cl-position` family — tracked separately.)

### R5-E. `split-string` regexp + `cl` bounding keywords + misc — ✅ FIXED
- `(split-string "a1b2c" "[0-9]")` → Emacs `("a" "b" "c")`, elisprs `("a1b2c")` (SEPARATORS was
  matched literally, not as a regexp)
- `(cl-remove-if #'cl-oddp '(1 2 3 4) :count 1)` → Emacs `(2 3 4)`, was `wrong-number-of-arguments`
- `(cl-position 3 '(1 2 3 4 3) :start 3)` → Emacs `4`, elisprs `2`; `cl-count` ignored `:start`/`:end`
- `(format-message "use `%s'" "x")` → Emacs `"use ‘x’"` (grave/apostrophe not curve-quoted)
- `(string-version-lessp "foo2" "foo10")` → Emacs `t`, was void
- Fixed: `split_string` now compiles SEPARATORS via the regexp engine; added `:count` to the
  `cl-remove-if` family and `:start`/`:end` (via `cl--in-bounds`) to `cl-position`/`cl-count`/
  `cl-position-if`/`cl-count-if`; `format-message` curve-quotes its format string; added
  `string-version-lessp` (numeric-run compare). Resolves the `:count`/`:start`/`:end` note above.

### R5-F. Missing `cl-do`/`cl-the`/`cl-etypecase` + `cl-loop`/`cl-db` destructuring — ✅ FIXED
- `(cl-do ((i 0 (1+ i)) (s 0 (+ s i))) ((= i 4) s))` → Emacs `6` (parallel steps), was
  `Symbol's value as variable is void: s`
- `(cl-the integer 5)` → `5`; `(cl-etypecase 5 (integer 'i))`/`cl-ecase`: all void
- `(cl-loop for (a b) in '((1 2) (3 4)) collect (+ a b))` → Emacs `(3 7)`, was `let: binding
  name must be a symbol`; dotted `(k . v)` patterns errored `wrong-type-argument: listp v`
- Fixed: added the macros (`cl-do` uses temp-bound parallel stepping); `cl-loop`'s `for … in`
  now destructures a pattern via `cl-db--binds`; `cl-db--binds` handles a dotted-list tail (so
  `cl-destructuring-bind` `(a . b)` works too).
- STILL TODO: `(pcase S ((rx …) …))` — `rx` patterns inside `pcase` are unsupported (needs the
  `rx`→regexp compiler wired into `pcase--compile`).

### R5-G. Missing width/byte/type utilities — ✅ FIXED
- `(string-width "日本語")`→`6`, `(char-width ?日)`→`2`, `truncate-string-to-width`,
  `(string-bytes "héllo")`→`6`, `(subst-char-in-string ?a ?X "banana")`→`"bXnXnX"`: all void
- `(cl-type-of 5)`→`fixnum`, `(number-or-marker-p 5)`/`(integer-or-marker-p 5)`→`t`: all void
- Fixed: added all the above in the prelude. `char-width` covers the East-Asian wide/fullwidth
  ranges (→2) and combining marks (→0); `cl-type-of` refines `type-of` (`fixnum`/`null`/`cons`).
- KNOWN GAPS this sweep (deferred): `(type-of (lambda …))` → Emacs 30 `interpreted-function`
  (we return `function`); `(string-replace "" …)` should signal `wrong-length-argument`.

### R5-H. `format` `%g` C-printf semantics — ✅ FIXED
- See R4-I above (now resolved). `(format "%g" 1234567.0)`→`"1.23457e+06"`,
  `(format "%g" 0.00001)`→`"1e-05"`, `(format "%#g" 1.5)`→`"1.50000"`, all match Emacs.
- Still deferred: `%E` is invalid in Emacs (signals an error) but we emit it verbatim — minor.

### R5-I. `sort` panic / Emacs-30 keyword form + `cl-defstruct` options — ✅ FIXED
- `(sort (list 3 1 2))` (no predicate) → **Rust panic** `index out of bounds` (indexed `args[1]`
  unconditionally); now `(1 2 3)` via default `value<`.
- `(sort SEQ :key … :lessp … :reverse …)` (Emacs-30 keyword form) → `void-function: :key`; now
  supported (`:key`/`:lessp`/`:predicate`/`:reverse`).
- `(cl-defstruct (pt3 (:constructor mk)) a)` then `(mk :a 5)` → `void-function: mk`; now the
  `(:constructor NAME)` and `(:conc-name PREFIX)` options are honored.
- Fixed in `host.rs` (`merge_sort_by` now sorts `(key,item)` pairs with an optional predicate +
  `value_lt` fallback; the `sort` arm parses both call forms) and the `cl-defstruct` macro.
- STILL TODO: real `record`/`make-record`/`recordp` primitives — `cl-defstruct` rides on
  tagged vectors, so `(record 'foo 1 2)` and a true record type (distinct from `vectorp`) are
  unsupported. Architectural (needs a new heap object kind); deferred pending owner go-ahead.

### R5-J. More `seq.el` functions + `seq-partition` type — ✅ FIXED
- `seq-sort-by`/`seq-split`/`seq-positions`/`seq-remove-at-position`: all void;
  `(seq-mapcat #'list '(1 2) 'list)` → `wrong-number-of-arguments` (missing optional TYPE)
- `(seq-partition [1 2 3 4 5] 2)` → Emacs `([1 2] [3 4] [5])`, elisprs returned list chunks
- Fixed: added the four functions, gave `seq-mapcat` its TYPE arg (via `seq-concatenate`), and
  made `seq-partition` keep the input's element type.
- Note: `with-memoization` still void — its only sweep case was a degenerate misuse; skipped.

### R5-K. Transcendental float math was entirely missing — ✅ FIXED
- `(log 100 10)`→`2.0`, `(exp 1)`, `(sin 0)`, `(cos 0)`, `(tan 0)`, `(asin 1)`, `(acos 1)`,
  `(atan 1)`/`(atan 1 1)`, `(ldexp 1.5 3)`→`12.0`, `(frexp 8.0)`→`(0.5 . 4)`,
  `(copysign 3.0 -1.0)`→`-3.0`, `(cl-parse-integer "42")`: all void
- Fixed: added all the above as Rust builtins (`log` takes an optional base; `atan` does `atan2`
  with two args; `frexp` returns a `(significand . exponent)` cons) plus `cl-parse-integer` in
  the prelude. Results match Emacs (both use the platform libm).
- Deferred (architectural): `(truncate 1.0e+300)` needs bignums; `float-time`/`current-time`
  are non-deterministic so not parity-testable.

### R5-L. `cl` list/plist gaps + `cl-remove-duplicates :from-end` — ✅ FIXED
- `(cl-remove-duplicates '(1 2 1 3) :from-end t)` → Emacs `(1 2 3)` (keep first), elisprs `(2 1 3)`
- `cl-pairlis`, `cl-tailp`, `cl-ldiff`, `lax-plist-get`: all void
- Fixed: `cl-remove-duplicates` now branches on `:from-end`; added the four functions
  (`cl-tailp`/`cl-ldiff` walk the cdr chain by `eq` identity, matching Emacs).
- Note: `map-merge` (and the rest of `map.el`) are void in `emacs -Q` too — not divergences.

### R5-M. `char-equal` case-fold + `cl-assert`/`cl-check-type`/`format-spec` — ✅ FIXED
- `(char-equal ?a ?A)` → Emacs `t`, elisprs `nil` — it ignored `case-fold-search` (which
  defaults to `t`, so the comparison folds case by default)
- `cl-assert`/`cl-check-type` void (`integer` read as a variable); `format-spec` void
- Fixed: `char_equal` now folds case via `case_fold_search`; added the macros (`cl-assert`
  signals `cl-assertion-failed`, seeded as a child of `error`; `cl-check-type` uses `cl-typep`)
  and `format-spec` in the prelude.
- (Resolved in R5-N below.)

### R5-N. `cl-defgeneric` / `cl-defmethod` type-dispatch generics — ✅ FIXED
- `(cl-defgeneric area (s)) (cl-defmethod area ((s integer)) (* s s)) (area 4)` → `16`; were void.
- Implemented in the prelude: a per-name method table (`cl--generic-table`), a dispatcher that
  matches each arg against its specializer and picks the most specific applicable method
  (`integer` > `number`, `(eql V)`/`(head V)` > a plain type), unspecialized args, multi-arg
  dispatch, method redefinition (replace by equal specializers), and `cl-no-applicable-method`.
- Verified vs Emacs across 10 cases (disjoint types, specificity, eql, fallback, multi-arg).
- Follow-up DONE in R5-O: full method combination implemented.

### R5-P. `read-from-string`/`pp-to-string` + `seq-contains-p`/`remove` type + cl bits — ✅ FIXED
- `read-from-string`/`pp-to-string`/`cl-substitute-if`/`cl-mapcan`/`string-to-multibyte`/
  `multibyte-string-p`: all void
- `(seq-contains-p "abc" ?b)` → `wrong-type-argument: listp` (only handled lists)
- `(remove 3 [1 2 3])` → Emacs `[1 2]`, elisprs `(1 2)` (didn't preserve the vector type)
- Fixed: added `read_one(src,start)->(form,end)` in the reader + a `read-from-string` subr;
  `pp-to-string`/`pp`, the `cl-substitute-if[-not]`/`cl-mapcan` and uni/multibyte shims;
  `seq-contains-p` coerces via `append`, `remove` re-`vconcat`s a vector input.
- (Hit the prelude-ordering gotcha once more — `multibyte-string-p` first used `dolist` before
  it's defined; rewrote with `while`.)
- Still architectural/deferred: bool-vectors (`#&N…`), text properties (`propertize`/
  `#("x" 0 1 (…))`). (Buffer functions `with-temp-buffer`/`insert` and the full
  text-buffer/point/narrowing/`save-excursion` core landed later — see CHANGELOG.)

### R5-Q. More `cl-loop` clauses — ✅ FIXED
- `for V across SEQ`, `for V being [the|each] {elements|hash-keys|hash-values} of SRC`,
  `for V = INIT [then STEP]`, `when/unless COND return X`, `named NAME`: all errored
  (`unsupported clause` / `expected an accumulation clause, got return`).
- Fixed in the `cl-loop` macro. Subtlety: `for = then` must be modeled like the numeric `for`
  (init in `binds`, step at end in `steps`) so V is current when a later `until`/`while` test
  runs — `(cl-loop for x = 5 then (1- x) until (= x 0) collect x)` → `(5 4 3 2 1)`. `return`
  added as an action in `cl-loop--accum` (so it works inside `when`/`if`).

### R5-R. `pcase-let*` / `pcase-dolist` / pcase `seq` pattern — ✅ FIXED
- `pcase-let*` and `pcase-dolist` were undefined → `invalid-function` (the binding list parsed
  as a call). `(pcase '(1 2) ((seq a b) …))` → `unsupported pattern (seq a b)`.
- Fixed: `pcase-let*` reuses `pcase-let` (its `let*` expansion is already sequential);
  `pcase-dolist` wraps `dolist` + `pcase-let`; the `seq` pattern compiles each subpattern
  against `(elt VAL i)` under a `sequencep` guard (works on lists, vectors, strings).
- Vector patterns DONE in R5-S. `(rx …)` patterns still TODO.

### R5-S. Backquoted vector templates + pcase vector patterns — ✅ FIXED
- `` `[,a ,b] `` in value position stayed a literal vector of `(unquote a)` forms (didn't
  evaluate); as a pcase pattern it errored `unsupported pattern`.
- Fixed: `bq_expand` now folds a vector template into `(vconcat LISTFORM)` (so `,`/`,@` work);
  `pcase--compile` reads a `vconcat`-headed pattern as a vector match — `(vectorp VAL)` guard +
  match the cons-pattern against `(append VAL nil)`. Exact-length matching falls out of the
  cons pattern's `nil` terminator; non-vectors fail cleanly (the `lv` binding is guarded).
- Also hardened the `seq` pattern (R5-R) the same way so `(pcase 5 ((seq a b) …))` fails
  instead of erroring on `(elt 5 0)`.
- `(rx …)` patterns DONE in R5-T.

### R5-T. `rx` macro + `(rx …)` pcase pattern — ✅ FIXED
- `rx` was entirely void. Added a prelude `rx`→regexp-string compiler covering string/char
  literals; the named classes/anchors (`bol`/`eol`/`bos`/`eos`/`digit`/`alpha`/`space`/`word`/
  …); `seq`/`and`/`or`/`group`/`group-n`; quantifiers `*`/`0+`/`+`/`1+`/`?`/`opt`/`=`/`>=`/
  `**`/`repeat`; char sets `(any …)`/`(in …)` and `(not …)`; and `literal`/`regexp`. Matches
  Emacs's output including the single-char-`or`→`[abc]` folding.
- Wired `(rx …)` into `pcase--compile` (string-match the value, guarded by `stringp`).
- Minor remaining cosmetic gap: Emacs sorts char-set ranges (`(any "a-z" "0-9")` → `"[0-9a-z]"`,
  we emit `"[a-z0-9]"`) — functionally identical, byte order differs.

### R5-O. `cl-defmethod` method combination + `cl-coerce`/`cl-gensym`/`cl-digit-char-p` — ✅ FIXED
- A qualified method clobbered the primary: `(cl-defmethod q :before ((x integer)) …)` made
  `(q 5)` return the `:before` value (`nil`) because dedup keyed only on specializers, so the
  `:before` replaced the primary with equal specs.
- `cl-coerce`/`cl-gensym`/`cl-digit-char-p` were void.
- Fixed: methods now store a QUALIFIER (dedup keys on qualifier+specs); the dispatcher orders
  applicable methods by specificity and runs the CLOS effective method — `:around` (most
  specific first, wrapping) → `:before` (all, most specific first) → primary chain → `:after`
  (all, least specific first). `cl-call-next-method`/`cl-next-method-p` walk the chain via the
  dynamic `cl--cnm-next`/`cl--cnm-args`; exhaustion signals `cl-no-next-method`. Added the three
  `cl-` functions. Verified vs Emacs (before/after order, around wrapping, primary chaining `1 2 3`).

### R5-U. Real `record` type + `bool-vector` + `nadvice` — ✅ FIXED
- **Record slot-0 leak.** Records/cl-defstruct instances were `cl-struct-NAME`-tagged vectors, so
  `(aref (record 'foo 1 2) 0)` returned `cl-struct-foo` instead of `foo`, and `(vectorp REC)`
  was wrongly `t`. Added a real `Obj::Record` variant (slot 0 = the bare type symbol): `aref`/
  `aset`/`length`/`equal`/`copy-sequence`/`type-of`/`recordp`/print (`#s(…)`) and the `#s(NAME …)`
  reader all handle it; `record`/`make-record` are now primitives; `vectorp` is `nil` and a record
  is not a sequence (`vconcat`/`append`/`mapcar` signal `sequencep`). cl-defstruct builds records
  (bare-NAME tag; predicate walks the bare-name parent chain). This resolves the R5-I "STILL TODO"
  above. Verified vs Emacs 30.2 (14 cases, byte-for-byte).
- **`bool-vector`.** Was type-name-only. Added `Obj::BoolVector`, `make-bool-vector`/`bool-vector`,
  the `#&N"…"` reader+printer (LSB-first byte packing, print.c escaping), `aref`/`aset` (t/nil,
  non-nil stored as t)/`length`/`elt`, `bool-vector-p`, `arrayp`/`sequencep` (bool-vector is an
  array AND a sequence, not a vector), and `bool-vector-count-population`/`-subsetp`/`-not`.
  Resolves the R5-P "architectural/deferred" note. Verified vs Emacs 30.2 (32 cases, incl. the
  `(wrong-length-argument …)` shapes).
- **`nadvice`.** `advice-add`/`advice-remove`/`add-function`/`remove-function`/`define-advice`/
  `advice-member-p` were all void. Faithful port of `emacs-lisp/nadvice.el` (Emacs 30.2) into the
  prelude (`prelude::NADVICE`, loaded after the oclosure/gv substrate it needs) — advices are
  `advice` oclosures threaded into the symbol-function cell honoring `depth`. All ten combinators
  (`:around`/`:before`/`:after`/`:override`/`:filter-args`/`:filter-return`/`:before-while`/
  `:before-until`/`:after-while`/`:after-until`) work. Distinct from the glob-AOP intercept layer
  (`src/intercepts.rs`). Emacs-help cosmetics (docstring/interactive-form/print/called-interactively
  machinery, buffer-local advice places) are omitted (NAMED in the port header). Verified vs Emacs
  30.2. Still absent: legacy `defadvice` (a separate ~3.3k-line `advice.el` `ad-*` subsystem, not
  nadvice — deferred rather than shimmed).

## Round 6 — text properties, and what a wider fuzz sweep turned up

The differential fuzzer against Emacs 30.2 (`scripts/fuzz_parity.sh`) run over eight
seeds × 600 forms had six divergences; three of them were one gap.

### R6-A. Text properties do not survive the functions that copy characters — ✅ FIXED
`propertize` registered per-character plists and the printer emitted `#(…)` intervals,
but every function that *built a new string* dropped them, which is most of the ways a
propertized string is ever used.

- `(concat (propertize "a" 'x 1) "b")` → Emacs `#("ab" 0 1 (x 1))`, elisprs `"ab"`
- `(substring (propertize "abcd" 'face 'bold) 1 3)` → `#("bc" 0 2 (face bold))` vs `"bc"`
- `(upcase (propertize "ab" 'p 1))`, `(capitalize …)` → properties lost
- `(format "x%sy" (propertize "A" 'p 1))` → `#("xAy" 1 2 (p 1))` vs `"xAy"`

Fixed by `PhpHost`-style piece mapping in the host (`ElispHost::string_carry_props`):
each builder names, for every run of its result, the source string and offset the run
came from, and the per-character plists follow. Wired into `concat` (per argument, with
char-list and vector arguments contributing property-less runs), `substring` (re-based
to the slice), `upcase`/`downcase` (character-for-character), `capitalize` (via the
`elisprs--carry-text-properties` internal primitive, since it is written in elisp), and
`format`'s `%s`. `substring-no-properties` explicitly drops them again.

Padding follows Emacs: `(format "%-4s|" (propertize "ab" 'p 1))` is propertized over all
four columns because padding that *follows* the argument is inside its interval, while
`(format "%4s|" …)` leaves the leading spaces outside it.

Also fixed a printer bug the work exposed: `(p nil)` and "no properties at all" were
merged into one interval, because the plist-subset walk cannot tell a key whose value is
nil from an absent key. `(concat (propertize "ab" 'p nil) "c")` now prints
`#("abc" 0 2 (p nil))` as Emacs does.

**Not modelled:** Emacs also propagates the *format string's own* text properties onto
its literal characters. Only the `%s` argument's half is carried here.

### R6-B. `(ash N HUGE)` builds the number instead of signalling — ✅ FIXED
`(ash 3 123456788)` hung: the guard was `count > 2^30`, so a count of 1.2e8 was allowed
and the shift attempted a 15-megabyte bignum. Emacs bounds an integer at `integer-width`
bits (65536 by default) and signals `overflow-error` past it, which is now what happens —
`(ash 1 65535)` still succeeds, `(ash 1 65536)` signals, and `(ash 0 ANY)` is 0.

### R6-C. `replace-regexp-in-string` accepts invalid replacement escapes — ✅ FIXED
`search.c` allows only `\&`, `\N`, `\\` and `\?` after a backslash in replacement text
and signals `Invalid use of ‘\’ in replacement text` otherwise. Both bad cases were
silently accepted: `(replace-regexp-in-string "a" "x\\" "ab")` produced `"x\\b"` and
`(… "a" "\\q" "ab")` produced `"qb"`, where Emacs signals for each.

### R6-D. `take` / `seq-take` signal the wrong type for a non-integer N — ✅ FIXED
`(seq-take nil 'car)` gave `number-or-marker-p` where Emacs gives `integerp`, and
`(seq-take '(1 2) 1.5)` returned the list instead of signalling. `take` now requires an
integer (fns.c `Ftake`), and `seq-take` only pre-checks `number-or-marker-p` for the
non-list sequences that reach a comparison first.

### Still open after the sweep

- **`hash-table-size`** stays void. Emacs 30 reports the table's allocated *capacity*
  (`(make-hash-table)` → 0, two `puthash` later → 6), which is an artifact of its
  hashing internals; elisprs stores entries in a `Vec` and has no such number, so
  answering would mean inventing one.
- **A closure's captured environment inside an error message.** `(dotimes (i 1)
  (cl-reduce (lambda (x) …) nil))` reports the arity error with elisprs's expansion
  environment (`((--dotimes-limit-- . 1) (i . 0))`) where Emacs shows `(t)`. The lambda
  is the same one; only the env printed beside it differs.
- **Printer variables baked into a builtin's error data** — see the residual note on
  R3-K above.

---

## Round 7 — the areas the fuzz grammar never generated

`scripts/fuzz_parity.sh -n 1500 -s 1` reported **1/1500**: the generated corpus was
mined out. The gaps below came from hand-built differential probe corpora covering
what `scripts/fuzz/gen.el` does not emit — `print-circle`, binding forms, the
error-condition system, macro definition order, `cl-loop` clause shapes. The
generator now emits all of them, so this ground is fuzzed from here on.

### R7-A. `print-circle` did nothing, and a circular list hung the printer — ✅ FIXED
The printer had no notion of shared structure, and — worse — no way to stop.

- `(let ((print-circle t)) (prin1-to-string (let ((y (list 1))) (list y y))))`
  → Emacs `"(#1=(1) #1#)"`, elisprs `"((1) (1))"`
- `(let ((print-circle t)) (prin1-to-string (let ((x (list 1 2))) (setcdr (cdr x) x) x)))`
  → Emacs `"#1=(1 2 . #1#)"`, elisprs **hung forever**

The hang is the important half: the `PRINT_CIRCLE` = 200 depth guard counts
*nesting*, and `print_list` walks the cdr chain in an iterative loop, which never
grows depth — so a circular cdr appended to the output buffer until the process
died. Under an untimed harness that is indistinguishable from a stall.

Fixed with a reference-counting pre-pass (`ElispHost::scan_shared`) that runs only
when `print-circle` is non-nil and records every arena id reachable more than once;
those objects print `#N=` on first appearance and `#N#` thereafter, in conses,
vectors and records alike, including a shared/circular *tail* (`#1=(1 2 . #1#)`).
`print-circle` also had to become a `defvar` — without one, `let` bound it
lexically and the Rust printer, which reads the value cell, never saw it.

**Still open:** with `print-circle` **nil**, Emacs prints a ` . #N` marker from its
own cycle detector — `(1 2 . #2)` for a period-2 cycle, `(1 2 3 4 1 2 3 4 1 2 . #6)`
for period 4. The marker sequence (0, 2, 2, 6, 6, 6, 6 for periods 1–7) is a Brent-
style tortoise schedule that could not be reproduced from behavior alone, and no
`print.c` is vendored in this repo to port from. elisprs now detects the cycle with
a Floyd tortoise and takes print.c's *other* branch — `error "Apparently circular
structure being printed"`. It terminates, which beats hanging, but it is a NAMED
divergence until the real schedule can be ported.

### R7-B. `dolist` reused one binding for every iteration — ✅ FIXED
subr.el binds the variable with a `let` *inside* the loop; elisprs hoisted it into
the enclosing `let` and `setq`'d it each pass. Invisible until a closure escapes:

- `(let (r) (dolist (x '(1 2 3) r) (push (lambda () x) r)) (mapcar #'funcall r))`
  → Emacs `(3 2 1)`, elisprs `(nil nil nil)` — every closure shared one cell.

### R7-C. An `error` handler caught `quit` — ✅ FIXED
`condition-case` matched a handler named `error` unconditionally instead of
consulting the signal's `error-conditions`, and `define-error` had separately given
`quit` the default parent `error` (data.c seeds it with `(quit)` alone).

- `(condition-case nil (signal 'quit nil) (error 'caught) (t 'top))`
  → Emacs `top`, elisprs `caught`

Only `t` is a catch-all. `error` now matches a signal whose chain contains `error`,
plus the internal Rust-string errors that carry no `define-error` registration.

### R7-D. An `unwind-protect` cleanup that signalled was swallowed — ✅ FIXED
The cleanup's result was discarded (`let _ =`), so a failure inside a cleanup form
vanished and the body's error propagated in its place.

- `(condition-case e (unwind-protect (error "in") (error "cleanup")) (error (cadr e)))`
  → Emacs `"cleanup"`, elisprs `"in"`

### R7-E. A macro was unusable in the form that defined it — ✅ FIXED
- `(progn (defmacro m (a b) (list 'list a b)) (m 1 2))`
  → Emacs `(1 2)`, elisprs `error "macro called as a function"`

Expansion walked the whole `progn` before the `defmacro` had run, so the use site
compiled as a function call. `defmacro` siblings are now installed *during*
expansion — what Emacs's byte-compiler does explicitly, and what its interpreter
gets for free by evaluating `progn` forms one at a time.

### R7-F. `cl-loop` `while`/`until` tested the previous iteration — ✅ FIXED
They were folded into the `while` test, which runs *before* the `for` clause steps
its variable — so on the first pass the variable was still nil.

- `(cl-loop for i in '(1 2 3) while (< i 3) collect i)`
  → Emacs `(1 2)`, elisprs `wrong-type-argument number-or-marker-p nil`

They now emit where they appear and exit through a separate `--cl-loop-end--` tag,
because a while/until exit is a *normal* termination: the accumulator and `finally`
still have to produce the value (`… while (< i 3) collect i finally return 'fin`
is `fin`, not the collected list).

### R7-G. `cl-loop` `downfrom … to` never ran — ✅ FIXED
`to` was unconditionally `<=`, so the guard was `(<= 3 1)`.

- `(cl-loop for i downfrom 3 to 1 collect i)` → Emacs `(3 2 1)`, elisprs `nil`

### R7-H. Smaller confirmed divergences — ✅ FIXED
- `(type-of (lambda ()))` → `interpreted-function` (Emacs 30 renamed it), and a
  macro's function cell is a `(macro . FUNCTION)` cons, so `type-of` says `cons`.
- `(with-output-to-string (print 'a))` → `"\na\n"`: print.c writes a newline
  *before* the object as well as after, which is what separates successive `print`s.
- `(symbol-value :a)` → `:a`; a keyword is its own value, not `void-variable`.
- `(cl-letf* ((x 1)) x)` → `1`; a plain-symbol place must become an ordinary `let`
  (cl-macs.el does this), since saving the old value first *reads* an unbound `x`.
- `(macroexpand '(cl-incf x))` → `(setq x (1+ x))`, not `(progn (setq x (+ x 1)))`.
  A duplicate `cl-decf` definition shadowing the first was removed at the same time.
- `cl-psetq` and `substitute-command-keys` were void.

### R7-I. What the widened generator found on its first run — partly ✅ FIXED
Re-running `scripts/fuzz_parity.sh -n 1500 -s 1` with the new clause shapes took
the count from **1/1500** (the old grammar, mined out) to **14/1500** — all of them
in functions this round had not touched, i.e. shapes the corpus had never produced.
Four are fixed, taking it to **11/1500**; regenerating the *pre-change* corpus from
`HEAD:scripts/fuzz/gen.el` still gives exactly **1/1500**, so nothing regressed.

Fixed:

- `(error-message-string nil)` → Emacs `"peculiar error"`, elisprs `"nil"`; and
  `(error-message-string '(t nil))` → `"peculiar error: nil"` vs `"t: nil"`. A car
  with no `error-conditions` chain does not name a condition.
- `(seq-into "ab" 'foo)` → Emacs signals `Not a sequence type name: foo`; elisprs
  handed the string back.
- `(cl-rem 7.5 2)` → Emacs `1.5`, elisprs `wrong-type-argument integer-or-marker-p`.
  cl-lib.el builds `cl-rem`/`cl-mod` from `cl-truncate`/`cl-floor`, so they take
  floats; `%`/`mod` do not.
- `(last (cons "z" 1.5) 7)` → Emacs `("z" . 1.5)`, elisprs `wrong-type-argument
  listp 1.5` — it measured the list with `length`, which an improper list has none
  of. `safe-length` now.

Still open from that run (all error-data shape, both engines signal or both
return; none is a wrong *value* in the ordinary case):

- `cl-gcd`/`cl-lcm`/`cl-adjoin` name a different predicate in the
  `wrong-type-argument` data than Emacs does (`numberp` vs `number-or-marker-p`,
  `sequencep` vs `listp`), because a different internal check is reached first.
- `compare-strings` and `seq-position` accept a bad START/index where Emacs
  signals `wrong-type-argument integerp`.
- `cl-some`/`format` report the *second* wrong argument where Emacs reports the
  first, an argument-evaluation-order artifact.

### R7-J. Loop-variable capture and nested `cl-flet` scoping — ✅ FIXED
A cross-frontend sweep flagged "one environment per call frame, no block scopes"
(closures made in a loop all sharing one binding). elisprs's **core environment
model is not affected**: `let`/`let*` create a fresh binding on every execution,
a binding does not leak past its body, and `unwind-protect` / `catch`-`throw`
crossing a `let` all restore correctly — 46 of 51 probes matched Emacs on the
first run, and the failures were confined to the MACRO layer, where a desugaring
had hoisted the loop variable out of the loop.

- `(let ((fs '())) (dotimes (i 3) (push (lambda () i) fs)) (mapcar #'funcall (nreverse fs)))`
  → Emacs `(0 1 2)`, elisprs `(3 3 3)` — the classic signature, and note it leaked
  the *post-loop* value, not even the last iteration's. Same defect and same fix
  as R7-B: subr.el runs the body inside `(let ((VAR counter)) …)`.
- `(cl-flet ((f (n) (* n 2))) (cl-flet ((f (n) (+ n 1))) (f 5)))` → Emacs `6`,
  elisprs `error "malformed lambda list"`. The call-site rewriter descended into
  the nested form's BINDING LIST and rewrote the binding's own head to
  `(funcall G_outer (n) …)`. It now skips a nested `cl-flet`/`cl-flet*`/`cl-labels`
  binding list and drops the names it binds from the alist for its body
  (`cl-labels` binding bodies see the inner names, `cl-flet` bodies do not).

`cl-loop`'s `for` variable is hoisted in **Emacs too**, so `(cl-loop for i from 0
below 3 do (push (lambda () i) fs))` agrees with Emacs without change — matching
Emacs is the contract, not an idealized per-iteration rule.

**Not applicable: thrown values across a coroutine boundary.** elisprs has no
coroutine machinery — `iter-defun`, `iter-yield`, `generator.el`, `corosensei` and
`GenContext` have zero occurrences in `src/` or `Cargo.toml`. Non-local exits are
plain Rust `Err` returns through `--catch--`/`--unwind--`/`--condition-case--`
intrinsics that take lambda thunks (`host.rs`), with the error object carried in
the host's `pending_error`/`pending_throw` slots; there is no context swap that
could strand them.

### R7-K. Closures always capture lexically, even under `lexical-binding` nil — ✅ FIXED in round 8 (R8-A)
Probing the same forms through `(eval FORM nil)` (dynamic binding) found one
precise, systematic gap: 14 of 17 probes matched, and all three failures are the
same cause — a `lambda` captures its free variables lexically regardless of the
binding mode.

- `(eval '(funcall (let ((x 1)) (lambda () x))) nil)` → Emacs `void-variable x`
  (the binding is gone by call time), elisprs `1`.
- `(eval '(let (fs) (dotimes (i 3) (push (lambda () i) fs)) …) nil)` → Emacs
  `void-variable i`, elisprs `(0 1 2)`.

`eval`'s LEXICAL argument does control the starting *environment* (`(eval 'x t)`
correctly signals `void-variable` rather than leaking the caller's scope), and
`let`/`let*`/`setq`/`defun` all behave identically under both modes. Only closure
*creation* ignores the mode. Closing it means a dynamic closure kind in
`compiler.rs`, with free variables resolved at call time instead of captured —
substrate work, not a prelude change, so it is named rather than half-done.
`(defvar lexical-binding nil)` in the prelude already documents that this
milestone binds dynamically.

### Still open after round 7

> Superseded by round 8 below: the `setf` entry was only half right (its
> *semantic* half is fixed — see R8-C), and the `type-of` and deep-recursion
> entries are fixed (R8-G, R8-D). Only the backquote and `hash-table-size`
> entries survive. Kept as written for the record.

- **`(prin1-to-string '`(a ,b ,@c))`** → Emacs `` "`(a ,b ,@c)" ``, elisprs
  `"(cons 'a (cons b (append c nil)))"`. The reader desugars backquote eagerly, so
  the backquote form is gone before anything can print it. Emacs's reader builds
  `` (` (a (, b) (,@ c))) `` and leaves the expansion to the `` ` `` macro; matching
  it means moving expansion out of `reader.rs` into a macro, which changes what
  every quoted-backquote datum in the prelude looks like — too broad to land here.
- **`(macroexpand '(setf (car x) 1))`** → Emacs `(let* ((v x)) (setcar v 1))`,
  elisprs `(progn (setcar x 1))`. Emacs routes every place through `gv-letplace`,
  which binds a temporary; elisprs's `setf` substitutes the place form directly.
  The two agree on every value and on evaluation count for the places `setf`
  supports — only the printed expansion differs — so this is shape, not semantics.
- **`(type-of (symbol-function 'when))`** → Emacs `cons`, elisprs `symbol`.
  `when`/`unless` are compiler intrinsics here with no function cell at all; the
  `macroexpand` family already consults `expand_intrinsic_macro` to compensate, but
  `symbol-function` has nothing to return.
- **`hash-table-size`** stays void, for the reason already recorded above: Emacs
  reports allocated capacity (`:size 10` → 10; five entries in a `:size 1` table →
  24), an artifact of its hashing internals that elisprs's `Vec` has no analogue
  for. Answering would mean inventing a number.
- **Deep recursion overflows the Rust stack.** `(cl-labels ((go (n acc) (if (= n 0)
  acc (go (1- n) (+ acc n))))) (go 100 0))` aborts with `fatal runtime error: stack
  overflow`; it survives at depth 60 and dies by 80, while Emacs handles 100
  comfortably. Each elisp frame costs several native frames, so this is a
  substrate-level fix (a growable stack, or trampolining `run_closure`), not a
  prelude one.

---

## Round 8 — dynamic binding, generalized places, and the AOT numeric contract

Round 7 left `scripts/fuzz_parity.sh -n 1500 -s 1` at **11/1500** on the widened
generator and named four open items. All eleven generator divergences are closed
and the corpus is at **1500/1500**; four of the five named items are closed too.

### R8-A. Closures captured lexically even under `lexical-binding` nil — ✅ FIXED

The round-7 entry (R7-K) measured this as "14 of 17 probes match". That
understated it, because the probes were run in one mode. Evaluating the SAME
corpus twice — `(eval FORM t)` and `(eval FORM nil)` — separates the columns:

| | before | after |
|---|---|---|
| `lexical=t` | 45/45 | 45/45 |
| `lexical=nil` | **35/45** | 45/45 |

Ten of the 45 forms have a mode-dependent answer in Emacs, and those ten were
*exactly* the ten failures: the engine answered every form as though it were
lexical, so the `t` column could never fail and matching it proved nothing.

There is now a dynamic closure kind. `Obj::Closure` carries a `dynamic` flag
(`host.rs`); `ElispHost::dynamic_binding` is a dynamic-extent mode that `eval`
installs from its LEXICAL argument and `run_closure` re-installs for the duration
of every call, so the mode travels with the function rather than with the `eval`
that made it. Under it, `bind_here` puts *every* symbol on the specstack and
`instantiate_closure` captures nothing.

- `(eval '(funcall (let ((x 1)) (lambda () x))) nil)` → `(void-variable x)`; with
  `t` it is `1`. LEXICAL omitted defaults to nil, as in Emacs.
- `(eval '(let (fs) (dotimes (i 3) (push (lambda () i) fs)) …) nil)` →
  `(void-variable i)`.
- A dynamic function prints a **`nil`** environment where a lexical one with
  nothing captured prints `(t)`: `(eval '(let ((x 1)) (lambda (y) x)) nil)` is
  `#[(y) (x) nil]`. That is the only way to tell the two apart from Lisp, so the
  printer distinguishes them.
- Parameters bind dynamically, so a callee sees them:
  `(progn (defun g () x) (funcall (eval '(lambda (x) (g)) nil) 7))` → `7`, while
  the same with `t` is `(void-variable x)`.

The default stays lexical — elisprs's own prelude and every file it loads are
lexical, and nothing but `eval` with a nil LEXICAL enters the mode.

### R8-B. The AOT runtime installed no numeric contract at all — ✅ FIXED

`fusevm_aot_register_builtins` (`src/aot_runtime.rs`) set the extension handlers
and nothing else — no `set_numeric_hook`, no `set_fixnum_range` — while
`run_chunk` sets both on the interpreted VM. An AOT-compiled program therefore
ran fusevm's native `i64` ops with no promotion path. Reproduced end-to-end
through elisprs's own `--aot` on real arithmetic at `most-positive-fixnum`
(same source, three engines):

| form | emacs 30.2 | interpreted | AOT (before) |
|---|---|---|---|
| `(+ most-positive-fixnum 1)` | `2305843009213693952` | same | same |
| `(bignump (+ most-positive-fixnum 1))` | `t` | `t` | **`nil`** |
| `(fixnump (+ most-positive-fixnum 1))` | `nil` | `nil` | **`t`** |
| `(* 4611686018427387903 4611686018427387903)` | `21267647932558653957237540927630737409` | same | **`0.0`** |
| `(+ 9223372036854775807 1)` | `9223372036854775808` | same | **`1.0`** |
| `(* 1000000000000 1000000000000)` | `1000000000000000000000000` | same | **`2003764205206896640`** |

The `bignump`/`fixnump` rows are the sharp ones: no `i64` overflow is involved at
all, so they fail on the missing *range* alone. After the fix the AOT binary
matches Emacs on every row, and on a second 13-form probe covering the 2^31
boundary and hot loops (where a trace is compiled). `tests/aot_numeric_contract.rs`
pins it by driving the registration hook directly, so it needs no `cc`.

Two notes, because they bound the claim:

- The fix is entirely inside elisprs. fusevm is a shared VM with many dependents
  and was not touched.
- `elisp --aot-exe` still fails to link on macOS: the `sysinfo` dependency needs
  `-framework IOKit`, which `aot.rs`'s link line omits. The measurements above
  were taken by linking the `--aot` object by hand with that flag added. Not
  fixed here, and unrelated to the numeric contract.

### R8-C. Read-modify-write places evaluated their subforms more than once — ✅ FIXED

Round 7 recorded the `setf` gv gap as "shape, not semantics", asserting the two
"agree on every value and on evaluation count". That is true of `setf` itself and
false of every macro built on it. `push`/`pop`/`incf`/`decf`/`cl-incf`/`cl-decf`/
`cl-callf`/`cl-callf2`/`cl-pushnew` mention the place two or three times, and
substituting the place FORM at each mention re-ran its subforms:

- `(let ((n 0) (l (list 1 2))) (cl-incf (car (progn (setq n (1+ n)) l))) n)`
  → Emacs `1`, elisprs `2`
- `(let ((n 0) (l (list 1 2 3))) (pop (cdr (progn (setq n (1+ n)) l))) n)`
  → Emacs `1`, elisprs `3`

A wrong number of side effects, not a wrong printed expansion. All nine macros
now go through one helper (`setf--place-once`) that rebinds each non-copyable
subform to a temporary, mirroring what `gv-letplace` does. A *symbol* subform is
deliberately left alone, exactly as gv.el does: re-reading a variable has no
side effect, and some setters assign to the subform itself — `(setf (nthcdr 0 l) v)`
is `(setq l v)` — which a temporary would redirect away from the caller.

`setf`'s own printed expansion still differs from Emacs's (`(progn (setcar x 1))`
vs `(let* ((v x)) (setcar v 1))`); that part really is shape, and rewriting
`setf--expand` onto `gv-get` is left for its own change.

### R8-D. Deep recursion aborted the process — ✅ FIXED

`(cl-labels ((go (n acc) …)) (go 100 0))` died with `fatal runtime error: stack
overflow`. Two independent causes, both now addressed:

- **No limit existed.** Emacs's eval.c raises `excessive-lisp-nesting` *before*
  the native stack runs out, so runaway recursion is catchable. `run_closure`
  now counts frames against `max-lisp-eval-depth` (defvar'd at 1600, clamped up
  to a floor of 100 exactly as eval.c does), and
  `(condition-case e (letrec ((f (lambda () (funcall f)))) (funcall f)) (error e))`
  answers `(excessive-lisp-nesting 1601)` — Emacs's value.
- **The stack was the platform default.** Each elisp frame costs several native
  frames. Everything now runs on a thread with `elisprs::INTERP_STACK_BYTES`
  (512 MiB of *address space*; pages are backed only as touched). Depth went from
  dying by 80 to completing at 3000. The constant lives in `lib.rs` so the binary
  and the tests share one definition — a cargo test thread's default 2 MiB is
  smaller than what the old ceiling had.

### R8-E. The eleven generator divergences — ✅ ALL FIXED

`scripts/fuzz_parity.sh -n 1500 -s 1`: **11/1500 → 1500/1500**. Four causes:

- **`error-message-string` is now print.c's `print_error_message`, ported
  statement for statement** (4 of the 11). Every corner is observable: the
  `error` symbol is special-cased *before* any property lookup, so
  `(error-message-string '(error 1 2))` is `"peculiar error: 2"`; every other
  ERRNAME goes through `get`, whose `CHECK_SYMBOL` is what makes
  `(error-message-string '(1 2))` signal `(wrong-type-argument symbolp 1)`; the
  data walk is `cdr-safe` + "while CONSP", so a dotted tail is dropped rather
  than signalled; an EMPTY message suppresses the separator
  (`'(error "" 1 2)` → `"1, 2"`); and `file-error`, `end-of-file` and
  `user-error` print their data with `princ`, which is the whole difference
  between `(user-error "a" "b")` → `"a, b"` and a *child* of user-error with the
  same data → `"\"a\", \"b\""`. Along the way `get`/`put`/`symbol-plist`/
  `setplist` gained the `CHECK_SYMBOL` they were missing, `function-get` became
  subr.el's symbolp-guarded loop, and the twenty standard error symbols
  data.c/fileio.c seeds (the whole `file-error` family, `range-error`,
  `overflow-error`, `no-catch`, …) are registered, so `condition-case` can
  dispatch a `file-error` handler onto `file-missing`.
- **`compare-strings` now runs fns.c's `validate_subarray`** (3 of the 11). A
  non-integer START/END is `(wrong-type-argument integerp BOUND)`; elisprs
  silently defaulted it and answered a comparison. Negative bounds count from the
  end, an out-of-range pair is `args_out_of_range_3`, and a too-large positive
  END is clamped first, "for backward compatibility", exactly as in C.
- **`cl-gcd`/`cl-lcm`/`cl-floor`/`cl-ceiling`/`cl-truncate`/`cl-round`/`cl-mod`/
  `cl-rem`/`cl-adjoin` are cl-extra.el's and cl-lib.el's definitions verbatim**
  (4 of the 11). The exact expressions matter, not just the values: `cl-gcd`'s
  `(/= b 0)` tests B before touching A, so `(cl-gcd 100 'car)` names `car` under
  `number-or-marker-p`; `(% a (setq a b))` reads the OLD `a`, so
  `(cl-lcm "" 65535)` reports `integer-or-marker-p`; `cl-truncate` tests
  `(>= y 0)` before dividing, so `(cl-rem 1.0e+INF '(1 2))` names the list under
  `number-or-marker-p`; and `cl-adjoin`'s no-keys path is literally `memq`, so
  `(cl-adjoin "" t)` is `(wrong-type-argument listp t)` where a `seq-some`
  paraphrase said `sequencep`.
- The eleventh was `seq-position`, which was `compare-strings` underneath.

### R8-F. `sort` with a nil PREDICATE — ✅ FIXED

The one divergence the *old* generator still produced. Emacs 30 documents
PREDICATE as defaulting to `value<`, and passing nil takes that default;
elisprs funcalled it. `(sort '(3 1 2) nil)` → `(1 2 3)`, `:lessp nil` likewise,
and `(sort '(t "010") nil)` → `(type-mismatch "010" t)`. The Rust default-order
helper now delegates its non-numeric/non-string cases to the prelude's `value<`
rather than carrying a second, weaker copy of the same order.

### R8-G. `type-of` of an intrinsic's function cell — ✅ FIXED

`when`/`unless` are lowered by name in `compiler.rs` and own no function cell, so
`(type-of (symbol-function 'when))` answered `symbol` where Emacs answers `cons`.
They now register the `(macro . FUNCTION)` pair subr.el defines, consulted by
`symbol-function` / `fboundp` / `indirect-function` only. It is deliberately kept
OUT of the real function cell: putting it there would make `resolve_function` —
and therefore `macroexpand_1` on the compile path — treat every `when` in the
prelude as a macro call to run, losing the `compile_when` lowering.

### Still open after round 8

- **Reader-level backquote preservation** — unchanged from round 7, and for the
  same reason: `pcase`'s backquote patterns are specified against the reader's
  eager expansion, so moving expansion into a macro changes what every
  quoted-backquote datum in the prelude looks like. Too broad to land beside this
  round's work.
- **`setf`'s own gv expansion shape** — see R8-C. The *semantic* half is fixed;
  the printed expansion still differs.
- **`hash-table-size`** stays void — Emacs reports allocated capacity, which
  elisprs's `Vec` has no analogue for. Answering would mean inventing a number.
- **`print-circle` nil's ` . #N` marker** — the Brent-style schedule
  (0, 2, 2, 6, 6, 6, 6 for periods 1–7) could not be derived from behavior and no
  `print.c` is vendored to port it from. elisprs terminates with a named
  divergence instead of hanging, which is the right trade.
- **`aset` on a string** is `(wrong-type-argument arrayp …)`; Emacs mutates in
  place (`(let ((s (copy-sequence "abc"))) (aset s 1 ?z) s)` → `"azc"`). elisprs
  strings are `fusevm::Value::Str(Arc<String>)` — an immutable value, not a heap
  object with identity — so in-place mutation would only affect the one `Value`
  that happened to be aset, not every holder. Same constraint `fillarray` already
  records. Substrate work. The error data was fixed to name the offender.
- **A warm script cache can re-intern a `make-symbol` result.** Running the fuzz
  corpus warms the cache for `drive.el`; a later run of the *same* driver then
  finds `(intern-soft "a,b,,c")` non-nil, because the corpus contains
  `(make-symbol "a,b,,c")` and the heap image claims the name back on import.
  Pre-existing (the `interned` flag added in the round-1 cache fix covers the
  prelude's uninterned symbols but not one created at runtime by the script
  itself). It makes fuzz counts cache-state-dependent, so the numbers above were
  all taken with `ELISPRS_CACHE=0` and a cleared shard.
