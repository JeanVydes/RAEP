#![warn(missing_docs)]

/// Session lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    /// The session can accept a turn.
    Idle,
    /// The session has an active or queued turn.
    Active,
    /// The session has been closed.
    Closed,
}

/// Turn lifecycle defined by RAEP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnState {
    /// Input is accepted but not executing yet.
    Queued,
    /// The provider is generating output.
    CallingProvider,
    /// A tool is executing.
    ExecutingTool,
    /// The turn completed successfully.
    Completed,
    /// The turn failed.
    Failed,
    /// The turn was cancelled.
    Cancelled,
}

/// Lifecycle of a detached background job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    /// The job is executing.
    Running,
    /// The job completed successfully.
    Completed,
    /// The job failed.
    Failed,
    /// The job was terminated.
    Killed,
    /// The job exceeded its configured timeout.
    TimedOut,
}

impl JobStatus {
    /// Returns whether the job can no longer change state.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    pub(crate) fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Running,
                Self::Completed | Self::Failed | Self::Killed | Self::TimedOut
            )
        )
    }
}
