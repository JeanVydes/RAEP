#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};
use raep_domain::{EntityId, Job};
use raep_types::{JobResult, OutputChunk};

/// Durable storage for detached job state and retained output.
pub trait JobStore {
    /// Adapter-specific failure.
    type Error: AdapterError;

    /// Loads a job by identifier.
    fn load(&self, id: &EntityId) -> Result<Option<Job>, Self::Error>;

    /// Persists a job aggregate and optional output chunks atomically.
    fn commit(
        &mut self,
        job: Job,
        output: Vec<OutputChunk>,
        result: Option<JobResult>,
    ) -> Result<(), Self::Error>;

    /// Returns retained output after a chunk index.
    fn output_after(
        &self,
        id: &EntityId,
        index: u64,
        limit: usize,
    ) -> Result<Vec<OutputChunk>, Self::Error>;
}

/// Process-independent detached job runner.
pub trait JobRunnerPort {
    /// Adapter-specific failure.
    type Error: AdapterError;

    /// Starts a detached job.
    fn spawn<'a>(&'a mut self, job: Job, command: String)
        -> BoxFuture<'a, Result<(), Self::Error>>;

    /// Requests graceful then forced cancellation according to adapter policy.
    fn cancel<'a>(&'a mut self, job_id: &'a EntityId) -> BoxFuture<'a, Result<(), Self::Error>>;
}
