# Changelog

All notable changes to elisprs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions track `Cargo.toml`.

## [Unreleased]

### Fixed (Emacs parity — see BUGS.md)
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
