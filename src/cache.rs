//! rkyv-backed bytecode cache for elisp scripts (mirrors zshrs `script_cache`).
//!
//! Single-file shard at `~/.elisprs/scripts.rkyv`. On the 2nd+ run of an
//! unchanged file, elisprs skips reading / macro-expanding / lowering AND the
//! prelude rebuild: it deserializes the per-form `fusevm::Chunk`s + a clean heap
//! image and runs them directly.
//!
//! Layout: the *outer* container is a zero-copy rkyv archive (validated via
//! `check_archived_root`); the *inner* per-form `Chunk` blobs and heap image are
//! bincode, because `fusevm::Chunk`/`Value` are serde-owned, not `rkyv::Archive`
//! (the same split zshrs uses). Keyed by absolute path + mtime + elisprs version.

use crate::host::SerObj;
use fusevm::Chunk;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct Shard {
    version: String,
    entries: HashMap<String, Entry>,
}

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct Entry {
    mtime_ns: i64,
    /// bincode `fusevm::Chunk`, one per top-level form.
    forms: Vec<Vec<u8>>,
    /// bincode `Vec<SerObj>` — the clean (pre-user-run) heap image.
    heap: Vec<u8>,
}

fn elisprs_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".elisprs"))
}
fn shard_path() -> Option<PathBuf> {
    Some(elisprs_dir()?.join("scripts.rkyv"))
}

fn read_shard() -> Option<Shard> {
    let bytes = std::fs::read(shard_path()?).ok()?;
    let archived = rkyv::check_archived_root::<Shard>(&bytes).ok()?;
    archived.deserialize(&mut rkyv::Infallible).ok()
}

fn write_shard(shard: &Shard) -> std::io::Result<()> {
    let dir = elisprs_dir().ok_or_else(|| std::io::Error::other("no HOME"))?;
    std::fs::create_dir_all(&dir)?;
    let bytes = rkyv::to_bytes::<_, 4096>(shard)
        .map_err(|e| std::io::Error::other(format!("rkyv: {e:?}")))?;
    let tmp = dir.join(format!("scripts.rkyv.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, dir.join("scripts.rkyv"))
}

/// Cache lookup. Returns the per-form chunks + clean heap image on a fresh hit.
pub fn get(path: &str, mtime_ns: i64) -> Option<(Vec<Chunk>, Vec<SerObj>)> {
    let shard = read_shard()?;
    if shard.version != env!("CARGO_PKG_VERSION") {
        return None;
    }
    let entry = shard.entries.get(path)?;
    if entry.mtime_ns != mtime_ns {
        return None;
    }
    let chunks: Vec<Chunk> = entry
        .forms
        .iter()
        .map(|b| bincode::deserialize(b))
        .collect::<Result<_, _>>()
        .ok()?;
    let heap: Vec<SerObj> = bincode::deserialize(&entry.heap).ok()?;
    Some((chunks, heap))
}

/// Store a compiled script. Best-effort — any failure just skips caching.
pub fn put(path: &str, mtime_ns: i64, chunks: &[Chunk], heap: &[SerObj]) {
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
    let mut shard = read_shard()
        .filter(|s| s.version == env!("CARGO_PKG_VERSION"))
        .unwrap_or_else(|| Shard {
            version: env!("CARGO_PKG_VERSION").to_string(),
            entries: HashMap::new(),
        });
    shard.entries.insert(
        path.to_string(),
        Entry {
            mtime_ns,
            forms,
            heap: heap_blob,
        },
    );
    let _ = write_shard(&shard);
}
