//! Multi-fabric state and Matter access-control evaluation.

use core::num::NonZeroU8;

use heapless::Vec;

use crate::{Error, Result};

/// Minimum fabric capacity required by Matter accessories.
pub const MIN_FABRIC_CAPACITY: usize = 5;
/// Maximum subjects in one portable ACL entry.
pub const MAX_ACL_SUBJECTS: usize = 8;
/// Maximum targets in one portable ACL entry.
pub const MAX_ACL_TARGETS: usize = 8;

/// Local fabric index.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FabricIndex(NonZeroU8);

impl FabricIndex {
    /// Create a valid local fabric index.
    pub fn new(value: u8) -> Result<Self> {
        NonZeroU8::new(value)
            .map(Self)
            .ok_or(Error::InvalidArgument)
    }

    /// Return the wire value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// A commissioned fabric record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FabricRecord {
    /// Local table index.
    pub index: FabricIndex,
    /// Compressed fabric identifier.
    pub compressed_id: u64,
    /// Operational node identifier on this fabric.
    pub node_id: u64,
    /// Vendor that commissioned the fabric.
    pub vendor_id: u16,
}

/// Fixed-capacity multi-fabric table.
pub struct FabricTable<const N: usize> {
    records: Vec<FabricRecord, N>,
}

impl<const N: usize> FabricTable<N> {
    /// Create an empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Add a fabric. Index and compressed-ID collisions fail closed.
    pub fn add(&mut self, record: FabricRecord) -> Result<()> {
        if self.records.iter().any(|existing| {
            existing.index == record.index || existing.compressed_id == record.compressed_id
        }) {
            return Err(Error::Conflict);
        }
        self.records.push(record).map_err(|_| Error::Capacity)
    }

    /// Remove a fabric.
    pub fn remove(&mut self, index: FabricIndex) -> Result<FabricRecord> {
        let position = self
            .records
            .iter()
            .position(|record| record.index == index)
            .ok_or(Error::NotFound)?;
        Ok(self.records.swap_remove(position))
    }

    /// Find a fabric.
    #[must_use]
    pub fn get(&self, index: FabricIndex) -> Option<&FabricRecord> {
        self.records.iter().find(|record| record.index == index)
    }

    /// Iterate over commissioned fabrics.
    pub fn iter(&self) -> impl Iterator<Item = &FabricRecord> {
        self.records.iter()
    }

    /// Return the number of fabrics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Return true when there are no fabrics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl<const N: usize> Default for FabricTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Session authentication mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMode {
    /// Password-authenticated commissioning session.
    Pase,
    /// Certificate-authenticated operational session.
    Case,
    /// Group session.
    Group,
}

/// Matter privilege in increasing order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Privilege {
    /// Read-only access.
    View = 1,
    /// Proxy-discovery access.
    ProxyView = 2,
    /// Normal device operation.
    Operate = 3,
    /// Device management.
    Manage = 4,
    /// Fabric administration.
    Administer = 5,
}

/// An ACL subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Subject {
    /// Operational node identifier.
    Node(u64),
    /// Group identifier.
    Group(u16),
    /// CASE Authenticated Tag value.
    Cat(u32),
}

/// One ACL target filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    /// Endpoint filter. `None` matches all endpoints.
    pub endpoint: Option<u16>,
    /// Cluster filter. `None` matches all clusters.
    pub cluster: Option<u32>,
    /// Device-type filter. `None` matches all device types.
    pub device_type: Option<u32>,
}

impl Target {
    /// Create a target that matches all paths.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            endpoint: None,
            cluster: None,
            device_type: None,
        }
    }

    fn matches(&self, request: &AccessRequest) -> bool {
        self.endpoint.is_none_or(|value| value == request.endpoint)
            && self.cluster.is_none_or(|value| value == request.cluster)
            && self
                .device_type
                .is_none_or(|value| request.device_types.iter().any(|item| *item == value))
    }
}

/// One fabric-scoped ACL entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclEntry {
    /// Owning fabric.
    pub fabric: FabricIndex,
    /// Granted privilege.
    pub privilege: Privilege,
    /// Required authentication mode.
    pub auth_mode: AuthMode,
    /// Matching subjects. An empty list matches all authenticated subjects on
    /// the owning fabric.
    pub subjects: Vec<Subject, MAX_ACL_SUBJECTS>,
    /// Matching targets. An empty list matches all targets.
    pub targets: Vec<Target, MAX_ACL_TARGETS>,
}

/// Access request evaluated against an ACL table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessRequest {
    /// Accessing fabric.
    pub fabric: FabricIndex,
    /// Authentication mode.
    pub auth_mode: AuthMode,
    /// Accessing subject.
    pub subject: Subject,
    /// Endpoint.
    pub endpoint: u16,
    /// Cluster.
    pub cluster: u32,
    /// Device types declared by the endpoint.
    pub device_types: Vec<u32, 8>,
    /// Required privilege.
    pub required: Privilege,
}

/// Fixed-capacity ACL table.
pub struct AclTable<const N: usize> {
    entries: Vec<AclEntry, N>,
}

impl<const N: usize> AclTable<N> {
    /// Create an empty ACL table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an ACL entry.
    pub fn add(&mut self, entry: AclEntry) -> Result<()> {
        if self.entries.iter().any(|existing| existing == &entry) {
            return Err(Error::Conflict);
        }
        self.entries.push(entry).map_err(|_| Error::Capacity)
    }

    /// Remove all entries for a fabric.
    pub fn remove_fabric(&mut self, fabric: FabricIndex) {
        self.entries.retain(|entry| entry.fabric != fabric);
    }

    /// Decide access. No match is a denial.
    #[must_use]
    pub fn permits(&self, request: &AccessRequest) -> bool {
        self.entries.iter().any(|entry| {
            entry.fabric == request.fabric
                && entry.auth_mode == request.auth_mode
                && entry.privilege >= request.required
                && (entry.subjects.is_empty() || entry.subjects.contains(&request.subject))
                && (entry.targets.is_empty()
                    || entry.targets.iter().any(|target| target.matches(request)))
        })
    }

    /// Iterate over ACL entries.
    pub fn iter(&self) -> impl Iterator<Item = &AclEntry> {
        self.entries.iter()
    }
}

impl<const N: usize> Default for AclTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabric(value: u8) -> FabricIndex {
        match FabricIndex::new(value) {
            Ok(index) => index,
            Err(error) => std::panic::panic_any(error),
        }
    }

    #[test]
    fn acl_is_fabric_scoped_and_fails_closed() {
        let mut table = AclTable::<4>::new();
        let mut subjects = Vec::new();
        assert_eq!(subjects.push(Subject::Node(44)), Ok(()));
        let mut targets = Vec::new();
        assert_eq!(
            targets.push(Target {
                endpoint: Some(2),
                cluster: Some(6),
                device_type: None,
            }),
            Ok(())
        );
        assert_eq!(
            table.add(AclEntry {
                fabric: fabric(1),
                privilege: Privilege::Operate,
                auth_mode: AuthMode::Case,
                subjects,
                targets,
            }),
            Ok(())
        );

        let request = AccessRequest {
            fabric: fabric(1),
            auth_mode: AuthMode::Case,
            subject: Subject::Node(44),
            endpoint: 2,
            cluster: 6,
            device_types: Vec::new(),
            required: Privilege::Operate,
        };
        assert!(table.permits(&request));

        let mut wrong_fabric = request.clone();
        wrong_fabric.fabric = fabric(2);
        assert!(!table.permits(&wrong_fabric));

        let mut elevated = request;
        elevated.required = Privilege::Administer;
        assert!(!table.permits(&elevated));
    }

    #[test]
    fn fabric_table_rejects_collisions() {
        let mut table = FabricTable::<5>::new();
        let first = FabricRecord {
            index: fabric(1),
            compressed_id: 10,
            node_id: 20,
            vendor_id: 1,
        };
        assert_eq!(table.add(first), Ok(()));
        assert_eq!(table.add(first), Err(Error::Conflict));
    }
}
