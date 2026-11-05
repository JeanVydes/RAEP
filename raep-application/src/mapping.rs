#![warn(missing_docs)]

use raep_domain::{JobStatus, SessionStatus, TurnState};
use raep_types::{JobState, SessionState, TurnState as WireTurnState};

/// Maps a domain session status to its wire representation.
pub fn session_state(status: SessionStatus) -> SessionState {
    match status {
        SessionStatus::Idle | SessionStatus::Active => SessionState::Active,
        SessionStatus::Closed => SessionState::Closed,
    }
}

/// Maps a domain turn state to its wire representation.
pub fn turn_state(state: TurnState) -> WireTurnState {
    match state {
        TurnState::Queued => WireTurnState::Queued,
        TurnState::CallingProvider => WireTurnState::CallingProvider,
        TurnState::ExecutingTool => WireTurnState::ExecutingTool,
        TurnState::Completed => WireTurnState::Completed,
        TurnState::Failed => WireTurnState::Failed,
        TurnState::Cancelled => WireTurnState::Cancelled,
    }
}

/// Maps a domain job status to its wire representation.
pub fn job_state(status: JobStatus) -> JobState {
    match status {
        JobStatus::Running => JobState::Running,
        JobStatus::Completed => JobState::Completed,
        JobStatus::Failed => JobState::Failed,
        JobStatus::Killed => JobState::Killed,
        JobStatus::TimedOut => JobState::TimedOut,
    }
}
