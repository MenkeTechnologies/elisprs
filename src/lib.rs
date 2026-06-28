//! elisprs — Emacs Lisp as a fusevm frontend.
//!
//! Pipeline: `reader` builds elisp forms as ElispHost heap objects → `compiler`
//! lowers each to a `fusevm::Chunk` → fusevm executes it, calling back into the
//! `host` (via fusevm's extension handler) for elisp-specific operations. There
//! is no bespoke VM or JIT here — execution and codegen live in fusevm.

pub mod aot;
pub mod builtins;
pub mod compiler;
pub mod dap;
pub mod host;
pub mod lsp;
pub mod prelude;
pub mod reader;

pub use fusevm::Value;
pub use host::{reset_host, run_chunk, with_host};

/// Read, lower, and run a source string on fusevm; return the last value.
pub fn eval_str(src: &str) -> Result<Value, String> {
    load_prelude();
    eval_forms(src)
}

/// Evaluate a sequence of top-level forms (macro-expand → lower → run).
fn eval_forms(src: &str) -> Result<Value, String> {
    let forms = host::with_host(|h| reader::read_all(h, src))?;
    let mut last = Value::Undef;
    for form in &forms {
        // Macro-expand before lowering (a prior form's `defmacro` is in effect).
        let expanded = host::macroexpand_all(form)?;
        let chunk = host::with_host(|h| compiler::compile_top(h, &expanded))?;
        last = host::run_chunk(chunk)?;
    }
    Ok(last)
}

/// Load the derived-surface prelude once per host, best-effort (a broken
/// definition is skipped, not fatal).
fn load_prelude() {
    if host::prelude_loaded() {
        return;
    }
    host::set_prelude_loaded(true);
    let Ok(forms) = host::with_host(|h| reader::read_all(h, prelude::PRELUDE)) else {
        return;
    };
    for form in &forms {
        let r = (|| -> Result<(), String> {
            let expanded = host::macroexpand_all(form)?;
            let chunk = host::with_host(|h| compiler::compile_top(h, &expanded))?;
            host::run_chunk(chunk)?;
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("elisprs: prelude form failed: {e}");
        }
    }
}

/// Render a value (prin1 style when `readable`).
pub fn print(v: &Value, readable: bool) -> String {
    host::with_host(|h| h.print(v, readable))
}
