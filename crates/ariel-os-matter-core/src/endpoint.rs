//! Typed, fixed-capacity endpoint registration.

use core::marker::PhantomData;

use bitflags::bitflags;
use heapless::Vec;

use crate::devices::{DeviceTypeDescriptor, DeviceTypeMarker};
use crate::{Error, Result};

/// Maximum device types declared by one endpoint.
pub const MAX_DEVICE_TYPES_PER_ENDPOINT: usize = 8;

/// Matter endpoint identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EndpointId(pub u16);

bitflags! {
    /// Portable endpoint capabilities used to select cluster handlers and services.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct EndpointCapabilities: u64 {
        /// On/Off behavior.
        const ON_OFF = 1 << 0;
        /// Level behavior.
        const LEVEL = 1 << 1;
        /// Color behavior.
        const COLOR = 1 << 2;
        /// Identify behavior.
        const IDENTIFY = 1 << 3;
        /// Groups and scenes.
        const GROUPS_SCENES = 1 << 4;
        /// Generic sensor reporting.
        const SENSOR = 1 << 5;
        /// Door-lock behavior.
        const LOCK = 1 << 6;
        /// Closure Control behavior.
        const CLOSURE = 1 << 7;
        /// Window-covering behavior.
        const WINDOW_COVERING = 1 << 8;
        /// Thermostat or HVAC behavior.
        const HVAC = 1 << 9;
        /// Pump behavior.
        const PUMP = 1 << 10;
        /// Valve or irrigation behavior.
        const VALVE = 1 << 11;
        /// Electrical measurement behavior.
        const ENERGY_METER = 1 << 12;
        /// EVSE behavior.
        const EVSE = 1 << 13;
        /// Appliance operational-state behavior.
        const APPLIANCE = 1 << 14;
        /// Audio behavior.
        const AUDIO = 1 << 15;
        /// Media behavior.
        const MEDIA = 1 << 16;
        /// Bridged-device information.
        const BRIDGED = 1 << 17;
        /// Aggregator behavior.
        const AGGREGATOR = 1 << 18;
        /// OTA requestor behavior.
        const OTA_REQUESTOR = 1 << 19;
        /// OTA provider behavior.
        const OTA_PROVIDER = 1 << 20;
        /// Thread infrastructure behavior.
        const THREAD_INFRASTRUCTURE = 1 << 21;
    }
}

/// Runtime endpoint metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointRecord {
    /// Endpoint identifier.
    pub id: EndpointId,
    /// Declared Matter device types.
    pub device_types: Vec<DeviceTypeDescriptor, MAX_DEVICE_TYPES_PER_ENDPOINT>,
    /// Portable capability set.
    pub capabilities: EndpointCapabilities,
    /// Current endpoint data version.
    pub data_version: u32,
}

/// A typed endpoint handle exposed to application code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointHandle<T> {
    id: EndpointId,
    marker: PhantomData<T>,
}

impl<T> EndpointHandle<T> {
    /// Return the Matter endpoint identifier.
    #[must_use]
    pub const fn id(self) -> EndpointId {
        self.id
    }
}

/// Fixed-capacity registry for all endpoints on one node.
pub struct EndpointRegistry<const N: usize> {
    endpoints: Vec<EndpointRecord, N>,
}

impl<const N: usize> EndpointRegistry<N> {
    /// Create an empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            endpoints: Vec::new(),
        }
    }

    /// Register a typed endpoint.
    pub fn register<T: DeviceTypeMarker>(
        &mut self,
        id: EndpointId,
        capabilities: EndpointCapabilities,
    ) -> Result<EndpointHandle<T>> {
        if self.endpoints.iter().any(|endpoint| endpoint.id == id) {
            return Err(Error::Conflict);
        }
        if id.0 == 0 && T::DESCRIPTOR.id != 0x0016 {
            return Err(Error::InvalidArgument);
        }
        if id.0 != 0 && T::DESCRIPTOR.id == 0x0016 {
            return Err(Error::InvalidArgument);
        }

        let mut device_types = Vec::new();
        device_types
            .push(T::DESCRIPTOR)
            .map_err(|_| Error::Capacity)?;
        self.endpoints
            .push(EndpointRecord {
                id,
                device_types,
                capabilities,
                data_version: 0,
            })
            .map_err(|_| Error::Capacity)?;

        Ok(EndpointHandle {
            id,
            marker: PhantomData,
        })
    }

    /// Add an additional device type to an endpoint, for example Bridged Node.
    pub fn add_device_type<T: DeviceTypeMarker>(&mut self, id: EndpointId) -> Result<()> {
        let endpoint = self.get_mut(id).ok_or(Error::NotFound)?;
        if endpoint
            .device_types
            .iter()
            .any(|device| device.id == T::DESCRIPTOR.id)
        {
            return Err(Error::Conflict);
        }
        endpoint
            .device_types
            .push(T::DESCRIPTOR)
            .map_err(|_| Error::Capacity)?;
        endpoint.data_version = endpoint.data_version.wrapping_add(1);
        Ok(())
    }

    /// Remove a non-root endpoint.
    pub fn remove(&mut self, id: EndpointId) -> Result<EndpointRecord> {
        if id.0 == 0 {
            return Err(Error::AccessDenied);
        }
        let index = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.id == id)
            .ok_or(Error::NotFound)?;
        Ok(self.endpoints.swap_remove(index))
    }

    /// Find endpoint metadata.
    #[must_use]
    pub fn get(&self, id: EndpointId) -> Option<&EndpointRecord> {
        self.endpoints.iter().find(|endpoint| endpoint.id == id)
    }

    /// Find mutable endpoint metadata.
    pub fn get_mut(&mut self, id: EndpointId) -> Option<&mut EndpointRecord> {
        self.endpoints.iter_mut().find(|endpoint| endpoint.id == id)
    }

    /// Increment one endpoint data version.
    pub fn changed(&mut self, id: EndpointId) -> Result<u32> {
        let endpoint = self.get_mut(id).ok_or(Error::NotFound)?;
        endpoint.data_version = endpoint.data_version.wrapping_add(1);
        Ok(endpoint.data_version)
    }

    /// Iterate over endpoints in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &EndpointRecord> {
        self.endpoints.iter()
    }

    /// Return the number of registered endpoints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.endpoints.len()
    }

    /// Return true if no endpoint is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
    }
}

impl<const N: usize> Default for EndpointRegistry<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Typed endpoint state owned by an application.
pub struct TypedEndpoint<T, S> {
    handle: EndpointHandle<T>,
    state: S,
}

impl<T, S> TypedEndpoint<T, S> {
    /// Create typed application state for a registered endpoint.
    pub const fn new(handle: EndpointHandle<T>, state: S) -> Self {
        Self { handle, state }
    }

    /// Return the endpoint handle.
    #[must_use]
    pub const fn handle(&self) -> EndpointHandle<T> {
        EndpointHandle {
            id: self.handle.id,
            marker: PhantomData,
        }
    }

    /// Read typed state.
    #[must_use]
    pub const fn state(&self) -> &S {
        &self.state
    }

    /// Mutate typed state.
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }

    /// Consume the endpoint state.
    #[must_use]
    pub fn into_state(self) -> S {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{BridgedNode, OnOffLight, RootNode};

    #[test]
    fn registry_supports_multiple_typed_endpoints() {
        let mut registry = EndpointRegistry::<4>::new();
        let root = registry.register::<RootNode>(EndpointId(0), EndpointCapabilities::empty());
        let lamp = registry.register::<OnOffLight>(
            EndpointId(1),
            EndpointCapabilities::ON_OFF | EndpointCapabilities::IDENTIFY,
        );
        assert!(root.is_ok());
        assert!(lamp.is_ok());
        assert_eq!(
            registry.add_device_type::<BridgedNode>(EndpointId(1)),
            Ok(())
        );
        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry
                .get(EndpointId(1))
                .map(|value| value.device_types.len()),
            Some(2)
        );
    }

    #[test]
    fn registry_reserves_root_endpoint() {
        let mut registry = EndpointRegistry::<2>::new();
        assert_eq!(
            registry.register::<OnOffLight>(EndpointId(0), EndpointCapabilities::ON_OFF),
            Err(Error::InvalidArgument)
        );
    }
}
