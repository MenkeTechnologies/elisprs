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
pub const SHARD_FORMAT_VERSION: u32 = 4;

/// The cache schema key: elisprs version + a builtin/prelude fingerprint. A
/// shard built under a different key is ignored (and overwritten on the next
/// `put`), so editing `builtins::install` or the prelude never serves a stale
/// chunk.
pub fn schema_key(builtin_fingerprint: u64) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    builtin_fingerprint.hash(&mut hasher);
    crate::prelude::PRELUDE.hash(&mut hasher);
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
    /// bincode `Vec<SerObj>` — the clean (pre-user-run) heap image.
    heap: Vec<u8>,
    /// bincode `Vec<(u32, u32, Vec<u32>)>` — the OClosure side table
    /// (`closure-handle, type, slots`). Not derivable from `heap`: it is built
    /// when the prelude runs, which a cache hit skips.
    oclosure_meta: Vec<u8>,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct Shard {
    header: ShardHeader,
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

fn fresh_shard(schema_key: &str) -> Shard {
    Shard {
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
/// `schema_key` must match the key the entry was written under (see `schema_key`).
/// Misses on: cache disabled, format/schema drift, mtime mismatch, or a binary
/// newer than the cached entry.
#[allow(clippy::type_complexity)]
pub fn get(
    path: &str,
    mtime_ns: i64,
    schema_key: &str,
) -> Option<(Vec<Chunk>, Vec<SerObj>, Vec<(u32, u32, Vec<u32>)>)> {
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
    let heap: Vec<SerObj> = bincode::deserialize(&entry.heap).ok()?;
    let oclosure_meta: Vec<(u32, u32, Vec<u32>)> =
        bincode::deserialize(&entry.oclosure_meta).ok()?;
    Some((chunks, heap, oclosure_meta))
}

/// Store a compiled script. Best-effort — any failure just skips caching. Takes
/// an exclusive `flock` so concurrent writers can't clobber each other's shard.
pub fn put(
    path: &str,
    mtime_ns: i64,
    schema_key: &str,
    chunks: &[Chunk],
    heap: &[SerObj],
    oclosure_meta: &[(u32, u32, Vec<u32>)],
) {
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
    let Ok(heap_blob) = bincode::serialize(heap) else {
        return;
    };
    let Ok(oclosure_blob) = bincode::serialize(oclosure_meta) else {
        return;
    };

    // Serialize concurrent writers: without this, two elisprs processes each
    // read the shard, insert their own entry, and the last writer wins —
    // silently dropping the other's entry.
    let _lock = acquire_lock();

    // A shard built under a different schema key / format is discarded wholesale:
    // its chunks reference a builtin layout that no longer exists.
    let mut shard = read_shard()
        .filter(|s| owned_header_ok(&s.header, schema_key))
        .unwrap_or_else(|| fresh_shard(schema_key));

    let bin_mtime = current_binary_mtime_secs().unwrap_or(0);
    shard.entries.insert(
        path.to_string(),
        Entry {
            mtime_ns,
            binary_mtime_at_cache: bin_mtime,
            cached_at_secs: now_secs(),
            forms,
            heap: heap_blob,
            oclosure_meta: oclosure_blob,
        },
    );
    shard.header.built_at_secs = now_secs() as u64;
    let _ = write_shard(&shard);
}

/// `(entry_count, total_blob_bytes)` snapshot for `--cache-stats`.
pub fn stats() -> (i64, i64) {
    let Some(shard) = read_shard() else {
        return (0, 0);
    };
    let count = shard.entries.len() as i64;
    let bytes: i64 = shard
        .entries
        .values()
        .map(|e| (e.forms.iter().map(|f| f.len()).sum::<usize>() + e.heap.len()) as i64)
        .sum();
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
        let mut shard = fresh_shard("v-test");
        shard.entries.insert(
            "/tmp/x.el".to_string(),
            Entry {
                mtime_ns: 1,
                binary_mtime_at_cache: 3,
                cached_at_secs: 4,
                forms: vec![vec![9, 9, 9]],
                heap: vec![1, 2],
                oclosure_meta: vec![3, 4],
            },
        );
        let bytes = rkyv::to_bytes::<_, 4096>(&shard).unwrap();
        let archived = rkyv::check_archived_root::<Shard>(&bytes[..]).unwrap();
        assert!(header_ok(&archived.header, "v-test"));
        assert!(!header_ok(&archived.header, "v-other"));
        let back: Shard = archived.deserialize(&mut rkyv::Infallible).unwrap();
        assert_eq!(back.entries["/tmp/x.el"].forms, vec![vec![9, 9, 9]]);
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
}
