use raep_domain::{DomainError, EntityId, Session, SessionStatus, TurnState};

fn id(value: &str) -> EntityId {
    EntityId::new(value).unwrap()
}

#[test]
fn session_enforces_one_active_turn() {
    let mut session = Session::new(id("session"));
    session.start_turn(id("turn-1")).unwrap();
    assert_eq!(
        session.start_turn(id("turn-2")),
        Err(DomainError::SessionBusy)
    );
}

#[test]
fn terminal_turn_returns_session_to_idle() {
    let mut session = Session::new(id("session"));
    session.start_turn(id("turn")).unwrap();
    session.transition_turn(TurnState::CallingProvider).unwrap();
    session.transition_turn(TurnState::Completed).unwrap();
    assert_eq!(session.status(), SessionStatus::Idle);
    assert!(session.active_turn().is_none());
}

#[test]
fn invalid_transition_is_rejected() {
    let mut session = Session::new(id("session"));
    session.start_turn(id("turn")).unwrap();
    assert!(matches!(
        session.transition_turn(TurnState::Completed),
        Err(DomainError::InvalidTransition { .. })
    ));
}

#[test]
fn an_idle_session_can_close() {
    let mut session = Session::new(id("session"));
    session.close().unwrap();
    assert_eq!(session.status(), SessionStatus::Closed);
}
