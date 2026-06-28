//! Callable objects (subrs, closures, macros).
//!
//! rust_lisp's `Value` enum is external, so we can't add elisp-specific
//! variants to it. Instead we stash our callables in `Value::Foreign(Rc<dyn
//! Any>)` and downcast on the way out. This keeps the reader/data model from
//! rust_lisp while letting elisp own function objects, dynamic closures, and
//! macros.

use crate::error::ElResult;
use crate::interp::Interp;
use rust_lisp::model::Value;

/// A builtin implemented in Rust. Receives already-evaluated arguments.
pub type SubrFn = fn(&mut Interp, &[Value]) -> ElResult<Value>;

/// An elisp lambda list, parsed once at definition time.
#[derive(Clone, Debug)]
pub struct Params {
    pub required: Vec<String>,
    pub optional: Vec<String>,
    pub rest: Option<String>,
}

/// Everything that can sit in a symbol's function cell or be `funcall`ed.
pub enum Callable {
    /// Native builtin.
    Subr { name: String, min: usize, max: Option<usize>, func: SubrFn },
    /// User function (`defun`/`lambda`). Dynamically scoped in milestone 1.
    Closure { name: Option<String>, params: Params, body: Vec<Value> },
    /// User macro (`defmacro`). Receives *unevaluated* args; result is re-evaluated.
    Macro { name: Option<String>, params: Params, body: Vec<Value> },
}

impl Callable {
    pub fn label(&self) -> String {
        match self {
            Callable::Subr { name, .. } => format!("#<subr {name}>"),
            Callable::Closure { name, .. } => {
                format!("#<closure {}>", name.as_deref().unwrap_or("anonymous"))
            }
            Callable::Macro { name, .. } => {
                format!("#<macro {}>", name.as_deref().unwrap_or("anonymous"))
            }
        }
    }
}
