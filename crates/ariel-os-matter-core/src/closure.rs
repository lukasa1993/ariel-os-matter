//! Matter 1.6 Closure Control state and typed garage-door API.
//!
//! Position uses the Matter closure convention: `0` is fully open and `10000`
//! is fully closed.

use heapless::Vec;

use crate::{Error, Result};

/// Fully open position.
pub const FULLY_OPEN: ClosurePosition = ClosurePosition(0);
/// Fully closed position.
pub const FULLY_CLOSED: ClosurePosition = ClosurePosition(10_000);
/// Maximum number of simultaneous closure errors in the Matter model.
pub const MAX_CLOSURE_ERRORS: usize = 10;

/// Closure position in hundredths of one percent closed.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClosurePosition(u16);

impl ClosurePosition {
    /// Create a valid position.
    pub fn new(value: u16) -> Result<Self> {
        if value <= 10_000 {
            Ok(Self(value))
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Return the Matter position value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Return true at the fully open limit.
    #[must_use]
    pub const fn is_fully_open(self) -> bool {
        self.0 == 0
    }

    /// Return true at the fully closed limit.
    #[must_use]
    pub const fn is_fully_closed(self) -> bool {
        self.0 == 10_000
    }
}

/// Discrete current position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentPosition {
    /// Fully closed.
    FullyClosed,
    /// Fully open.
    FullyOpen,
    /// Between the two limits.
    PartiallyOpen,
}

/// Discrete target position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetPosition {
    /// Move to the fully closed limit.
    FullyClosed,
    /// Move to the fully open limit.
    FullyOpen,
    /// Move to an intermediate position.
    Partial(ClosurePosition),
}

impl TargetPosition {
    fn position(self) -> ClosurePosition {
        match self {
            Self::FullyOpen => FULLY_OPEN,
            Self::FullyClosed => FULLY_CLOSED,
            Self::Partial(position) => position,
        }
    }
}

/// Active direction of movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Motion {
    /// The closure is not moving.
    Stopped,
    /// The closed percentage is decreasing.
    Opening,
    /// The closed percentage is increasing.
    Closing,
}

/// Matter Closure Control main state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainState {
    /// No movement is active.
    Stopped,
    /// The closure is moving.
    Moving,
    /// The closure is waiting before movement.
    WaitingForMotion,
    /// A fault prevents normal operation.
    Error,
    /// Calibration is active.
    Calibrating,
    /// Protection prevents movement.
    Protected,
    /// The actuator is disengaged.
    Disengaged,
    /// Setup or calibration is required.
    SetupRequired,
}

/// Matter Closure Control error value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosureError {
    /// A physical obstacle stopped movement.
    PhysicallyBlocked,
    /// A safety sensor blocks movement.
    BlockedBySensor,
    /// Temperature limits operation.
    TemperatureLimited,
    /// Service is required.
    MaintenanceRequired,
    /// An internal mechanism prevents movement.
    InternalInterference,
    /// The position sensor is unreliable.
    PositionSensorFault,
    /// The motor or drive failed.
    DriveFault,
}

/// Complete closure state visible to a typed application and the Matter adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureSnapshot {
    /// Continuous current position.
    pub current_position: ClosurePosition,
    /// Continuous target position.
    pub target_position: ClosurePosition,
    /// Discrete current position.
    pub current: CurrentPosition,
    /// Discrete target position.
    pub target: TargetPosition,
    /// Current direction.
    pub motion: Motion,
    /// Matter main state.
    pub main_state: MainState,
    /// Safety obstruction input.
    pub obstruction: bool,
    /// Active error list.
    pub errors: Vec<ClosureError, MAX_CLOSURE_ERRORS>,
    /// Monotonic state generation for persistence and subscriptions.
    pub generation: u64,
}

/// Typed command for a closure endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosureCommand {
    /// Open fully.
    Open,
    /// Close fully.
    Close,
    /// Stop at the current position.
    Stop,
    /// Move to a precise position.
    MoveTo(ClosurePosition),
}

/// Portable Closure Control state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureController {
    state: ClosureSnapshot,
}

impl ClosureController {
    /// Create a controller at a known position.
    #[must_use]
    pub fn new(position: ClosurePosition) -> Self {
        let current = classify(position);
        let target = target_from_position(position);
        Self {
            state: ClosureSnapshot {
                current_position: position,
                target_position: position,
                current,
                target,
                motion: Motion::Stopped,
                main_state: MainState::Stopped,
                obstruction: false,
                errors: Vec::new(),
                generation: 0,
            },
        }
    }

    /// Return a stable state snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &ClosureSnapshot {
        &self.state
    }

    /// Apply an open, close, stop, or move command.
    pub fn command(&mut self, command: ClosureCommand) -> Result<()> {
        match command {
            ClosureCommand::Open => self.move_to(FULLY_OPEN),
            ClosureCommand::Close => self.move_to(FULLY_CLOSED),
            ClosureCommand::Stop => self.stop(),
            ClosureCommand::MoveTo(position) => self.move_to(position),
        }
    }

    /// Start movement to a target.
    pub fn move_to(&mut self, target: ClosurePosition) -> Result<()> {
        if !self.state.errors.is_empty() {
            return Err(Error::InvalidState);
        }
        if self.state.main_state != MainState::Stopped && self.state.main_state != MainState::Moving
        {
            return Err(Error::InvalidState);
        }
        if self.state.obstruction && target > self.state.current_position {
            return Err(Error::Obstructed);
        }

        self.state.target_position = target;
        self.state.target = target_from_position(target);
        self.state.motion = if target < self.state.current_position {
            Motion::Opening
        } else if target > self.state.current_position {
            Motion::Closing
        } else {
            Motion::Stopped
        };
        self.state.main_state = if self.state.motion == Motion::Stopped {
            MainState::Stopped
        } else {
            MainState::Moving
        };
        self.bump_generation();
        Ok(())
    }

    /// Stop at the current position.
    pub fn stop(&mut self) -> Result<()> {
        if matches!(
            self.state.main_state,
            MainState::Calibrating | MainState::Disengaged | MainState::SetupRequired
        ) {
            return Err(Error::InvalidState);
        }
        self.state.target_position = self.state.current_position;
        self.state.target = target_from_position(self.state.current_position);
        self.state.motion = Motion::Stopped;
        self.state.main_state = if self.state.errors.is_empty() {
            MainState::Stopped
        } else {
            MainState::Error
        };
        self.bump_generation();
        Ok(())
    }

    /// Accept a position update from the actuator or position sensor.
    pub fn update_position(&mut self, position: ClosurePosition) -> Result<()> {
        match self.state.motion {
            Motion::Opening if position > self.state.current_position => {
                return Err(Error::InvalidArgument);
            }
            Motion::Closing if position < self.state.current_position => {
                return Err(Error::InvalidArgument);
            }
            Motion::Stopped | Motion::Opening | Motion::Closing => {}
        }

        self.state.current_position = position;
        self.state.current = classify(position);

        let reached_target = match self.state.motion {
            Motion::Opening => position <= self.state.target_position,
            Motion::Closing => position >= self.state.target_position,
            Motion::Stopped => false,
        };
        if reached_target {
            self.state.current_position = self.state.target_position;
            self.state.current = classify(self.state.target_position);
            self.state.motion = Motion::Stopped;
            self.state.main_state = MainState::Stopped;
        }
        self.bump_generation();
        Ok(())
    }

    /// Update the obstruction input.
    ///
    /// An obstruction during closing stops movement and sets both the
    /// obstruction state and a Matter error. Opening remains available so the
    /// device can move away from the obstruction.
    pub fn set_obstruction(&mut self, obstructed: bool) -> Result<()> {
        self.state.obstruction = obstructed;
        if obstructed {
            add_error(&mut self.state.errors, ClosureError::BlockedBySensor)?;
            if self.state.motion == Motion::Closing {
                self.state.target_position = self.state.current_position;
                self.state.target = target_from_position(self.state.current_position);
                self.state.motion = Motion::Stopped;
                self.state.main_state = MainState::Error;
            }
        } else {
            remove_error(&mut self.state.errors, ClosureError::BlockedBySensor);
            if self.state.errors.is_empty() && self.state.main_state == MainState::Error {
                self.state.main_state = MainState::Stopped;
            }
        }
        self.bump_generation();
        Ok(())
    }

    /// Set a fault and stop movement.
    pub fn set_fault(&mut self, error: ClosureError) -> Result<()> {
        add_error(&mut self.state.errors, error)?;
        self.state.target_position = self.state.current_position;
        self.state.target = target_from_position(self.state.current_position);
        self.state.motion = Motion::Stopped;
        self.state.main_state = MainState::Error;
        self.bump_generation();
        Ok(())
    }

    /// Clear one fault.
    pub fn clear_fault(&mut self, error: ClosureError) {
        remove_error(&mut self.state.errors, error);
        if self.state.errors.is_empty() && self.state.main_state == MainState::Error {
            self.state.main_state = MainState::Stopped;
        }
        self.bump_generation();
    }

    /// Put the closure in a protected state.
    pub fn set_protected(&mut self, protected: bool) {
        self.state.motion = Motion::Stopped;
        self.state.target_position = self.state.current_position;
        self.state.target = target_from_position(self.state.current_position);
        self.state.main_state = if protected {
            MainState::Protected
        } else if self.state.errors.is_empty() {
            MainState::Stopped
        } else {
            MainState::Error
        };
        self.bump_generation();
    }

    fn bump_generation(&mut self) {
        self.state.generation = self.state.generation.wrapping_add(1);
    }
}

/// Hardware actions required by the typed closure API.
pub trait ClosureActuator {
    /// Drive toward fully open or an intermediate open target.
    async fn open_to(&mut self, target: ClosurePosition) -> Result<()>;
    /// Drive toward fully closed or an intermediate closed target.
    async fn close_to(&mut self, target: ClosurePosition) -> Result<()>;
    /// Stop drive power and engage the safe holding state.
    async fn stop(&mut self) -> Result<()>;
}

/// Typed closure endpoint that coordinates the protocol state and actuator.
pub struct ClosureEndpoint<A> {
    controller: ClosureController,
    actuator: A,
}

impl<A> ClosureEndpoint<A>
where
    A: ClosureActuator,
{
    /// Create an endpoint.
    pub const fn new(controller: ClosureController, actuator: A) -> Self {
        Self {
            controller,
            actuator,
        }
    }

    /// Return the current Matter state.
    #[must_use]
    pub const fn state(&self) -> &ClosureSnapshot {
        self.controller.snapshot()
    }

    /// Execute a typed command. State changes are committed only after the
    /// actuator accepts the action.
    pub async fn execute(&mut self, command: ClosureCommand) -> Result<()> {
        let mut planned = self.controller.clone();
        planned.command(command)?;

        match planned.snapshot().motion {
            Motion::Opening => {
                self.actuator
                    .open_to(planned.snapshot().target_position)
                    .await?
            }
            Motion::Closing => {
                self.actuator
                    .close_to(planned.snapshot().target_position)
                    .await?
            }
            Motion::Stopped => self.actuator.stop().await?,
        }

        self.controller = planned;
        Ok(())
    }

    /// Apply a hardware position update.
    pub fn update_position(&mut self, position: ClosurePosition) -> Result<()> {
        self.controller.update_position(position)
    }

    /// Apply the safety obstruction input.
    pub async fn set_obstruction(&mut self, obstructed: bool) -> Result<()> {
        if obstructed && self.controller.snapshot().motion == Motion::Closing {
            self.actuator.stop().await?;
        }
        self.controller.set_obstruction(obstructed)
    }

    /// Split the endpoint into its state machine and actuator.
    #[must_use]
    pub fn into_parts(self) -> (ClosureController, A) {
        (self.controller, self.actuator)
    }
}

fn classify(position: ClosurePosition) -> CurrentPosition {
    if position.is_fully_open() {
        CurrentPosition::FullyOpen
    } else if position.is_fully_closed() {
        CurrentPosition::FullyClosed
    } else {
        CurrentPosition::PartiallyOpen
    }
}

fn target_from_position(position: ClosurePosition) -> TargetPosition {
    if position.is_fully_open() {
        TargetPosition::FullyOpen
    } else if position.is_fully_closed() {
        TargetPosition::FullyClosed
    } else {
        TargetPosition::Partial(position)
    }
}

fn add_error(
    errors: &mut Vec<ClosureError, MAX_CLOSURE_ERRORS>,
    error: ClosureError,
) -> Result<()> {
    if !errors.contains(&error) {
        errors.push(error).map_err(|_| Error::Capacity)?;
    }
    Ok(())
}

fn remove_error(errors: &mut Vec<ClosureError, MAX_CLOSURE_ERRORS>, error: ClosureError) {
    if let Some(index) = errors.iter().position(|item| *item == error) {
        errors.swap_remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(value: u16) -> ClosurePosition {
        match ClosurePosition::new(value) {
            Ok(position) => position,
            Err(error) => std::panic::panic_any(error),
        }
    }

    #[test]
    fn open_close_stop_and_limits_are_consistent() {
        let mut closure = ClosureController::new(FULLY_CLOSED);
        assert_eq!(closure.command(ClosureCommand::Open), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Opening);
        assert_eq!(closure.snapshot().target, TargetPosition::FullyOpen);

        assert_eq!(closure.update_position(position(5_000)), Ok(()));
        assert_eq!(closure.snapshot().current, CurrentPosition::PartiallyOpen);
        assert_eq!(closure.command(ClosureCommand::Stop), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Stopped);
        assert_eq!(closure.snapshot().target_position, position(5_000));

        assert_eq!(closure.command(ClosureCommand::Close), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Closing);
        assert_eq!(closure.update_position(FULLY_CLOSED), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Stopped);
        assert_eq!(closure.snapshot().current, CurrentPosition::FullyClosed);
    }

    #[test]
    fn obstruction_stops_closing_and_allows_opening() {
        let mut closure = ClosureController::new(position(3_000));
        assert_eq!(closure.command(ClosureCommand::Close), Ok(()));
        assert_eq!(closure.set_obstruction(true), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Stopped);
        assert_eq!(closure.snapshot().main_state, MainState::Error);
        assert!(
            closure
                .snapshot()
                .errors
                .contains(&ClosureError::BlockedBySensor)
        );
        assert_eq!(
            closure.command(ClosureCommand::Close),
            Err(Error::InvalidState)
        );

        assert_eq!(closure.set_obstruction(false), Ok(()));
        assert_eq!(closure.command(ClosureCommand::Open), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Opening);
    }

    #[test]
    fn fault_is_fail_closed() {
        let mut closure = ClosureController::new(position(4_000));
        assert_eq!(closure.command(ClosureCommand::Open), Ok(()));
        assert_eq!(closure.set_fault(ClosureError::DriveFault), Ok(()));
        assert_eq!(closure.snapshot().motion, Motion::Stopped);
        assert_eq!(closure.snapshot().main_state, MainState::Error);
        assert_eq!(
            closure.command(ClosureCommand::Open),
            Err(Error::InvalidState)
        );
        closure.clear_fault(ClosureError::DriveFault);
        assert_eq!(closure.command(ClosureCommand::Open), Ok(()));
    }
}
