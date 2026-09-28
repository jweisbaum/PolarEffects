//! The on-disk chunk cache (spec.md 3.4, 13).
//!
//! A reanalysis chunk is a few megabytes of compressed bytes, and a race
//! needs hundreds of them. Two boats of the same race need the *same*
//! chunks, so the raw bytes of every chunk read are kept on disk, under the
//! folder the settings name, until the size limit evicts the least recently
//! used ones.
//!
//! The cache holds **compressed chunk bytes exactly as the archive served
//! them**, never decoded values, so what is read from it goes through the
//! same codec path as a fresh download. Metadata (`.zarray`, `.zattrs`, …) is
//! not cached: it is small, and ARCO-ERA5's root attributes change as ECMWF
//! publishes hours, so a stale copy would misstate coverage.
//!
//! Deleting the folder is always lossless (invariant 3): every sample a
//! project needs is saved in the project.
//!
//! Layout: `<root>/<namespace>/<store key>`, where the namespace names the
//! dataset (e.g. `wb2-era5-1h`) and the store key is the Zarr key
//! (`10m_u_component_of_wind/539724.0.0`). Recency survives restarts through
//! file modification times, which a hit refreshes.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use zarrs_storage::byte_range::ByteRangeIterator;
use zarrs_storage::{
    Bytes, MaybeBytes, MaybeBytesIterator, ReadableStorageTraits, StorageError, StoreKey,
};

use crate::error::{EnvError, Result};

/// Suffix of a file being written; never read, and removed on open.
const PARTIAL: &str = "partial";

/// Counters for how the cache has been used.
#[derive(Debug, Default)]
pub struct CacheStats {
    /// Reads answered from disk.
    pub hits: AtomicU64,
    /// Reads that had to go to the archive.
    pub misses: AtomicU64,
    /// Bytes answered from disk.
    pub hit_bytes: AtomicU64,
    /// Files removed to stay under the limit.
    pub evictions: AtomicU64,
}

impl CacheStats {
    /// `(hits, misses, hit_bytes, evictions)` at this moment.
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.hit_bytes.load(Ordering::Relaxed),
            self.evictions.load(Ordering::Relaxed),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    size: u64,
    /// Logical clock of the last use; lower is older.
    used: u64,
}

#[derive(Debug, Default)]
struct Index {
    /// Ordered, so eviction ties break the same way every run.
    entries: BTreeMap<PathBuf, Entry>,
    total: u64,
    clock: u64,
}

impl Index {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

/// A size-limited, least-recently-used cache of chunk bytes on disk.
#[derive(Debug)]
pub struct ChunkCache {
    root: PathBuf,
    limit: u64,
    index: Mutex<Index>,
    stats: CacheStats,
}

/// Every regular file under `dir`, with its size and modification time.
fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, out);
        } else if kind.is_file() {
            if path.extension().is_some_and(|ext| ext == PARTIAL) {
                // Left by a write that never finished.
                let _ = std::fs::remove_file(&path);
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                out.push((path, meta.len(), modified));
            }
        }
    }
}

/// A relative path for `namespace/key`, refusing anything that could climb
/// out of the cache folder.
fn relative(namespace: &str, key: &str) -> Option<PathBuf> {
    let path = PathBuf::from(namespace).join(key);
    path.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then_some(path)
}

impl ChunkCache {
    /// Opens (creating if needed) the cache at `root`, holding at most
    /// `limit_bytes`. Existing files count against the limit, oldest first
    /// to go.
    ///
    /// # Errors
    /// [`EnvError::Cache`] if the folder cannot be created.
    pub fn open(root: impl Into<PathBuf>, limit_bytes: u64) -> Result<Arc<Self>> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .map_err(|e| EnvError::Cache(format!("cannot create {}: {e}", root.display())))?;
        let mut files = Vec::new();
        walk(&root, &mut files);
        // Oldest first, then by path, so the logical clock reproduces the
        // on-disk recency order.
        files.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
        let mut index = Index::default();
        for (path, size, _) in files {
            let Ok(rel) = path.strip_prefix(&root) else {
                continue;
            };
            let used = index.tick();
            index.total += size;
            index
                .entries
                .insert(rel.to_path_buf(), Entry { size, used });
        }
        let cache = Self {
            root,
            limit: limit_bytes,
            index: Mutex::new(index),
            stats: CacheStats::default(),
        };
        cache.evict_to(cache.limit)?;
        Ok(Arc::new(cache))
    }

    /// The folder the cache writes into.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Bytes currently held.
    pub fn size(&self) -> u64 {
        self.index.lock().map_or(0, |index| index.total)
    }

    /// The usage counters.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Index>> {
        self.index
            .lock()
            .map_err(|_| EnvError::Cache("the cache index lock was poisoned".to_owned()))
    }

    /// The bytes stored for `namespace/key`, refreshing its recency.
    pub fn get(&self, namespace: &str, key: &str) -> Option<Bytes> {
        let rel = relative(namespace, key)?;
        let path = self.root.join(&rel);
        let known = self.lock().ok()?.entries.contains_key(&rel);
        if !known {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            // Removed behind our back (a Clear, or the user). Forget it.
            if let Ok(mut index) = self.lock()
                && let Some(entry) = index.entries.remove(&rel)
            {
                index.total = index.total.saturating_sub(entry.size);
            }
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        if let Ok(mut index) = self.lock() {
            let used = index.tick();
            if let Some(entry) = index.entries.get_mut(&rel) {
                entry.used = used;
            }
        }
        // Recency for the next launch; failing to record it only makes this
        // file look older than it is.
        if let Ok(file) = std::fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        self.stats.hits.fetch_add(1, Ordering::Relaxed);
        self.stats
            .hit_bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Some(Bytes::from(bytes))
    }

    /// Stores `bytes` for `namespace/key`, evicting the least recently used
    /// files until the total is within the limit. A value larger than the
    /// whole limit is not stored.
    ///
    /// # Errors
    /// [`EnvError::Cache`] if the key is not a safe relative path or the
    /// file cannot be written.
    pub fn put(&self, namespace: &str, key: &str, bytes: &[u8]) -> Result<()> {
        let rel = relative(namespace, key)
            .ok_or_else(|| EnvError::Cache(format!("unsafe cache key {namespace}/{key}")))?;
        let size = bytes.len() as u64;
        if size > self.limit {
            return Ok(());
        }
        let path = self.root.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EnvError::Cache(format!("cannot create {}: {e}", parent.display())))?;
        }
        // Written beside, then renamed: a reader never sees half a chunk,
        // and two writers of the same chunk each leave a whole one.
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let partial = path.with_extension(format!(
            "{}-{}.{PARTIAL}",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&partial, bytes)
            .and_then(|()| std::fs::rename(&partial, &path))
            .map_err(|e| {
                let _ = std::fs::remove_file(&partial);
                EnvError::Cache(format!("cannot write {}: {e}", path.display()))
            })?;
        {
            let mut index = self.lock()?;
            let used = index.tick();
            if let Some(old) = index.entries.insert(rel, Entry { size, used }) {
                index.total = index.total.saturating_sub(old.size);
            }
            index.total += size;
        }
        self.evict_to(self.limit)
    }

    /// Removes least recently used files until at most `limit` bytes remain.
    fn evict_to(&self, limit: u64) -> Result<()> {
        let mut index = self.lock()?;
        while index.total > limit {
            let Some((oldest, entry)) = index
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(p, e)| (p.clone(), *e))
            else {
                break;
            };
            let _ = std::fs::remove_file(self.root.join(&oldest));
            index.entries.remove(&oldest);
            index.total = index.total.saturating_sub(entry.size);
            self.stats.evictions.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// Whether a store key names a chunk (cached) rather than metadata (not).
/// Zarr v2 metadata keys all end in a dot-file: `.zarray`, `.zattrs`,
/// `.zgroup`, `.zmetadata`.
fn is_chunk(key: &StoreKey) -> bool {
    key.as_str()
        .rsplit('/')
        .next()
        .is_some_and(|last| !last.is_empty() && !last.starts_with('.'))
}

/// A store that answers chunk reads from a [`ChunkCache`] before asking the
/// store it wraps.
#[derive(Debug)]
pub struct CachedStore<S> {
    inner: S,
    cache: Arc<ChunkCache>,
    namespace: String,
}

impl<S> CachedStore<S> {
    /// Wraps `inner`, keeping its chunks under `namespace` in `cache`.
    pub fn new(inner: S, cache: Arc<ChunkCache>, namespace: impl Into<String>) -> Self {
        Self {
            inner,
            cache,
            namespace: namespace.into(),
        }
    }
}

impl<S: ReadableStorageTraits> ReadableStorageTraits for CachedStore<S> {
    fn get(&self, key: &StoreKey) -> std::result::Result<MaybeBytes, StorageError> {
        if !is_chunk(key) {
            return self.inner.get(key);
        }
        if let Some(bytes) = self.cache.get(&self.namespace, key.as_str()) {
            return Ok(Some(bytes));
        }
        let fetched = self.inner.get(key)?;
        if let Some(bytes) = &fetched {
            // A full disk costs the cache, not the read.
            let _ = self.cache.put(&self.namespace, key.as_str(), bytes);
        }
        Ok(fetched)
    }

    /// Chunks are read whole (every store here has one blosc container per
    /// chunk, which cannot be partially decoded), so a partial read of a
    /// chunk is served from the whole cached value.
    fn get_partial_many<'a>(
        &'a self,
        key: &StoreKey,
        byte_ranges: ByteRangeIterator<'a>,
    ) -> std::result::Result<MaybeBytesIterator<'a>, StorageError> {
        if !is_chunk(key) {
            return self.inner.get_partial_many(key, byte_ranges);
        }
        let Some(whole) = self.get(key)? else {
            return Ok(None);
        };
        let size = whole.len() as u64;
        let mut out: Vec<std::result::Result<Bytes, StorageError>> = Vec::new();
        for range in byte_ranges {
            let (start, end) = (range.start(size) as usize, range.end(size) as usize);
            if start > end || end > whole.len() {
                return Err(StorageError::Other(format!(
                    "range {start}..{end} is outside the {size}-byte chunk {}",
                    key.as_str()
                )));
            }
            out.push(Ok(whole.slice(start..end)));
        }
        Ok(Some(Box::new(out.into_iter())))
    }

    fn size_key(&self, key: &StoreKey) -> std::result::Result<Option<u64>, StorageError> {
        if is_chunk(key)
            && let Some(bytes) = self.cache.get(&self.namespace, key.as_str())
        {
            return Ok(Some(bytes.len() as u64));
        }
        self.inner.size_key(key)
    }

    fn supports_get_partial(&self) -> bool {
        self.inner.supports_get_partial()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pe-cache-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A store that counts how often it is asked.
    #[derive(Debug, Default)]
    struct Counting {
        gets: AtomicUsize,
    }

    impl ReadableStorageTraits for Counting {
        fn get(&self, key: &StoreKey) -> std::result::Result<MaybeBytes, StorageError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            if key.as_str().ends_with("missing") {
                return Ok(None);
            }
            Ok(Some(Bytes::from(key.as_str().as_bytes().to_vec())))
        }
        fn get_partial_many<'a>(
            &'a self,
            _key: &StoreKey,
            _byte_ranges: ByteRangeIterator<'a>,
        ) -> std::result::Result<MaybeBytesIterator<'a>, StorageError> {
            Err(StorageError::Other("not used".to_owned()))
        }
        fn size_key(&self, _key: &StoreKey) -> std::result::Result<Option<u64>, StorageError> {
            Ok(None)
        }
        fn supports_get_partial(&self) -> bool {
            false
        }
    }

    fn key(k: &str) -> StoreKey {
        StoreKey::new(k).expect("a key")
    }

    #[test]
    fn a_second_read_of_a_chunk_comes_from_disk() {
        let dir = temp("hit");
        let cache = ChunkCache::open(&dir, 1 << 20).expect("opens");
        let store = CachedStore::new(Counting::default(), Arc::clone(&cache), "ds");
        let k = key("u/1.0.0");
        let first = store.get(&k).expect("reads").expect("present");
        let second = store.get(&k).expect("reads").expect("present");
        assert_eq!(first, second);
        assert_eq!(store.inner.gets.load(Ordering::SeqCst), 1);
        assert_eq!(cache.stats().snapshot().0, 1, "one hit");
        assert!(dir.join("ds/u/1.0.0").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn metadata_and_missing_chunks_are_not_cached() {
        let dir = temp("meta");
        let cache = ChunkCache::open(&dir, 1 << 20).expect("opens");
        let store = CachedStore::new(Counting::default(), Arc::clone(&cache), "ds");
        for _ in 0..2 {
            store.get(&key("u/.zarray")).expect("reads");
            assert!(store.get(&key("u/9.missing")).expect("reads").is_none());
        }
        assert_eq!(store.inner.gets.load(Ordering::SeqCst), 4);
        assert_eq!(cache.size(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_least_recently_used_chunk_is_evicted_first() {
        let dir = temp("lru");
        let cache = ChunkCache::open(&dir, 30).expect("opens");
        cache.put("ds", "a", &[1; 10]).expect("put");
        cache.put("ds", "b", &[2; 10]).expect("put");
        cache.put("ds", "c", &[3; 10]).expect("put");
        // Using `a` makes `b` the oldest.
        assert!(cache.get("ds", "a").is_some());
        cache.put("ds", "d", &[4; 10]).expect("put");
        assert_eq!(cache.size(), 30);
        assert!(cache.get("ds", "b").is_none(), "b was evicted");
        for kept in ["a", "c", "d"] {
            assert!(cache.get("ds", kept).is_some(), "{kept} kept");
        }
        assert!(!dir.join("ds/b").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reopened_cache_counts_what_is_on_disk_and_enforces_a_smaller_limit() {
        let dir = temp("reopen");
        {
            let cache = ChunkCache::open(&dir, 100).expect("opens");
            cache.put("ds", "x/0", &[0; 40]).expect("put");
            cache.put("ds", "x/1", &[0; 40]).expect("put");
        }
        let cache = ChunkCache::open(&dir, 100).expect("reopens");
        assert_eq!(cache.size(), 80);
        assert!(cache.get("ds", "x/1").is_some());
        let smaller = ChunkCache::open(&dir, 50).expect("reopens smaller");
        assert!(smaller.size() <= 50);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_value_bigger_than_the_limit_is_not_kept() {
        let dir = temp("big");
        let cache = ChunkCache::open(&dir, 8).expect("opens");
        cache.put("ds", "huge", &[0; 9]).expect("skipped quietly");
        assert_eq!(cache.size(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_cannot_climb_out_of_the_cache() {
        let dir = temp("climb");
        let cache = ChunkCache::open(&dir, 100).expect("opens");
        assert!(cache.put("ds", "../escape", &[0]).is_err());
        assert!(cache.put("..", "x", &[0]).is_err());
        assert!(cache.put("ds", "/abs", &[0]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_removed_behind_the_cache_is_a_miss() {
        let dir = temp("gone");
        let cache = ChunkCache::open(&dir, 100).expect("opens");
        cache.put("ds", "k", &[7; 5]).expect("put");
        std::fs::remove_dir_all(&dir).expect("cleared by the user");
        assert!(cache.get("ds", "k").is_none());
        assert_eq!(cache.size(), 0);
    }
}
