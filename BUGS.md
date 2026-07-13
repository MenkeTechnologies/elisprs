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

Reproduce any of it:

```sh
bash scripts/fuzz_parity.sh -n 2000 -s 1      # seeded: same corpus every run
bash scripts/fuzz_parity.sh -c corpus.el      # re-check a saved corpus
```

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
`#s(NAME …)`, `type-of`/`recordp`/`cl-struct-p` recognize them — but `vectorp` is
still t since they're vectors underneath). `cl-member`/`cl-assoc`/`cl-find`/
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

### 1. No bignum support — silent integer overflow (⚠️ NOT a pure-frontend fix)
- `(expt 2 100)` → Emacs `1267650600228229401496703205376`, elisprs `0`
- `(* 1000000000000 1000000000000)` → Emacs `1000000000000000000000000`, elisprs `2003764205206896640`
- Integer ops wrap i64 instead of promoting to bignum.
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

### R2-A. Arithmetic silently coerces non-numbers instead of signaling
- `(+ 1 "a")` → Emacs signals `(wrong-type-argument number-or-marker-p "a")`, elisprs `1.0`
- `(* 2 "x")` → Emacs signals, elisprs `0.0`; `(+ 1 'sym)` → elisprs `1.0`
- Most dangerous: arithmetic on bad data never errors and returns silent wrong numbers.

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

### R2-D. Float printer doesn't use exponent form for large / small magnitudes
- `(prin1-to-string 1e20)` → Emacs `"1e+20"`, elisprs `"100000000000000000000.0"`
- `1e15`→`"1e+15"` vs `"1000000000000000.0"`; `1.5e-10`→`"1.5e-10"` vs `"0.00000000015"`
- Affects `prin1`, `number-to-string`, `format "%s"`. (Distinct from the inf/NaN entries.)

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

### R2-H. `mapcar` (and `seq-map`) reject vector/string sequences
- `(mapcar #'1+ [1 2 3])` → Emacs `(2 3 4)`, elisprs `error: mapcar: not a list`
- `(mapcar #'1+ "abc")` → Emacs `(98 99 100)`, elisprs errors. (Broader than #22.)

### R2-I. ✅ FIXED — `seq-empty-p` wrong on the empty string
(prelude: `(= 0 (length l))` so vectors/strings count too)
- `(seq-empty-p "")` → Emacs `t`, elisprs `nil`

### R2-J. ✅ FIXED — `string-blank-p` returns `t` instead of the match position
(prelude: `(string-match-p "\\`[ \t\n\r]*\\'" s)`)
- `(string-blank-p "  ")` → Emacs `0`, elisprs `t`

### R2-K. `string-pad` 4-arg form (PADDING + START) unsupported
- `(string-pad "ab" 5 ?* t)` → Emacs `"***ab"`, elisprs `error: wrong-number-of-arguments`

### R2-L. `make-hash-table` print format diverges from Emacs 30
- `(make-hash-table)` → Emacs `#s(hash-table)`, elisprs `#s(hash-table size 0)`

### R2-M. `assoc` TESTFN (3rd arg) unsupported
- `(assoc 2 '((1 . 10) (2 . 20)) #'=)` → Emacs `(2 . 20)`, elisprs `error: wrong-number-of-arguments`

### R2-N. Emacs-30 `sort` keyword API (`:key`/`:lessp`) unsupported
- `(sort '(3 1 2) :key #'- :lessp #'<)` → Emacs `(3 2 1)`, elisprs `error: void-function: :key`

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

### R3-A. String/char `\` escapes: named-control, hex, and octal all wrong
The reader's `unescape` only maps `n t r 0 e`; every other escape falls through to the
literal letter, and multi-char numeric escapes aren't consumed.
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

### R3-E. `format` `%x`/`%o` on negatives print two's-complement, not signed
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

### R3-J. `char-equal` ignores `case-fold-search`
- `(char-equal ?a ?A)` → Emacs `t` (case-fold defaults t in batch), elisprs `nil`

### R3-K. `signal`/`condition-case` stringify the entire error DATA list
- `(condition-case e (signal 'my-err '(a b)) (t (cdr e)))` → Emacs `(a b)`, elisprs `("(a b)")`
- General form of R2-B/R2-C: any signalled DATA is collapsed to one printed string, so
  every handler reading `(cdr e)` gets garbage — even user `signal`.

### R3-L. Hex reader rejects values above i64 range (hard error)
- `#xFFFFFFFFFFFFFFFF` → Emacs `18446744073709551615`, elisprs `error: invalid digits for
  base 16` (a reader error variant of the round-1 #1 bignum gap)

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

### R4-A. `letrec` is broken
- `(letrec ((a 1) (b (+ a 1))) (list a b))` → Emacs `(1 2)`, elisprs `error: …void: a`
- `(letrec ((f (lambda (n) (if (= n 0) 1 (* n (funcall f (1- n))))))) (funcall f 5))` →
  Emacs `120`, elisprs `error: invalid-function`. Forward/self references don't resolve
  (`letrec` not in `src/prelude.rs`).

### R4-B. `if-let` / `when-let` only bind the FIRST clause of a multi-binding list
- `(if-let ((a 1) (b 2)) (+ a b) 'no)` → Emacs `3`, elisprs `error: …void: b`
- `(if-let (a 1) a)` (single var-form) → Emacs `1`, elisprs `error: wrong-type-argument: listp a`
- Macros at `src/prelude.rs:368-373` use only `(car binding)`. (Round 2's single nested-binding
  case passes; the multi-binding and short forms don't.)

### R4-C. `if-let*` / `when-let*` / `and-let*` undefined
- `(when-let* ((a 1) (b 2)) (+ a b))` → Emacs `3`, elisprs `error: void-function: b`
  (the `*` variants aren't defined, so `b` evaluates as a call)

### R4-D. `seq-let` is broken
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

### R4-H. `format` `%e` uses wrong exponent format and drops default precision
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
