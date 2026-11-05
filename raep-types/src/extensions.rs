#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A payload not yet assigned a dedicated Rust structure.
///
/// The RFC permits extension payloads and server implementations may use this
/// type while preserving the envelope and frame catalog. Dedicated payload
/// modules can replace it without changing transport code.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonPayload(pub Value);

/// A server capability name from the RFC registry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Capability(pub String);

/// A server limit name and value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    /// Registry key.
    pub name: String,
    /// Limit value.
    pub value: Value,
}

/// A stable RFC error code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(pub String);

/// A deprecation notice advertised by a server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deprecation {
    /// Deprecated registry name.
    pub name: String,
    /// Registry kind.
    pub kind: String,
    /// Replacement name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// RFC sunset timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset_at: Option<String>,
}
