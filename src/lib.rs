//! elisprs — Emacs Lisp as a fusevm frontend.
//!
//! Built on the rust_lisp (MIT) reader + `Value`/`List` model, with the elisp
//! semantics (Lisp-2 obarray, dynamic binding, special forms, subr library)
//! implemented on top. Milestone 1 is a self-contained tree-walk runtime;
//! [`compiler`] is the seam where milestone 2 lowers to `fusevm::Chunk`.

pub mod aot;
pub mod builtins;
pub mod callable;
pub mod compiler;
pub mod dap;
pub mod error;
pub mod interp;
pub mod lsp;
pub mod reader;

pub use error::{ElError, ElResult};
pub use interp::Interp;
pub use rust_lisp::model::Value;
