//! Dynamic Matter bridge endpoint registry.

use heapless::{String, Vec};

use crate::endpoint::EndpointId;
use crate::{Error, Result};

/// Maximum stable unique-ID length for a bridged device.
pub const MAX_BRIDGED_UNIQUE_ID: usize = 64;
/// Maximum user-visible node label length.
pub const MAX_BRIDGED_LABEL: usize = 32;

/// State of one bridged device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgedDevice {
    /// Matter endpoint assigned by the bridge.
    pub endpoint: EndpointId,
    /// Stable identifier that does not change across restarts.
    pub unique_id: String<MAX_BRIDGED_UNIQUE_ID>,
    /// Current user-visible label.
    pub node_label: String<MAX_BRIDGED_LABEL>,
    /// Primary Matter device type.
    pub device_type: u32,
    /// Reachability state.
    pub reachable: bool,
    /// Monotonic deadline requested through KeepActive.
    pub keep_active_until_ms: u64,
    /// Data version for subscription reporting.
    pub data_version: u32,
}

impl BridgedDevice {
    /// Create bridge metadata.
    pub fn new(
        endpoint: EndpointId,
        unique_id: &str,
        node_label: &str,
        device_type: u32,
    ) -> Result<Self> {
        if endpoint.0 == 0 || unique_id.is_empty() {
            return Err(Error::InvalidArgument);
        }
        let mut stored_id = String::new();
        stored_id.push_str(unique_id).map_err(|_| Error::Capacity)?;
        let mut stored_label = String::new();
        stored_label
            .push_str(node_label)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            endpoint,
            unique_id: stored_id,
            node_label: stored_label,
            device_type,
            reachable: true,
            keep_active_until_ms: 0,
            data_version: 0,
        })
    }
}

/// Fixed-capacity bridge table.
pub struct BridgeTable<const N: usize> {
    devices: Vec<BridgedDevice, N>,
}

impl<const N: usize> BridgeTable<N> {
    /// Create an empty bridge table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Add a bridged device. Endpoint and unique-ID collisions fail.
    pub fn add(&mut self, device: BridgedDevice) -> Result<()> {
        if self.devices.iter().any(|existing| {
            existing.endpoint == device.endpoint || existing.unique_id == device.unique_id
        }) {
            return Err(Error::Conflict);
        }
        self.devices.push(device).map_err(|_| Error::Capacity)
    }

    /// Remove one bridged endpoint.
    pub fn remove(&mut self, endpoint: EndpointId) -> Result<BridgedDevice> {
        let index = self
            .devices
            .iter()
            .position(|device| device.endpoint == endpoint)
            .ok_or(Error::NotFound)?;
        Ok(self.devices.swap_remove(index))
    }

    /// Find a bridged endpoint.
    #[must_use]
    pub fn get(&self, endpoint: EndpointId) -> Option<&BridgedDevice> {
        self.devices
            .iter()
            .find(|device| device.endpoint == endpoint)
    }

    /// Find a bridged endpoint by stable identifier.
    #[must_use]
    pub fn by_unique_id(&self, unique_id: &str) -> Option<&BridgedDevice> {
        self.devices
            .iter()
            .find(|device| device.unique_id.as_str() == unique_id)
    }

    /// Update reachability and return the new data version.
    pub fn set_reachable(&mut self, endpoint: EndpointId, reachable: bool) -> Result<u32> {
        let device = self
            .devices
            .iter_mut()
            .find(|device| device.endpoint == endpoint)
            .ok_or(Error::NotFound)?;
        if device.reachable != reachable {
            device.reachable = reachable;
            device.data_version = device.data_version.wrapping_add(1);
        }
        Ok(device.data_version)
    }

    /// Update the user-visible label.
    pub fn set_node_label(&mut self, endpoint: EndpointId, label: &str) -> Result<u32> {
        let device = self
            .devices
            .iter_mut()
            .find(|device| device.endpoint == endpoint)
            .ok_or(Error::NotFound)?;
        let mut stored = String::new();
        stored.push_str(label).map_err(|_| Error::Capacity)?;
        if device.node_label != stored {
            device.node_label = stored;
            device.data_version = device.data_version.wrapping_add(1);
        }
        Ok(device.data_version)
    }

    /// Honor a KeepActive request with a bounded duration.
    pub fn keep_active(
        &mut self,
        endpoint: EndpointId,
        now_ms: u64,
        requested_ms: u32,
        maximum_ms: u32,
    ) -> Result<u64> {
        if requested_ms == 0 || requested_ms > maximum_ms {
            return Err(Error::InvalidArgument);
        }
        let device = self
            .devices
            .iter_mut()
            .find(|device| device.endpoint == endpoint)
            .ok_or(Error::NotFound)?;
        if !device.reachable {
            return Err(Error::Network);
        }
        let deadline = now_ms
            .checked_add(u64::from(requested_ms))
            .ok_or(Error::InvalidArgument)?;
        device.keep_active_until_ms = device.keep_active_until_ms.max(deadline);
        device.data_version = device.data_version.wrapping_add(1);
        Ok(device.keep_active_until_ms)
    }

    /// Iterate over bridged devices.
    pub fn iter(&self) -> impl Iterator<Item = &BridgedDevice> {
        self.devices.iter()
    }
}

impl<const N: usize> Default for BridgeTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Device-specific bridge transport. It uses typed application values rather
/// than Matter TLV.
pub trait BridgeBackend {
    /// Return whether a remote device is reachable.
    async fn probe(&mut self, unique_id: &str) -> Result<bool>;
    /// Keep a sleepy remote device active until the supplied deadline.
    async fn keep_active(&mut self, unique_id: &str, until_ms: u64) -> Result<()>;
    /// Remove a remote-device binding after an endpoint is removed.
    async fn detach(&mut self, unique_id: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(endpoint: u16, unique_id: &str) -> Result<BridgedDevice> {
        BridgedDevice::new(EndpointId(endpoint), unique_id, "device", 0x0100)
    }

    #[test]
    fn stable_ids_and_endpoints_are_unique() {
        let mut table = BridgeTable::<3>::new();
        let first = device(2, "bus-7/node-4");
        assert!(first.is_ok());
        if let Ok(first) = first {
            assert_eq!(table.add(first.clone()), Ok(()));
            assert_eq!(table.add(first), Err(Error::Conflict));
        }
        let same_id = device(3, "bus-7/node-4");
        if let Ok(same_id) = same_id {
            assert_eq!(table.add(same_id), Err(Error::Conflict));
        }
    }

    #[test]
    fn keep_active_is_bounded_and_requires_reachability() {
        let mut table = BridgeTable::<2>::new();
        if let Ok(value) = device(2, "remote-2") {
            assert_eq!(table.add(value), Ok(()));
        }
        assert_eq!(
            table.keep_active(EndpointId(2), 1_000, 500, 1_000),
            Ok(1_500)
        );
        assert_eq!(
            table.keep_active(EndpointId(2), 1_000, 2_000, 1_000),
            Err(Error::InvalidArgument)
        );
        assert_eq!(table.set_reachable(EndpointId(2), false), Ok(1));
        assert_eq!(
            table.keep_active(EndpointId(2), 2_000, 500, 1_000),
            Err(Error::Network)
        );
    }
}
