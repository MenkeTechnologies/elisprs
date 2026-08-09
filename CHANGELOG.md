# Changelog

All notable changes to elisprs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions track `Cargo.toml`.

## [Unreleased]

### Fixed
- **A subr's arity is checked before its arguments are evaluated.** `compile_call`
  emitted the argument code ahead of the `CALL` op and arity was enforced in
  `call_function`, so a wrong-arity call to a builtin ran its arguments' side
  effects first: `(let ((x 0) (y 0)) (ignore-errors (car (setq x 1) (setq y 2))) (list x y))`
  was `(1 2)` where Emacs 30.2 gives `(0 0)`, and
  `(condition-case e (car (error "inner") 2) (error e))` returned the argument's
  own error instead of `(wrong-number-of-arguments car 2)`. A `CHECK_ARITY`
  extension op now precedes the argument code; it resolves the live function cell
  and signals only when that cell holds a subr rejecting the count. The rule is
  subr-only — Emacs evaluates a *closure's* arguments before checking its arity
  too — and the verdict is taken at run time, so `fset` and `defalias` still
  retarget a symbol between compilation and the call in both directions:
  pointing `car` at a two-argument lambda keeps `(car 1 2)` legal, and
  `(fset 'myfn (symbol-function 'car))` then `(myfn 1 2)` signals without
  evaluating either argument. Verified across the interpreter, a warm bytecode
  cache and `--aot-exe` (caught and uncaught). `cache::SHARD_FORMAT_VERSION`
  6 → 7, since a v6 chunk carries no guard and would otherwise keep serving the
  old behaviour from a warm cache.

### Added
- **Real `record` type (`Obj::Record`).** Records (and every `cl-defstruct`
  instance) were `cl-struct-NAME`-tagged vectors, so `(aref (record 'foo 1 2) 0)`
  leaked `cl-struct-foo` instead of `foo`, and `(vectorp REC)` was wrongly `t`.
  There is now a distinct `Obj::Record` heap kind whose slot 0 holds the bare type
  symbol: `record` / `make-record` are primitives; `aref` / `aset` / `length` /
  `equal` / `copy-sequence` / `type-of` / `recordp` / the printer (`#s(…)`) and the
  `#s(NAME …)` reader all handle it; `vectorp` is `nil` and a record is not a
  sequence (`vconcat` / `append` / `mapcar` signal `sequencep`). `cl-defstruct`
  builds records tagged by bare NAME, with the predicate walking a bare-name parent
  chain. Matches Emacs 30.2 byte-for-byte.
- **`bool-vector`.** Added `Obj::BoolVector`, `make-bool-vector` / `bool-vector`,
  the `#&N"…"` reader + printer (LSB-first byte packing with print.c escaping),
  `aref` / `aset` (`t`/`nil`; any non-nil stored as `t`) / `length` / `elt`,
  `bool-vector-p`, `arrayp` / `sequencep` membership (a bool-vector is an array and
  a sequence but not a vector), and `bool-vector-count-population` / `-subsetp` /
  `-not`. Matches Emacs 30.2 including the `wrong-length-argument` data shapes.
- **`nadvice` (source-symbol advice).** Faithful port of `emacs-lisp/nadvice.el`
  (Emacs 30.2): `advice-add` / `advice-remove` / `add-function` / `remove-function`
  / `define-advice` / `advice-member-p`, with all ten combinators (`:around`
  `:before` `:after` `:override` `:filter-args` `:filter-return` `:before-while`
  `:before-until` `:after-while` `:after-until`) and `depth` ordering. Advices are
  `advice` oclosures threaded into the symbol-function cell. Distinct from the
  glob-AOP intercept layer (`src/intercepts.rs`). Legacy `defadvice` (the separate
  `advice.el` `ad-*` subsystem) remains unimplemented.
- **Bignums.** Emacs has no fixed-width integers: an integer that leaves fixnum
  range (±2^61, `most-positive-fixnum`) becomes a bignum and stays exact. elisprs
  now has `Obj::Bignum` (num-bigint), and integers promote through the whole
  interpreter — arithmetic, `expt` / `ash` / `lsh` / `abs` / the bit ops, `/` `%`
  `mod`, the rounding family, the reader, the printer, `eq` / `eql` / `equal`,
  `sxhash`, hash-table keys, `format` `%d` / `%x` / `%o`, `number-to-string` /
  `string-to-number`. `(expt 2 70)` was `0`; `(* 1000000000000 1000000000000)`
  wrapped. `fixnump` / `bignump` are real subrs now (they were prelude stubs
  answering `(integerp x)` / `nil`).
- **Strict numeric typing.** `+` `-` `*` `1+` `1-` and the numeric comparisons are
  lowered straight to fusevm ops, whose semantics are awk's: a non-numeric operand
  coerced (`(+ 1 "a")` was `1.0`) and integer overflow wrapped. fusevm 0.14.6 adds
  a *numeric hook* — installed by `host::run_chunk` — so those ops hand the host
  every case they cannot compute natively, and elisp signals
  `(wrong-type-argument number-or-marker-p "a")` or promotes to a bignum. The
  arithmetic stays JIT-compiled: the checked lowering folds the overflow and
  fixnum-range tests into one accumulator with no branch on the hot path.
- **Differential fuzz harness** (`scripts/fuzz_parity.sh`, `scripts/fuzz/`):
  generates a seeded corpus of random elisp forms, evaluates every form under both
  `emacs -Q --batch` and `elisp` through one shared driver, and reports each form
  whose value — or whose signalled error — differs. Everything below was found
  with it.

### Fixed (round 11 — the AOT path failed in total silence)
- **An uncaught elisp error on the AOT path exited 0 printing nothing.** An elisp
  error never becomes a fusevm `VMResult::Error`: `host::abort` parks the message
  in the thread-local host error slot and winds `vm.ip` past the last op, so the VM
  terminates the way a program that ran off the end does. `host::run_chunk` reads
  that slot; fusevm's AOT driver lives in fusevm and cannot, so it mapped the clean
  termination to exit 0. A standalone `--aot-exe` binary for `(error "boom")`
  produced **0 bytes of stdout, 0 bytes of stderr, exit 0** — byte-identical to a
  successful run of a program that prints nothing, so no caller could tell it had
  failed. It applied to every uncaught error, not a corner case.
  `elisprs_aot_finish` (`src/aot_runtime.rs`) now reads the slot after the run and
  reports it with the interpreted driver's exact message and exit status
  (`error: boom`, exit 1), so both paths fail identically. A handled error
  (`condition-case` / `ignore-errors`) is consumed by the nested `run_chunk` and
  still exits 0.
- **A closure lost its printed source through the heap image.** `SerObj::Closure`
  carried the compiled body, params and captured environment but not `ClosureSrc`
  (the arglist as written and the body forms), and the importer rebuilt every
  closure with `ClosureSrc::default()`. The closure still ran, so nothing errored —
  it just printed as `#[nil () …]`. Because the rkyv script cache uses the same
  image, this was a silent wrong answer on the *default* path from the second run
  of any script onward, and identically under `--aot-exe`:
  `(prin1-to-string (lambda (x) (* x 2)))` was `#[(x) ((* x 2)) (t)]` cold and
  `#[nil () (t)]` warm, where Emacs 30.2 says the former. The image now carries
  `arglist` + `src_body`, and `cache::SHARD_FORMAT_VERSION` is bumped 5 -> 6 so
  already-written caches on disk are rejected by the header check. That bump is
  required, not hygiene: bincode reads fields positionally and ignores
  `#[serde(default)]`, so decoding a v5 closure with the v6 struct runs off the end
  of the object (measured: `io error: unexpected end of file`). Without the bump a
  closure in the middle of an image could over-read a following object and still
  yield a plausible, wrong heap. The AOT image is serde_json, which is
  self-describing, and is rebuilt with the object anyway.
- **The standard syntax table classed the control characters wrong.** C0 controls
  and DEL were punctuation (`46`) where Emacs 30.2 makes them symbol constituents
  (`95`); `?\n` was whitespace where Emacs makes it comment-end (`62`); and `?\r`
  was whitespace where Emacs leaves it a symbol constituent. `(char-syntax C)` now
  agrees with Emacs across 0–32, 127 and 160.
- **`\s-` matched a newline.** `src/regexp.rs` translated it to the `regex` crate's
  `\s`, which matches `\n`, `\r` and `\v`; Emacs reads the syntax table, where
  none of the three are whitespace, so `(string-match "\\s-" "\n")` is `nil` there
  and was `0` here — every `\s-` silently matched across line boundaries. It now
  translates to the standard table's whitespace set `[\t\x0C\x{A0} ]`, and `\S-`
  to its complement.

### Fixed (Emacs parity — round 8, exact arithmetic, argument order, filevercmp, non-finite floats)
Found by `scripts/fuzz_parity.sh` plus hand-built probe corpora, and verified by
byte-diffing stdout, stderr and exit status against `emacs -Q --batch` 30.2 on
every execution path: interpreter, cache-disabled, warm rkyv bytecode cache, and
an AOT-compiled native executable. The bignum/rounding/argument-order/filevercmp
fixes were confirmed on all four. The non-finite-float fixes were confirmed on
the three interpreted paths only — an AOT executable whose constants are not
reconstructible still exits 0 printing nothing (the constant-reification work
item noted in `src/aot_runtime.rs`), which is a pre-existing limitation
reproducible on the unmodified tree, not a regression.

- **`floor`/`ceiling`/`round`/`truncate` with a DIVISOR saturated to `i64`.**
  With a float on either side the quotient went through `apply_rm(x/y) as i64`,
  so `(floor 1e30 3)` answered `9223372036854775807` instead of
  `333333333333333339961541612885`. Emacs divides *exactly* — every finite float
  is a dyadic rational — so the two-argument forms now convert both operands to
  an exact fraction and reuse the bignum rounding division. Rounding the `f64`
  quotient is not enough either: that gives `333333333333333316505293553664`.
  The one-argument forms already promoted correctly, which is what made the
  divisor path's silence so easy to miss.
- **`+`/`-`/`*` with three or more arguments skipped later arguments' side
  effects.** They lowered to a chain of binary opcodes that folded as it
  evaluated, so a type error in argument N aborted before argument N+1 ran:
  `(let ((n 0)) (ignore-errors (* 1 t (setq n 9))) n)` left `n` at 0 where Emacs
  leaves 9, and `(* 1 t (error "boom"))` reported the wrong error. Emacs
  evaluates every argument first and folds afterwards; three-plus operands now
  go to the n-ary builtin, which the VM calls with all arguments evaluated.
- **A one-argument `+`/`*` skipped its type check.** `(+ t)` answered `t` and
  `(+ "a")` answered `"a"` instead of signalling `wrong-type-argument`, because
  the lone operand was emitted bare. It now routes through the builtin, which
  seeds its accumulator from the first argument rather than the identity — so
  the check happens *and* `(+ -0.0)` stays `-0.0`.
- **`string-version-lessp` was a byte comparison, wrong on ~50% of inputs.**
  It is gnulib `filevercmp`, whose `order()` sorts `~` first, then digits, then
  letters, then every other byte *after* the letters, with `.`/`..`/leading-dot
  names special-cased and file suffixes cut before the first pass. Comparing raw
  character codes made `(string-version-lessp "a" " ")` `nil` (Emacs: `t`) and
  `(string-version-lessp "." "9")` `nil` (Emacs: `t`). A 4,000-pair differential
  corpus went from 1,985 divergences to 0.
- **`string-to-number` did not accept the non-finite float syntax.**
  `(string-to-number "1.0e+INF")` silently answered the finite `1.0`, so any
  float round-tripped through `number-to-string` lost its infinity. Ported
  lread.c's rule: after `e`/`E` an explicit `+` may be followed by exactly `INF`
  or `NaN`.
- **NaN payloads were discarded by the reader and the printer.** Emacs stores a
  token's leading integer in the NaN's significand and prints it back, so
  `3.7e+NaN` reads and prints as `3.0e+NaN`; everything used to collapse to
  `0.0e+NaN`. The reader also only matched a lowercase `e`, so `1.0E+INF` read
  back as a *symbol*.
- **`/` and `mod` flattened the sign of a NaN operand.** Both unconditionally
  `abs()`ed a NaN result to paper over the hardware-dependent sign of a NaN the
  operation *invents* (`(/ 0.0 0.0)`). That also destroyed the sign of a NaN
  merely passing through, which IEEE propagates unchanged on every ISA:
  `(/ -0.0e+NaN -1)` is `-0.0e+NaN` in Emacs. Only invented NaNs are
  canonicalized now.
- **Short-circuiting cl-seq searches rejected improper lists.** `cl-position`,
  `cl-find`, `cl-position-if` and `cl-find-if` normalized through
  `(append SEQ nil)`, which signals `listp` up front, so `(cl-position 1 (cons 1
  t))` errored where Emacs answers `0`. They walk the list in place now, which
  also preserves the signal for a search that really does run off the end.
- **`--aot-exe` could not link on macOS.** The link line omitted `-framework
  IOKit`, which `sysinfo` needs, so every standalone AOT build died with
  "symbol(s) not found for architecture arm64" — the native path could not be
  exercised at all.

### Changed (fuzz harness)
- The batch timeout scales with corpus size instead of being a fixed 20s. It was
  under the ~30s a debug `elisp` needs for 6,000 forms, and a timeout is *not*
  visible as a divergence: both engines emit `<HANG>`, which compares equal, so a
  run that timed out reported perfect parity.
- Forms that produced no value under *either* engine are counted and reported
  separately rather than silently counting as agreement.
- Every run now prints how many forms the reference actually evaluated, and warns
  when more than 5% of the corpus is `void-function` under Emacs — those forms
  make both engines fail identically and measure nothing. (`float-to-string`,
  which exists in neither engine, was removed from the call table for this
  reason.)
- The generator emits the shapes the bugs above needed: exact-division operands
  (`1e30`, `2^100`, `9.3e18`), two-argument rounding, one- and four-argument
  arithmetic, filevercmp-relevant strings (`.`, `..`, `~`, `a.tar.gz`, `foo01`),
  non-finite float tokens, and improper lists.

### Fixed (Emacs parity — round 7, semantic areas the fuzz grammar does not generate)
`scripts/fuzz_parity.sh` reported 1/1500 on its own corpus, so these came from
hand-built differential probe corpora in the areas `scripts/fuzz/gen.el` never
emitted: `print-circle`, binding forms, the error-condition system, macro
definition order, and `cl-loop` clause shapes. The generator now emits all of
them, so the fuzzer covers this ground from here on.

- **`print-circle` did nothing, and a circular list hung the printer forever.**
  There was no notion of shared structure: `(let ((print-circle t)) (prin1-to-string
  (let ((y (list 1))) (list y y))))` printed `((1) (1))` instead of `(#1=(1) #1#)`,
  and a circular cdr chain — `(setcdr (cdr x) x)` — spun in `print_list`'s
  iterative walk appending to its output buffer until the process died (the
  `PRINT_CIRCLE` depth guard only counts *nesting*, which a cdr chain never grows).
  The printer now runs a reference-counting pre-pass when `print-circle` is
  non-nil, labels every multiply-reachable cons/vector/record `#N=` on first print
  and `#N#` afterwards, and walks cdr chains behind a Floyd tortoise so a cycle
  terminates even with `print-circle` nil. `print-circle` is also a `defvar` now —
  without it `let` bound it lexically and the Rust printer, which reads the value
  cell, never saw it.
- **`dolist` reused one binding for every iteration.** subr.el binds the variable
  with a `let` *inside* the loop; elisprs hoisted it and `setq`'d it. Observable
  under lexical binding: `(let (r) (dolist (x '(1 2 3) r) (push (lambda () x) r))
  (mapcar #'funcall r))` answered `(nil nil nil)` instead of `(3 2 1)`, because
  every closure captured the same cell.
- **An `error` handler caught `quit`.** `condition-case` treated `error` as an
  unconditional catch-all instead of consulting the signal's `error-conditions`
  chain, and `define-error` had additionally given `quit` the parent `error`.
  `(condition-case nil (signal 'quit nil) (error 'caught) (t 'top))` answered
  `caught`; Emacs answers `top`. Only `t` is a catch-all.
- **An `unwind-protect` cleanup that signalled was silently swallowed.** The
  cleanup's result was discarded, so `(condition-case e (unwind-protect (error "in")
  (error "cleanup")) (error (cadr e)))` answered `"in"` where Emacs answers
  `"cleanup"` — and every failure inside a cleanup form vanished.
- **A macro was unusable in the form that defined it.** `(progn (defmacro m …)
  (m 1))` failed with "macro called as a function": macro expansion walked the
  whole `progn` before the `defmacro` had run. `defmacro` siblings are now
  installed during expansion, which is what Emacs's byte-compiler does and what
  its interpreter gets for free by evaluating `progn` forms one at a time.
- **`cl-loop`: `while`/`until` tested the previous iteration.** They were folded
  into the `while` test, which runs *before* the `for` clause steps its variable —
  so on the first pass the variable was still nil and `(cl-loop for i in '(1 2 3)
  while (< i 3) collect i)` signalled `wrong-type-argument number-or-marker-p nil`.
  They are now emitted where they appear, exiting through a separate tag so the
  accumulator and `finally` still produce the loop's value.
- **`cl-loop`: `downfrom … to` never ran.** `to` was always `<=`, so
  `(cl-loop for i downfrom 3 to 1 collect i)` answered nil instead of `(3 2 1)`.
  `to` now takes its direction from the iteration.
- **`type-of` used the pre-Emacs-30 function type names.** `(type-of (lambda ()))`
  is `interpreted-function`, and a macro's function cell is a cons, not a function.
- **`print` omitted its leading newline.** print.c brackets the object with
  newlines; `(with-output-to-string (print 'a))` answered `"a\n"`, not `"\na\n"`.
- **`(symbol-value :keyword)` signalled `void-variable`.** A keyword is its own
  value.
- **`cl-letf` on a plain symbol read the symbol before binding it**, so
  `(cl-letf* ((x 1)) x)` signalled `void-variable x`. Symbol places now become an
  ordinary `let`, as in cl-macs.el.
- **`cl-incf`/`cl-decf` expanded through `setf`**, printing
  `(progn (setq x (+ x 1)))` where cl-lib.el gives `(setq x (1+ x))`. A duplicate
  `cl-decf` definition that shadowed the first was also removed.
- Added `cl-psetq` and `substitute-command-keys` (the quote-translation half —
  `\\[COMMAND]` needs interactive keymaps, a NAMED limitation).

The widened generator then found these on its first run (none in code this round
had touched — they are simply shapes the corpus never produced before):

- **`error-message-string` printed a non-error car as if it named a condition.**
  `(error-message-string nil)` answered `"nil"` and `(error-message-string '(t nil))`
  answered `"t: nil"`; Emacs answers `"peculiar error"` and `"peculiar error: nil"`.
- **`seq-into` silently accepted an unknown type name**, handing the input back
  where seq.el signals `Not a sequence type name: foo`.
- **`cl-rem`/`cl-mod` rejected floats.** cl-lib.el defines them as the remainder
  half of `cl-truncate`/`cl-floor`, so `(cl-rem 7.5 2)` is `1.5`; routing them
  through the integer-only `%`/`mod` signalled `wrong-type-argument
  integer-or-marker-p 7.5`.
- **`last` with a count signalled on an improper list.** It measured with `length`,
  which has none to give: `(last (cons "z" 1.5) 7)` errored where Emacs answers
  `("z" . 1.5)`. It measures with `safe-length` now.

A 51-form lexical-scoping sweep then found two more, both in the macro layer (the
underlying environment model is sound — `let`/`let*` freshness, non-leaking, and
binding restoration across `unwind-protect` / `catch`-`throw` all already matched):

- **`dotimes` shared one binding across iterations**, exactly like `dolist`:
  `(let (fs) (dotimes (i 3) (push (lambda () i) fs)) (mapcar #'funcall (nreverse fs)))`
  answered `(3 3 3)` instead of `(0 1 2)` — and `3` is the *post-loop* count, not
  even the last iteration's value. Ported subr.el's expansion, which runs the body
  inside `(let ((VAR counter)) …)` against a separate uninterned counter.
- **A nested `cl-flet` corrupted its own binding list.** The call-site rewriter
  descended into the inner form's bindings and rewrote the binding head, so
  `(cl-flet ((f (n) (* n 2))) (cl-flet ((f (n) (+ n 1))) (f 5)))` failed with
  "malformed lambda list". It now skips a nested `cl-flet`/`cl-flet*`/`cl-labels`
  binding list and shadows the names it binds for that form's body.

### Fixed (correctness)
- **The script cache could shadow a builtin.** A cache hit skips the prelude and
  re-imports a serialized heap image, which used to re-intern *every* symbol it
  carried into the global obarray — including uninterned ones (a lambda parameter,
  a `let` binding in a macro body). The prelude binds a local named `exp`, so a
  *warm* cache rebound the global `exp` to a symbol with no function cell:
  `(exp -1.0)` worked on the first run of a script and answered `void-function exp`
  on every run after. `SerObj::Symbol` now records whether the obarray maps the
  name to that symbol, and only those re-claim it. Cache format → v3.
- **A float chunk result was truncated to an integer** once the block-JIT cache
  warmed up (fusevm returned the result register as `Value::Int` unconditionally):
  the second and later `(eval 2.5 t)` in a process answered `2`. Fixed in fusevm
  0.14.6.

### Fixed (Emacs parity — round 6)
- **An improper list is reported differently depending on who walks it.**
  `memq` / `member` / `memql` / `rassq` use Emacs's `CHECK_LIST_END`, which names the
  WHOLE list — `(memq 1 (cons 9 3))` is `(wrong-type-argument listp (9 . 3))` —
  while `reverse` / `append` / `sort` name the offending TAIL. elisprs silently
  stopped at the dotted tail and answered nil.
- **Calling `t` or `nil` is `void-function`, not `invalid-function`**: both are
  symbols in Emacs, they just have no function cell. elisprs represents them as
  `Value::Bool`/`Value::Undef` rather than heap symbols, so they fell through to the
  "not a function object" arm.
- The float math primitives (`sin`, `cos`, `sqrt`, `exp`, `log`, …) signal `numberp`,
  not the arithmetic ops' `number-or-marker-p`.
- `copy-sequence` of a non-sequence signals rather than handing the value back;
  `string-width` signals `stringp`; `plist-put` validates the plist's shape and names
  the whole plist; `upcase-initials` of a character upcases it; `assoc-string` skips
  an element it cannot compare and stops at a dotted tail.

### Fixed (Emacs parity — round 5, widened fuzz grammar)
The fuzz generator now also covers hash tables, Emacs regexp syntax (valid *and*
malformed), text properties, cl-lib, the printer's dynamic variables, and `format`
directives with widths/flags. On that new surface it found:

- **`(length 'car)` answered 0** instead of signalling `sequencep` — so
  `(ash (length 'car) 5)` was a silent 0 rather than an error. (`safe-length` is
  the one that answers 0.)
- The bit-logic ops name a predicate that depends on the argument's **position**:
  Emacs's `bit_op` checks the first with a direct `CHECK_INTEGER`
  (`integer-or-marker-p`) and each later one for number-ness first, so
  `(logand "x" 2)` and `(logand 2 "x")` report *different* predicates for the same
  value.
- An array index must be a fixnum: `(elt "abc" <bignum>)` is `fixnump`, and a
  bignum index used to be coerced and reported back as a different number in the
  `args-out-of-range` data.
- `prin1-to-string` takes NOESCAPE (`(prin1-to-string "a" t)` → `a`).
- `isnan` signals `floatp` on a non-float rather than answering nil; `plist-put` /
  `plist-member` signal `plistp` (while `plist-get` stays lenient).
- The higher-order primitives (`sort`, `mapcar`, `mapc`) resolve their function
  designator before calling, so an arity error names the resolved function
  (`#<subr abs>`), and an improper list argument names its tail.

### Fixed (correctness — the warm script cache)
A cache hit skips the reader, the compiler AND the prelude, replaying cached chunks
onto a restored heap image. That is a different code path from a cold run — and the
one every user hits on the *second* run of any script. It was wrong in four ways,
all of which made a script behave differently the second time it ran:

- **The image double-applied the file's own effects.** It was captured *after* the
  file ran, so a prelude object the file mutated came back already mutated and the
  replayed chunks mutated it again — `(get 'g 'custom-group)` returned the previous
  run's entries. An order-dependent flag changed the result outright: a
  `make-variable-buffer-local` left `buffer_local_auto` set, so replaying the file's
  own `(defvar bl-y nil)` created a buffer-local binding the cold run never had and
  `local-variable-p` answered `t` instead of `nil`. The image is now the heap as it
  stood *before* the file ran; only `special` (which the compiler sets, and a hit
  does not compile) is carried forward.
- **The OClosure side table was lost.** It is built when the prelude runs, so every
  prelude OClosure came back a plain closure: `oclosure--copy: "not an OClosure"`.
- **A closure's captured environment was not serialized.** Restored closures had
  captured nothing, so the prelude's OClosure accessors signalled
  `void-variable index`.
- `scripts/run_examples.sh` now runs every example **twice** — cold, then warm.
  Running each once is what let all of this hide: `oclosure`, `mode-buffer-local`,
  `language-info-alist`, `custom-autoload` and `defcustom-decl` each passed cold and
  failed warm. All 71 now pass on both runs.

Cache format → v4.

### Fixed (correctness — round 3, from the same fuzz harness)
- **A handled error poisoned the next one.** An error travels on two channels: the
  message returns as a `Result::Err` string, while the structured object
  `(SYMBOL . DATA)` that `condition-case` binds is parked on the host. Nothing
  paired them, so an object left by an error that had *already been caught* stood
  in for the next error that only produced a message —
  `(condition-case e (progn (ignore-errors (error "boom")) (car 1)) (error e))`
  answered `(error "boom")` instead of `(wrong-type-argument listp 1)`. The object
  is now recorded with the message it belongs to and used only for that message.
- **A speculative function lookup left an error behind.** `macroexpand-1` resolves a
  form's head to ask "is this a macro?"; when the head is not a function (a `cond`
  clause's test, `((car 1) 1)`), that failed probe registered an `invalid-function`
  object which then replaced the clause's real error. Resolution is side-effect-free
  again.

### Fixed (Emacs parity — round 3)
- `min` / `max` are subrs, as in Emacs — which is what `(min)` naming `min` and
  `(seq-min (vector))` naming `#<subr min>` depend on. They check their arguments
  left to right, so `(max t 'foo)` names `t`.
- An improper list names its offending TAIL: `(reverse (cons 1 2))` is
  `(wrong-type-argument listp 2)`, not `sequencep` on the whole cons.
- `substring`'s FROM/TO must be integers (a float signalled nothing and truncated);
  `seq-take` / `seq-drop` check N before touching the sequence.

### Fixed (Emacs parity — round 2, from the same fuzz harness)
- **`(eval FORM t)` leaked the caller's lexical scope.** `t` means "lexical
  binding", not "inherit my bindings": `(let ((x 5)) (eval 'x t))` returned 5 where
  Emacs signals `(void-variable x)`. FORM now runs in an empty lexical environment,
  and a closure FORM builds no longer captures — or prints — the caller's bindings.
- **A closure prints as its source**, the way Emacs prints an interpreted function:
  `#[(x) ((list x x)) (t)]`, with the captured lexical alist (newest first) as the
  third slot. It printed an opaque `#<closure>`; elisprs lowers the body to a
  fusevm `Chunk`, so closures now retain their arglist and body forms.
- `wrong-number-of-arguments` carries `(FUNCTION COUNT)` — with the function
  *object* (`#<subr char-to-string>`, `#[(x) (x) (t)]`) when called through
  `funcall`/`apply`, which resolve the designator, and the symbol on a direct call.
  `invalid-function` carries the offending object. Both are built as real error
  objects: a closure cannot survive being rendered to a message and re-read.
- Numeric arguments are checked left to right, so `(max t 'foo)` names `t`, and
  `seq-max`/`seq-min` inherit that.
- `seq-subseq` signals out of range instead of clamping: `args-out-of-range` for an
  array, `(error "Start index out of bounds: N")` for a list, `(error "Unsupported
  sequence: X")` otherwise.
- Type contracts: `capitalize` / `upcase-initials` (`char-or-string-p`), `sort`
  (`list-or-vector-p`), `mapcar` / `mapc` (`sequencep`), `format` and `intern`
  (`stringp`), `string-equal-ignore-case` (`stringp`). And where Emacs is *lenient*
  and elisprs signalled: `(last t 0)` is `t`, `(plist-get 'sym 1)` is nil.
- `split-string` with an empty separator yields the leading and trailing `""`
  Emacs does: `(split-string "a1b" "")` is `("" "a" "1" "b" "")`.
- `ash` accepts a bignum shift count (`overflow-error` on a left shift that cannot
  be materialised, sign collapse on a right shift).

### Fixed (Emacs parity — see BUGS.md)
- Float printing follows Emacs's `float_to_string` (gnulib `dtoastr`): the shortest
  `%g` form that reads back as the same float, so `(float most-positive-fixnum)` is
  `2.305843009213694e+18` (was `2305843009213694000.0`) and `(ldexp 1.0 -1074)` is
  `5e-324`.
- Integer comparison is exact. `=` `<` `>` `<=` `>=` compared as `f64`, which runs
  out of mantissa at 2^53: `(= 2305843009213693950 2305843009213693951)` answered
  `t`.
- Error data carries the offending value and the predicate the *specific* builtin
  checks. The value used to be a raw heap handle (`(wrong-type-argument stringp
  (obj:154357))`) or missing entirely. `abs` / `floor` / `ceiling` / `round` /
  `truncate` / `float` / `expt` / `sqrt` / `number-to-string` signal `numberp`;
  arithmetic signals `number-or-marker-p`; `logand` / `logior` / `logxor` / `%`
  signal `integer-or-marker-p`; `ash` / `lsh` / `lognot` / `logcount` signal
  `integerp`. `wrong-number-of-arguments` now carries `(CALLEE COUNT)`.
- Regexp diagnostics are Emacs's own strings (`"Unmatched [ or [^"`, `"Invalid
  content of \{\}"`, `"Trailing backslash"`, …) under `invalid-regexp`, instead of
  leaking `fancy-regex`'s parser message and byte offsets. Emacs's tolerances too:
  a repetition operator with nothing to repeat is a literal (`(string-match "*x"
  "*x")` is 0) and a reversed range `[z-a]` matches nothing rather than failing to
  compile.
- `match-data` drops trailing unmatched groups, as Emacs does.
- `print-escape-control-characters` is honoured (and defvar'd), printing a control
  character as a backslash + octal escape.
- `seq-union` dedups *within* the first sequence, not only across the two.
- Sequence functions inherit their Emacs Lisp definitions' tolerances:
  `string-suffix-p` length-tests before it type-checks, `nconc`'s last argument may
  be any object (`(nconc (list 1) (cons 2 "s"))` → `(1 2 . "s")`), `string-to-vector`
  / `string-to-list` take any sequence, `concat` / `string-join` signal `sequencep`,
  `remq` / `delq` signal `listp`, `string=` signals `stringp`.

- `assq` / `assoc` / `rassq` now skip non-cons list elements instead of signalling
  `wrong-type-argument listp` (Emacs C `FOR_EACH_TAIL` + a `CONSP` guard), so e.g.
  `(assq 'interactive '("doc" (…)))` returns nil — the lookup `cl-defmethod`
  expansion performs over a method body that starts with a docstring.
- `cl-defstruct` slots accept per-slot `:type` / `:read-only` / `:documentation`
  options (`(name nil :type symbol :read-only t)`): `:type`/`:documentation` are
  parsed and dropped (Emacs does not enforce `:type` at runtime), and setf on a
  `:read-only` slot signals `"SLOT is a read-only slot"` (cl-macs.el). A docstring
  preceding the slot specs is dropped rather than mistaken for a slot.
- `cl-flet` / `cl-labels` support the `(FUNC EXP)` binding form (bind a local
  function to the value of an expression), not just `(FUNC ARGLIST BODY…)`
  (cl-macs.el) — used by cl-generic's `(cl-flet ((cl-call-next-method CNM)) …)`.
- Ported the help.el usage/docstring helpers `help-add-fundoc-usage`,
  `help-split-fundoc`, `help--make-usage`, `help--make-usage-docstring` and
  `help--docstring-quote` (they append/split the `"(fn ARGS)"` usage line on a
  docstring during `cl-defgeneric`/`cl-defmethod` expansion); the macroexp
  predicates `macroexp-const-p` / `macroexp-copyable-p` / `macroexp--fgrep`; and
  the byte-run advertised-calling-convention table
  (`get-advertised-calling-convention` / `set-advertised-calling-convention`).
  Together these let the upstream `emacs-lisp/cl-generic.el` load through its full
  generic/method machinery (up to the built-in-type generalizer prefill, which
  needs cl-preloaded.el's `cl--class` registry).
- `condition-case` now binds the handler variable to the real `(ERROR-SYMBOL .
  DATA)` object, preserving `signal`'s data list — `(signal 'wrong-type-argument
  '(integerp 5))` caught binds `(wrong-type-argument integerp 5)`, so `(cadr e)` =>
  `integerp`. Previously the data was stringified. Also added `ignore-error`
  (singular) and `with-suppressed-warnings`.
- `#'(lambda …)` / `(function (lambda …))` now compiles to a closure instead of
  loading the literal lambda form (which `funcall` rejected as `invalid-function`).
- `user-error` signals the `user-error` condition (not `error`). Added the symbol
  property system (`get` / `put` / `symbol-plist`), `define-error`, and seeded the
  standard error conditions, so `error-message-string` matches Emacs
  (`"Wrong type argument: integerp, 5"`). Added `seq-let`, `macroexp-progn`,
  `cl-function`, and `pcase-let` destructuring patterns.
- Added `cl-letf` (save/set/restore generalized places), `letrec`, `dlet`;
  `cl-destructuring-bind` handles nested patterns, and `seq-let` handles `&rest`.
- `cl-defstruct` instances now print as Emacs record syntax `#s(NAME …)`
  (recursively), `type-of` returns the struct name, and `recordp` / `cl-struct-p`
  recognize them. (`vectorp` still returns t — they're vectors under the hood.)
- Added `eval` (macroexpands, compiles, and runs a form), and `cl-loop`'s numeric
  `for` accepts an implicit `from 0` (`(cl-loop for i below 5 collect i)`).
- Added the `rx` macro — compiles an S-expression regexp (string/char literals, the named
  character classes and anchors, `group`/`group-n`/`or`/`seq`, the quantifiers `*`/`+`/`?`/
  `=`/`>=`/`**`/`repeat`, char sets `(any …)`/`(not …)`, `literal`/`regexp`) to a regexp
  string, with Emacs's single-char-`or`→char-class folding. Also wired `(rx …)` as a pcase
  pattern (matches the value string against the compiled regexp).
- Backquoted **vector** templates now evaluate (`` `[,a ,b] `` => `[1 2]`, including `,@`
  splicing), and pcase supports backquoted vector **patterns** `` `[,a ,b] `` with
  exact-length matching and no error on non-vector values.
- Added `pcase-let*` and `pcase-dolist`, and the pcase `(seq P0 P1 …)` pattern (matches each
  subpattern against successive elements of a list/vector/string).
- `cl-loop` learned more iteration clauses: `for V across SEQ`, `for V being [the|each]
  {elements|hash-keys|hash-values} of SOURCE`, `for V = INIT [then STEP]` (correctly
  interleaved with `until`/`while`), `when/unless COND return X`, and an accepted `named NAME`.
- Added `read-from-string` (returns `(OBJECT . END-INDEX)`, honoring START) and `pp-to-string`
  / `pp`. Added `cl-substitute-if` / `cl-substitute-if-not`, `cl-mapcan`, and the
  uni/multibyte shims `string-to-multibyte` / `string-as-multibyte` / `string-to-unibyte` /
  `string-as-unibyte` / `multibyte-string-p`. `seq-contains-p` now works on strings/vectors
  (not just lists), and `remove` preserves the sequence type (a vector input yields a vector).
- `cl-defmethod` gained full CLOS method combination: `:before` / `:after` / `:around`
  qualifiers (run in standard order around the primary), and `cl-call-next-method` /
  `cl-next-method-p` (a primary chains to the next-most-specific primary; an `:around` chains
  to the rest of the combination). A qualified method no longer collides with the primary that
  shares its specializers. `cl-call-next-method` past the end signals `cl-no-next-method`.
- Added `cl-coerce`, `cl-gensym`, and `cl-digit-char-p`.
- Added `cl-defgeneric` / `cl-defmethod` — single- and multi-argument type dispatch with
  specificity ordering (a more specific specializer wins: `integer` over `number`, `(eql V)`
  over a type), `(eql V)` and `(head V)` specializers, unspecialized args, and per-specializer
  method redefinition. An unhandled call signals `cl-no-applicable-method`.
- `char-equal` now honors `case-fold-search` (default `t`), so `(char-equal ?a ?A)` => `t`.
  Added `cl-assert` (signals `cl-assertion-failed`, a subtype of `error`), `cl-check-type`
  (signals `wrong-type-argument`), and `format-spec`.
- Added `cl-pairlis`, `cl-tailp`, `cl-ldiff` (the latter two key off `eq` tail identity),
  and `lax-plist-get`. `cl-remove-duplicates` now honors `:from-end` (keep the first
  occurrence instead of the last).
- Added the transcendental float-math builtins (none existed before): `exp`, `log` (with an
  optional base), `sin`, `cos`, `tan`, `asin`, `acos`, `atan` (1- or 2-arg `atan2`), plus
  `ldexp`, `frexp` (returns `(SIGNIFICAND . EXPONENT)`), and `copysign`. Added `cl-parse-integer`.
- Added more `seq.el` functions: `seq-sort-by`, `seq-split`, `seq-positions`,
  `seq-remove-at-position`, and gave `seq-mapcat` its optional TYPE argument. `seq-partition`
  now preserves the element type (a vector input yields vector chunks).
- `sort` no longer panics on `(sort SEQ)` with no predicate — it falls back to the
  default `value<` ordering (numbers numerically, strings/symbols lexically). It also
  accepts the Emacs-30 keyword form `(sort SEQ &key :lessp :key :reverse)`.
- `cl-defstruct` now honors the `(:constructor NAME)` and `(:conc-name PREFIX)` options
  in the name/options head.
- `format` `%g` now implements proper C-printf semantics — it switches to exponent
  notation when the decimal exponent is `>= precision` (default 6) or `< -4`, honors the
  precision field as significant digits, trims trailing zeros (kept with the `#` flag), and
  respects width/sign flags. `(format "%g" 1000000.0)` => `"1e+06"`, `(format "%.3g" 3.14159)`
  => `"3.14"`. (Was returning the value's default rendering, ignoring precision and the
  threshold — closes BUGS.md R4-I `%g`.)
- Added display-width and byte/char utilities: `string-width`, `char-width` (East-Asian
  wide/fullwidth chars count as 2 columns, combining marks as 0), `truncate-string-to-width`,
  `string-bytes` (UTF-8 byte count), and `subst-char-in-string`. Added `cl-type-of`
  (`fixnum`/`null`/`cons` refinements over `type-of`), `number-or-marker-p`,
  `integer-or-marker-p`.
- Added `cl-the`, `cl-etypecase`, `cl-ecase`, and `cl-do` (with CL parallel stepping).
  `cl-loop` now destructures `for PATTERN in LIST` (e.g. `for (a b) in …`, including
  dotted `(k . v)` patterns), and `cl-destructuring-bind` accepts a dotted arglist tail.
- `split-string` now treats its SEPARATORS argument as a regexp (Emacs semantics),
  so `(split-string "a1b2c" "[0-9]")` => `("a" "b" "c")`. The `cl-remove-if` /
  `cl-remove-if-not` family honors `:count`, and `cl-position` / `cl-count` /
  `cl-position-if` / `cl-count-if` honor `:start` / `:end`. Added `string-version-lessp`
  (numeric-run-aware compare) and made `format-message` curve-quote its format string.
- Added the Common-Lisp set/sequence family: `cl-union`, `cl-intersection`,
  `cl-set-difference` (all honoring `:test`), `cl-adjoin`, `cl-subst`, `cl-maplist`,
  `cl-merge`, `cl-stable-sort`, `cl-delete-duplicates`, and `cl-endp`. `cl-reduce` now
  honors `:from-end` (right fold) and `:key`.
- Added the Common-Lisp integer-math family: `cl-floor`, `cl-ceiling`, `cl-truncate`,
  `cl-round` (each returning `(QUOTIENT REMAINDER)`), `cl-mod`, `cl-rem`, `cl-gcd`,
  `cl-lcm` (variadic, with the `()`→0/1 and zero-arg identities), and `cl-isqrt`.
  Fixed `cl-oddp` to hold for negative odd integers (`(cl-oddp -3)` => `t`).
- Added the `cl-*-if` count/position family: `cl-count-if`, `cl-count-if-not`,
  `cl-position-if`, `cl-position-if-not` (all honoring `:key`). Added `string-fill`
  (greedy column-wrapping at spaces).
- `pcase` now supports the `(app FN PAT)` pattern (match PAT against `(FN value)`),
  and `(pred FN)` / `(app FN …)` accept a `lambda` as FN, not just a named function.
- `setf` learned two more generalized places: `(alist-get K A)` (setcdr an existing
  cell, else cons a new `(K . V)` pair onto the front) and `(plist-get P K)` (set an
  existing value cell, else prepend `K V` — matching Emacs's order for a new key).
  Added `cl-typep`.
- Exposed `macroexpand` / `macroexpand-1` / `macroexpand-all` (work on user and
  prelude macros; the built-in `when`/`unless`/… are compiler intrinsics, not real
  macros, so they pass through unchanged). Added `indirect-function`, `cl-sort`
  (with `:key`), `commandp`, `plistp`.
- Float printing matches Emacs: the shortest round-tripping representation, in
  exponential notation when the decimal exponent is ≤ -5, or ≥ 15 and shorter
  (`1e100` => `1e+100`, `1e15` => `1e+15`, but `1234567890123456.0` stays decimal).
- Added the `pcase` `(cl-type TYPE)` pattern and `pcase-exhaustive`.
- Lexical scope leak on non-local exit: a `throw` or `error` out of an inner
  `let` skipped its scope cleanup, so `run_closure` (the catch/condition-case
  thunk runner) left inner scopes open and corrupted the caller's lexical
  bindings. It now unwinds the scope stack to the entry depth. This was the root
  cause of the "void variable" failures seen when an ERT `should` wrapped a
  macro that expands to `catch`/nested-`let` (e.g. `pcase`, `cl-loop`).
- `eq` is now object identity: `(eq 1.0 1.0)` => `nil` (distinct float objects).
  `eql` keeps by-value float comparison; `eq`/`eql` split into separate subrs.
- `round` uses banker's rounding (half to even): `(round 2.5)` => `2`.
- `mod` is a primitive subr handling float operands and divisor-sign semantics:
  `(mod 13.5 4)` => `1.5`, `(mod -1 3)` => `2`.
- `split-string` honors OMIT-NULLS (3rd arg); default separators omit implicitly.
- `dotimes` / `dolist` evaluate and return their optional RESULT form.
- `capitalize` upcases the first letter of every word, not just the first.
- `expt` returns a float for negative/fractional exponents (`(expt 2 -1)` => `0.5`,
  `(expt 2.0 0.5)`), integer power otherwise. Now a primitive subr.
- `string-to-number` parses floats and scientific notation, and takes an optional
  BASE argument (`(string-to-number "ff" 16)` => `255`). Now a primitive subr.
- Added missing core functions: `type-of`, `functionp`, `char-or-string-p`,
  `sqrt`, `fround`, `ffloor`, `fceiling`, `ftruncate`, `isnan`, `char-equal`.
- `floor` / `ceiling` / `round` / `truncate` accept an optional DIVISOR
  (`(floor 7 2)` => `3`), with exact integer division for integer operands.
- `last` / `butlast` accept an optional N (`(last '(1 2 3) 2)` => `(2 3)`).
- `reverse` works on any sequence (string / vector / list); `downcase` / `upcase`
  accept a string or a character (`(downcase ?A)` => `97`). Now primitive subrs.
- `append`'s final argument is the tail as-is, so a non-list last arg yields a
  dotted result: `(append '(1 2) 3)` => `(1 2 . 3)`.
- Non-finite floats print in Emacs read syntax: `1.0e+INF`, `-1.0e+INF`, `0.0e+NaN`.
- A `(lambda …)` form in operator (head) position is now applied directly:
  `((lambda (x) x) 5)` => `5` (previously `invalid-function`).
- `1+` / `1-` preserve float contagion (`(1+ 1.0)` => `2.0`): they lower to the
  float-aware native `Add`/`Sub` ops instead of integer `Inc`/`Dec`, keeping the
  fast path for integer loop counters.
- Reader: radix-prefixed integers `#x1f` / `#o17` / `#b101` (and uppercase /
  general `#NNr…`); character modifier syntax `?\C-` / `?\^` / `?\M-` / `?\S-` /
  `?\H-` / `?\s-` / `?\A-`, nestable (`?\C-\M-a`); and the non-finite float read
  syntax `1.0e+INF` / `-1.0e+INF` / `0.0e+NaN`.
- `format` supports `%N$` argument fields (`(format "%2$s %1$s" "a" "b")` =>
  `"b a"`), combinable with flags/width (`%2$05d`).
- Added more core functions: `logb`, `read`, `compare-strings`,
  `error-message-string`, `seq-mapn`, and `format-message` (alias of `format`).
- Found via a fresh Emacs parity sweep: added `vconcat`, `string-to-vector`,
  `logcount`, `string-equal-ignore-case`, `upcase-initials`, and the constants
  `most-positive-fixnum` / `most-negative-fixnum`; `abs` is now a subr (keeps
  int/float type and normalizes `-0.0` => `0.0`); `string-prefix-p` /
  `string-suffix-p` honor IGNORE-CASE; `assoc` takes an optional TESTFN;
  `string-pad` takes PADDING and START.
- Introspection sweep: added `symbol-function`, `intern-soft`, `subrp`, `macrop`,
  `special-form-p`, `char-uppercase-p`, `string-distance`, `fixnump`, `bignump`.
  `macrop` / `special-form-p` follow Emacs's classification (`when`/`unless`/
  `lambda`/`defun`/`defmacro` report as macros even though elisprs lowers them as
  compiler intrinsics).
- Added `seq-concatenate`, `copy-alist`, `substring-no-properties`; `alist-get`
  takes DEFAULT and TESTFN; `string-trim` / `string-trim-left` / `string-trim-right`
  accept regexp arguments.
- `format`: the `+` and space sign flags now work on signed conversions
  (`%+d`, `% d`, `%+.2f`, `%+05d`), and `%e` uses C-style formatting
  (`3.141590e+04` — default 6-digit mantissa, signed ≥2-digit exponent).
- Added `hash-table-test` and `nbutlast`.
- `string-search` honors its optional START char index; added `memql` and
  `assoc-string`.
- `format`: `%x` / `%X` / `%o` print sign + magnitude for negatives (`-ff`, not
  two's complement), and the `#` flag adds a `0x` / `0X` / `0` prefix (zero-fill
  goes after the sign and prefix: `%#010x` of 255 => `0x000000ff`).
- `case-fold-search` (defvar, default `t`): `string-match` / `string-match-p` /
  `replace-regexp-in-string` now fold case by default, and honor a `let`-bound nil.
- `incf` / `decf` / `cl-incf` / `cl-decf` operate on generalized `setf` places
  (`(cl-incf (car l))`), not only symbols.
- `when-let` / `if-let` accept multiple sequential bindings; added `when-let*` /
  `if-let*` and `named-let`.
- `replace-regexp-in-string` accepts a function REP: it's called on each match's
  text (with match data set) and its result is the replacement. Handled via the
  re-entrant `call_function` path (like `mapcar`/`sort`), outside the host borrow.
- `plist-put` and `delete-dups` are now destructive (mutate in place), matching
  Emacs; added `nconc`, `rassq-delete-all`, and `fillarray`; `number-sequence`
  handles a negative step.
- cl-lib/seq parity (ground-truthed with the libraries loaded): `cl-reduce` takes
  `:initial-value`; `cl-mapcar` walks N sequences; `cl-remove-duplicates` keeps the
  last occurrence (Emacs default); `seq-group-by` lists groups in first-encounter
  order.
- Added `length=` / `length<` / `length>`, `cl-typecase`, `cl-destructuring-bind`
  (positional / `&optional` / `&rest`), and `string-clean-whitespace`; `cl-getf`
  takes a DEFAULT.
- `cl-loop` (common subset): numeric `for … from/to/below/downto/above/by`,
  `for … in/on`, `repeat`, `while`/`until`; the `collect`/`append`/`nconc`/`sum`/
  `count`/`maximize`/`minimize` accumulators; `do`; and `finally [return]`.
  Plus `with VAR = VAL`, accumulate `into VAR`, `when`/`unless`/`if`…`else`
  conditionals, and the `always`/`never`/`thereis` boolean clauses.
  (Not yet: parallel `for`, `across`, destructuring.)
- `mapcar` / `mapc` and the `seq-*` iterators accept any sequence (vector / string,
  not just lists). Added `boundp`, `gensym`, `default-value`.
- Hash tables print in Emacs-30 syntax: `#s(hash-table [test T] [data (k v …)])`,
  omitting `test` when `eql` and `data` when empty.
- `pcase` now supports backquote (structural) patterns — `` `(,a ,b) ``,
  `` `(,a . ,rest) ``, nested, and literals — recognized from the reader's eager
  backquote expansion (no lazy backquote needed).
- Reader: fixed dotted backquote `` `(,a . ,b) `` (and `` `(a b . ,x) ``), which
  previously mis-expanded the unquoted dotted tail.
- Added `cl-flet` / `cl-labels` (lexical local functions via a call-rewriting code
  walk; `cl-labels` supports self- and mutual recursion), `let-alist`, `and-let*`,
  `cl-dolist` / `cl-dotimes`, and the `fset` / `fboundp` primitives.
- Added `cl-block` / `cl-return-from` / `cl-return` (named escapes; `cl-dolist` /
  `cl-dotimes` now establish the nil block), `cl-pushnew`, `cl-find-if-not`;
  `cl-subseq` / `seq-subseq` work on any sequence with an optional and negative END.
- `cl-defstruct` — generates the keyword constructor (per-slot defaults), `NAME-p`
  predicate, `NAME-SLOT` accessors (`setf`-able), and `copy-NAME`. Instances are
  `[cl-struct-NAME …]` vectors, so behavior matches but printing / `type-of` differ
  from Emacs records.
- `cl-member` / `cl-assoc` / `cl-find` / `cl-position` / `cl-count` / `cl-remove` /
  `cl-delete` / `cl-substitute` now take `:test` / `:key` (and `:count` where
  applicable) keyword arguments, and preserve the input sequence's type.

### Added
- **OClosures (`emacs-lisp/oclosure.el`)** — a faithful port of the Open Closure
  subsystem: `oclosure-define` (a callable type with named, typed, optionally
  `:mutable` slots and single-parent inheritance), `oclosure-lambda` (an instance
  — a closure that also carries its type and slot values), the generated per-slot
  accessors (`TYPE--SLOT`, with `setf` support for mutable slots via the gv
  function-setter), the type predicates (`TYPE--internal-p`), `oclosure-type`,
  `oclosure--slot-value` / `oclosure--set-slot-value`, and the `accessor` /
  `oclosure-accessor` bootstrap types. Verified value-for-value against the Emacs
  30.2 binary. The host-specific primitives — `closurep`, `oclosure--fix-type`,
  `oclosure-type`, `oclosure--get`, `oclosure--set`, `oclosure--copy` — are C
  (`src/builtins.rs`): because elisprs closures are compiled (a fusevm `Chunk` +
  captured env) rather than aref-indexable interpreted-functions, a closure's
  OClosure type + slot layout is attached via a side table and its slot values
  live in the closure's captured lexical env (the same storage the body reads, so
  `oclosure--set` and an in-body `setq` stay mutually visible — exactly as Emacs
  stores slots in the env). A minimal cl--class / cl-slot-descriptor substrate
  (from `cl-preloaded.el`) backs the type hierarchy. This clears the `oclosure`
  wall that blocked the cl-generic cluster: `cl-generic.el`'s `cl--generic-nnm`
  OClosure and its `oclosure-type`-based method-dispatch touchpoints now load and
  evaluate; the remaining cl-generic gaps are outside oclosure (`help-fns`'s
  `help-add-fundoc-usage`, and cl-macs `cl-defstruct` slot-option / `:type`
  handling for the `cl--generic` struct).
- **Text buffers, point, narrowing & marker-based `save-excursion`** — the
  buffer.c/editfns.c/insdel.c editing core, verified value-for-value against the
  Emacs 30.2 binary. A global registry of **named live buffers**, each with its
  own text (`Vec<char>`), 1-based point, mark, and narrowing bounds. New C
  primitives (`src/builtins.rs`): registry — `current-buffer`, `set-buffer`,
  `get-buffer`, `get-buffer-create`, `generate-new-buffer`,
  `generate-new-buffer-name`, `buffer-name`, `buffer-live-p`, `bufferp`,
  `kill-buffer`, `rename-buffer`, `buffer-list`; text — `insert` / `insert-char`
  now shift point and every later marker-like position, `delete-region` /
  `delete-char` / `erase-buffer`; position — `point-min` / `point-max` honor
  narrowing, `goto-char` clamps into the accessible region (returning its raw
  arg), `char-after` / `char-before` / `bolp` / `eolp` / `bobp` / `eobp` and line
  motion respect `[begv, zv]`; mark — `set-mark`, `mark`, `region-beginning`,
  `region-end`; narrowing — `narrow-to-region`, `widen`. `begv`/`zv`/`mark`/the
  save stacks track edits with Emacs marker rules (insertion-type nil for `begv`,
  t for `zv`). Prelude macros (`src/prelude.rs`): `save-current-buffer`,
  `with-current-buffer`, `with-temp-buffer` (fresh ` *temp*` buffer, killed after),
  `save-excursion` (restores current buffer **and** point via a marker that tracks
  intervening insertions/deletions), and `save-restriction`. Buffer-local
  resolution keys off the current buffer, and `buffer-local-value` now honors its
  BUFFER argument. This carries `tabulated-list.el` past load into
  `(tabulated-list-mode)` buffer initialization (blocked only at the named
  redisplay + text-property boundaries). Boundaries named, not faked: **text
  properties** (`propertize` / interval trees), **general marker objects**
  (`make-marker` / `point-marker`), and **redisplay** (windows/header-line/faces)
  are not modeled.
- **Buffer-local variables + major/minor mode machinery** — the buffer.c/data.c
  local-binding primitives and the subr.el / derived.el / easy-mmode.el mode
  layer, verified byte-for-byte against the Emacs 30.2 binary. New C primitives
  (`src/builtins.rs`, backed by a per-buffer local-binding table on the current
  buffer): `make-local-variable` (snapshots the current default into the local),
  `make-variable-buffer-local` (auto-local + special), `local-variable-p`,
  `local-variable-if-set-p`, `kill-local-variable`, `buffer-local-value`,
  `default-value` / `set-default` (now read/write the global cell directly,
  bypassing locals), and `use-local-map` / `current-local-map` (a per-buffer
  keymap slot). `symbol-value` / `set` (and `let`) now resolve
  lexical → buffer-local → global, so `(setq x 5)` after `(make-local-variable
  'x)` shadows the default per Emacs. Ported the mode plumbing in `src/prelude.rs`:
  `run-mode-hooks`, `delay-mode-hooks`, `setq-local`, `kill-all-local-variables`
  (honors `permanent-local`), `derived-mode-set-parent` / `derived-mode-all-parents`
  / `derived-mode-p` / `provided-mode-derived-p`, `merge-ordered-lists`, and the
  macros `define-derived-mode` and `define-minor-mode` (with `add-minor-mode`,
  `easy-mmode-pretty-mode-name`, `easy-mmode--mode-docstring`, `ensure-empty-lines`,
  `prefix-numeric-value`, and the minor-mode registries). This unblocks
  `tabulated-list.el`, which now loads fully (all `define-derived-mode` /
  `defvar-keymap` forms and `(provide 'tabulated-list)`). Boundaries named, not
  faked: syntax tables and abbrev tables are placeholder constructors (separate
  subsystems) and docstrings are not line-filled (`fill-region` cosmetic).
  (Multi-buffer text editing landed subsequently — see the text-buffer entry
  above.) Also fixed two engine bugs the port exposed: the regexp translator now
  escapes a literal `[` inside a bracket expression (`[{[]`, per POSIX/Emacs) for
  the `regex` crate, and `macroexpand-all` no longer expands a `let`-binding
  variable as a macro call (a symbol can be both a special variable *and* a macro,
  e.g. `delay-mode-hooks`, which previously looped forever).
- Customize **declaration** machinery — faithful port of custom.el / cus-face.el:
  `defgroup` / `defcustom` / `defface` and the `custom-declare-group` /
  `custom-declare-variable` / `custom-declare-face` functions they expand into,
  plus the `custom-initialize-*` handlers, `custom-handle-keyword`,
  `custom-add-*`, `face-spec-set`, `documentation-stringp`, and the
  `internal--define-uninitialized-variable` subr. `defcustom` defines the
  variable like `defvar` (respecting an existing binding) and stores the same
  observable symbol properties Emacs stores at declaration time —
  `standard-value`, `custom-type`, `custom-requests`, `custom-group`,
  `group-documentation`, `face-defface-spec` — verified byte-for-byte against
  the Emacs 30.2 binary. This unblocks libraries that declare options at load;
  `ansi-color.el` (1 `defgroup` + 5 `defcustom` + 23 `defface`) now loads clean.
  Scope is declaration only: the Customize UI (widget.el, `custom-set-variables`
  persistence) and live face objects / frame recalculation are out of scope
  (`face-spec-set` stores the spec prop but creates no face object).
- `pcase` — structural dispatch (non-backquote subset): `_` wildcard,
  self-quoting literals, `'x` / `(quote x)`, symbol binders, `(pred FN)` /
  `(pred (FN ARGS...))`, `(guard EXPR)`, `(and …)`, `(or …)`. Plus a minimal
  `pcase-let`. Backquote patterns (`` `(,a ,b) ``) are not yet supported — this
  reader expands backquote eagerly at read time, so they need lazy backquote
  first. Expands to a `cond` (the `cl-case` shape) rather than `catch`/`throw`,
  which expands cleanly when nested inside a macro-produced `defun` (e.g. an ERT
  `should`).
- Regexp support: `string-match` / `string-match-p`, `match-beginning` /
  `match-end` / `match-string`, `match-data` / `set-match-data`,
  `replace-regexp-in-string` (with `\&` / `\N` template expansion),
  `regexp-quote`, and the `save-match-data` macro. Elisp regexp syntax (`\(`
  grouping, `\|` alternation, `\{m,n\}` bounds, `\<`/`\>` boundaries, `\w`/`\s-`
  classes) is translated to the backing `regex` engine in `src/regexp.rs`; match
  data is char-indexed to match Emacs. Pattern backreferences (`\1`) are rejected
  rather than silently mismatched (the engine does not backtrack).
- `elisp --lsp` — a Language Server over stdio: positioned reader-error
  diagnostics, completion, hover, document symbols, and signature help.
- `elisp --dap` — a Debug Adapter over stdio: top-level-form breakpoints,
  stepping, `stackTrace` / `scopes` / `variables` / `evaluate` against the live
  `ElispHost`, with the debuggee's stdout captured through a pipe so program
  output streams as `output` events instead of corrupting the JSON-RPC channel.
- JetBrains plugin (`editors/intellij`) — Emacs Lisp lexer, `.el` / `.emacs`
  file types, `;;` commenter, paren/vector brace matcher, paren-balancing
  smart-enter, elisp new-file templates, color page, and LSP/DAP/run wiring.
- `release.yml` — tag-triggered multi-target binary builds, GitHub Release
  publishing, and Homebrew tap auto-bump.
- `completions/_elisp` — zsh completion for the `elisp` CLI.

### Fixed
- Bytecode cache invalidation: the `~/.elisprs` shard key now folds in a
  fingerprint of the builtin object layout and the prelude source, so adding or
  reordering subrs (or editing the prelude) no longer serves stale chunks that
  resolve builtin handles to the wrong objects. Previously only an elisprs
  version bump invalidated the cache.

## [0.1.0]

### Added
- Milestone-1 runtime: an elisp-correct reader, a compiler that lowers each
  top-level form to a `fusevm::Chunk`, and an `ElispHost` object heap reached
  through fusevm's extension handler — no bespoke VM or JIT.
- `Value::Obj` heap handles for cons / symbol / vector cells.
- ~65 primitive subrs (lists, arithmetic, predicates, strings, I/O, functional).
- Special-form lowering: `quote` `function` `if` `when` `unless` `and` `or`
  `progn` `setq` `let` `let*` `lambda` `defun` `defmacro` `while` `cond` …
- The `elisp` binary: run a file, `-e EXPR`, and a REPL.
- README (house template), `docs/` (reference + engineering report), and man
  pages (`elisp.1`, `elispall.1`).
