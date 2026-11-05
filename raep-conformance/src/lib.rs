//! Utilities for running the repository's RFC conformance vectors.
//!
//! This crate deliberately performs structural checks only. Full JSON Schema
//! validation remains a deployment choice because servers may add schemas and
//! registries, but every official vector can be loaded and inspected here.

#![warn(missing_docs)]

use jsonschema::{Draft, JSONSchema, SchemaResolver};
use raep_types::{Envelope, FrameType};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use url::Url;

/// A vector loaded from `vectors.json`.
#[derive(Clone, Debug)]
pub struct FrameVector {
    /// Vector identifier.
    pub id: String,
    /// Whether the RFC expects the frame to be valid.
    pub valid: bool,
    /// Raw frame value.
    pub frame: Value,
}

/// A scenario from the RFC conformance document.
#[derive(Clone, Debug)]
pub struct Scenario {
    /// Scenario identifier.
    pub id: String,
    /// Scenario title.
    pub title: String,
    /// Required profile.
    pub profile: String,
    /// Raw setup and steps.
    pub body: Value,
}

/// Error raised while loading or checking a vector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceError(String);

impl fmt::Display for ConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ConformanceError {}

/// Loads frame vectors from the official repository document.
pub fn load_frame_vectors(document: &str) -> Result<Vec<FrameVector>, ConformanceError> {
    let root: Value =
        serde_json::from_str(document).map_err(|error| ConformanceError(error.to_string()))?;
    let frames = root
        .get("frames")
        .and_then(Value::as_array)
        .ok_or_else(|| ConformanceError("vectors document has no frames array".to_owned()))?;
    frames
        .iter()
        .map(|vector| {
            Ok(FrameVector {
                id: required_string(vector, "id")?,
                valid: vector
                    .get("valid")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| ConformanceError("vector has no boolean valid".to_owned()))?,
                frame: vector
                    .get("frame")
                    .cloned()
                    .ok_or_else(|| ConformanceError("vector has no frame".to_owned()))?,
            })
        })
        .collect()
}

/// Loads scenario metadata and steps from `vectors.json`.
pub fn load_scenarios(document: &str) -> Result<Vec<Scenario>, ConformanceError> {
    let root: Value =
        serde_json::from_str(document).map_err(|error| ConformanceError(error.to_string()))?;
    root.get("scenarios")
        .and_then(Value::as_array)
        .ok_or_else(|| ConformanceError("vectors document has no scenarios array".to_owned()))?
        .iter()
        .map(|scenario| {
            Ok(Scenario {
                id: required_string(scenario, "id")?,
                title: required_string(scenario, "title")?,
                profile: required_string(scenario, "profile")?,
                body: scenario.clone(),
            })
        })
        .collect()
}

/// A report for one scenario or frame-vector run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceReport {
    /// Subject identifier.
    pub id: String,
    /// Whether all checks passed.
    pub passed: bool,
    /// Human-readable failures.
    pub failures: Vec<String>,
}

/// Checks that every official scenario has executable setup and steps.
pub fn check_scenarios(document: &str) -> Result<Vec<ConformanceReport>, ConformanceError> {
    load_scenarios(document)?
        .into_iter()
        .map(|scenario| {
            let mut failures = Vec::new();
            if scenario
                .body
                .get("setup")
                .and_then(Value::as_object)
                .is_none()
            {
                failures.push("scenario setup must be an object".to_owned());
            }
            if scenario
                .body
                .get("steps")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            {
                failures.push("scenario steps must be a non-empty array".to_owned());
            }
            Ok(ConformanceReport {
                id: scenario.id,
                passed: failures.is_empty(),
                failures,
            })
        })
        .collect()
}

/// Checks the common envelope shape of a frame vector.
pub fn check_envelope(frame: &Value) -> Result<(), ConformanceError> {
    let object = frame
        .as_object()
        .ok_or_else(|| ConformanceError("frame is not an object".to_owned()))?;
    for field in [
        "version",
        "id",
        "session_id",
        "timestamp",
        "type",
        "payload",
    ] {
        if !object.contains_key(field) {
            return Err(ConformanceError(format!("frame is missing `{field}`")));
        }
    }
    let _: Envelope<Value> = serde_json::from_value(frame.clone())
        .map_err(|error| ConformanceError(format!("invalid envelope: {error}")))?;
    Ok(())
}

/// Checks all frame vectors, returning invalid vectors that fail structurally.
pub fn check_vectors(document: &str) -> Result<Vec<String>, ConformanceError> {
    load_frame_vectors(document)?
        .into_iter()
        .filter_map(|vector| match check_envelope(&vector.frame) {
            Ok(()) => None,
            Err(error) if vector.valid => {
                Some(Err(ConformanceError(format!("{}: {}", vector.id, error))))
            }
            Err(_) => None,
        })
        .collect()
}

fn required_string(value: &Value, field: &str) -> Result<String, ConformanceError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ConformanceError(format!("vector has no string `{field}`")))
}

/// Parses a frame type from an envelope without imposing payload semantics.
pub fn frame_type(frame: &Value) -> Result<FrameType, ConformanceError> {
    serde_json::from_value(
        frame
            .get("type")
            .cloned()
            .ok_or_else(|| ConformanceError("frame has no type".to_owned()))?,
    )
    .map_err(|error| ConformanceError(format!("invalid frame type: {error}")))
}

/// A compiled set of the RFC's schemas, addressable by their `$id`.
pub struct SchemaRegistry {
    schemas: HashMap<String, Value>,
}

impl SchemaRegistry {
    /// Loads every schema object from the repository's schema document.
    pub fn from_document(document: &str) -> Result<Self, ConformanceError> {
        let values: Vec<Value> =
            serde_json::from_str(document).map_err(|error| ConformanceError(error.to_string()))?;
        let mut schemas = HashMap::with_capacity(values.len());
        for schema in values {
            let id = required_string(&schema, "$id")?;
            schemas.insert(id, schema);
        }
        Ok(Self { schemas })
    }

    /// Validates a frame against the common envelope schema.
    pub fn validate_envelope(&self, frame: &Value) -> Result<(), Vec<String>> {
        self.validate_by_id("https://raep.vyde.dev/v1/envelope.json", frame)
    }

    /// Validates a frame against the RFC frame schema, including its payload
    /// branch for known message types.
    pub fn validate_frame(&self, frame: &Value) -> Result<(), Vec<String>> {
        self.validate_by_id("https://raep.vyde.dev/v1/frame.json", frame)
    }

    /// Compiles every registered schema and resolves all local references.
    pub fn validate_all(&self) -> Result<(), Vec<String>> {
        let mut failures = Vec::new();
        for id in self.schemas.keys() {
            if let Err(error) = self.compile_by_id(id) {
                failures.push(format!("{id}: {error:?}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }

    fn validate_by_id(&self, id: &str, value: &Value) -> Result<(), Vec<String>> {
        let validator = self.compile_by_id(id)?;
        let result = match validator.validate(value) {
            Ok(()) => Ok(()),
            Err(errors) => Err(errors.map(|error| error.to_string()).collect()),
        };
        result
    }

    fn compile_by_id(&self, id: &str) -> Result<jsonschema::JSONSchema, Vec<String>> {
        let schema = self
            .schemas
            .get(id)
            .ok_or_else(|| vec![format!("missing schema `{id}`")])?;
        JSONSchema::options()
            .with_draft(Draft::Draft202012)
            .with_resolver(LocalResolver {
                schemas: self.schemas.clone(),
            })
            .compile(schema)
            .map_err(|error| vec![error.to_string()])
    }
}

#[derive(Clone)]
struct LocalResolver {
    schemas: HashMap<String, Value>,
}

impl SchemaResolver for LocalResolver {
    fn resolve(
        &self,
        _root_schema: &Value,
        url: &Url,
        _original_reference: &str,
    ) -> Result<Arc<Value>, anyhow::Error> {
        self.schemas
            .get(url.as_str())
            .cloned()
            .map(Arc::new)
            .ok_or_else(|| anyhow::anyhow!("schema not found: {url}"))
    }
}
