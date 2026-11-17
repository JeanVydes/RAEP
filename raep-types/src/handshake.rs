#![warn(missing_docs)]

use crate::Deprecation;
use serde::{Deserialize, Serialize};

/// A known standard message or a namespaced extension.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FrameType {
    /// A standard RAEP message.
    Standard(StandardFrameType),
    /// An implementation-defined extension message.
    Extension(String),
}

/// Standard message types covered by the initial public contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StandardFrameType {
    /// Server handshake.
    Hello,
    /// Client session attachment.
    SessionAttach,
    /// Server attachment result.
    SessionAttached,
    /// Server session snapshot.
    SessionState,
    /// Client turn input.
    UserInput,
    /// Client turn cancellation.
    CancelTurn,
    /// Incremental model output.
    TokenDelta,
    /// Turn lifecycle state change.
    TurnStateChanged,
    /// Tool invocation started.
    ToolCallStarted,
    /// Incremental tool output.
    ToolOutputDelta,
    /// Tool invocation finished.
    ToolCallFinished,
    /// Detached job started.
    JobStarted,
    /// Detached job output.
    JobProgressDelta,
    /// Detached job finished.
    JobFinished,
    /// Client acknowledgement.
    Ack,
    /// Protocol or application error.
    Error,
    /// Client detachment.
    SessionDetach,
    /// Client session close.
    SessionClose,
    /// Server confirmation that a session closed.
    SessionClosed,
    /// Client job cancellation.
    CancelJob,
    /// Tool approval response.
    ToolApprovalResponse,
    /// Job listing request.
    JobList,
    /// Job listing response.
    JobListResult,
    /// Detached job state.
    JobState,
    /// Job lookup request.
    JobGet,
    /// History page request.
    HistoryGet,
    /// History page response.
    HistoryPage,
    /// Session fork request.
    SessionFork,
    /// Session rewind request.
    SessionRewind,
    /// Context compaction request.
    SessionCompact,
    /// Context pin request.
    ContextPin,
    /// Realtime input stream start.
    InputStreamStart,
    /// Realtime input stream data.
    InputStreamData,
    /// Realtime input stream end.
    InputStreamEnd,
    /// Client-side tool result.
    ClientToolResult,
    /// Presence update.
    PresenceUpdate,
    /// Session update notification.
    SessionUpdated,
    /// Session fork notification.
    SessionForked,
    /// Accepted user input notification.
    UserInputAccepted,
    /// Provider retry notification.
    ProviderRetry,
    /// Approval resolution notification.
    ApprovalResolved,
    /// Client-side tool request.
    ClientToolRequest,
    /// History rewind notification.
    HistoryRewound,
    /// Context update notification.
    ContextUpdated,
    /// Quota warning notification.
    QuotaWarning,
    /// Server draining notification.
    ServerDraining,
    /// Presence notification.
    Presence,
    /// Authentication is expiring.
    AuthExpiring,
    /// Authentication refresh request.
    AuthRefresh,
    /// Authentication refresh result.
    AuthRefreshed,
    /// Session configuration request.
    SessionConfigure,
    /// Session configuration result.
    SessionConfigured,
    /// Agent changed notification.
    AgentChanged,
    /// Budget warning notification.
    BudgetWarning,
    /// History compacted notification.
    HistoryCompacted,
    /// Child session started.
    ChildSessionStarted,
    /// Child session updated.
    ChildSessionUpdated,
    /// Delegator control request.
    DelegatorControl,
    /// Delegator state notification.
    DelegatorStateChanged,
    /// Handoff request.
    Handoff,
    /// Notification status.
    NotificationStatus,
    /// Output part started.
    OutputPartStarted,
    /// Output part delta.
    OutputPartDelta,
    /// Output part finished.
    OutputPartFinished,
    /// Session export request.
    SessionExport,
    /// Session export ready notification.
    SessionExportReady,
    /// Session import request.
    SessionImport,
    /// Session import commit request.
    ImportCommitRequest,
    /// Session import result.
    ImportResult,
}

/// Output modality a client can render.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    /// Plain text.
    Text,
    /// Image output.
    Image,
    /// Document output.
    Document,
    /// Audio output.
    Audio,
    /// Video output.
    Video,
}

/// Optional client feature negotiated during attachment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientFeature {
    /// Approval prompts.
    #[serde(rename = "approvals")]
    Approvals,
    /// Multipart output.
    #[serde(rename = "output.parts")]
    OutputParts,
    /// Presence information.
    #[serde(rename = "presence")]
    Presence,
    /// Realtime input streams.
    #[serde(rename = "input.stream")]
    InputStream,
}

/// Session attachment role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachMode {
    /// May issue controller commands.
    Controller,
    /// Receives events but cannot control the session.
    Observer,
}

/// Policy for input received while a turn is active.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueuePolicy {
    /// Reject the new input.
    Reject,
    /// Queue the new input.
    Queue,
    /// Cancel the current turn before accepting the input.
    Interrupt,
    /// Reach the request already in flight: the running turn keeps going and
    /// the input joins it before its next provider request. When no turn is
    /// running, the input starts one, like every other policy.
    Steer,
}

/// Core session states exposed in `SESSION_STATE`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session is active.
    #[serde(rename = "ACTIVE")]
    Active,
    /// Session is suspended.
    #[serde(rename = "SUSPENDED")]
    Suspended,
    /// Session is closed.
    #[serde(rename = "CLOSED")]
    Closed,
}

/// Turn states exposed by the wire protocol.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurnState {
    /// Input is queued.
    #[serde(rename = "QUEUED")]
    Queued,
    /// Provider generation is active.
    #[serde(rename = "CALLING_PROVIDER")]
    CallingProvider,
    /// A tool is executing.
    #[serde(rename = "EXECUTING_TOOL")]
    ExecutingTool,
    /// Human approval is pending.
    #[serde(rename = "AWAITING_APPROVAL")]
    AwaitingApproval,
    /// Turn completed.
    #[serde(rename = "COMPLETED")]
    Completed,
    /// Turn was cancelled.
    #[serde(rename = "CANCELLED")]
    Cancelled,
    /// Turn failed.
    #[serde(rename = "FAILED")]
    Failed,
}

/// Reason a turn stopped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Normal provider completion.
    EndTurn,
    /// Provider output limit.
    MaxTokens,
    /// Explicit cancellation.
    Cancelled,
    /// Unclassified failure.
    Error,
    /// Tool-call limit reached.
    ToolLimit,
    /// Budget limit reached.
    BudgetExceeded,
}

/// A bounded server limit advertised during handshake.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HelloLimits {
    /// Largest accepted frame.
    pub max_frame_bytes: Option<u64>,
    /// Largest prompt.
    pub max_prompt_bytes: Option<u64>,
    /// Maximum concurrent jobs.
    pub max_concurrent_jobs: Option<u64>,
    /// Retained replay events.
    pub replay_buffer_events: Option<u64>,
    /// Maximum queued inputs.
    pub max_queued_inputs: Option<u64>,
    /// Maximum commands per second.
    pub max_commands_per_s: Option<u64>,
}

/// Server handshake payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    /// Server implementation version.
    pub server_version: String,
    /// Wire versions supported by the server.
    pub wire_versions: Vec<String>,
    /// Advertised capabilities.
    pub capabilities: Vec<String>,
    /// Operational limits.
    pub limits: HelloLimits,
    /// Heartbeat interval in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_s: Option<u64>,
    /// Whether the server is draining.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draining: Option<bool>,
    /// Deprecated registry entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deprecations: Vec<Deprecation>,
}
