#![warn(missing_docs)]

use crate::{StopReason, TurnState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Turn cancellation payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelTurn {
    /// Optional human-readable reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Acknowledgement payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    /// Highest contiguous sequence processed by the client.
    pub last_seq: u64,
}

/// Normalized protocol error payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorPayload {
    /// Stable protocol error code.
    pub code: String,
    /// Human-readable diagnostic.
    pub message: String,
    /// Whether retrying may succeed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Structured safe-to-expose details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Turn lifecycle event payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnStateChanged {
    /// New turn state.
    pub state: TurnState,
    /// Why the turn stopped, when terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// Provider usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// Normalized error, when failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}
