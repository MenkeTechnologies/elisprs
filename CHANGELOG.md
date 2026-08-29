# Changelog

All notable changes to elisprs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions track `Cargo.toml`.

## [Unreleased]

### Added
- **Strings are mutable objects.** `aset` on a string signalled
  `(wrong-type-argument arrayp "ab")`; `store-substring` and `clear-string` were
  void; `fillarray` refused anything but a vector. A string is now an arena
  object (`Obj::Str`) whose handle is the identity `eq` compares, so a write is
  visible through every reference to it — an alias, a list element, the literal
  a function returns — while `(let ((s "abc")) (eq s s))` stays `t` and two
  equal literals stay distinct. `store-substring` and `clear-string` are
  ported (`Fstore_substring`, `Fclear_string`), `fillarray` takes a string, and
  `aset` reproduces Emacs's check order (index before character) and its
  per-character `store-substring` bounds check, which leaves the prefix written
  and names the partially-written string in the error. Text properties travel
  with the write, so `(aset (propertize "abc" 'p 1) 0 ?z)` is still
  `#("zbc" 0 3 (p 1))`. Every string constant in a compiled chunk is now an
  object handle rather than an inline value, so the cache shard format is v10
  and a v9 shard is rejected.
- **`store-substring`, `clear-string`**, and `fillarray` on a string.
- **`define-hash-table-test`**, so `make-hash-table :test MY=` calls the elisp
  test and hash functions it declares instead of silently falling back to
  `eql`. The missing builtin was not the obstacle: those functions are elisp,
  so a lookup on such a table has to call elisp, and `hash_eq`/`hash_key` ran
  inside the host borrow a subr body holds. `gethash`/`puthash`/`remhash`/
  `make-hash-table` therefore join `mapcar`/`sort`/`maphash`/`mapatoms` on the
  intercepted path — registered with `defsubr`, so `subrp` and `#'NAME` are
  unchanged, but dispatched from `call_function` OUTSIDE the borrow. A built-in
  test still runs in one borrow; only a user test re-enters. The table carries
  its own `(NAME TESTFN HASHFN)`, which the heap image serializes, so the shard
  format is v11.
- **`pcase-lambda`**, whose parameters may be patterns:
  `(funcall (pcase-lambda (`(,a ,b)) (+ a b)) '(1 2))` is 3.
- **`string-pixel-width`** and the `tab-width` it reads. Not `string-width`: in
  batch a character is one pixel per display column, and a TAB advances to the
  next multiple of `tab-width` rather than counting a flat `tab-width` columns,
  so `(string-pixel-width "ab\tc")` is 9 where `(string-width "ab\tc")` is 11.
- **`rx`: `(category …)`, `*?`/`+?`**, and the `not` spellings
  `(not (syntax X))`, `(not (not X))` and `(not CLASS)`.
- **Overlays.** The whole family was absent: `make-overlay`, `overlayp`,
  `overlay-start`/`-end`/`-buffer`, `overlay-get`/`-put`/`-properties`,
  `delete-overlay`, `move-overlay`, `copy-overlay`, `overlays-at`,
  `overlays-in`, `next-overlay-change`, `previous-overlay-change` and
  `remove-overlays`. An overlay is an object rather than a text property: it
  survives the text under it changing, both ends move with edits like markers,
  and deleting one detaches it (`overlay-start` reads nil, `move-overlay` puts
  it back). `FRONT-ADVANCE`/`REAR-ADVANCE` decide which side of an insertion at
  an endpoint the new text lands on, and `remove-overlays` moves or SPLITS an
  overlay that only partly overlaps the range rather than deleting it.
- **`format-time-string`: `%G`, `%g`, `%V`, `%U`, `%W`, `%C`** and the `^`/`#`
  case flags; a field width now also right-aligns a string directive.
- **`looking-back`**, **`text-property-any`**, **`text-property-not-all`** and
  **`listify-key-sequence`** — all previously void.
- **`regexp-opt-charset`**, and `regexp-opt`/`regexp-opt-group` ported from
  regexp-opt.el.
- **Compiler macros are applied by `macroexpand-all`.** A function may declare
  a rewrite for its own call sites; elisprs had the storage and the `declare`
  bridge but nothing consulted them. `eval` deliberately does not apply them,
  which is why `(add-to-list 'l 1)` on a lexical `l` works in a loaded file and
  signals through `eval` — in Emacs too.
- **The hook API's missing four.** `remove-hook` (subr.el:2186, including the
  depth-alist cleanup of bug#46414), `run-hook-with-args-until-success`,
  `run-hook-with-args-until-failure` and `run-hook-wrapped` — all previously
  `void-function`, with only `add-hook`/`run-hooks`/`run-hook-with-args`
  present.
- **`format-spec.el`, ported.** The old `format-spec` substituted `%CHAR` and
  copied everything else through, so the whole
  `%<flags><width><precision>CHAR` syntax fell out as literal text:
  `(format-spec "%-5a|" '((?a . "x")))` answered `"5a|"` where Emacs answers
  `"x    |"`. Flags `0 - < > ^ _`, width and precision in display columns,
  IGNORE-MISSING's `nil`/`ignore`/`delete`/other, SPLIT, function-valued
  substitutions and `format-spec-make` are all in, matching Emacs 30.2 on 32
  probes.
- **`insert-and-inherit`** (C `insert_and_inherit` →
  `graft_intervals_into_buffer`): the inserted text takes the preceding
  character's text properties, honouring `rear-nonsticky`, plus any the
  following character marks `front-sticky`. This is the insert `format-spec`
  uses to carry FORMAT's properties onto each substitution.
- **`hash-table-size`, `hash-table-weakness`, `hash-table-rehash-size`,
  `hash-table-rehash-threshold`** — the four hash-table accessors that were
  void. `hash-table-size` reports the ALLOCATION size and follows Emacs 30.2's
  growth series (measured: 0 → 6 → 24 → 96, and a `:size N` table jumps to
  `max(4N, 24)` up to 64, doubling above).
- **`copy-tree`'s VECTORS-AND-RECORDS argument** (subr.el:877), and with it the
  iterative cdr walk that copies a dotted tail correctly.
- **pcase's `map` and `cl-struct` patterns, `mapp` and `map-let`.** Both
  patterns were absent from elisprs's pattern compiler, which signalled
  `(error "pcase: unsupported pattern (map a)")` rather than matching, and that
  took `map-let` (map.el:73 is a `pcase-let` over the `map` pattern) with it.
  The `map` pattern accepts a bare symbol, a keyword, and the
  `(KEY VAR [DEFAULT])` form whose KEY and DEFAULT are evaluated; `cl-struct`
  reads slots through `cl-struct-slot-value` behind a `cl-typep` guard, so a
  value of the wrong type fails the clause instead of signalling out of it.

### Fixed
- **`cl-member` with no keywords is `memql`, not a lisp walk.** `cl-seq.el`
  delegates the no-keyword case straight to `memql`, which walks the list in C,
  so `CHECK_LIST_END` names the WHOLE list: `(cl-member 1 '(2 . 3))` is
  `(wrong-type-argument listp (2 . 3))` and not `… listp 3`. `cl-set-difference`,
  `cl-intersection` and `cl--adjoin` route their no-keyword comparisons through
  it and inherited the wrong error data. The keyword path is still a lisp walk
  and still names the tail, exactly as Emacs's does.
- **A pattern backreference `\N` was never validated**, so an out-of-range one
  surfaced the regex crate's own wording inside the `invalid-regexp` error data
  (`"Error compiling regex: Invalid back reference to group 1"`) and a
  self-referential one was not diagnosed at all — `\(\1\)` compiled and
  matched. `regex-emacs.c` rejects both while reading the pattern, as
  `(invalid-regexp "Invalid back reference")`. Tracking Emacs's `re_nsub` and
  its compile stack to do that also fixed two group-numbering bugs: an explicit
  `\(?N:` only ever RAISES the counter, so `\(a\)\(b\)\(?1:c\)\(d\)`
  numbers `d` 3 rather than reusing 2 and dropping a register from `match-data`;
  and `\N` names an EMACS group number, which under explicit numbering is not
  the position fancy-regex gave the group, so `\(?3:a\)\3` is now compiled
  instead of refused.
- **Three character-alternative diagnostics.** A range's two characters are
  fetched as soon as a member is followed by `-`, so `[a-` and `[]-` are
  `(invalid-regexp "Premature end of regular expression")` — while the trailing
  `-` of `[a-b-` is an ordinary member and that stays unmatched-bracket. An
  unknown class name is `(invalid-regexp "Invalid character class name")`
  rather than literal class-body text, so `[[:foo:]]`, `[[:Alpha:]]` and
  `[[::]]` no longer compile. And `[:NAME:]` is recognized on Emacs's terms
  (`re_wctype_parse` needs a real `:]` within the count) instead of by scanning
  to the next `]`, so `[[:a]b]` is the members `[`, `:`, `a` followed by the
  literals `b]`. A `]` that is not the terminator is also escaped on the way to
  the engine now, which makes the range in `[]-a]` a range again.
- **`regexp-opt` built the right language in the wrong shape.** It joined the
  strings with `\|`; Emacs factors common prefixes and suffixes and folds
  one-character alternatives into a character set, so
  `(regexp-opt '("cat" "cot" "cut"))` is `"\(?:c\(?:[aou]t\)\)"` and
  `(regexp-opt '("a" "b" "c"))` is `"[abc]"`. That also closes `rx`'s `or`:
  literal branches go through `regexp-opt` (which sorts but does not condense,
  unlike `(any …)`), nested `or`s flatten, branches that all denote character
  sets merge into one, and an alternation is bare unless a sibling or a
  quantifier needs it grouped.
- **`re-search-backward` searched forwards.** It kept the last non-overlapping
  forward match before point; Emacs tries START positions from point downwards,
  takes the first that matches, and bounds the match end at the position the
  search started from — so `a+` in `"aaa"` from point 4 is a one-character
  match at 3, not the three-character match at 1. BOUND was ignored entirely.
- **`expand-file-name` did not always answer an absolute name.** A relative,
  empty or `~`-prefixed DIR is now expanded first, and the trailing slash comes
  from NAME rather than from the joined path.
- **`narrow-to-region` clamped where Emacs signals** — `(narrow-to-region 0 3)`
  is `(args-out-of-range 0 3)`, naming the arguments in the order given.
- **`let-alist` did not descend a dotted chain.** `.b.c` was bound as one key;
  it reads `(cdr (assq 'c (cdr (assq 'b ALIST))))`, and `..a` is the escape
  hatch for the ordinary symbol `.a`.
- **A leading-dot symbol printed escaped.** `(prin1-to-string (intern ".foo"))`
  was `"\.foo"`; the escape is needed only when the name would otherwise read
  as something else (`.` is the dotted-pair separator, `.5` is a number).
- **`rx`'s character alternatives were concatenation.** Emacs sorts the
  characters, merges adjacent ones into ranges, moves `]` to the front and
  `^`/`-` to the back, and drops the brackets for a single character:
  `(rx (in ?a ?b "0-9"))` is `"[0-9ab]"` not `"[ab0-9]"`, `(rx (in "abc"))` is
  `"[a-c]"`, `(rx (any ?a))` is `"a"`. `rx--string-to-intervals`,
  `rx--condense-intervals`, `rx--parse-any` and `rx--generate-alt` are ported
  from rx.el. `(rx (or))` is the never-matching `regexp-unmatchable` rather
  than `"\(?:\)"`, and `minimal-match`/`maximal-match` now set the greediness
  of every quantifier in their body — of the LONG spellings only, since
  `*`/`+`/`?` and `*?`/`+?`/`??` state their own.
- **The elisp call path could not reach `max-lisp-eval-depth`.** One nested
  elisp call is several Rust frames deep, and elisp recursion is bounded by
  `max-lisp-eval-depth` (1600), not by the OS stack — so on a thread with the
  platform default stack the process aborted on a hardware stack overflow
  before the limit could signal, which no `condition-case` can catch.
  `run_closure` grows the stack on demand, as the reader already did.
- **`(head SYMBOL)` was an evaluated specializer.**
  `(cl-defmethod g ((x (head foo))) …)` was a `void-variable foo`, and where
  the name happened to be bound it dispatched on that value instead.
- **`cl-defgeneric` answered its own name** where Emacs answers nil.
- **`pcase`'s `cl-type` rejected a compound specifier.**
  `(pcase 3 ((cl-type (integer 0 5)) 'in) (_ 'out))` was
  `(wrong-type-argument symbolp (integer 0 5))`; it routes through `cl-typep`
  now, as `cl-typecase` already did.
- **An interpreted closure captured its whole enclosing scope.**
  `(let ((n 1) (m 2)) (format "%S" (lambda () n)))` printed
  `"#[nil (n) ((m . 2) (n . 1))]"` where Emacs prints `"#[nil (n) ((n . 1))]"`,
  and a closure referencing nothing printed the enclosing bindings instead of
  Emacs's `(t)`. `src/freevars.rs` ports `cconv-fv` / `cconv-analyze-form`
  (cconv.el) and `ElispHost::trim_lex` applies it: the surviving bindings keep
  the environment's order, only the innermost binding of a shadowed name
  survives, and the kept nodes SHARE the enclosing chain's value cells so `setq`
  still crosses the closure boundary. Visible through `prin1` and through
  `equal`, which compares the captured environment.
- **`macroexpand-all` did not expand `lambda`.** `lambda` is a macro in Emacs
  (`subr.el`: `(list 'function (cons 'lambda cdr))`), so
  `(macroexpand-all '(list (lambda () n)))` is `(list #'(lambda nil n))`; it
  answered `(list (lambda nil n))`. The same arm stopped treating
  `#'(lambda ...)` as quoted data — its body is code and Emacs expands it.

### Fixed (Emacs parity — object identity, hash-table slot order, closures)
- **A string is an object, and `eq` is object identity.** `el_eq` answered `t`
  only for the empty string, so `(let ((s "abc")) (eq s s))` was `nil` where
  Emacs says `t`, and every identity-based primitive lied about strings:
  `memq`, `memql`, `assq`, `delq`, `remq`, `eql`. Two references to one string
  now share one `Arc` and compare `eq`; two equal literals still do not.
- **`copy-sequence` returned its argument for a string or a vector.** Worse
  than an identity gap: `(aset COPY 0 9)` wrote through to the original, so
  every caller that copies before mutating shared one object.
  `(let* ((v (vector 1 2)) (c (copy-sequence v))) (aset c 0 9) (list v c))`
  answered `([9 2] [9 2])` instead of `([1 2] [9 2])`.
- **A hash table's slot order is observable, and it was wrong.** Emacs stores
  entries in a slot vector plus a free list: `remhash` frees a slot that the
  next `puthash` reuses (LIFO), and `maphash`, the `#s(hash-table …)` printer
  and `hash-table-keys` all walk slots in index order. elisprs stored a packed
  association vector and appended instead, so a remove-then-add reordered the
  whole walk. `hash-table-keys`/`hash-table-values` also come back in REVERSE
  slot order, because subr-x's definitions `push` inside a `maphash` and never
  reverse.
- **`equal` on interpreted closures.** An interpreted closure IS its
  `#[ARGLIST BODY ENV]` structure and `equal` descends into it, so
  `(equal (lambda () 1) (lambda () 1))` is `t` in Emacs and was `nil` here.
  That is also what makes an anonymous hook function removable — `remove-hook`
  matches with `member`.

### Performance
- **Hash tables are hashed, not scanned.** Every `gethash`/`puthash`/`remhash`
  walked a `Vec<(Value, Value)>` linearly *and cloned the whole vector first*,
  so each operation cost O(n) comparisons and one O(n) allocation. A 20 000-key
  build-then-read (`format`ed string keys) went **36.07 s → 0.69 s** of user
  time (min of repeated runs, debug build). The new index maps a key hash to
  candidate slots, with the hash computed once when the key goes in — which is
  also why a key mutated after insertion is now lost, exactly as in Emacs.
- **Compiled regexps are cached.** `string-match`/`re-search-forward`/
  `looking-at` re-ran the elisp→`fancy_regex` translation *and*
  `fancy_regex::Regex::new` on every call, so a match inside a loop paid a full
  compile per iteration. 40 000 `(string-match "\\([a-z]+\\)-\\([0-9]+\\)"
  "abc-123")` went **54.62 s → 0.85 s**. Emacs keeps a compiled-pattern cache
  for the same reason (`compile_pattern`, search.c). The cache hands out a
  shared `Rc<CompiledRe>` rather than a copy — a cloned regex carries an empty
  match-cache and rebuilds its lazy DFA per call — keys on the pattern plus
  `case-fold-search`, keeps the FAILURE so an invalid pattern still signals
  every time, and skips patterns whose translation read the syntax table
  (`\sC`, `\w`, `\cC`), which are only valid for the table they read.
- **VMs are pooled per closure body.** Every elisp call did
  `VM::new((*body).clone())` — a fresh VM plus a deep copy of the body `Chunk`
  (`ops`, `constants`, `lines`, and one `String` per entry in `names`).
  Profiling a 200 000-call loop put `_platform_memmove` at 141 of the
  interpreter thread's 978 samples, second only to `VM::exec_op` at 177. A
  pooled VM already holds its chunk, so the chunk is moved back in
  (`std::mem::take` + `VM::reset`) and never copied; its JIT state survives with
  it. A 20 000-call loop over a 120-form body went **2.95 s → 2.45 s** of user
  time, and the copy is gone from the profile.
- **`syntax.c`'s sexp/comment scanner.** `scan-lists`, `scan-sexps`,
  `parse-partial-sexp`, `syntax-ppss`, `forward-comment`,
  `backward-prefix-chars`, `matching-paren`, `syntax-after`, and the motion
  commands `forward-sexp` / `backward-sexp` / `forward-list` / `backward-list` /
  `down-list` / `up-list` / `backward-up-list` — all previously void. This is
  one port, not eleven: `Fforward_sexp` *is* `scan_lists`, and `scan_lists` and
  `Fparse_partial_sexp` share the comment/string state machine
  (`scan_sexps_forward`, `forw_comment`, `back_comment`, `char_quoted`,
  `find_defun_start`, `in_2char_comment_start`). Two-character comment
  delimiters, comment styles `a`/`b`/`c`, nested comments, generic string and
  comment fences, the `$` math class, escape runs,
  `parse-sexp-ignore-comments`, `comment-end-can-be-escaped`,
  `multibyte-syntax-as-symbol`, `open-paren-in-column-0-is-defun-start`, and the
  `syntax-table` text property under `parse-sexp-lookup-properties` are all in.
  A partial state can be resumed, including one captured *between* the two
  halves of a comment delimiter (element 10 of the state). 236 differential
  probes against Emacs 30.2 agree exactly, and the three `forward-sexp` /
  `scan-sexps` / `parse-partial-sexp` rows that the round-17 entry-point table
  marked void now match on both entry points.
  - Named boundary: `syntax-ppss` parses from `point-min` on every call rather
    than memoizing in `syntax-ppss-cache`. That is what Emacs itself does on a
    cold cache; the difference is speed, plus the elements 2 and 6 that Emacs's
    own docstring already disclaims.
- **`buffer-end`, `case-symbols-as-words`, `parse-sexp-ignore-comments`,
  `parse-sexp-lookup-properties`, `comment-end-can-be-escaped`,
  `multibyte-syntax-as-symbol`, `open-paren-in-column-0-is-defun-start`,
  `words-include-escapes`, `forward-sexp-function`, `syntax-propertize`,
  `syntax-propertize-function`, `syntax-ppss-table`, `syntax-ppss-context`,
  `syntax-ppss-toplevel-pos`, `syntax-ppss-depth`, `syntax-prefix-flag-p`** and
  the `ppss` accessors — the surface `syntax.el` and `lisp.el` expect.
- **`cl-defstruct`'s `:type` and `:named` options.** `(:type list)` and
  `(:type vector)` were parsed and thrown away, so both produced a *record* and
  every accessor was off by the tag slot: `(cl-defstruct (foo (:type list)) a b)`
  then `(foo-a '(1 2))` signalled `(wrong-type-argument arrayp (1 2))` instead of
  answering 1. Constructors now build the requested representation, accessors use
  `nth` for a list, `setf` on a list slot expands to `(setcar (nthcdr I S) V)`,
  `cl-struct-slot-offset` counts from 0 when there is no tag, and a predicate is
  generated only for a named struct — as in `cl-macs.el`. Found because
  `syntax.el`'s `ppss` struct is `(:type list)`.
- **Function cells for the intercepted higher-order primitives.** `funcall`,
  `apply`, `mapcar`, `mapc`, `sort`, `maphash`, `mapatoms`, `load`, `eval`,
  `macroexpand`, `macroexpand-1` and `macroexpand-all` run inside
  `host::call_function`, which matches them by name before any function-cell
  lookup — so they had no cell at all, and `(fboundp 'mapcar)`,
  `(functionp 'eval)`, `(func-arity 'funcall)`, `(indirect-function 'apply)` and
  `(symbol-function 'load)` all answered as though the name were undefined.
  Calling them always worked; only introspection was wrong. `call_function` now
  also matches a *subr object*, so `(funcall (symbol-function 'mapcar) …)`
  reaches the intercept too.
  - Named boundary: `macroexpand-1` and `macroexpand-all` are byte-compiled Lisp
    in Emacs, so `(subrp (symbol-function 'macroexpand-1))` is nil there and t
    here. The other four observables agree.
- **`elisp --script FILE`** — Emacs's other file entry point. `emacs -l FILE` and
  `emacs --script FILE` evaluate the file in *different buffers*, so they read
  different syntax tables and disagree on every `char-syntax` / `\sC` /
  `skip-syntax-*` answer: `-l` runs in `*scratch*` under the Emacs Lisp mode
  table (`(char-syntax ?.)` => 95), `--script` runs in ` *load*` under the
  standard one (=> 46). Plain `elisp FILE` is unchanged and still models `-l`,
  the column `scripts/fuzz_parity.sh` compares against. The entry point is folded
  into the bytecode cache key, since a top-level form that reads the buffer can
  change how a later form macro-expands.
- **The ` *load*` buffer.** `emacs -Q --batch --eval` reports three buffers;
  `emacs -Q --batch -l FILE` reports four, because `load` reads the file through
  a buffer of its own (mule.el, `(generate-new-buffer " *load*")`), holds the
  file's text in it, and kills it on the way out. elisprs reported three either
  way. A nested `(load …)` now nests the same way Emacs does, down to the
  ` *load*-NNNNNN` name. The slot is reserved inside the built-in arena prefix so
  the cache-hit and cache-miss paths see the same handle.
- **`command-line-args`, `command-line-args-left` and `argv`.** All three were
  void, so a script could not see its own arguments. `elisp FILE a b c` now
  answers `("a" "b" "c")` for the latter two — `argv` is a `defvaralias`, as in
  startup.el, not a copy — and they are re-installed per run on both cache paths
  rather than baked into the heap image.
- **`skip-syntax-forward` / `skip-syntax-backward`.** Ported from `skip_syntaxes`
  (syntax.c): the syntax-class counterparts of `skip-chars-*`, with the leading
  `^` negation, LIM clamped to the accessible portion, a marker LIM, and an
  unrecognized spec character contributing nothing rather than signalling.
- **A version gate on the fuzz oracle.** `scripts/fuzz_parity.sh` took its ground
  truth from `EMACS="${EMACS:-emacs}"` and checked only that the binary existed.
  A different Emacs does not fail loudly — it reports a different set of
  diverging forms, which reads as a regression. The script now prints the
  resolved oracle path and version on every run and exits 2 when the version is
  not the one BUGS.md's header pins (single-sourced from that line;
  `EMACS_VERSION_EXPECT` overrides for a deliberate cross-version run).
- **A guard against silent identifier collisions.** Extension-op IDs
  (`host::ops`) are hand-assigned `u16` constants and subr names go through
  `defsubr`, which ends in `set_function` — so two registrations sharing a number
  or a name merge without a conflict marker and the later one silently replaces
  the earlier. `tests/registration_ids_unique.rs` reads both sets out of the
  source and fails on a duplicate ID, a duplicate constant name, an op with
  anything other than one `ext_dispatch` arm, or a repeated subr name. The audit
  it encodes found no collisions in the current tree.
- **`char-width` is a subr**, matching Emacs's `Fchar_width` (`indent.c`). It was
  a prelude `defun`; `format` needs the same width table from Rust, and two
  copies would be two answers.
- **Constant symbols.** `nil`, `t`, and keywords interned in the standard obarray
  are constant, as in Emacs's `make_symbol_constant`. `set`, `setq`,
  `set-default`, `setq-default`, `makunbound`, `let`, `let*`, `setq-local`,
  `make-local-variable`, `make-variable-buffer-local`, and `defconst` all signal
  `(setting-constant SYM)` for them.
- **The two other buffers `emacs -Q --batch` starts with.** `buffer-list` is
  `("*scratch*" " *Minibuf-0*" "*Messages*")`, so `(get-buffer "*Messages*")`
  answers a live buffer instead of nil.
- **`keywordp` is a subr.** The test is obarray identity, not the leading colon:
  `(make-symbol ":u")` and `(intern ":u" (obarray-make))` are not keywords, which
  a prelude name-prefix check could not express.

### Fixed
- **`\(?N:RE\)` named the wrong capture group.** `fancy_regex` numbers groups
  positionally and the translator dropped the explicit number, so
  `(string-match "\\(?2:a\\)" "a")` left `(match-data)` as `(0 1 0 1)` against
  Emacs's `(0 1 nil nil 0 1)`, `match-beginning` was inverted, and `\N` in a
  `replace-regexp-in-string` replacement read an empty group. Emacs continues
  counting from `N + 1` (`regnum = N`), so `\(?5:a\)\(b\)` has groups 5 and 6.
  `\(?0:` and `\(?i:` now report Emacs's `(invalid-regexp "Invalid regular
  expression")` rather than the regex crate's parser text.
- **Special forms had no function cell.** `(fboundp 'if)` was nil,
  `(symbol-function 'if)` was nil, and `(funcall 'if 1 2)` reported
  `(void-function if)`. In Emacs they are subrs with `max_args == UNEVALLED`:
  `#<subr if>`, `subrp` t, `subr-name` `"if"`, and `(invalid-function #<subr
  if>)` on a call. `(functionp 'if)` stays nil.
- **A warm script cache lost the introspection function cells.**
  `(fboundp 'when)` answered `t` on a cold run and nil on every run after, since
  the `(macro . FUNCTION)` cells live in a host side table the heap image never
  carried. Shard format v8 carries it.
- **base64 encoded the UTF-8 expansion instead of the bytes**
  (`"\303\251"` → `"w4PCqQ=="`, not `"w6k="`, and it did not round-trip through
  the decoder), and the decoder accepted `"YWJ"`, `"="`, `"===="`, `"A==="`,
  `"AB=C"` and the base64url alphabet, all of which Emacs rejects with
  `(error "Invalid base64 data")`. A character above 255 now signals
  `(error "Multibyte character in data for base64 encoding")`.
- **`url-unhex-string` decoded UTF-8** — `"%CE%B1"` came back as one character
  instead of two, and a string with no escapes at all came back longer than it
  went in. `url-hexify-string` now takes any sequence, as Emacs's `mapconcat`
  implementation does.
- **The digest range arguments indexed characters and were clamped.** They are
  byte offsets into the encoded text, must be integers, count from the end when
  negative, and signal `(args-out-of-range OBJECT START END)` outside —
  `(md5 "abc" 5 nil)` used to answer the digest of `""`. An unknown algorithm now
  reports Emacs's `(error "Invalid algorithm arg: bogus")`.
- **Seven argument checks named the wrong predicate or none at all**:
  `(string "a")` and `(format "%c" -1)` (`characterp`), `(copysign 1 2.0)`
  (`floatp`), `(lsh "" 6)` (`number-or-marker-p`, where COUNT still reports
  `integerp`), `(atan -2 'car)` (`numberp`), `(member-ignore-case "a" 1.5)`
  (`listp`, which used to answer nil), and `(upcase (symbol-function '+))`, whose
  datum was dropped because `#<subr +>` has no read syntax.
- **`(logb 2305843009213693951)`** answered 61 for lack of an exact integer path
  (2^61-1 rounds up to 2^61 in `f64`); Emacs answers 60. A NaN now passes through
  with its own sign and payload.
- **The time ZONE argument was never validated.** A float, a vector, a
  one-element list and any symbol other than `wall` are
  `(error "Invalid time zone specification" ZONE)`; all were accepted and read as
  UTC, and `wall` — which means *local* — was read as UTC too.
- **A bool-vector's packed bytes ignored `print-escape-control-characters`** —
  the flag was applied to plain strings only, so a zero byte printed as a raw NUL
  where Emacs writes `\0`.
- **`end-of-file` dropped its datum.** Inside a loaded file Emacs signals
  `(end-of-file "/path/to/file.el")`; only a bare `--eval` gets `(end-of-file)`.
  `examples/error-data.el` pinned the old empty form and called it
  oracle-verified — `emacs -Q --batch -l` fails that assertion — so the example
  now asserts the shape Emacs actually produces.
- **`(intern "nil")` built a new symbol** that printed as `nil` but was not `eq`
  to it, so `(and (intern "nil") 1)` answered 1. Same root as `(intern-soft t)`
  answering nil and `(indirect-function t)` answering `t` instead of nil.
- **`advice-add` on a Rust subr corrupted it for the rest of the process.**
  `(advice-add 'car :filter-return #'1+)` did not just fail — every later `car`,
  including `(symbol-function 'car)`, raised
  `(wrong-type-argument number-or-marker-p #[nil ((get v 'defalias-fset-function)) …])`.
  `advice-add` advises the function cell before calling
  `(add-function :around (get symbol 'defalias-fset-function) …)`, and `gv-deref`
  is `(funcall (car ref))`, so the getter closure went to the advice it had just
  installed. Emacs is immune because its preloaded Lisp is **byte-compiled**, and
  the byte compiler turns ~80 primitives into opcodes that never consult the
  function cell. That set is now measured (not guessed: byte-compile a wrapper,
  advise the name, see whether the advice fires — 82 names it does not fire for,
  30 it does) and lowered the same way inside the prelude only, since an
  interpreted user `defun` in Emacs *does* honour advice on `car`. See BUGS.md
  R19-A, with a cold/warm cache regression.
- **A hardcoded-reference-string audit of `src/`** — 776 literals extracted, 751
  checked against Emacs 30.2's Lisp sources, binary string table and `etc/DOC`,
  131 executed side by side; **40 sites / 30 texts were wrong and are fixed.**
  The reader now signals `(end-of-file)` and `(invalid-read-syntax …)` instead of
  nine kinds of lowercase prose, and reads `##` and `#:foo`; the four
  `Wrong type argument: closurep` OClosure sites signal
  `(cl-assertion-failed (closurep oclosure))`; `setf` on a bad place signals
  `gv-invalid-place` from all three paths (two of which disagreed with each other
  over a colon); `cyclic-variable-indirection`, `map-not-inplace`,
  `pcase-exhaustive`, `char-table-range`'s curly quotes, and five `rx`
  diagnostics now read as Emacs writes them; `setcar`/`setcdr`/`unintern`/the
  hash-table accessors/`buffer-local-value` name the offending value;
  `goto-char`/`forward-char`/`char-equal` use `integer-or-marker-p`/`fixnump`/
  `characterp` instead of one shared `integerp`; `fset`, `get-buffer-create ""`,
  `(format "%")` and `buffer-substring` out of range all signal what Emacs
  signals. Full table in BUGS.md R19-B.
- **`replace-match` panicked the interpreter thread.** Match data is global and
  outlives the buffer or string it was set from, and the span was indexed with no
  bounds check: `(with-temp-buffer (insert "abc") (replace-match "x"))` aborted
  with `range start index 8 out of range for slice of length 3`. `Freplace_match`
  bounds-checks against BEGV/ZV (and against `SCHARS` in string mode) and signals
  `args-out-of-range`; a missing subexpression is
  `(error "replace-match subexpression does not exist" SUBEXP)`.
- **`string-to-number` skipped the wrong whitespace and mis-typed BASE.** Emacs
  skips exactly SPC and TAB, so `(string-to-number "\n12")` is 0 — `\r`, `\f`,
  `\v`, U+00A0 and U+3000 likewise; `trim_start` skipped all of them. BASE is
  `CHECK_FIXNUM`: `(string-to-number "1" 2.0)` truncated the float to base 2 and
  answered 1 where Emacs signals `(wrong-type-argument fixnump 2.0)`, and a
  symbol, bignum or marker said `integerp` instead of `fixnump`.
- **ERT ran tests in definition order, and ran each body in the current
  buffer.** Emacs orders by name (measured over ten deliberately unsorted names)
  and wraps every body in `with-temp-buffer` (ert.el:796), so a body runs in
  ` *temp*` under the standard syntax table — `(char-syntax ?.)` is 46 there,
  against 95 at top level in `*scratch*`. Definition order let a test depend on
  an earlier test's side effects and pass here while failing under
  `emacs -Q --batch -l`. Both changes were measured across all 71 examples
  before landing: `ELISPRS_CACHE=0` turns the ~60 s per example (which is the
  rkyv cache write, not the evaluation) into 3.1 s, so the full gate runs in
  about four minutes — 71 examples, 0 failing.
- **Error DATA dropped any value with no read syntax.** The data list is rebuilt
  by re-reading the rendered message, so a marker, buffer or closure — printed as
  `#<…>` / `#[…]` — vanished, leaving `(wrong-type-argument fixnump)` with no
  offender. `ElispHost::signal_wrong_type` now carries the object.
- **`string-to-syntax` dropped the `c` flag, mis-handled `@`, and consed.**
  `Fstring_to_syntax` accepts eight flag characters; `c` (bit 23, the third
  comment style) was the one missing, so `(string-to-syntax ". c")` answered
  `(1)` where Emacs answers `(8388609)`. The inherit class `@` returns nil in
  Emacs, not `(13)`. The error is `"Invalid syntax description letter: %c"`, not
  a `%S` of the whole string. And a flagless, matchless descriptor returns the
  shared `Vsyntax_code_object` cell, so `(eq (string-to-syntax "w")
  (string-to-syntax "w"))` is t, where elisprs consed a fresh cons and answered
  nil.
- **`parse-partial-sexp` clamped an out-of-range region instead of signalling.**
  `validate_region` (editfns.c) rejects the pair and names the buffer and both
  arguments as passed: `(parse-partial-sexp 1 40)` in a three-character buffer is
  `(args-out-of-range #<buffer …> 1 40)`. Clamping answered a plausible state for
  a region that does not exist. Eight probes including narrowing now agree.
- **`capitalize` / `upcase-initials` rejected an out-of-range integer.** Found
  by the fuzzer immediately after the fix below: routing a character argument
  through the title-case table first reported `(wrong-type-argument characterp
  -1)` where `upcase` reports `char-or-string-p`, and would have rejected the
  above-range integers Emacs returns unchanged.
- **`when` / `unless` with an EMPTY body expanded wrongly.** subr.el has a second
  arm for that case, `(progn COND nil)`; only the first was ported, so
  `(macroexpand '(when x))` answered `(if x (progn))`. The value is the same
  either way, which is why only `macroexpand` could see it.
- **The initial buffer's syntax table did not survive a cache hit.** The
  prelude's `(set-syntax-table emacs-lisp-mode-syntax-table)` is a buffer-local
  binding, and buffer locals are not in the heap image, so a warm cache left the
  buffer on `standard-syntax-table`: `(char-syntax ?.)` was 95 cold and 46 warm.
  Every syntax-derived observable followed. Re-installed per run in
  `install_entry_point_state`, the one place both cache paths run through.
- **`capitalize` / `upcase-initials` used the wrong word test and the wrong
  case.** Two bugs in one function, both surfaced by a locale sweep.
  `casefiddle.c` asks the *syntax table* — `case_ch_is_word (SYNTAX (ch))` —
  where elisprs asked "is this a digit or a cased letter". `ﬁ` has no one-to-one
  upper case, so that test called it a non-word character and made the *next*
  letter a word start: `(capitalize "ﬁnd")` answered `"ﬁNd"` where Emacs answers
  `"Find"`. And a word's first character is TITLE-cased, not upper-cased, which
  is one-to-many for some characters and a distinct code point for the Latin
  digraphs: `(capitalize "ßäöü")` is `"Ssäöü"` (not `"ẞäöü"`) and
  `(capitalize "ǳa")` is `"ǲa"` (not `"Ǳa"`). `case_character`'s Greek rule is in
  too — a capital sigma that ends a word down-cases to ς, so `(capitalize "ΟΔΟΣ")`
  is `"Οδος"`. `case-symbols-as-words` is honored. Measured character by
  character over the whole BMP: 55,295 characters, 0 remaining differences in
  what the two engines *do*, and 8 characters where Emacs 30.2's case table has
  no entry at all (see the Unicode-version note in BUGS.md).
- **An integer and a float compared through `f64`, losing Emacs's exact rule.**
  `(expt 3 34)` is 16677181699666569 — a fixnum, and past 2^53, where an `f64`
  runs out of mantissa — so its float image is 16677181699666568.0, a different
  number. Emacs's `arithcompare` decides on exact values, so `(= L (float L))`
  is nil and `(> L (float L))` is t; elisprs rounded both operands and answered
  the opposite. Three sites had to agree and did not: the numeric hook fusevm
  calls (`host::apply_num_op`), the `=` `<` `>` `<=` `>=` subrs
  (`builtins::cmp`), and `max`/`min` (`builtins::min_max`) — each compared
  `(Int, Int)` exactly but fell back to `to_f64` for a mixed pair. `min` showed
  it plainly: `(min L (float L))` answered the integer where Emacs answers the
  float. All three now use `host::num_cmp`, which compares exact values and
  treats a NaN as incomparable. Arithmetic is unchanged and stays
  float-contagious, as in Emacs. A *two-argument* comparison is lowered to a
  fusevm op that 0.17.0 answered natively on rounded images; the 0.22.0 bump
  delegates that pair to the hook, so `(= L F)` and `(/= L F)` are correct too
  and the whole family now matches `emacs -Q --batch`.
- **`nil`, `t`, and keywords were writable.** `(set :kw 1)` and `(setq :a 1)`
  performed the write and returned the value; `(set nil 1)` signalled
  `(set "not a symbol")` — a made-up condition naming the builtin rather than
  Emacs's `(setting-constant nil)`. The `SETVAR` and `FSET` dispatch arms also
  discarded the host's `Err` with `let _ =`, so a failed write still let the form
  answer its own value.
- **A keyword was not its own value.** `intern` never seeded the value cell that
  `intern_driver` (lread.c) seeds, so `(boundp :a)` was nil, `(default-value :a)`
  signalled `void-variable`, and `(special-variable-p :a)` was nil — `t`, `:a`,
  and `t` in Emacs.
- **`defvar` overwrote an existing value.** `defvar` initializes only a void
  variable; only `defconst` always assigns. `(progn (setq zz 5) (defvar zz 9) zz)`
  answered 9 and is now 5, which is what makes it safe to `setq` a library's
  variable before loading the library.
- **An empty closure body printed as `()`.** Emacs normalizes an absent body to
  the single form `nil`, so `(lambda ())` prints `#[nil (nil) (t)]`, uniformly
  across `lambda`, `defun`, and `defmacro`.
- **`defvaralias` onto a constant** signals
  `(error "Cannot make a constant an alias: nil")` rather than a
  `"not a symbol"` message.
- **`kill-buffer` could leave a dead buffer current with stale positions, and the
  next `buffer-substring` aborted the process.**
  `(progn (insert "hello") (kill-buffer) (buffer-substring (point-min) (point)))`
  sliced `text[..5]` of a zero-length vector and panicked. The killed slot's
  `point`/`begv`/`zv` now go back to BEG (Emacs's `reset_buffer`), and the
  successor follows `Fkill_buffer`'s `Fset_buffer (Fother_buffer (…))`, whose
  tail creates `*scratch*` when no live buffer remains — so a dead buffer can no
  longer be current.
- **`get-buffer` / `set-buffer` collapsed three distinct Emacs failures into
  one.** Ported `Fget_buffer`, `Fset_buffer` and `nsberror` (`src/buffer.c`): a
  buffer object comes back from `get-buffer` whether or not it is live
  (`#<killed buffer>`, was `nil`), a non-buffer non-string is
  `(wrong-type-argument stringp 5)` before any lookup, an unknown name is
  `No buffer named nope` unquoted, and a killed buffer object is
  `Selecting deleted buffer`.
- **A wrong-arity call to a closure named the symbol, not the closure.**
  `(progn (defun f1 (a) a) (f1 1 2))` reported `f1` where Emacs reports
  `#[(a) (a) (t)]`, and a `defalias`ed second name reported that name.
  `funcall_lambda` signals with the resolved function; only `eval_sub`'s subr
  branch signals with the designator, and the subr rows are unchanged.
- **`\sC`, `\SC`, `\w` and `\W` ignored the syntax table.** `translate_escape`
  mapped a few class letters to fixed sets and fell back to whitespace for the
  rest, so `(string-match "\\s_" "-")` was `nil` (Emacs: `0`) and no class
  noticed `with-syntax-table` or `modify-syntax-entry`. They now compile to an
  explicit character class built from the live table's runs. `\w` is the table's
  word class too, so `(string-match "\\w" "_")` is `nil` as in Emacs, and the
  class is emitted `(?-i:…)` because Emacs never case-folds a syntax class.
- **`format`'s field width and `%.Ns` precision counted characters, not display
  columns.** `(format "%.3s" "\tXY")` was `"\tXY"` and is now `""`; `(format
  "%5s" "a\tb")` no longer pads. Columns apply to every conversion — `(format
  "%4c|" ?中)` is `"  中|"` — and `%S` gained the precision it never applied at
  all.
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
  evaluating either argument. A symbol `fset` has pointed at a subr once is
  recorded and always carries the guard afterwards, so retargeting it at a
  *narrower* subr — `(fset 'f (symbol-function 'cons))` then
  `(fset 'f (symbol-function 'cdr))` — is caught even though the cell live when
  the call compiled accepted the count. Verified across the interpreter, a warm
  bytecode cache and `--aot-exe` (caught and uncaught). `cache::SHARD_FORMAT_VERSION`
  6 → 7, since a v6 chunk carries no guard and would otherwise keep serving the
  old behaviour from a warm cache.

### Added
- **`setFunctionBreakpoints` in `--dap`.** The adapter answered the request from
  its catch-all arm — `success: true` with an empty body, no `breakpoints` array —
  and never advertised `supportsFunctionBreakpoints`, so a conforming client would
  not have sent it in the first place and naming a function produced no stop at
  all. The handler is ported from awkrs `src/dap.rs:408` (identical in strykelang
  `dap.rs:414`) and the firing from awkrs `src/vm.rs:3226`: entering a named
  function *arms step mode* rather than pausing on the call, so the stop lands on
  the function's first real statement instead of on the call site. It is hooked at
  the single place a user function body is entered — `host::call_function`'s
  closure arm, which every compiled `CALL`, `funcall` and `apply` funnels through.
  Which calls match is elisp-specific and was measured against GNU Emacs 30.2 with
  `advice-add`: instrumentation lives on the *function cell*, so `(add2 1)`,
  `(funcall 'add2 1)`, `(apply 'add2 '(1))` and `(funcall (symbol-function 'add2) 1)`
  all break, while a function object captured before a redefinition does not. The
  set can be replaced or cleared while the executor is paused. One deviation from
  awkrs, stated rather than hidden: awkrs arms step mode and nothing else, so its
  stop is indistinguishable from a `next`. The cause is carried through here, so
  the stop reports the DAP-specified `"function breakpoint"` reason; the
  attribution is consumed by the stop it labels, and stepping on from there is an
  ordinary `"step"`.
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

### Fixed (Emacs parity — round 13, propertized-string read syntax, cl-seq `*-if`, nil's two spellings, Latin-1 syntax)
Found by `scripts/fuzz_parity.sh` (seeds 777 and 424242, 3,000 and 5,000 forms),
by hand-built probe corpora, and by a self-consistency sweep that calls every
bound function with a literal `nil` and with `(= 5 42)` and reports any answer
that differs. Every expectation was byte-verified against `emacs -Q --batch` 30.2.

- **`#("TEXT" START END PLIST …)` was read as a bare string.** The reader
  consumed the intervals and dropped them, so a propertized literal lost every
  property it was written with and `(read "#(\"foo\" 0 3 (a 1))")` answered
  `"foo"`. It is now lread.c's `#(` arm: the string, then triples applied with
  `Fset_text_properties` — including `validate_interval_range`'s range swap and
  bounds check and `validate_plist`'s odd-length error and non-list wrapping —
  with lread.c's own `invalid-read-syntax "#"` / `"Invalid string property list"`
  diagnostics.
- **Error DATA lost a string's text properties.** `make_error_object` rebuilds a
  condition's data by re-reading the rendered message, so the reader gap above
  made `(car (propertize "foo" 'a 1))` report `(wrong-type-argument listp "foo")`
  where Emacs reports `(wrong-type-argument listp #("foo" 0 3 (a 1)))`. Same for
  `args-out-of-range` on `aref` / `elt`.
- **A nil predicate crashed every cl-seq `*-if` function.** cl-seq.el's `*-if`
  wrappers pass their predicate through as `:if`, and `cl--check-test-nokey`
  falls through to `(eql ITEM X)` — with the implicit nil ITEM — when it is nil,
  so `(cl-position-if nil '(1 nil 2))` is `1` in Emacs and was `(void-function
  nil)` here. Fixed across `cl-position-if`, `cl-find-if`, `cl-count-if`,
  `cl-member-if`, `cl-assoc-if`, `cl-rassoc-if`, `cl-substitute-if`, their
  `-if-not` siblings, and `cl-remove-if` / `cl-remove-if-not` / `cl-delete-if` /
  `cl-delete-if-not`.
- **`cl-subst-if`, `cl-subst-if-not`, `cl-nsubst-if`, `cl-nsubst-if-not` and
  `cl-nsublis` were missing.** Added as cl-seq.el defines them — `cl-sublis` over
  the one-entry alist `((nil . NEW))` with the predicate as `:if` / `:if-not`,
  which is why `(cl-subst-if 9 nil '(1 nil 2))` is `(1 9 2 . 9)`. `cl-sublis` now
  honours `:if` / `:if-not` as well as `:test` / `:test-not` / `:key`.
- **`nil` has two VM spellings and half the runtime only knew one.** A literal
  `nil` compiles to fusevm `Undef`, but a comparison that answers false produces
  `Value::Bool(false)`; both are elisp's one `nil`. Treating only `Undef` as the
  empty list made `(length (= 5 42))` signal `(wrong-type-argument sequencep nil)`
  — an error naming the very value it refused to recognise — and did the same for
  `reverse`, `mapcar`, `mapc`, `mapcan`, `mapconcat`, `sort`, `apply`, `butlast`,
  `nbutlast`, `delete-dups`, `string-join`, `seq-empty-p`, `cl-list-length`,
  `symbol-name`, `symbol-value`, `default-toplevel-value`, `run-hooks`,
  `run-hook-with-args`, `bare-symbol-p` and `assoc-string`. There is now one
  `el_nil` predicate and `list_vec` / `sym_name` / `length` / the symbol-value
  cells use it.
- **`standard-syntax-table` and the current buffer's table were each other's.**
  Round 11 read `(char-syntax C)` in the batch `*scratch*` buffer and wrote those
  answers into `standard-syntax-table`, but those are two different tables:
  `emacs -Q --batch` starts in `lisp-interaction-mode`, whose table answers
  `(95 62 95 95 60)` for `(char-syntax 1)`/`?\n`/`?\r`/`127`/`?\;` while
  `(with-syntax-table (standard-syntax-table) …)` answers `(46 32 32 46 46)`.
  `standard-syntax-table` is restored to syntax.c `init_syntax_once`'s, and the
  initial buffer now gets the `emacs-lisp-mode-syntax-table` the prelude already
  builds. Over all 256 characters, `(char-syntax C)` goes from 16 wrong to 0 and
  `(with-syntax-table (standard-syntax-table) (char-syntax C))` from 31 to 0.
  Round 11's `\s-` fix is unaffected — it is a fixed character set in
  `src/regexp.rs`, not a table read.
- **`standard-syntax-table` called the whole Latin-1 supplement word.**
  syntax.c defaults everything at or above U+0080 to word and characters.el's
  Latin-1 block then reclassifies its punctuation and symbols, so `(char-syntax
  ?¡)` is `?.` and `(char-syntax ?±)` is `?_` in Emacs where both were `?w` here.
  U+0080–U+00FF now matches Emacs 30.2 character for character.

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
