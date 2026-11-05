#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates an identifier from a non-empty, bounded string.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                if value.is_empty() || value.chars().count() > 128 {
                    return Err(IdentifierError);
                }
                Ok(Self(value))
            }

            /// Creates a fresh identifier for local correlation.
            pub fn generate() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Returns the identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

/// Error returned when an identifier violates the wire bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentifierError;

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("identifier must contain between 1 and 128 characters")
    }
}

impl std::error::Error for IdentifierError {}

id_type!(/// A frame identifier.
    FrameId);
id_type!(/// A session identifier.
    SessionId);
id_type!(/// A turn identifier.
    TurnId);
id_type!(/// A detached job identifier.
    JobId);
id_type!(/// A tool-call identifier.
    ToolCallId);
