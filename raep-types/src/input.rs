#![warn(missing_docs)]

use crate::{Modality, QueuePolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// A multimodal input part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentPart {
    /// Part modality.
    #[serde(rename = "type")]
    pub kind: Modality,
    /// Text content for text parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// External or inline source for non-text parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
    /// MIME type for non-text parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// User input payload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserInput {
    /// Plain text input. Exactly one of `prompt` and `content` is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Multimodal input. Exactly one of `prompt` and `content` is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ContentPart>>,
    /// Behavior when another turn is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_policy: Option<QueuePolicy>,
    /// Per-turn configuration overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Value>,
    /// Arbitrary caller metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl UserInput {
    /// Creates a text input.
    pub fn prompt(prompt: impl Into<String>) -> Result<Self, PayloadError> {
        let prompt = prompt.into();
        if prompt.is_empty() {
            return Err(PayloadError::EmptyPrompt);
        }
        Ok(Self {
            prompt: Some(prompt),
            ..Self::default()
        })
    }

    /// Creates multimodal input after checking that it is non-empty.
    pub fn content(content: Vec<ContentPart>) -> Result<Self, PayloadError> {
        if content.is_empty() {
            return Err(PayloadError::EmptyContent);
        }
        Ok(Self {
            content: Some(content),
            ..Self::default()
        })
    }

    /// Validates the exactly-one content rule from the wire schema.
    pub fn validate(&self) -> Result<(), PayloadError> {
        match (self.prompt.as_deref(), self.content.as_deref()) {
            (Some(prompt), None) if !prompt.is_empty() => Ok(()),
            (None, Some(content)) if !content.is_empty() => Ok(()),
            (Some(_), Some(_)) => Err(PayloadError::MultipleInputs),
            _ => Err(PayloadError::MissingInput),
        }
    }
}

/// Errors raised while constructing a payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadError {
    /// Text input was empty.
    EmptyPrompt,
    /// Multimodal input had no parts.
    EmptyContent,
    /// Both input forms were supplied.
    MultipleInputs,
    /// Neither input form was supplied.
    MissingInput,
}

impl fmt::Display for PayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyPrompt => "prompt must not be empty",
            Self::EmptyContent => "content must contain at least one part",
            Self::MultipleInputs => "exactly one of prompt and content is required",
            Self::MissingInput => "prompt or content is required",
        })
    }
}

impl std::error::Error for PayloadError {}
