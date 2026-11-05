//! Domain entities and invariants for a RAEP session.
//!
//! Modules are split by aggregate and shared value concepts. The domain does
//! not know how commands arrive, where state is stored, or how execution runs.

#![warn(missing_docs)]

mod events;
mod identity;
mod job;
mod session;
mod states;
mod turn;

pub use events::*;
pub use identity::*;
pub use job::*;
pub use session::*;
pub use states::*;
pub use turn::*;
