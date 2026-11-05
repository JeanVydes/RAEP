#![warn(missing_docs)]

use raep_domain::{DomainEvent, EntityId, Session};
use std::error::Error;
use thiserror::Error;

/// Error contract implemented by every adapter boundary.
pub trait AdapterError: Error + Send + Sync + 'static {}

impl<T> AdapterError for T where T: Error + Send + Sync + 'static {}

/// A durable event paired with its session-local sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedEvent {
    sequence: u64,
    event: DomainEvent,
}

impl RecordedEvent {
    /// Creates a recorded event. Adapters assign the sequence.
    pub fn new(sequence: u64, event: DomainEvent) -> Self {
        Self { sequence, event }
    }

    /// Returns the session-local sequence.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the durable domain event.
    pub fn event(&self) -> &DomainEvent {
        &self.event
    }
}

/// Atomic session boundary for aggregate and event persistence.
///
/// Implementations must make `commit` all-or-nothing. This is the boundary
/// that lets an adapter use a transaction, an event store, or another durable
/// mechanism without exposing that choice to the application layer.
pub trait SessionStore {
    /// Adapter-specific failure.
    type Error: AdapterError;

    /// Loads the current aggregate.
    fn load(&self, id: &EntityId) -> Result<Option<Session>, Self::Error>;

    /// Persists the aggregate and its events as one logical commit.
    fn commit(
        &mut self,
        session: Session,
        events: Vec<DomainEvent>,
    ) -> Result<Vec<RecordedEvent>, Self::Error>;

    /// Reads retained events strictly after a session-local sequence.
    fn events_after(
        &self,
        id: &EntityId,
        sequence: u64,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, Self::Error>;
}

/// Clock port for deterministic application behavior.
pub trait Clock {
    /// Returns Unix epoch milliseconds.
    fn now_millis(&self) -> i64;
}

/// Errors raised by a session store boundary.
#[derive(Debug, Error)]
pub enum StoreError<E: AdapterError> {
    /// The adapter rejected or failed the atomic commit.
    #[error("session store failed: {0}")]
    Adapter(E),
}
