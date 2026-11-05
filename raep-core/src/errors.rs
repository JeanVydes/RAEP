#![warn(missing_docs)]

use thiserror::Error;

/// Errors raised by delivery primitives.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DeliveryError {
    /// A replay window cannot have zero capacity.
    #[error("replay capacity must be greater than zero")]
    InvalidCapacity,
    /// A sequence was not strictly greater than the last appended sequence.
    #[error("sequence {sequence} is not greater than {last}")]
    NonMonotonic {
        /// Sequence supplied by the caller.
        sequence: u64,
        /// Last sequence already retained.
        last: u64,
    },
    /// The requested sequence is older than the retained window.
    #[error("replay starts at {requested}, but the first retained sequence is {first}")]
    ReplayTruncated {
        /// First sequence requested by the client.
        requested: u64,
        /// First sequence still retained.
        first: u64,
    },
}
