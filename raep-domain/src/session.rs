#![warn(missing_docs)]

use crate::{DomainError, EntityId, SessionStatus, Turn, TurnState};

/// Session aggregate containing the invariant that only one turn is active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    id: EntityId,
    agent_id: String,
    status: SessionStatus,
    active_turn: Option<Turn>,
}

impl Session {
    /// Creates an idle session.
    pub fn new(id: EntityId) -> Self {
        Self::new_with_agent(id, "default")
    }

    /// Creates an idle session owned by an agent.
    pub fn new_with_agent(id: EntityId, agent_id: impl Into<String>) -> Self {
        Self {
            id,
            agent_id: agent_id.into(),
            status: SessionStatus::Idle,
            active_turn: None,
        }
    }

    /// Returns the session identifier.
    pub fn id(&self) -> &EntityId {
        &self.id
    }

    /// Returns the effective agent identifier.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Returns the session status.
    pub fn status(&self) -> SessionStatus {
        self.status
    }

    /// Returns the active turn, if one exists.
    pub fn active_turn(&self) -> Option<&Turn> {
        self.active_turn.as_ref()
    }

    /// Starts a new queued turn.
    pub fn start_turn(&mut self, turn_id: EntityId) -> Result<(), DomainError> {
        if self.status == SessionStatus::Closed || self.active_turn.is_some() {
            return Err(DomainError::SessionBusy);
        }
        self.status = SessionStatus::Active;
        self.active_turn = Some(Turn::new(turn_id));
        Ok(())
    }

    /// Applies a transition to the active turn.
    pub fn transition_turn(&mut self, next: TurnState) -> Result<(), DomainError> {
        let turn = self.active_turn.as_mut().ok_or(DomainError::SessionBusy)?;
        turn.transition(next)?;
        if next.is_terminal() {
            self.status = SessionStatus::Idle;
            self.active_turn = None;
        }
        Ok(())
    }

    /// Closes an idle session.
    pub fn close(&mut self) -> Result<(), DomainError> {
        if self.active_turn.is_some() {
            return Err(DomainError::SessionBusy);
        }
        self.status = SessionStatus::Closed;
        Ok(())
    }
}
