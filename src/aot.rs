//! `elisp --aot`: lower a `.el` file to a fusevm `Chunk` (and, with fusevm's
//! `aot` feature, on to a native object via `fusevm::aot::compile_object`).
//!
//! The lowering is real now (`compiler::compile_program`); native object
//! emission is gated behind fusevm's `aot` cargo feature, so this reports the
//! lowered chunk and notes when object emission isn't compiled in.

use crate::{compiler, host, reader};
use std::path::{Path, PathBuf};

pub fn compile_file(src: &str, out: &Path) -> Result<(), String> {
    // Load the prelude into the host first, so user code referencing it resolves
    // and the embedded heap image includes the prelude objects.
    let _ = crate::eval_str("");
    let forms = host::with_host(|h| reader::read_all(h, src))?;
    // Macro-expand each form before lowering (outside the host borrow), exactly
    // as the interpreted driver does — otherwise macros like dolist/push/when
    // compile as bare macro calls and fail.
    let mut expanded = Vec::with_capacity(forms.len());
    for form in &forms {
        expanded.push(host::macroexpand_all(form)?);
    }
    let mut chunk = host::with_host(|h| compiler::compile_program(h, &expanded))?;
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
    chunk
        .names
        .push(format!("{}{}", host::HEAP_IMAGE_TAG, json));
    // fusevm bincode-serializes the chunk into the object plus a native entry
    // that deserializes and runs it on the VM. A standalone binary links this
    // object against the elisprs runtime (aot_runtime::fusevm_aot_register_builtins).
    fusevm::aot::compile_object(&chunk, out)?;
    Ok(())
}

/// Compile a `.el` file all the way to a standalone native executable: emit the
/// AOT object, then link it against the elisprs runtime staticlib (which carries
/// fusevm's AOT runtime + `fusevm_aot_register_builtins`) and a tiny C entry.
pub fn compile_executable(src: &str, out: &Path) -> Result<(), String> {
    let tmp = std::env::temp_dir();
    let obj = tmp.join("elisprs_aot.o");
    compile_file(src, &obj)?;

    let main_c = tmp.join("elisprs_aot_main.c");
    std::fs::write(
        &main_c,
        "extern long fusevm_aot_run_embedded(void);\n\
         int main(void) { return (int)fusevm_aot_run_embedded(); }\n",
    )
    .map_err(|e| e.to_string())?;

    let lib = staticlib_path()?;
    let mut cmd = std::process::Command::new("cc");
    cmd.arg(&main_c).arg(&obj).arg(&lib).arg("-o").arg(out);
    // Platform libraries the Rust staticlib needs.
    if cfg!(target_os = "macos") {
        cmd.args([
            "-framework",
            "CoreFoundation",
            "-framework",
            "Security",
            "-liconv",
            "-lc++",
        ]);
    } else {
        cmd.args(["-lpthread", "-ldl", "-lm", "-lrt"]);
    }
    let status = cmd.status().map_err(|e| format!("cc: {e}"))?;
    if !status.success() {
        return Err(format!("link failed (cc exit {:?})", status.code()));
    }
    Ok(())
}

/// Locate `libelisprs.a` (a sibling of the running `elisp` binary, or `$ELISPRS_STATICLIB`).
fn staticlib_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("ELISPRS_STATICLIB") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let lib = exe.parent().ok_or("no exe dir")?.join("libelisprs.a");
    if lib.exists() {
        Ok(lib)
    } else {
        Err(format!(
            "libelisprs.a not found next to {}; build the staticlib or set ELISPRS_STATICLIB",
            exe.display()
        ))
    }
}
