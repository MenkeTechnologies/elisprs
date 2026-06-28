# elisprs

**Emacs Lisp in Rust — run `.el` outside Emacs.**

`elisprs` is an Emacs Lisp runtime that ships a standalone `elisp` binary. It is
built to become a [`fusevm`](https://github.com/MenkeTechnologies/fusevm)
frontend — the same bytecode VM behind `strykelang`, `zshrs`, `awkrs`, and
`vimlrs` — so that elisp eventually executes on the shared engine (with Cranelift
JIT and AOT) rather than a bespoke interpreter.

> **Status: milestone 1 (early).** Today elisp runs on a self-contained
> tree-walk evaluator. The `fusevm` lowering (`src/compiler.rs`) is the
> milestone-2 seam and is not wired yet. **Free / OSS (MIT).**

---

## [0x00] Why it's built this way

Emacs Lisp is a **Lisp-2** (every symbol has a separate *value* cell and
*function* cell) and is, by default, **dynamically scoped**. Those two facts are
the whole personality of the language, and no general-purpose embeddable Lisp
gives them to you for free.

So elisprs splits the problem:

- **Data model — reused.** We depend on
  [`rust_lisp`](https://crates.io/crates/rust_lisp) (MIT) for its `Value` /
  `List` / `Symbol` types — the well-trodden value representation.
- **Reader — ours.** rust_lisp's *parser* mis-tokenizes core elisp syntax
  (`1+` / `1-`, `#'foo`, `?c` char literals, dotted pairs), which is far too
  common in `.el` to live with. So the reader is a small, elisp-correct one
  (`src/reader.rs`) that emits rust_lisp `Value`s.
- **Semantics — ours.** The evaluator, the obarray (Lisp-2 value/function
  cells), dynamic binding, the special forms, and the subr standard library all
  live in this crate (`src/interp.rs`, `src/builtins.rs`). rust_lisp's own
  `eval` is Lisp-1 + lexical and is deliberately **not** used.

Function objects (closures, macros, subrs) are stored in
`Value::Foreign(Rc<dyn Any>)` and downcast on the way out, so we get elisp
function semantics without forking rust_lisp's `Value` enum.

---

## [0x01] Build & run

```sh
cargo build --release
```

```sh
elisp FILE.el            # evaluate a file
elisp -e "(+ 1 2)"       # evaluate an expression, print its value
elisp                    # REPL (balanced-paren continuation, Ctrl-D to exit)
elisp --lsp              # language server over stdio        (stub — see roadmap)
elisp --dap              # debug adapter over stdio          (stub — see roadmap)
elisp --aot FILE -o a.o  # AOT-compile to a native object    (milestone 2)
elisp --version
```

### A taste

```elisp
(defun fact (n) (if (<= n 1) 1 (* n (fact (1- n)))))
(fact 6)                                  ; => 720

(mapcar (lambda (x) (* x x)) '(1 2 3 4))  ; => (1 4 9 16)

(let ((x 10) (y 20)) (+ x y))             ; => 30

(format "%s = %d (hex %x)" 'count 255 255); => "count = 255 (hex ff)"

(condition-case e (/ 1 0)
  (arith-error (format "caught %s" e)))   ; => "caught (arith-error division by zero)"
```

---

## [0x02] Language coverage (milestone 1)

**Reader syntax.** integers, floats, strings (with escapes), symbols (including
`1+` / `1-` / `<=` / `:keywords`), `nil` / `t`, `'quote`, `#'function`, `?c`
char literals, and `;` comments.

**Special forms.** `quote` `function` `lambda` `progn` `prog1` `if` `when`
`unless` `cond` `and` `or` `while` `setq` `let` `let*` `defun` `defmacro`
`defvar` `defconst` `condition-case` `unwind-protect`.

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

`defun`/`defmacro`/`lambda` support `&optional` and `&rest`. Macros expand and
re-evaluate. `condition-case` matches the `error` umbrella and specific error
symbols; the error object is bound as `(error-symbol "message")`.

---

## [0x03] Known limitations (milestone 1)

These are tracked deliberately, not papered over:

- **No dotted pairs.** rust_lisp's cons cell always has a *list* cdr, so
  `(cons 1 2)` and `(a . b)` cannot be represented; both `cons` and the reader
  error loudly rather than silently misread. Alists must use `(key value)` form,
  not `(key . value)`. Replacing the cons model is the top milestone-2 item.
- **No backquote / unquote.** `` ` `` and `,` are rejected by the reader for
  now; write macros with `list`/`cons` instead.
- **Dynamic scope only.** `lexical-binding` is not honored yet.
- **`setcar` / `setcdr`** are absent (rust_lisp's `List` doesn't expose cons
  mutation) — arrives with the new cons model.

This is a useful elisp core, **not** the ~1000-subr GNU Emacs surface, and it is
not buffer-aware — editor integration (buffers, point, markers) is a separate
track (see the zemacs roadmap).

---

## [0x04] Architecture

```
                 ┌──────────────── rust_lisp (MIT) ────────────────┐
                 │  Value / List / Symbol   (data model only)      │
                 └──────────────────────────▲──────────────────────┘
                                            │ emits rust_lisp Values
                 ┌──────────────────────────┴──────────────────────┐
   .el source ─▶ │  elisprs                                         │
                 │   reader.rs   elisp-correct S-expression reader  │
                 │   interp.rs   obarray (value+function cells),    │
                 │               dynamic binding (specstack),       │
                 │               eval, special forms                │
                 │   callable.rs Callable in Value::Foreign         │
                 │   builtins.rs the subr library                   │
                 └──────────────────────────┬──────────────────────┘
                                            │
                 ┌──────────────────────────▼──────────────────────┐
   milestone 2 ─▶│  compiler.rs  lower(forms) → fusevm::Chunk       │
                 │  aot.rs       fusevm::aot::compile_object → .o    │
                 └──────────────────────────────────────────────────┘
```

| File | Role |
|---|---|
| `src/reader.rs` | Elisp-correct S-expression reader → rust_lisp `Value`s |
| `src/interp.rs` | Lisp-2 obarray, dynamic binding, `eval`, special forms, printer |
| `src/builtins.rs` | The subr standard library |
| `src/callable.rs` | `Callable` (subr/closure/macro), stored in `Value::Foreign` |
| `src/compiler.rs` | **Seam:** lower elisp forms to `fusevm::Chunk` (milestone 2) |
| `src/aot.rs` | `--aot` driver over `compiler::lower` + `fusevm::aot` |
| `src/lsp.rs` / `src/dap.rs` | `--lsp` / `--dap` servers (stubs) |
| `src/main.rs` | The `elisp` CLI + REPL |

---

## [0x05] Roadmap

**Milestone 2 — execute on `fusevm`** (the reason elisprs exists). This is the
same frontend pattern as `strykelang/strykelang/fusevm_native.rs`:

1. Add `Value::{Cons, Symbol}` to `fusevm/src/value.rs` and a dynamic-binding
   stack to the `VM` struct — the one genuinely invasive core change (the other
   fusevm frontends never needed dynamic scope or Lisp cells).
2. Reserve an elisp `Op::Extended(id, arg)` range; register a handler via
   `vm.set_extension_handler(...)` for quote / funcall / special-var bind /
   cons navigation.
3. Lower each top-level form in `compiler.rs`; lambda bodies become sub-chunks.
4. Bind the subr library through `vm.register_builtin(...)`.

Then the JIT and `--aot` (via `fusevm::aot::compile_object`) come for free, the
way they do for the sibling frontends.

**Tooling.** `--lsp` (completion/hover/definition/diagnostics over the obarray,
mirroring `awkrs --lsp`), `--dap` (breakpoints/stepping/inspection off
`eval` + the dynamic specstack, reusing `zemacs-dap` transport), and editor
plugins (`vscode-elisp` / `vim-elisp` / `emacs-elisp`).

---

## License

MIT OR Apache-2.0. Bundles `rust_lisp` (MIT).
