//! Transport-independent primitives for ordered and resumable delivery.

#![warn(missing_docs)]

mod errors;
mod idempotency;
mod replay;
mod sequencing;

pub use errors::*;
pub use idempotency::*;
pub use replay::*;
pub use sequencing::*;
