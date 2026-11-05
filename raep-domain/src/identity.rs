#![warn(missing_docs)]

use crate::{JobStatus, TurnState};
use thiserror::Error;

/// A domain identifier with no wire or storage assumptions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(String);

impl EntityId {
    /// Creates a bounded non-empty identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.chars().count() > 128 {
            return Err(DomainError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lifecycle errors raised by domain state machines.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    /// An identifier was empty or too long.
    #[error("identifier must contain between 1 and 128 characters")]
    InvalidIdentifier,
    /// A turn cannot be started while another one is active.
    #[error("session already has an active turn")]
    SessionBusy,
    /// The requested session does not exist in the configured repository.
    #[error("session was not found")]
    SessionNotFound,
    /// A transition is not valid from the current state.
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// State before the attempted transition.
        from: TurnState,
        /// State requested by the caller.
        to: TurnState,
    },
    /// A background job transition is not valid from its current state.
    #[error("invalid job transition from {from:?} to {to:?}")]
    InvalidJobTransition {
        /// State before the attempted transition.
        from: JobStatus,
        /// State requested by the caller.
        to: JobStatus,
    },
}
/// A permission scope used by authorization policies.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Scope(String);

impl Scope {
    /// Creates a non-empty scope.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.chars().count() > 128 {
            return Err(DomainError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    /// Returns the scope text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
