//! Ariel OS adapters for the portable Matter subsystem.
//!
//! The synchronous cache bridges `rs-matter` persistence calls to an Ariel OS
//! async flash task. Cache changes are not acknowledged as durable until the
//! flush task completes its backend barrier.

#![no_std]
#![forbid(unsafe_code)]
#![allow(async_fn_in_trait)]

use ariel_os_matter_core::storage::{
    StorageDomain, StorageKey, SyncBlobStore, SyncSecureStore,
};
use ariel_os_matter_core::{Error, Result};
use heapless::{Vec, Vec as ByteVec};

/// Maximum cached value size. The value matches the default `rs-matter` KV
/// scratch buffer and can be changed through a type parameter in a future
/// format migration.
pub const MAX_CACHED_VALUE: usize = 4096;

/// Asynchronous Ariel storage backend used by the cache flush task.
pub trait ArielStorageBackend {
    /// Load one normal record during startup.
    async fn load(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically write one normal record.
    async fn store(&mut self, key: StorageKey, value: &[u8]) -> Result<()>;
    /// Remove one normal record.
    async fn remove(&mut self, key: StorageKey) -> Result<()>;
    /// Flush media write caches.
    async fn sync(&mut self) -> Result<()>;
}

/// Cached record state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheState {
    Clean,
    Dirty,
    Removed,
}

/// One synchronous cache entry.
struct CacheEntry {
    key: StorageKey,
    value: ByteVec<u8, MAX_CACHED_VALUE>,
    state: CacheState,
}

/// Fixed-capacity, synchronous persistence cache.
pub struct ArielKvCache<const N: usize> {
    entries: Vec<CacheEntry, N>,
}

impl<const N: usize> ArielKvCache<N> {
    /// Create an empty cache.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Preload one committed record before `rs-matter` startup.
    pub fn preload(&mut self, key: StorageKey, value: &[u8]) -> Result<()> {
        if self.entries.iter().any(|entry| entry.key == key) {
            return Err(Error::Conflict);
        }
        let mut stored = ByteVec::new();
        stored
            .extend_from_slice(value)
            .map_err(|_| Error::Capacity)?;
        self.entries
            .push(CacheEntry {
                key,
                value: stored,
                state: CacheState::Clean,
            })
            .map_err(|_| Error::Capacity)
    }

    /// Flush every dirty cache entry and then issue a durable backend barrier.
    ///
    /// Entries become clean only after their own write succeeds. If the final
    /// barrier fails, all written entries become dirty again so a later flush
    /// retries them.
    pub async fn flush<B: ArielStorageBackend>(&mut self, backend: &mut B) -> Result<()> {
        let mut written_indices: Vec<usize, N> = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            match entry.state {
                CacheState::Clean => {}
                CacheState::Dirty => {
                    backend.store(entry.key, entry.value.as_slice()).await?;
                    entry.state = CacheState::Clean;
                    written_indices.push(index).map_err(|_| Error::Capacity)?;
                }
                CacheState::Removed => {
                    backend.remove(entry.key).await?;
                    entry.state = CacheState::Clean;
                    entry.value.clear();
                    written_indices.push(index).map_err(|_| Error::Capacity)?;
                }
            }
        }

        if backend.sync().await.is_err() {
            for index in written_indices {
                if let Some(entry) = self.entries.get_mut(index) {
                    entry.state = if entry.value.is_empty() {
                        CacheState::Removed
                    } else {
                        CacheState::Dirty
                    };
                }
            }
            return Err(Error::Storage);
        }

        self.entries
            .retain(|entry| !(entry.state == CacheState::Clean && entry.value.is_empty()));
        Ok(())
    }

    /// Return true when persistence work is pending.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.state != CacheState::Clean)
    }

    fn entry_mut(&mut self, key: StorageKey) -> Option<&mut CacheEntry> {
        self.entries.iter_mut().find(|entry| entry.key == key)
    }
}

impl<const N: usize> Default for ArielKvCache<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SyncBlobStore for ArielKvCache<N> {
    fn load(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>> {
        let entry = match self.entries.iter().find(|entry| entry.key == key) {
            Some(entry) if entry.state != CacheState::Removed => entry,
            Some(_) | None => return Ok(None),
        };
        if output.len() < entry.value.len() {
            return Err(Error::Capacity);
        }
        output[..entry.value.len()].copy_from_slice(entry.value.as_slice());
        Ok(Some(entry.value.len()))
    }

    fn store(&mut self, key: StorageKey, data: &[u8]) -> Result<()> {
        if let Some(entry) = self.entry_mut(key) {
            entry.value.clear();
            entry
                .value
                .extend_from_slice(data)
                .map_err(|_| Error::Capacity)?;
            entry.state = CacheState::Dirty;
            return Ok(());
        }
        let mut value = ByteVec::new();
        value
            .extend_from_slice(data)
            .map_err(|_| Error::Capacity)?;
        self.entries
            .push(CacheEntry {
                key,
                value,
                state: CacheState::Dirty,
            })
            .map_err(|_| Error::Capacity)
    }

    fn remove(&mut self, key: StorageKey) -> Result<()> {
        if let Some(entry) = self.entry_mut(key) {
            entry.value.clear();
            entry.state = CacheState::Removed;
            return Ok(());
        }
        self.entries
            .push(CacheEntry {
                key,
                value: ByteVec::new(),
                state: CacheState::Removed,
            })
            .map_err(|_| Error::Capacity)
    }

    fn sync(&mut self) -> Result<()> {
        if self.is_dirty() {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }

    fn erase_domain(&mut self, domain: StorageDomain) -> Result<()> {
        for entry in &mut self.entries {
            if entry.key.domain == domain {
                entry.value.clear();
                entry.state = CacheState::Removed;
            }
        }
        Ok(())
    }
}

/// Secure backend with synchronous access. Production ESP32-S3 adapters bind
/// this trait to hardware key slots or authenticated encrypted storage.
pub trait ArielSecureBackend {
    /// Load protected data.
    fn load(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically store protected data.
    fn store(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Remove protected data.
    fn remove(&mut self, key: StorageKey) -> Result<()>;
    /// Issue a secure-media durability barrier.
    fn sync(&mut self) -> Result<()>;
    /// Erase all Matter secrets and hardware key handles.
    fn erase_all(&mut self) -> Result<()>;
}

/// Adapter from an Ariel secure backend to the portable secure-store trait.
pub struct ArielSecureStore<B> {
    backend: B,
}

impl<B> ArielSecureStore<B> {
    /// Create a secure-store adapter.
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Consume the adapter.
    #[must_use]
    pub fn into_inner(self) -> B {
        self.backend
    }
}

impl<B: ArielSecureBackend> SyncSecureStore for ArielSecureStore<B> {
    fn load_secret(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>> {
        self.backend.load(key, output)
    }

    fn store_secret(&mut self, key: StorageKey, data: &[u8]) -> Result<()> {
        self.backend.store(key, data)
    }

    fn remove_secret(&mut self, key: StorageKey) -> Result<()> {
        self.backend.remove(key)
    }

    fn sync_secure(&mut self) -> Result<()> {
        self.backend.sync()
    }

    fn erase_all_secrets(&mut self) -> Result<()> {
        self.backend.erase_all()
    }
}

/// Ariel OS monotonic clock adapter.
#[cfg(feature = "ariel")]
pub struct ArielClock;

#[cfg(feature = "ariel")]
impl ariel_os_matter_core::platform::Clock for ArielClock {
    fn now_ms(&self) -> u64 {
        ariel_os::time::Instant::now().as_millis()
    }

    async fn sleep_ms(&self, duration_ms: u64) {
        ariel_os::time::Timer::after_millis(duration_ms).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_cache_requires_async_flush_before_sync_succeeds() {
        let mut cache = ArielKvCache::<4>::new();
        let key = StorageKey::new(StorageDomain::Device, 1);
        assert_eq!(cache.store(key, b"state"), Ok(()));
        assert_eq!(cache.sync(), Err(Error::Busy));
        let mut output = [0_u8; 8];
        assert_eq!(cache.load(key, &mut output), Ok(Some(5)));
        assert_eq!(&output[..5], b"state");
    }
}
