#![warn(missing_docs)]

use raep_ports::{
    BoxFuture, ModelEvent, ModelProvider, ModelRequest, ModelStream, ToolExecutorPort, ToolRequest,
    ToolResult,
};
use raep_types::ToolDescriptor;
use thiserror::Error;

/// Collected result of one provider/tool execution pass.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionReport {
    /// Provider events in arrival order.
    pub model_events: Vec<ModelEvent>,
    /// Tool results produced during the pass.
    pub tool_results: Vec<ToolResult>,
}

/// Error raised by provider or tool execution.
#[derive(Debug, Error)]
pub enum ExecutionError<PE, TE>
where
    PE: std::error::Error + Send + Sync + 'static,
    TE: std::error::Error + Send + Sync + 'static,
{
    /// Provider failed.
    #[error("provider failed: {0}")]
    Provider(PE),
    /// Tool failed.
    #[error("tool failed: {0}")]
    Tool(TE),
    /// The provider requested an unknown tool.
    #[error("provider requested unknown tool `{0}`")]
    UnknownTool(String),
}

/// Future returned by an execution coordinator.
pub type ExecutionFuture<'a, PE, TE> =
    BoxFuture<'a, Result<ExecutionReport, ExecutionError<PE, TE>>>;

/// Runtime-independent coordinator for one model/tool pass.
pub struct ExecutionCoordinator<P, T> {
    provider: P,
    tools: T,
    descriptors: Vec<ToolDescriptor>,
}

impl<P, T> ExecutionCoordinator<P, T>
where
    P: ModelProvider + Send,
    T: ToolExecutorPort + Send,
    P::Stream: Send,
{
    /// Creates an execution coordinator with host-provided adapters.
    pub fn new(provider: P, tools: T, descriptors: Vec<ToolDescriptor>) -> Self {
        Self {
            provider,
            tools,
            descriptors,
        }
    }

    /// Starts one provider stream and executes tool calls it emits.
    ///
    /// Continuing generation after a tool result is intentionally a host
    /// policy: the host may call this coordinator again with an updated
    /// `ModelRequest`, or implement a richer orchestration loop above it.
    pub fn run<'a>(&'a mut self, request: ModelRequest) -> ExecutionFuture<'a, P::Error, T::Error> {
        Box::pin(async move {
            let mut stream = self
                .provider
                .generate(request)
                .await
                .map_err(ExecutionError::Provider)?;
            let mut model_events = Vec::new();
            let mut tool_results = Vec::new();
            while let Some(event) = stream.next().await.map_err(ExecutionError::Provider)? {
                if let ModelEvent::ToolCall {
                    tool_call_id,
                    tool_name,
                    arguments,
                } = &event
                {
                    let descriptor = self
                        .descriptors
                        .iter()
                        .find(|descriptor| descriptor.name == *tool_name)
                        .cloned()
                        .ok_or_else(|| ExecutionError::UnknownTool(tool_name.clone()))?;
                    let result = self
                        .tools
                        .execute(ToolRequest {
                            descriptor,
                            tool_call_id: tool_call_id.clone(),
                            arguments: arguments.clone(),
                        })
                        .await
                        .map_err(ExecutionError::Tool)?;
                    tool_results.push(result);
                }
                model_events.push(event);
            }
            Ok(ExecutionReport {
                model_events,
                tool_results,
            })
        })
    }
}
