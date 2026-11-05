#![warn(missing_docs)]

use crate::{DomainError, EntityId, JobStatus};

/// Detached job aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Job {
    id: EntityId,
    tool_call_id: EntityId,
    status: JobStatus,
}
impl Job {
    /// Creates a running job associated with a tool call.
    pub fn new(id: EntityId, tool_call_id: EntityId) -> Self {
        Self {
            id,
            tool_call_id,
            status: JobStatus::Running,
        }
    }

    /// Returns the job identifier.
    pub fn id(&self) -> &EntityId {
        &self.id
    }

    /// Returns the originating tool-call identifier.
    pub fn tool_call_id(&self) -> &EntityId {
        &self.tool_call_id
    }

    /// Returns the current job status.
    pub fn status(&self) -> JobStatus {
        self.status
    }

    /// Applies a terminal transition.
    pub fn finish(&mut self, status: JobStatus) -> Result<(), DomainError> {
        if !self.status.can_transition_to(status) {
            return Err(DomainError::InvalidJobTransition {
                from: self.status,
                to: status,
            });
        }
        self.status = status;
        Ok(())
    }
}
