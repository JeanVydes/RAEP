#![warn(missing_docs)]

use raep_domain::{DomainEvent, EntityId, Session};
use raep_ports::{RecordedEvent, SessionStore};
use std::collections::HashMap;
use std::convert::Infallible;

/// In-memory session store with a per-session durable event sequence.
#[derive(Clone, Debug, Default)]
pub struct MemorySessionStore {
    sessions: HashMap<EntityId, Session>,
    events: HashMap<EntityId, Vec<RecordedEvent>>,
}

impl MemorySessionStore {
    /// Returns all events retained for a session.
    pub fn events(&self, id: &EntityId) -> &[RecordedEvent] {
        self.events.get(id).map_or(&[], Vec::as_slice)
    }
}

impl SessionStore for MemorySessionStore {
    type Error = Infallible;

    fn load(&self, id: &EntityId) -> Result<Option<Session>, Self::Error> {
        Ok(self.sessions.get(id).cloned())
    }

    fn commit(
        &mut self,
        session: Session,
        events: Vec<DomainEvent>,
    ) -> Result<Vec<RecordedEvent>, Self::Error> {
        let session_id = session.id().clone();
        let retained = self.events.entry(session_id.clone()).or_default();
        let first = retained.last().map_or(1, |event| event.sequence() + 1);
        let recorded = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| RecordedEvent::new(first + index as u64, event))
            .collect::<Vec<_>>();
        retained.extend(recorded.iter().cloned());
        self.sessions.insert(session_id, session);
        Ok(recorded)
    }

    fn events_after(
        &self,
        id: &EntityId,
        sequence: u64,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, Self::Error> {
        Ok(self
            .events
            .get(id)
            .into_iter()
            .flatten()
            .filter(|event| event.sequence() > sequence)
            .take(limit)
            .cloned()
            .collect())
    }
}
