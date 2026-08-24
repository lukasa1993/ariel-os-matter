//! Error values used by the portable Matter subsystem.

/// A portable Matter subsystem error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Authentication or authorization failed.
    AccessDenied,
    /// A fixed-capacity collection has no free entry.
    Capacity,
    /// A requested item already exists.
    Conflict,
    /// Stored data failed an integrity check.
    Corrupt,
    /// A cryptographic operation failed.
    Crypto,
    /// The device is busy with another operation.
    Busy,
    /// A hardware or network input is invalid.
    InvalidArgument,
    /// The requested state transition is not permitted.
    InvalidState,
    /// The requested item does not exist.
    NotFound,
    /// A network operation failed.
    Network,
    /// The operation is not supported by the active platform.
    NotSupported,
    /// An obstruction prevents safe movement.
    Obstructed,
    /// Persistent storage failed.
    Storage,
    /// The operation did not complete before its deadline.
    Timeout,
    /// A secure secret store rejected the operation.
    SecureStorage,
    /// An update image failed verification.
    Verification,
}

/// A portable result type.
pub type Result<T> = core::result::Result<T, Error>;
