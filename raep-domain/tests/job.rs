use raep_domain::{DomainError, EntityId, Job, JobStatus};

fn id(value: &str) -> EntityId {
    EntityId::new(value).unwrap()
}

#[test]
fn job_finishes_once_with_a_terminal_status() {
    let mut job = Job::new(id("job"), id("tool-call"));
    assert_eq!(job.status(), JobStatus::Running);
    job.finish(JobStatus::Completed).unwrap();
    assert!(job.status().is_terminal());
    assert_eq!(
        job.finish(JobStatus::Failed),
        Err(DomainError::InvalidJobTransition {
            from: JobStatus::Completed,
            to: JobStatus::Failed
        })
    );
}

#[test]
fn job_can_be_killed_or_timed_out() {
    let mut killed = Job::new(id("job-killed"), id("tool-call"));
    killed.finish(JobStatus::Killed).unwrap();
    assert_eq!(killed.status(), JobStatus::Killed);

    let mut timed_out = Job::new(id("job-timeout"), id("tool-call"));
    timed_out.finish(JobStatus::TimedOut).unwrap();
    assert_eq!(timed_out.status(), JobStatus::TimedOut);
}
