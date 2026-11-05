//! Runtime-independent RAEP protocol dispatch.
//!
//! This crate owns protocol state and command routing, not sockets, HTTP,
//! persistence engines, provider SDKs, or async runtimes. A host feeds decoded
//! envelopes into [`ProtocolServer::handle`] and writes the returned envelopes
//! through its WebSocket or SSE adapter.

#![warn(missing_docs)]

use raep_core::{IdempotencyCache, ReplayWindow, Sequencer};
use raep_domain::{EntityId, SessionStatus};
use raep_ports::{Clock, SessionStore};
use raep_types::{
    AttachMode, Envelope, FrameId, FrameType, Hello, HelloLimits, SessionState, StandardFrameType,
    TurnState as WireTurnState, UserInput, WIRE_VERSION,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Configuration advertised during the RAEP handshake.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Opaque host implementation version.
    pub server_version: String,
    /// Capability registry entries.
    pub capabilities: Vec<String>,
    /// Negotiated operational limits.
    pub limits: HelloLimits,
    /// Maximum retained events per session connection.
    pub replay_capacity: usize,
    /// Maximum remembered command outcomes per connection.
    pub idempotency_capacity: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_version: "raep-host".to_owned(),
            capabilities: vec!["transport.ws".to_owned(), "transport.sse".to_owned()],
            limits: HelloLimits {
                max_frame_bytes: Some(1024 * 1024),
                replay_buffer_events: Some(10_000),
                ..HelloLimits::default()
            },
            replay_capacity: 10_000,
            idempotency_capacity: 1024,
        }
    }
}

/// Protocol failures returned as `ERROR` payloads.
#[derive(Debug, Error)]
pub enum ServerError<E: std::error::Error + Send + Sync + 'static> {
    /// The incoming envelope violated protocol rules.
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    /// The application store failed.
    #[error("store failed: {0}")]
    Store(E),
    /// A replay request cannot be satisfied from retained events.
    #[error("replay unavailable: {0}")]
    Replay(String),
}

/// A stateful protocol dispatcher for one transport connection.
pub struct ProtocolServer<S, C>
where
    S: SessionStore,
    C: Clock,
{
    config: ServerConfig,
    store: S,
    clock: C,
    attached: HashSet<String>,
    sequences: HashMap<String, Sequencer>,
    replay: HashMap<String, ReplayWindow<Envelope<Value>>>,
    idempotency: IdempotencyCache<FrameId, Vec<Envelope<Value>>>,
}

impl<S, C> ProtocolServer<S, C>
where
    S: SessionStore,
    C: Clock,
{
    /// Creates a dispatcher with explicit host-owned ports.
    pub fn new(config: ServerConfig, store: S, clock: C) -> Result<Self, raep_core::DeliveryError> {
        let replay_capacity = config.replay_capacity;
        let _ = ReplayWindow::<Envelope<Value>>::new(replay_capacity)?;
        Ok(Self {
            idempotency: IdempotencyCache::new(config.idempotency_capacity)?,
            replay: HashMap::new(),
            sequences: HashMap::new(),
            attached: HashSet::new(),
            config,
            store,
            clock,
        })
    }

    /// Produces the connection-level `HELLO` frame.
    pub fn hello(&self) -> Envelope<Value> {
        Envelope::new(
            standard(StandardFrameType::Hello),
            serde_json::to_value(Hello {
                server_version: self.config.server_version.clone(),
                wire_versions: vec![WIRE_VERSION.to_owned()],
                capabilities: self.config.capabilities.clone(),
                limits: self.config.limits.clone(),
                heartbeat_s: Some(20),
                draining: Some(false),
                deprecations: Vec::new(),
            })
            .expect("wire payload serialization cannot fail"),
            self.clock.now_millis(),
        )
    }

    /// Returns the server discovery payload.
    pub fn info(&self) -> Value {
        json!({
            "server_version": self.config.server_version,
            "wire_versions": [WIRE_VERSION],
            "capabilities": self.config.capabilities,
            "limits": self.config.limits,
        })
    }

    /// Returns the configured capability names.
    pub fn capabilities(&self) -> &[String] {
        &self.config.capabilities
    }

    /// Returns the configured maximum inbound frame size.
    pub fn max_frame_bytes(&self) -> usize {
        self.config
            .limits
            .max_frame_bytes
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1024 * 1024)
    }

    /// Loads a session through the host-provided store.
    pub fn load_session(
        &self,
        id: &EntityId,
    ) -> Result<Option<raep_domain::Session>, ServerError<S::Error>> {
        self.store.load(id).map_err(ServerError::Store)
    }

    /// Closes a session through the domain lifecycle invariant.
    pub fn close_session(&mut self, id: &EntityId) -> Result<(), ServerError<S::Error>> {
        let mut session = self
            .store
            .load(id)
            .map_err(ServerError::Store)?
            .ok_or_else(|| ServerError::InvalidFrame("session not found".to_owned()))?;
        session
            .close()
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        self.store
            .commit(session, Vec::new())
            .map_err(ServerError::Store)?;
        Ok(())
    }

    /// Creates an empty session using a server-generated identifier.
    pub fn create_session(&mut self, agent_id: &str) -> Result<String, ServerError<S::Error>> {
        let session_id = format!("session-{}", FrameId::generate());
        let id = EntityId::new(session_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        self.store
            .commit(
                raep_domain::Session::new_with_agent(id, agent_id),
                Vec::new(),
            )
            .map_err(ServerError::Store)?;
        Ok(session_id)
    }

    /// Returns retained events for a session after a sequence number.
    pub fn replay_after(
        &self,
        session_id: &str,
        last_seen: u64,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.replay
            .get(session_id)
            .ok_or_else(|| ServerError::Replay("session replay window is missing".to_owned()))?
            .replay_after(last_seen)
            .map(|events| events.into_iter().map(|(_, event)| event).collect())
            .map_err(|error| ServerError::Replay(error.to_string()))
    }

    /// Handles one decoded client envelope and returns server events.
    pub fn handle(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        if let Some(previous) = self.idempotency.get(&frame.id) {
            return Ok(previous.clone());
        }
        let command_id = frame.id.clone();
        if frame.version != WIRE_VERSION {
            return Err(ServerError::InvalidFrame(
                "unsupported wire version".to_owned(),
            ));
        }
        let result = match frame.kind {
            FrameType::Standard(StandardFrameType::SessionAttach) => self.attach(frame),
            FrameType::Standard(StandardFrameType::Ack) => self.ack(frame),
            FrameType::Standard(StandardFrameType::UserInput) => self.user_input(frame),
            FrameType::Standard(StandardFrameType::CancelTurn) => self.cancel_turn(frame),
            FrameType::Standard(StandardFrameType::SessionDetach) => self.detach(frame),
            FrameType::Standard(StandardFrameType::SessionClose) => self.close(frame),
            _ => Err(ServerError::InvalidFrame("unsupported command".to_owned())),
        }?;
        self.idempotency.insert(command_id, result.clone());
        Ok(result)
    }

    fn attach(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        let attach: raep_types::SessionAttach = serde_json::from_value(frame.payload)
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let session_id = if frame.session_id.is_empty() {
            format!("session-{}", FrameId::generate())
        } else {
            frame.session_id.clone()
        };
        let id = EntityId::new(session_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let created = self.store.load(&id).map_err(ServerError::Store)?.is_none();
        if created && attach.create_if_missing == Some(false) {
            return Err(ServerError::InvalidFrame(
                "session not found and create_if_missing is false".to_owned(),
            ));
        }
        if created {
            let session = raep_domain::Session::new_with_agent(id.clone(), &attach.agent_id);
            self.store
                .commit(session, Vec::new())
                .map_err(ServerError::Store)?;
        } else {
            let session = self
                .store
                .load(&id)
                .map_err(ServerError::Store)?
                .ok_or_else(|| {
                    ServerError::InvalidFrame("session disappeared during attach".to_owned())
                })?;
            if session.agent_id() != attach.agent_id {
                return Err(ServerError::InvalidFrame(
                    "agent_id does not match the session agent".to_owned(),
                ));
            }
        }
        self.attached.insert(session_id.clone());
        self.sequences.entry(session_id.clone()).or_default();
        self.replay.entry(session_id.clone()).or_insert_with(|| {
            ReplayWindow::new(self.config.replay_capacity).expect("validated capacity")
        });
        let mode = attach.mode.unwrap_or(AttachMode::Controller);
        let attached = self.event(
            &session_id,
            StandardFrameType::SessionAttached,
            json!({
                "created": created,
                "mode": mode,
                "last_seq": self.sequences[&session_id].peek().saturating_sub(1),
                "negotiated_capabilities": self.config.capabilities,
            }),
        )?;
        let state = self
            .store
            .load(&id)
            .map_err(ServerError::Store)?
            .ok_or_else(|| {
                ServerError::InvalidFrame("session disappeared during attach".to_owned())
            })?;
        let status = match state.status() {
            SessionStatus::Idle | SessionStatus::Active => SessionState::Active,
            SessionStatus::Closed => SessionState::Closed,
        };
        let snapshot = self.event(
            &session_id,
            StandardFrameType::SessionState,
            json!({"status": status}),
        )?;
        let mut output = vec![attached, snapshot];
        if let Some(after) = attach.resume_from_seq {
            let replay = self.replay[&session_id]
                .replay_after(after)
                .map_err(|error| ServerError::Replay(error.to_string()))?;
            output.extend(replay.into_iter().map(|(_, event)| event));
        }
        Ok(output)
    }

    fn user_input(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.require_attached(&frame.session_id)?;
        let input: UserInput = serde_json::from_value(frame.payload)
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        input
            .validate()
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let session_id = EntityId::new(frame.session_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let turn_id = frame
            .turn_id
            .as_ref()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_else(|| FrameId::generate().to_string());
        let turn_id_wire = raep_types::TurnId::new(turn_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let turn_id =
            EntityId::new(turn_id).map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let session = self.store.load(&session_id).map_err(ServerError::Store)?;
        if session.is_none() {
            return Err(ServerError::InvalidFrame("session not found".to_owned()));
        }
        let mut current =
            session.ok_or_else(|| ServerError::InvalidFrame("session not found".to_owned()))?;
        current
            .start_turn(turn_id)
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        self.store
            .commit(current, Vec::new())
            .map_err(ServerError::Store)?;
        let mut event = self.event(
            &frame.session_id,
            StandardFrameType::TurnStateChanged,
            json!({"state": WireTurnState::Queued}),
        )?;
        event.turn_id = Some(turn_id_wire);
        Ok(vec![event])
    }

    fn ack(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.require_attached(&frame.session_id)?;
        let last_seq = frame
            .payload
            .get("last_seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| ServerError::InvalidFrame("ACK.last_seq is required".to_owned()))?;
        if let Some(window) = self.replay.get_mut(&frame.session_id) {
            window.acknowledge(last_seq);
        }
        Ok(Vec::new())
    }

    fn cancel_turn(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.require_attached(&frame.session_id)?;
        let session_id = EntityId::new(frame.session_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let mut session = self
            .store
            .load(&session_id)
            .map_err(ServerError::Store)?
            .ok_or_else(|| ServerError::InvalidFrame("session not found".to_owned()))?;
        let turn_id = session
            .active_turn()
            .map(|turn| turn.id().as_str().to_owned())
            .ok_or_else(|| ServerError::InvalidFrame("session has no active turn".to_owned()))?;
        session
            .transition_turn(raep_domain::TurnState::Cancelled)
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        self.store
            .commit(session, Vec::new())
            .map_err(ServerError::Store)?;
        let mut event = self.event(
            &frame.session_id,
            StandardFrameType::TurnStateChanged,
            json!({"state": WireTurnState::Cancelled}),
        )?;
        event.turn_id = Some(
            raep_types::TurnId::new(turn_id)
                .map_err(|error| ServerError::InvalidFrame(error.to_string()))?,
        );
        Ok(vec![event])
    }

    fn close(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.require_attached(&frame.session_id)?;
        let session_id = EntityId::new(frame.session_id.clone())
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        let mut session = self
            .store
            .load(&session_id)
            .map_err(ServerError::Store)?
            .ok_or_else(|| ServerError::InvalidFrame("session not found".to_owned()))?;
        session
            .close()
            .map_err(|error| ServerError::InvalidFrame(error.to_string()))?;
        self.store
            .commit(session, Vec::new())
            .map_err(ServerError::Store)?;
        self.attached.remove(&frame.session_id);
        let mut event = self.event(
            &frame.session_id,
            StandardFrameType::SessionState,
            json!({"status": SessionState::Closed}),
        )?;
        event.session_id = frame.session_id;
        Ok(vec![event])
    }

    fn detach(
        &mut self,
        frame: Envelope<Value>,
    ) -> Result<Vec<Envelope<Value>>, ServerError<S::Error>> {
        self.attached.remove(&frame.session_id);
        Ok(Vec::new())
    }

    fn require_attached(&self, session_id: &str) -> Result<(), ServerError<S::Error>> {
        if self.attached.contains(session_id) {
            Ok(())
        } else {
            Err(ServerError::InvalidFrame(
                "session is not attached".to_owned(),
            ))
        }
    }

    fn event(
        &mut self,
        session_id: &str,
        kind: StandardFrameType,
        payload: Value,
    ) -> Result<Envelope<Value>, ServerError<S::Error>> {
        let sequence = self
            .sequences
            .entry(session_id.to_owned())
            .or_default()
            .allocate();
        let mut event = Envelope::new(standard(kind), payload, self.clock.now_millis());
        event.session_id = session_id.to_owned();
        event.seq = Some(sequence);
        self.replay
            .get_mut(session_id)
            .ok_or_else(|| {
                ServerError::InvalidFrame("session replay window is missing".to_owned())
            })?
            .append(sequence, event.clone())
            .map_err(|error| ServerError::Replay(error.to_string()))?;
        Ok(event)
    }
}

fn standard(kind: StandardFrameType) -> FrameType {
    FrameType::Standard(kind)
}
