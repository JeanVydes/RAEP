#![warn(missing_docs)]

use crate::{AttachMode, ClientFeature, Modality, ToolDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client capabilities sent during attachment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Output modalities the client can render.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub render: Vec<Modality>,
    /// Optional client features.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<ClientFeature>,
    /// Tools executed by the attached client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_tools: Vec<ToolDescriptor>,
}

/// Client attachment request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionAttach {
    /// Agent requested for the session.
    pub agent_id: String,
    /// Optional bounded agent override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_override: Option<Value>,
    /// Whether a missing session may be created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_if_missing: Option<bool>,
    /// Attachment role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<AttachMode>,
    /// Whether to replace an existing controller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover: Option<bool>,
    /// Last server sequence processed by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_from_seq: Option<u64>,
    /// Client feature negotiation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
    /// Client identity for diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<ClientInfo>,
    /// Server-defined session options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

/// Client implementation identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Client version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
