//! Axum bindings for RAEP.
//!
//! The host still owns the listener, TLS, authentication middleware and
//! provider/storage implementations. This crate maps HTTP/WebSocket carriers
//! to the runtime-independent [`raep_server::ProtocolServer`].

#![warn(missing_docs)]

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, StreamExt};
use raep_server::ProtocolServer;
use raep_types::{Envelope, FrameType, SessionAttach, StandardFrameType};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// WebSocket subprotocol required by RAEP.
pub const WEBSOCKET_PROTOCOL: &str = "raep.v1";

#[derive(Debug, Deserialize)]
struct EventQuery {
    agent_id: Option<String>,
    mode: Option<raep_types::AttachMode>,
    create_if_missing: Option<bool>,
    takeover: Option<bool>,
}

/// Shared state used by the HTTP handlers.
pub struct SharedServer<S, C>
where
    S: raep_ports::SessionStore,
    C: raep_ports::Clock,
{
    /// Protocol dispatcher.
    pub server: Arc<Mutex<ProtocolServer<S, C>>>,
    events: broadcast::Sender<Envelope<Value>>,
}

impl<S, C> SharedServer<S, C>
where
    S: raep_ports::SessionStore,
    C: raep_ports::Clock,
{
    /// Creates HTTP state with a bounded in-process event fanout.
    pub fn new(server: ProtocolServer<S, C>, event_capacity: usize) -> Result<Self, &'static str> {
        if event_capacity == 0 {
            return Err("event capacity must be positive");
        }
        let (events, _) = broadcast::channel(event_capacity);
        Ok(Self {
            server: Arc::new(Mutex::new(server)),
            events,
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<Envelope<Value>> {
        self.events.subscribe()
    }

    fn publish(&self, frames: &[Envelope<Value>]) {
        for frame in frames {
            let _ = self.events.send(frame.clone());
        }
    }
}

/// Builds the RFC transport routes.
pub fn router<S, C>(state: Arc<SharedServer<S, C>>) -> Router
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    Router::new()
        .route("/raep/v1/ws", get(websocket::<S, C>))
        .route("/raep/v1/info", get(info::<S, C>))
        .route("/raep/v1/health/live", get(health_live))
        .route("/raep/v1/health/ready", get(health_ready::<S, C>))
        .route("/raep/v1/sessions", post(create_session::<S, C>))
        .route(
            "/raep/v1/sessions/{session_id}",
            get(session_resource::<S, C>).delete(delete_session::<S, C>),
        )
        .route("/raep/v1/sessions/{session_id}/events", get(events::<S, C>))
        .route(
            "/raep/v1/sessions/{session_id}/commands",
            post(commands::<S, C>),
        )
        .with_state(state)
}

async fn websocket<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    mut upgrade: WebSocketUpgrade,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let offered = upgrade
        .requested_protocols()
        .any(|protocol| protocol == HeaderValue::from_static(WEBSOCKET_PROTOCOL));
    if !offered {
        return (
            StatusCode::BAD_REQUEST,
            "Sec-WebSocket-Protocol raep.v1 is required",
        )
            .into_response();
    }
    let max_frame_bytes = state.server.lock().await.max_frame_bytes();
    upgrade = upgrade
        .protocols([WEBSOCKET_PROTOCOL])
        .max_frame_size(max_frame_bytes);
    upgrade
        .on_upgrade(move |socket| websocket_session(state, socket))
        .into_response()
}

async fn websocket_session<S, C>(state: Arc<SharedServer<S, C>>, mut socket: WebSocket)
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let hello = {
        let server = state.server.lock().await;
        serde_json::to_string(&server.hello()).ok()
    };
    if let Some(hello) = hello {
        if socket.send(Message::Text(hello.into())).await.is_err() {
            return;
        }
    }
    while let Some(Ok(message)) = socket.recv().await {
        let response = match message {
            Message::Text(text) => {
                let result = {
                    let mut server = state.server.lock().await;
                    serde_json::from_str::<Envelope<Value>>(&text)
                        .map_err(|error| error.to_string())
                        .and_then(|frame| server.handle(frame).map_err(|error| error.to_string()))
                };
                match result {
                    Ok(frames) => {
                        state.publish(&frames);
                        frames
                    }
                    Err(error) => {
                        let error = error_frame(error);
                        let Ok(text) = serde_json::to_string(&error) else {
                            return;
                        };
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                        continue;
                    }
                }
            }
            Message::Binary(_) => {
                let _ = socket
                    .send(Message::Close(Some(CloseFrame {
                        code: 1003,
                        reason: "RAEP requires text JSON frames".into(),
                    })))
                    .await;
                return;
            }

            Message::Ping(payload) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    return;
                }
                continue;
            }
            Message::Pong(_) => continue,
            Message::Close(_) => return,
        };
        for frame in response {
            let Ok(text) = serde_json::to_string(&frame) else {
                return;
            };
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
}

async fn info<S, C>(State(state): State<Arc<SharedServer<S, C>>>) -> Json<Value>
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    Json(state.server.lock().await.info())
}

async fn health_live() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn health_ready<S, C>(State(state): State<Arc<SharedServer<S, C>>>) -> StatusCode
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let server = state.server.lock().await;
    if server.capabilities().is_empty() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::NO_CONTENT
    }
}

async fn events<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    Path(session_id): Path<String>,
    Query(query): Query<EventQuery>,
    headers: HeaderMap,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let last_seen = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let receiver = state.subscribe();
    let attach = Envelope::new(
        FrameType::Standard(StandardFrameType::SessionAttach),
        serde_json::to_value(SessionAttach {
            agent_id: query.agent_id.unwrap_or_else(|| "default".to_owned()),
            agent_override: None,
            create_if_missing: query.create_if_missing,
            mode: query.mode,
            takeover: query.takeover,
            resume_from_seq: Some(last_seen).filter(|sequence| *sequence > 0),
            capabilities: None,
            client_info: None,
            options: None,
        })
        .unwrap_or_else(|_| json!({})),
        0,
    );
    let attach = Envelope {
        session_id: if session_id == "new" {
            String::new()
        } else {
            session_id.clone()
        },
        ..attach
    };
    let (hello, attached, attach_failed) = {
        let mut server = state.server.lock().await;
        let hello = server.hello();
        match server.handle(attach) {
            Ok(attached) => (hello, attached, None),
            Err(error) => (hello, Vec::new(), Some(error.to_string())),
        }
    };
    let stream_session_id = attached
        .iter()
        .find(|frame| frame.kind == FrameType::Standard(StandardFrameType::SessionAttached))
        .map(|frame| frame.session_id.clone())
        .unwrap_or_else(|| session_id.clone());
    let initial_frames = attach_failed
        .map(|error| vec![error_frame(error)])
        .unwrap_or(attached);
    let initial = stream::iter(
        std::iter::once(Ok::<_, std::convert::Infallible>(sse_event(hello)))
            .chain(initial_frames.into_iter().map(|frame| Ok(sse_event(frame)))),
    );
    let live = stream::unfold(
        (receiver, stream_session_id),
        |(mut receiver, session_id)| async move {
            loop {
                match receiver.recv().await {
                    Ok(frame) if frame.session_id == session_id => {
                        return Some((Ok(sse_event(frame)), (receiver, session_id)));
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        return Some((
                            Ok(Event::default()
                                .event("ERROR")
                                .data(json!({"code": "replay_truncated"}).to_string())),
                            (receiver, session_id),
                        ));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );
    let mut response = Sse::new(initial.chain(live))
        .keep_alive(KeepAlive::default())
        .into_response();
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

async fn session_resource<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    Path(session_id): Path<String>,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let id = match raep_domain::EntityId::new(session_id) {
        Ok(id) => id,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": {"code": "INVALID_FRAME", "message": error.to_string()}})),
            )
                .into_response();
        }
    };
    let server = state.server.lock().await;
    match server.load_session(&id) {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(json!({
                "session_id": session.id().as_str(),
                "status": format!("{:?}", session.status()).to_uppercase(),
                "active_turn": session.active_turn().map(|turn| json!({
                    "turn_id": turn.id().as_str(),
                    "state": format!("{:?}", turn.state()).to_uppercase(),
                })),
            })),
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "UNAVAILABLE", "message": error.to_string()}})),
        )
            .into_response(),
    }
}

async fn create_session<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    Json(payload): Json<raep_types::SessionCreate>,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    if payload.agent_id.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": {"code": "INVALID_FRAME", "message": "agent_id is required"}})),
        )
            .into_response();
    }
    let mut server = state.server.lock().await;
    match server.create_session(&payload.agent_id) {
        Ok(session_id) => (
            StatusCode::CREATED,
            Json(json!({"session_id": session_id, "agent_id": payload.agent_id})),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"code": "UNAVAILABLE", "message": error.to_string()}})),
        )
            .into_response(),
    }
}

async fn delete_session<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    Path(session_id): Path<String>,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    let id = match raep_domain::EntityId::new(session_id) {
        Ok(id) => id,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": {"code": "INVALID_FRAME", "message": error.to_string()}})),
            )
                .into_response();
        }
    };
    let mut server = state.server.lock().await;
    match server.close_session(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(raep_server::ServerError::InvalidFrame(message)) if message == "session not found" => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => (
            StatusCode::CONFLICT,
            Json(json!({"error": {"code": "SESSION_BUSY", "message": error.to_string()}})),
        )
            .into_response(),
    }
}

async fn commands<S, C>(
    State(state): State<Arc<SharedServer<S, C>>>,
    Path(session_id): Path<String>,
    Json(frame): Json<Envelope<Value>>,
) -> Response
where
    S: raep_ports::SessionStore + Send + 'static,
    C: raep_ports::Clock + Send + 'static,
    S::Error: Send + Sync,
{
    if session_id != "new" && frame.session_id != session_id {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "accepted": false,
                "error": {"code": "INVALID_FRAME", "message": "session_id does not match path"}
            })),
        )
            .into_response();
    }
    let result = {
        let mut server = state.server.lock().await;
        server.handle(frame).map_err(|error| error.to_string())
    };
    if let Ok(ref frames) = result {
        state.publish(frames);
    }
    match result {
        Ok(events) => (
            StatusCode::ACCEPTED,
            Json(json!({ "accepted": true, "events": events })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "accepted": false,
                "error": {"code": "INVALID_FRAME", "message": error}
            })),
        )
            .into_response(),
    }
}

fn sse_event(frame: Envelope<Value>) -> Event {
    let sequence = frame.seq;
    let event = serde_json::to_value(&frame.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "message".to_owned());
    let event = Event::default()
        .event(event)
        .json_data(frame)
        .unwrap_or_else(|_| Event::default().event("ERROR").data("{}"));
    match sequence {
        Some(sequence) => event.id(sequence.to_string()),
        None => event,
    }
}

fn error_frame(message: impl Into<String>) -> Envelope<Value> {
    Envelope::new(
        FrameType::Standard(StandardFrameType::Error),
        json!({
            "code": "INVALID_FRAME",
            "message": message.into(),
            "retryable": false,
        }),
        0,
    )
}

/// Returns whether a frame is a standard RAEP command.
pub fn is_standard(frame: &Envelope<Value>, expected: StandardFrameType) -> bool {
    frame.kind == FrameType::Standard(expected)
}

/// Creates HTTP state from an already configured protocol server.
pub fn shared_server<S, C>(
    server: ProtocolServer<S, C>,
    event_capacity: usize,
) -> Result<Arc<SharedServer<S, C>>, &'static str>
where
    S: raep_ports::SessionStore,
    C: raep_ports::Clock,
{
    if event_capacity == 0 {
        return Err("event capacity must be positive");
    }
    Ok(Arc::new(SharedServer::new(server, event_capacity)?))
}
