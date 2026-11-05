#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_types::Envelope;
use serde_json::Value;
use std::fmt;

/// Opaque identifier assigned to one transport connection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionId(String);

impl ConnectionId {
    /// Creates a bounded connection identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A transport-independent inbound frame.
pub type InboundFrame = Envelope<Value>;

/// A transport-independent outbound frame.
pub type OutboundFrame = Envelope<Value>;

/// Connection lifecycle and frame delivery boundary.
pub trait TransportConnection {
    /// Adapter-specific transport failure.
    type Error: AdapterError;

    /// Connection identifier.
    fn id(&self) -> &ConnectionId;

    /// Sends one frame to the remote peer.
    fn send<'a>(&'a mut self, frame: OutboundFrame) -> BoxFuture<'a, Result<(), Self::Error>>;

    /// Closes the connection with an implementation-defined reason.
    fn close<'a>(&'a mut self, reason: &'a str) -> BoxFuture<'a, Result<(), Self::Error>>;
}

/// Accepts transport connections without selecting a protocol stack.
pub trait TransportAcceptor {
    /// Adapter-specific transport failure.
    type Error: AdapterError;
    /// Connection implementation.
    type Connection: TransportConnection<Error = Self::Error>;

    /// Waits for the next connection.
    fn accept<'a>(&'a mut self) -> BoxFuture<'a, Result<Self::Connection, Self::Error>>;
}
