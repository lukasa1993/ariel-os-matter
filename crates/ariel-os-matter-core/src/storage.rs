//! Persistent storage contracts and integrity-protected records.

use crate::{Error, Result};

/// Record format magic value.
pub const RECORD_MAGIC: u32 = 0x4D41_5452;
/// Record header size in bytes.
pub const RECORD_HEADER_LEN: usize = 20;

/// Persistent data domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum StorageDomain {
    /// Fabrics and fabric-scoped metadata.
    Fabrics = 1,
    /// Access control lists.
    AccessControl = 2,
    /// Commissioning progress and fail-safe data.
    Commissioning = 3,
    /// Network credentials and network state.
    Network = 4,
    /// Device and cluster state.
    Device = 5,
    /// Active and persisted subscriptions.
    Subscriptions = 6,
    /// Event epochs and retained events.
    Events = 7,
    /// OTA requestor/provider state.
    Ota = 8,
    /// Bridge endpoint data.
    Bridge = 9,
    /// Operational and commissioning keys.
    Keys = 10,
}

/// A stable persistent key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageKey {
    /// Data domain.
    pub domain: StorageDomain,
    /// Domain-local record number.
    pub id: u16,
}

impl StorageKey {
    /// Create a key.
    #[must_use]
    pub const fn new(domain: StorageDomain, id: u16) -> Self {
        Self { domain, id }
    }
}

/// Asynchronous normal storage.
pub trait BlobStore {
    /// Load a record into `output` and return its length.
    async fn load(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically replace a record.
    async fn store(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Remove a record. Missing records are successful removals.
    async fn remove(&mut self, key: StorageKey) -> Result<()>;
    /// Flush all preceding operations to durable media.
    async fn sync(&mut self) -> Result<()>;
    /// Erase one storage domain.
    async fn erase_domain(&mut self, domain: StorageDomain) -> Result<()>;
}

/// Synchronous storage used by the synchronous `rs-matter` persistence API.
pub trait SyncBlobStore {
    /// Load a record into `output` and return its length.
    fn load(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically replace a record.
    fn store(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Remove a record. Missing records are successful removals.
    fn remove(&mut self, key: StorageKey) -> Result<()>;
    /// Flush all preceding operations to durable media.
    fn sync(&mut self) -> Result<()>;
    /// Erase one storage domain.
    fn erase_domain(&mut self, domain: StorageDomain) -> Result<()>;
}

/// Purpose of a hardware-backed secret key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyPurpose {
    /// Device attestation signing key.
    DeviceAttestation,
    /// Operational fabric key.
    Operational,
    /// Group key.
    Group,
    /// Storage encryption key.
    StorageEncryption,
    /// OTA image verification key.
    OtaVerification,
}

/// Opaque hardware-backed key reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyHandle(pub u32);

/// Asynchronous secure secret storage.
pub trait SecureStore {
    /// Load encrypted or hardware-protected secret data.
    async fn load_secret(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically store secret data.
    async fn store_secret(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Remove secret data.
    async fn remove_secret(&mut self, key: StorageKey) -> Result<()>;
    /// Create a non-exportable key.
    async fn generate_key(&mut self, purpose: KeyPurpose) -> Result<KeyHandle>;
    /// Sign a message with a non-exportable key.
    async fn sign(&mut self, key: KeyHandle, message: &[u8], signature: &mut [u8])
    -> Result<usize>;
    /// Perform ECDH with a non-exportable key.
    async fn ecdh(
        &mut self,
        key: KeyHandle,
        peer_public: &[u8],
        shared: &mut [u8],
    ) -> Result<usize>;
    /// Destroy a non-exportable key.
    async fn destroy_key(&mut self, key: KeyHandle) -> Result<()>;
    /// Flush all preceding operations to durable media.
    async fn sync_secure(&mut self) -> Result<()>;
    /// Erase all Matter secrets.
    async fn erase_all_secrets(&mut self) -> Result<()>;
}

/// Synchronous secure storage used by the `rs-matter` persistence adapter.
pub trait SyncSecureStore {
    /// Load protected secret data.
    fn load_secret(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically store secret data.
    fn store_secret(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Remove secret data.
    fn remove_secret(&mut self, key: StorageKey) -> Result<()>;
    /// Flush all preceding operations to durable media.
    fn sync_secure(&mut self) -> Result<()>;
    /// Erase all Matter secrets.
    fn erase_all_secrets(&mut self) -> Result<()>;
}

/// Integrity-protected record header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordHeader {
    /// Schema version.
    pub schema: u16,
    /// Monotonic record generation.
    pub generation: u64,
    /// Payload length.
    pub payload_len: u16,
    /// CRC-32 of the payload.
    pub payload_crc: u32,
}

/// Encode an integrity-protected record.
pub fn encode_record(
    schema: u16,
    generation: u64,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize> {
    let payload_len = u16::try_from(payload.len()).map_err(|_| Error::Capacity)?;
    let total = RECORD_HEADER_LEN
        .checked_add(payload.len())
        .ok_or(Error::Capacity)?;
    if output.len() < total {
        return Err(Error::Capacity);
    }

    output[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
    output[4..6].copy_from_slice(&schema.to_le_bytes());
    output[6..8].copy_from_slice(&payload_len.to_le_bytes());
    output[8..16].copy_from_slice(&generation.to_le_bytes());
    output[16..20].copy_from_slice(&crc32(payload).to_le_bytes());
    output[RECORD_HEADER_LEN..total].copy_from_slice(payload);
    Ok(total)
}

/// Decode and verify an integrity-protected record.
pub fn decode_record(input: &[u8]) -> Result<(RecordHeader, &[u8])> {
    if input.len() < RECORD_HEADER_LEN {
        return Err(Error::Corrupt);
    }

    let magic = u32::from_le_bytes(copy_array::<4>(&input[0..4])?);
    if magic != RECORD_MAGIC {
        return Err(Error::Corrupt);
    }

    let schema = u16::from_le_bytes(copy_array::<2>(&input[4..6])?);
    let payload_len = u16::from_le_bytes(copy_array::<2>(&input[6..8])?);
    let generation = u64::from_le_bytes(copy_array::<8>(&input[8..16])?);
    let payload_crc = u32::from_le_bytes(copy_array::<4>(&input[16..20])?);
    let end = RECORD_HEADER_LEN
        .checked_add(usize::from(payload_len))
        .ok_or(Error::Corrupt)?;
    let payload = input.get(RECORD_HEADER_LEN..end).ok_or(Error::Corrupt)?;
    if crc32(payload) != payload_crc || end != input.len() {
        return Err(Error::Corrupt);
    }

    Ok((
        RecordHeader {
            schema,
            generation,
            payload_len,
            payload_crc,
        },
        payload,
    ))
}

fn copy_array<const N: usize>(input: &[u8]) -> Result<[u8; N]> {
    input.try_into().map_err(|_| Error::Corrupt)
}

/// Calculate the IEEE CRC-32 of a byte slice.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trip_detects_corruption() {
        let mut bytes = [0_u8; 64];
        let length = encode_record(3, 27, b"fabric-state", &mut bytes);
        assert_eq!(length, Ok(32));

        let decoded = decode_record(&bytes[..32]);
        assert!(decoded.is_ok());
        if let Ok((header, payload)) = decoded {
            assert_eq!(header.schema, 3);
            assert_eq!(header.generation, 27);
            assert_eq!(payload, b"fabric-state");
        }

        bytes[31] ^= 0x80;
        assert_eq!(decode_record(&bytes[..32]), Err(Error::Corrupt));
    }

    #[test]
    fn crc_matches_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
