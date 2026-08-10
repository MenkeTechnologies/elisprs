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
`?\N{U+41}` → 65 — match emacs 30.2. **The by-NAME form does not**, and the
"all match" claim above was wrong when it was written: `?\N{LATIN SMALL LETTER A}`
is 97 in Emacs and `(unsupported-character-name "LATIN SMALL LETTER A")` here,
because the Unicode name table is not carried. Re-measured in round 18. The
original report follows.
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
  is not a sequence (`vconcat`/`append` signal `sequencep`; `mapcar` signals here but
  NOT in Emacs, whose `mapcar1` dispatches on `VECTORLIKEP` and walks the record —
  see round 18). cl-defstruct builds records
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

---

## Round 9 — cycles in the printer, and the classes the fuzzer could not reach

Round 8 ended with `scripts/fuzz_parity.sh -n 1500 -s 1` at **1500/1500** and five
named items. Two of those five were already closed by round 8 itself and are
re-verified below; the `print-circle` nil marker is closed; and re-running the
*same* widened generator with **fresh seeds** found 25 divergences the seed-1
corpus never produced — 19 of which are closed here.

### Regression control

The point of the control is that the widened generator finds new SHAPES rather
than regressions, so the corpora from every generator generation are re-scored
before and after. Corpora regenerated at `-n 1500 -s 1` from
`scripts/fuzz/gen.el` as of `6e000a3dad` (pre-widening), `899a868908` (round 7's
widening) and `HEAD`:

| generator | before this round | after this round |
|---|---|---|
| `6e000a3dad` (old) | 1500/1500 | 1500/1500 |
| `899a868908` (round 7 widened) | 1500/1500 | 1500/1500 |
| `HEAD` (round 8 widened) | 1500/1500 | 1500/1500 |

Note for anyone reading round 8's numbers: the old generator's historical score
of **1/1500** was its state *before* round 8. R8-F closed that one divergence
(`sort` with a nil PREDICATE), so the old corpus has scored 0 since then. The
number to preserve going forward is **0**, not 1.

Fresh seeds on the round-8 generator, `-n 1500` each:

| seed | before | after |
|---|---|---|
| 2 | 8 | 2 |
| 7 | 2 | 1 |
| 42 | 4 | 1 |
| 2026 | 6 | 1 |
| 31337 | 5 | 1 |
| **total** | **25** | **6** |

A corpus at 100% means the current seeds are exhausted, not that the frontend is
correct. Every seed above is new work, on an unchanged grammar.

### R9-A. `print-circle` nil printed no ` . #N` marker — ✅ FIXED

Round 8 left this open because "the Brent-style schedule (0, 2, 2, 6, 6, 6, 6 for
periods 1–7) could not be derived from behavior and no `print.c` is vendored".
It is derivable — from the algorithm, not from the seven cases. Emacs 30.2's
`print.c` carries a Brent teleporting tortoise inside its `PE_list` continuation
(`tortoise`, step countdown `n`, step period `m`, `tortoise_idx`), and the marker
prints `tortoise_idx`: the tortoise's position at its LAST TELEPORT, not the
cycle's period. The teleport takes precedence over the equality test, so the
index can only ever be 2^k − 2 — which is what generates 0, 2, 2, 6, 6, 6, 6 for
periods 1–7 without anyone choosing those numbers.

`print_list` (`src/host.rs:2646`) is now that state machine. Fitting seven
constants would not have survived the check that mattered: the marker's index is
only half the output, and how far the list unrolls before it depends on the
leading non-cycle prefix as well as the period. Verified over a 6×13 grid of
(prefix 0,1,2,3,5,8) × (period 1..13) — 78 shapes, byte-identical:

```
rho=0  lam=1   (0 . #0)
rho=0  lam=4   (0 1 2 3 0 1 2 3 0 1 . #6)
rho=3  lam=1   (-3 -2 -1 0 0 0 0 . #6)
rho=8  lam=13  (-8 -7 -6 -5 -4 -3 -2 -1 0 1 2 3 4 5 6 7 8 9 10 11 12 0 1 2 3 4 5 . #14)
```

### R9-B. …and no `#N` back-reference either — ✅ FIXED

The same `NILP (Vprint_circle)` block in print.c has a second half nobody had
noticed was missing: `being_printed[]`, the chain of objects open at every
shallower depth, scanned with `BASE_EQ` to print `#I` — no dot, no trailing `#`.
That is what terminates a cycle closing through a CAR, or through a vector,
record or hash-table slot, rather than through the cdr chain. elisprs signalled
"Apparently circular structure being printed" for all of them.

`ElispHost::print_being` (`src/host.rs:497`), scanned in `print_inner`
(`src/host.rs:2388`).

| form | emacs 30.2 | elisprs before | after |
|---|---|---|---|
| `(let ((x (list 1))) (setcar x x) x)` | `(#0)` | *signalled* | `(#0)` |
| `(let ((v (vector 1 2))) (aset v 0 v) v)` | `[#0 2]` | *signalled* | `[#0 2]` |
| `(let ((r (record 'foo 1))) (aset r 1 r) r)` | `#s(foo #0)` | *signalled* | `#s(foo #0)` |
| `(let ((h (make-hash-table))) (puthash 'k h h) h)` | `#s(hash-table data (k #0))` | *signalled* | same |
| `(let ((a (list 1 2 3))) (setcar (nthcdr 2 a) a) a)` | `(1 2 #0)` | *signalled* | `(1 2 #0)` |

### R9-C. The `PRINT_CIRCLE` ceiling fired under `print-circle` t — ✅ FIXED

Both mechanisms above sit inside `if (NILP (Vprint_circle))`, and so does the
depth ceiling that raises "Apparently circular structure being printed". With the
label table on there is no ceiling: every container that can close a cycle
carries a `#N=` instead. elisprs applied it unconditionally, so a deep but
perfectly finite structure signalled where Emacs prints it.

- `(let ((print-circle t)) (prin1-to-string DEEP-250))` → Emacs prints
  `((((((((((((((((((((…`; elisprs signalled. Now prints.
- With `print-circle` nil the ceiling still applies and still signals, unchanged.

Termination under `print-circle` t therefore rests entirely on the candidate set,
so `print_preprocess` now covers every container the printer recurses into —
cons, vector, record, char-table, closure, hash-table — matching
`PRINT_CIRCLE_CANDIDATE_P`.

### R9-D. `print-level` truncated vectors and records — ✅ FIXED

print.c tests `Vprint_level` in exactly ONE place: `case Lisp_Cons`. Only a list
is ever replaced by `...`. A vector or record still costs a level (`print_depth++`
runs for every object) but can never be truncated itself. elisprs checked it in
the vector and record branches too.

| form (`print-level` 2) | emacs 30.2 | elisprs before |
|---|---|---|
| `[[[[1]]]]` | `[[[[1]]]]` | `[[...]]` |
| `#s(a #s(b #s(c #s(d 1))))` | unchanged | `#s(a #s(b ...))` |
| `(([[(1)]]))` | `(([[...]]))` | `((...))` |
| `(1 [2 (3 [4 (5)])])` (level 3) | `(1 [2 (3 [4 ...])])` | `(1 [2 (3 ...)])` |

### R9-E. `print-circle` numbered its labels in the wrong ORDER — ✅ FIXED

Found by a 400-trial randomized differential over circular/shared object graphs
(4 printer modes each), not by the form fuzzer. The `#N=` number is assigned in
print.c's `print_preprocess`, at the moment an object is met for the SECOND time
in a car-before-cdr DFS — **not** when it is finally printed. elisprs numbered on
first print. The two orders differ whenever the object printed first is
met-twice later than one printed after it:

- Emacs: `([#2=#s(r #s(r 0)) [#1=(9 . 9) #s(r 3)]] #1# (#2# 3))`
- elisprs before: `([#1=#s(r #s(r 0)) [#2=(9 . 9) #s(r 3)]] #2# (#1# 3))`

`print_preprocess` (`src/host.rs:2239`) is now print.c's explicit-stack DFS, and
`print_labels` carries `Vprint_number_table`'s own status encoding (`0` = `Qt`,
`-N` = assigned-not-printed, `N` = printed), read by `circle_label`
(`src/host.rs:2316`).

**Randomized differential, 400 trials × 4 modes (`print-circle` nil / t,
`print-length` 4, `print-level` 3) = 1600 lines per seed:**

| stage | divergent lines |
|---|---|
| before | 768 (all `print-level`) |
| after R9-D | 112 (all label ordering) |
| after R9-E | **0** |

Re-run on four further seeds (7, 4242, 99991, 31337): **0 / 6400**.

### R9-F. `nthcdr` rejected a bignum index and HUNG on a circular list — ✅ FIXED

The sharper half is the hang: `(nthcdr 4611686018427387903 CYCLE)` counted down
one cdr at a time and never returned. fns.c `Fnthcdr` handles both cases and is
now ported to Rust (`src/builtins.rs:747`); the elisp definition it replaces is
gone (`src/prelude.rs:95`), and `nth` is again literally `Fcar (Fnthcdr …)`.

- **Bignum N.** `CHECK_INTEGER` accepts one, so `(nth (floor 1.5e+300) '(a))` is
  `nil`, where elisprs answered `(wrong-type-argument integerp 15000000…0240)`.
  A negative bignum returns LIST untouched.
- **Circular LIST.** Brent's tortoise, then the remaining count reduced modulo
  the distance the hare travelled since the last teleport. That distance is always
  a multiple of the true period — both pointers sit on the same cell when they
  meet — which is why the answer does not depend on the tortoise's schedule, and
  is the reason a faithful port did not need C's exact two-level countdown.

`(let ((l (number-sequence 0 6))) (setcdr (nthcdr 6 l) l) (car (nthcdr 300 l)))`
→ `6`; at 301 → `0`; with a 3-cell prefix and a bignum index → `3`. All Emacs's.

### R9-G. `split-string` ignored TRIM entirely — ✅ FIXED

Three divergences in one function, now a port of subr.el's `split-string`
including its `push-one` closure (`src/builtins.rs:2397`):

- **TRIM was never applied and never type-checked.**
  `(split-string "abc" "b" nil 97)` must be `(wrong-type-argument stringp 97)`.
- **The default SEPARATORS were Rust's `split_whitespace`**, which is
  Unicode-aware; subr.el's are the six ASCII characters in
  `split-string-default-separators`, which was also simply missing as a variable
  (`src/prelude.rs:29`). `(split-string "a\u{a0}b")` is ONE element in Emacs.
- **Reproducing the index walk reproduces its sharp edge:** a leading TRIM whose
  match runs past the end of the segment leaves `this-start > this-end`, and
  `substring` then signals — `(split-string "aXb" "X" nil "a.")` is
  `(args-out-of-range "aXb" 2 1)`, not `("a" "b")`.

### R9-H. Emacs's `[:class:]` names stopped at ASCII — ✅ MOSTLY FIXED

`(string-match "[[:alpha:]]" "Ü")` was nil where Emacs answers 0: the `regex`
crate's POSIX classes are ASCII-only and were being copied through verbatim.
Each class is now re-expressed as the Unicode property the Elisp manual's "Char
Classes" node names (`posix_class`, `src/regexp.rs:275`).

Measured over 72 codepoints × 17 classes × both `case-fold-search` settings
(2448 probes) against Emacs 30.2:

| class | before | after | | class | before | after |
|---|---|---|---|---|---|---|
| alpha | 59 | **0** | | punct | 62 | 28 |
| alnum | 61 | **0** | | word | 81 | 18 |
| cntrl | 2 | **0** | | space | 18 | 8 |
| blank | 10 | **0** | | upper | 24 | 3 |
| ascii | 1 | 1 | | lower | 26 | 4 |
| nonascii | 161 | 1 | | graph | 131 | 131 |
| multibyte | 168 | 1 | | print | 141 | 141 |
| unibyte | 233 | 1 | | **total** | **1064** | **337** |

What is closed is closed exactly; what is left is left for a stated reason, below.

### R9-I. Re-verified as already fixed in round 8

The task brief listed these as open; they were closed by round 8 and are
confirmed against Emacs 30.2 here rather than taken on trust.

- **Deep recursion.** `(cl-labels ((go (n acc) …)) (go 100 0))` → `5050` in both;
  at 3000 under a raised `max-lisp-eval-depth` → `4501500` in both; and
  `(condition-case e (letrec ((f (lambda () (funcall f)))) (funcall f)) (error e))`
  → `(excessive-lisp-nesting 1601)` in both. No abort.
- **`(type-of (symbol-function 'when))`** → `cons` in both, likewise `unless`.

### Still open after round 9

- ~~**`string-version-lessp`'s ORDERING**~~ — CLOSED by round 10's `filevercmp`
  port; this list was never updated. Re-measured in round 18:
  `(string-version-lessp "quote\"d" "  padded  ")` is `t` in both engines.
- **`[:graph:]` and `[:print:]`.** The manual defines both by COMPLEMENT ("any
  character except whitespace, control characters, surrogates, and unassigned
  codepoints"), and a complement is not expressible as character-class BODY text.
  `[[^…]…]` needs nested classes, which fancy-regex's parser rejects outright
  ("error parsing pattern"). Reaching them means translating the whole `[...]`
  alternative rather than one member of it.
- **`[:space:]`, `[:punct:]`, `[:word:]`.** Improved but still approximations, and
  they cannot be finished with a Unicode property at all: in Emacs these three
  read the SYNTAX TABLE, so the answer depends on the major mode —
  `(string-match "[[:space:]]" "\n")` is nil in fundamental-mode and 0 in
  text-mode. elisprs has no syntax table to consult.
- **`[:upper:]`/`[:lower:]`, and the four byte-width classes under
  `case-fold-search`.** The residue is Emacs's CASE TABLES: `\p{Uppercase}`
  accepts `U+1D400` (MATHEMATICAL BOLD CAPITAL A) where Emacs does not, because
  Emacs's test is "downcasing changes the character". The one fold-only mismatch
  in `[:ascii:]` and friends is the same root: `compile_cf` applies `(?i)` to the
  whole pattern, so the crate folds `U+017F` (ſ) into an ASCII class and Emacs
  does not.
- **`seq-mapn`'s argument-error order** — `(seq-mapn #'string-to-number "-4.5"
  (cons 2 10))` is `(wrong-type-argument stringp 45)` in Emacs and
  `(wrong-type-argument listp 10)` here: the two walk their sequences in a
  different order before the first bad element is reached.
- **`cl-rem` / `cl-floor` at the float boundary** — `(cl-rem 1.5e+300 (mod 1e-10 …))`
  is `-1.0e+INF` in Emacs and `(overflow-error)` here; `(cl-floor 4611686018427387903 0.5)`
  is `(9223372036854775806 0.0)` vs `(9223372036854775807 0.0)`. Both are one
  rounding step inside the `cl-truncate` chain, not a contract difference.
- **A closure prints the environment it captured, and Emacs prints `(t)`** —
  `(dotimes (i 3) … (lambda (x) (list x x)))` reports
  `#[(x) ((list x x)) ((i . 0) (counter . 0) (upper-bound . 3))]` where Emacs says
  `#[(x) ((list x x)) (t)]`. Two things at once: Emacs captures nothing here, and
  elisprs's `dotimes` leaks its OWN internal variable names (`counter`,
  `upper-bound`) into the visible environment. Closing it means pruning a
  closure's captured environment to the free variables of its body — substrate
  work in `compiler.rs`, not a prelude change.
- Unchanged from round 8, for the reasons already recorded there:
  reader-level backquote preservation, `setf`'s own gv expansion SHAPE,
  `hash-table-size`, `aset` on a string, and the warm-cache `make-symbol`
  re-intern.

---

## Round 10 — exact division with a divisor, argument order, filevercmp, non-finite floats

Every fix in this round was verified by byte-diffing stdout, stderr and exit
status against `emacs -Q --batch` 30.2 on the interpreter, with the bytecode
cache disabled, on a warm rkyv cache, and — for everything except the
non-finite-float items — on an AOT-compiled native executable.

- **`floor`/`ceiling`/`round`/`truncate` with a DIVISOR saturated to `i64`** —
  ✅ FIXED. `(floor 1.0e30 3)` answered `9223372036854775807`; Emacs says
  `333333333333333339961541612885`. With a float on either side the quotient
  went through `apply_rm(x / y) as i64`. Emacs divides *exactly* (every finite
  float is a dyadic rational), so rounding the `f64` quotient is not enough
  either — that yields `333333333333333316505293553664`. Both operands now
  convert to an exact fraction and reuse the bignum rounding division. The
  one-argument forms were already correct, which is what kept this quiet.
- **`+`/`-`/`*` with 3+ arguments skipped later arguments' side effects** —
  ✅ FIXED. They lowered to a chain of binary opcodes that folded as it
  evaluated, so `(let ((n 0)) (ignore-errors (* 1 t (setq n 9))) n)` left `n` at
  `0` where Emacs leaves `9`, and `(* 1 t (error "boom"))` reported
  `(wrong-type-argument number-or-marker-p t)` instead of `(error "boom")`.
- **A one-argument `+`/`*` skipped its type check** — ✅ FIXED. `(+ t)` answered
  `t`, `(+ "a")` answered `"a"`. The lone operand was emitted bare. It now goes
  through the n-ary builtin, which seeds its accumulator from the first argument
  rather than the identity, so the check happens *and* `(+ -0.0)` stays `-0.0`.
- **`string-version-lessp` was a byte comparison** — ✅ FIXED. It is gnulib
  `filevercmp`: `order()` sorts `~` first, then digits, then letters, then every
  other byte *after* the letters; `.`/`..`/leading-dot names are special-cased
  and file suffixes are cut before the first pass. `(string-version-lessp "a" " ")`
  was `nil` (Emacs: `t`), `(string-version-lessp "." "9")` was `nil` (Emacs: `t`).
  A 4,000-pair differential corpus went from **1,985 divergences to 0**.
- **`string-to-number` rejected the non-finite float syntax** — ✅ FIXED.
  `(string-to-number "1.0e+INF")` silently answered the finite `1.0`, so any
  float round-tripped through `number-to-string` lost its infinity.
- **NaN payloads were discarded by the reader and the printer** — ✅ FIXED.
  Emacs stores a token's leading integer in the NaN's significand and prints it
  back, so `3.7e+NaN` reads and prints as `3.0e+NaN`; everything collapsed to
  `0.0e+NaN`. The reader also matched only a lowercase `e`, so `1.0E+INF` read
  back as a *symbol*.
- **`/` and `mod` flattened the sign of a NaN operand** — ✅ FIXED. Both
  unconditionally `abs()`ed a NaN result to hide the hardware-dependent sign of a
  NaN the operation *invents* (`(/ 0.0 0.0)`). That also destroyed the sign of a
  NaN merely passing through, which IEEE propagates unchanged on every ISA:
  `(/ -0.0e+NaN -1)` is `-0.0e+NaN` in Emacs. Only invented NaNs are
  canonicalized now.
- **Short-circuiting cl-seq searches rejected improper lists** — ✅ FIXED.
  `cl-position`, `cl-find`, `cl-position-if` and `cl-find-if` normalized through
  `(append SEQ nil)`, which signals `listp` up front, so `(cl-position 1 (cons 1
  t))` errored where Emacs answers `0`. They walk the list in place now, which
  also preserves the signal for a search that really does run off the end
  (`(cl-find 9 (cons 1 t))`).
- **`--aot-exe` could not link on macOS** — ✅ FIXED. The link line omitted
  `-framework IOKit`, which `sysinfo` needs, so every standalone AOT build died
  with "symbol(s) not found for architecture arm64" and the native path could not
  be exercised at all.

## Round 11 — the AOT path exited 0 in silence, and the standard syntax table

### R11-A. Every uncaught elisp error vanished on the AOT path — ✅ FIXED

An elisp error never becomes a fusevm `VMResult::Error`. `host::abort`
(`src/host.rs`) parks the message in the thread-local host error slot and winds
`vm.ip` past the last op, so the VM terminates exactly the way a program that ran
off the end does. `host::run_chunk` copes by reading that slot right after
`vm.run()`; fusevm's AOT driver (`fusevm_aot_run_embedded`) lives in fusevm,
cannot know about the slot, and mapped the clean termination to **exit 0**.

```text
$ cat r.el
(error "boom")
$ emacs --batch -l r.el ; echo "exit=$?"     # ground truth
boom
exit=255
$ elisp r.el ; echo "exit=$?"                # interpreter
error: boom
exit=1
$ elisp --aot-exe r.el -o r.bin && ./r.bin ; echo "exit=$?"
exit=0                                       # 0 bytes stdout, 0 bytes stderr
```

Exit 0 with empty stdout *and* empty stderr is byte-identical to a successful run
of a program that prints nothing, so no caller could detect the failure. It hit
every uncaught error — `(error …)`, `void-function`, `arith-error` — not some
narrow corner.

`elisprs_aot_finish` (`src/aot_runtime.rs`) now reads the slot after the run and
reports it with the interpreted driver's exact message and status, so both paths
fail identically (`error: boom`, exit 1). A *handled* error (`condition-case`,
`ignore-errors`) is consumed by the nested `run_chunk` and correctly leaves the
slot empty, so it still exits 0.

This is what the round-10 note called "an AOT executable whose constants are not
reconstructible exits 0 printing nothing". That diagnosis was wrong: the form it
named — `(princ (format "a %s\n" (string-to-number (number-to-string
-1.0e+INF))))` — builds, runs and prints `a -1.0e+INF` correctly on the
*unmodified* pre-round-11 tree. The silence was the error path, not constants.

### R11-B. A closure lost its printed source through the heap image — ✅ FIXED

There *was* a real reification gap, but a different and narrower one, and it was
not AOT-only. `SerObj::Closure` carried the compiled body chunk, the params and
the captured environment, but not `ClosureSrc` — the arglist as written and the
body forms — and `import_heap_image` rebuilt every closure with
`ClosureSrc::default()`. The closure still *ran*; it just printed empty.

That made it a silent wrong answer on the **default** path: the rkyv script cache
uses the same image, so every run of a script after the first was affected.

```text
;;; -*- lexical-binding: t -*-
(princ (prin1-to-string (lambda (x) (* x 2))))

  emacs --batch    #[(x) ((* x 2)) (t)]
  elisp, cold      #[(x) ((* x 2)) (t)]
  elisp, warm      #[nil () (t)]          <- source gone
  --aot-exe        #[nil () (t)]
```

`SerObj::Closure` now carries `arglist` + `src_body`, and
`cache::SHARD_FORMAT_VERSION` goes 5 -> 6 so caches already written to disk are
rejected by the header check rather than decoded. The bump is required: bincode
reads fields positionally and ignores `#[serde(default)]`, so a v5 closure decoded
with the v6 struct runs off the end of the object (measured: `io error: unexpected
end of file`), and one in the middle of an image can over-read a following object
and still produce a plausible, wrong heap. All four paths now print what Emacs
prints, byte for byte.

### R11-C. The standard syntax table classed the control characters wrong — ✅ FIXED

`--init-standard-syntax-table--` made C0 controls and DEL *punctuation* and made
`?\n` and `?\r` *whitespace*. Emacs 30.2 makes controls and DEL symbol
constituents, newline comment-end, and leaves CR a symbol constituent:

| char | Emacs 30.2 | was |
|---|---|---|
| 0–8, 11, 14–31, 127 | `95` (`_`) | `46` (`.`) |
| `?\n` (10) | `62` (`>`) | `32` (whitespace) |
| `?\r` (13) | `95` (`_`) | `32` (whitespace) |

`(char-syntax C)` now agrees with Emacs across 0–32, 127 and 160.

### R11-D. `\s-` matched a newline — ✅ FIXED

Downstream of R11-C, and independently wrong: `src/regexp.rs` translated `\s-` to
the `regex` crate's `\s`, which matches `\n`, `\r` and `\v`. Emacs reads the
syntax table, where none of those three are whitespace, so `\s-` silently matched
across line boundaries. It now translates to the standard table's whitespace set
`[\t\x0C\x{A0} ]` (and `\S-` to its complement). Verified against Emacs 30.2 for
`\n \r \v \t \f` space, U+00A0 and a letter, in both polarities.

### Re-verified as already fixed

- **`(assoc-string nil LIST)`** — both `nil` now.
- **A closure prints the environment it captured** — `(lambda (x) x)` is
  `#[(x) (x) (t)]` and `(let ((n 5)) (lambda () n))` is `#[nil (n) ((n . 5))]`
  under both. The round-10 report compared a file *without* a
  `lexical-binding` cookie, where Emacs 30 uses dynamic binding.

### Still open after round 11

- ~~**An empty-name uninterned symbol prints over-escaped**~~ — CLOSED; this list
  was never updated. Re-measured in round 18:
  `(prin1-to-string (make-symbol ""))` is `"##"` in both engines.

## Round 12 — a subr's arity is checked before its arguments run

### R12-A. A fixed-arity subr's arity was checked after its arguments were evaluated — ✅ FIXED

Carried over from round 11. `compile_call` emitted the argument expressions ahead
of the `CALL` extension op and arity was enforced in `call_function`, by which
point every argument had already run:

| form | Emacs 30.2 | was |
|---|---|---|
| `(let ((n 0)) (ignore-errors (nth 1 t (setq n 9))) n)` | `0` | `9` |
| `(let ((x 0) (y 0)) (ignore-errors (car (setq x 1) (setq y 2))) (list x y))` | `(0 0)` | `(1 2)` |
| `(let ((n 0)) (ignore-errors (cons (setq n 1))) n)` | `0` | `1` |
| `(let ((n 0)) (ignore-errors (point (setq n 3))) n)` | `0` | `3` |
| `(condition-case e (car (error "inner") 2) (error e))` | `(wrong-number-of-arguments car 2)` | `(error "inner")` |

That last row is the sharpest one: the arity error is raised so early that an
argument which would itself signal never gets the chance to.

**The rule is narrower than "arity is checked first", and round 11's prescription
for closing it was wrong on both halves.** Measured against `GNU Emacs 30.2`:

- It holds for **subrs only**. `eval_sub` reads `XSUBR (fun)->max_args` and
  signals straight from the argument-count switch; a closure instead reaches
  `funcall_lambda` only after its arguments have been evaluated into a vector. So
  with `(defun f1 (a) a)`, `(f1 (setq x 1) (setq y 2))` leaves `x` and `y` set to
  `1` and `2` — **in Emacs too**. A guard that fired for closures would be a new
  bug, not a fix.
- The verdict has to be taken at **run time**, not compile time. Round 11
  proposed emitting the check "only for the calls a static read can already see
  are wrong". That misses the cases where the callee is not statically visible at
  all, and those are real:

  | form | Emacs 30.2 |
  |---|---|
  | `(let ((x 0) (y 0)) (fset 'myfn (symbol-function 'car)) (ignore-errors (myfn (setq x 1) (setq y 2))) (list x y))` | `(0 0)` |
  | `(progn (fset 'myfn (symbol-function 'car)) (condition-case e (myfn 1 2) (error e)))` | `(wrong-number-of-arguments myfn 2)` |
  | `(let ((x 0) (y 0)) (defalias 'mycar 'car) (ignore-errors (mycar (setq x 1) (setq y 2))) (list x y))` | `(0 0)` |

  `myfn` and `mycar` name nothing when the call compiles. Note also that the
  error names the symbol **as the caller wrote it** — `myfn`, not `car` — which
  elisprs already got right.

  The redefinition has to keep working in the other direction as well: pointing
  `car` at a two-argument lambda makes `(car 1 2)` legal, and Emacs then
  evaluates both arguments —
  `(let ((x 0) (y 0)) (fset 'car (lambda (a b) (list 'mycar a b))) (let ((r (car (setq x 1) (setq y 2)))) (list r x y)))`
  is `((mycar 1 2) 1 2)`.

**The fix.** A `CHECK_ARITY` extension op (`ops::CHECK_ARITY`, id 11) is emitted
ahead of the argument code; it pops the head symbol, resolves the live function
cell through the same alias chain `resolve_function` walks, and signals only when
that cell holds a subr whose arity rejects the count. A closure, a macro or an
empty cell falls straight through, so the argument code runs exactly as before.
Because the resolution happens in the op and not in the compiler, `fset` and
`defalias` between compile and call are honoured either way.

`ElispHost::fn_kind` → `FnKind` is the shared probe, used by the compiler and the
op so the two can never disagree about what counts as a subr. It is deliberately
cheaper than `resolve_function`: it clones no closure body, parameter list or
captured environment on the path it exists to wave through.

The compiler decides only whether emitting the op can ever pay, which keeps it
off the two shapes that make up almost every call: a symbol that already names a
subr accepting the count, and a symbol that already names a closure. It is
emitted when the call is already wrong against the live cell, when the symbol
names nothing yet, and when `fset` has already pointed the symbol at a subr once
(see R12-B). Verified from `--dump-bytecode`: a loop over `cons`, `car`, `1+` and
`<` emits extension ops `{0, 1, 2, 3}` and no `11` at all.

An AOP intercept fronts the callee, so the underlying subr's arity is not the
arity being called; the op leaves those to `CALL`.

`cache::SHARD_FORMAT_VERSION` 6 → 7. No serialized struct changed shape, which is
exactly why the bump was needed: a v6 shard still decodes cleanly, but a v6 chunk
carries no guard, so a warm cache would have gone on serving the old behaviour.
Measured — the pre-fix binary warms the cache under schema key
`0.1.8-4442f3f3fc…` and prints `x=1 y=2`; the new binary reads that same file,
rejects the shard at `header_ok` (which compares `format_version`), recompiles,
and prints `x=0 y=0`. The schema key is identical across the two, so it would not
have invalidated anything on its own.

All four paths check out — interpreter cold, cache-warm, `--aot-exe` with the
error caught, and `--aot-exe` with it uncaught (which still reports and exits 1
rather than the silent 0 that path used to give) — each byte-identical to
`emacs --batch`.

Covered by `tests/parity_subr_arity_before_args.rs` (12 tests, two of them added
by R12-B). Seven of its cases fail against the pre-fix binary; the rest are the
regression guards for closures, `fset` widening and `funcall`, which passed
before and still do.

### R12-B. The generator could not express the bug — harness widened, then a second gap fell out

Worth recording on its own, because the clean number was meaningless.
`scripts/fuzz/gen.el` emitted **no wrong-arity calls at all** — `grep -c` for one
was `0`, and every `setq` in the file was the PRNG's own plumbing or a loop
counter. Round 11's `1500/1500` therefore said nothing about R12-A: *when* the
arity is checked is invisible in both the value and the signalled error, which
already agreed, so the only observable is a side effect in an argument, and no
generated form had one.

`fz-arity-form` now emits `(let ((n 0)) (ignore-errors (SUBR … (setq n 9))) n)`
in four flavours — the subr called directly, through `defalias`, through `fset`,
and a closure control that must still return `9`. It is reached from `fz-expr`
(6%), not from `fz-control`, where it was buried behind enough preceding clauses
to land once in 1500. The indirections always go on `fzwa`, never on a real subr:
`(fset 'car …)` would leak into every later form in the corpus.

Reach on seed 1 / 1500 forms: 80 arity forms — 33 direct, 14 `defalias`, 7
`fset`, 26 closure controls — and the same corpus scores **44/1500 divergences
against the pre-fix binary**.

Against the R12-A fix it still reported **7/1500**, all one shape:

```elisp
(let ((n 0)) (defalias 'fzwa 'cons)          ; an earlier form; fzwa := a 2-arg subr
  …)
(let ((n 0)) (defalias 'fzwa 'cdr)           ; this form retargets it narrower
  (ignore-errors (fzwa 1 (setq n 9))) n)     ; Emacs 0, was 9
```

When that call compiled, `fzwa` still named `cons`, which accepts two arguments,
so no guard was emitted; the `defalias` narrowing it to `cdr` runs inside the
same form, after compilation. The unit tests missed it because `reset_host` gives
each one a fresh host where `fzwa` names nothing.

Closed by `ElispHost::subr_aliased`: the `fset` subr records any symbol it points
at a subr, and calls to a recorded symbol always carry the guard. Only the `fset`
subr records — `defun` lowers to the `FSET` op and `defsubr` installs the
builtins, so neither `car` nor an ordinary user function ever enters the set, and
the hot path stays guard-free. Retargeting a recorded symbol back at a *lambda*
correctly goes back to evaluating the arguments:
`(progn (fset 'fzwa (symbol-function 'car)) (let ((n 0)) (fset 'fzwa (lambda (a b) (list a b))) (list (fzwa (setq n 9) 2) n)))`
is `((9 2) 9)` under both.

The widened corpus is now **1500/1500**.

### Still open after round 12

- **A void function's arguments are still evaluated** — the same
  resolve-before-evaluate ordering, one step earlier in `eval_sub`:
  `(let ((x 0)) (ignore-errors (nosuchfn (setq x 1))) x)` is `0` in Emacs and `1`
  here. Both engines signal `(void-function nosuchfn)`; only the argument's side
  effect differs. `CHECK_ARITY` deliberately does not cover it — an empty
  function cell also reaches the inline Rust FFI fallback in `call_function`, so
  signalling early here would have to be reconciled with that first.
- **A wrong-arity call to a *closure* names the symbol, not the closure** —
  `(progn (defun f1 (a) a) (condition-case e (f1 1 2) (error e)))` is
  `(wrong-number-of-arguments #[(a) (a) nil] 2)` in Emacs and
  `(wrong-number-of-arguments f1 2)` here. Only the first datum differs, and only
  for closures — the subr rows above are already right. Unrelated to the argument
  ordering fixed in R12-A; it is `call_function` passing the callee as written
  where Emacs passes the resolved closure.
- **Special forms and macros do not check arity at all** — `(eval '(setq zz) t)`
  and `(eval '(if) t)` are `(wrong-number-of-arguments setq 1)` and
  `(wrong-number-of-arguments if 0)` in Emacs, and both `nil` here;
  `(defmacro m1 (a) a)` then `(eval '(m1 1 2) t)` is
  `(wrong-number-of-arguments #[(a) (a) nil] 2)` in Emacs and
  `(error "wrong-number-of-arguments")` here. `compile_call` dispatches the
  special forms by name and never counts their operands.
- **A symbol that named a *closure* when it compiled, then retargeted by `fset`
  at a subr that rejects the count, still evaluates its arguments** — the one
  shape `CHECK_ARITY` is still not emitted for. R12-B closed the subr→subr half
  of this (a symbol that has ever named a subr is recorded and always guarded);
  the closure→subr half would need every call to a user function to carry the
  guard, which is most calls in most programs. Not reproduced by the fuzzer —
  `fz-arity-form` only installs subrs on `fzwa`.

---

## Round 14 — a guard against silent identifier collisions, and a `kill-buffer` panic

### The identifier-collision audit — clean, and now enforced

Two sibling fusevm frontends shipped a defect that nothing caught: `scalars` had
`MAKE_ORDERING` and `MAKE_QUEUE` both assigned 754, and `phplang` had
`INDEX_ISSET` colliding with `LIST_ELEM_GET` at 105. Both arrived through a
merge — two branches each appending a registration to a different part of a file
merge with no conflict marker, and the collision exists only in the result.

elisprs has two such namespaces:

| namespace | where | what a duplicate does |
| --- | --- | --- |
| extension-op IDs | `host::ops`, hand-assigned `u16` | the later `ext_dispatch` arm is dead code; `rustc` emits only an `unreachable_patterns` warning |
| subr names | `builtins::install` + `intercepts`, 340 registrations | `defsubr` ends in `set_function`, so the last registration silently replaces the first |

**Audit result: no collisions.** The 12 `ops::*` constants are `0..=11` with no
repeats, each dispatched by exactly one `match` arm, and no subr name is
registered twice.

`tests/registration_ids_unique.rs` now reads both sets out of the source text —
the only place they exist as a *set*, since `ops::*` are separate constants with
no registry and `defsubr` keeps no record of what it replaced — and fails on a
duplicate ID, a duplicate constant name, an op with anything other than one
dispatch arm, or a repeated subr name.

The guard was verified by reintroducing both collision shapes and reverting
them. With `CHECK_ARITY` moved from 11 to 9 and a second `s("length", …)`
appended:

```
test extension_op_ids_are_unique ... FAILED
test subr_names_are_registered_once ... FAILED

extension-op IDs collide; the later arm in ext_dispatch is dead code:
  id 9: ops::MAKE_CLOSURE (host.rs:132), ops::CHECK_ARITY (host.rs:134)

a subr is registered more than once; only the last registration survives:
  "length": builtins.rs:7100, builtins.rs:7243
```

Both are real breakage, and both are silent without the guard: `cargo build`
reported one `unreachable_patterns` warning for the op clash and *nothing* for
the subr clash, and the built binary failed at startup with
`prelude form failed: void-function: cadr` — a diagnostic that names neither
`length` nor the duplicate registration.

## Round 13 — the propertized-string read syntax, cl-seq's `*-if` contract, and nil's two spellings

Sources this round: `scripts/fuzz_parity.sh` at two fresh seeds (777 × 3,000
forms and 424242 × 5,000 forms, depth 4–5), hand-built probe corpora, and a
*self-consistency* sweep — call every bound function once with a literal `nil`
and once with `(= 5 42)` and report any pair of answers that differs. The last
one needs no reference Emacs at all and found the widest class below.

- **`#("TEXT" START END PLIST …)` read as a bare string** — ✅ FIXED
  (`src/reader.rs:170`, `src/reader.rs:573`). The reader consumed the intervals
  and threw them away, so `(read "#(\"foo\" 0 3 (a 1))")` answered `"foo"` where
  Emacs answers `#("foo" 0 3 (a 1))`, and `(get-text-property 0 'a …)` was `nil`.
  It is lread.c's `#(` arm now: read the string, then read triples until `)`,
  applying each with `Fset_text_properties`. That brings textprop.c's contract
  with it — `validate_interval_range` swaps an inverted range before the bounds
  check and reports the *swapped* pair (`#("foo" 2 1 (a 1))` → `#("foo" 1 2 (a
  1))`, `#("foo" -1 3 …)` → `(args-out-of-range -1 3)`), a bound that is not an
  integer or marker is `(wrong-type-argument integer-or-marker-p 3.5)`, and
  `validate_plist` signals `(error "Odd length text property list")` for an
  odd-length list and wraps a non-list as the single pair `(PLIST nil)`
  (`#("foo" 0 3 5)` → `#("foo" 0 3 (5 nil))`). A later triple *sets* rather than
  merges, so `#("foo" 0 3 (a 1) 0 1 (b 2))` reads as
  `#("foo" 0 1 (b 2) 1 3 (a 1))`. Both diagnostics are lread.c's own text:
  a non-string first element (`#()`, `#(1 2 3)`) is `(invalid-read-syntax "#")`
  and a trailing group of one or two elements is
  `(invalid-read-syntax "Invalid string property list")`.
- **Error DATA lost a string's text properties** — ✅ FIXED, as a consequence of
  the above. `make_error_object` (`src/host.rs:3617`) rebuilds a condition's DATA
  by *re-reading* the rendered message, so a reader that could not read `#(…)`
  produced data whose offending value had been stripped:
  `(car (propertize "foo" 'a 1))` was `(wrong-type-argument listp "foo")` against
  Emacs's `(wrong-type-argument listp #("foo" 0 3 (a 1)))`. Same for
  `(elt (propertize "abc" 'a 1) 9)` and `(aref (vector (propertize "foo" 'a 1)) 5)`.
  This is what the fuzzer surfaced at seed 777; the reader was the root.
- **A nil PREDICATE crashed every cl-seq `*-if` function** — ✅ FIXED
  (`src/prelude.rs:460` `cl--if-test`, and its 16 call sites). cl-seq.el has no
  `*-if` loop of its own: `cl-find-if` is `(apply #'cl-find nil SEQ :if PRED
  KEYS)`, and `cl--check-test-nokey`'s cond only calls `cl-if` when it is
  non-nil, falling through to `(eql ITEM X)` — with the nil ITEM the wrapper
  passed — when it is not. So a nil predicate matches the *nil elements*:
  `(cl-position-if nil '(1 nil 2))` is `1`, `(cl-count-if nil '(1 nil 2 nil))` is
  `2`, `(cl-member-if nil '(1 nil 2))` is `(nil 2)`. elisprs funcalled it and
  answered `(void-function nil)` in all of `cl-position-if`, `cl-find-if`,
  `cl-count-if`, `cl-member-if`, `cl-assoc-if`, `cl-rassoc-if`,
  `cl-substitute-if` and every `-if-not` sibling. `:if-not` takes the same route
  (`cl--parsing-keywords` moves its value into `cl-if`, so a nil there also
  leaves both unset) — which is why `(cl-position-if-not nil '(1 nil 2))` is also
  `1` and not `0`. `cl-remove-if` already had the fallback, with a comment
  naming this exact rule; the rest did not.
- **`cl-subst-if` / `cl-subst-if-not` / `cl-nsubst-if` / `cl-nsubst-if-not` /
  `cl-nsublis` were void** — ✅ FIXED (`src/prelude.rs:597`). Added as cl-seq.el
  defines them: `cl-sublis` over the one-entry alist `((nil . NEW))` with the
  predicate as `:if` / `:if-not`. That is why a nil predicate replaces the list's
  own terminating nil — `(cl-subst-if 9 nil '(1 nil 2))` is `(1 9 2 . 9)` — and
  why a real predicate matches cons nodes too, so
  `(cl-subst-if-not 9 #'numberp '(1 nil 2))` collapses to `9`. `cl-sublis` now
  reads `:if` / `:if-not` alongside `:test` / `:test-not` / `:key`.
- **`nil` has two spellings on the VM and half the runtime knew one** — ✅ FIXED
  (`src/host.rs:3763` `el_nil`, `src/host.rs:982` `list_vec`,
  `src/host.rs:960` `sym_name`, `src/host.rs:1245`/`1271` the value cells,
  `src/builtins.rs:671` `length`, `src/builtins.rs:4896` `bare-symbol-p`). A
  literal `nil` compiles to fusevm `Undef`; every op that answers a boolean —
  the comparisons the compiler lowers `<`/`=`/`>` to — answers
  `Value::Bool(false)`. Both are elisp's one `nil`, and treating only `Undef` as
  the empty list made `(length (= 5 42))` signal
  `(wrong-type-argument sequencep nil)`: an error naming the very value it
  refused to recognise. Emacs answers `0`. The same gap hit `reverse`, `mapcar`,
  `mapc`, `mapcan`, `mapconcat`, `sort`, `apply`, `butlast`, `nbutlast`,
  `delete-dups`, `string-join`, `seq-empty-p`, `cl-list-length`, `copy-alist`,
  `symbol-name`, `symbol-value`, `default-toplevel-value`, `run-hooks`,
  `run-hook-with-args`, `bare-symbol-p` and `assoc-string` — the last of which is
  the round-10 open item "`(assoc-string nil LIST)` is `nil` in Emacs and
  `(wrong-type-argument symbolp nil)` here", whose real trigger was a *computed*
  nil, not a literal one. The self-consistency sweep now reports 0 functions
  whose answer depends on which spelling it is handed.
- **The syntax tables were each other's** — ✅ FIXED (`src/prelude.rs:7327`,
  `src/prelude.rs:7503`). Round 11's R11-C read `(char-syntax C)` in the batch
  `*scratch*` buffer and wrote those answers into `standard-syntax-table`. The
  two are different tables, and the differential says so:

  ```
  $ emacs -Q --batch --eval '(prin1 (list (char-syntax 1) (char-syntax ?\n) (char-syntax ?\r) (char-syntax 127) (char-syntax ?\;)))'
  (95 62 95 95 60)
  $ emacs -Q --batch --eval '(with-syntax-table (standard-syntax-table) (prin1 (list (char-syntax 1) (char-syntax ?\n) (char-syntax ?\r) (char-syntax 127) (char-syntax ?\;))))'
  (46 32 32 46 46)
  ```

  The first list is `lisp-interaction-mode`'s table — `emacs -Q --batch` starts
  in `*scratch*`, and `initial-major-mode` puts it in that mode. The second is
  syntax.c `init_syntax_once`'s, which is what `standard-syntax-table` must
  return. R11-C moved the first list's values into the second table: it took
  `(char-syntax C)` from 75 wrong characters to 16 and `standard-syntax-table`
  from 28 wrong to 31. The root cause is one table short, not one table wrong —
  `standard-syntax-table` is restored to syntax.c's, and the *initial buffer*
  now gets `emacs-lisp-mode-syntax-table`, which the prelude already built.
  Measured over all 256 characters of `(char-syntax C)` and of
  `(with-syntax-table (standard-syntax-table) (char-syntax C))`:

  | table | base | after round 11 | now |
  |---|---|---|---|
  | current buffer | 75 wrong | 16 wrong | **0** |
  | `standard-syntax-table` | 28 wrong | 31 wrong | **0** |

  R11-D's `\s-` result is unaffected — it is a fixed character set in
  `src/regexp.rs`, not a table read — and still matches Emacs for
  `\n \r \v \t \f`, space, U+00A0 and a letter in both polarities.
- **`standard-syntax-table` called the whole Latin-1 supplement word** — ✅ FIXED
  (`src/prelude.rs:7347`), and the 28 wrong characters in the table above are
  these. syntax.c defaults every character at or above U+0080 to word and
  lisp/international/characters.el's Latin-1 block then reclassifies the
  punctuation and the symbols, so `(char-syntax ?¡)` is `?.` (46) and
  `(char-syntax ?±)` is `?_` (95) in Emacs where both were `?w` (119) here. It is
  carried as data, not as a rule: the classification is not re-derivable from
  Unicode's general categories — U+00A5 YEN SIGN keeps word syntax while
  U+00A2–U+00A4 (the same `Sc` category) do not, and U+00B2/B3/B9 keep it while
  U+00BC–U+00BE (the same `No` category) do not.

### Still open after round 13

- **fusevm's block JIT coerces `t` in arithmetic** — SUBSTRATE, not fixable in
  elisprs. Once a block has been JIT-compiled, `(- t)` answers `-1`, `(+ t 1)`
  answers `2`, `(* t 2)` answers `2` and `(1+ t)` answers `2`, where Emacs
  signals `(wrong-type-argument number-or-marker-p t)` every time. It reproduces
  from the second evaluation of the same form:

  ```
  $ cat neg.el
  (princ (format "1: %S\n" (condition-case e (- t) (error e))))
  (princ (format "2: %S\n" (condition-case e (- t) (error e))))
  $ emacs -Q --batch -l neg.el
  1: (wrong-type-argument number-or-marker-p t)
  2: (wrong-type-argument number-or-marker-p t)
  $ elisp neg.el
  1: (wrong-type-argument number-or-marker-p t)
  2: -1
  ```

  The interpreter is correct (run 1); the JIT is not. The cause is in the
  vendored crate: `fusevm-0.17.0/src/vm.rs:1010` and `src/jit.rs:5971` classify
  `Value::Bool` as an `Int` slot (`*b as i64`) when they decide whether native
  code may run, so `slots_all_numeric` stays true for a frame holding `t` and
  the compiled trace does integer arithmetic on it. A `Value::Str` or a
  `Value::Obj` correctly blocks the trace, which is why `(- "a")` and `(- 'sym)`
  keep signalling. Strict numeric mode (a `NumericHook` installed) must not treat
  `Bool` as numeric; that is a one-line change in fusevm, and elisprs must not
  make it. The host-side alternatives are both unacceptable: stop lowering
  arithmetic to native ops (surrenders the JIT on the hot path) or stop
  representing `t` as `Value::Bool` (a 70-site change that also has to re-wrap
  every comparison result the VM produces).
- ~~**`kill-buffer` can leave point past the buffer end, and the next
  `buffer-substring` panics.**~~ — ✅ FIXED (R14-A). The repro was
  `(progn (insert "hello") (kill-buffer) (buffer-substring (point-min) (point)))`:
  `kill_buffer` cleared the slot's text but left `point`/`begv`/`zv` at 6, and
  when the killed buffer was the last live one its `.unwrap_or(0)` left that dead
  slot *current*, so `buffer_substring_core` sliced `text[..5]` of a zero-length
  vector and aborted the process. Two ports close it: the killed slot's positions
  go back to BEG (`reset_buffer`: `b->pt = BEG; b->begv = BEG; b->zv = BEG;`),
  and the successor follows `Fkill_buffer`'s `Fset_buffer (Fother_buffer (…))`,
  whose tail is `get-scratch-buffer-create` — "If no other buffer exists, return
  the buffer `*scratch*' (creating it if necessary)". A dead buffer can no longer
  be current.

  ```
  $ emacs -Q --batch --eval '(progn (insert "hello") (kill-buffer) (prin1 (list (point) (point-min) (point-max) (buffer-substring (point-min) (point)))))'
  (1 1 1 "")
  $ elisp -e '(progn (insert "hello") (kill-buffer) (prin1 (list (point) (point-min) (point-max) (buffer-substring (point-min) (point)))))'
  (1 1 1 "")
  ```

  (`(buffer-name)` is `"*Messages*"` in Emacs and `"*scratch*"` here — elisprs
  models neither `*Messages*` nor ` *Minibuf-0*`, so its `other-buffer` reaches
  the `*scratch*` fallback one step earlier. Recorded below.)
- ~~**`set-buffer` collapsed three distinct Emacs failures into one message.**~~
  — ✅ FIXED (R14-A), alongside the above, by porting `Fget_buffer` /
  `Fset_buffer` / `nsberror` (`src/buffer.c`). `Fget_buffer` returns a buffer
  object unchanged whether or not it is live and `CHECK_STRING`s anything else,
  so the three cases separate:

  | form | Emacs 30.2 | elisprs before |
  | --- | --- | --- |
  | `(get-buffer <killed>)` | `#<killed buffer>` | `nil` |
  | `(get-buffer 5)` | `(wrong-type-argument stringp 5)` | `nil` |
  | `(set-buffer 5)` | `(wrong-type-argument stringp 5)` | `(error "No buffer named 5")` |
  | `(set-buffer "nope")` | `(error "No buffer named nope")` | `(error "No buffer named \"nope\"")` |
  | `(set-buffer <killed>)` | `(error "Selecting deleted buffer")` | `(error "No buffer named #<killed buffer>")` |
- **`(seq-elt (cons 1 2) 5)`** is `(wrong-type-argument listp 2)` in Emacs and
  `(wrong-type-argument listp (1 . 2))` here — but *only* because Emacs's seq.el
  is byte-compiled and the `Belt` bytecode has an inline cons walk whose final
  `CAR` names the tail, while `Felt` (what interpreted code and elisprs both run)
  is `Fcar (Fnthcdr …)` and `nthcdr_impl` names the whole list. Interpreted
  Emacs agrees with elisprs: `(elt (cons 1 2) 5)` is
  `(wrong-type-argument listp (1 . 2))` on both. Same root as the arity item
  above — elisprs has no "this callee was byte-compiled" distinction.
- ~~**`\s_`, `\s<` and `\s>` do not read the syntax table.**~~ — ✅ FIXED
  (R14-C), and `\w`/`\W` with them. `ElispHost::syntax_class_ranges` answers
  "which characters are in class C" for the *current* table, and
  `regexp::translate_with` compiles `\sC`/`\SC`/`\w`/`\W` into an explicit
  character class from it. Answering for the whole character space at compile
  time is cheap because a `CharTable` stores runs, so the breakpoints of the
  table and of every table in its parent chain bound each place the answer can
  change — no 4M-character walk.

  ```
  $ emacs -Q --batch --eval '(prin1 (list (string-match "\\s_" "-") (string-match "\\s<" ";") (string-match "\\s>" "\n")))'
  (0 0 0)
  $ elisp -e '(prin1 (list (string-match "\\s_" "-") (string-match "\\s<" ";") (string-match "\\s>" "\n")))'
  (0 0 0)
  ```

  Two things fell out of it. `\w` was the regex crate's `[0-9A-Za-z_]` and is
  really `SYNTAX (c) == Sword` (`regex-emacs.c`), so `(string-match "\\w" "_")`
  answered `0` where Emacs answers `nil` — `_` is a symbol constituent under any
  lisp-mode table. And the emitted class needs `(?-i:…)`: `case-fold-search` is
  `t` by default, and its `(?i)` made `(progn (modify-syntax-entry ?a "_")
  (string-match "\\sw" "a"))` answer `0` by folding `a` onto the class's `A-Z`
  run, where Emacs answers `nil`. Emacs never case-folds a syntax class.
- **A wrong-arity call to a *closure* named the symbol** — ✅ FIXED (R14-B).
  `funcall_lambda` signals with `fun`, what indirection landed on; only
  `eval_sub`'s subr branch signals with `original_fun`. elisprs passed the
  designator in both branches.

  ```
  $ emacs -Q --batch --eval '(progn (defun f1 (a) a) (prin1 (condition-case e (f1 1 2) (error e))))'
  (wrong-number-of-arguments #[(a) (a) (t)] 2)
  $ elisp -e '(progn (defun f1 (a) a) (prin1 (condition-case e (f1 1 2) (error e))))'
  (wrong-number-of-arguments #[(a) (a) (t)] 2)
  ```

  The subr rows are unchanged and pinned: `(car 1 2)` is still
  `(wrong-number-of-arguments car 2)` and the applied subr is still
  `#<subr car>`.
- **A closure still prints the environment it captured** — round 11's
  "Re-verified as already fixed" is wrong, and the fuzzer found it again at seed
  777 (form #1591). The re-verification used `(lambda (x) x)` with nothing in
  scope to capture, which cannot show the bug. With an enclosing binding:

  ```
  $ cat c.el
  (princ (format "%S\n" (let ((q 5)) (condition-case e (funcall (lambda (x) (list x x)) 1 2) (error e)))))
  $ emacs -Q --batch -l c.el
  (wrong-number-of-arguments #[(x) ((list x x)) nil] 2)
  $ elisp c.el
  (wrong-number-of-arguments #[(x) ((list x x)) ((q . 5))] 2)
  ```

  (and `(t)` on the Emacs side when the same form is run through
  `(eval FORM t)` rather than loaded from a file without a `lexical-binding`
  cookie — elisprs prints `((q . 5))` either way). Emacs prunes a closure's
  captured environment to the free variables of its BODY; `(lambda (x) (list x
  x))` has none, so it captures nothing. elisprs keeps every binding that was in
  scope.

  Round 13 read the actual pruner and the substrate requirement is larger than
  "a free-variable analysis in `compiler.rs`". Emacs 30 does this in
  `cconv-make-interpreted-closure` (`lisp/emacs-lisp/cconv.el`), and the
  analysis is not run on the source — the function **macroexpands the lambda
  first** and stores the *expanded* body in the closure:

  ```lisp
  (let* ((form `#'(lambda ,args ,iform . ,body))
         (expanded-form (let ((lexical-binding t) …)
                          (macroexpand-all form macroexpand-all-environment)))
         (expanded-fun-body (pcase expanded-form
                              (`#'(lambda ,_args ,_iform . ,newbody) newbody)
                              (_ body)))
         (fvs (cconv-fv expanded-form lexvars dynvars))
         (newenv (nconc (mapcar (lambda (fv) (assq fv env)) (car fvs)) (cdr fvs))))
    (make-interpreted-closure args expanded-fun-body (or newenv '(t)) …))
  ```

  That is directly observable, and elisprs diverges on the body as well as on
  the environment — `macroexp--expand-all` unfolds `(funcall (lambda () X))` to
  `X` and rewrites a nested `(lambda …)` to `#'(lambda …)`:

  ```
  $ cat elisprs_closure_body.el     # -*- lexical-binding: t -*-
  (let ((q 5) (w 7)) (princ (format "f=%S\n" (lambda (x) (funcall (lambda () q)) 99))))
  (let ((q 5)) (princ (format "h=%S\n" (lambda (x) (mapcar (lambda (y) (+ y q)) x)))))
  $ emacs -Q --batch -l elisprs_closure_body.el
  f=#[(x) (q 99) ((q . 5))]
  h=#[(x) ((mapcar #'(lambda (y) (+ y q)) x)) ((q . 5))]
  $ elisp elisprs_closure_body.el
  f=#[(x) ((funcall (lambda nil q)) 99) ((w . 7) (q . 5))]
  h=#[(x) ((mapcar (lambda (y) (+ y q)) x)) ((q . 5))]
  ```

  So closing this means (a) macroexpanding the lambda at closure-creation time
  and storing that as `ClosureSrc::body`, then (b) porting `cconv-fv` —
  `cconv-analyze-form`'s full special-form dispatch, not a symbol scan, since
  the analysis has to respect `quote`, `let`/`let*` shadowing, a nested lambda's
  own parameters, and `setq` as a use. Approximating (b) without (a) is not an
  option that can be shipped: the pruned environment is the *runtime* one, so an
  analysis that misses a reference turns a working closure into
  `void-variable`. Deliberately left unfixed rather than approximated.

  What round 13 *did* fix is the neighbouring half of the same printout: the
  arity error now names the closure rather than the symbol (see the entry
  above), so `(f1 1 2)` differs from Emacs only in the environment field.
- ~~**`format`'s `%Ns` width and `%.Ns` precision count characters, not display
  columns.**~~ — ✅ FIXED (R14-D).

  ```
  $ emacs -Q --batch --eval '(prin1 (list (format "%.3s" "\tXY") (format "%.3s" "中XY") (format "%.3s" "中中") (format "%5s" "a\tb")))'
  ("" "中X" "中" "a\tb")
  $ elisp -e '(prin1 (list (format "%.3s" "\tXY") (format "%.3s" "中XY") (format "%.3s" "中中") (format "%5s" "a\tb")))'
  ("" "中X" "中" "a\tb")
  ```

  The width model was already in the tree — the prelude's `char-width` — just
  not reachable from `format`, which is Rust. It moved to
  `builtins::char_display_width` and `char-width` became a subr, which is what
  Emacs has anyway (`Fchar_width`, `indent.c`; `(subrp (symbol-function
  'char-width))` is `t` on 30.2). One table, one answer. Measurements the model
  reproduces: `(list (char-width ?\t) (char-width ?\n) (char-width 7)
  (char-width 127) (char-width 200) (char-width ?中) (string-width "a\t"))` =>
  `(8 0 2 2 1 2 9)` — a TAB is a flat `tab-width` rather than a distance to the
  next tab stop, which the `"a\t"` => 9 row settles.

  Both the field width and the precision are columns, for *every* conversion,
  not only `%s`: `(format "%4c|" ?中)` is `"  中|"` and `(format "%6S|" "中")` is
  `"  \"中\"|"`. `%S` also gained the precision it never applied at all —
  `(format "%.3S" "中中中")` is `"\"中"`, one column for the opening quote plus
  two for 中.

  Not covered: a raw byte inside a *multibyte* string, the `\200` display form
  the original report mentioned. `(string-width (unibyte-string 200))` is `1` on
  30.2 (U+00C8 is one column) and matches; the 4-column form belongs to
  `print-escape-nonascii`-style rendering, which is a separate model.
- **`emacs -Q --batch` starts with three buffers, elisprs with one** — ✅ FIXED
  in round 16 (R16-D).
- **A closure's body is stored unexpanded** — the second half of the closure
  entry above, recorded separately because it is observable on its own:
  `(let ((q 5)) (lambda (x) (funcall (lambda () q))))` prints
  `#[(x) (q) ((q . 5))]` in Emacs and
  `#[(x) ((funcall (lambda nil q))) ((q . 5))]` here, and a nested lambda prints
  as `#'(lambda …)` there and `(lambda …)` here. Both follow from
  `cconv-make-interpreted-closure` storing `macroexpand-all`'s output.
- Unchanged from rounds 9, 10, 11 and 12, for the reasons already recorded there:
  reader-level backquote preservation, `setf`'s gv expansion shape,
  `hash-table-size`, `aset` on a string, the warm-cache `make-symbol` re-intern,
  an empty-name uninterned symbol printing over-escaped, a void function's
  arguments still being evaluated, special forms and macros not checking arity,
  and the silent AOT run whose constants are not reconstructible.

---

## Round 15 — an integer and a float compared through `f64`, so Emacs's exact rule was lost

`(= (expt 3 34) (float (expt 3 34)))` answered `t`; Emacs answers `nil`.

`(expt 3 34)` is 16677181699666569 — a *fixnum*, since `most-positive-fixnum` is
2^61-1 on a 64-bit build — and it is past 2^53, where an `f64` runs out of
mantissa. Its float image is 16677181699666568.0, one *less*. So the integer and
its own float are different numbers, and everything turns on whether the
comparison rounds the integer or not.

Emacs does not round. `arithcompare` (data.c) decides on exact values, which is
Emacs's own rule and not the one its neighbours picked: Java, Scala and Groovy
promote the integer to `double` and accept the rounded answer, while Go rejects
a mixed pair outright. Measured, `emacs --batch`, GNU Emacs 30.2:

```text
(=  L F) => nil   (<  L F) => nil   (>  L F) => t
(=  F L) => nil   (<  F L) => t     (>  F L) => nil
(max L F) => 16677181699666569      (min L F) => 16677181699666568.0
```

elisprs rounded, in three places that had to agree and did not:

- `host::apply_num_op` — the numeric hook fusevm calls,
- `builtins::cmp` — the `=` `<` `>` `<=` `>=` subrs,
- `builtins::min_max` — `max` / `min`.

Each had an `(Int, Int)` arm comparing exactly (Round 10's fix) and a fallback
arm that ran both operands through `to_f64`. The mixed pair took the fallback.
`min` was the visible one: `(min L F)` answered the *integer* where Emacs
answers the float, because F is the smaller number even though both round to the
same `f64`.

All three now go through `host::num_cmp`, which compares exact values. A float is
a dyadic rational, so truncating it loses nothing and its fractional part breaks
a tie on equal integer parts; a NaN is incomparable and an infinity is beyond
every integer. Arithmetic is deliberately left alone — Emacs is float-contagious
there, so `(+ L 0.0)` is 16677181699666568.0 and rounding is the right answer.

One case was not closed by this crate alone. A **two-argument** comparison is
lowered to a fusevm op (`compiler.rs`, `try_native_op`), and fusevm 0.17.0
answered a mixed `Int`/`Float` pair natively — on the rounded images — without
ever consulting the hook (`vm.rs`, `cmp_int_fast`: both operands are "native
nums"), so `(= L F)` still answered `t`, as did `(/= L F)`, which the prelude
defines as `(not (= a b))`. Everything reaching the subrs — `funcall`, `apply`,
three or more arguments, `sort`, `max`, `min` — was already correct.

**Closed by the fusevm 0.22.0 bump** (round 16 rebased onto it): fusevm now
delegates exactly that pair, and the hook above answers it. Re-measured on the
bumped tree, `L` = `(expt 3 34)` and `F` = `(float L)` — every column agrees with
`emacs -Q --batch` on GNU Emacs 30.2:

```text
(= L F)  => nil   (/= L F) => t     (< L F)  => nil   (> L F)  => t
(<= L F) => nil   (>= L F) => t
(max L F) => 16677181699666569      (min L F) => 16677181699666568.0
```

---

## Round 16 — constant symbols, `defvar` vs `defconst`, and the startup buffers

The self-consistency sweep (call every bound *function* with a literal and with
an expression of the same value; anything that differs is a bug provable without
a reference implementation) reported `set-default` as the only real divergence
once macros and special forms were excluded — macros do not evaluate their
arguments, so the sweep only means anything for `functionp` symbols. Pulling on
`set-default` turned up a whole model that was missing rather than a single
builtin: elisprs had no notion of a constant symbol.

### R16-A. `nil`, `t`, and keywords were not constants — ✅ FIXED

Emacs marks exactly three kinds of symbol constant (`make_symbol_constant`), and
every writer funnels through `set_internal`, which signals `setting-constant`
with the rejected symbol as the error data. elisprs did neither. It failed in
two different directions at once:

| form | Emacs 30.2 | elisprs (before) |
| --- | --- | --- |
| `(set nil 1)` | `(setting-constant nil)` | `(set "not a symbol")` |
| `(set t 1)` | `(setting-constant t)` | `(set "not a symbol")` |
| `(set :kw 1)` | `(setting-constant :kw)` | `1` |
| `(setq :a 1)` | `(setting-constant :a)` | `1` |
| `(set-default :kw 1)` | `(setting-constant :kw)` | `1` |
| `(setq-default nil 5)` | `(setting-constant nil)` | `(set-default "not a symbol")` |
| `(makunbound t)` | `(setting-constant t)` | `(makunbound "not a symbol")` |
| `(make-local-variable nil)` | `(setting-constant nil)` | `(make-local-variable "not a symbol")` |
| `(let ((nil 1)) nil)` | `(setting-constant nil)` | `(let "binding name must be a symbol")` |
| `(let (:kw) 1)` | `(setting-constant :kw)` | `1` |
| `(defconst :dc 1)` | `(setting-constant :dc)` | `:dc` |
| `(set (intern "t") 1)` | `(setting-constant t)` | `1` |

For `nil` and `t` the condition was invented and named the *builtin* rather than
the symbol; for keywords there was no error at all and the write went through.
The `ops::SETVAR` and `ops::FSET` dispatch arms compounded it by discarding the
host's result with `let _ =`, so even a failing write let the form answer its own
value; both now `abort` on `Err` the way `GETVAR` already did.

`setting-constant` was added to the conditions whose message is re-read as error
*data* (`make_error_object`), which is what turns `"setting-constant: nil"` into
`(setting-constant nil)` rather than a message string.

The prelude was already relying on this: `macroexp--const-symbol-p` with
`any-value` detects a constant by doing `(set symbol (symbol-value symbol))`
inside a `condition-case` for `setting-constant`. That branch could never fire
before, so the predicate silently answered nil for every constant.

### R16-B. A keyword was not its own value — ✅ FIXED

The other half of the same model, and a `intern_driver` (lread.c) port. Interning
a `:`-prefixed name in the standard obarray seeds the symbol's value cell with
the symbol itself, declares it special, and makes it constant. `intern`
(`src/host.rs`) set `value: None` unconditionally, so:

```elisp
(boundp :fresh)          ; Emacs t   — elisprs nil
(default-boundp :fresh)  ; Emacs t   — elisprs nil
(default-value :fresh)   ; Emacs :fresh — elisprs (void-variable :fresh)
(special-variable-p :kw) ; Emacs t   — elisprs nil
```

`(symbol-value :a)` did answer `:a`, but only because the builtin special-cased
the name; the docstring next to it claimed `intern` seeded the cell and made it
constant, which was not true of either half. The cell is really seeded now and
the docstring describes what the code does.

**The obarray is part of the keyword test, not just the spelling.** Emacs applies
the treatment only for `initial_obarray`, so a `:`-spelled symbol created any
other way is an ordinary, writable, unbound symbol:

```elisp
(let ((s (make-symbol ":u")))
  (list (keywordp s) (boundp s) (progn (set s 1) (symbol-value s))))
;; Emacs 30.2 => (nil nil 1);  elisprs => (t nil :u)

(let* ((ob (obarray-make)) (s (intern ":ob" ob)))
  (list (keywordp s) (boundp s) (progn (set s 7) (symbol-value s))))
;; Emacs 30.2 => (nil nil 7);  elisprs => (t nil :ob)
```

`keywordp` was a name-prefix check in the prelude and answered `t` for both. It
is a subr now — only the host can tell which object the obarray holds — and the
prelude definition is gone rather than duplicated, so the
`subr_names_are_registered_once` guard still sees one registration.

`:` itself is a keyword and is constant; there is no exception for the
one-character name.

### R16-B2. The rejection happens at bind time, not at compile time

Worth recording separately because the first fix put it in the wrong place.
Emacs evaluates every initializer before it attempts the first write:

```elisp
(let ((x 0)) (list (condition-case e (let ((a (setq x 1)) (nil 2)) nil) (error e)) x))
;; Emacs 30.2 => ((setting-constant nil) 1)   — x is 1, so the init ran
```

Rejecting in `parse_binding` at compile time produced the right error object but
skipped the initializer, and — the part that actually broke — put the failure
outside the enclosing `condition-case`, because nothing ever started running. The
check lives in the `SPECBIND` / `LETBIND` dispatch arms instead. `LETBIND` opens
no scope on the failing path, so nothing needs unwinding.

### R16-C. `defvar` overwrote an existing value — ✅ FIXED

Independent of the above, found while checking that `(defvar :dv 1)` stayed
legal. `defvar` and `defconst` differ in exactly one way, and it is not the
declaration: both mark the variable special, but `defvar`'s initializer runs
*only when the variable is still void*, while `defconst` always assigns.
elisprs compiled both to an unconditional `SETVAR`.

```elisp
(progn (setq zz 5) (defvar zz 9) zz)      ; Emacs 5  — elisprs 9
(progn (defvar yy 1) (defvar yy 2) yy)    ; Emacs 1  — elisprs 2
(progn (setq zz 5) (defconst zz 9) zz)    ; Emacs 9  — elisprs 9 (control)
```

This is the rule that makes it safe to `setq` a library's variable before loading
the library, so the old behaviour silently discarded user configuration. A new
`ops::DEFVAR_INIT` writes only a void cell; `defconst` keeps `SETVAR`. It also
makes `(defvar :dv 1)` a no-op returning `:dv` — matching Emacs — because a
keyword is never void, rather than a `setting-constant` error.

### R16-D. The startup buffer list — ✅ FIXED

Carried over from round 14. `emacs -Q --batch` starts with three buffers and
elisprs started with one:

```elisp
(mapcar #'buffer-name (buffer-list))
;; Emacs 30.2 => ("*scratch*" " *Minibuf-0*" "*Messages*")
;; elisprs     => ("*scratch*")
```

`(get-buffer "*Messages*")` answered nil, so nothing could address it. Both
buffers are created in the bootstrap next to `*scratch*`, inside the built-in
arena prefix so they are never serialized as user heap. The leading space on
` *Minibuf-0*` is what marks a buffer hidden.

### R16-E. An empty closure body printed as `()` rather than `(nil)` — ✅ FIXED

Fell out of the `fset` comparisons. Emacs normalizes an absent body to the single
form `nil`, uniformly across `lambda`, `defun`, and `defmacro`:

```elisp
(prin1-to-string (lambda ()))                     ; Emacs "#[nil (nil) (t)]"  — elisprs "#[nil () (t)]"
(prin1-to-string (lambda (x)))                    ; Emacs "#[(x) (nil) (t)]"  — elisprs "#[(x) () (t)]"
(progn (defmacro m7 ()) (prin1-to-string (symbol-function 'm7)))
                                                  ; Emacs "(macro . #[nil (nil) (t)])"
```

Only the printed source is affected — an empty compiled body already evaluated
to nil in both.

### Declined this round, with the evidence

- **Closure environment pruning, and the unexpanded closure body.** Unchanged
  from round 14, and still the right refusal. Closing it needs
  macroexpand-at-closure-creation *and* a port of `cconv-fv`'s special-form
  dispatch, together: `cconv-make-interpreted-closure` macroexpands the lambda
  first and stores the expanded body, and the pruned environment *is* the runtime
  environment, so a reference the free-variable walk misses turns a working
  closure into `void-variable`. Half of it is worse than none of it.
- **Loading a file does not create Emacs's ` *load*` buffer.** Turned up while
  re-checking R16-D. `emacs -Q --batch --eval` reports the clean three, but
  `emacs -Q --batch -l FILE` reports
  `("*scratch*" " *Minibuf-0*" "*Messages*" " *load*")` — `load` reads the file
  through a buffer of its own — and elisprs reports the three either way. Same
  family as the `ert` entry below: buffers created by machinery rather than by
  the program.
- **`elisp FILE` models `emacs -l FILE`, not `emacs --script FILE`.** Same
  family, and it is observable through `char-syntax`. The two Emacs entry points
  evaluate the file in different buffers, so they read different syntax tables:

  ```text
  $ emacs -Q --batch -l probe.el     => buf " *scratch*" lisp-interaction-mode
                                        (char-syntax ?.) => 95  (?_)
  $ emacs --script probe.el          => buf " *load*"    fundamental-mode
                                        (char-syntax ?.) => 46  (?.)
  ```

  elisprs answers 95, i.e. the `-l` column, which is the invocation
  `scripts/fuzz_parity.sh` compares against. A script that reads the buffer's
  syntax table and is run under `emacs --script` will therefore disagree;
  wrapping the read in `(with-syntax-table (standard-syntax-table) …)` makes both
  agree. Closing it needs the entry point to select the initial buffer's table,
  which in turn needs the buffer machinery the ` *load*` entry above describes.
- **`other-buffer`.** Suggested as the observable for R16-D, but it is not
  implemented at all here — `src/host.rs` only mentions it in a comment — and its
  result is not a function of `buffer-list`. Measured on 30.2, with
  `(buffer-list)` = `("*scratch*" " *Minibuf-0*" "*Messages*" " *load*" "aaa")`
  and `(frame-parameter nil 'buffer-list)` = `("*scratch*")`:

  | BUFFER arg | current | `other-buffer` |
  | --- | --- | --- |
  | nil | `*scratch*` | `*Messages*` |
  | nil | `aaa` | `*Messages*` |
  | `*scratch*` | `aaa` | `*Messages*` |
  | `*Messages*` | `*scratch*` | `*scratch*` |
  | nil (after killing `*Messages*`) | `aaa` | `aaa` |

  No first-match or last-match rule over `buffer-list` reproduces all five rows.
  What does: buffers in the frame's own buffer list are demoted to the fallback
  (`notsogood`) and only used when nothing else qualifies, which is why
  `*scratch*` — the sole entry of `(frame-parameter nil 'buffer-list)` — is never
  returned unless it is the only candidate left. That is the `record_buffer` MRU
  model, and elisprs has no frame representation at all (zero references to
  `selected-frame` or `frame-parameter`). A version that returns a plausible but
  wrong successor is worse than the current `void-function`.
- **`(fset t …)` and `defvaralias` onto `nil`/`t`.** One shared root cause:
  `nil` and `t` are `Value::Undef` / `Value::Bool(true)` here, not arena symbols,
  and `alias_of` is an `Option<u32>` into the arena. Emacs allows both —
  `(progn (fset t (lambda () 42)) (funcall t))` is `42`, and
  `(progn (defvaralias 'xa nil) (setq xa 5))` signals `(setting-constant xa)`,
  naming the *alias*. Supporting the write without the read-back would be a
  silent wrong value, so both are left alone. The alias side that does not need
  arena identity — a constant as the *alias* — is fixed:
  `(defvaralias nil 'x)` now signals
  `(error "Cannot make a constant an alias: nil")`, which is a plain `error` in
  Emacs, not `setting-constant`.

### Not a bug — sweep artifacts

Recorded so the next round does not re-investigate them. The sweep flags any pair
that differs, and these differ for reasons that are not divergences:
`time-convert` / `time-to-seconds` (clock), `make-temp-name` / `sxhash-eq` /
`sxhash-eql` (identity or randomness), `unintern` / `kill-buffer` /
`re-search-forward` / `search-backward` / `skip-chars-forward` (stateful — the
first call of the pair changes what the second sees), and the
`Error setting nil: …` line on stderr, which is `custom-theme-set-variables`'
own `condition-case` handler reporting a call the sweep made with nil.

### Found this round, not fixed — two failures that predate it

Both reproduce byte-identically at the round's start commit
(`2156c2e1cab92914078932e17965d3dd82b7b5cf`), so neither is a regression from the
work above. Neither the tests nor the examples were touched to hide them.

- **`ert` runs a test body in `*scratch*`; Emacs runs it in a temp buffer.**
  The mechanism is right and is still open (see round 18 for the `ert.el`
  citation), but the conclusion drawn here was wrong on both halves, and
  re-measured in round 18: `emacs -Q --batch -l examples/char-syntax-tables.el`
  **FAILS** `st-standard-table` — the pinned 95 is the answer for a TOP-LEVEL
  form under `-l`, not for an ERT body, which Emacs runs in ` *temp*` where
  `(char-syntax ?.)` is 46. So the expectation is wrong too, and the example
  passed here only because elisprs's ERT does not switch buffers. The
  whole difference is which buffer is current inside the body:

  ```elisp
  (require 'ert)
  (ert-deftest where-am-i ()
    (message "buffer=%S mode=%S char-syntax(.)=%S"
             (buffer-name) major-mode (char-syntax ?.))
    (should t))
  (ert-run-tests-batch-and-exit)
  ;; Emacs 30.2 => buffer=" *temp*"   mode=fundamental-mode        char-syntax(.)=46
  ;; elisprs     => buffer="*scratch*" mode=fundamental-mode        char-syntax(.)=95
  ```

  `*scratch*` carries the lisp syntax table, where `.` is a symbol constituent
  (`_`, 95); a temp buffer carries the standard table, where it is punctuation
  (`.`, 46). Everything else agrees — `(aref (standard-syntax-table) ?.)` is
  `(1)` in both, and `(with-temp-buffer (char-syntax ?.))` is 46 in both. The fix
  is to run each `ert` body inside a temp buffer, but that changes the current
  buffer for every existing example self-test at once, so it wants its own round
  with a full `--test examples` re-run rather than a drive-by.
- **`regexp::tests::classes_pass_through`** (a `--lib` unit test) fails at the
  start commit: `[[:alpha:]]` is expected to pass through unchanged and is
  rewritten to `[\p{Alphabetic}\p{M}]`. Untouched here; it is the unit-test
  counterpart of the R9-H `[:class:]` work already recorded above.

---

## Round 17 — the entry point's own state, and the `format-message` family

Round 16 closed with two entries in "declined": the ` *load*` buffer, and
`elisp FILE` modeling `emacs -l FILE` rather than `emacs --script FILE`. Both
turned out to be the same missing piece, and it was small. This round is the
sweep that finished the audit and the fixes that fell out of it.

The observable × entry-point table, all measured on **GNU Emacs 30.2** with one
probe file run three ways (`--eval '(load FILE)'`, `-l FILE`, `--script FILE`),
against `elisp FILE` on the tree at the end of this round. `--eval` and `-l`
agreed on every row, so they share a column.

| observable | `--eval` / `-l` | `--script` | `elisp FILE` | `elisp --script FILE` |
| --- | --- | --- | --- | --- |
| `(buffer-name)` | `"*scratch*"` | `" *load*"` | `"*scratch*"` ✅ | `" *load*"` ✅ |
| `major-mode` | `lisp-interaction-mode` | `fundamental-mode` | `fundamental-mode` ❌ | `fundamental-mode` ✅ |
| `(buffer-list)` | 3 + `" *load*"` | 3 + `" *load*"` | 3 + `" *load*"` ✅ | 3 + `" *load*"` ✅ |
| `load-file-name` | the file | the file | the file ✅ | the file ✅ |
| `noninteractive` | `t` | `t` | `t` ✅ | `t` ✅ |
| `command-line-args-left` / `argv` | args after the file | args after the file | args after the file ✅ | args after the file ✅ |
| `(eq (syntax-table) (standard-syntax-table))` | `nil` | `t` | `nil` ✅ | `t` ✅ |
| `(char-syntax ?.)` | `95` | `46` | `95` ✅ | `46` ✅ |
| `(char-syntax ?\;)` | `60` | `46` | `60` ✅ | `46` ✅ |
| `(aref (syntax-table) ?.)` | `(3)` | `(1)` | `(3)` ✅ | `(1)` ✅ |
| `(string-match "\\s_" ".")` | `0` | `nil` | `0` ✅ | `nil` ✅ |
| `(skip-syntax-forward "w_")` over `"foo.bar ;c"` | `7` | `3` | `7` ✅ | `3` ✅ |
| `(forward-word 1)` | `4` | `4` | `4` ✅ | `4` ✅ |
| `(forward-sexp 1)` | `8` | `4` | void ❌ | void ❌ |
| `(scan-sexps 1 1)` | `8` | `4` | void ❌ | void ❌ |
| `(parse-partial-sexp …)` / `syntax-ppss` | `t` / `nil` | `nil` / `nil` | void ❌ | void ❌ |
| `thing-at-point` / `forward-symbol` | `"foo.bar"` / `8` | `"foo"` / `4` | no `thingatpt` ❌ | no `thingatpt` ❌ |

Everything not marked ❌ agreed. The three ❌ rows are described under "still
open" at the end of this section.

### R17-A. `elisp --script FILE` — ✅ ADDED (the R16 boundary, closed)

The two Emacs file entry points evaluate the file in different buffers, so they
read different syntax tables, and every `char-syntax` / `\sC` / `skip-syntax-*`
answer follows:

```text
$ emacs -Q --batch -l probe.el   => buf "*scratch*", lisp-interaction-mode, (char-syntax ?.) => 95
$ emacs --script    probe.el     => buf " *load*",   fundamental-mode,      (char-syntax ?.) => 46
```

R16 declined this because "closing it needs the entry point to select the initial
buffer's table, which in turn needs the buffer machinery the ` *load*` entry
describes." Building that buffer (R17-B) is what made it cheap: with ` *load*`
live, `--script` is `set-buffer` to it, and `--current-syntax-table--` is
buffer-local — unset there — so the standard table falls out with no table
plumbing at all. `elisp FILE` is unchanged and still the `-l` column, which is
what `scripts/fuzz_parity.sh` compares against.

The entry point is folded into `cache::schema_key`, because a top-level form that
reads the buffer can change how a *later* form macro-expands; chunks compiled
under one entry point must not be replayed under the other.

### R17-B. Loading a file did not create Emacs's ` *load*` buffer — ✅ FIXED

`emacs -Q --batch --eval` reports the clean three; `emacs -Q --batch -l FILE`
reports four. mule.el `load-with-code-conversion` is explicit about why:

```elisp
(let ((buffer (generate-new-buffer " *load*")) …)
  (unwind-protect (with-current-buffer buffer … (insert-file-contents fullname)) …
    (kill-buffer buffer)))
```

so the buffer exists for the duration of the load, holds the file's text, and is
killed on the way out. Measured on 30.2, a nested load nests:

```text
outer during: ("*scratch*" " *Minibuf-0*" "*Messages*" " *load*")
inner during: ("*scratch*" " *Minibuf-0*" "*Messages*" " *load*" " *load*-711551")
outer after:  ("*scratch*" " *Minibuf-0*" "*Messages*" " *load*")
```

elisprs reproduces all three lines now, including the random suffix (R17-C).

The slot is reserved in `ElispHost::new` — inside the built-in arena prefix — and
left *dead*, so `elisp -e` (the `--eval` model) still reports three. `eval_file`
revives it. Reserving rather than allocating is what keeps the bytecode cache
honest: a cache hit skips the prelude entirely, so an arena handle allocated on
the miss path would not exist on the hit path. Both paths now see the same
handle, and the source is read on both (a hit that skipped the read would make
`(buffer-size)` depend on whether the cache was warm).

**Named boundary:** point in ` *load*` is 1, where `insert-file-contents` leaves
it. Emacs's is the reader's *current* position (176 partway through a 1043-char
probe file). elisprs reads every top-level form before running any of them, so
there is no meaningful "the read is here" position while the file's own code
observes it.

### R17-C. `generate-new-buffer-name` ignored both of its special cases — ✅ FIXED

`Fgenerate_new_buffer_name` (buffer.c) is not a `<N>` counter:

```c
  if ((!NILP (ignore) && !NILP (Fstring_equal (name, ignore)))
      || NILP (Fget_buffer (name)))
    return name;
  if (SREF (name, 0) != ' ') /* See bug#1229.  */
    genbase = name;
  else
    { int i = get_random () % 1000000;
      genbase = concat2 (name, "-<i>");
      if (NILP (Fget_buffer (genbase))) return genbase; }
  for (ptrdiff_t count = 2; ; count++)
    { gentemp = concat2 (genbase, "<count>");
      if (!NILP (Fstring_equal (gentemp, ignore)) || NILP (Fget_buffer (gentemp)))
        return gentemp; }
```

Two behaviors were missing. Measured on 30.2:

```text
(generate-new-buffer-name " *hid*")           ; taken => " *hid*-368192", not " *hid*<2>"
(generate-new-buffer-name "abc" "abc")        ; => "abc",     elisprs "abc<2>"
(generate-new-buffer-name "abc" "abc<2>")     ; => "abc<2>",  elisprs "abc<3>"
```

The IGNORE argument was declared (`s("generate-new-buffer-name", 1, Some(2), …)`)
and dropped on the floor, which silently disabled `rename-buffer`'s UNIQUE
argument (R17-D) — the only in-tree caller that passes one.

### R17-D. `rename-buffer` ignored UNIQUE, and accepted the empty name — ✅ FIXED

`Frename_buffer` (buffer.c) uses IGNORE = the current buffer's own name, which is
why renaming a buffer to a name it already holds succeeds in *both* spellings.
Measured on 30.2, with a live buffer `taken`:

| form | Emacs 30.2 | elisprs before |
| --- | --- | --- |
| `(rename-buffer "taken" t)` | `"taken<2>"` | error, "is in use" |
| `(rename-buffer "taken")` | error | error ✅ |
| `(rename-buffer "s2")` twice | `"s2"` | `"s2"` ✅ |
| `(rename-buffer "s2" t)` on itself | `"s2"` | error |
| `(rename-buffer "")` | error, "Empty string is invalid as a buffer name" | `""` |

The error text also gains its curved quotes: `error()` in C runs its template
through `format-message` (R17-F), so Emacs reports
`Buffer name ‘taken’ is in use`.

### R17-E. `skip-syntax-forward` / `skip-syntax-backward` were void — ✅ ADDED

The syntax-class counterparts of `skip-chars-*`, and the base of most word and
symbol motion. Ported from `skip_syntaxes` (syntax.c): the C builds a 256-entry
fastmap over syntax *class codes*, complemented in place when SYNTAX begins with
`^`; the port carries the accepted classes as a list and flips the membership
test, which is the same thing without the array. `syntax_spec_code` is the
inverse of `--syntax-code-spec--` plus `-` as an alias for whitespace; every
other unlisted character addresses a slot no real class can match, so
`(skip-syntax-forward "Z")` skips nothing rather than signalling.

All 22 rows of the edge-case probe match `emacs -Q --batch -l` byte for byte,
including LIM clamping to `[point-min, point-max]`, a marker LIM, narrowing, the
negated form, and `(wrong-type-argument stringp 5)` for a non-string SYNTAX.

### R17-F. `error`, `user-error` and `message` used `format`, not `format-message` — ✅ FIXED

`Ferror` is literally `Fsignal (Qerror, list1 (Fformat_message (nargs, args)))`,
and `Fmessage` is `styled_format (nargs, args, true)`, so the default
`text-quoting-style` of `curve` applies to all three. Only the *template* is
translated — a substituted argument is not re-scanned. Measured on 30.2:

```elisp
(error "a `b' c")          ; "a ‘b’ c"      elisprs "a `b' c"
(error "a %s" "x `y'")     ; "a x `y'"      (argument untouched — control)
(user-error "a `b'")       ; "a ‘b’"        elisprs "a `b'"
(message "a `b' c")        ; "a ‘b’ c"      elisprs "a `b' c"
(signal 'error '("a `b'")) ; "a `b'"        (signal does not format — control)
(format "a `b' c")         ; "a `b' c"      (format never curves — control)
```

### R17-G. `substitute-command-keys` did not honor `\=` — ✅ FIXED

The mirror image of R17-F, and the reason the two cannot share an implementation:
`substitute-command-keys` treats `\=` as an escape that quotes the next character
and is discarded, and `format-message` does not. elisprs implemented both as the
same plain `string-replace` pair, so both were wrong in opposite directions.
Measured on 30.2:

| input | `substitute-command-keys` | `format-message` |
| --- | --- | --- |
| `"a \\=`b"` | `"a `b"` | `"a \\=‘b"` |
| `"a \\=\\= b"` | `"a \\= b"` | — |
| `"a \\=\\[f] b"` | `"a \\[f] b"` | — |
| `"a \\=' b"` | `"a ' b"` | — |
| `"a \\="` (nothing follows) | `"a \\="` | — |

### R17-H. `(message nil)` signalled, and batch `message` skipped the pending newline — ✅ FIXED

Two things in `Fmessage`'s batch path. First, a nil or empty template clears the
echo area and answers the argument unchanged:

```c
  if (NILP (args[0]) || (STRINGP (args[0]) && SBYTES (args[0]) == 0))
    { message1 (0); return args[0]; }
```

so `(message nil)` is nil in Emacs and was `(wrong-type-argument stringp nil)`
here — the error `(message t)` is supposed to have exclusively.

Second, `message_to_stderr` (xdisp.c) flushes `noninteractive_need_newline`,
which print.c's `printchar`/`strout` set on *every* batch write to stdout:

```c
  if (noninteractive_need_newline)
    { noninteractive_need_newline = false; errputc ('\n'); }
  if (STRINGP (m)) errwrite (SDATA (s), SBYTES (s));
  if (STRINGP (m) || !cursor_in_echo_area) errputc ('\n');
```

That is what keeps a `princ` on stdout and a following `message` on stderr off
the same line, and — since `cursor_in_echo_area` is always false in batch — it is
why `(message nil)` emits *two* newlines. A probe mixing `princ` and `message`
now produces byte-identical stderr under `emacs -Q --batch -l` and `elisp`
(`\n a ‘b’ c \n \n a `b' \n \n \n`); before, elisprs emitted neither the leading
flush nor the blanks. `with-output-to-string` captures instead of writing to
stdout, so it correctly does not set the flag.

### R17-I. `command-line-args`, `command-line-args-left` and `argv` were void — ✅ ADDED

`emacs -l FILE a b c` gives `argv` = `command-line-args-left` = `("a" "b" "c")`
(startup.el makes `argv` a `defvaralias` of the other, so `(setq argv …)` is
visible through both names) and `command-line-args` = the whole invocation.
All three were unbound here, so a script could not see its own arguments at all.

They are (re)set by the entry point on both cache paths, never baked into the
heap image: the clean prelude snapshot is captured *before* they are installed,
so a cache hit cannot replay the previous run's arguments. Verified across three
consecutive runs of the same cached script with different arguments.

### Still open, with the evidence

- **`major-mode` is `fundamental-mode` under `elisp FILE`; Emacs's `-l` column is
  `lisp-interaction-mode`.** Only the initial buffer's syntax *table* is modeled
  (`src/prelude.rs`, `(set-syntax-table emacs-lisp-mode-syntax-table)`); the mode
  chain `prog-mode → lisp-data-mode → emacs-lisp-mode → lisp-interaction-mode` is
  not ported, and `define-derived-mode` bodies pull in font-lock, keymaps and
  hook variables that nothing else here needs. Every syntax-derived observable
  already agrees, so the symbol itself is the whole divergence. Note that
  `elisp --script FILE` is correct — `--script` really is `fundamental-mode`.
- **`forward-sexp` / `backward-sexp` / `scan-sexps` / `scan-lists` / `up-list` /
  `down-list` / `forward-list`, and `parse-partial-sexp` / `syntax-ppss`.** All
  void. These are one port, not seven: `Fforward_sexp` is `scan_lists`, and
  `scan_lists` and `Fparse_partial_sexp` share `syntax.c`'s comment/string state
  machine (`scan_sexps_forward`, `forw_comment`, `back_comment`) including the
  two-character comment delimiters, nested comments, `syntax-propertize`
  properties and the `parse-sexp-ignore-comments` flag. `skip-syntax-*` (R17-E)
  needs none of that, which is why it is done and these are not. Doing a subset —
  `forward-sexp` over balanced parens with no comment handling — would answer
  plausibly on the common case and wrongly inside a string or comment, which is
  worse than void.
- **`thingatpt` is not available**, so `thing-at-point`, `bounds-of-thing-at-point`
  and `forward-symbol` are unreachable. `forward-symbol` is
  `(skip-syntax-forward "w_")` and would work today; the rest of the library is
  built on `forward-sexp` and the `*-at-point` provider table, so it waits on the
  entry above.
- **`buffer-modified-p`** is void, so one row of the ` *load*` probe cannot be
  compared at all. Not attempted here; it is buffer bookkeeping rather than
  entry-point state. (This entry also named `buffer-file-name`; re-measured in
  round 18, that one is bound and answers `nil`, as Emacs's does in batch.)

---

## Round 18 — the scanner, and what the harnesses structurally cannot see

Round 17 closed the entry-point boundary and left three rows of its table void:
`forward-sexp`, `scan-sexps` and `parse-partial-sexp`/`syntax-ppss`. Those are
this round's opening. The rest of the round audits the two things that sit above
the oracle: what each harness is *incapable* of reporting, and which recorded
claims were never true.

Oracle: **GNU Emacs 30.2** at `/opt/homebrew/bin/emacs` (`emacs --version` =>
`GNU Emacs 30.2`). Entry points measured: `emacs -Q --batch --eval` (modelled by
`elisp -e`), `emacs -Q --batch -l FILE` (modelled by `elisp FILE`), and
`emacs --script FILE` (modelled by `elisp --script FILE`).

### R18-A. Harness blind-spot census

What each harness is *structurally* unable to report — not "has not caught yet",
but "cannot, by construction".

| harness | cannot see | why |
| --- | --- | --- |
| `tests/examples.rs` | a script that ran **no tests** | `ert-run-tests-batch-and-exit` exits 0 after `Ran 0 tests`. A renamed `ert-deftest`, or one moved inside a false `when`, drops coverage to zero and still passes. **CLOSED** (R18-B) |
| `tests/examples.rs` | a script whose tests were all **skipped** | same: `Ran 5 tests: 0 unexpected, 5 skipped.` exits 0. **CLOSED** (R18-B) |
| `tests/examples.rs` | wrong *output* | only the exit status is checked. The examples are self-checking, so a wrong value fails from the inside — but a wrong value its own `should` agrees with is invisible. Not closed: the only honest fix is an Emacs-side oracle, and the examples depend on the preloaded prelude, so Emacs cannot run them |
| `tests/examples.rs` | entry-point and cache-path divergence | every example runs as `elisp FILE`, never `--script`, never twice. A `--script`-only or warm-cache-only regression cannot appear |
| `scripts/fuzz/gen.el` | anything with **buffer or text state** | every generated form must be pure and bounded, so `insert`/`point`/`goto-char`/narrowing/markers/buffer text properties are never emitted. The entire text-editing subsystem — including this round's scanner — is unreachable |
| `scripts/fuzz/gen.el` | anything with **syntax tables** | no `modify-syntax-entry`, `with-syntax-table`, `char-syntax` in the call table |
| `scripts/fuzz/gen.el` | **multi-form** behavior | one form per line, evaluated independently: `setq` across forms, `defvar` semantics, dynamic binding, `defun`/`defmacro` definition order, and load-order effects are all out of reach |
| `scripts/fuzz/gen.el` | **non-local exit** shapes | no `catch`/`throw`/`unwind-protect`/`condition-case` in the call table |
| `scripts/fuzz/gen.el` | astral-plane and raw-byte characters | the char pool is `?a ?z ?A ?0 ?\s ?\n ?\t ?é` |
| `scripts/fuzz/drive.el` | **stdout, stderr, exit code, the message buffer, point** | the comparison is `prin1` of the value, or of the error object. A form that prints the wrong thing and returns the right thing is parity |
| `scripts/fuzz/drive.el` | a divergence past character **400** | results are clipped. Two results agreeing for 400 characters compared *equal*. **NARROWED** (R18-C) |
| `scripts/fuzz/drive.el` | dynamic-binding paths | `(eval FORM t)` — lexical only |
| `scripts/fuzz/drive.el` | a library Emacs does not preload but elisprs does | it `require`s `cl-lib`/`seq`/`subr-x` on the Emacs side to compare like with like, which also hides any case where elisprs providing something early is itself the divergence |
| `scripts/fuzz_parity.sh` | a **corpus generator** mismatch | the generator runs under `$EMACS`, the same binary the version gate pins. Verified separately that it does not matter: generating 400 forms at seed 1 under `emacs` and under `elisp` gives byte-identical corpora (`diff` empty), so `gen.el`'s claim to be engine-independent is TRUE |
| the in-process `eval_str` suites | anything about the **cache** | they call `reset_host()` + `eval_str`; `~/.elisprs/scripts.rkyv` is never consulted |
| the in-process `eval_str` suites | **entry-point** variation | always the `--eval` model (`*scratch*`, `lisp-interaction-mode`), so a syntax-table-derived expectation captured under `-l`/`--script` cannot be checked here |
| the in-process `eval_str` suites | a **fabricated** expectation | every expectation is a frozen string literal with no live oracle. Nothing in the harness can tell "Emacs said this once" from "someone typed this" |
| all of the above | a **stack-depth** regression | the shipped binary runs the interpreter on a 512 MiB thread (`INTERP_STACK_BYTES`); a test thread gets 2 MiB. This round hit it: `cl-flet` around the scanner's largest form pushed prelude loading past 2 MiB and aborted seven suites with `stack overflow` while `./target/debug/elisp` ran the same code fine. The closure was rewritten as a plain lexical `lambda`; the asymmetry itself remains |

### R18-B. `tests/examples.rs` accepted a script that tested nothing — ✅ CLOSED

The harness now parses ERT's summary line and requires the count to equal the
number of top-level `(ert-deftest ` forms in the file, with zero unexpected
results and not every test skipped. Measured over all 71 examples before the
change: 71/71 already satisfy it (`Ran N` equals the `ert-deftest` count in every
file; one file, `examples/ert.el`, skips exactly one of five on purpose, which is
why the rule is "not *all* skipped" rather than "none skipped"). It costs no
extra runtime — the output was already captured for the failure message.

### R18-C. `scripts/fuzz/drive.el` clipped away divergences — ✅ NARROWED

`fz-clip` truncated a result at 400 characters and appended a constant marker, so
two results that agree for 400 characters and then differ printed the same line
under both engines and were counted as parity. The marker now carries the full
length and the last 40 characters, both computed from the string itself, so they
introduce no oracle of their own. Still not a proof: a divergence confined to the
middle of a >400-character result with the same total length and the same tail is
still invisible.

### R18-D. The `syntax.c` scanner family — ✅ ADDED

Ported as one machine, from `syntax.c` (Emacs 30.2): `scan_lists`,
`scan_sexps_forward`, `forw_comment`, `back_comment`, `char_quoted`,
`prev_char_comend_first`, `find_defun_start`, `in_2char_comment_start`,
`internalize_parse_state`, plus `Fscan_lists`, `Fscan_sexps`,
`Fparse_partial_sexp`, `Fforward_comment`, `Fbackward_prefix_chars`,
`Fmatching_paren`, and `lisp.el`'s motion commands and `syntax.el`'s
`syntax-ppss`. Round 17 declined a subset on the grounds that
"`forward-sexp` over balanced parens with no comment handling would answer
plausibly on the common case and wrongly inside a string or comment"; that still
holds, which is why the comment/string state machine is in.

`scan_sexps_forward` is one C function with eleven labels and a fallthrough
switch. It is transcribed as an explicit label machine — a `label` variable
holding the C's program counter — rather than re-derived as structured elisp,
because re-deriving the control flow is the step that turns a port into a
rewrite.

Verification: 236 differential probes under `emacs -Q --batch -l` and under
`elisp`, covering balanced text, strings, escapes, C-style `/* */` and `//`
comments in both `parse-sexp-ignore-comments` settings, nested `#| |#`, generic
string and comment fences, the `$` math class, resumption from a partial state
(including one stopped between the two halves of a two-character delimiter),
`targetdepth`, `stopbefore`, `commentstop`, the `syntax-table` text property,
every motion command, and the error shapes. **236/236 agree**, on the cold and
the warm cache path alike.

The three ❌ rows of round 17's entry-point table are now:

| observable | `--eval` / `-l` | `--script` | `elisp FILE` | `elisp --script FILE` |
| --- | --- | --- | --- | --- |
| `(forward-sexp 1)` | `8` | `4` | `8` ✅ | `4` ✅ |
| `(scan-sexps 1 1)` | `8` | `4` | `8` ✅ | `4` ✅ |
| `(nth 4 (parse-partial-sexp …))` / `(nth 4 (syntax-ppss …))` | `t` / `t` | `nil` / `nil` | `t` / `t` ✅ | `nil` / `nil` ✅ |

(The `parse-partial-sexp` row is re-measured here with a probe of its own —
`"foo.bar ;c"` inserted into the entry point's buffer, asking whether position
`point-max` is inside a comment — so its `--eval`/`-l` cell is `t`, not the
`t` / `nil` pair round 17 recorded for its differently-shaped probe.)

**Named boundary:** `syntax-ppss` parses from `point-min` on every call instead
of memoizing in `syntax-ppss-cache` / `syntax-ppss-last`. That is exactly what
Emacs does on a cold cache, which is the only state a batch process is ever in;
the difference is speed, plus elements 2 and 6, which Emacs's own docstring
already says "cannot be relied upon".

**Named boundary:** `syntax-propertize` runs `syntax-propertize-function` (nil by
default, so a no-op) but does not implement `syntax-propertize-rules` or the
`syntax-multiline` machinery.

### R18-E. Locale

`grep -rn 'setlocale' src/` finds nothing: elisprs never calls `setlocale(3)`, so
its libc stays in the C locale and **no elisprs answer probed here changes with
`LC_ALL`**. All 43 frozen case/time expressions in the tests produce byte-identical
output under `LC_ALL=en_US.UTF-8`, `LC_ALL=C` and `LC_ALL=de_DE.UTF-8`, and the
case tests pass under all three. The exposure is on the *oracle* side.

Three frozen records encode an Emacs answer that only holds under an English
`LC_TIME`, so re-deriving them on a German or French machine would produce a
different string:

| record | frozen | `LC_ALL=de_DE.UTF-8` Emacs | `LC_ALL=fr_FR.UTF-8` Emacs |
| --- | --- | --- | --- |
| `tests/eval.rs:1833` `(format-time-string "%A %B %e, %Y" 0 t)` | `"Thursday January  1, 1970"` | `"Donnerstag Januar  1, 1970"` | `"jeudi janvier  1, 1970"` |
| `tests/eval.rs:1841` `(format-time-string "%I:%M %p" 0 t)` | `"12:00 AM"` | `"12:00 "` | `"12:00 "` |
| `tests/eval.rs:1845` `(format-time-string "%j (%a)" 0 t)` | `"001 (Thu)"` | `"001 (Do.)"` | `"001 (jeu.)"` |

`LC_ALL=C` is safe for all three — C and `en_US.UTF-8` give byte-identical Emacs
output for `%A %B %a %b %p %c`. Left as they are, and named here rather than
"fixed", because the frozen values are what elisprs structurally produces: it
hardcodes the English tables and has no `system-time-locale`.

Still open from the sweep, each verified against 30.2 and none of them
locale-*dependent* in elisprs:

- `%x` and `%X` are unimplemented — `(format-time-string "%x" 0 t)` answers
  `"%x"` where Emacs answers `"01/01/1970"` (and `"01/01/70"` under `LC_ALL=C`,
  the only C-vs-en_US drift found anywhere). `%Ec %EX %Ex` and `%Om %Od`
  likewise.
- `system-time-locale`, `system-messages-locale` and `locale-coding-system` are
  void; Emacs binds all three (the first two nil, the third `utf-8-unix` — and
  `nil` under `LC_ALL=C`).
- `locale-info` is void.
- `(current-time-zone 0)` answers `(-18000 nil)` where Emacs names the zone,
  `(-18000 "EST")`. `(current-time-zone 0 t)` agrees: `(0 "UTC")`.
- `(string-collate-equalp "a" "A" nil t)` is t here, nil on this Emacs — but this
  macOS build has no working collation at all (its own docstring says so, and it
  ignores an explicit LOCALE argument), so that row is darwin-scoped. On a glibc
  box the divergence would invert and `string-collate-lessp` on non-ASCII would
  become locale-sensitive in Emacs while staying `string-lessp` here. **Not
  verified from this machine.**

### R18-F. `capitalize` / `upcase-initials` — ✅ FIXED

See the CHANGELOG entry. Measured character by character over the whole BMP
(55,295 code points, `(capitalize (string C))` and `(upcase-initials (string C))`
under both engines): **0 remaining behavioral differences.**

### R18-G. Emacs 30.2's case tables lag Unicode — NOT FIXED, measured

Sweeping `(upcase C)` and `(downcase C)` over every code point:

| range | code points | where elisprs maps a character Emacs 30.2 leaves alone |
| --- | --- | --- |
| BMP (1–0xD7FF) | 55,295 | 19 |
| SMP (0x10000–0x1FFFF) | 65,536 | 94 |

and the same sweep for `capitalize`/`upcase-initials` leaves 8 in the BMP, all a
subset of the 19. Examples: U+0131 ı, U+017F ſ, U+019B ƛ, U+0264 ɤ, U+212A KELVIN
SIGN, the U+A7Bx Latin Extended-D additions, and in the SMP the Vithkuqi, Garay
and Old Hungarian blocks. There is no logic to fix: Rust's Unicode tables are
newer than the `uni-*.el` tables this Emacs was built with, and some entries
(ı → I) Emacs deliberately omits because its case table is a *pairing* and i → I
already owns the target. Freezing an exclusion list would pin this tree to one
Emacs build's Unicode version, so it is recorded rather than encoded.

Reproduce:

```sh
emacs -Q --batch --eval '(dotimes (c 55296) (princ (format "%d %d %d\n" c (upcase c) (downcase c))))' > /tmp/a
# `elisp -e' prints the expression's own value too, so drop the trailing `nil'.
elisp -e '(dotimes (c 55296) (princ (format "%d %d %d\n" c (upcase c) (downcase c))))' \
  | grep -v '^nil$' > /tmp/b
diff /tmp/a /tmp/b | grep -c '^<'
```

### R18-H. The initial buffer's syntax table did not survive a cache hit — ✅ FIXED

The worst thing found this round, and exactly the failure mode the cache rules
exist for. The prelude ends with `(set-syntax-table emacs-lisp-mode-syntax-table)`,
modelling `emacs -Q --batch` starting in `*scratch*` under `lisp-interaction-mode`.
That is a **buffer-local** binding, and buffer locals live in the buffer struct,
not in the arena — so the heap image does not carry it, and a cache hit, which
skips the prelude entirely, left the initial buffer on `standard-syntax-table`:

```text
$ elisp probe.el   # run 1, cold
top buffer="*scratch*" table-is-standard=nil char-syntax(.)=95
$ elisp probe.el   # run 2, warm
top buffer="*scratch*" table-is-standard=t   char-syntax(.)=46
$ emacs -Q --batch -l probe.el
top buffer="*scratch*" table-is-standard=nil char-syntax(.)=95
```

Every syntax-derived observable followed: `char-syntax`, `\sC` regexps,
`skip-syntax-*`, `forward-word`, and this round's whole scanner family answered
the `--script` column on a warm cache and the `-l` column on a cold one. It also
defeated the twice-per-example guard in `scripts/run_examples.sh`, and it is why
`examples/char-syntax-tables.el` was observed both passing and failing.

Fixed in `install_entry_point_state`, the one place both cache paths run through
— the same treatment `command-line-args` got in R17-I, and for the same reason:
per-run state must never be reconstructed from the image. Verified over three
consecutive runs of the same cached file on both entry points.

### R18-I. `when` / `unless` with an EMPTY body expanded wrongly — ✅ FIXED

subr.el's `when` has two arms, and only the first was ported:

```elisp
(defmacro when (cond &rest body)
  (if body (list 'if cond (cons 'progn body))
    (macroexp-warn-and-return … (list 'progn cond nil) '(empty-body when) t)))
```

so `(macroexpand '(when x))` is `(progn x nil)` in Emacs 30.2 and was
`(if x (progn))` here; `(macroexpand '(unless x))` is `(progn x nil)` and was
`(if x nil)`. The value is the same either way, so nothing but `macroexpand`
could see it — which is precisely why it survived. (The warning itself is not
emitted: `macroexp-warn-and-return` needs the byte-compiler's diagnostic
channel, which this tree does not model.)

### R18-K. Differential fuzz, after the round's changes

`scripts/fuzz_parity.sh -n 1200 -s 18` and `-n 1500 -s 181`, against the pinned
GNU Emacs 30.2 oracle. The second seed found one regression from this round's
`capitalize` work and nothing else:

```text
#1431  (string-width (capitalize -1))
  emacs: !(wrong-type-argument char-or-string-p -1)
  elisp: !(wrong-type-argument characterp -1)
```

Routing a *character* argument through the title-case table first reported
`characterp` where `upcase` reports `char-or-string-p`, and would also have
rejected the above-range integers Emacs returns unchanged (it reads their high
bits as event modifiers). Fixed, with a regression test. Both seeds then report
**1200/1200** and **1500/1500** forms agreeing with Emacs.

### R18-J. Doc-claim audit

Every behavioral claim in README.md, CHANGELOG.md and BUGS.md was extracted and
run under both engines: **266 probes**, ~340 individual assertions.

| verdict | count |
| --- | --- |
| TRUE | 255 |
| FALSE — the code is wrong | 4 |
| FALSE — the doc is stale or was never true | 7 |

Separately, all **3,338** `assert_eq!` pairs in `tests/*.rs` were extracted and
re-run against the live oracle. 3,278 matched exactly; 55 of the 60 mismatches
are harness artifacts (a function Emacs 30.2 does not have, a cwd-relative
filesystem test, a stdout side effect, or a file whose own header says its
expectations were captured from the running interpreter rather than from Emacs).

**Five were fabricated** — in files that state "Every expectation was taken from
GNU Emacs 30.2", pinning a string no version of Emacs produces. A stale pin at
least described some real version; a fabricated one never did, and a version
gate on the oracle cannot catch it, because gating the oracle does not check
that the expectation ever came from the oracle.

| pin | claimed as Emacs 30.2 | Emacs 30.2 actually says | disposition |
| --- | --- | --- | --- |
| `tests/parity_macroexpand_intrinsics.rs:23` `(macroexpand '(when x))` | `(if x (progn))` | `(progn x nil)` | code fixed (R18-I), pin corrected |
| `tests/parity_macroexpand_intrinsics.rs:31` `(macroexpand '(unless x))` | `(if x nil)` | `(progn x nil)` | code fixed (R18-I), pin corrected |
| `tests/parity_record_type.rs:76` `(mapcar #'identity (record 'foo 1 2))` | `(wrong-type-argument sequencep #s(foo 1 2))` | `(nil t t)` — no signal | pin kept, relabelled as a deliberate divergence with its own test |
| `tests/parity_bool_vector.rs:170` `(read "#&10\"\377\3\"")` | `(10 t t t t)` | `(invalid-read-syntax "#&...")` | pin kept, relabelled; elisprs has no unibyte/multibyte string distinction |
| `tests/parity_defvar_init_and_startup_buffers.rs:83` inserting into `*Messages*` | `"hi"` | `(buffer-read-only #<buffer *Messages*>)` | pin kept, relabelled; `buffer-read-only` is bound here but unenforced |

The macroexpand file also carried a stale *provenance* note quoting only the
first arm of subr.el's `when` and calling it the whole definition — which is what
made the fabricated value look derivable. Corrected.

Stale doc claims, each re-measured and corrected in place:

- `BUGS.md` R3-A claimed the named-character escape was fixed; `?\N{U+41}` is 65
  in both, but `?\N{LATIN SMALL LETTER A}` is 97 in Emacs and
  `(unsupported-character-name …)` here — the Unicode name table is not carried.
- `BUGS.md` R5-U's record entry and `CHANGELOG.md`'s copy said `mapcar` signals
  `sequencep` for a record. It does here; Emacs does not.
- "Still open after round 9" listed `string-version-lessp`'s ordering; round 10's
  `filevercmp` port closed it and the list was never updated. Both engines now
  answer `t` for the cited probe.
- "Still open after round 11" listed `(make-symbol "")` printing `\#\#`. Both
  engines print `##`.
- Round 17's "still open" listed `buffer-file-name` as void. It is bound and
  answers `nil`, as Emacs's does in batch. `buffer-modified-p` really is void.
- Round 16's note on `examples/char-syntax-tables.el` said the example "passes
  under `emacs -Q --batch -l`". It **fails** — see below.
- `README.md`'s `condition-case` example showed `arith-error`'s data as
  `(arith-error division by zero)`. It is `(arith-error)`; that string is the
  *message*, never the data.

Twenty `/// Port of` doc comments name a concrete C function. Every one was read
against its body and twelve were probed behaviorally. **No stub, no rename, no
body that fails to implement the function it names.** One deviation is
self-declared at `src/builtins.rs:5027`: `decode-char` for a charset Emacs
registers but this tree does not takes the unknown-charset path, so
`(decode-char 'japanese-jisx0208 13185)` signals `charsetp` where Emacs answers
nil. The adjacent phrase "exactly as Emacs does for an unknown charset" is true
only for genuinely unknown names.

## Round 19 — what the frontend hardcodes about Emacs

Reference: **GNU Emacs 30.2** (`/opt/homebrew/bin/emacs`, `emacs --version` →
`GNU Emacs 30.2`). Entry points measured: `emacs -Q --batch --eval '(prin1 FORM)'`
against `elisp -e 'FORM'`, and `emacs -Q --batch -l FILE` against `elisp FILE`
for the ERT examples. Emacs's own Lisp sources are on disk at
`/opt/homebrew/Cellar/emacs/30.2_2/share/emacs/30.2/lisp/` (1,622 `*.el.gz`); the
C sources are not, so C-level literals were checked against
`strings -a -n 6 /opt/homebrew/Cellar/emacs/30.2_2/bin/emacs-30.2` (4,856
strings) and `etc/DOC`.

### R19-A. Advising a Rust subr corrupted it process-wide — ✅ FIXED

Round 18's worst open item, and the root cause is not in `nadvice.el` at all.
`(advice-add 'car :filter-return #'1+)` did not merely fail; it left `car`
broken for the rest of the process, and `(symbol-function 'car)` raised the same
error:

```text
error: wrong-type-argument: number-or-marker-p #[nil ((get v 'defalias-fset-function))
       ((v . car) (nf . #<subr car>) (f . #<subr car>) … (function . 1+))]
```

That closure is the `gv-ref` *getter* for the `(get symbol 'defalias-fset-function)`
place. `advice-add` advises the symbol's function cell first and only then calls
`(add-function :around (get symbol 'defalias-fset-function) …)`; `gv-deref` is
`(funcall (car ref))`, so by that point `car` was the advised `car` and the
getter closure went to `1+`.

**Emacs does not have this problem because its preloaded Lisp is byte-compiled**,
and the byte compiler turns a call to one of ~80 primitives into an opcode that
calls the C function directly and never consults the function cell. elisprs's
prelude is the same layer of Lisp and had been lowered as ordinary symbol calls.

The open-code set was measured rather than guessed — `bytecode.c`'s opcode table
is not in the installed tree, and disassembling is ambiguous (the compiler also
*source*-inlines `error`, `add-to-list`, `eql`, …). The probe asks the question
that actually matters: byte-compile `(lambda (a…) (NAME a…))`, `advice-add` NAME
with an `:around` that sets a flag, call it, read the flag.

| verdict | count | examples |
| --- | --- | --- |
| advice does NOT fire from byte-compiled code | 82 | `car` `cdr` `nth` `memq` `aref` `length` `+` `=` `list` `concat` `substring` `funcall` `insert` `goto-char` `point` `upcase` `string=` … |
| advice DOES fire | 30 | `assoc` `eql` `safe-length` `symbol-function` `set` `append` `vectorp` `functionp` `boundp` `put` `format` `message` `intern` `vector` `signal` `error` `mapcar` `beginning-of-line` … |

`compiler::OPEN_CODED` is exactly the first list. It applies **only while the
prelude is being lowered** (`host::prelude_compiling`), because a user's
*interpreted* `defun` in Emacs does honour advice on `car` — measured:
`(progn (defun my-g (x) (car x)) (advice-add 'car :filter-return #'1+) (my-g '(1 2)))`
is `2` in both engines now. Where the name is open-coded the compiler loads the
subr *value* as the call's operator instead of the symbol, which is what the
opcode does.

```text
(advice-add 'car :filter-return #'1+) then (car '(1 2))        emacs 2        elisp 2
… (list (car '(1 2)) (cdr '(1 2)) (nth 0 '(5 6)) (assq 'a …))  emacs (2 (2) 5 (a . 1))  elisp same
(advice-add 'length :filter-return #'1+) (length "abc")        emacs 4        elisp 4
(advice-remove 'car #'1+) (car '(1 2))                         emacs 1        elisp 1
(advice-add 'message :filter-return #'upcase) (message "hi")   emacs "HI"     elisp "HI"
```

Cold/warm regression (`~/.elisprs/scripts.rkyv` removed, then two more runs):
identical on all three, and `char-syntax`/`buffer-name` still answer 95 and
`*scratch*` on both paths.

### R19-B. Hardcoded-reference-string audit

776 candidate literals extracted from `src/` (373 Rust message literals, 111
`prelude.rs` `error`/`user-error` strings, 42 `define-error` messages, 250
prelude docstring first lines); **751 checked** against the Lisp corpus, the
binary's string table or `etc/DOC`, of which **131 were additionally executed
side by side**. **40 sites / 30 distinct texts were wrong.** Every one was
re-measured independently before being touched; **35 of 35 re-measurements
reproduced**. Fixed this round:

| what | was | Emacs 30.2 |
| --- | --- | --- |
| `oclosure--get`/`--set`/`--copy`/`--fix-type` (4 sites) | `(Wrong\ type\ argument "closurep")` | `(cl-assertion-failed (closurep oclosure))` |
| `read "("` / `"[1"` / `"\"ab"` / `"?"` (11 sites) | `(error "unclosed list")` … | `(end-of-file)` |
| `read ")"` / `"]"` | `(error "unexpected )")` | `(invalid-read-syntax ")")` |
| `read "#z"` / `"#"` / `"(#)"` | the *symbol* `\#z` | `(invalid-read-syntax "#z")` |
| `read "#&3"` | `(error "expected packed string …")` | `(invalid-read-syntax "#&")` |
| `read "##"` | symbol named `##` | the interned empty-name symbol |
| `read "#:foo"` | symbol named `#:foo` | an *uninterned* symbol named `foo` |
| `setf 5` / `cl-incf 5` (3 sites, two disagreeing texts) | `(error "setf: unsupported place: %S")` / `(error "setf: unsupported place %S")` | `(gv-invalid-place 5)` |
| `define-error 'cyclic-variable-indirection` | `"Cyclic variable indirection"` | `"Symbol's chain of variable indirections contains a loop"` |
| `(defvaralias 'q1 'q1)` data | `(cyclic-variable-indirection "q1")` | `(cyclic-variable-indirection q1)` |
| `char-table-range` / `set-char-table-range` (3 sites) | `'char-table-range'` | `‘char-table-range’` |
| `setcar` `setcdr` `unintern` `gethash`/`puthash`/`remhash` `buffer-local-value` | `(wrong-type-argument consp)` — no offender | `(wrong-type-argument consp 5)` |
| `goto-char` | `integerp` | `integer-or-marker-p`, and it returns POSITION *as given* (a marker stays the marker) |
| `forward-char` / `backward-char` | `integerp`, and a float/marker was silently accepted | `fixnump` |
| `char-equal` | `integerp`, and `1.5`/`-1`/`#x400000`/a marker were accepted | `characterp` |
| `make-char-table` with a bad `char-table-extra-slots` | `(error "Invalid number of extra slots")` — absent from every Emacs corpus | `(args-out-of-range 99 nil)` |
| `pcase-exhaustive` | `No clause matching 5` | `No clause matching ‘5’` |
| `map-put!` on a growing alist | `(error "Cannot modify map in-place: …")` | `(map-not-inplace ((1 . 2)))` |
| `rx` (5 texts) | `rx: unknown form %S` … | `Unknown rx form ‘zzz’`, `Unknown rx symbol ‘zzz’`, `Unknown rx syntax name ‘zzz’`, `Bad rx expression: %S`, `Bad rx operator ‘97’`, `Illegal argument to rx ‘not’: %S` |
| `(rx-to-string '(not 5))` | signalled | `"[^\5]"` — `(not CHAR)` is legal |
| `fset` on a non-symbol | `(fset "not a symbol")` | `(wrong-type-argument symbolp 5)` |
| `(get-buffer-create "")` / `(generate-new-buffer "")` | made a buffer named `""` | `(error "Empty string for buffer name is not allowed")` |
| `(format "%")` / `(format "abc%")` | returned `"%"` / `"abc%"` | `(error "Format string ends in middle of format specifier")` |
| `(buffer-substring 1 999)` | clamped, answered the whole buffer | `(args-out-of-range #<buffer zb> 1 999)` |
| `(replace-match "x")` with stale match data | **panicked the interpreter thread** (`range start index 8 out of range for slice of length 3`) | `(args-out-of-range …)` |
| `(replace-match "X" nil nil "abc" 5)` | `(args-out-of-range no such subexpression)` | `(error "replace-match subexpression does not exist" 5)` |

Two structural repairs came out of it. `make_error_object` now re-reads DATA as
values for `cl-assertion-failed` and `cyclic-variable-indirection` as well; and
`ElispHost::signal_wrong_type` carries the offending value as an **object**
rather than rendering it into the message — a marker, buffer or closure prints
as `#<…>` / `#[…]`, which the reader rejects, so the datum used to be dropped
entirely and the condition came back as `(wrong-type-argument fixnump)` with no
offender.

The round's named defect shape — an older wording frozen in source while a
sibling path emits the current one — appears three times: the four
`Wrong type argument: closurep` sites next to ~40 correct
`wrong-type-argument: PRED VALUE` sentinels; `reader.rs:210`/`:215` emitting
`invalid-read-syntax:` while nine sibling branches emitted lowercase prose; and
the two `setf` paths that disagreed *with each other* over a colon while
`gv-get` three thousand lines away already signalled `gv-invalid-place`.

**Verified correct, against expectation:** 41 of 42 `define-error` messages; 83
of 111 prelude `error` strings appear verbatim in Emacs 30.2's own sources; every
printed form (`#<subr car>`, `#[(x) (x) (t)]`, `#s(hash-table)`, `##`,
`#<marker in no buffer>`, `#&3""`); and
`"Memory exhausted--use C-x s then exit and restart Emacs"`, which *looks* like
frozen pre-substitution text (the binary only contains the `M-x
save-some-buffers` template) but is what `substitute-command-keys` produces at
run time.

### R19-C. Name-lookalike mapping sweep

| family | probes | disagreed | note |
| --- | --- | --- | --- |
| `/` `%` `mod` on negatives, floats, zero divisors, bignums, `most-negative-fixnum` | 50 | 0 | `%` truncates, `mod` floors — both confirmed, including `(mod -7 2)`=1 / `(% -7 2)`=-1 and the bignum and infinity cases |
| `round`/`truncate`/`floor`/`ceiling`, with and without a divisor, plus `fround`&co | 58 | 0 | half-to-even confirmed at `.5` for both signs and through a divisor |
| `abs` at `most-negative-fixnum`, `-0.0`, NaN, bignum; `max`/`min` on NaN and mixed int/float | 39 | 0 | `(abs most-negative-fixnum)` promotes to a bignum in both |
| `string-lessp` / `string>` / `compare-strings` / `string-distance` / sorting, incl. non-ASCII and embedded NUL | 40 | 0 | |
| `string-to-number` prefix parsing and BASE | 74 | 3 → 0 | see below |
| float printing shortest-round-trip (hand-picked corners) | 30 | 0 | |
| float printing, 4,000 random literals through `number-to-string` | 4,000 | 0 | seeded `srand(6)`, mantissa 1–17 digits, exponent −20…+20 |
| `upcase`/`downcase` as **strings** over the BMP (1–0xD7FF) | 55,295 | 19 | |
| `upcase`/`downcase` as **strings** over the SMP (0x10000–0x1FFFF) | 65,536 | 94 | |

The `string-to-number` failures were real and are fixed:

- **Leading whitespace.** Emacs skips exactly SPC and TAB. `trim_start` skipped
  the whole Unicode set, so `(string-to-number "\n12")` answered 12 where Emacs
  answers 0 — likewise `\r`, `\f`, `\v`, U+00A0 and U+3000.
- **BASE is `CHECK_FIXNUM`.** `(string-to-number "1" 'a)` said `integerp`, not
  `fixnump`; `(string-to-number "1" 2.0)` *truncated the float to base 2* and
  answered 1; a bignum and a marker were likewise mis-typed.

The case sweep confirms the string forms behave exactly like the character forms
already recorded in R18-G: the diverging code points are **the same 19**, set
for set (`comm -3` over the two sorted lists is empty), and they are the
Unicode-table-lag set, not a lookalike. Notably absent from the diff are the
full-case-mapping traps — `(upcase "ß")` is `(83 83)`, `(downcase "İ")` is
`(105 775)`, `(upcase "ΐ")` is `(921 776 769)` — identical in both engines, so
elisprs is not falling through to Rust's simple per-`char` mapping.

### R19-D. ERT ran tests in definition order; Emacs runs them by name — ✅ FIXED

Ten deliberately unsorted names:

```text
defined: q7 b2 z9 a1 m5 c3 y8 d4 x6 e0
emacs:   a1 b2 c3 d4 e0 m5 q7 x6 y8 z9
elisprs: q7 b2 z9 a1 m5 c3 y8 d4 x6 e0   (before)
```

This is not cosmetic: it let an example test depend on an earlier test's side
effects and pass here while failing under `emacs -Q --batch -l`.
`examples/script-demo.el`'s cleanup test was exactly that, and it is now
self-sufficient.

### R19-E. Assertions that could not fail

Swept 45 `tests/*.rs` files (672 `#[test]` fns, 3,633 assertion macros, all 42
bare `assert!`s read individually) and 71 `examples/*.el` (400 `ert-deftest`,
1,470 `should` forms). Zero hits for `assert!(true)`, `is_ok() || is_err()`,
`len() >= 0`, `assert_eq!(x, x)`, or a helper that swallows a failure — every
shared helper panics. Nine assertions were strengthened; none deleted:

| site | why it could not fail | now |
| --- | --- | --- |
| `examples/temporary-file-directory.el` | the whole body sat inside `(when tmp …)`; with `TMPDIR` unset — every container CI image — it ran **zero** `should` forms and reported PASS, so a hardcoded `"/tmp/"` was invisible | `skip-unless`, so the harness counts a skip, plus a `file-directory-p` assertion that runs on both branches |
| `examples/script-demo.el:64` | `(should t)` after deleting two files: a `delete-file` that no-opped, or a wrong path so nothing was ever deleted, passed identically | asserts both files exist, deletes, asserts both are gone |
| `examples/coding-systems.el` | expected value was `(aref (coding-system-eol-type cs) N)` — the very expression the implementation evaluates, so 369 assertions compared the code to itself | expectation spelled from `(coding-system-base cs)`, plus `(should (= checked 123))` so the `when` cannot become a never-taken branch |
| `examples/secure-hash-algorithms.el` | `(secure-hash 'sha256 "aabcd" 1 4)` compared against `(secure-hash 'sha256 "abc")` — a `secure-hash` ignoring its STRING passed; sha256 was the one algorithm never pinned to a literal | both pinned to the FIPS-180 digest |
| `examples/version-compare.el` | four untyped `should-error`s under a docstring naming a distinction they cannot observe | pinned to the four exact condition objects |
| `examples/byte-run-defs.el` (2) | two *different* rejections both checked as "some error" | pinned to Emacs's exact strings |
| `examples/prelude-audit.el` | untyped `should-error` on `(number-sequence 1 10 0)` | pinned to `"The increment can not be zero"` |
| `examples/stdlib.el` | `(should (string= "ab" "ab"))` with no negative case: a `string=` returning t unconditionally passed | negatives added, plus the symbol-name case |
| `tests/intercepts.rs:122` | `>= 2` on a counter whose whole point is that the guard stops over-firing; 4, 8, 40 all passed | `assert_eq!(out, "2")` |
| `tests/dap_integration.rs` (3) | three "never stops" tests asserted only `terminated` + stdout; `run_to_end` *discards* a `stopped` event, so an adapter announcing a bogus stop was indistinguishable | a `stops_seen` counter on the session, asserted `== 0`, `== 1`, `== 2` |
| `tests/ffi.rs` (2) | two of three FFI tests returned silently with no output, so a green run could not be told from a suite that never ran | a named skip that prints, and fails outright under `ELISPRS_REQUIRE_RUSTC=1` |

### Still open after round 19, with the evidence

- **`(match-data)` at startup differs.** Emacs answers `(0 3)`, elisprs `(9 10)`
  — each an artifact of its own startup. It is observable through
  `replace-match` with no intervening search: `(replace-match "x" nil nil "ab")`
  is `(args-out-of-range 0 3)` there and `(args-out-of-range 9 10)` here (both
  signal now; before this round elisprs panicked). Pinning Emacs's pair would be
  freezing an artifact, so it is recorded instead.
- **`(read "#[1 2]")`** is `(invalid-read-syntax "#[")` here and
  `(invalid-read-syntax "Invalid byte-code object")` in Emacs: elisprs has no
  reader for `#[…]` at all, even though it *prints* closures in that syntax.
- **`(read "# ")`** loses the trailing space from the datum (`"#"` vs `"# "`) —
  `make_error_object` trims the rendered message.
- **`(car-safe)`** reports the prelude closure where Emacs reports
  `#<subr car-safe>`; `car-safe` is Lisp here and C there.
- **`(kbd "C-")`** is `[C-]` here, `"C-"` in Emacs.
- **A macroexpansion-time signal escapes an enclosing `condition-case`.**
  `(condition-case e (cl-incf 5) (error e))` catches `(gv-invalid-place 5)` in
  Emacs; here the expansion runs before the handler is established and the error
  reaches top level. The *condition* is right now; the timing is not.
- **`ert` runs a test body in the current buffer; Emacs runs it in ` *temp*`.**
  See the round-18 entry; still open, and see the verdict below.
- **`condition-case` matches a handler symbol that carries no
  `error-conditions`.** `(condition-case nil (signal 'my-err '(1 2)) (my-err
  'caught) (t 'top))` is `top` in Emacs and `caught` here.
- **`(pcase nil (nil 'n))`** answers `n` here; Emacs signals
  `Unknown pattern ‘nil’`.
- **`(regexp-opt '("a" "b"))`** is `"[ab]"` in Emacs and `"\\(?:a\\|b\\)"` here.
- **`unibyte-string` is void.**
- **`%x` / `%X` / `%Ec` / `%EX` / `%Ex` / `%Om` / `%Od`** in
  `format-time-string`, and `locale-info`, `system-time-locale`,
  `system-messages-locale`, `locale-coding-system` — see R18-E.
- **61 prelude docstring first lines do not match Emacs's**, but nothing can
  print them: `documentation`, `describe-function` and `documentation-property`
  are all void here. Latent, not observable.

### Still open after round 18, with the evidence

- **`ert` runs a test body in the current buffer; Emacs runs it in ` *temp*`.**
  `ert--run-test-internal` (ert.el:796) wraps the body in
  `(with-temp-buffer (save-window-excursion …))`. Measured:

  ```text
  emacs:   TOP buf="*scratch*" cs.=95   BODY buf=" *temp*"  cs.=46
  elisprs: TOP buf="*scratch*" cs.=95   BODY buf="*scratch*" cs.=95
  ```

  The fix is two lines. It is declined this round because it changes the current
  buffer for every `ert-deftest` body in all 71 examples, and measuring that
  blast radius costs a full `cargo test --test examples` run (~70 minutes at ~60
  seconds per example, dominated by the rkyv shard, not by evaluation) — twice,
  for a before and an after. Landing it blind could break the example gate.
  `examples/char-syntax-tables.el` is the one example known to depend on it, and
  its pinned `95` is wrong in the same place: Emacs fails that test.
- **Advising a Rust subr corrupts it.** Closed in round 19 — see R19-A. The
  claim in `README.md`, `CHANGELOG.md` and R5-U that the combinators are
  verified against 30.2 was true only of the `defun` case until then.
- **`condition-case` matches a handler symbol that carries no
  `error-conditions`.** `(condition-case nil (signal 'my-err '(1 2)) (my-err
  'caught) (t 'top))` is `top` in Emacs — `my-err` has no condition list, so the
  handler cannot match — and `caught` here.
- **`(pcase nil (nil 'n))`** answers `n` here; Emacs signals
  `Unknown pattern ‘nil’`.
- **`(regexp-opt '("a" "b"))`** is `"[ab]"` in Emacs and
  `"\\(?:a\\|b\\)"` here.
- **`unibyte-string` is void**, which is what `BUGS.md`'s round-13 note on
  `(string-width (unibyte-string 200))` actually measures.
- **`%x` / `%X` / `%Ec` / `%EX` / `%Ex` / `%Om` / `%Od`** in
  `format-time-string`, and `locale-info`, `system-time-locale`,
  `system-messages-locale`, `locale-coding-system` — see R18-E.
- **README's subr count** was given as "~90" in one place and "~80" in another,
  both stale. Round 19 removed both rather than typing a third: a hand-kept
  count goes stale again, so README names the one-liner that computes it.
