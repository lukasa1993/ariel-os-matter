//! Runtime Wi-Fi management with transactional credential updates.

use heapless::{String, Vec};

use crate::{Error, Result};

/// Maximum number of scan results returned by the portable API.
pub const MAX_SCAN_RESULTS: usize = 32;
/// Maximum SSID length in octets.
pub const MAX_SSID_LEN: usize = 32;
/// Maximum WPA passphrase length in octets.
pub const MAX_PASSPHRASE_LEN: usize = 63;

/// Wi-Fi authentication mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiSecurity {
    /// An open network.
    Open,
    /// WPA2 Personal.
    Wpa2Personal,
    /// WPA3 Personal.
    Wpa3Personal,
    /// WPA2/WPA3 transition mode.
    Wpa2Wpa3Personal,
}

/// Wi-Fi secret material.
#[derive(Clone, Eq, PartialEq)]
pub enum WifiSecret {
    /// No secret is required.
    None,
    /// A WPA passphrase.
    Passphrase(String<MAX_PASSPHRASE_LEN>),
    /// A 256-bit pre-shared key.
    Psk([u8; 32]),
}

/// Runtime Wi-Fi credentials.
///
/// This type does not implement `Debug` because logs must not expose secrets.
#[derive(Clone, Eq, PartialEq)]
pub struct WifiCredentials {
    ssid: String<MAX_SSID_LEN>,
    security: WifiSecurity,
    secret: WifiSecret,
}

impl WifiCredentials {
    /// Create credentials for an open network.
    pub fn open(ssid: &str) -> Result<Self> {
        let ssid = copy_string::<MAX_SSID_LEN>(ssid)?;
        Ok(Self {
            ssid,
            security: WifiSecurity::Open,
            secret: WifiSecret::None,
        })
    }

    /// Create credentials with a WPA passphrase.
    pub fn personal(ssid: &str, security: WifiSecurity, passphrase: &str) -> Result<Self> {
        if matches!(security, WifiSecurity::Open)
            || !(8..=MAX_PASSPHRASE_LEN).contains(&passphrase.len())
        {
            return Err(Error::InvalidArgument);
        }

        Ok(Self {
            ssid: copy_string::<MAX_SSID_LEN>(ssid)?,
            security,
            secret: WifiSecret::Passphrase(copy_string::<MAX_PASSPHRASE_LEN>(passphrase)?),
        })
    }

    /// Create credentials with a raw 256-bit WPA key.
    pub fn psk(ssid: &str, security: WifiSecurity, psk: [u8; 32]) -> Result<Self> {
        if matches!(security, WifiSecurity::Open) {
            return Err(Error::InvalidArgument);
        }

        Ok(Self {
            ssid: copy_string::<MAX_SSID_LEN>(ssid)?,
            security,
            secret: WifiSecret::Psk(psk),
        })
    }

    /// Return the SSID.
    #[must_use]
    pub fn ssid(&self) -> &str {
        self.ssid.as_str()
    }

    /// Return the authentication mode.
    #[must_use]
    pub const fn security(&self) -> WifiSecurity {
        self.security
    }

    /// Give a Wi-Fi driver temporary access to the secret.
    #[must_use]
    pub const fn secret(&self) -> &WifiSecret {
        &self.secret
    }
}

fn copy_string<const N: usize>(value: &str) -> Result<String<N>> {
    let mut output = String::new();
    output.push_str(value).map_err(|_| Error::Capacity)?;
    if output.is_empty() {
        return Err(Error::InvalidArgument);
    }
    Ok(output)
}

/// A discovered Wi-Fi network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WifiScanResult {
    /// Network name.
    pub ssid: String<MAX_SSID_LEN>,
    /// Received signal strength in dBm.
    pub rssi_dbm: i8,
    /// Radio channel.
    pub channel: u8,
    /// Authentication mode.
    pub security: WifiSecurity,
}

/// Current Wi-Fi link state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiLinkState {
    /// The radio is stopped.
    Disabled,
    /// The radio is ready but is not connected.
    Disconnected,
    /// An association is in progress.
    Connecting,
    /// The station has an active association.
    Connected,
}

/// Chip adapter for runtime Wi-Fi operations.
pub trait WifiDriver {
    /// Scan for access points.
    async fn scan(&mut self) -> Result<Vec<WifiScanResult, MAX_SCAN_RESULTS>>;
    /// Connect with the supplied runtime credentials.
    async fn connect(&mut self, credentials: &WifiCredentials) -> Result<()>;
    /// Disconnect from the current access point.
    async fn disconnect(&mut self) -> Result<()>;
    /// Read the current link state.
    fn state(&self) -> WifiLinkState;
}

/// Persistent, secure Wi-Fi credential transaction.
pub trait WifiCredentialStore {
    /// Load the last committed credentials.
    async fn load_committed(&mut self) -> Result<Option<WifiCredentials>>;
    /// Write a candidate credential set without making it active.
    async fn stage(&mut self, credentials: &WifiCredentials) -> Result<()>;
    /// Atomically make the staged credentials active.
    async fn commit(&mut self) -> Result<()>;
    /// Discard the staged credentials.
    async fn rollback(&mut self) -> Result<()>;
    /// Remove committed and staged credentials.
    async fn remove_all(&mut self) -> Result<()>;
}

/// Transactional runtime Wi-Fi controller.
pub struct WifiManager<D, S> {
    driver: D,
    store: S,
}

impl<D, S> WifiManager<D, S>
where
    D: WifiDriver,
    S: WifiCredentialStore,
{
    /// Create a controller.
    pub const fn new(driver: D, store: S) -> Self {
        Self { driver, store }
    }

    /// Return the current link state.
    #[must_use]
    pub fn state(&self) -> WifiLinkState {
        self.driver.state()
    }

    /// Scan at runtime.
    pub async fn scan(&mut self) -> Result<Vec<WifiScanResult, MAX_SCAN_RESULTS>> {
        self.driver.scan().await
    }

    /// Connect with the committed credentials.
    pub async fn connect(&mut self) -> Result<()> {
        let credentials = self.store.load_committed().await?.ok_or(Error::NotFound)?;
        self.driver.connect(&credentials).await
    }

    /// Disconnect without deleting credentials.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.driver.disconnect().await
    }

    /// Test and atomically commit a new credential set.
    ///
    /// The previous committed set remains authoritative until the new network
    /// association succeeds. If the test fails, the manager restores the old
    /// set and reconnects it. A recovery failure returns `InvalidState` so the
    /// caller cannot mistake a damaged network state for a clean rollback.
    pub async fn update_credentials(&mut self, candidate: &WifiCredentials) -> Result<()> {
        let previous = self.store.load_committed().await?;
        self.store.stage(candidate).await?;

        if let Err(connect_error) = self.driver.connect(candidate).await {
            let disconnected = self.driver.disconnect().await.is_ok();
            let rolled_back = self.store.rollback().await.is_ok();
            let reconnected = match previous.as_ref() {
                Some(old) => self.driver.connect(old).await.is_ok(),
                None => true,
            };

            if disconnected && rolled_back && reconnected {
                return Err(connect_error);
            }
            return Err(Error::InvalidState);
        }

        if self.store.commit().await.is_err() {
            let disconnected = self.driver.disconnect().await.is_ok();
            let rolled_back = self.store.rollback().await.is_ok();
            let reconnected = match previous.as_ref() {
                Some(old) => self.driver.connect(old).await.is_ok(),
                None => true,
            };

            if disconnected && rolled_back && reconnected {
                return Err(Error::Storage);
            }
            return Err(Error::InvalidState);
        }

        Ok(())
    }

    /// Disconnect and remove all Wi-Fi credentials.
    pub async fn remove_credentials(&mut self) -> Result<()> {
        self.driver.disconnect().await?;
        self.store.remove_all().await
    }

    /// Split the manager into its platform objects.
    #[must_use]
    pub fn into_parts(self) -> (D, S) {
        (self.driver, self.store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_reject_invalid_secret_shapes() {
        assert_eq!(WifiCredentials::open(""), Err(Error::InvalidArgument));
        assert_eq!(
            WifiCredentials::personal("net", WifiSecurity::Open, "12345678"),
            Err(Error::InvalidArgument)
        );
        assert_eq!(
            WifiCredentials::personal("net", WifiSecurity::Wpa3Personal, "short"),
            Err(Error::InvalidArgument)
        );
    }

    #[test]
    fn credentials_accept_runtime_values() {
        let value = WifiCredentials::personal(
            "factory-floor",
            WifiSecurity::Wpa2Wpa3Personal,
            "correct horse battery staple",
        );
        assert!(value.is_ok());
    }
}
