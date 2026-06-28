//! `elisp --aot`: lower a `.el` file to a fusevm `Chunk` (and, with fusevm's
//! `aot` feature, on to a native object via `fusevm::aot::compile_object`).
//!
//! The lowering is real now (`compiler::compile_program`); native object
//! emission is gated behind fusevm's `aot` cargo feature, so this reports the
//! lowered chunk and notes when object emission isn't compiled in.

use crate::{compiler, host, reader};
use std::path::Path;

pub fn compile_file(src: &str, out: &Path) -> Result<(), String> {
    let chunk = host::with_host(|h| {
        let forms = reader::read_all(h, src)?;
        compiler::compile_program(h, &forms)
    })?;
    eprintln!(
        "lowered to fusevm chunk: {} ops, {} constants",
        chunk.ops.len(),
        chunk.constants.len()
    );
    // Native object emission:
    //   fusevm::aot::compile_object(&chunk, out).map_err(|e| e)?;
    // is available when fusevm is built with its `aot` feature. Until that is
    // wired into elisprs's Cargo features, report the lowering result.
    let _ = out;
    Err("native object emission requires fusevm's `aot` feature (lowering succeeded)".to_string())
}
