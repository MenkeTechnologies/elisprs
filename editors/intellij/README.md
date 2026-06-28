# elisprs JetBrains Plugin

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![IDE](https://img.shields.io/badge/IDE-2025.2%2B-orange.svg)](https://plugins.jetbrains.com/)
[![JDK](https://img.shields.io/badge/JDK-17-blue.svg)](https://adoptium.net/)
[![Plugin SDK](https://img.shields.io/badge/IntelliJ%20Platform%20Gradle-2.16-purple.svg)](https://plugins.jetbrains.com/docs/intellij/tools-intellij-platform-gradle-plugin.html)

### `[FULL IDE FRONT-END FOR THE STANDALONE Emacs Lisp INTERPRETER]`

> *"Emacs Lisp, without Vim — now with breakpoints."*

## `[BUILT FOR ELISPRS]`

A JetBrains-platform plugin that drives the LSP and DAP servers compiled into the `elisprs` binary — a standalone Emacs Lisp (Emacs Lisp) interpreter on the fusevm bytecode VM. Hand-rolled lexer for instant highlighting, semantic-token overlay from the LSP, hover cards, a full breakpoint debugger over DAP, run configs that auto-create from any `.vim` / vimrc-family file, and Extract / Rename refactors routed through the LSP. Talks to the in-tree `src/lsp.rs` + `src/dap.rs` over JSON-RPC; no upstream `lsp-server` / `dap-types` crates anywhere in the build.

### [`elisprs`](https://github.com/MenkeTechnologies/elisprs) · [`fusevm`](https://github.com/MenkeTechnologies/fusevm) · [`strykelang`](https://github.com/MenkeTechnologies/strykelang)

---

## Table of Contents

- [\[0x00\] Overview](#0x00-overview)
- [\[0x01\] Install](#0x01-install)
- [\[0x02\] Editor](#0x02-editor)
- [\[0x03\] LSP](#0x03-lsp)
- [\[0x04\] Code Actions](#0x04-code-actions)
- [\[0x05\] Run / Debug](#0x05-run--debug)
- [\[0x06\] DAP Protocol](#0x06-dap-protocol)
- [\[0x07\] Refactor / Rename](#0x07-refactor--rename)
- [\[0x08\] Configuration](#0x08-configuration)
- [\[0x09\] Logs](#0x09-logs)
- [\[0x0A\] Building](#0x0a-building)
- [\[0x0B\] Plugin Architecture](#0x0b-plugin-architecture)
- [\[0x0C\] Version Compatibility](#0x0c-version-compatibility)
- [\[0x0D\] Limitations](#0x0d-limitations)
- [\[0xFF\] License](#0xff-license)

---

## [0x00] OVERVIEW

elisprs ships an **LSP server** and **DAP debug adapter** built into the `elisprs` binary (`elisprs --lsp`, `elisprs --dap`, both over stdio). This plugin is the JetBrains-side driver:

- Spawns the LSP / DAP servers on demand, frames JSON-RPC over stdio, and renders responses through the IDE's native UI affordances (gutter breakpoints, intentions popup, refactor menu, semantic-tokens layer).
- Adds **zero new language code paths**. Everything the user sees in the editor comes from one of two sources: the hand-rolled `ElisprsLexer.kt` (instant first-paint highlighting) or the `textDocument/semanticTokens` overlay (LSP-driven full classification).
- No upstream `lsp-server` / `lsp-types` / `dap-types` / `lsp4ij` dependencies on the Rust side. JetBrains' own `LspServerSupportProvider` is the only LSP4J consumer; everything else is hand-framed JSON-RPC on top of `serde_json`. Same on the DAP side.

---

## [0x01] INSTALL

```sh
# Install from disk: Settings → Plugins → ⚙ → Install Plugin from Disk…
# Then pick:
editors/intellij/build/distributions/elisprs-intellij-<version>.zip
```

After install: restart the IDE → open any `.vim` file (or `vimrc` / `.vimrc` / `_vimrc` / `gvimrc` / `.gvimrc` / `.exrc` / `init.vim`) → the LSP starts automatically → the debugger activates the first time you click Debug.

The `elisprs` binary must be on `$PATH`, or configured under *Settings → Tools → Elisprs → elisprs executable*. The plugin resolves the executable via `ElisprsSettings.elisprsExecutable` first, then falls back to `which elisprs`.

---

## [0x02] EDITOR

| Surface | Behavior |
|---------|----------|
| File association | `.vim` plus the `vimrc` / `gvimrc` / `exrc` / `init.vim` family (configurable; see [§0x08](#0x08-configuration)) |
| Lexer | Hand-rolled in `ElisprsLexer.kt` — instant first-paint highlighting before the LSP semantic-tokens response lands |
| Color slots | One stable `ELISPRS_*` `TextAttributesKey` per token category under *Settings → Editor → Color Scheme → elisprs* |
| Brace matching | `{` / `}`, `(` / `)`, `[` / `]` via `ElisprsBraceMatcher.kt` |
| Comments | Cmd/Ctrl-`/` for `"` line comments via `ElisprsCommenter.kt` (Emacs Lisp has no block-comment form) |
| Quote handler | `"` and `'` auto-pair; inside-string typing recognized via `ElisprsQuoteHandler.kt` |
| Complete Current Statement | Cmd-Shift-Enter closes `if`/`while`/`for`/`function`/`try` blocks and balances brackets via `ElisprsSmartEnterProcessor.kt` |

### Lexer coverage

| Token category | Examples |
|----------------|----------|
| Comments | `"` line (command position only), `#!` shebang on line 1 |
| Strings | `"…"` (backslash escapes), `'…'` (literal, `''` escapes) |
| Numbers | `42`, `3.14`, `0x1F`, `0b1010`, `1.0e3` |
| Keywords | `if` / `elseif` / `else` / `endif` / `while` / `for` / `function` / `endfunction` / `try` / `catch` / `let` / `call` / `echo` / `return` … |
| Ex commands | `set` / `setlocal` / `autocmd` / `augroup` / `nnoremap` / `highlight` / `syntax` / `source` / `silent` … |
| Scope vars | `g:` `s:` `b:` `w:` `t:` `l:` `a:` `v:` followed by a name |
| Specials | `v:true` / `v:false` / `v:count` / `v:val` / `v:shell_error` / `v:exception` … |
| Options / env / register | `&number` / `&l:textwidth`, `$HOME`, `@a` |
| Builtin functions | `len(` / `has(` / `printf(` / `substitute(` (only before `(`) |
| Autoload | `plug#begin(` (colored as a declaration) |
| Operators | `==` `!=` `=~` `!~` (with `#` / `?` case flags), `..` `->` `+=` `-=` `.=`, `|` bar, `\` line continuation |

---

## [0x03] LSP

The LSP server is in-process inside the `elisprs` binary — `elisprs --lsp` spawns it over stdio. Plugin side starts it via `ElisprsLspServerSupportProvider.kt`; descriptor in `ElisprsLspServerDescriptor.kt`.

### Capabilities

| Capability | Trigger / scope |
|------------|-----------------|
| `completion` | builtins, keywords, options, scope vars, in-file functions |
| `hover` | markdown cards for builtins / commands / options / special variables |
| `definition` / `references` | function names declared in the open document |
| `documentSymbol` | `function Foo`, `let` decls, `command` / `augroup` blocks |
| `foldingRange` | `if … endif`, `function … endfunction`, `while … endwhile` blocks |
| `rename` | scope vars, function names, command names |
| `semanticTokens/full` | token classes mirroring the lexer; the standard LSP token types map to the `ELISPRS_*` color keys |
| `formatting` | trailing-whitespace strip, indent normalize, final-newline guarantee |
| `publishDiagnostics` | Vim-style `E121: Undefined variable` etc. on `didOpen` / `didChange` / `didSave` |

### Transport

- **Stdio**, Content-Length-framed JSON-RPC. Hand-rolled framer on top of `serde_json` — no `lsp-server` / `lsp-types` crates.
- Optional `ELISPRS_LSP_LOG=<path>` env var dumps every request/response to a file for debugging.

---

## [0x04] CODE ACTIONS

LSP `refactor.extract` code actions surface under **Alt-Enter** (intentions popup). The IntelliJ Refactor menu (Ctrl-T) routes via `ElisprsRefactoringSupportProvider.kt` so Extract Method / Variable / Constant on the platform's binding all reach the LSP. Failure modes (no LSP, no matching action) surface as balloon notifications instead of silent dead keys.

---

## [0x05] RUN / DEBUG

### Run

| Surface | Behavior |
|---------|----------|
| **Run config** (`ElisprsRunConfigurationType`) | runs `elisprs FILE.vim` (positional file argument); toggle for `--disasm` (fusevm bytecode listing); working directory + script args + interpreter args |
| **Context menu** | *Run with elisprs* on any `.vim` file in the editor or project view; auto-creates a config |
| **Producer** | `ElisprsRunConfigurationProducer` materializes a run config from the active file |
| **Output** | Standard `ConsoleView` — `echo` / `echomsg` stream in real time |
| **File → New → Emacs Lisp File** | Pick *Script* (shebanged), *Autoload*, *Ftplugin*, or *Empty* |

### Debug

DAP-backed, over the `elisprs --dap` server's stdio. The plugin spawns `elisprs --dap`; the protocol frames flow over the process's stdout/stdin while the debuggee's own output arrives as DAP `output` events.

| Feature | Notes |
|---------|-------|
| Line breakpoints | Gutter toggle / enable / disable; persistent across sessions |
| Continue / Step Over / Step Into / Step Out / Pause / Run to Cursor | Standard XDebugger actions |
| Frames | `file:line` per frame, click to navigate source |
| Variables panel | Scalars, lists, dictionaries; expandable on click |
| Evaluate dialog | Arbitrary Emacs Lisp expressions resolved against the paused frame |
| Console | `echo` / `echomsg` streams in real time via DAP `output` events |

---

## [0x06] DAP PROTOCOL

Plugin side (`com.menketechnologies.elisprs.dap`):

1. `ElisprsDebugRunner.doExecute` spawns `elisprs --dap` and keeps its stdio for the DAP protocol.
2. `ElisprsDapClient` reads Content-Length-framed JSON-RPC from the process stdout — **byte-based, not char-based** — so multi-byte UTF-8 in variable reprs doesn't desync framing.
3. On `stopped` event, `onStopped` synchronously fetches `stackTrace` + `scopes` + `variables`, builds `ElisprsStackFrame` objects with pre-populated children, then calls `session.positionReached`.
4. `ElisprsEvaluator` sends `evaluate` requests for the Evaluate dialog.

elisprs side (`src/dap.rs`): DAP requests handled include `initialize`, `launch`, `setBreakpoints`, `configurationDone`, `threads`, `stackTrace`, `scopes`, `variables`, `continue`, `next`, `stepIn`, `stepOut`, `pause`, `evaluate`, `disconnect`. Same JSON-RPC framing as the LSP server.

---

## [0x07] REFACTOR / RENAME

**Shift-F6** on a scope variable, function name, or command renames it across the workspace via `textDocument/rename`. Implementation: plugin handler in `ElisprsRenameHandler.kt`; server-side rename in `src/lsp.rs::rename`.

---

## [0x08] CONFIGURATION

*Settings → Tools → Elisprs*:

| Section     | Setting                                | Default              | Notes |
|-------------|----------------------------------------|----------------------|-------|
| Interpreter | elisprs executable                      | first `elisprs` on `$PATH` | absolute path or blank |
| LSP         | Enable LSP                             | on                   | master toggle |
| LSP         | Extra LSP args                         | empty                | passed after `--lsp` |
| LSP         | LSP environment                        | empty                | `KEY=VAL` pairs (e.g. `RUST_LOG=info`) |
| LSP         | Auto-restart LSP on settings change    | on                   | restart picks up new env |
| LSP         | Show builtin hovers                    | on                   | server-provided cards |
| LSP         | Log LSP traffic to file                | off                  | sets `ELISPRS_LSP_LOG=<path>` |
| Editor      | Disable lexer highlighting             | off                  | rely only on LSP semantic tokens |
| Editor      | File extensions                        | `vim`                | comma-separated; the vimrc dotfiles always match |

Color scheme entries: *Settings → Editor → Color Scheme → elisprs*.

---

## [0x09] LOGS

The plugin writes an append-only log under `~/.elisprs/` (or `$ELISPRS_HOME/` when that env var is set):

| File | Source | Contents |
|------|--------|----------|
| `~/.elisprs/elisprs-plugin.log` | Kotlin (plugin) | LSP command line built, DAP `send` / receive, rename / semantic-token routing, breakpoint handler steps |

Tail with `tail -f ~/.elisprs/elisprs-plugin.log`.

---

## [0x0A] BUILDING

```sh
cd editors/intellij
export JAVA_HOME=$(/usr/libexec/java_home -v 17)   # macOS; or set to any JDK 17 install
./gradlew buildPlugin             # → build/distributions/elisprs-intellij-<v>.zip
./gradlew runIde                  # launches a sandbox IDE with the plugin installed
./gradlew verifyPlugin            # plugin verifier against recommended IDE matrix
./gradlew test                    # runs ElisprsLexerTest + ElisprsCommenterTest + ElisprsSettingsTest + ElisprsSmartEnterProcessorTest
```

**JDK 17 is required.** Set `JAVA_HOME` to a JDK 17 install before running gradle. The plugin itself targets JVM 17, so any IDE on 2025.2+ runs it. First build downloads the IntelliJ Platform SDK (~1 GB), takes a few minutes, and is cached under `editors/intellij/.intellijPlatform/` (which is gitignored).

---

## [0x0B] PLUGIN ARCHITECTURE

```
editors/intellij/
├── build.gradle.kts                          # IntelliJ Platform Gradle Plugin 2.16
├── gradle.properties                         # platform version, plugin version, JVM
├── settings.gradle.kts
└── src/main/
    ├── kotlin/com/menketechnologies/elisprs/
    │   ├── ElisprsLanguage.kt                 # Language singleton
    │   ├── ElisprsFileType.kt                 # .vim + vimrc family → Emacs Lisp
    │   ├── ElisprsIcons.kt                    # icon loader
    │   ├── ElisprsColors.kt                   # ELISPRS_* TextAttributesKey constants
    │   ├── ElisprsTokenTypes.kt               # token type set
    │   ├── ElisprsLexer.kt                    # hand-rolled Emacs Lisp lexer
    │   ├── ElisprsSyntaxHighlighter.kt        # token → color mapping
    │   ├── ElisprsColorSettingsPage.kt        # IDE color-scheme entries
    │   ├── ElisprsBraceMatcher.kt             # {} () []
    │   ├── ElisprsCommenter.kt                # `"` line comments
    │   ├── ElisprsQuoteHandler.kt             # " ' auto-pair
    │   ├── ElisprsSmartEnterProcessor.kt      # block / bracket completion
    │   ├── ElisprsSpellcheckingStrategy.kt    # suppress typos on strings/comments
    │   ├── ElisprsSettings.kt                 # persistent settings
    │   ├── ElisprsSettingsConfigurable.kt
    │   ├── ElisprsDebugLog.kt                 # plugin-side log writer
    │   ├── lsp/
    │   │   ├── ElisprsLspServerSupportProvider.kt
    │   │   └── ElisprsLspServerDescriptor.kt
    │   ├── refactor/
    │   │   ├── ElisprsRefactoringSupportProvider.kt
    │   │   └── ElisprsRenameHandler.kt
    │   ├── navigate/
    │   │   └── ElisprsGotoDeclarationHandler.kt
    │   ├── run/
    │   │   ├── ElisprsRunConfigurationType.kt
    │   │   ├── ElisprsRunConfigurationOptions.kt
    │   │   ├── ElisprsRunConfiguration.kt
    │   │   ├── ElisprsRunConfigurationEditor.kt
    │   │   ├── ElisprsRunConfigurationProducer.kt
    │   │   ├── ElisprsProgramRunner.kt        # Run executor
    │   │   └── ElisprsDebugRunner.kt          # Debug executor (DAP over stdio)
    │   ├── dap/
    │   │   ├── ElisprsDapClient.kt            # byte-based DAP protocol client
    │   │   ├── ElisprsDebugProcess.kt         # XDebugProcess
    │   │   ├── ElisprsDebuggerEditorsProvider.kt
    │   │   ├── ElisprsBreakpointType.kt
    │   │   ├── ElisprsBreakpointHandler.kt
    │   │   ├── ElisprsStackFrame.kt
    │   │   ├── ElisprsSuspendContext.kt
    │   │   ├── ElisprsValue.kt
    │   │   └── ElisprsEvaluator.kt
    │   └── actions/
    │       ├── RunElisprsFileAction.kt
    │       └── CreateElisprsFileAction.kt
    └── resources/
        ├── META-INF/plugin.xml
        └── icons/elisprs.svg
```

The Rust side lives in `elisprs/src/lsp.rs` (LSP server, `elisprs --lsp`) and `elisprs/src/dap.rs` (DAP server, `elisprs --dap`).

---

## [0x0C] VERSION COMPATIBILITY

Plugin version tracks the elisprs Cargo workspace version. `gradle.properties` controls the supported IDE range via `pluginSinceBuild` / `pluginUntilBuild`. Currently targets the `2025.2` SDK against builds `252..261.*` — every paid JetBrains IDE on **2025.2 +** loads it (RustRover, IDEA Ultimate, GoLand, PyCharm Pro, WebStorm, RubyMine, PhpStorm, CLion, Rider, DataGrip, Aqua). Community editions don't have the LSP API, so the plugin won't load there.

---

## [0x0D] LIMITATIONS

- **No PSI tree** — every symbol-navigation feature (Cmd-click, Cmd-B, Find Usages, rename) routes through the LSP server. Disabling the LSP under Settings disables them all.
- **Debugger v1**: no conditional breakpoints, no hit-count breakpoints, no exception breakpoints, no watch expressions, no Set Value, single-thread only.
- **Lexer is approximate** for the `"` comment-vs-string ambiguity in pathological cases (a `"` at command position is a comment; otherwise a string — Vim's own runtime syntax uses the same heuristic). Server-side semantic tokens fill in where the lexer is wrong.

---

## [0xFF] LICENSE

MIT, same as elisprs.
