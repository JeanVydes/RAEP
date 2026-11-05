//! Carrier codecs for RAEP.
//!
//! This crate implements the protocol framing rules for WebSocket text
//! messages and HTTP Server-Sent Events without choosing a socket, HTTP
//! server, TLS stack, or asynchronous runtime.

#![warn(missing_docs)]

mod sse;
mod websocket;

pub use sse::*;
pub use websocket::*;
