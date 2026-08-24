//! Ordered, fail-closed factory reset.

use crate::platform::{ResetControl, ResetReason};
use crate::{Error, Result};

/// Factory-reset phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryResetPhase {
    /// Stop protocol activity and reject new sessions.
    Quiesce,
    /// Remove runtime network credentials and Thread datasets.
    NetworkCredentials,
    /// Remove fabrics, ACLs, groups, and subscriptions.
    FabricState,
    /// Remove device, bridge, event, and commissioning state.
    DeviceState,
    /// Remove OTA state and pending images.
    OtaState,
    /// Destroy Matter secrets and hardware key handles.
    Secrets,
    /// Flush normal and secure storage.
    Sync,
    /// Restart the device.
    Reset,
}

/// Failure returned with the exact reset phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryResetFailure {
    /// Phase that failed.
    pub phase: FactoryResetPhase,
    /// Backend error.
    pub error: Error,
}

/// Platform and subsystem operations required for a factory reset.
pub trait FactoryResetBackend {
    /// Stop new Matter work and drain in-flight writes.
    async fn quiesce(&mut self) -> Result<()>;
    /// Erase Wi-Fi credentials and Thread datasets.
    async fn erase_network_credentials(&mut self) -> Result<()>;
    /// Erase fabrics, ACLs, groups, and subscriptions.
    async fn erase_fabric_state(&mut self) -> Result<()>;
    /// Erase commissioning, events, bridge data, and device state.
    async fn erase_device_state(&mut self) -> Result<()>;
    /// Erase pending image data and OTA state.
    async fn erase_ota_state(&mut self) -> Result<()>;
    /// Destroy secure Matter secrets and key handles.
    async fn erase_secrets(&mut self) -> Result<()>;
    /// Flush all erase operations.
    async fn sync(&mut self) -> Result<()>;
}

/// Factory-reset coordinator.
pub struct FactoryReset<B, R> {
    backend: B,
    reset: R,
}

impl<B, R> FactoryReset<B, R>
where
    B: FactoryResetBackend,
    R: ResetControl,
{
    /// Create a coordinator.
    pub const fn new(backend: B, reset: R) -> Self {
        Self { backend, reset }
    }

    /// Execute a complete reset.
    ///
    /// The device restarts only after every erase and both durability barriers
    /// succeed. A partial failure leaves the device quiesced for service. It
    /// does not restart into an ambiguous security state.
    pub async fn execute(
        &mut self,
    ) -> core::result::Result<core::convert::Infallible, FactoryResetFailure> {
        map_phase(self.backend.quiesce().await, FactoryResetPhase::Quiesce)?;
        map_phase(
            self.backend.erase_network_credentials().await,
            FactoryResetPhase::NetworkCredentials,
        )?;
        map_phase(
            self.backend.erase_fabric_state().await,
            FactoryResetPhase::FabricState,
        )?;
        map_phase(
            self.backend.erase_device_state().await,
            FactoryResetPhase::DeviceState,
        )?;
        map_phase(
            self.backend.erase_ota_state().await,
            FactoryResetPhase::OtaState,
        )?;
        map_phase(
            self.backend.erase_secrets().await,
            FactoryResetPhase::Secrets,
        )?;
        map_phase(self.backend.sync().await, FactoryResetPhase::Sync)?;
        self.reset
            .reset(ResetReason::FactoryReset)
            .await
            .map_err(|error| FactoryResetFailure {
                phase: FactoryResetPhase::Reset,
                error,
            })
    }

    /// Split the coordinator into platform components.
    #[must_use]
    pub fn into_parts(self) -> (B, R) {
        (self.backend, self.reset)
    }
}

fn map_phase(value: Result<()>, phase: FactoryResetPhase) -> core::result::Result<(), FactoryResetFailure> {
    value.map_err(|error| FactoryResetFailure { phase, error })
}
