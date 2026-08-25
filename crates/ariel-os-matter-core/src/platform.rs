//! Platform traits for IP, discovery, BLE, Thread, Ethernet, time, and reset.

use core::net::{Ipv6Addr, SocketAddrV6};

use heapless::{String, Vec};

use crate::{Error, Result};

/// Maximum DNS-SD instance or host label length.
pub const MAX_DNS_NAME_LEN: usize = 63;
/// Maximum DNS-SD TXT key length.
pub const MAX_TXT_KEY_LEN: usize = 16;
/// Maximum DNS-SD TXT value length.
pub const MAX_TXT_VALUE_LEN: usize = 128;
/// Maximum TXT records per service.
pub const MAX_TXT_RECORDS: usize = 24;
/// Maximum resolved IPv6 addresses.
pub const MAX_RESOLVED_ADDRS: usize = 8;

/// A monotonic platform clock.
pub trait Clock {
    /// Return monotonic milliseconds since boot.
    fn now_ms(&self) -> u64;
    /// Sleep for at least the supplied duration.
    async fn sleep_ms(&self, duration_ms: u64);
}

/// A cryptographically secure random source.
pub trait Entropy {
    /// Fill the buffer with unpredictable bytes.
    fn fill(&mut self, output: &mut [u8]) -> Result<()>;
}

/// A UDP/IPv6 transport used by Matter.
pub trait Udp6Transport {
    /// Bind the Matter UDP port.
    async fn bind(&mut self, port: u16) -> Result<()>;
    /// Send one datagram.
    async fn send_to(&mut self, payload: &[u8], peer: SocketAddrV6) -> Result<()>;
    /// Receive one datagram.
    async fn receive_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddrV6)>;
    /// Join an IPv6 multicast group.
    async fn join_multicast(&mut self, group: Ipv6Addr) -> Result<()>;
    /// Leave an IPv6 multicast group.
    async fn leave_multicast(&mut self, group: Ipv6Addr) -> Result<()>;
}

/// One DNS-SD TXT item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TxtRecord {
    /// TXT key.
    pub key: String<MAX_TXT_KEY_LEN>,
    /// TXT value.
    pub value: String<MAX_TXT_VALUE_LEN>,
}

impl TxtRecord {
    /// Create a TXT record.
    pub fn new(key: &str, value: &str) -> Result<Self> {
        let mut stored_key = String::new();
        stored_key.push_str(key).map_err(|_| Error::Capacity)?;
        let mut stored_value = String::new();
        stored_value.push_str(value).map_err(|_| Error::Capacity)?;
        if stored_key.is_empty() || stored_key.contains('=') {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            key: stored_key,
            value: stored_value,
        })
    }
}

/// A Matter DNS-SD service advertisement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsSdService {
    /// Service instance label.
    pub instance: String<MAX_DNS_NAME_LEN>,
    /// Service type, for example `_matter._tcp`.
    pub service_type: String<MAX_DNS_NAME_LEN>,
    /// Service port.
    pub port: u16,
    /// TXT records.
    pub txt: Vec<TxtRecord, MAX_TXT_RECORDS>,
}

/// A resolved DNS-SD service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedService {
    /// Service port.
    pub port: u16,
    /// Resolved IPv6 addresses.
    pub addresses: Vec<Ipv6Addr, MAX_RESOLVED_ADDRS>,
    /// TXT records.
    pub txt: Vec<TxtRecord, MAX_TXT_RECORDS>,
}

/// mDNS/DNS-SD provider.
pub trait Mdns {
    /// Publish or replace a local service.
    async fn publish(&mut self, service: &DnsSdService) -> Result<()>;
    /// Withdraw one service instance.
    async fn withdraw(&mut self, instance: &str, service_type: &str) -> Result<()>;
    /// Resolve one remote service instance.
    async fn resolve(&mut self, instance: &str, service_type: &str) -> Result<ResolvedService>;
}

/// Ethernet link state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EthernetState {
    /// No physical carrier is present.
    Down,
    /// A physical carrier is present and the interface is usable.
    Up,
}

/// Ethernet platform control.
pub trait Ethernet {
    /// Return the current physical link state.
    fn state(&self) -> EthernetState;
    /// Start the interface.
    async fn start(&mut self) -> Result<()>;
    /// Stop the interface.
    async fn stop(&mut self) -> Result<()>;
}

/// Operational Thread dataset.
///
/// This type does not implement `Debug` because the dataset contains network
/// secrets.
#[derive(Clone, Eq, PartialEq)]
pub struct ThreadDataset {
    bytes: Vec<u8, 254>,
}

impl ThreadDataset {
    /// Copy an active operational dataset.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(Error::InvalidArgument);
        }
        let mut stored = Vec::new();
        stored
            .extend_from_slice(bytes)
            .map_err(|_| Error::Capacity)?;
        Ok(Self { bytes: stored })
    }

    /// Give the Thread driver temporary access to the dataset.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

/// Thread role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadRole {
    /// The radio is disabled.
    Disabled,
    /// The node is detached.
    Detached,
    /// The node is a child.
    Child,
    /// The node is a router.
    Router,
    /// The node is the leader.
    Leader,
}

/// Thread platform control.
pub trait Thread {
    /// Scan for Thread networks and return active datasets that can be shown to
    /// an authorized commissioner.
    async fn scan(&mut self) -> Result<Vec<ThreadDataset, 16>>;
    /// Attach with an operational dataset.
    async fn attach(&mut self, dataset: &ThreadDataset) -> Result<()>;
    /// Detach from the current network.
    async fn detach(&mut self) -> Result<()>;
    /// Remove the stored operational dataset.
    async fn clear_dataset(&mut self) -> Result<()>;
    /// Return the current role.
    fn role(&self) -> ThreadRole;
}

/// A BLE peer address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleAddress(pub [u8; 6]);

/// A BLE GATT event for the Matter BTP service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleGattEvent {
    /// A commissioner connected.
    Connected {
        /// Peer address.
        peer: BleAddress,
        /// Negotiated ATT MTU.
        mtu: u16,
    },
    /// A commissioner disconnected.
    Disconnected {
        /// Peer address.
        peer: BleAddress,
    },
    /// The peer wrote the Matter C1 characteristic.
    C1Write {
        /// Peer address.
        peer: BleAddress,
        /// Number of valid bytes placed in the caller buffer.
        len: usize,
        /// Negotiated ATT MTU.
        mtu: u16,
    },
}

/// BLE peripheral transport for Matter commissioning.
pub trait BleCommissioning {
    /// Start connectable Matter advertising.
    async fn start(&mut self, discriminator: u16, vendor_id: u16, product_id: u16) -> Result<()>;
    /// Stop Matter advertising.
    async fn stop(&mut self) -> Result<()>;
    /// Wait for the next GATT event. C1 data is written to `buffer`.
    async fn next_event(&mut self, buffer: &mut [u8]) -> Result<BleGattEvent>;
    /// Send a confirmed indication on the Matter C2 characteristic.
    async fn indicate_c2(&mut self, peer: BleAddress, payload: &[u8]) -> Result<()>;
    /// Disconnect a peer after a protocol or policy failure.
    async fn disconnect(&mut self, peer: BleAddress) -> Result<()>;
}

/// Hardware reset reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetReason {
    /// A normal software restart.
    Software,
    /// A completed factory reset.
    FactoryReset,
    /// An OTA activation restart.
    OtaActivation,
    /// A watchdog recovery.
    Watchdog,
}

/// Reset controller.
pub trait ResetControl {
    /// Restart the system. A successful implementation does not return.
    async fn reset(&mut self, reason: ResetReason) -> Result<core::convert::Infallible>;
}
