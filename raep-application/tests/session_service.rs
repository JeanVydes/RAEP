use raep_application::SessionService;
use raep_domain::{EntityId, TurnState};
use raep_memory::{MemoryClock, MemorySessionStore};

#[test]
fn service_uses_ports_for_the_full_session_lifecycle() {
    let mut service = SessionService::new(MemorySessionStore::default(), MemoryClock::new(1_000));
    let session_id = EntityId::new("session").unwrap();
    service.create_session(session_id.clone()).unwrap();
    service
        .start_turn(&session_id, EntityId::new("turn").unwrap())
        .unwrap();
    let session = service
        .transition_turn(&session_id, TurnState::CallingProvider)
        .unwrap();
    assert!(session.session().active_turn().is_some());
    assert_eq!(session.events().len(), 1);
    service
        .transition_turn(&session_id, TurnState::Completed)
        .unwrap();
    let closed = service.close_session(&session_id).unwrap();
    assert_eq!(closed.events().len(), 1);
}
