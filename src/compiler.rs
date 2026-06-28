//! Milestone-2 seam: lower elisp forms to a `fusevm::Chunk`.
//!
//! Milestone 1 executes via the tree-walk evaluator in [`crate::interp`]. This
//! module is where elisp stops being interpreted and starts being *compiled* to
//! the shared fusevm bytecode VM — the same path strykelang/awkrs/zshrs take.
//!
//! Planned shape (mirrors `strykelang/strykelang/fusevm_native.rs`):
//! 1. Add `Value::{Cons, Symbol}` to `fusevm/src/value.rs` and a dynamic-binding
//!    stack to the `VM` struct (the one genuinely invasive core change).
//! 2. Reserve an elisp Extended-op ID range (`Op::Extended(id, arg)`); register
//!    a handler via `vm.set_extension_handler(...)` for `quote`/`funcall`/
//!    special-var bind/unbind/cons-nav.
//! 3. Walk each top-level form, emitting ops into a `ChunkBuilder`; lambda
//!    bodies become sub-chunks (`ChunkBuilder::add_sub_chunk`).
//! 4. Bind the subr library through `vm.register_builtin(id, ...)`.
//!
//! Until then this returns an explicit error so callers (`--aot`) fail loudly
//! rather than silently no-op.

use crate::error::{ElError, ElResult};
use rust_lisp::model::Value;

/// Lower a sequence of top-level forms to fusevm bytecode.
///
/// Returns the not-yet-implemented signal in milestone 1.
pub fn lower(_forms: &[Value]) -> ElResult<()> {
    Err(ElError::new(
        "not-implemented",
        "fusevm lowering lands in milestone 2; milestone 1 runs on the tree-walk evaluator",
    ))
}
