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
pub mod lsp;
pub mod prelude;
pub mod reader;
pub mod regexp;

pub use fusevm::Value;
pub use host::{reset_host, run_chunk, with_host};

/// Read, lower, and run a source string on fusevm; return the last value.
pub fn eval_str(src: &str) -> Result<Value, String> {
    load_prelude();
    eval_forms(src)
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
    let forms = host::with_host(|h| reader::read_all(h, src).map(|fs| splice_top_forms(h, fs)))?;
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
    let Ok(forms) = host::with_host(|h| reader::read_all(h, prelude::PRELUDE)) else {
        return;
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

/// Run a `.el` file, using the rkyv bytecode cache at `~/.elisprs/scripts.rkyv`.
/// On a fresh hit, the per-form chunks + a clean heap image are loaded and run
/// directly — skipping read / macro-expand / lower AND the prelude rebuild.
pub fn eval_file(path: &str) -> Result<Value, String> {
    let mtime_ns = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);

    // Schema key folds the builtin layout + prelude into the version, so editing
    // either invalidates stale bytecode without a manual version bump. Computed
    // on a builtins-only host (no user file loaded yet) so it's stable per build.
    let schema_key = {
        host::reset_host();
        cache::schema_key(host::with_host(|h| h.builtin_fingerprint()))
    };

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
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    host::reset_host();
    load_prelude();
    let builtin_count = host::with_host(|h| h.builtin_count());
    let prelude_end = host::with_host(|h| h.arena_len());
    // The clean prelude heap, captured BEFORE the file runs: this is the state the
    // cached chunks replay onto, so it must not contain any of their effects.
    let clean_prelude = host::with_host(|h| h.export_heap_range(builtin_count, prelude_end));

    // Bind load-file-name only while the forms run; unbind before the clean heap
    // image is captured so the cached image carries no transient load binding.
    let (chunks, last) = with_load_file_name(path, || run_top_forms(&src))?;
    let heap = host::with_host(|h| h.export_heap_image_clean(prelude_end, &clean_prelude));
    let oclosure_meta = host::with_host(|h| h.export_oclosure_meta());
    cache::put(path, mtime_ns, &schema_key, &chunks, &heap, &oclosure_meta);
    Ok(last)
}
