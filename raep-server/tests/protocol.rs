use raep_memory::{MemoryClock, MemorySessionStore};
use raep_server::{ProtocolServer, ServerConfig};
use raep_types::{Envelope, FrameType, StandardFrameType};
use serde_json::json;

#[test]
fn server_performs_hello_attach_and_ack_without_a_runtime() {
    let mut server = ProtocolServer::new(
        ServerConfig::default(),
        MemorySessionStore::default(),
        MemoryClock::new(1),
    )
    .unwrap();
    assert_eq!(
        server.hello().kind,
        FrameType::Standard(StandardFrameType::Hello)
    );
    let attach = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        json!({"agent_id":"demo"}),
        1,
    );
    let attached = server.handle(attach).unwrap();
    assert_eq!(attached.len(), 2);
    let session_id = attached[0].session_id.clone();
    assert_eq!(
        attached[0].kind,
        FrameType::Standard(StandardFrameType::SessionAttached)
    );
    let ack = Envelope::new(
        FrameType::Standard(StandardFrameType::Ack),
        json!({"last_seq": attached[1].seq.unwrap()}),
        2,
    );
    let mut ack = ack;
    ack.session_id = session_id;
    assert!(server.handle(ack).unwrap().is_empty());
}

#[test]
fn server_cancels_a_queued_turn_and_closes_the_session() {
    let mut server = ProtocolServer::new(
        ServerConfig::default(),
        MemorySessionStore::default(),
        MemoryClock::new(1),
    )
    .unwrap();
    let attach = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        json!({"agent_id":"demo"}),
        1,
    );
    let attached = server.handle(attach).unwrap();
    let session_id = attached[0].session_id.clone();

    let mut input = Envelope::new(
        FrameType::Standard(StandardFrameType::UserInput),
        json!({"prompt":"cancel me"}),
        2,
    );
    input.session_id = session_id.clone();
    let queued = server.handle(input).unwrap();
    assert_eq!(
        queued[0].kind,
        FrameType::Standard(StandardFrameType::TurnStateChanged)
    );

    let mut cancel = Envelope::new(
        FrameType::Standard(StandardFrameType::CancelTurn),
        json!({}),
        3,
    );
    cancel.session_id = session_id.clone();
    let cancelled = server.handle(cancel).unwrap();
    assert_eq!(cancelled[0].payload["state"], json!("CANCELLED"));

    let mut close = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionClose),
        json!({}),
        4,
    );
    close.session_id = session_id;
    let closed = server.handle(close).unwrap();
    assert_eq!(closed[0].payload["status"], json!("CLOSED"));
}

#[test]
fn attach_does_not_create_when_creation_is_disabled() {
    let mut server = ProtocolServer::new(
        ServerConfig::default(),
        MemorySessionStore::default(),
        MemoryClock::new(1),
    )
    .unwrap();
    let mut attach = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        json!({"agent_id":"demo","create_if_missing":false}),
        1,
    );
    attach.session_id = "missing-session".to_owned();
    assert!(server.handle(attach).is_err());
}

#[test]
fn attach_rejects_an_agent_mismatch() {
    let mut server = ProtocolServer::new(
        ServerConfig::default(),
        MemorySessionStore::default(),
        MemoryClock::new(1),
    )
    .unwrap();
    let first = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        json!({"agent_id":"agent-a"}),
        1,
    );
    let session_id = server.handle(first).unwrap()[0].session_id.clone();
    let mut second = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        json!({"agent_id":"agent-b","create_if_missing":false}),
        2,
    );
    second.session_id = session_id;
    assert!(server.handle(second).is_err());
}
