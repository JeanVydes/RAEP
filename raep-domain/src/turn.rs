#![warn(missing_docs)]

use crate::{DomainError, EntityId, TurnState};

impl TurnState {
    /// Returns whether the state is terminal.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(next, Self::CallingProvider | Self::Cancelled | Self::Failed),
            Self::CallingProvider => matches!(
                next,
                Self::ExecutingTool | Self::Completed | Self::Failed | Self::Cancelled
            ),
            Self::ExecutingTool => matches!(
                next,
                Self::CallingProvider | Self::Completed | Self::Failed | Self::Cancelled
            ),
            Self::Completed | Self::Failed | Self::Cancelled => false,
        }
    }
}

/// A turn and its explicit state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Turn {
    id: EntityId,
    state: TurnState,
}

impl Turn {
    /// Creates a queued turn.
    pub fn new(id: EntityId) -> Self {
        Self {
            id,
            state: TurnState::Queued,
        }
    }

    /// Returns the turn identifier.
    pub fn id(&self) -> &EntityId {
        &self.id
    }

    /// Returns the current state.
    pub fn state(&self) -> TurnState {
        self.state
    }

    /// Applies a valid lifecycle transition.
    pub fn transition(&mut self, next: TurnState) -> Result<(), DomainError> {
        if !self.state.can_transition_to(next) {
            return Err(DomainError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}
