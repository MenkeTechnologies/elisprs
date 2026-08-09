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
//! handles into the ElispHost heap, so they only mean anything if the heap they
//! index is rebuilt first. `elisp --aot` therefore exports the whole arena as a
//! `SerObj` image and parks it in `chunk.names` behind [`crate::host::HEAP_IMAGE_TAG`];
//! the hook below re-imports it before the chunk runs, in arena order, so every
//! handle lands on the object it had at compile time. A user program's interned
//! symbols, quoted data, vectors, records, hash tables, char-tables, bignums and
//! closures all come back this way — they are *not* limited to what the prelude
//! happens to rebuild.
//!
//! The image is the whole contract, so anything it drops is a silent wrong
//! answer rather than an error. Two things it still drops, both deliberate:
//! `Obj::Buffer` / `Obj::Marker` / `Obj::Obarray` become placeholder symbols
//! (they are live runtime state, and the slot exists only to keep arena indices
//! aligned), and a subr is re-created by `install` rather than carried. Closure
//! *source* used to be dropped too — see `SerObj::Closure::arglist`.

use fusevm::VM;

/// Turn the AOT run's raw exit code into the process exit code, reporting any
/// uncaught elisp error first. `main` (emitted by [`crate::aot`]) pipes
/// `fusevm_aot_run_embedded()`'s return value through here.
///
/// This exists because an elisp error never reaches fusevm's `VMResult`.
/// `host::abort` parks the message in the thread-local host error slot and winds
/// `vm.ip` past the last op, so the VM terminates *normally* and fusevm's AOT
/// driver sees `VMResult::Halted` and returns 0. The interpreted driver reads
/// that slot right after `vm.run()` (`host::run_chunk`); the AOT driver lives in
/// fusevm and cannot. The result, before this hook: EVERY uncaught elisp error
/// in a standalone AOT binary exited 0 with nothing on stdout or stderr —
/// byte-identical to a successful run of a program that prints nothing. A
/// `(error "boom")` binary and a `(princ "")` binary were indistinguishable.
///
/// The message and exit status match the interpreted driver's (`main.rs`:
/// `eprintln!("error: {}", format_error(&e))` + `ExitCode::FAILURE`) so the two
/// execution paths report an identical failure.
///
/// Kept separate from `main` (rather than wrapping `fusevm_aot_run_embedded`
/// in Rust) so this function references no AOT-object-only symbols: it is
/// compiled into the plain `elisprs` rlib too, and pulling in
/// `fusevm_aot_chunk_blob` would leave the ordinary `elisp` binary with
/// undefined symbols at link time.
#[no_mangle]
pub extern "C" fn elisprs_aot_finish(code: i64) -> i64 {
    // Take the error BEFORE formatting: `format_error` calls back into elisp
    // (`error-message-string`), and that call's `run_chunk` clears the slot.
    let err = crate::host::with_host(|h| h.take_error());
    let Some(e) = err else { return code };
    eprintln!("error: {}", crate::format_error(&e));
    1
}

/// Register the elisp subrs + extension handlers on the AOT VM. Required link
/// symbol for a standalone elisp AOT binary.
///
/// # Safety
/// `vm` must be a valid, exclusively-borrowable pointer (fusevm's AOT entry
/// passes one).
#[no_mangle]
pub unsafe extern "C" fn fusevm_aot_register_builtins(vm: *mut VM) {
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
    // The same numeric contract `run_chunk` installs on the interpreted VM. Both
    // are required, and leaving them off is silent: fusevm's native int ops wrap,
    // so an AOT binary answered `(* 1000000000000 1000000000000)` =>
    // 2003764205206896640 and `(+ 9223372036854775807 1)` => 1.0 where the
    // interpreter (and Emacs) promote to a bignum. `set_fixnum_range` additionally
    // makes a result that merely leaves Emacs's 62-bit fixnum range — still an
    // `i64` — promote, which is what `bignump`/`fixnump` report on.
    vm.set_numeric_hook(std::sync::Arc::new(crate::host::numeric_hook));
    vm.set_fixnum_range(
        crate::host::MOST_NEGATIVE_FIXNUM,
        crate::host::MOST_POSITIVE_FIXNUM,
    );
}
