//! Typed application APIs for Matter device behavior.
//!
//! Applications use these values and commands. The `rs-matter` adapter owns
//! protocol paths and TLV encoding.

use heapless::{String, Vec};

use crate::{Error, Result};

/// Maximum mode-label length.
pub const MAX_MODE_LABEL: usize = 32;
/// Maximum lock users.
pub const MAX_LOCK_USERS: usize = 64;
/// Maximum lock credentials.
pub const MAX_LOCK_CREDENTIALS: usize = 128;

/// Common On/Off state for lights, plugs, mounted controls, and switches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnOffState {
    /// Current state.
    pub on: bool,
    /// State restored after a restart.
    pub startup_on: Option<bool>,
    /// Monotonic data generation.
    pub generation: u64,
}

impl OnOffState {
    /// Create state.
    #[must_use]
    pub const fn new(on: bool) -> Self {
        Self {
            on,
            startup_on: None,
            generation: 0,
        }
    }

    /// Set the state.
    pub fn set(&mut self, on: bool) {
        if self.on != on {
            self.on = on;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// Toggle the state.
    pub fn toggle(&mut self) {
        self.set(!self.on);
    }
}

/// Dimming level. Matter uses 0 through 254.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Level(u8);

impl Level {
    /// Create a valid level.
    pub fn new(value: u8) -> Result<Self> {
        if value <= 254 {
            Ok(Self(value))
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Return the Matter value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Level-control state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelState {
    /// Current level.
    pub current: Level,
    /// Target level during a transition.
    pub target: Level,
    /// Remaining transition time in milliseconds.
    pub remaining_ms: u32,
    /// Monotonic data generation.
    pub generation: u64,
}

impl LevelState {
    /// Create level state.
    #[must_use]
    pub const fn new(level: Level) -> Self {
        Self {
            current: level,
            target: level,
            remaining_ms: 0,
            generation: 0,
        }
    }

    /// Start a transition.
    pub fn move_to(&mut self, target: Level, transition_ms: u32) {
        self.target = target;
        self.remaining_ms = transition_ms;
        if transition_ms == 0 {
            self.current = target;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Complete or update a transition.
    pub fn update(&mut self, current: Level, remaining_ms: u32) -> Result<()> {
        if remaining_ms > self.remaining_ms && self.remaining_ms != 0 {
            return Err(Error::InvalidArgument);
        }
        self.current = current;
        self.remaining_ms = remaining_ms;
        if remaining_ms == 0 {
            self.target = current;
        }
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// CIE xy color coordinate in the Matter range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorXy {
    /// X coordinate.
    pub x: u16,
    /// Y coordinate.
    pub y: u16,
}

/// Color-control state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorState {
    /// Current xy color.
    pub xy: ColorXy,
    /// Color temperature in mireds.
    pub color_temperature_mireds: u16,
    /// Enhanced hue.
    pub enhanced_hue: u16,
    /// Saturation from 0 through 254.
    pub saturation: u8,
    /// Monotonic data generation.
    pub generation: u64,
}

impl ColorState {
    /// Create color state.
    pub fn new(xy: ColorXy, color_temperature_mireds: u16) -> Result<Self> {
        if color_temperature_mireds == 0 {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            xy,
            color_temperature_mireds,
            enhanced_hue: 0,
            saturation: 0,
            generation: 0,
        })
    }

    /// Set xy color.
    pub fn set_xy(&mut self, xy: ColorXy) {
        self.xy = xy;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Set color temperature.
    pub fn set_temperature(&mut self, mireds: u16) -> Result<()> {
        if mireds == 0 {
            return Err(Error::InvalidArgument);
        }
        self.color_temperature_mireds = mireds;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Generic fixed-point sensor sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SensorSample {
    /// Signed value.
    pub value: i64,
    /// Decimal exponent. A value of `-2` means hundredths.
    pub exponent: i8,
    /// Monotonic sample time in milliseconds.
    pub timestamp_ms: u64,
    /// Measurement is valid.
    pub valid: bool,
}

/// Generic sensor state used by temperature, humidity, pressure, flow, light,
/// air-quality, electrical, soil, rain, freeze, leak, occupancy, and contact
/// devices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SensorState {
    /// Latest sample.
    pub sample: SensorSample,
    /// Optional low alarm threshold.
    pub low_alarm: Option<i64>,
    /// Optional high alarm threshold.
    pub high_alarm: Option<i64>,
    /// Monotonic data generation.
    pub generation: u64,
}

impl SensorState {
    /// Create sensor state.
    #[must_use]
    pub const fn new(sample: SensorSample) -> Self {
        Self {
            sample,
            low_alarm: None,
            high_alarm: None,
            generation: 0,
        }
    }

    /// Publish a sample. Timestamps cannot move backwards.
    pub fn publish(&mut self, sample: SensorSample) -> Result<()> {
        if sample.timestamp_ms < self.sample.timestamp_ms {
            return Err(Error::InvalidArgument);
        }
        self.sample = sample;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Return true when an active threshold is crossed.
    #[must_use]
    pub fn alarm_active(&self) -> bool {
        self.sample.valid
            && (self
                .low_alarm
                .is_some_and(|threshold| self.sample.value <= threshold)
                || self
                    .high_alarm
                    .is_some_and(|threshold| self.sample.value >= threshold))
    }
}

/// Door-lock operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockState {
    /// Bolt is locked.
    Locked,
    /// Bolt is unlocked.
    Unlocked,
    /// Bolt is moving.
    NotFullyLocked,
    /// Position cannot be determined.
    Unknown,
}

/// Door-lock command source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockOperation {
    /// Lock the door.
    Lock,
    /// Unlock the door.
    Unlock,
    /// Unlatch without leaving the door unlocked.
    Unbolt,
}

/// Lock user status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockUserStatus {
    /// User slot is unused.
    Available,
    /// User is active.
    Occupied,
    /// User is disabled.
    Disabled,
}

/// Lock user record without secret credential material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockUser {
    /// User index.
    pub index: u16,
    /// User name.
    pub name: String<32>,
    /// Status.
    pub status: LockUserStatus,
    /// Unique credential references in secure storage.
    pub credential_refs: Vec<u32, 8>,
}

/// Typed lock database and state.
pub struct LockDatabase {
    /// Current physical state.
    pub state: LockState,
    /// Auto-relock delay.
    pub auto_relock_seconds: u32,
    users: Vec<LockUser, MAX_LOCK_USERS>,
    credential_refs: Vec<u32, MAX_LOCK_CREDENTIALS>,
    generation: u64,
}

impl LockDatabase {
    /// Create an empty lock database.
    #[must_use]
    pub const fn new(state: LockState) -> Self {
        Self {
            state,
            auto_relock_seconds: 0,
            users: Vec::new(),
            credential_refs: Vec::new(),
            generation: 0,
        }
    }

    /// Add or replace a user.
    pub fn set_user(&mut self, user: LockUser) -> Result<()> {
        if user.index == 0 {
            return Err(Error::InvalidArgument);
        }
        if let Some(existing) = self.users.iter_mut().find(|item| item.index == user.index) {
            *existing = user;
        } else {
            self.users.push(user).map_err(|_| Error::Capacity)?;
        }
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Remove a user and all non-exportable credential references assigned to it.
    pub fn clear_user(&mut self, index: u16) -> Result<Vec<u32, 8>> {
        let position = self
            .users
            .iter()
            .position(|user| user.index == index)
            .ok_or(Error::NotFound)?;
        let user = self.users.swap_remove(position);
        for reference in &user.credential_refs {
            if let Some(position) = self
                .credential_refs
                .iter()
                .position(|stored| stored == reference)
            {
                self.credential_refs.swap_remove(position);
            }
        }
        self.generation = self.generation.wrapping_add(1);
        Ok(user.credential_refs)
    }

    /// Register an opaque secure credential reference.
    pub fn add_credential_ref(&mut self, reference: u32) -> Result<()> {
        if reference == 0 || self.credential_refs.contains(&reference) {
            return Err(Error::Conflict);
        }
        self.credential_refs
            .push(reference)
            .map_err(|_| Error::Capacity)?;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Return one user.
    #[must_use]
    pub fn user(&self, index: u16) -> Option<&LockUser> {
        self.users.iter().find(|user| user.index == index)
    }

    /// Update the physical state after the actuator confirms an operation.
    pub fn operation_completed(&mut self, operation: LockOperation) {
        self.state = match operation {
            LockOperation::Lock => LockState::Locked,
            LockOperation::Unlock | LockOperation::Unbolt => LockState::Unlocked,
        };
        self.generation = self.generation.wrapping_add(1);
    }
}

/// Thermostat system mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThermostatMode {
    /// System is off.
    Off,
    /// Automatic heat/cool selection.
    Auto,
    /// Cooling only.
    Cool,
    /// Heating only.
    Heat,
    /// Emergency heat.
    EmergencyHeat,
    /// Fan only.
    FanOnly,
    /// Dry/dehumidify.
    Dry,
}

/// Thermostat state in hundredths of a degree Celsius.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThermostatState {
    /// Measured local temperature.
    pub local_temperature: i16,
    /// Occupied heating setpoint.
    pub heating_setpoint: i16,
    /// Occupied cooling setpoint.
    pub cooling_setpoint: i16,
    /// Minimum deadband between heating and cooling setpoints.
    pub minimum_deadband: u16,
    /// System mode.
    pub mode: ThermostatMode,
    /// Monotonic data generation.
    pub generation: u64,
}

impl ThermostatState {
    /// Create valid thermostat state.
    pub fn new(
        local_temperature: i16,
        heating_setpoint: i16,
        cooling_setpoint: i16,
        minimum_deadband: u16,
    ) -> Result<Self> {
        let state = Self {
            local_temperature,
            heating_setpoint,
            cooling_setpoint,
            minimum_deadband,
            mode: ThermostatMode::Off,
            generation: 0,
        };
        state.validate_setpoints(heating_setpoint, cooling_setpoint)?;
        Ok(state)
    }

    /// Change occupied setpoints.
    pub fn set_setpoints(&mut self, heating: i16, cooling: i16) -> Result<()> {
        self.validate_setpoints(heating, cooling)?;
        self.heating_setpoint = heating;
        self.cooling_setpoint = cooling;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Set the mode.
    pub fn set_mode(&mut self, mode: ThermostatMode) {
        if self.mode != mode {
            self.mode = mode;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn validate_setpoints(&self, heating: i16, cooling: i16) -> Result<()> {
        let deadband = i32::from(cooling) - i32::from(heating);
        if deadband < i32::from(self.minimum_deadband) {
            return Err(Error::InvalidArgument);
        }
        Ok(())
    }
}

/// Pump operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PumpOperationState {
    /// Pump is stopped.
    Stopped,
    /// Pump is running.
    Running,
    /// Pump has a fault.
    Fault,
}

/// Typed pump state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PumpState {
    /// Operating state.
    pub state: PumpOperationState,
    /// Requested speed in hundredths of one percent.
    pub speed: u16,
    /// Current flow in a device-defined fixed-point unit.
    pub flow: i64,
    /// Current pressure in a device-defined fixed-point unit.
    pub pressure: i64,
    /// Monotonic data generation.
    pub generation: u64,
}

impl PumpState {
    /// Set speed and state.
    pub fn set_speed(&mut self, speed: u16) -> Result<()> {
        if speed > 10_000 || self.state == PumpOperationState::Fault {
            return Err(Error::InvalidState);
        }
        self.speed = speed;
        self.state = if speed == 0 {
            PumpOperationState::Stopped
        } else {
            PumpOperationState::Running
        };
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Valve position and safety state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValveState {
    /// Current open percentage in hundredths of one percent.
    pub current_open: u16,
    /// Target open percentage.
    pub target_open: u16,
    /// Safety fault prevents opening.
    pub safety_fault: bool,
    /// Monotonic data generation.
    pub generation: u64,
}

impl ValveState {
    /// Set a valve target.
    pub fn set_target(&mut self, target: u16) -> Result<()> {
        if target > 10_000 || (self.safety_fault && target > self.current_open) {
            return Err(Error::InvalidState);
        }
        self.target_open = target;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Publish actuator position.
    pub fn update_position(&mut self, current: u16) -> Result<()> {
        if current > 10_000 {
            return Err(Error::InvalidArgument);
        }
        self.current_open = current;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Electrical measurement state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnergyState {
    /// RMS voltage in millivolts.
    pub voltage_mv: u32,
    /// RMS current in milliamperes.
    pub current_ma: i64,
    /// Active power in milliwatts.
    pub active_power_mw: i64,
    /// Imported energy in milliwatt-hours.
    pub imported_energy_mwh: u64,
    /// Exported energy in milliwatt-hours.
    pub exported_energy_mwh: u64,
    /// Monotonic data generation.
    pub generation: u64,
}

impl EnergyState {
    /// Publish a measurement. Cumulative energy cannot decrease.
    pub fn update(&mut self, next: Self) -> Result<()> {
        if next.imported_energy_mwh < self.imported_energy_mwh
            || next.exported_energy_mwh < self.exported_energy_mwh
        {
            return Err(Error::InvalidArgument);
        }
        let generation = self.generation.wrapping_add(1);
        *self = next;
        self.generation = generation;
        Ok(())
    }
}

/// EVSE operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvseState {
    /// No vehicle is connected.
    NotPluggedIn,
    /// Vehicle is connected but charging is disabled.
    PluggedInNoDemand,
    /// Vehicle requests power.
    PluggedInDemand,
    /// Charging is active.
    Charging,
    /// A fault prevents charging.
    Fault,
}

/// Typed EVSE controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvseControl {
    /// EVSE state.
    pub state: EvseState,
    /// User or grid authorization.
    pub enabled: bool,
    /// Maximum charging current in milliamperes.
    pub current_limit_ma: u32,
    /// Earliest allowed charge start in UTC seconds.
    pub charge_start_utc: Option<u64>,
    /// Required charge end in UTC seconds.
    pub charge_end_utc: Option<u64>,
    /// Monotonic data generation.
    pub generation: u64,
}

impl EvseControl {
    /// Enable charging within an optional time window.
    pub fn enable(
        &mut self,
        current_limit_ma: u32,
        start_utc: Option<u64>,
        end_utc: Option<u64>,
    ) -> Result<()> {
        if current_limit_ma == 0
            || matches!(self.state, EvseState::Fault | EvseState::NotPluggedIn)
            || matches!((start_utc, end_utc), (Some(start), Some(end)) if start >= end)
        {
            return Err(Error::InvalidState);
        }
        self.enabled = true;
        self.current_limit_ma = current_limit_ma;
        self.charge_start_utc = start_utc;
        self.charge_end_utc = end_utc;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Disable charging.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.generation = self.generation.wrapping_add(1);
    }
}

/// Common appliance operational state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplianceOperationState {
    /// Appliance is stopped.
    Stopped,
    /// Appliance is running a cycle.
    Running,
    /// Appliance is paused.
    Paused,
    /// Appliance is in an error state.
    Error,
}

/// Typed appliance cycle state for washer, dryer, dishwasher, oven, cooktop,
/// microwave, refrigerator, cabinet, hood, and room air conditioner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplianceState {
    /// Current operational state.
    pub state: ApplianceOperationState,
    /// Selected mode identifier.
    pub mode: u8,
    /// Current phase identifier.
    pub phase: u8,
    /// Remaining cycle time in seconds.
    pub remaining_seconds: Option<u32>,
    /// User-visible mode label.
    pub mode_label: String<MAX_MODE_LABEL>,
    /// Monotonic data generation.
    pub generation: u64,
}

impl ApplianceState {
    /// Create appliance state.
    pub fn new(mode: u8, mode_label: &str) -> Result<Self> {
        let mut label = String::new();
        label.push_str(mode_label).map_err(|_| Error::Capacity)?;
        if label.is_empty() {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            state: ApplianceOperationState::Stopped,
            mode,
            phase: 0,
            remaining_seconds: None,
            mode_label: label,
            generation: 0,
        })
    }

    /// Start a cycle.
    pub fn start(&mut self, remaining_seconds: Option<u32>) -> Result<()> {
        if self.state == ApplianceOperationState::Error {
            return Err(Error::InvalidState);
        }
        self.state = ApplianceOperationState::Running;
        self.remaining_seconds = remaining_seconds;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Pause a running cycle.
    pub fn pause(&mut self) -> Result<()> {
        if self.state != ApplianceOperationState::Running {
            return Err(Error::InvalidState);
        }
        self.state = ApplianceOperationState::Paused;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Stop a cycle.
    pub fn stop(&mut self) {
        self.state = ApplianceOperationState::Stopped;
        self.remaining_seconds = None;
        self.generation = self.generation.wrapping_add(1);
    }
}

/// Audio state for speaker, intercom, chime, and audio doorbell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioState {
    /// Mute state.
    pub muted: bool,
    /// Volume from 0 through 254.
    pub volume: u8,
    /// Monotonic data generation.
    pub generation: u64,
}

impl AudioState {
    /// Set volume.
    pub fn set_volume(&mut self, volume: u8) -> Result<()> {
        if volume > 254 {
            return Err(Error::InvalidArgument);
        }
        self.volume = volume;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Set mute.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.generation = self.generation.wrapping_add(1);
    }
}

/// Media playback state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackState {
    /// No content is active.
    NotPlaying,
    /// Content is playing.
    Playing,
    /// Content is paused.
    Paused,
    /// Content is buffering.
    Buffering,
}

/// Typed media player state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaState {
    /// Playback state.
    pub playback: PlaybackState,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Playback speed in thousandths, where 1000 is normal speed.
    pub speed_milli: i32,
    /// Monotonic data generation.
    pub generation: u64,
}

impl MediaState {
    /// Seek to a position.
    pub fn seek(&mut self, position_ms: u64) {
        self.position_ms = position_ms;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Set playback state and speed.
    pub fn set_playback(&mut self, playback: PlaybackState, speed_milli: i32) -> Result<()> {
        if playback == PlaybackState::Playing && speed_milli == 0 {
            return Err(Error::InvalidArgument);
        }
        self.playback = playback;
        self.speed_milli = speed_milli;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Application actuator for On/Off endpoints.
pub trait OnOffActuator {
    /// Apply output state.
    async fn set_on(&mut self, on: bool) -> Result<()>;
}

/// Application sensor source.
pub trait SensorSource {
    /// Read a fresh typed sample.
    async fn sample(&mut self) -> Result<SensorSample>;
}

/// Application lock actuator. Credential verification stays in secure platform
/// code and is represented by an authorization result.
pub trait LockActuator {
    /// Apply a physical lock operation.
    async fn operate(&mut self, operation: LockOperation, authorized: bool) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(value: u8) -> Level {
        match Level::new(value) {
            Ok(level) => level,
            Err(error) => std::panic::panic_any(error),
        }
    }

    #[test]
    fn thermostat_enforces_deadband() {
        let thermostat = ThermostatState::new(2_000, 1_900, 2_300, 200);
        assert!(thermostat.is_ok());
        if let Ok(mut thermostat) = thermostat {
            assert_eq!(
                thermostat.set_setpoints(2_100, 2_200),
                Err(Error::InvalidArgument)
            );
            assert_eq!(thermostat.set_setpoints(2_000, 2_300), Ok(()));
        }
    }

    #[test]
    fn energy_counters_never_move_back() {
        let mut current = EnergyState {
            voltage_mv: 230_000,
            current_ma: 1_000,
            active_power_mw: 230_000,
            imported_energy_mwh: 10_000,
            exported_energy_mwh: 20,
            generation: 0,
        };
        let mut next = current;
        next.imported_energy_mwh = 9_999;
        assert_eq!(current.update(next), Err(Error::InvalidArgument));
    }

    #[test]
    fn level_transition_tracks_target() {
        let mut state = LevelState::new(level(10));
        state.move_to(level(200), 1_000);
        assert_eq!(state.target, level(200));
        assert_eq!(state.update(level(100), 500), Ok(()));
        assert_eq!(state.update(level(200), 0), Ok(()));
        assert_eq!(state.current, level(200));
    }
}
