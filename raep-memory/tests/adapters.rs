use raep_domain::{DomainEvent, DomainEventKind, EntityId, Job, JobStatus, Scope, Session};
use raep_memory::{MemoryArtifactStore, MemoryAuthorizer, MemoryJobStore, MemorySessionStore};
use raep_ports::{ArtifactStore, Authorization, Authorizer, JobStore, SessionStore};
use raep_types::{JobResult, JobState, OutputChunk};

#[test]
fn memory_store_assigns_session_local_sequences() {
    let id = EntityId::new("session").unwrap();
    let mut store = MemorySessionStore::default();
    let event = || {
        DomainEvent::new(
            1,
            DomainEventKind::SessionCreated {
                session_id: id.clone(),
            },
        )
    };

    store
        .commit(Session::new(id.clone()), vec![event()])
        .unwrap();
    let events = store
        .commit(Session::new(id.clone()), vec![event()])
        .unwrap();

    assert_eq!(store.events(&id).len(), 2);
    assert_eq!(events[0].sequence(), 2);
    assert_eq!(store.events_after(&id, 1, 10).unwrap().len(), 1);
}

#[test]
fn memory_job_store_retains_output_after_commit() {
    let job_id = EntityId::new("job").unwrap();
    let mut job = Job::new(job_id.clone(), EntityId::new("call").unwrap());
    job.finish(JobStatus::Completed).unwrap();
    let mut store = MemoryJobStore::default();
    store
        .commit(
            job,
            vec![OutputChunk {
                stream: "stdout".to_owned(),
                index: 0,
                data: "done".to_owned(),
            }],
            Some(JobResult {
                status: JobState::Completed,
                exit_code: Some(0),
                summary: "done".to_owned(),
                output_truncated: false,
                output_bytes_total: 4,
            }),
        )
        .unwrap();
    assert_eq!(store.output_after(&job_id, 0, 10).unwrap().len(), 0);
    assert_eq!(store.output(&job_id).len(), 1);
}

#[test]
fn memory_authorizer_and_artifact_store_are_explicit_adapters() {
    let scope = Scope::new("session:read").unwrap();
    let mut authorizer = MemoryAuthorizer::default();
    authorizer.grant("alice", scope.clone());
    let allowed = futures_lite::future::block_on(authorizer.authorize("alice", &scope)).unwrap();
    assert_eq!(allowed, Authorization::Allowed);

    let mut artifacts = MemoryArtifactStore::default();
    let reference = futures_lite::future::block_on(artifacts.put("text/plain", b"hello")).unwrap();
    assert_eq!(
        futures_lite::future::block_on(artifacts.get(&reference.id)).unwrap(),
        Some(b"hello".to_vec())
    );
}
