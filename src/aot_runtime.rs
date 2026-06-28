//! AOT runtime hook.
//!
//! fusevm's AOT model embeds the bincode-serialized `Chunk` in the object and,
//! at load, deserializes it and runs it on a `VM` (`fusevm_aot_run_embedded`).
//! Before running, fusevm calls back into the frontend via the C symbol
//! `fusevm_aot_register_builtins` to install the frontend's subrs and extension
//! handlers. A standalone elisp binary = the AOT object + the elisprs runtime
//! (this hook) + a `main` that calls `fusevm::aot::fusevm_aot_run_embedded()`.
//!
//! NOTE (the elisp-specific catch): elisp chunk constants are `Value::Obj`
//! handles into the ElispHost heap. The prelude is reloaded here so prelude
//! symbols re-intern to the same deterministic handles, but a *user* program's
//! interned symbols / quoted data / closure templates are not yet reconstructed
//! at load — that requires constant reification (compiling symbols/quotes/
//! closures as runtime construction instead of baked heap handles). Until then,
//! AOT objects run correctly only for programs whose constants are all
//! reconstructed by the prelude/builtins. See compiler.rs for the reification
//! work item.

use fusevm::VM;

/// Register the elisp subrs + extension handlers on the AOT VM. Required link
/// symbol for a standalone elisp AOT binary.
///
/// # Safety
/// `vm` must be a valid, exclusively-borrowable pointer (fusevm's AOT entry
/// passes one).
#[no_mangle]
pub extern "C" fn fusevm_aot_register_builtins(vm: *mut VM) {
    let vm = unsafe { &mut *vm };
    // Rebuild the user/prelude heap from the image embedded in `chunk.names`.
    // (The image already contains the prelude, so we do NOT load it separately —
    // that would duplicate objects and misalign handles.)
    let images: Vec<Vec<crate::host::SerObj>> = vm
        .chunk
        .names
        .iter()
        .filter_map(|n| n.strip_prefix(crate::host::HEAP_IMAGE_TAG))
        .filter_map(|j| serde_json::from_str(j).ok())
        .collect();
    crate::host::with_host(|h| {
        for img in images {
            h.import_heap_image(img);
        }
    });
    vm.set_extension_handler(Box::new(crate::host::ext_dispatch));
    vm.set_extension_wide_handler(Box::new(crate::host::ext_dispatch_wide));
}
