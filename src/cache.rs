//! rkyv-backed bytecode cache for elisp scripts (mirrors zshrs/awkrs/strykelang
//! `script_cache`, brought to full toolchain parity).
//!
//! Single-file shard at `~/.elisprs/scripts.rkyv`. On the 2nd+ run of an
//! unchanged file, elisprs skips reading / macro-expanding / lowering AND the
//! prelude rebuild: it deserializes the per-form `fusevm::Chunk`s + a clean heap
//! image and runs them directly.
//!
//! Layout: the *outer* container is a zero-copy rkyv archive (validated via
//! `check_archived_root`); the *inner* per-form `Chunk` blobs and heap image are
//! bincode, because `fusevm::Chunk`/`Value` are serde-owned, not `rkyv::Archive`
//! (the same split zshrs uses). Keyed by absolute path + mtime + a *schema key*.
//!
//! The schema key (`schema_key`) is the elisprs version combined with a
//! fingerprint of the builtin object layout and the prelude source. Compiled
//! chunks bake in builtin arena handles and macro-expansions, so any change to
//! the registered subrs or the prelude must invalidate cached bytecode even
//! within a single released version — otherwise stale chunks resolve handles to
//! the wrong builtins. Folding the fingerprint into the key makes that automatic.
//!
//! Parity guards over the previous bare version:
//!   - **`flock(LOCK_EX)`** on `scripts.rkyv.lock` so concurrent elisprs
//!     processes serialize their read-modify-write and never clobber each
//!     other's entries (multiple loop sessions run at once).
//!   - **`fsync` + unique `.tmp.<pid>.<nanos>` + atomic rename** so a crash mid
//!     write can't leave a torn shard.
//!   - **magic / format_version / pointer_width header** so a wrong-format or
//!     cross-arch shard fails fast instead of feeding mismatched bytecode.
//!   - **binary-mtime guard** so a dev rebuild that changes lowering without
//!     touching builtins/prelude (which `schema_key` wouldn't catch) still
//!     invalidates stale entries.
//!   - **`stats` / `clear` / `evict_stale` / `cache_enabled`** management
//!     surface, matching the other four frontends.

use crate::host::SerObj;
use fusevm::Chunk;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write as IoWrite;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Magic header bytes — fail fast if a wrong-format file is read. ("ELSP")
pub const SHARD_MAGIC: u32 = 0x454C_5350;
/// Bumped on incompatible rkyv schema changes. v2 adds the header + binary-mtime;
/// v3 adds `SerObj::Symbol::interned`; v4 rolls every runtime-mutated symbol cell
/// (function, buffer-local-auto, alias) back to its pre-run state, not just the value.
/// symbol in the heap image and an uninterned prelude local could shadow a builtin.
/// v5 adds `SerObj::Record`/`SerObj::BoolVector` — mid-enum variants that shift the
/// bincode discriminants of every later `SerObj`, so a v4 heap image misdeserializes.
/// v6 adds `arglist`/`src_body` to `SerObj::Closure` (the closure's printed source).
/// bincode is NOT self-describing and reads fields positionally, so `#[serde(default)]`
/// does nothing for it: decoding a v5 closure with the v6 struct runs off the end of
/// that object and into the next one. Measured — serializing the v5 `Closure` layout
/// and deserializing it as the v6 `SerObj` gives
/// `io error: unexpected end of file`. `get` would turn that into a cache miss, but
/// only when the over-read happens to fail; a closure in the middle of an image can
/// consume a following object's bytes and still produce a plausible, wrong image.
/// The version bump makes `header_ok` reject a stale shard outright, before any
/// inner decode is attempted. (The AOT heap image is serde_json, which IS
/// self-describing, so it honours `#[serde(default)]` — but it is embedded in the
/// object and rebuilt with it, so it is never stale.)
/// v7 adds the `CHECK_ARITY` guard op ahead of the argument code of a call. No
/// serialized *struct* changed shape, so a v6 shard still decodes cleanly — that
/// is exactly why the bump is needed. A v6 chunk carries no guard, so replaying
/// it would evaluate a wrong-arity subr's arguments before signalling, silently
/// serving the pre-fix behaviour to anyone with a warm cache.
/// v8 adds `Entry::introspection_cells`: the special-form / intrinsic-macro
/// function cells (`when`, `unless`) live in a side table on the host, not in the
/// arena, so a cache hit — which skips the prelude that registers them — used to
/// come back with `(fboundp 'when)` nil and `(symbol-function 'when)` nil where a
/// cold run answered `t` and the `(macro . FUNCTION)` pair. `Entry` gained a field,
/// which shifts the rkyv layout, so a v7 shard must be rejected outright.
/// v10 makes an elisp string an arena OBJECT (`Obj::Str`) instead of a bare
/// `Value::Str`, so `aset`/`store-substring` can write through every reference
/// to it. `SerObj` gained a `Str` variant, and — more to the point — every
/// string literal in a cached chunk is now a `Value::Obj` handle rather than an
/// inline `Value::Str`. A v9 shard therefore replays literals that are not
/// string objects at all: they would print correctly and then signal
/// `arrayp` on the first write, which is the pre-fix behaviour served from a
/// warm cache.
/// v12 adds `builtin_cells`: the value/function cells the prelude installs on
/// symbols that already exist when `builtins::install` finishes. A v11 shard has
/// none, so replaying it leaves those symbols as `install` made them — which is
/// how `(macrop 'save-current-buffer)` answered `t` cold and `nil` warm, and how
/// a warm run then failed to compile a `save-current-buffer` form at all.
///
/// v11 adds a hash table's `define-hash-table-test` functions
/// (`SerObj::HashTable::user_test`). A v10 shard replays such a table as one
/// with NO user test, which is worse than rejecting it: the table would come
/// back answering `eql` and silently miss every key it used to find.
///
/// v13 moves the post-prelude heap out of the entry and into the SHARD. Through
/// v12 each entry carried a whole image — measured at 6,694,994 bytes for
/// `examples/arithmetic.el` against 1,208 bytes of actual bytecode — even
/// though `arena[builtin_count, prelude_end)` is produced by running the
/// prelude and nothing else, so it is the same bytes for every file compiled by
/// this binary under this schema key. 73 entries therefore stored 73 copies of
/// it, which is what drove the shard to 525 MB and made `put` (a whole-file
/// rewrite) cost O(entries x prelude) per run. It is now stored once as
/// `Shard::base`, and an entry keeps only `heap_tail` — `arena[prelude_end..]`,
/// the objects the file itself created.
///
/// Old shards do not migrate in place: `header_ok` compares `format_version`,
/// so a v12 shard misses on every `get` and the next `put` writes a fresh v13
/// shard over it. That is deliberate — a v12 `Entry` has a `heap` field where
/// v13 has `heap_tail` + `base_fingerprint`, and rkyv reads its archive
/// positionally, so a v12 shard read as v13 is not a partial answer but a wrong
/// one. Rebuilding silently costs one cold compile per script and is the same
/// path every previous bump took.
pub const SHARD_FORMAT_VERSION: u32 = 13;

/// The cache schema key: elisprs version + a builtin/prelude fingerprint. A
/// shard built under a different key is ignored (and overwritten on the next
/// `put`), so editing `builtins::install` or the prelude never serves a stale
/// chunk.
pub fn schema_key(builtin_fingerprint: u64) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    builtin_fingerprint.hash(&mut hasher);
    crate::prelude::PRELUDE.hash(&mut hasher);
    // NADVICE is a second prelude segment (loaded after PRELUDE); it defines heap
    // symbols too, so a change to it must invalidate the heap image just like PRELUDE.
    crate::prelude::NADVICE.hash(&mut hasher);
    format!("{}-{:016x}", env!("CARGO_PKG_VERSION"), hasher.finish())
}

/// Shard header: format identity + provenance. Guards against wrong-format,
/// cross-arch, and cross-version shards before any entry is trusted.
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct ShardHeader {
    magic: u32,
    format_version: u32,
    pointer_width: u32,
    built_at_secs: u64,
    /// `schema_key` the shard was written under.
    schema_key: String,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct Entry {
    mtime_ns: i64,
    /// elisprs binary mtime (secs) when this entry was written.
    binary_mtime_at_cache: i64,
    /// Unix seconds the entry was written.
    cached_at_secs: i64,
    /// bincode `fusevm::Chunk`, one per top-level form.
    forms: Vec<Vec<u8>>,
    /// bincode `Vec<SerObj>` — `arena[prelude_end..]`, the objects THIS file
    /// created, cleaned of its own run-time effects. The image a hit replays is
    /// [`Shard::base`]`.heap` followed by this.
    heap_tail: Vec<u8>,
    /// [`BaseImage::fingerprint`] of the base this entry's handles were compiled
    /// against. The schema key already pins the prelude source and the builtin
    /// layout, so a mismatch means the base is not reproducible from them after
    /// all; a hit is refused rather than replayed onto a base whose handles may
    /// not line up.
    base_fingerprint: u64,
    /// bincode `Vec<(u32, u32, Vec<u32>)>` — the OClosure side table
    /// (`closure-handle, type, slots`). Not derivable from `heap`: it is built
    /// when the prelude runs, which a cache hit skips.
    oclosure_meta: Vec<u8>,
    /// bincode `Vec<(u32, Value)>` — the introspection function cells
    /// (`symbol-handle, cell`) of the forms elisprs lowers in the compiler. Like
    /// `oclosure_meta` this is host state outside the arena that the prelude
    /// builds, so a cache hit has to restore it or `(fboundp 'when)` answers nil
    /// on a warm run and `t` on a cold one.
    introspection_cells: Vec<u8>,
}

/// The post-prelude state, which every entry in the shard shares.
///
/// Running the prelude is the only thing that produces it, and the schema key
/// pins the prelude source, the builtin layout and the elisprs version — so all
/// entries under one shard replay onto identical bytes. Storing it per entry
/// (v12 and earlier) multiplied ~6.8 MB by the number of cached scripts.
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct BaseImage {
    /// bincode `Vec<SerObj>` — `arena[builtin_count, prelude_end)` as the
    /// prelude left it, captured before any file ran.
    heap: Vec<u8>,
    /// bincode `Vec<Option<SymbolBaseline>>` — the cells of the arena's builtin
    /// prefix, indexed by handle. `heap` deliberately starts at `builtin_count`
    /// (a builtin object is rebuilt by `install`, and an `Obj::Subr` cannot be
    /// serialized at all), but the prelude WRITES to symbols below that line, and
    /// those writes belong to the image just as much as the objects above it.
    builtin_cells: Vec<u8>,
    /// Hash of the two blobs above; recorded in every entry written against it.
    fingerprint: u64,
}

impl BaseImage {
    fn fingerprint_of(heap: &[u8], builtin_cells: &[u8]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        heap.hash(&mut h);
        builtin_cells.hash(&mut h);
        h.finish()
    }

    fn new(heap: Vec<u8>, builtin_cells: Vec<u8>) -> Self {
        let fingerprint = Self::fingerprint_of(&heap, &builtin_cells);
        Self {
            heap,
            builtin_cells,
            fingerprint,
        }
    }

    fn bytes(&self) -> u64 {
        (self.heap.len() + self.builtin_cells.len()) as u64
    }
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct Shard {
    header: ShardHeader,
    /// Shared by every entry — see [`BaseImage`].
    base: BaseImage,
    entries: HashMap<String, Entry>,
}

fn elisprs_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".elisprs"))
}

fn shard_path() -> Option<PathBuf> {
    Some(elisprs_dir()?.join("scripts.rkyv"))
}

fn lock_path() -> Option<PathBuf> {
    Some(elisprs_dir()?.join("scripts.rkyv.lock"))
}

/// Default shard path for tooling / diagnostics.
pub fn default_cache_path() -> PathBuf {
    shard_path().unwrap_or_else(|| PathBuf::from("/tmp/.elisprs/scripts.rkyv"))
}

/// `ELISPRS_CACHE=0|false|no` disables the cache entirely.
pub fn cache_enabled() -> bool {
    !matches!(
        std::env::var("ELISPRS_CACHE").as_deref(),
        Ok("0") | Ok("false") | Ok("no")
    )
}

/// Default byte budget for the shard, in bytes (64 MiB): the shared
/// [`BaseImage`] plus every entry's blobs.
///
/// The shard is a SINGLE file that `put` rewrites whole: it reads every byte,
/// rkyv-validates it, inserts one entry, re-serializes and renames. That is
/// O(shard) per script run, so an unbounded shard makes every run slower than
/// the last.
///
/// Through format v12 that was ruinous, because each entry carried its own copy
/// of the post-prelude heap (~6.8 MB): the shard here reached 77 entries /
/// 525,470,299 bytes, at which point `elisp FILE` burned 18.9s of CPU without
/// finishing while `ELISPRS_CACHE=0 elisp FILE` needed 0.52s. v13 stores that
/// image once per shard, so the budget now bounds a base plus a per-entry tail
/// of a few kB rather than a per-entry image — but it still bounds, because the
/// rewrite is still O(shard).
pub const DEFAULT_MAX_SHARD_BYTES: u64 = 64 * 1024 * 1024;

/// The shard's byte budget. `ELISPRS_CACHE_MAX_BYTES` overrides it; `0` means
/// no budget (the pre-budget behaviour, for a deliberate benchmark).
pub fn max_shard_bytes() -> u64 {
    match std::env::var("ELISPRS_CACHE_MAX_BYTES") {
        Ok(v) => v.trim().parse().unwrap_or(DEFAULT_MAX_SHARD_BYTES),
        Err(_) => DEFAULT_MAX_SHARD_BYTES,
    }
}

/// Serialized size of one entry's blobs — what the budget counts, alongside the
/// shard's single [`BaseImage`].
fn entry_bytes(e: &Entry) -> u64 {
    (e.forms.iter().map(Vec::len).sum::<usize>()
        + e.heap_tail.len()
        + e.oclosure_meta.len()
        + e.introspection_cells.len()) as u64
}

/// Drop entries until the shard fits `budget`, and drop entries whose source
/// file is gone or has been edited since (they can never be served again, so
/// they are pure weight — the same predicate [`evict_stale`] applies on
/// demand).
///
/// `keep` is the entry just written and is never evicted: evicting it would
/// make the run that paid for the compile get nothing for it, and a shard at
/// its budget would then never serve a hit again.
///
/// Eviction is by `cached_at_secs`, oldest first, so the working set of scripts
/// a session actually re-runs survives and one-off scripts age out.
fn enforce_budget(shard: &mut Shard, keep: &str, budget: u64) {
    shard.entries.retain(|p, e| {
        p == keep
            || match file_mtime_ns(Path::new(p)) {
                Some(ns) => ns == e.mtime_ns,
                None => false,
            }
    });
    if budget == 0 {
        return;
    }
    // The base counts: it is bytes `put` rewrites on every run just like an
    // entry's, and leaving it out would let the shard exceed the budget by a
    // whole heap image.
    let mut total: u64 = shard.base.bytes() + shard.entries.values().map(entry_bytes).sum::<u64>();
    if total <= budget {
        return;
    }
    // Oldest first. `keep` is excluded from the candidate list, not just
    // skipped, so a shard whose single entry already exceeds the budget still
    // serves that entry rather than emptying itself every run.
    let mut by_age: Vec<(i64, String)> = shard
        .entries
        .iter()
        .filter(|(p, _)| p.as_str() != keep)
        .map(|(p, e)| (e.cached_at_secs, p.clone()))
        .collect();
    by_age.sort_unstable();
    for (_, path) in by_age {
        if total <= budget {
            break;
        }
        if let Some(e) = shard.entries.remove(&path) {
            total -= entry_bytes(&e);
        }
    }
}

// ── flock guard ──────────────────────────────────────────────────────────────

/// Holds an exclusive `flock` on the lock file for the guard's lifetime; the
/// lock releases when the wrapped `File` is dropped (closed).
struct FlockGuard {
    _file: File,
}

fn acquire_lock() -> Option<FlockGuard> {
    let path = lock_path()?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .ok()?;
    // SAFETY: valid fd owned by `file`; blocks until the exclusive lock is held.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return None;
    }
    Some(FlockGuard { _file: file })
}

// ── header / mtime helpers ───────────────────────────────────────────────────

fn header_ok(h: &ArchivedShardHeader, schema_key: &str) -> bool {
    let magic: u32 = h.magic.into();
    let fv: u32 = h.format_version.into();
    let pw: u32 = h.pointer_width.into();
    magic == SHARD_MAGIC
        && fv == SHARD_FORMAT_VERSION
        && pw as usize == std::mem::size_of::<usize>()
        && h.schema_key.as_str() == schema_key
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// mtime of the running elisprs binary; cached for the process lifetime.
fn current_binary_mtime_secs() -> Option<i64> {
    static BIN_MTIME: OnceLock<Option<i64>> = OnceLock::new();
    *BIN_MTIME.get_or_init(|| {
        use std::os::unix::fs::MetadataExt;
        let exe = std::env::current_exe().ok()?;
        Some(std::fs::metadata(exe).ok()?.mtime())
    })
}

/// Source-file mtime as nanoseconds since the epoch (matches `eval_file`).
fn file_mtime_ns(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_nanos() as i64)
}

// ── shard read / write ───────────────────────────────────────────────────────

fn read_shard() -> Option<Shard> {
    let bytes = std::fs::read(shard_path()?).ok()?;
    let archived = rkyv::check_archived_root::<Shard>(&bytes).ok()?;
    archived.deserialize(&mut rkyv::Infallible).ok()
}

fn fresh_shard(schema_key: &str, base: BaseImage) -> Shard {
    Shard {
        base,
        header: ShardHeader {
            magic: SHARD_MAGIC,
            format_version: SHARD_FORMAT_VERSION,
            pointer_width: std::mem::size_of::<usize>() as u32,
            built_at_secs: now_secs() as u64,
            schema_key: schema_key.to_string(),
        },
        entries: HashMap::new(),
    }
}

fn owned_header_ok(h: &ShardHeader, schema_key: &str) -> bool {
    h.magic == SHARD_MAGIC
        && h.format_version == SHARD_FORMAT_VERSION
        && h.pointer_width as usize == std::mem::size_of::<usize>()
        && h.schema_key == schema_key
}

fn write_shard(shard: &Shard) -> std::io::Result<()> {
    let dir = elisprs_dir().ok_or_else(|| std::io::Error::other("no HOME"))?;
    std::fs::create_dir_all(&dir)?;
    let bytes = rkyv::to_bytes::<_, 4096>(shard)
        .map_err(|e| std::io::Error::other(format!("rkyv: {e:?}")))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!("scripts.rkyv.tmp.{}.{}", std::process::id(), nanos));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, dir.join("scripts.rkyv"))
}

// ── public API ───────────────────────────────────────────────────────────────

/// Cache lookup. Returns the per-form chunks + clean heap image on a fresh hit.
/// Everything a cache hit has to replay: the compiled chunks plus the host state
/// that building them produced but the arena does not hold.
pub struct CachedScript {
    pub chunks: Vec<Chunk>,
    pub heap: Vec<SerObj>,
    pub oclosure_meta: Vec<(u32, u32, Vec<u32>)>,
    pub introspection_cells: Vec<(u32, fusevm::Value)>,
    pub builtin_cells: Vec<Option<crate::host::SymbolBaseline>>,
}

/// `schema_key` must match the key the entry was written under (see `schema_key`).
/// Misses on: cache disabled, format/schema drift, mtime mismatch, or a binary
/// newer than the cached entry.
#[allow(clippy::type_complexity)]
pub fn get(path: &str, mtime_ns: i64, schema_key: &str) -> Option<CachedScript> {
    if !cache_enabled() {
        return None;
    }
    let bytes = std::fs::read(shard_path()?).ok()?;
    let shard = rkyv::check_archived_root::<Shard>(&bytes).ok()?;
    if !header_ok(&shard.header, schema_key) {
        return None;
    }
    let entry = shard.entries.get(path)?;
    let entry_mtime: i64 = entry.mtime_ns.into();
    if entry_mtime != mtime_ns {
        return None;
    }
    if let Some(bin_mtime) = current_binary_mtime_secs() {
        let cached: i64 = entry.binary_mtime_at_cache.into();
        if cached < bin_mtime {
            return None;
        }
    }
    let chunks: Vec<Chunk> = entry
        .forms
        .iter()
        .map(|b| bincode::deserialize(b))
        .collect::<Result<_, _>>()
        .ok()?;
    // The entry's handles were assigned against a specific base. The schema key
    // should already guarantee it, so a mismatch is a miss rather than an error.
    let base_fingerprint: u64 = entry.base_fingerprint.into();
    if base_fingerprint != BaseImage::fingerprint_of(&shard.base.heap, &shard.base.builtin_cells) {
        return None;
    }
    // The full image is the shard's shared base followed by this file's tail —
    // the same `Vec<SerObj>` v12 stored per entry, reassembled.
    let mut heap: Vec<SerObj> = bincode::deserialize(&shard.base.heap).ok()?;
    let tail: Vec<SerObj> = bincode::deserialize(&entry.heap_tail).ok()?;
    heap.extend(tail);
    let oclosure_meta: Vec<(u32, u32, Vec<u32>)> =
        bincode::deserialize(&entry.oclosure_meta).ok()?;
    let introspection_cells: Vec<(u32, fusevm::Value)> =
        bincode::deserialize(&entry.introspection_cells).ok()?;
    let builtin_cells: Vec<Option<crate::host::SymbolBaseline>> =
        bincode::deserialize(&shard.base.builtin_cells).ok()?;
    Some(CachedScript {
        chunks,
        heap,
        oclosure_meta,
        introspection_cells,
        builtin_cells,
    })
}

/// The parts of a compiled script, borrowed for the write — what
/// [`CachedScript`] hands back on the read, split along the entry/shard line.
///
/// One argument rather than six: `clippy::too_many_arguments` fails the build
/// at 8 (`-D warnings` in CI), and the pieces travel together anyway.
pub struct ScriptParts<'a> {
    pub chunks: &'a [Chunk],
    /// `arena[builtin_count, prelude_end)` — the shared base. Written once per
    /// shard; identical for every script this binary compiles.
    pub prelude_heap: &'a [SerObj],
    /// `arena[prelude_end..]` — the objects this file created.
    pub heap_tail: &'a [SerObj],
    pub oclosure_meta: &'a [(u32, u32, Vec<u32>)],
    pub introspection_cells: &'a [(u32, fusevm::Value)],
    /// Part of the shared base, like `prelude_heap`.
    pub builtin_cells: &'a [Option<crate::host::SymbolBaseline>],
}

/// Store a compiled script. Best-effort — any failure just skips caching. Takes
/// an exclusive `flock` so concurrent writers can't clobber each other's shard.
pub fn put(path: &str, mtime_ns: i64, schema_key: &str, parts: ScriptParts<'_>) {
    let ScriptParts {
        chunks,
        prelude_heap,
        heap_tail,
        oclosure_meta,
        introspection_cells,
        builtin_cells,
    } = parts;
    if !cache_enabled() {
        return;
    }
    let Ok(forms) = chunks
        .iter()
        .map(bincode::serialize)
        .collect::<Result<Vec<_>, _>>()
    else {
        return;
    };
    let Ok(tail_blob) = bincode::serialize(heap_tail) else {
        return;
    };
    let Ok(oclosure_blob) = bincode::serialize(oclosure_meta) else {
        return;
    };
    let Ok(introspection_blob) = bincode::serialize(introspection_cells) else {
        return;
    };
    let Ok(builtin_cells_blob) = bincode::serialize(builtin_cells) else {
        return;
    };
    let Ok(prelude_blob) = bincode::serialize(prelude_heap) else {
        return;
    };

    // Serialize concurrent writers: without this, two elisprs processes each
    // read the shard, insert their own entry, and the last writer wins —
    // silently dropping the other's entry.
    let _lock = acquire_lock();

    let base = BaseImage::new(prelude_blob, builtin_cells_blob);

    // A shard built under a different schema key / format is discarded wholesale:
    // its chunks reference a builtin layout that no longer exists.
    let mut shard = read_shard()
        .filter(|s| owned_header_ok(&s.header, schema_key))
        .unwrap_or_else(|| fresh_shard(schema_key, BaseImage::new(Vec::new(), Vec::new())));

    // Every entry's handles are indices into the base, so a base that is not the
    // one they were written against invalidates all of them. Under a matching
    // schema key the two are the same bytes; if they ever are not, the entries
    // go rather than being replayed onto a base they do not fit.
    if shard.base.fingerprint != base.fingerprint {
        shard.base = base;
        shard.entries.clear();
    }

    let bin_mtime = current_binary_mtime_secs().unwrap_or(0);
    shard.entries.insert(
        path.to_string(),
        Entry {
            mtime_ns,
            binary_mtime_at_cache: bin_mtime,
            cached_at_secs: now_secs(),
            forms,
            heap_tail: tail_blob,
            base_fingerprint: shard.base.fingerprint,
            oclosure_meta: oclosure_blob,
            introspection_cells: introspection_blob,
        },
    );
    if std::env::var_os("ELISPRS_CACHE_DEBUG").is_some() {
        let e = &shard.entries[path];
        eprintln!(
            "elisprs: entry bytes forms={} heap_tail={} oclosure={} intro={}; shard base={} entries={}",
            e.forms.iter().map(Vec::len).sum::<usize>(),
            e.heap_tail.len(),
            e.oclosure_meta.len(),
            e.introspection_cells.len(),
            shard.base.bytes(),
            shard.entries.len()
        );
    }
    // Bound the shard before writing it: `put` rewrites the whole file, so an
    // unbounded shard makes every later run pay for every script ever cached.
    enforce_budget(&mut shard, path, max_shard_bytes());
    shard.header.built_at_secs = now_secs() as u64;
    let _ = write_shard(&shard);
}

/// `(entry_count, total_blob_bytes)` snapshot for `--cache-stats`.
pub fn stats() -> (i64, i64) {
    let Some(shard) = read_shard() else {
        return (0, 0);
    };
    let count = shard.entries.len() as i64;
    // The shared base is counted once, because that is how it is stored — a
    // per-entry sum would report the v12 shape the split removed.
    let bytes: i64 = shard.base.bytes() as i64
        + shard
            .entries
            .values()
            .map(|e| entry_bytes(e) as i64)
            .sum::<i64>();
    (count, bytes)
}

/// Delete the shard file. Idempotent; `Ok(())` even when absent.
pub fn clear() -> std::io::Result<()> {
    let _lock = acquire_lock();
    let Some(p) = shard_path() else {
        return Ok(());
    };
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Drop entries whose source file vanished or whose mtime changed. Returns the
/// number evicted.
pub fn evict_stale() -> usize {
    let _lock = acquire_lock();
    let Some(mut shard) = read_shard() else {
        return 0;
    };
    let before = shard.entries.len();
    shard
        .entries
        .retain(|p, e| match file_mtime_ns(Path::new(p)) {
            Some(ns) => ns == e.mtime_ns,
            None => false,
        });
    let evicted = before - shard.entries.len();
    if evicted > 0 {
        let _ = write_shard(&shard);
    }
    evicted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_roundtrip_via_rkyv() {
        let mut shard = fresh_shard("v-test", BaseImage::new(vec![11, 12], vec![13]));
        shard.entries.insert(
            "/tmp/x.el".to_string(),
            Entry {
                mtime_ns: 1,
                binary_mtime_at_cache: 3,
                cached_at_secs: 4,
                forms: vec![vec![9, 9, 9]],
                heap_tail: vec![1, 2],
                base_fingerprint: 0,
                oclosure_meta: vec![3, 4],
                introspection_cells: vec![5, 6],
            },
        );
        let bytes = rkyv::to_bytes::<_, 4096>(&shard).unwrap();
        let archived = rkyv::check_archived_root::<Shard>(&bytes[..]).unwrap();
        assert!(header_ok(&archived.header, "v-test"));
        assert!(!header_ok(&archived.header, "v-other"));
        let back: Shard = archived.deserialize(&mut rkyv::Infallible).unwrap();
        assert_eq!(back.entries["/tmp/x.el"].forms, vec![vec![9, 9, 9]]);
        // The v8 side table has to survive the round trip too: it is the only
        // record of the special-form / intrinsic-macro function cells, which a
        // cache hit cannot rebuild (it skips the prelude that registers them).
        assert_eq!(back.entries["/tmp/x.el"].introspection_cells, vec![5, 6]);
        // The v13 base is shard-level: the post-prelude heap and the v12
        // builtin-prefix cells now round-trip once for the whole shard, and the
        // entry carries only the tail and the fingerprint that ties it to that
        // base. `heap` deliberately starts above `builtin_count`, so
        // `builtin_cells` is still the only record of what the prelude wrote to a
        // symbol `builtins::install` had already created.
        assert_eq!(back.base.heap, vec![11, 12]);
        assert_eq!(back.base.builtin_cells, vec![13]);
        assert_eq!(back.entries["/tmp/x.el"].heap_tail, vec![1, 2]);
        assert_eq!(back.header.magic, SHARD_MAGIC);
    }

    #[test]
    fn cache_enabled_env() {
        // Default (unset) is enabled; explicit "0" disables. Uses a distinct var
        // read so this doesn't race global cache state.
        std::env::set_var("ELISPRS_CACHE", "0");
        assert!(!cache_enabled());
        std::env::set_var("ELISPRS_CACHE", "1");
        assert!(cache_enabled());
        std::env::remove_var("ELISPRS_CACHE");
        assert!(cache_enabled());
    }

    /// Build an entry of a given payload size and age, for a path that exists
    /// on disk with a matching mtime (so only the BUDGET decides its fate, not
    /// the staleness sweep).
    fn sized_entry(dir: &Path, name: &str, bytes: usize, age: i64) -> (String, Entry) {
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        let mtime_ns = file_mtime_ns(&p).unwrap();
        (
            p.to_string_lossy().into_owned(),
            Entry {
                mtime_ns,
                binary_mtime_at_cache: 0,
                cached_at_secs: age,
                forms: vec![vec![0u8; bytes]],
                heap_tail: vec![],
                base_fingerprint: 0,
                oclosure_meta: vec![],
                introspection_cells: vec![],
            },
        )
    }

    /// The shard is rewritten whole on every `put`, so it must stay bounded.
    /// Unbounded, it reached 525 MB here and `elisp FILE` stopped terminating.
    #[test]
    fn budget_evicts_oldest_and_never_the_entry_just_written() {
        let dir = std::env::temp_dir().join(format!("elisprs-budget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut shard = fresh_shard("v-test", BaseImage::new(vec![11, 12], vec![13]));
        // 4 x 1000 bytes, ages 10 (oldest) .. 40 (newest).
        let mut paths = Vec::new();
        for (i, age) in [10, 20, 30, 40].iter().enumerate() {
            let (p, e) = sized_entry(&dir, &format!("e{i}.el"), 1000, *age);
            shard.entries.insert(p.clone(), e);
            paths.push(p);
        }
        assert_eq!(shard.entries.len(), 4);

        // Budget of 2500 fits two entries. The newest-written (`paths[0]`, the
        // one "just put") is kept even though it is the OLDEST by timestamp —
        // otherwise the run that paid for the compile caches nothing.
        enforce_budget(&mut shard, &paths[0], 2500);
        assert!(
            shard.entries.contains_key(&paths[0]),
            "the entry just written was evicted; a shard at its budget would then never serve a hit"
        );
        let total: u64 = shard.entries.values().map(entry_bytes).sum();
        assert!(total <= 2500, "budget not enforced: {total} bytes remain");
        // Of the rest, the newest survives and the oldest goes.
        assert!(shard.entries.contains_key(&paths[3]));
        assert!(!shard.entries.contains_key(&paths[1]));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An entry larger than the whole budget is still served. Evicting it would
    /// make every run recompile and re-cache the same script forever.
    #[test]
    fn a_single_oversized_entry_survives_its_own_budget() {
        let dir = std::env::temp_dir().join(format!("elisprs-budget1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut shard = fresh_shard("v-test", BaseImage::new(vec![11, 12], vec![13]));
        let (p, e) = sized_entry(&dir, "big.el", 10_000, 1);
        shard.entries.insert(p.clone(), e);
        enforce_budget(&mut shard, &p, 100);
        assert_eq!(shard.entries.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `enforce_budget` also drops entries that can never be served again —
    /// source deleted, or edited since it was cached — but never the entry just
    /// written, whose file it must not have to stat as a live path.
    #[test]
    fn budget_sweep_drops_unservable_entries() {
        let dir = std::env::temp_dir().join(format!("elisprs-budget2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut shard = fresh_shard("v-test", BaseImage::new(vec![11, 12], vec![13]));
        let (live, e1) = sized_entry(&dir, "live.el", 10, 5);
        let (gone, e2) = sized_entry(&dir, "gone.el", 10, 5);
        let (edited, mut e3) = sized_entry(&dir, "edited.el", 10, 5);
        e3.mtime_ns += 1; // simulate an edit after caching
        shard.entries.insert(live.clone(), e1);
        shard.entries.insert(gone.clone(), e2);
        shard.entries.insert(edited.clone(), e3);
        std::fs::remove_file(&gone).unwrap();

        // Budget of 0 = no byte cap, so ONLY the unservable sweep can act here.
        enforce_budget(&mut shard, &live, 0);
        assert!(shard.entries.contains_key(&live));
        assert!(
            !shard.entries.contains_key(&gone),
            "deleted source retained"
        );
        assert!(
            !shard.entries.contains_key(&edited),
            "entry whose source changed since caching was retained"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn max_shard_bytes_env_override() {
        std::env::set_var("ELISPRS_CACHE_MAX_BYTES", "4096");
        assert_eq!(max_shard_bytes(), 4096);
        // 0 is a real value: no budget.
        std::env::set_var("ELISPRS_CACHE_MAX_BYTES", "0");
        assert_eq!(max_shard_bytes(), 0);
        // Garbage falls back to the default rather than to "unbounded".
        std::env::set_var("ELISPRS_CACHE_MAX_BYTES", "not-a-number");
        assert_eq!(max_shard_bytes(), DEFAULT_MAX_SHARD_BYTES);
        std::env::remove_var("ELISPRS_CACHE_MAX_BYTES");
        assert_eq!(max_shard_bytes(), DEFAULT_MAX_SHARD_BYTES);
    }
}
