//! `rs-matter` isolation boundary for Ariel OS.
//!
//! Applications depend on `ariel-os-matter-core`. This crate converts typed
//! endpoint metadata and platform state to the protocol engine. It does not
//! expose raw Matter TLV in its public API.

#![no_std]
#![forbid(unsafe_code)]

use core::ops::RangeInclusive;

use ariel_os_matter_core::devices::{DeviceTypeDescriptor, DeviceTypeMarker};
use ariel_os_matter_core::storage::{StorageDomain, StorageKey, SyncBlobStore, SyncSecureStore};
use ariel_os_matter_core::{Error, Result};
use heapless::Vec;

/// The pinned Matter specification version, encoded as `0xMMmmppbb`.
pub const MATTER_SPEC_VERSION: u32 =
    rs_matter::dm::clusters::basic_info::DEFAULT_MATTER_SPEC_VERSION;
/// Matter 1.6 encoded version.
pub const REQUIRED_MATTER_SPEC_VERSION: u32 = 0x0106_0000;

const _: () = assert!(MATTER_SPEC_VERSION == REQUIRED_MATTER_SPEC_VERSION);

/// Adapter-owned protocol device-type value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolDeviceType {
    /// Device-type identifier.
    pub id: u32,
    /// Device Library revision.
    pub revision: u16,
}

impl From<DeviceTypeDescriptor> for ProtocolDeviceType {
    fn from(value: DeviceTypeDescriptor) -> Self {
        Self {
            id: value.id,
            revision: value.revision,
        }
    }
}

impl ProtocolDeviceType {
    /// Convert to the internal `rs-matter` metadata type.
    #[must_use]
    pub const fn into_internal(self) -> rs_matter::dm::DeviceType {
        rs_matter::dm::DeviceType {
            dtype: self.id,
            drev: self.revision,
        }
    }
}

/// Return the internal metadata for a typed Ariel OS device.
#[must_use]
pub const fn internal_device_type<T: DeviceTypeMarker>() -> rs_matter::dm::DeviceType {
    ProtocolDeviceType {
        id: T::DESCRIPTOR.id,
        revision: T::DESCRIPTOR.revision,
    }
    .into_internal()
}

/// Maximum secret key ranges in the protocol persistence policy.
pub const MAX_SECRET_KEY_RANGES: usize = 32;

/// Classification of an `rs-matter` persistence key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceClass {
    /// Normal integrity-protected storage.
    Normal,
    /// Encrypted or hardware-protected secret storage.
    Secret,
}

/// Explicit key-classification policy.
///
/// The adapter uses a deny-by-default policy for unknown protocol keys. The
/// caller must register normal ranges. Any key not registered as normal is
/// stored in the secure backend. This prevents a new upstream secret key from
/// silently moving to normal flash after an `rs-matter` update.
pub struct PersistencePolicy {
    normal_ranges: Vec<RangeInclusive<u16>, MAX_SECRET_KEY_RANGES>,
}

impl PersistencePolicy {
    /// Create a fail-closed policy with no normal keys.
    #[must_use]
    pub const fn secure_by_default() -> Self {
        Self {
            normal_ranges: Vec::new(),
        }
    }

    /// Mark an inclusive key range as normal data.
    pub fn allow_normal(&mut self, range: RangeInclusive<u16>) -> Result<()> {
        if range.is_empty() {
            return Err(Error::InvalidArgument);
        }
        self.normal_ranges.push(range).map_err(|_| Error::Capacity)
    }

    /// Classify a key.
    #[must_use]
    pub fn classify(&self, key: u16) -> PersistenceClass {
        if self.normal_ranges.iter().any(|range| range.contains(&key)) {
            PersistenceClass::Normal
        } else {
            PersistenceClass::Secret
        }
    }
}

impl Default for PersistencePolicy {
    fn default() -> Self {
        Self::secure_by_default()
    }
}

/// Split protocol persistence across normal and secure stores.
pub struct SplitPersistence<N, S> {
    normal: N,
    secure: S,
    policy: PersistencePolicy,
}

impl<N, S> SplitPersistence<N, S>
where
    N: SyncBlobStore,
    S: SyncSecureStore,
{
    /// Create a split store.
    pub const fn new(normal: N, secure: S, policy: PersistencePolicy) -> Self {
        Self {
            normal,
            secure,
            policy,
        }
    }

    /// Load one protocol key.
    pub fn load(&mut self, key: u16, output: &mut [u8]) -> Result<Option<usize>> {
        let storage_key = map_key(key);
        match self.policy.classify(key) {
            PersistenceClass::Normal => self.normal.load(storage_key, output),
            PersistenceClass::Secret => self.secure.load_secret(storage_key, output),
        }
    }

    /// Store one protocol key.
    pub fn store(&mut self, key: u16, data: &[u8]) -> Result<()> {
        let storage_key = map_key(key);
        match self.policy.classify(key) {
            PersistenceClass::Normal => self.normal.store(storage_key, data),
            PersistenceClass::Secret => self.secure.store_secret(storage_key, data),
        }
    }

    /// Remove one protocol key.
    pub fn remove(&mut self, key: u16) -> Result<()> {
        let storage_key = map_key(key);
        match self.policy.classify(key) {
            PersistenceClass::Normal => self.normal.remove(storage_key),
            PersistenceClass::Secret => self.secure.remove_secret(storage_key),
        }
    }

    /// Flush both stores.
    pub fn sync(&mut self) -> Result<()> {
        self.normal.sync()?;
        self.secure.sync_secure()
    }

    /// Consume the adapter.
    #[must_use]
    pub fn into_parts(self) -> (N, S, PersistencePolicy) {
        (self.normal, self.secure, self.policy)
    }
}

fn map_key(key: u16) -> StorageKey {
    StorageKey::new(StorageDomain::Device, key)
}

/// Lifecycle interface exposed by the protocol adapter to Ariel OS.
///
/// Concrete implementations own the internal `rs-matter::Matter`, interaction
/// model, responder, BTP session, and DNS-SD tasks.
pub trait MatterProtocol {
    /// Load persistent state and validate all registered handlers.
    async fn startup(&mut self) -> Result<()>;
    /// Run the protocol until a platform error or controlled shutdown occurs.
    async fn run(&mut self) -> Result<()>;
    /// Stop new exchanges and flush persistent protocol state.
    async fn quiesce(&mut self) -> Result<()>;
    /// Notify the protocol engine that an endpoint attribute changed.
    fn notify_attribute_changed(&self, endpoint: u16, cluster: u32, attribute: u32) -> Result<()>;
    /// Notify the protocol engine that a dynamic endpoint was added or removed.
    fn notify_topology_changed(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_rs_matter_is_matter_1_6() {
        assert_eq!(MATTER_SPEC_VERSION, REQUIRED_MATTER_SPEC_VERSION);
    }

    #[test]
    fn persistence_policy_is_secure_by_default() {
        let mut policy = PersistencePolicy::secure_by_default();
        assert_eq!(policy.classify(20), PersistenceClass::Secret);
        assert_eq!(policy.allow_normal(100..=120), Ok(()));
        assert_eq!(policy.classify(110), PersistenceClass::Normal);
        assert_eq!(policy.classify(121), PersistenceClass::Secret);
    }
}
