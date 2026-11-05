#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider usage reported for a turn or an attempt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens consumed by the input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Tokens generated in the output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Input tokens served from cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// Reasoning tokens, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Tool calls counted by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u64>,
    /// Provider call duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Provider-specific usage dimensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

/// Where a tool invocation is executed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutor {
    /// Executed by the RAEP server.
    Server,
    /// Requested from the attached client.
    Client,
}

/// Tool risk classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolRisk {
    /// Read-only or otherwise low-risk operation.
    Low,
    /// Operation requiring policy evaluation.
    Medium,
    /// Sensitive operation requiring explicit approval.
    High,
}

/// Declared side effects of a tool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSideEffects {
    /// No state outside the tool call changes.
    None,
    /// Local workspace or process state can change.
    Local,
    /// External systems can change.
    External,
}

/// Public description of an executable tool.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Stable tool name.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON schema for tool arguments.
    pub input_schema: Value,
    /// JSON schema for tool output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    /// Execution side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<ToolExecutor>,
    /// Risk classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<ToolRisk>,
    /// Declared side effects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<ToolSideEffects>,
    /// Whether retrying can be safe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent: Option<bool>,
    /// Maximum execution time in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u64>,
    /// Required authorization scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Whether this tool may become a detached job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detachable: Option<bool>,
}

/// Detached job status exposed on the wire.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    /// Job is running.
    #[serde(rename = "RUNNING")]
    Running,
    /// Job completed.
    #[serde(rename = "COMPLETED")]
    Completed,
    /// Job failed.
    #[serde(rename = "FAILED")]
    Failed,
    /// Job was killed.
    #[serde(rename = "KILLED")]
    Killed,
    /// Job timed out.
    #[serde(rename = "TIMED_OUT")]
    TimedOut,
}

/// Result of a detached job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobResult {
    /// Final job state.
    pub status: JobState,
    /// Process exit code, when one exists.
    pub exit_code: Option<i32>,
    /// Safe human-readable summary.
    pub summary: String,
    /// Whether retained output was truncated.
    pub output_truncated: bool,
    /// Total output bytes produced.
    pub output_bytes_total: u64,
}

/// A bounded chunk of job output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputChunk {
    /// Stream name, normally `stdout` or `stderr`.
    pub stream: String,
    /// Chunk position within that stream.
    pub index: u64,
    /// UTF-8 output fragment.
    pub data: String,
}
