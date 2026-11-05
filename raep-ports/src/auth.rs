#![warn(missing_docs)]

use crate::{AdapterError, Authorization, BoxFuture};
use raep_domain::Scope;

/// Authorization port for sessions, tools and management operations.
pub trait Authorizer {
    /// Adapter-specific failure.
    type Error: AdapterError;

    /// Checks whether a principal has a required scope.
    fn authorize<'a>(
        &'a self,
        principal: &'a str,
        scope: &'a Scope,
    ) -> BoxFuture<'a, Result<Authorization, Self::Error>>;
}
