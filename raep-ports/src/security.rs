#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_domain::Scope;

/// Authenticated caller identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Principal {
    /// Stable subject identifier.
    pub subject: String,
    /// Tenant or organization boundary.
    pub tenant: Option<String>,
    /// Provider-specific claims.
    pub claims: serde_json::Value,
}

/// Authentication result.
#[derive(Clone, Debug, PartialEq)]
pub struct Authentication {
    /// Authenticated principal.
    pub principal: Principal,
    /// Token expiry in Unix milliseconds, if known.
    pub expires_at_millis: Option<i64>,
}

/// Validates connection and refresh credentials.
pub trait Authenticator {
    /// Adapter-specific authentication failure.
    type Error: AdapterError;

    /// Authenticates a credential without retaining it.
    fn authenticate<'a>(
        &'a self,
        credential: &'a str,
    ) -> BoxFuture<'a, Result<Authentication, Self::Error>>;

    /// Refreshes an expiring credential.
    fn refresh<'a>(
        &'a self,
        credential: &'a str,
    ) -> BoxFuture<'a, Result<Authentication, Self::Error>>;
}

/// Exclusive ownership lease for a session or job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    /// Resource being leased.
    pub resource: String,
    /// Lease owner.
    pub owner: String,
    /// Fencing token.
    pub fencing_token: u64,
    /// Expiry in Unix milliseconds.
    pub expires_at_millis: i64,
}

/// Durable lease and failover boundary.
pub trait LeaseStore {
    /// Adapter-specific lease failure.
    type Error: AdapterError;

    /// Attempts to acquire or renew a lease.
    fn acquire<'a>(
        &'a mut self,
        resource: &'a str,
        owner: &'a str,
        ttl_millis: i64,
    ) -> BoxFuture<'a, Result<Lease, Self::Error>>;

    /// Releases a lease only when the fencing token matches.
    fn release<'a>(
        &'a mut self,
        resource: &'a str,
        owner: &'a str,
        fencing_token: u64,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;

    /// Checks a required scope for a principal.
    fn scope<'a>(
        &'a self,
        principal: &'a Principal,
        scope: &'a Scope,
    ) -> BoxFuture<'a, Result<Authorization, Self::Error>>;
}

/// Authorization result shared by security adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Authorization {
    /// The operation is allowed.
    Allowed,
    /// The operation is denied.
    Denied {
        /// Safe denial reason.
        reason: String,
    },
}
