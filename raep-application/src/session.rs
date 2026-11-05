#![warn(missing_docs)]

use raep_domain::{DomainError, DomainEvent, DomainEventKind, EntityId, Session, TurnState};
use raep_ports::{Clock, RecordedEvent, SessionStore};
use thiserror::Error;

/// Errors returned by the session application service.
#[derive(Debug, Error)]
pub enum ApplicationError<RE: std::error::Error + Send + Sync + 'static> {
    /// The domain rejected the requested operation.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// The configured repository failed.
    #[error("repository failed: {0}")]
    Store(RE),
}

/// Result of a state-changing application command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandResult {
    session: Session,
    events: Vec<RecordedEvent>,
}

impl CommandResult {
    /// Creates a command result from the committed aggregate and events.
    pub fn new(session: Session, events: Vec<RecordedEvent>) -> Self {
        Self { session, events }
    }

    /// Returns the resulting session state.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Returns the events committed by the command.
    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }
}

/// Coordinates session lifecycle use cases without selecting adapters.
pub struct SessionService<S, C> {
    store: S,
    clock: C,
}

impl<S, C> SessionService<S, C>
where
    S: SessionStore,
    C: Clock,
{
    /// Creates a service from explicit ports.
    pub fn new(store: S, clock: C) -> Self {
        Self { store, clock }
    }

    /// Creates and persists an idle session.
    pub fn create_session(
        &mut self,
        session_id: EntityId,
    ) -> Result<CommandResult, ApplicationError<S::Error>> {
        let session = Session::new(session_id);
        let event = DomainEvent::new(
            self.clock.now_millis(),
            DomainEventKind::SessionCreated {
                session_id: session.id().clone(),
            },
        );
        let events = self
            .store
            .commit(session.clone(), vec![event])
            .map_err(ApplicationError::Store)?;
        Ok(CommandResult::new(session, events))
    }

    /// Starts a turn and persists the resulting aggregate.
    pub fn start_turn(
        &mut self,
        session_id: &EntityId,
        turn_id: EntityId,
    ) -> Result<CommandResult, ApplicationError<S::Error>> {
        let mut session = self
            .store
            .load(session_id)
            .map_err(ApplicationError::Store)?
            .ok_or(DomainError::SessionNotFound)?;
        let accepted_turn_id = turn_id.clone();
        session.start_turn(turn_id)?;
        let event = DomainEvent::new(
            self.clock.now_millis(),
            DomainEventKind::TurnStarted {
                session_id: session.id().clone(),
                turn_id: accepted_turn_id,
            },
        );
        let events = self
            .store
            .commit(session.clone(), vec![event])
            .map_err(ApplicationError::Store)?;
        Ok(CommandResult::new(session, events))
    }

    /// Applies a turn transition and persists the resulting aggregate.
    pub fn transition_turn(
        &mut self,
        session_id: &EntityId,
        state: TurnState,
    ) -> Result<CommandResult, ApplicationError<S::Error>> {
        let mut session = self
            .store
            .load(session_id)
            .map_err(ApplicationError::Store)?
            .ok_or(DomainError::SessionNotFound)?;
        let turn_id = session
            .active_turn()
            .ok_or(DomainError::SessionBusy)?
            .id()
            .clone();
        session.transition_turn(state)?;
        let event = DomainEvent::new(
            self.clock.now_millis(),
            DomainEventKind::TurnStateChanged {
                session_id: session.id().clone(),
                turn_id,
                state,
            },
        );
        let events = self
            .store
            .commit(session.clone(), vec![event])
            .map_err(ApplicationError::Store)?;
        Ok(CommandResult::new(session, events))
    }

    /// Closes an idle session and records the lifecycle event.
    pub fn close_session(
        &mut self,
        session_id: &EntityId,
    ) -> Result<CommandResult, ApplicationError<S::Error>> {
        let mut session = self
            .store
            .load(session_id)
            .map_err(ApplicationError::Store)?
            .ok_or(DomainError::SessionNotFound)?;
        session.close()?;
        let event = DomainEvent::new(
            self.clock.now_millis(),
            DomainEventKind::SessionClosed {
                session_id: session.id().clone(),
            },
        );
        let events = self
            .store
            .commit(session.clone(), vec![event])
            .map_err(ApplicationError::Store)?;
        Ok(CommandResult::new(session, events))
    }

    /// Reads retained events after a client acknowledgement.
    pub fn replay(
        &self,
        session_id: &EntityId,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, ApplicationError<S::Error>> {
        self.store
            .events_after(session_id, after_sequence, limit)
            .map_err(ApplicationError::Store)
    }
}
