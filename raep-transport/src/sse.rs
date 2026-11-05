#![warn(missing_docs)]

use raep_types::Envelope;
use serde_json::Value;
use thiserror::Error;

/// Default SSE retry hint from the RFC.
pub const DEFAULT_RETRY_MS: u64 = 2_000;

/// One server-sent event carrying a complete RAEP envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SseEvent {
    /// Envelope sequence, when present.
    pub id: Option<u64>,
    /// SSE event name, normally the envelope type.
    pub event: String,
    /// Complete JSON envelope serialized on one line.
    pub data: String,
}

/// SSE stream control record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SseRecord {
    /// Browser retry hint.
    Retry(u64),
    /// Comment or keepalive line.
    Comment(String),
    /// Protocol event.
    Event(SseEvent),
}

/// SSE encoding or decoding failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SseError {
    /// An envelope could not be serialized.
    #[error("invalid RAEP SSE envelope: {0}")]
    InvalidJson(String),
    /// An SSE event contained invalid JSON.
    #[error("invalid RAEP SSE data: {0}")]
    InvalidData(String),
    /// An event had no data field.
    #[error("SSE event is missing data")]
    MissingData,
}

/// Encodes RAEP events according to the SSE binding.
#[derive(Clone, Debug, Default)]
pub struct SseCodec;

impl SseCodec {
    /// Creates the initial retry and keepalive records.
    pub fn prelude(&self) -> String {
        format!("retry: {DEFAULT_RETRY_MS}\n\n: keepalive\n\n")
    }

    /// Encodes one envelope as one SSE event.
    pub fn encode(&self, envelope: &Envelope<Value>) -> Result<String, SseError> {
        let data = serde_json::to_string(envelope)
            .map_err(|error| SseError::InvalidJson(error.to_string()))?;
        let event = serde_json::to_value(&envelope.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "message".to_owned());
        let id = envelope
            .seq
            .map(|sequence| format!("id: {sequence}\n"))
            .unwrap_or_default();
        Ok(format!("{id}event: {event}\ndata: {data}\n\n"))
    }

    /// Decodes one complete SSE event into an envelope.
    pub fn decode(&self, event: &SseEvent) -> Result<Envelope<Value>, SseError> {
        serde_json::from_str(&event.data).map_err(|error| SseError::InvalidData(error.to_string()))
    }

    /// Parses one complete SSE block into a record.
    pub fn parse_record(&self, block: &str) -> Result<SseRecord, SseError> {
        let mut id = None;
        let mut event = "message".to_owned();
        let mut data = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("id:") {
                id = value.trim().parse().ok();
            } else if let Some(value) = line.strip_prefix("event:") {
                event = value.trim().to_owned();
            } else if let Some(value) = line.strip_prefix("data:") {
                data = Some(value.trim().to_owned());
            } else if let Some(value) = line.strip_prefix("retry:") {
                return value
                    .trim()
                    .parse()
                    .map(SseRecord::Retry)
                    .map_err(|_| SseError::InvalidData("invalid retry value".to_owned()));
            } else if let Some(value) = line.strip_prefix(':') {
                return Ok(SseRecord::Comment(value.trim().to_owned()));
            }
        }
        Ok(SseRecord::Event(SseEvent {
            id,
            event,
            data: data.ok_or(SseError::MissingData)?,
        }))
    }
}
