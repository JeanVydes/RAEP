#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_types::Envelope;
use serde_json::Value;

/// Durable or best-effort notification destination.
pub trait NotificationSink {
    /// Adapter-specific notification failure.
    type Error: AdapterError;

    /// Publishes an out-of-band notification.
    fn publish<'a>(
        &'a mut self,
        topic: &'a str,
        notification: Envelope<Value>,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;
}
