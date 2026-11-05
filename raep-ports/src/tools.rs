#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_domain::EntityId;
use raep_types::{ToolDescriptor, Usage};
use serde_json::Value;

/// Request passed to a server-side tool executor.
pub struct ToolRequest {
    /// Tool metadata.
    pub descriptor: ToolDescriptor,
    /// Tool-call identifier.
    pub tool_call_id: EntityId,
    /// Decoded arguments.
    pub arguments: Value,
}

/// Result returned by a tool executor.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    /// Tool-call identifier.
    pub tool_call_id: EntityId,
    /// Whether the tool succeeded.
    pub succeeded: bool,
    /// Serialized result content.
    pub content: Value,
    /// Usage charged to the tool, when applicable.
    pub usage: Option<Usage>,
}

/// Server-side tool execution port.
pub trait ToolExecutorPort {
    /// Adapter-specific execution failure.
    type Error: AdapterError;

    /// Executes one tool call.
    fn execute<'a>(
        &'a mut self,
        request: ToolRequest,
    ) -> BoxFuture<'a, Result<ToolResult, Self::Error>>;
}
