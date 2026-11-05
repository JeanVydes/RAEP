#![warn(missing_docs)]

use raep_domain::{EntityId, Job};
use raep_ports::JobStore;
use raep_types::{JobResult, OutputChunk};
use std::collections::HashMap;
use std::convert::Infallible;

/// In-memory detached job store.
#[derive(Clone, Debug, Default)]
pub struct MemoryJobStore {
    jobs: HashMap<EntityId, Job>,
    output: HashMap<EntityId, Vec<OutputChunk>>,
    results: HashMap<EntityId, JobResult>,
}

impl MemoryJobStore {
    /// Returns retained output for a job.
    pub fn output(&self, id: &EntityId) -> &[OutputChunk] {
        self.output.get(id).map_or(&[], Vec::as_slice)
    }
}

impl JobStore for MemoryJobStore {
    type Error = Infallible;

    fn load(&self, id: &EntityId) -> Result<Option<Job>, Self::Error> {
        Ok(self.jobs.get(id).cloned())
    }

    fn commit(
        &mut self,
        job: Job,
        output: Vec<OutputChunk>,
        result: Option<JobResult>,
    ) -> Result<(), Self::Error> {
        let id = job.id().clone();
        self.jobs.insert(id.clone(), job);
        self.output.entry(id.clone()).or_default().extend(output);
        if let Some(result) = result {
            self.results.insert(id, result);
        }
        Ok(())
    }

    fn output_after(
        &self,
        id: &EntityId,
        index: u64,
        limit: usize,
    ) -> Result<Vec<OutputChunk>, Self::Error> {
        Ok(self
            .output
            .get(id)
            .into_iter()
            .flatten()
            .filter(|chunk| chunk.index > index)
            .take(limit)
            .cloned()
            .collect())
    }
}
