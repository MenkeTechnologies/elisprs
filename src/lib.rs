//! elisprs — Emacs Lisp as a fusevm frontend.
//!
//! Pipeline: `reader` builds elisp forms as ElispHost heap objects → `compiler`
//! lowers each to a `fusevm::Chunk` → fusevm executes it, calling back into the
//! `host` (via fusevm's extension handler) for elisp-specific operations. There
//! is no bespoke VM or JIT here — execution and codegen live in fusevm.

pub mod aot;
pub mod aot_runtime;
pub mod banner;
pub mod builtins;
pub mod cache;
pub mod compiler;
pub mod dap;
pub mod host;
pub mod intercepts;
pub mod lsp;
pub mod prelude;
pub mod reader;
pub mod regexp;
pub mod rust_ffi;
pub mod tiers;

pub use fusevm::Value;
pub use host::{reset_host, run_chunk, with_host};

/// Native stack an elisp evaluation needs.
///
/// One elisp call frame costs several native frames — `call_function` →
/// `run_closure` → `run_chunk` → `VM::run` → `ext_dispatch` → `call_function` —
/// and an unoptimized build's frames are large, so the platform defaults (8 MiB
/// on the macOS main thread, 2 MiB for a spawned thread, which is also what the
/// test harness gives a test) ran out at roughly 70 elisp frames while Emacs
/// allows `max-lisp-eval-depth` = 1600. This is address space, not committed
/// memory: pages are backed only as they are touched.
///
/// The depth limit itself is enforced in elisp terms (`max-lisp-eval-depth`, in
/// [`host`]), so runaway recursion signals `excessive-lisp-nesting` and can be
/// caught; this stack only has to be big enough to reach that limit first.
pub const INTERP_STACK_BYTES: usize = 1 << 29; // 512 MiB

/// Run `f` on a thread with [`INTERP_STACK_BYTES`] of stack.
///
/// The host is a thread-local, so `f` gets a fresh interpreter — everything an
/// evaluation touches must run inside this call, not around it.
pub fn with_interpreter_stack<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("elisprs-interp".to_string())
        .stack_size(INTERP_STACK_BYTES)
        .spawn(f)
        .expect("spawn interpreter thread")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e))
}

/// Read, lower, and run a source string on fusevm; return the last value.
pub fn eval_str(src: &str) -> Result<Value, String> {
    load_prelude();
    eval_forms(src)
}

/// Read every top-level form of `src` into the reader's s-expression form — the
/// elisp AST. Backs `elisp --dump-ast`; does not macro-expand, lower, or run.
pub fn read_forms(src: &str) -> Result<Vec<Value>, String> {
    host::with_host(|h| reader::read_all(h, src))
}

/// Read + lower `src` into a single fusevm chunk, exactly as an evaluated run
/// would (prelude loaded, top-level `progn` spliced, macros expanded) but
/// WITHOUT running it. Backs `elisp --dump-bytecode` / `--disasm`.
pub fn compile_str(src: &str) -> Result<fusevm::Chunk, String> {
    load_prelude();
    let forms = host::with_host(|h| reader::read_all(h, src).map(|fs| splice_top_forms(h, fs)))?;
    let mut expanded = Vec::with_capacity(forms.len());
    for form in &forms {
        expanded.push(host::macroexpand_all(form)?);
    }
    host::with_host(|h| compiler::compile_program(h, &expanded))
}

/// Splice literal top-level `(progn …)` forms into their subforms (recursively),
/// so an earlier subform's `defmacro`/`defun` is in effect before a later subform
/// is compiled — matching Emacs's top-level handling.
fn splice_top_forms(h: &mut host::ElispHost, forms: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::new();
    for f in forms {
        let progn = match h.list_vec(&f) {
            Some(v) if !v.is_empty() && h.sym_name(&v[0]).as_deref() == Some("progn") => Some(v),
            _ => None,
        };
        match progn {
            Some(v) => out.extend(splice_top_forms(h, v[1..].to_vec())),
            None => out.push(f),
        }
    }
    out
}

/// Read every top-level form from `src` and evaluate them IN THE CURRENT host,
/// one at a time (read → splice → macro-expand → lower → run), so an in-file
/// `defmacro`/`defvar` is already in effect for the forms that follow it. Does
/// NOT reset the host or (re)load the prelude — the caller owns host lifecycle.
///
/// Returns the compiled per-form chunks (for the bytecode cache) and the value
/// of the last form. This is the single "run these forms in the live host"
/// machinery shared by `eval_forms`, `eval_file`'s cache-miss path, and the
/// `load` builtin, so none of them re-implement a divergent evaluator.
pub(crate) fn run_top_forms(src: &str) -> Result<(Vec<fusevm::Chunk>, Value), String> {
    // Rewrite any inline `rust { ... }` FFI block into a `(__rust-compile ...)`
    // call before the reader runs (no-op when the source has no `rust` token).
    let src = rust_ffi::desugar(src);
    let forms = host::with_host(|h| reader::read_all(h, &src).map(|fs| splice_top_forms(h, fs)))?;
    let mut chunks = Vec::with_capacity(forms.len());
    let mut last = Value::Undef;
    for form in &forms {
        // Macro-expand before lowering (a prior form's `defmacro` is in effect).
        let expanded = host::macroexpand_all(form)?;
        let chunk = host::with_host(|h| compiler::compile_top(h, &expanded))?;
        chunks.push(chunk.clone());
        last = host::run_chunk(chunk)?;
    }
    Ok((chunks, last))
}

/// Evaluate a sequence of top-level forms (macro-expand → lower → run).
fn eval_forms(src: &str) -> Result<Value, String> {
    run_top_forms(src).map(|(_, last)| last)
}

/// Load the derived-surface prelude once per host, best-effort (a broken
/// definition is skipped, not fatal).
fn load_prelude() {
    if host::prelude_loaded() {
        return;
    }
    host::set_prelude_loaded(true);
    // The nadvice segment is loaded after the core PRELUDE because it depends on
    // oclosure/gv/cl-lib defined there (and is kept separate — see prelude::NADVICE).
    for src in [prelude::PRELUDE, prelude::NADVICE] {
        let Ok(forms) = host::with_host(|h| reader::read_all(h, src)) else {
            continue;
        };
        for form in &forms {
            let r = (|| -> Result<(), String> {
                let expanded = host::macroexpand_all(form)?;
                let chunk = host::with_host(|h| compiler::compile_top(h, &expanded))?;
                host::run_chunk(chunk)?;
                Ok(())
            })();
            if let Err(e) = r {
                eprintln!("elisprs: prelude form failed: {e}");
            }
        }
    }
}

/// Render a value (prin1 style when `readable`).
pub fn print(v: &Value, readable: bool) -> String {
    host::with_host(|h| h.print(v, readable))
}

/// Render an internal error string as Emacs's `error-message-string` would: a
/// condition like `void-variable: foo` becomes "Symbol's value as variable is
/// void: foo". Falls back to the raw string if formatting fails.
pub fn format_error(e: &str) -> String {
    let obj = host::with_host(|h| h.make_error_object(e));
    let func = host::with_host(|h| h.intern("error-message-string"));
    match host::call_function(&func, &[obj]) {
        Ok(Value::Str(s)) => s.to_string(),
        _ => e.to_string(),
    }
}

/// Bind `load-file-name`/`load-true-file-name`/`load-in-progress` to `path`'s
/// absolute form for the duration of `run`, then restore them (even on error).
///
/// Emacs never evaluates an init/startup file "bare": it loads it via `load`
/// (`emacs -l FILE`, and the startup init-file load), so `load-file-name` is
/// bound to the file while its forms run. `eval_file` is elisprs's `emacs -l`
/// path, so it binds the same vars — otherwise `(file-name-directory
/// load-file-name)`, which real init files (e.g. Spacemacs `init.el`) use to
/// locate sibling files, sees a void/nil variable.
fn with_load_file_name<T>(
    path: &str,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let abs = Value::str(host::load_abspath(path).to_string_lossy().into_owned());
    let depth = host::with_host(|h| {
        let d = h.specdepth();
        let lfn = h.intern("load-file-name");
        let ltn = h.intern("load-true-file-name");
        let lip = h.intern("load-in-progress");
        let _ = h.specbind(&lfn, abs.clone());
        let _ = h.specbind(&ltn, abs.clone());
        let _ = h.specbind(&lip, Value::Bool(true));
        d
    });
    let r = run();
    host::with_host(|h| h.unbind_to(depth));
    r
}

/// Which Emacs invocation `elisp FILE` is standing in for.
///
/// The two differ in the buffer the file's forms run in, and therefore in the
/// syntax table every `char-syntax` / `\sC` / `skip-syntax-*` answer comes from.
/// Measured on GNU Emacs 30.2 with the same probe file:
///
/// ```text
/// emacs -Q --batch -l probe.el  => buffer "*scratch*", lisp-interaction-mode,
///                                  (char-syntax ?.) => 95 (?_)
/// emacs --script    probe.el    => buffer " *load*",   fundamental-mode,
///                                  (char-syntax ?.) => 46 (?.)
/// ```
///
/// [`EntryPoint::Load`] is the default and the column `scripts/fuzz_parity.sh`
/// compares against.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntryPoint {
    /// `emacs -l FILE`: the file is loaded, `*scratch*` stays current.
    Load,
    /// `emacs --script FILE`: the file is evaluated *in* the ` *load*` buffer, so
    /// it reads the standard syntax table rather than the elisp-mode one.
    Script,
}

/// Entry-point state that depends on *this* invocation rather than on the
/// prelude, applied to a host that is ready to run the script's forms.
///
/// All of it must be applied on the cache-hit path as well as the cache-miss
/// path: a hit skips the prelude entirely, and the argument list is different on
/// every run, so none of it can be baked into the heap image.
///
/// - ` *load*` is the buffer `load` reads the file through. `emacs -Q --batch
///   --eval` reports three buffers, `emacs -Q --batch -l FILE` reports four; the
///   slot is reserved in the built-in prefix (see `ElispHost::open_load_buffer`)
///   precisely so both cache paths see the same arena handle. Under
///   [`EntryPoint::Script`] it also becomes *current*, which is the whole
///   observable difference between the two entry points.
/// - `command-line-args-left` (and its alias `argv`) is everything after the
///   script path, matching `emacs -l FILE a b c` => `("a" "b" "c")`;
///   `command-line-args` is the whole invocation.
fn install_entry_point_state(path: &str, src: &str, entry: EntryPoint) {
    // The initial buffer's syntax table, re-installed on EVERY run.
    //
    // The prelude ends with `(set-syntax-table emacs-lisp-mode-syntax-table)`,
    // which models `emacs -Q --batch` starting in `*scratch*` under
    // `lisp-interaction-mode`. That is a BUFFER-LOCAL binding, and buffer locals
    // live in the buffer struct, not in the arena — so the heap image does not
    // carry it and a cache hit, which skips the prelude entirely, left the
    // initial buffer on `standard-syntax-table`. The whole `char-syntax` /
    // `\sC` / `skip-syntax-*` / `forward-sexp` family then answered the
    // `--script` column on a warm cache and the `-l` column on a cold one:
    //
    //   $ elisp probe.el   # cold: (char-syntax ?.) => 95   (correct)
    //   $ elisp probe.el   # warm: (char-syntax ?.) => 46   (the standard table)
    //
    // Setting it here — the one place both cache paths run through — is the
    // same treatment `command-line-args` gets below, and for the same reason:
    // per-run state must never be reconstructed from the image. It happens
    // before the ` *load*` buffer is opened or selected, so it lands on the
    // initial buffer; `--script` then selects ` *load*`, whose own local is
    // unset, and correctly reads the standard table.
    host::with_host(|h| {
        let tbl_sym = h.intern("emacs-lisp-mode-syntax-table");
        if let Ok(tbl) = h.get_value(&tbl_sym) {
            let cur = h.intern("--current-syntax-table--");
            let _ = h.set_value(&cur, tbl);
        }
    });
    let load_buf = host::with_host(|h| h.open_load_buffer(src, true));
    if entry == EntryPoint::Script {
        host::with_host(|h| {
            let obj = h.buffer_object(load_buf);
            let _ = h.set_buffer(&obj);
        });
    }
    let args: Vec<String> = std::env::args().collect();
    // Everything after the script argument. `main` picks the first non-flag
    // argument as the file, so find that same one rather than assuming a slot.
    let left: Vec<Value> = match args.iter().position(|a| a == path) {
        Some(i) => args[i + 1..].iter().cloned().map(Value::str).collect(),
        None => Vec::new(),
    };
    host::with_host(|h| {
        let all: Vec<Value> = args.iter().cloned().map(Value::str).collect();
        let all = h.list_from(all);
        let left = h.list_from(left);
        let cla = h.intern("command-line-args");
        let left_sym = h.intern("command-line-args-left");
        let _ = h.set_value(&cla, all);
        let _ = h.set_value(&left_sym, left);
    });
}

/// Run a `.el` file as `emacs -l FILE` would. See [`eval_file_as`].
pub fn eval_file(path: &str) -> Result<Value, String> {
    eval_file_as(path, EntryPoint::Load)
}

/// Run a `.el` file as `emacs --script FILE` would: the forms are evaluated in
/// the ` *load*` buffer, whose syntax table is the standard one.
pub fn eval_script(path: &str) -> Result<Value, String> {
    eval_file_as(path, EntryPoint::Script)
}

/// Run a `.el` file, using the rkyv bytecode cache at `~/.elisprs/scripts.rkyv`.
/// On a fresh hit, the per-form chunks + a clean heap image are loaded and run
/// directly — skipping read / macro-expand / lower AND the prelude rebuild.
pub fn eval_file_as(path: &str, entry: EntryPoint) -> Result<Value, String> {
    let mtime_ns = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);

    // Schema key folds the builtin layout + prelude into the version, so editing
    // either invalidates stale bytecode without a manual version bump. Computed
    // on a builtins-only host (no user file loaded yet) so it's stable per build.
    // The entry point is folded in: it selects the buffer the forms run in, and a
    // top-level form that reads the buffer can change how a *later* form
    // macro-expands, so chunks compiled under one entry point must not be
    // replayed under the other.
    let schema_key = {
        host::reset_host();
        let base = cache::schema_key(host::with_host(|h| h.builtin_fingerprint()));
        match entry {
            EntryPoint::Load => base,
            EntryPoint::Script => format!("{base}-script"),
        }
    };

    // The source is needed on BOTH paths, not just the miss: ` *load*` holds the
    // file's text in Emacs, so skipping the read on a hit would make `buffer-size`
    // depend on whether the cache was warm. Reading a script is negligible next to
    // the parse + macro-expand + lower + prelude rebuild the cache actually skips.
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

    let debug = std::env::var_os("ELISPRS_CACHE_DEBUG").is_some();
    if let Some((chunks, heap, oclosure_meta)) = cache::get(path, mtime_ns, &schema_key) {
        if debug {
            eprintln!("elisprs: cache HIT  {path} ({} chunks)", chunks.len());
        }
        host::reset_host();
        host::with_host(|h| {
            h.import_heap_image(heap);
            // The OClosure table is built when the prelude runs, which this hit
            // skips — restore it or every prelude OClosure comes back as a plain
            // closure.
            h.import_oclosure_meta(oclosure_meta);
        });
        install_entry_point_state(path, &src, entry);
        return with_load_file_name(path, || {
            let mut last = Value::Undef;
            for chunk in chunks {
                last = host::run_chunk(chunk)?;
            }
            Ok(last)
        });
    }
    if debug {
        eprintln!("elisprs: cache MISS {path}");
    }

    // Cache miss: compile + run form-by-form (so an in-file defmacro is in effect
    // before later forms), capturing each chunk and a clean heap image.
    host::reset_host();
    load_prelude();
    let builtin_count = host::with_host(|h| h.builtin_count());
    let prelude_end = host::with_host(|h| h.arena_len());
    // The clean prelude heap, captured BEFORE the file runs: this is the state the
    // cached chunks replay onto, so it must not contain any of their effects.
    // Also before `install_entry_point_state`, so THIS run's argv never reaches
    // the cached image and the next run's `install_entry_point_state` is the only
    // thing that sets it.
    let clean_prelude = host::with_host(|h| h.export_heap_range(builtin_count, prelude_end));
    install_entry_point_state(path, &src, entry);

    // Bind load-file-name only while the forms run; unbind before the clean heap
    // image is captured so the cached image carries no transient load binding.
    let (chunks, last) = with_load_file_name(path, || run_top_forms(&src))?;
    let heap = host::with_host(|h| h.export_heap_image_clean(prelude_end, &clean_prelude));
    let oclosure_meta = host::with_host(|h| h.export_oclosure_meta());
    cache::put(path, mtime_ns, &schema_key, &chunks, &heap, &oclosure_meta);
    Ok(last)
}
