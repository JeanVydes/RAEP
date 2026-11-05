#![warn(missing_docs)]

use crate::{EntityId, TurnState};

/// A state-changing event emitted by an aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainEvent {
    occurred_at: i64,
    kind: DomainEventKind,
}

impl DomainEvent {
    /// Creates an event with the supplied event time.
    pub fn new(occurred_at: i64, kind: DomainEventKind) -> Self {
        Self { occurred_at, kind }
    }

    /// Returns the event timestamp.
    pub fn occurred_at(&self) -> i64 {
        self.occurred_at
    }

    /// Returns the event kind.
    pub fn kind(&self) -> &DomainEventKind {
        &self.kind
    }
}

/// Kinds of durable state-changing events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainEventKind {
    /// A session was created.
    SessionCreated {
        /// Created session identifier.
        session_id: EntityId,
    },
    /// A turn was accepted.
    TurnStarted {
        /// Session containing the turn.
        session_id: EntityId,
        /// Accepted turn identifier.
        turn_id: EntityId,
    },
    /// A turn changed lifecycle state.
    TurnStateChanged {
        /// Session containing the turn.
        session_id: EntityId,
        /// Turn whose state changed.
        turn_id: EntityId,
        /// New lifecycle state.
        state: TurnState,
    },
    /// A session was closed.
    SessionClosed {
        /// Closed session identifier.
        session_id: EntityId,
    },
}
