use raep_conformance::{
    check_envelope, check_scenarios, frame_type, load_frame_vectors, SchemaRegistry,
};
use serde_json::Value;

const VECTORS: &str = include_str!("../../vectors.json");
const SCHEMAS: &str = include_str!("../../schemas.json");

#[test]
fn official_vectors_load_and_valid_frames_have_envelopes() {
    let vectors = load_frame_vectors(VECTORS).unwrap();
    assert_eq!(vectors.len(), 17);
    for vector in vectors {
        if vector.valid {
            check_envelope(&vector.frame).unwrap_or_else(|error| {
                panic!("{} failed: {error}", vector.id);
            });
            frame_type(&vector.frame).unwrap();
        }
    }
}

#[test]
fn official_envelope_schema_compiles_and_accepts_valid_frames() {
    let registry = SchemaRegistry::from_document(SCHEMAS).unwrap();
    for vector in load_frame_vectors(VECTORS).unwrap() {
        if vector.valid {
            registry
                .validate_envelope(&vector.frame)
                .unwrap_or_else(|errors| panic!("{} failed: {errors:?}", vector.id));
        }
    }
}

#[test]
fn official_frame_schema_resolves_payload_references() {
    let registry = SchemaRegistry::from_document(SCHEMAS).unwrap();
    for vector in load_frame_vectors(VECTORS).unwrap() {
        if vector.valid {
            registry
                .validate_frame(&vector.frame)
                .unwrap_or_else(|errors| panic!("{} failed: {errors:?}", vector.id));
        }
    }
}

#[test]
fn all_official_schemas_compile() {
    SchemaRegistry::from_document(SCHEMAS)
        .unwrap()
        .validate_all()
        .unwrap();
}

#[test]
fn official_document_is_json() {
    let value: Value = serde_json::from_str(VECTORS).unwrap();
    assert!(value.get("scenarios").and_then(Value::as_array).is_some());
}

#[test]
fn official_scenarios_have_setup_and_steps() {
    let reports = check_scenarios(VECTORS).unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|report| report.passed));
}
