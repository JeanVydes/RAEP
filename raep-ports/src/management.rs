#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_domain::{EntityId, Session};
use raep_types::JsonPayload;

/// Management query for sessions.
pub trait SessionCatalog {
    /// Adapter-specific management failure.
    type Error: AdapterError;

    /// Lists sessions visible to a principal.
    fn list<'a>(
        &'a self,
        principal: &'a str,
        filter: Option<&'a str>,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Session>, Self::Error>>;

    /// Applies a management patch to a session.
    fn patch<'a>(
        &'a mut self,
        session_id: &'a EntityId,
        patch: JsonPayload,
    ) -> BoxFuture<'a, Result<Session, Self::Error>>;
}

/// A scheduled or externally triggered run.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledRun {
    /// Stable run identifier.
    pub id: String,
    /// Session to execute.
    pub session_id: EntityId,
    /// Trigger name.
    pub trigger: String,
    /// Opaque run configuration.
    pub configuration: JsonPayload,
}

/// Schedules and starts headless runs.
pub trait RunScheduler {
    /// Adapter-specific scheduler failure.
    type Error: AdapterError;

    /// Creates a scheduled run.
    fn schedule<'a>(
        &'a mut self,
        run: ScheduledRun,
    ) -> BoxFuture<'a, Result<ScheduledRun, Self::Error>>;

    /// Cancels a scheduled run.
    fn cancel<'a>(&'a mut self, id: &'a str) -> BoxFuture<'a, Result<(), Self::Error>>;
}
