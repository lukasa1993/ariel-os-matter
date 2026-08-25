//! ESP32-S3 reference adapters.
//!
//! The concrete `esp-radio`, Trouble, flash-encryption, and bootloader objects
//! implement the small traits in this crate. The portable core does not depend
//! on any Espressif crate.

#![no_std]
#![forbid(unsafe_code)]
#![allow(async_fn_in_trait)]

use ariel_os_matter_core::platform::{BleAddress, BleCommissioning, BleGattEvent};
use ariel_os_matter_core::storage::{StorageKey, SyncSecureStore};
use ariel_os_matter_core::wifi::{
    MAX_SCAN_RESULTS, WifiCredentials, WifiDriver, WifiLinkState, WifiScanResult,
};
use ariel_os_matter_core::{Error, Result};
use heapless::Vec;

/// Narrow adapter over the ESP32-S3 station controller.
///
/// The Ariel OS ESP HAL owns the concrete controller. This trait contains every
/// operation required for Matter runtime Wi-Fi management and permits host-side
/// fault-injection tests.
pub trait EspWifiControl {
    /// Scan at runtime.
    async fn scan(&mut self) -> Result<Vec<WifiScanResult, MAX_SCAN_RESULTS>>;
    /// Replace the station configuration with runtime credentials.
    async fn configure_station(&mut self, credentials: &WifiCredentials) -> Result<()>;
    /// Start the radio if needed.
    async fn start(&mut self) -> Result<()>;
    /// Connect the station.
    async fn connect(&mut self) -> Result<()>;
    /// Disconnect the station.
    async fn disconnect(&mut self) -> Result<()>;
    /// Return the current link state.
    fn state(&self) -> WifiLinkState;
}

/// Runtime Wi-Fi driver for the ESP32-S3.
pub struct Esp32S3Wifi<C> {
    control: C,
}

impl<C> Esp32S3Wifi<C> {
    /// Create the driver from the Ariel OS ESP station controller adapter.
    pub const fn new(control: C) -> Self {
        Self { control }
    }

    /// Consume the adapter.
    #[must_use]
    pub fn into_inner(self) -> C {
        self.control
    }
}

impl<C: EspWifiControl> WifiDriver for Esp32S3Wifi<C> {
    async fn scan(&mut self) -> Result<Vec<WifiScanResult, MAX_SCAN_RESULTS>> {
        self.control.scan().await
    }

    async fn connect(&mut self, credentials: &WifiCredentials) -> Result<()> {
        self.control.configure_station(credentials).await?;
        self.control.start().await?;
        self.control.connect().await
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.control.disconnect().await
    }

    fn state(&self) -> WifiLinkState {
        self.control.state()
    }
}

/// Narrow adapter over a Trouble-host Matter GATT service.
pub trait EspBleGatt {
    /// Start connectable advertising.
    async fn start_advertising(
        &mut self,
        discriminator: u16,
        vendor_id: u16,
        product_id: u16,
    ) -> Result<()>;
    /// Stop advertising.
    async fn stop_advertising(&mut self) -> Result<()>;
    /// Receive the next connection, disconnection, or C1-write event.
    async fn next_event(&mut self, buffer: &mut [u8]) -> Result<BleGattEvent>;
    /// Send a confirmed C2 indication.
    async fn indicate(&mut self, peer: BleAddress, payload: &[u8]) -> Result<()>;
    /// Disconnect a peer.
    async fn disconnect(&mut self, peer: BleAddress) -> Result<()>;
}

/// BLE commissioning adapter for ESP32-S3 and Trouble host.
pub struct Esp32S3Ble<G> {
    gatt: G,
}

impl<G> Esp32S3Ble<G> {
    /// Create the BLE adapter.
    pub const fn new(gatt: G) -> Self {
        Self { gatt }
    }

    /// Consume the adapter.
    #[must_use]
    pub fn into_inner(self) -> G {
        self.gatt
    }
}

impl<G: EspBleGatt> BleCommissioning for Esp32S3Ble<G> {
    async fn start(&mut self, discriminator: u16, vendor_id: u16, product_id: u16) -> Result<()> {
        self.gatt
            .start_advertising(discriminator, vendor_id, product_id)
            .await
    }

    async fn stop(&mut self) -> Result<()> {
        self.gatt.stop_advertising().await
    }

    async fn next_event(&mut self, buffer: &mut [u8]) -> Result<BleGattEvent> {
        self.gatt.next_event(buffer).await
    }

    async fn indicate_c2(&mut self, peer: BleAddress, payload: &[u8]) -> Result<()> {
        self.gatt.indicate(peer, payload).await
    }

    async fn disconnect(&mut self, peer: BleAddress) -> Result<()> {
        self.gatt.disconnect(peer).await
    }
}

/// Authenticated-encryption engine backed by an eFuse-derived or hardware
/// protected key. Nonce reuse must be impossible across power loss.
pub trait EspStorageCipher {
    /// Seal plaintext. The backend prepends or otherwise stores nonce and tag.
    fn seal(&mut self, key: StorageKey, plaintext: &[u8], output: &mut [u8]) -> Result<usize>;
    /// Authenticate and decrypt data.
    fn open(&mut self, key: StorageKey, ciphertext: &[u8], output: &mut [u8]) -> Result<usize>;
    /// Destroy the root storage key.
    fn destroy_root_key(&mut self) -> Result<()>;
}

/// Raw redundant flash records used by encrypted secret storage.
pub trait EspSecretFlash {
    /// Read the newest valid encrypted record.
    fn read(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>>;
    /// Atomically append and commit a new encrypted record.
    fn write(&mut self, key: StorageKey, data: &[u8]) -> Result<()>;
    /// Tombstone a record.
    fn remove(&mut self, key: StorageKey) -> Result<()>;
    /// Flush controller caches and verify the commit marker.
    fn sync(&mut self) -> Result<()>;
    /// Erase the complete secure partition.
    fn erase_all(&mut self) -> Result<()>;
}

/// ESP32-S3 encrypted secure store.
///
/// The type keeps no plaintext records. A caller-provided scratch buffer is
/// used for each authenticated operation and is cleared before return.
pub struct Esp32S3SecureStore<F, C, const MAX_RECORD: usize> {
    flash: F,
    cipher: C,
    scratch: [u8; MAX_RECORD],
}

impl<F, C, const MAX_RECORD: usize> Esp32S3SecureStore<F, C, MAX_RECORD> {
    /// Create secure storage.
    pub const fn new(flash: F, cipher: C) -> Self {
        Self {
            flash,
            cipher,
            scratch: [0; MAX_RECORD],
        }
    }

    /// Consume secure storage.
    #[must_use]
    pub fn into_parts(self) -> (F, C) {
        (self.flash, self.cipher)
    }

    fn clear_scratch(&mut self) {
        self.scratch.fill(0);
    }
}

impl<F, C, const MAX_RECORD: usize> SyncSecureStore for Esp32S3SecureStore<F, C, MAX_RECORD>
where
    F: EspSecretFlash,
    C: EspStorageCipher,
{
    fn load_secret(&mut self, key: StorageKey, output: &mut [u8]) -> Result<Option<usize>> {
        let encrypted_len = match self.flash.read(key, &mut self.scratch)? {
            Some(length) => length,
            None => return Ok(None),
        };
        if encrypted_len > self.scratch.len() {
            self.clear_scratch();
            return Err(Error::Corrupt);
        }
        let result = self
            .cipher
            .open(key, &self.scratch[..encrypted_len], output);
        self.clear_scratch();
        result.map(Some)
    }

    fn store_secret(&mut self, key: StorageKey, data: &[u8]) -> Result<()> {
        let length = self.cipher.seal(key, data, &mut self.scratch)?;
        if length > self.scratch.len() {
            self.clear_scratch();
            return Err(Error::Capacity);
        }
        let result = self.flash.write(key, &self.scratch[..length]);
        self.clear_scratch();
        result
    }

    fn remove_secret(&mut self, key: StorageKey) -> Result<()> {
        self.flash.remove(key)
    }

    fn sync_secure(&mut self) -> Result<()> {
        self.flash.sync()
    }

    fn erase_all_secrets(&mut self) -> Result<()> {
        self.flash.erase_all()?;
        self.cipher.destroy_root_key()?;
        self.flash.sync()
    }
}

/// OTA boot-slot operations supplied by the ESP32-S3 bootloader adapter.
pub trait EspOtaBootControl {
    /// Select the inactive slot and erase it.
    async fn begin(&mut self, image_size: u64) -> Result<()>;
    /// Write one aligned image block.
    async fn write(&mut self, offset: u64, data: &[u8]) -> Result<()>;
    /// Flush and verify the image container.
    async fn finish(&mut self) -> Result<()>;
    /// Mark the slot pending with automatic rollback.
    async fn mark_pending(&mut self, software_version: u32) -> Result<()>;
    /// Confirm the running image.
    async fn confirm(&mut self) -> Result<()>;
    /// Erase the inactive slot.
    async fn abort(&mut self) -> Result<()>;
    /// Select the previous confirmed image.
    async fn rollback(&mut self) -> Result<()>;
}

/// ESP32-S3 OTA slot adapter.
pub struct Esp32S3OtaSlot<B> {
    boot: B,
}

impl<B> Esp32S3OtaSlot<B> {
    /// Create an OTA slot adapter.
    pub const fn new(boot: B) -> Self {
        Self { boot }
    }
}

impl<B: EspOtaBootControl> ariel_os_matter_core::ota::OtaSlot for Esp32S3OtaSlot<B> {
    async fn begin(&mut self, image: &ariel_os_matter_core::ota::OtaImage) -> Result<()> {
        self.boot.begin(image.image_size).await
    }

    async fn write(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        self.boot.write(offset, data).await
    }

    async fn finish(&mut self) -> Result<()> {
        self.boot.finish().await
    }

    async fn mark_pending(&mut self, image: &ariel_os_matter_core::ota::OtaImage) -> Result<()> {
        self.boot.mark_pending(image.software_version).await
    }

    async fn confirm_running(&mut self) -> Result<()> {
        self.boot.confirm().await
    }

    async fn abort(&mut self) -> Result<()> {
        self.boot.abort().await
    }

    async fn rollback(&mut self) -> Result<()> {
        self.boot.rollback().await
    }
}
