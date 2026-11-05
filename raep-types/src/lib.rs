//! Public wire types for the Remote Agent Execution Protocol.
//!
//! The modules are organized by protocol responsibility; reexports preserve
//! a flat public API for callers that prefer it.

#![warn(missing_docs)]

/// Wire version implemented by this crate.
pub const WIRE_VERSION: &str = "1.0";

mod attach;
mod control;
mod envelope;
mod extensions;
mod handshake;
mod ids;
mod input;
mod operations;
mod tools;

pub use attach::*;
pub use control::*;
pub use envelope::*;
pub use extensions::*;
pub use handshake::*;
pub use ids::*;
pub use input::*;
pub use operations::*;
pub use tools::*;
