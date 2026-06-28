//! `elisp --aot`: ahead-of-time compile a `.el` file to a native object.
//!
//! This is a thin driver over the milestone-2 lowering: read + parse the file,
//! lower to a `fusevm::Chunk` ([`crate::compiler::lower`]), then call
//! `fusevm::aot::compile_object(&chunk, out_path)` to emit a `.o` for static
//! linking — exactly the AOT path fusevm already exposes for the other
//! frontends.
//!
//! Milestone 1 surfaces the missing lowering step explicitly instead of
//! pretending to produce an object.

use crate::error::ElResult;
use crate::reader::read_all;
use std::path::Path;

pub fn compile_file(src: &str, _out: &Path) -> ElResult<()> {
    let forms = read_all(src)?;
    // Lowering is the milestone-2 seam; this returns the not-implemented signal.
    crate::compiler::lower(&forms)?;
    // Once lowering exists:
    //   fusevm::aot::compile_object(&chunk, _out)
    //       .map_err(|e| ElError::err(e))?;
    Ok(())
}
