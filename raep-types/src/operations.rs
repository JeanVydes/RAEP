#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to cancel a detached job.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CancelJob {
    /// Optional cancellation reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A tool approval decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolApprovalResponse {
    /// Approval identifier.
    pub approval_id: String,
    /// Whether execution is approved.
    pub approved: bool,
    /// Optional reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A history page request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryGet {
    /// Return items after this cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Maximum number of items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// A generic history item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    /// Stable item identifier.
    pub id: String,
    /// Item kind.
    pub kind: String,
    /// Item content.
    pub content: Value,
    /// Creation timestamp.
    pub created_at: i64,
}

/// A history page response.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryPage {
    /// Returned items.
    pub items: Vec<HistoryItem>,
    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// A detached job query.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JobQuery {
    /// Optional status filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Maximum results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// A delegation request for a child session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DelegationRequest {
    /// Agent identifier.
    pub agent_id: String,
    /// Task passed to the child.
    pub task: String,
    /// Child lifetime policy.
    #[serde(default = "default_lifetime")]
    pub lifetime: String,
    /// Optional child budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<Value>,
}

/// Result returned by a child session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DelegationResult {
    /// Child session identifier.
    pub child_session_id: String,
    /// Final child state.
    pub state: String,
    /// Result payload.
    pub result: Value,
}

/// Out-of-band notification configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// Notification channel.
    pub channel: String,
    /// Adapter-owned endpoint identifier.
    pub endpoint_id: String,
    /// Events to publish.
    pub events: Vec<String>,
    /// Optional coalescing interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coalesce_s: Option<u64>,
}

/// Session management create request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionCreate {
    /// Agent identifier.
    pub agent_id: String,
    /// Optional caller metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Session management patch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionPatch {
    /// New title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Labels to set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    /// Archive state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

fn default_lifetime() -> String {
    "turn".to_owned()
}
