#![warn(missing_docs)]

use raep_types::Envelope;
use serde_json::Value;
use thiserror::Error;

/// Default maximum JSON text frame size from the RFC.
pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;

/// WebSocket carrier message relevant to RAEP framing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebSocketMessage {
    /// UTF-8 JSON text frame.
    Text(String),
    /// Binary frames are forbidden by RAEP.
    Binary(Vec<u8>),
    /// Ping control frame.
    Ping(Vec<u8>),
    /// Pong control frame.
    Pong(Vec<u8>),
    /// Close control frame.
    Close,
}

/// WebSocket framing failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WebSocketError {
    /// The peer sent a binary frame.
    #[error("RAEP requires one JSON envelope per WebSocket text frame")]
    BinaryFrame,
    /// The text frame exceeded the negotiated limit.
    #[error("WebSocket text frame is {actual} bytes, limit is {limit}")]
    FrameTooLarge {
        /// Actual UTF-8 byte length.
        actual: usize,
        /// Configured byte limit.
        limit: usize,
    },
    /// The text frame was not a JSON envelope.
    #[error("invalid RAEP WebSocket envelope: {0}")]
    InvalidJson(String),
}

/// Runtime-independent WebSocket text codec.
#[derive(Clone, Debug)]
pub struct WebSocketCodec {
    max_frame_bytes: usize,
}

impl Default for WebSocketCodec {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FRAME_BYTES)
    }
}

impl WebSocketCodec {
    /// Creates a codec with a maximum UTF-8 JSON frame size.
    pub fn new(max_frame_bytes: usize) -> Self {
        Self { max_frame_bytes }
    }

    /// Encodes one envelope as one WebSocket text message.
    pub fn encode(&self, envelope: &Envelope<Value>) -> Result<String, WebSocketError> {
        let text = serde_json::to_string(envelope)
            .map_err(|error| WebSocketError::InvalidJson(error.to_string()))?;
        self.check_size(text.as_bytes())?;
        Ok(text)
    }

    /// Decodes one WebSocket text message into an envelope.
    pub fn decode(
        &self,
        message: WebSocketMessage,
    ) -> Result<Option<Envelope<Value>>, WebSocketError> {
        match message {
            WebSocketMessage::Text(text) => {
                self.check_size(text.as_bytes())?;
                serde_json::from_str(&text)
                    .map(Some)
                    .map_err(|error| WebSocketError::InvalidJson(error.to_string()))
            }
            WebSocketMessage::Binary(_) => Err(WebSocketError::BinaryFrame),
            WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) | WebSocketMessage::Close => {
                Ok(None)
            }
        }
    }

    fn check_size(&self, bytes: &[u8]) -> Result<(), WebSocketError> {
        if bytes.len() > self.max_frame_bytes {
            return Err(WebSocketError::FrameTooLarge {
                actual: bytes.len(),
                limit: self.max_frame_bytes,
            });
        }
        Ok(())
    }
}
