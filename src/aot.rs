//! `elisp --aot`: lower a `.el` file to a fusevm `Chunk` (and, with fusevm's
//! `aot` feature, on to a native object via `fusevm::aot::compile_object`).
//!
//! The lowering is real now (`compiler::compile_program`); native object
//! emission is gated behind fusevm's `aot` cargo feature, so this reports the
//! lowered chunk and notes when object emission isn't compiled in.

use crate::{compiler, host, reader};
use std::path::Path;

pub fn compile_file(src: &str, out: &Path) -> Result<(), String> {
    // Load the prelude into the host first, so user code referencing it resolves
    // and the embedded heap image includes the prelude objects.
    let _ = crate::eval_str("");
    let mut chunk = host::with_host(|h| {
        let forms = reader::read_all(h, src)?;
        compiler::compile_program(h, &forms)
    })?;
    // Embed the user/prelude heap image so `Value::Obj` handles in the chunk's
    // constants resolve in the fresh AOT-runtime host (see aot_runtime.rs).
    let image = host::with_host(|h| h.export_heap_image());
    let json = serde_json::to_string(&image).map_err(|e| e.to_string())?;
    eprintln!(
        "lowered to fusevm chunk: {} ops, {} constants; heap image: {} objects",
        chunk.ops.len(),
        chunk.constants.len(),
        image.len()
    );
    chunk.names.push(format!("{}{}", host::HEAP_IMAGE_TAG, json));
    // fusevm bincode-serializes the chunk into the object plus a native entry
    // that deserializes and runs it on the VM. A standalone binary links this
    // object against the elisprs runtime (aot_runtime::fusevm_aot_register_builtins).
    fusevm::aot::compile_object(&chunk, out)?;
    Ok(())
}
