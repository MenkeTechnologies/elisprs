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

`elisprs` runs **Emacs Lisp** (`.el`) as standalone programs from the command line: a **Lisp-2** obarray (separate value/function cells) with dynamic binding and an elisp-correct reader, built on the [`rust_lisp`](https://crates.io/crates/rust_lisp) value model and engineered to be lowered onto the [`fusevm`](https://github.com/MenkeTechnologies/fusevm) bytecode VM — the same engine behind `zshrs`, `stryke`, `awkrs`, and `vimlrs`.

 ┌──────────────────────────────────────────────────────────────┐
 │ STATUS: MILESTONE 1 &nbsp; ENGINE: TREE-WALK &nbsp; TARGET: FUSEVM &nbsp; ███░░░░░ │
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
- [\[0x06\] Roadmap](#0x06-roadmap--path-to-fusevm)
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

**Milestone status:** Today elisp runs on a self-contained **tree-walk evaluator**. The `fusevm` lowering (`src/compiler.rs`) is the milestone-2 seam and is not wired yet — so, unlike `vimlrs`, elisprs is *being lowered onto* fusevm, not yet hosted on it. See [§0x06](#0x06-roadmap--path-to-fusevm).

---

## [0x01] SYSTEM REQUIREMENTS

- **Rust** 2021 edition (stable). Builds on `rustc` 1.96+.
- **Platforms:** macOS (aarch64 / x86_64) and Linux (x86_64 / aarch64).
- **Dependencies:** one direct dependency at milestone 1 — `rust_lisp` (MIT) — so the crate builds offline and fast. `fusevm` is added at milestone 2 when lowering begins.

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
elisp --aot FILE -o a.o  # AOT-compile to a native object    (milestone 2)
elisp --version
```

---

## [0x03] LANGUAGE COVERAGE

**Reader syntax.** integers, floats, strings (with escapes), symbols (including `1+` / `1-` / `<=` / `:keywords`), `nil` / `t`, `'quote`, `#'function`, `?c` char literals, `;` comments.

**Special forms (21).** `quote` `function` `lambda` `progn` `prog1` `if` `when` `unless` `cond` `and` `or` `while` `setq` `let` `let*` `defun` `defmacro` `defvar` `defconst` `condition-case` `unwind-protect`.

**Subrs (~65).**

| Group | Functions |
|---|---|
| Arithmetic | `+ - * / % mod 1+ 1- abs max min = /= < > <= >=` |
| Lists | `car cdr cons list append nth nthcdr reverse length member memq assoc assq` |
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

**Known limitations (milestone 1)** — surfaced loudly rather than silently misread:

- **No dotted pairs.** `rust_lisp`'s cons cell always has a *list* cdr, so `(cons 1 2)` / `(a . b)` cannot be represented; both the reader and `cons` error on a non-list cdr. Alists must use `(key value)`, not `(key . value)`. Replacing the cons model is the top milestone-2 item.
- **No backquote / unquote.** `` ` `` and `,` are rejected by the reader.
- **Dynamic scope only.** `lexical-binding` is not honored yet.
- **`setcar` / `setcdr`** are absent (`rust_lisp`'s `List` doesn't expose cons mutation) — arrives with the new cons model.

This is a useful elisp core, **not** the ~1000-subr GNU Emacs surface, and it is not buffer-aware — editor integration (buffers, point, markers) is a separate track.

---

## [0x04] ARCHITECTURE // REUSE-OWN SPLIT

```
.el source  →  reader.rs  →  Value (rust_lisp model)  →  eval (Lisp-2 + dynamic)   [milestone 1]
                                                      ↘  compiler.rs → fusevm::Chunk [milestone 2]
```

Function objects (closures, macros, subrs) ride in `Value::Foreign(Rc<dyn Any>)` and are downcast on the way out, so elisprs gets elisp function semantics without forking `rust_lisp`'s `Value` enum.

| File | Role |
|---|---|
| `src/reader.rs` | Elisp-correct S-expression reader → `rust_lisp` `Value`s |
| `src/interp.rs` | Lisp-2 obarray, dynamic binding, `eval`, special forms, printer |
| `src/builtins.rs` | The subr standard library |
| `src/callable.rs` | `Callable` (subr/closure/macro), stored in `Value::Foreign` |
| `src/compiler.rs` | **Seam:** lower elisp forms to `fusevm::Chunk` (milestone 2) |
| `src/aot.rs` | `--aot` driver over `compiler::lower` + `fusevm::aot` |
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
| Subr standard library (~65) | Working |
| `elisp` CLI — file / `-e` / REPL | Working |
| `--lsp` / `--dap` servers | Stub (planned) |
| `--aot` → `fusevm::aot::compile_object` | Planned (milestone 2) |
| AST → `fusevm::Chunk` lowering (`compiler.rs`) | Seam (milestone 2) |
| Dotted pairs, backquote, lexical binding, `setcar`/`setcdr` | Not yet |

---

## [0x06] ROADMAP // PATH TO FUSEVM

**Milestone 2 — execute on `fusevm`** (the reason elisprs exists), the same frontend pattern as `strykelang/strykelang/fusevm_native.rs`:

1. Add `Value::{Cons, Symbol}` to `fusevm/src/value.rs` and a dynamic-binding stack to the `VM` struct — the one genuinely invasive core change (the other fusevm frontends never needed dynamic scope or Lisp cells).
2. Reserve an elisp `Op::Extended(id, arg)` range; register a handler via `vm.set_extension_handler(...)` for quote / funcall / special-var bind / cons navigation.
3. Lower each top-level form in `compiler.rs`; lambda bodies become sub-chunks.
4. Bind the subr library through `vm.register_builtin(...)`.

Then the three-tier Cranelift JIT and `--aot` (via `fusevm::aot::compile_object`) come for free, the way they do for the sibling frontends.

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

Coverage spans `reader.rs` unit tests (number-vs-symbol tokenization, `#'` desugaring, `?c` char literals, dotted-pair rejection) and the end-to-end evaluation suite in [`tests/eval.rs`](tests/eval.rs) — arithmetic, recursion, higher-order functions, special forms, macros, and error handling driven through the public `Interp` API.

---

## [0x09] DOCUMENTATION // RENDERED HTML + MARKDOWN

`docs/` is published to GitHub Pages and is the authoritative source for the rendered reference + engineering report.

| Doc | Source | Live URL |
|---|---|---|
| User reference (architecture, coverage, status, taste) | [`docs/index.html`](docs/index.html) | <https://menketechnologies.github.io/elisprs/> |
| Engineering report (reuse/own split, path to fusevm, dependency posture) | [`docs/report.html`](docs/report.html) | <https://menketechnologies.github.io/elisprs/report.html> |

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
