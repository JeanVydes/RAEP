use raep_types::{
    Ack, Envelope, FrameType, PayloadError, StandardFrameType, ToolDescriptor, TurnState,
    TurnStateChanged, Usage, UserInput, WIRE_VERSION,
};
use serde_json::json;

#[test]
fn envelope_uses_wire_field_names() {
    let envelope = Envelope::new(
        FrameType::Standard(StandardFrameType::Hello),
        json!({"server_version": "test"}),
        42,
    );
    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(value["version"], WIRE_VERSION);
    assert_eq!(value["type"], "HELLO");
    assert_eq!(value["payload"]["server_version"], "test");
}

#[test]
fn generated_ids_are_bounded() {
    let envelope = Envelope::new(FrameType::Standard(StandardFrameType::Ack), json!({}), 0);
    assert!(!envelope.id.as_str().is_empty());
    assert!(envelope.id.as_str().chars().count() <= 128);
}

#[test]
fn user_input_enforces_one_content_form() {
    let prompt = UserInput::prompt("hello").unwrap();
    assert!(prompt.validate().is_ok());

    let invalid = UserInput {
        prompt: Some("hello".to_owned()),
        content: Some(Vec::new()),
        ..UserInput::default()
    };
    assert_eq!(invalid.validate(), Err(PayloadError::MultipleInputs));
}

#[test]
fn typed_payloads_serialize_to_schema_names() {
    let ack = serde_json::to_value(Ack { last_seq: 4 }).unwrap();
    assert_eq!(ack["last_seq"], 4);

    let state = serde_json::to_value(TurnStateChanged {
        state: TurnState::CallingProvider,
        stop_reason: None,
        usage: None,
        error: None,
    })
    .unwrap();
    assert_eq!(state["state"], "CALLING_PROVIDER");
}

#[test]
fn usage_and_tool_descriptor_are_wire_compatible() {
    let usage = serde_json::to_value(Usage {
        input_tokens: Some(3),
        output_tokens: Some(5),
        ..Usage::default()
    })
    .unwrap();
    assert_eq!(usage["input_tokens"], 3);
    assert_eq!(usage["output_tokens"], 5);

    let tool = serde_json::to_value(ToolDescriptor {
        name: "read_file".to_owned(),
        description: None,
        input_schema: json!({"type": "object"}),
        output_schema: None,
        executor: None,
        risk: None,
        side_effects: None,
        idempotent: None,
        timeout_s: None,
        scopes: Vec::new(),
        detachable: None,
    })
    .unwrap();
    assert_eq!(tool["name"], "read_file");
    assert_eq!(tool["input_schema"]["type"], "object");
}
