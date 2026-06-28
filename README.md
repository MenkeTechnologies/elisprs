```
███████╗██╗     ██╗███████╗██████╗ ██████╗ ███████╗
██╔════╝██║     ██║██╔════╝██╔══██╗██╔══██╗██╔════╝
█████╗  ██║     ██║███████╗██████╔╝██████╔╝███████╗
██╔══╝  ██║     ██║╚════██║██╔═══╝ ██╔══██╗╚════██║
███████╗███████╗██║███████║██║     ██║  ██║███████║
╚══════╝╚══════╝╚═╝╚══════╝╚═╝     ╚═╝  ╚═╝╚══════╝
```

[![CI](https://github.com/MenkeTechnologies/elisprs/actions/workflows/ci.yml/badge.svg)](https://github.com/MenkeTechnologies/elisprs/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-online-blue.svg)](https://menketechnologies.github.io/elisprs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![status](https://img.shields.io/badge/status-milestone%201%20%C2%B7%20early-9b5de5.svg)

### `[EMACS LISP // RUN .EL OUTSIDE EMACS // LISP-2 + DYNAMIC SCOPE // RUST CORE]`

> *"The editor's language — without the editor."*

`elisprs` runs **Emacs Lisp** (`.el`) as standalone programs from the command line: a **Lisp-2** obarray (separate value/function cells) with dynamic binding and an elisp-correct reader, built on the [`rust_lisp`](https://crates.io/crates/rust_lisp) value model and lowered to — and run on — the [`fusevm`](https://github.com/MenkeTechnologies/fusevm) bytecode VM, the same engine behind `zshrs`, `stryke`, `awkrs`, and `vimlrs`. elisprs is a **pure frontend**: no bespoke VM or JIT — it compiles each form to a `fusevm::Chunk` and fusevm executes it, calling back into the elisp object heap through fusevm's extension handler.

 ┌──────────────────────────────────────────────────────────────┐
 │ STATUS: MILESTONE 1 &nbsp; ENGINE: FUSEVM &nbsp; FRONTEND: PURE &nbsp; ████░░░░ │
 └──────────────────────────────────────────────────────────────┘

### [`Read the Docs`](https://menketechnologies.github.io/elisprs/) &middot; [`Engineering Report`](https://menketechnologies.github.io/elisprs/report.html) · [`strykelang`](https://github.com/MenkeTechnologies/strykelang) · [`zshrs`](https://github.com/MenkeTechnologies/zshrs) · [`fusevm`](https://github.com/MenkeTechnologies/fusevm)

---

## Table of Contents

- [\[0x00\] System Scan](#0x00-system-scan)
- [\[0x01\] System Requirements](#0x01-system-requirements)
- [\[0x02\] Installation](#0x02-installation--arm-the-payload)
- [\[0x03\] Language Coverage](#0x03-language-coverage)
- [\[0x04\] Architecture](#0x04-architecture--reuse-own-split)
- [\[0x05\] Status](#0x05-status--component-grid)
- [\[0x06\] Roadmap](#0x06-roadmap)
- [\[0x07\] Build](#0x07-build--compile-the-payload)
- [\[0x08\] Test](#0x08-test--integrity-verification)
- [\[0x09\] Documentation](#0x09-documentation--rendered-html--markdown)
- [\[0xFF\] License](#0xff-license)

---

## [0x00] SYSTEM SCAN

**Positioning:** Emacs Lisp has only ever run inside Emacs. `elisprs` takes the language out of the editor and runs `.el` as ordinary programs — with a REPL, no Emacs process required. It is built to become the fifth language hosted on [`fusevm`](https://github.com/MenkeTechnologies/fusevm), after `zshrs`, `stryke`, `awkrs`, and `vimlrs`.

**Why it's built this way:** Emacs Lisp is a **Lisp-2** (every symbol carries a separate *value* cell and *function* cell) and is, by default, **dynamically scoped**. Those two facts are the whole personality of the language, and no general-purpose embeddable Lisp gives them to you for free — so the crate reuses a value model and owns the semantics:

| Layer | Source |
|---|---|
| `Value` / `List` / `Symbol` data model | **reused** from `rust_lisp` (MIT) |
| Reader (`1+`/`1-`, `#'foo`, `?c`, `:kw`) | **ours** — `rust_lisp`'s parser mis-tokenizes elisp syntax |
| Lisp-2 obarray, dynamic binding, special forms, subrs | **ours** — `rust_lisp`'s Lisp-1/lexical `eval` is not used |

**Milestone status:** elisp is **hosted on `fusevm`** — like `vimlrs`, elisprs is a pure frontend with no bespoke VM or JIT. Each top-level form is read, macro-expanded, and lowered to a `fusevm::Chunk` (`src/compiler.rs`); fusevm executes it and calls back into the elisp object heap (`src/host.rs`) through a registered extension handler, with cons/symbol/vector cells riding the VM as `Value::Obj` heap handles. Remaining work is coverage plus turning on the JIT/AOT tiers fusevm already provides — see [§0x06](#0x06-roadmap).

---

## [0x01] SYSTEM REQUIREMENTS

- **Rust** 2021 edition (stable). Builds on `rustc` 1.96+.
- **Platforms:** macOS (aarch64 / x86_64) and Linux (x86_64 / aarch64).
- **Dependencies:** two core dependencies — `rust_lisp` (MIT, the `Value` model) and `fusevm` (the bytecode VM elisp executes on). The crate still builds offline and fast.

---

## [0x02] INSTALLATION // ARM THE PAYLOAD

```sh
git clone https://github.com/MenkeTechnologies/elisprs   # from source
cd elisprs && cargo build --release
```

The build produces the `elisp` binary:

```sh
elisp FILE.el            # evaluate a file
elisp -e "(+ 1 2)"       # evaluate an expression, print its value
elisp                    # REPL (balanced-paren continuation, Ctrl-D to exit)
elisp --lsp              # language server over stdio        (stub — see roadmap)
elisp --dap              # debug adapter over stdio          (stub — see roadmap)
elisp --aot FILE -o a.o  # lower to a fusevm chunk (native object pending)
elisp --version
```

---

## [0x03] LANGUAGE COVERAGE

**Reader syntax.** integers, floats, strings (with escapes), symbols (including `1+` / `1-` / `<=` / `:keywords`), `nil` / `t`, `'quote`, `#'function`, `?c` char literals, `;` comments.

**Special forms (21).** `quote` `function` `lambda` `progn` `prog1` `if` `when` `unless` `cond` `and` `or` `while` `setq` `let` `let*` `defun` `defmacro` `defvar` `defconst` `condition-case` `unwind-protect`.

**Subrs (~80).**

| Group | Functions |
|---|---|
| Arithmetic | `+ - * / % mod 1+ 1- abs max min = /= < > <= >=` |
| Lists | `car cdr cons list append nth nthcdr reverse length member memq assoc assq` |
| Mutation | `setcar setcdr aset` |
| Vectors | `vector make-vector aref vectorp` |
| Hash tables | `make-hash-table gethash puthash remhash clrhash maphash hash-table-count hash-table-keys hash-table-values hash-table-p` |
| Predicates | `eq eql equal null not numberp integerp floatp stringp symbolp consp listp atom functionp` |
| Symbols/cells | `set symbol-value symbol-function fset boundp fboundp symbol-name intern make-symbol` |
| Strings | `concat string= string-equal string< upcase downcase number-to-string string-to-number` |
| IO/format | `format message princ prin1 print terpri` |
| Functional | `funcall apply mapcar mapc identity` |

`defun`/`defmacro`/`lambda` support `&optional` and `&rest`; macros expand and re-evaluate; `condition-case` matches the `error` umbrella and specific error symbols.

**A taste** (runnable as [`examples/demo.el`](examples/demo.el) via `elisp examples/demo.el`):

```elisp
(defun fact (n) (if (<= n 1) 1 (* n (fact (1- n)))))
(fact 6)                                  ; => 720

(mapcar (lambda (x) (* x x)) '(1 2 3 4))  ; => (1 4 9 16)
(mapcar #'1+ '(10 20 30))                 ; => (11 21 31)

(let ((x 10) (y 20)) (+ x y))             ; => 30

(format "%s = %d (hex %x)" 'count 255 255); => "count = 255 (hex ff)"

(condition-case e (/ 1 0)
  (arith-error (format "caught %s" e)))   ; => "caught (arith-error division by zero)"
```

**Now supported** (own cons model — `Obj::Cons(Value, Value)` heap cells, not `rust_lisp`'s list-only cdr):

- **Dotted pairs.** `(cons 1 2)` / `(a . b)` read, print (`(1 . 2)`), and round-trip; alists may use `(key . value)`.
- **Backquote / unquote.** `` ` ``, `,`, and `,@` are read and expanded.
- **`setcar` / `setcdr`** mutate cons cells in place.

**Known limitations** — surfaced loudly rather than silently misread:

- **Dynamic scope only.** `lexical-binding` is not honored yet (the lexical-binding frontend is in progress).

This is a useful elisp core, **not** the ~1000-subr GNU Emacs surface, and it is not buffer-aware — editor integration (buffers, point, markers) is a separate track.

---

## [0x04] ARCHITECTURE // REUSE-OWN SPLIT

```
.el source  →  reader.rs  →  forms on the ElispHost heap  →  compiler.rs → fusevm::Chunk  →  fusevm executes (calls back into host.rs)
```

elisp cells (cons / symbol / vector / closure / macro / subr) live in the `ElispHost` object heap and ride the VM as `Value::Obj(u32)` handles, so elisprs gets full elisp semantics — including dynamic scope and Lisp-2 cells — without forking either `rust_lisp`'s `Value` enum or the `fusevm` core.

| File | Role |
|---|---|
| `src/reader.rs` | Elisp-correct S-expression reader → forms on the `ElispHost` heap |
| `src/host.rs` | `ElispHost`: the object heap, Lisp-2 obarray, dynamic binding, and the `fusevm` extension handler that runs elisp ops |
| `src/compiler.rs` | Lowers elisp forms to a `fusevm::Chunk`; lambda bodies become sub-chunks |
| `src/builtins.rs` | The subr standard library (reached host-side from the `CALL` extension op) |
| `src/prelude.rs` | The `[DERIVED]` elisp prelude — breadth written in elisp on top of the primitives |
| `src/aot.rs` | `--aot` driver: lowers a `.el` file to a `fusevm::Chunk` (native object via `fusevm::aot` pending) |
| `src/lsp.rs` / `src/dap.rs` | `--lsp` / `--dap` servers (stubs) |
| `src/main.rs` | The `elisp` CLI + REPL |

---

## [0x05] STATUS // COMPONENT GRID

The grid reflects the current state of the tree, not aspiration — planned items are labelled.

| Component | State |
|---|---|
| Elisp-correct reader (`1+`/`#'`/`?c`/`:kw`, `nil`/`t`, `'quote`) | Working |
| `Value` / `List` / `Symbol` model (`rust_lisp`) | Reused |
| Lisp-2 obarray (value + function cells) | Working |
| Dynamic binding (`let`/`let*`, special vars) | Working |
| Special forms (21) + macros (`defmacro`) | Working |
| Subr standard library (~80) | Working |
| Hash tables (`make-hash-table`/`gethash`/`puthash`/`maphash`) | Working |
| Dotted pairs, backquote/unquote, `setcar`/`setcdr` | Working |
| `elisp` CLI — file / `-e` / REPL | Working |
| ERT test surface (`ert-deftest`/`should`/`should-error`) | Working (prelude) |
| `--lsp` / `--dap` servers | Stub (planned) |
| elisp → `fusevm::Chunk` lowering + execution (`compiler.rs` / `host.rs`) | Working |
| `--aot` → native object via `fusevm::aot::compile_object` | Planned (lowering works; native emit pending) |
| `lexical-binding` | In progress |

---

## [0x06] ROADMAP

**Done — elisp executes on `fusevm`** (the reason elisprs exists), the same frontend pattern as the sibling languages:

1. ✅ elisp cells (cons / symbol / vector / closure) live in the `ElispHost` heap (`src/host.rs`) and ride the VM as `Value::Obj` handles — no invasive `fusevm` core change was needed (it never had to learn dynamic scope or Lisp cells).
2. ✅ An elisp `Op::Extended(id, arg)` range dispatches quote / funcall / special-var bind / cons navigation through a handler registered with `vm.set_extension_handler(...)`.
3. ✅ Every top-level form lowers in `compiler.rs`; lambda bodies become sub-chunks.
4. ✅ The subr library is reachable host-side from the `CALL` extension op.

**Next:**

- **JIT / AOT tiers.** Build `fusevm` with the `jit` feature so elisp chunks pick up the three-tier Cranelift JIT, and wire `--aot` native-object emission through `fusevm::aot::compile_object` (today `--aot` lowers to a chunk; native emission needs fusevm's `aot` feature). Both come essentially for free, the way they do for the sibling frontends.
- **Coverage.** Broaden special-form / macro / backquote lowering toward full milestone-2 elisp.

**Tooling.** `--lsp` (completion/hover/definition/diagnostics over the obarray, mirroring `awkrs --lsp`), `--dap` (breakpoints/stepping off `eval` + the dynamic specstack, reusing `zemacs-dap` transport), and editor plugins (`vscode-elisp` / `vim-elisp` / `emacs-elisp`).

---

## [0x07] BUILD // COMPILE THE PAYLOAD

```bash
cargo build --release
```

`elisp --help` / `-h` prints the usage screen; `elisp --version` prints the version.

---

## [0x08] TEST // INTEGRITY VERIFICATION

```bash
cargo test
```

Coverage spans `reader.rs` unit tests (number-vs-symbol tokenization, `#'` desugaring, `?c` char literals, dotted-pair reading) and the end-to-end evaluation suite in [`tests/eval.rs`](tests/eval.rs) — arithmetic, recursion, higher-order functions, special forms, macros, and error handling driven through the public `eval_str` API.

The [`examples/*.el`](examples) scripts are self-testing: each uses the prelude's ERT surface (`ert-deftest` / `should` / `should-error`) and `ert-run-tests-batch-and-exit`, which exits non-zero on any failure. [`tests/examples.rs`](tests/examples.rs) runs every example through the built `elisp` binary as a `cargo test` gate, and the CI `examples` job runs them through the release binary on Linux + macOS.

---

## [0x09] DOCUMENTATION // RENDERED HTML + MARKDOWN

`docs/` is published to GitHub Pages and is the authoritative source for the rendered reference + engineering report.

| Doc | Source | Live URL |
|---|---|---|
| User reference (architecture, coverage, status, taste) | [`docs/index.html`](docs/index.html) | <https://menketechnologies.github.io/elisprs/> |
| Engineering report (reuse/own split, fusevm frontend design, dependency posture) | [`docs/report.html`](docs/report.html) | <https://menketechnologies.github.io/elisprs/report.html> |

The HUD-themed HTML docs share `hud-static.css`, `hud-theme.js`, and `tutorial.css` — open them locally via `file://` or browse the GitHub Pages URL above.

---

## [0xFF] LICENSE

 ┌──────────────────────────────────────────────────────────────┐
 │ MIT OR APACHE-2.0 // BUNDLES rust_lisp (MIT) // FREE / OSS   │
 └──────────────────────────────────────────────────────────────┘

---

```
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
░░ >>> READ THE FORM. BIND THE SYMBOL. EVAL THE LIST. <<< ░░
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
```

##### created by [MenkeTechnologies](https://github.com/MenkeTechnologies)
