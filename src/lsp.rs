//! `elisp --lsp`: language server (milestone-1 stub).
//!
//! The other frontends ship a real stdio LSP (`awkrs/src/lsp.rs:run_stdio`,
//! `vimlrs/src/lsp.rs`). This will mirror that shape: an `lsp-server` +
//! `lsp-types` loop over the `Interp`'s obarray, providing:
//! - completion from interned symbols (value + function cells)
//! - hover/signature from subr arity + docstrings
//! - go-to-definition for `defun`/`defvar`
//! - diagnostics from reader + `eval` errors
//!
//! Dependencies (lsp-server/lsp-types) are deferred until the protocol loop is
//! implemented, to keep milestone 1 dependency-light.

pub fn run_stdio() -> i32 {
    eprintln!("elisp --lsp: language server is a milestone-1 stub (not yet implemented).");
    eprintln!("Planned: completion/hover/definition/diagnostics over the elisp obarray,");
    eprintln!("mirroring awkrs/vimlrs `run_stdio` via lsp-server + lsp-types.");
    0
}
