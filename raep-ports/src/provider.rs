#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_domain::EntityId;
use raep_types::{ToolDescriptor, Usage};
use serde_json::Value;

/// Input sent to a model provider.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRequest {
    /// Provider-specific model identifier.
    pub model: String,
    /// Serialized conversation context.
    pub messages: Vec<Value>,
    /// Tools available to the model.
    pub tools: Vec<ToolDescriptor>,
}

/// Normalized provider stream event.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelEvent {
    /// Incremental text emitted by the provider.
    TextDelta {
        /// Text fragment.
        delta: String,
        /// Zero-based fragment index.
        index: u64,
    },
    /// Provider requested a tool call.
    ToolCall {
        /// Tool-call identifier.
        tool_call_id: EntityId,
        /// Tool name.
        tool_name: String,
        /// Decoded arguments.
        arguments: Value,
    },
    /// Provider completed generation.
    Completed {
        /// Reported usage, if available.
        usage: Option<Usage>,
    },
}

/// Normalized provider failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFailure {
    /// Stable server-side error category.
    pub code: String,
    /// Safe diagnostic message.
    pub message: String,
    /// Whether retrying may succeed.
    pub retryable: bool,
}

/// Model provider port.
pub trait ModelProvider {
    /// Adapter-specific provider failure.
    type Error: AdapterError;
    /// Provider stream returned after a request is accepted.
    type Stream: ModelStream<Error = Self::Error>;

    /// Starts a streaming model generation.
    fn generate<'a>(
        &'a mut self,
        request: ModelRequest,
    ) -> BoxFuture<'a, Result<Self::Stream, Self::Error>>;
}

/// Pull-based provider stream that works with any async runtime.
pub trait ModelStream {
    /// Adapter-specific provider failure.
    type Error: AdapterError;

    /// Retrieves the next provider event.
    fn next<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<ModelEvent>, Self::Error>>;
}
