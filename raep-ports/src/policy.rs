#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};

/// Resource dimensions used by quota policies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuotaDimension {
    /// Input tokens.
    InputTokens,
    /// Output tokens.
    OutputTokens,
    /// Provider spend in the host's accounting unit.
    ProviderCost,
    /// Concurrent sessions.
    Sessions,
    /// Concurrent jobs.
    Jobs,
    /// Stored bytes.
    StorageBytes,
}

/// Quota decision.
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaDecision {
    /// Whether the requested amount may be consumed.
    pub allowed: bool,
    /// Remaining amount after the decision.
    pub remaining: Option<f64>,
    /// Optional retry delay.
    pub retry_after_ms: Option<u64>,
}

/// Host-owned quota and rate-limit policy.
pub trait UsagePolicy {
    /// Adapter-specific policy failure.
    type Error: AdapterError;

    /// Checks and optionally reserves a quota amount.
    fn reserve<'a>(
        &'a mut self,
        principal: &'a str,
        dimension: QuotaDimension,
        amount: f64,
    ) -> BoxFuture<'a, Result<QuotaDecision, Self::Error>>;

    /// Checks command rate without mutating protocol state.
    fn rate_limit<'a>(
        &'a mut self,
        principal: &'a str,
        operation: &'a str,
    ) -> BoxFuture<'a, Result<QuotaDecision, Self::Error>>;
}
