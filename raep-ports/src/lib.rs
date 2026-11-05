//! Hexagonal ports used by RAEP application services.
//!
//! Each submodule owns one integration boundary. The crate reexports the
//! contracts so consumers can use either a flat or namespaced import style.

#![warn(missing_docs)]

use std::future::Future;
use std::pin::Pin;

/// A boxed asynchronous result that does not select a runtime.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

mod artifacts;
mod auth;
mod events;
mod jobs;
mod management;
mod notifications;
mod policy;
mod provider;
mod security;
mod tools;
mod transport;

pub use artifacts::*;
pub use auth::*;
pub use events::*;
pub use jobs::*;
pub use management::*;
pub use notifications::*;
pub use policy::*;
pub use provider::*;
pub use security::*;
pub use tools::*;
pub use transport::*;
