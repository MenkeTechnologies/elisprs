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
pub mod reader;

pub use fusevm::Value;
pub use host::{reset_host, run_chunk, with_host};

/// Read, lower, and run a source string on fusevm; return the last value.
pub fn eval_str(src: &str) -> Result<Value, String> {
    let forms = host::with_host(|h| reader::read_all(h, src))?;
    let mut last = Value::Undef;
    for form in &forms {
        let chunk = host::with_host(|h| compiler::compile_top(h, form))?;
        last = host::run_chunk(chunk)?;
    }
    Ok(last)
}

/// Render a value (prin1 style when `readable`).
pub fn print(v: &Value, readable: bool) -> String {
    host::with_host(|h| h.print(v, readable))
}
