# elisprs JetBrains Plugin

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![IDE](https://img.shields.io/badge/IDE-2025.2%2B-orange.svg)](https://plugins.jetbrains.com/)
[![JDK](https://img.shields.io/badge/JDK-17-blue.svg)](https://adoptium.net/)
[![Plugin SDK](https://img.shields.io/badge/IntelliJ%20Platform%20Gradle-2.16-purple.svg)](https://plugins.jetbrains.com/docs/intellij/tools-intellij-platform-gradle-plugin.html)

### `[FULL IDE FRONT-END FOR THE STANDALONE Emacs Lisp INTERPRETER]`

> *"Emacs Lisp, with an IDE."*

## `[BUILT FOR ELISPRS]`

A JetBrains-platform plugin that drives the LSP and DAP servers compiled into the `elisprs` binary — a standalone Emacs Lisp interpreter on the fusevm bytecode VM. Hand-rolled lexer for instant highlighting, LSP completion / hover / signature help / document symbols / diagnostics, a breakpoint debugger over DAP, and run configs that auto-create from any `.el` file. Talks to the in-tree `src/lsp.rs` + `src/dap.rs` over JSON-RPC.

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

elisprs ships an **LSP server** and **DAP debug adapter** built into the `elisp` binary (`elisp --lsp`, `elisp --dap`, both over stdio). This plugin is the JetBrains-side driver:

- Spawns the LSP / DAP servers on demand, frames JSON-RPC over stdio, and renders responses through the IDE's native UI affordances (gutter breakpoints, intentions popup, refactor menu, semantic-tokens layer).
- Adds **zero new language code paths**. Highlighting comes from the hand-rolled `ElisprsLexer.kt` (instant first-paint highlighting); the plugin maps standard LSP semantic-token types to color keys should the server send them.
- The Rust LSP server uses the `lsp-server` / `lsp-types` crates; the DAP server is hand-framed JSON-RPC on top of `serde_json`. JetBrains' own `LspServerSupportProvider` is the only LSP4J consumer on the plugin side.

---

## [0x01] INSTALL

```sh
# Install from disk: Settings → Plugins → ⚙ → Install Plugin from Disk…
# Then pick:
editors/intellij/build/distributions/elisprs-intellij-<version>.zip
```

After install: restart the IDE → open any `.el` file (or `.emacs` / `_emacs` / `.gnus` / `.spacemacs` / `.viper`) → the LSP starts automatically → the debugger activates the first time you click Debug.

The `elisp` binary must be on `$PATH`, or configured under *Settings → Tools → Elisprs → elisprs executable*. The plugin resolves the executable via `ElisprsSettings.elisprsExecutable` first, then falls back to `elisp` on `$PATH`.

---

## [0x02] EDITOR

| Surface | Behavior |
|---------|----------|
| File association | `.el` (configurable) plus the `.emacs` / `_emacs` / `.gnus` / `.spacemacs` / `.viper` dotfiles; see [§0x08](#0x08-configuration) |
| Lexer | Hand-rolled in `ElisprsLexer.kt` — instant first-paint highlighting before the LSP semantic-tokens response lands |
| Color slots | One stable `ELISPRS_*` `TextAttributesKey` per token category under *Settings → Editor → Color Scheme → elisprs* |
| Brace matching | `(` / `)`, `[` / `]` via `ElisprsBraceMatcher.kt` |
| Comments | Cmd/Ctrl-`/` for `;; ` line comments via `ElisprsCommenter.kt` (Emacs Lisp has no block-comment form) |
| Quote handler | `"` auto-pairs; inside-string typing recognized via `ElisprsQuoteHandler.kt` |
| Complete Current Statement | Cmd-Shift-Enter closes unbalanced parens / brackets on the line via `ElisprsSmartEnterProcessor.kt` |

### Lexer coverage

| Token category | Examples |
|----------------|----------|
| Comments | `;` line comments |
| Strings | `"…"` (backslash escapes) |
| Characters | `?c` / `?\n` literals |
| Numbers | `42`, `3.14`, `2e-3`, `#x1F`, `#o17`, `#b1010` |
| Special forms / macros | `defun` / `let` / `if` / `lambda` / `condition-case` / `cl-loop` … (the name after a `def…` form is lexed as a declaration) |
| Constants | `t` / `nil` / `:keyword` |
| Builtin functions | `car` / `cons` / `mapcar` / `+` … (only in call-head position) |
| Reader prefixes | `'` `` ` `` `,` `,@` `#'` |

---

## [0x03] LSP

The LSP server is in-process inside the `elisp` binary — `elisp --lsp` spawns it over stdio. Plugin side starts it via `ElisprsLspServerSupportProvider.kt`; descriptor in `ElisprsLspServerDescriptor.kt`.

### Capabilities

| Capability | Trigger / scope |
|------------|-----------------|
| `completion` | builtins and special forms; triggered on `(` |
| `hover` | markdown cards for builtins |
| `documentSymbol` | top-level definitions in the open document |
| `signatureHelp` | triggered on `(` and space |
| `publishDiagnostics` | on `didOpen` / `didChange` / `didSave` |

### Transport

- **Stdio**, Content-Length-framed JSON-RPC via the `lsp-server` / `lsp-types` crates; text sync is full-document.
- The plugin sets `ELISPRS_LSP_LOG=<path>` in the server's environment when *Log LSP traffic to file* is on; the server in `src/lsp.rs` does not read it.

---

## [0x04] CODE ACTIONS

The plugin routes the IntelliJ Refactor menu (Ctrl-T) through `ElisprsRefactoringSupportProvider.kt` to LSP `textDocument/codeAction`. The bundled server does not advertise code actions, so no extract actions are produced; failure modes (no LSP, no matching action) surface as balloon notifications.

---

## [0x05] RUN / DEBUG

### Run

| Surface | Behavior |
|---------|----------|
| **Run config** (`ElisprsRunConfigurationType`) | runs `elisp FILE.el` (positional file argument); toggle for `--disasm` (fusevm bytecode listing); working directory + script args + interpreter args |
| **Context menu** | *Run with elisprs* on any `.el` file in the editor or project view; auto-creates a config |
| **Producer** | `ElisprsRunConfigurationProducer` materializes a run config from the active file |
| **Output** | Standard `ConsoleView` — `princ` / `message` output streams in real time |
| **File → New → Emacs Lisp File** | Pick *Script* (`#!/usr/bin/env elisp`), *Library*, or *Empty* |

### Debug

DAP-backed, over the `elisp --dap` server's stdio. The plugin spawns `elisp --dap`; the protocol frames flow over the process's stdout/stdin while the debuggee's own output arrives as DAP `output` events.

| Feature | Notes |
|---------|-------|
| Line breakpoints | Gutter toggle / enable / disable; persistent across sessions |
| Continue / Step Over / Step Into / Step Out / Pause / Run to Cursor | Standard XDebugger actions |
| Frames | `file:line` per frame, click to navigate source |
| Variables panel | Scalars and lists; expandable on click |
| Evaluate dialog | Arbitrary Emacs Lisp expressions resolved against the paused frame |
| Console | `princ` / `prin1` output streams in real time via DAP `output` events |

---

## [0x06] DAP PROTOCOL

Plugin side (`com.menketechnologies.elisprs.dap`):

1. `ElisprsDebugRunner.doExecute` spawns `elisp --dap` and keeps its stdio for the DAP protocol.
2. `ElisprsDapClient` reads Content-Length-framed JSON-RPC from the process stdout — **byte-based, not char-based** — so multi-byte UTF-8 in variable reprs doesn't desync framing.
3. On `stopped` event, `onStopped` synchronously fetches `stackTrace` + `scopes` + `variables`, builds `ElisprsStackFrame` objects with pre-populated children, then calls `session.positionReached`.
4. `ElisprsEvaluator` sends `evaluate` requests for the Evaluate dialog.

elisprs side (`src/dap.rs`): DAP requests handled include `initialize`, `launch`, `setBreakpoints`, `setFunctionBreakpoints`, `configurationDone`, `threads`, `stackTrace`, `scopes`, `variables`, `continue`, `next`, `stepIn`, `stepOut`, `pause`, `evaluate`, `disconnect`. Same JSON-RPC framing as the LSP server.

`initialize` advertises `supportsFunctionBreakpoints`, so a client may name functions to break on instead of (or as well as) source lines; entering one arms stepping, so the stop lands on the function's first statement and reports `reason: "function breakpoint"`. The match follows the *function cell*, which is what Emacs's own instrumentation does — `(add2 1)`, `(funcall 'add2 1)`, `(apply 'add2 '(1))` and `(funcall (symbol-function 'add2) 1)` all break, a function object captured before a redefinition does not. The plugin does not send this request yet; the adapter answers it for any DAP client that does.

---

## [0x07] REFACTOR / RENAME

**Shift-F6** is routed by `ElisprsRenameHandler.kt` to LSP `textDocument/rename`. The bundled server in `src/lsp.rs` does not implement `rename`, so no edits are produced.

---

## [0x08] CONFIGURATION

*Settings → Tools → Elisprs*:

| Section     | Setting                                | Default              | Notes |
|-------------|----------------------------------------|----------------------|-------|
| Interpreter | elisprs executable                      | first `elisp` on `$PATH` | absolute path or blank |
| LSP         | Enable LSP                             | on                   | master toggle |
| LSP         | Extra LSP args                         | empty                | passed after `--lsp` |
| LSP         | LSP environment                        | empty                | `KEY=VAL` pairs (e.g. `RUST_LOG=info`) |
| LSP         | Auto-restart LSP on settings change    | on                   | restart picks up new env |
| LSP         | Show builtin hovers                    | on                   | server-provided cards |
| LSP         | Log LSP traffic to file                | off                  | sets `ELISPRS_LSP_LOG=<path>` |
| Editor      | Disable lexer highlighting             | off                  | rely only on LSP semantic tokens |
| Editor      | File extensions                        | `el`                 | comma-separated; the Emacs dotfiles always match |

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
    │   ├── ElisprsFileType.kt                 # .el + Emacs dotfiles → Emacs Lisp
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
        └── icons/vimlrs.svg
```

The Rust side lives in `src/lsp.rs` (LSP server, `elisp --lsp`) and `src/dap.rs` (DAP server, `elisp --dap`).

---

## [0x0C] VERSION COMPATIBILITY

Plugin version tracks the elisprs Cargo workspace version. `gradle.properties` controls the supported IDE range via `pluginSinceBuild` / `pluginUntilBuild`. Currently targets the `2025.2` SDK against builds `252..261.*` — every paid JetBrains IDE on **2025.2 +** loads it (RustRover, IDEA Ultimate, GoLand, PyCharm Pro, WebStorm, RubyMine, PhpStorm, CLion, Rider, DataGrip, Aqua). Community editions don't have the LSP API, so the plugin won't load there.

---

## [0x0D] LIMITATIONS

- **No PSI tree** — every symbol-navigation feature (Cmd-click, Cmd-B, Find Usages, rename) routes through the LSP server. Disabling the LSP under Settings disables them all.
- **Debugger v1**: no conditional breakpoints, no hit-count breakpoints, no exception breakpoints, no watch expressions, no Set Value, single-thread only.

---

## [0xFF] LICENSE

MIT, same as elisprs.
