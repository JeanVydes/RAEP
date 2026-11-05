use raep_transport::{SseCodec, SseRecord, WebSocketCodec, WebSocketError, WebSocketMessage};
use raep_types::{Envelope, FrameType, StandardFrameType};
use serde_json::json;

fn envelope() -> Envelope<serde_json::Value> {
    Envelope::new(
        FrameType::Standard(StandardFrameType::Hello),
        json!({"server_version": "test"}),
        1,
    )
}

#[test]
fn websocket_codec_uses_one_text_message_per_envelope() {
    let codec = WebSocketCodec::default();
    let text = codec.encode(&envelope()).unwrap();
    let decoded = codec.decode(WebSocketMessage::Text(text)).unwrap().unwrap();
    assert_eq!(decoded.kind, FrameType::Standard(StandardFrameType::Hello));
    assert_eq!(
        codec.decode(WebSocketMessage::Binary(vec![1])).unwrap_err(),
        WebSocketError::BinaryFrame
    );
}

#[test]
fn websocket_codec_enforces_byte_limit() {
    let error = WebSocketCodec::new(1).encode(&envelope()).unwrap_err();
    assert!(matches!(error, WebSocketError::FrameTooLarge { .. }));
}

#[test]
fn sse_codec_preserves_sequence_and_event_name() {
    let codec = SseCodec;
    let mut frame = envelope();
    frame.seq = Some(57);
    let encoded = codec.encode(&frame).unwrap();
    assert!(encoded.starts_with("id: 57\nevent: HELLO\ndata: "));
    let record = codec.parse_record(encoded.trim_end()).unwrap();
    let SseRecord::Event(event) = record else {
        panic!("expected event");
    };
    assert_eq!(codec.decode(&event).unwrap().seq, Some(57));
}
