# Changelog

All notable changes to elisprs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions track `Cargo.toml`.

## [Unreleased]

### Fixed (Emacs parity — see BUGS.md)
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
