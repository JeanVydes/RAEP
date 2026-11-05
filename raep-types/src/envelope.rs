#![warn(missing_docs)]

use crate::{FrameId, FrameType, JobId, TurnId, WIRE_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The envelope shared by every RAEP transport binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<P = Value> {
    /// Constant wire version.
    pub version: String,
    /// Unique sender identifier.
    pub id: FrameId,
    /// Server sequence number, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    /// Identifier of the frame being answered.
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<FrameId>,
    /// Session scope. Empty for connection-level frames.
    pub session_id: String,
    /// Optional turn scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// Optional job scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    /// Sender timestamp in Unix epoch milliseconds.
    pub timestamp: i64,
    /// Message type.
    #[serde(rename = "type")]
    pub kind: FrameType,
    /// Type-specific payload.
    pub payload: P,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Value>,
}

impl<P> Envelope<P> {
    /// Creates an unsequenced client or connection-level envelope.
    pub fn new(kind: FrameType, payload: P, timestamp: i64) -> Self {
        Self {
            version: WIRE_VERSION.to_owned(),
            id: FrameId::generate(),
            seq: None,
            reference: None,
            session_id: String::new(),
            turn_id: None,
            job_id: None,
            timestamp,
            kind,
            payload,
            ext: None,
        }
    }

    /// Maps the payload while preserving envelope metadata.
    pub fn map_payload<T>(self, map: impl FnOnce(P) -> T) -> Envelope<T> {
        Envelope {
            version: self.version,
            id: self.id,
            seq: self.seq,
            reference: self.reference,
            session_id: self.session_id,
            turn_id: self.turn_id,
            job_id: self.job_id,
            timestamp: self.timestamp,
            kind: self.kind,
            payload: map(self.payload),
            ext: self.ext,
        }
    }
}
