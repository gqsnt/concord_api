//! Manifest continuation guards preserve discovered jobs and discovery progress.

mod support;

use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{
    Budget, JobOptions, JobsError, Manifest, OnError, Options, Outcome, Resume, Unfinished,
    manifest_jobs,
};
use support::{Server, now};

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn options(discovery: usize, jobs: usize, policy: OnError) -> JobOptions {
    JobOptions::new(
        Budget::new(n(discovery)),
        Options::new(Budget::new(n(jobs)))
            .window(n(2))
            .on_error(policy),
    )
}

#[test]
fn immediate_manifest_repeat_is_typed_value_free_and_not_another_call() {
    let calls = Cell::new(0);
    let mut jobs = manifest_jobs(
        Resume::At("private-manifest"),
        options(5, 10, OnError::Stop),
        |key| {
            calls.set(calls.get() + 1);
            ready(Ok::<_, Infallible>(Manifest::<u32, _>::new(vec![], key)))
        },
        |id| ready(Ok::<_, Infallible>(id)),
    )
    .reject_repeated_keys();
    let error = now(jobs.next()).unwrap().unwrap().unwrap_err();
    assert!(matches!(error, JobsError::RepeatedKey));
    assert_eq!(format!("{error:?}"), "RepeatedKey");
    assert!(!format!("{error}").contains("private-manifest"));
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(calls.get(), 1);
    assert_eq!(jobs.progress().discovery_calls, 1);
    assert_eq!(jobs.progress().jobs.failed, 0);
    assert_eq!(
        jobs.checkpoint().discovery,
        Some(Resume::At("private-manifest"))
    );
    assert!(now(jobs.next()).unwrap().is_none());
    assert!(now(jobs.next()).unwrap().is_none());
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryRepeatedKey);
    assert!(report.outcome.is_discovery_incomplete());
    assert_eq!(
        report.discovery.resume,
        Some(Resume::At("private-manifest"))
    );
    assert!(report.discovery.failed.is_none());
    assert_eq!(calls.get(), 1);
}

#[test]
fn a_longer_manifest_cycle_preserves_history_on_reenabling_and_resume_starts_fresh() {
    let manifests = Server::<Option<u32>, Result<Manifest<u32, u32>, Infallible>>::new();
    let mut jobs = manifest_jobs(
        Resume::Start,
        options(5, 10, OnError::Stop),
        |key| manifests.call(key),
        |id| ready(Ok::<_, Infallible>(id)),
    )
    .reject_repeated_keys();
    for (key, next) in [(None, 1), (Some(1), 2), (Some(2), 1)] {
        assert!(now(jobs.next()).is_none());
        manifests.open(&key, Ok(Manifest::new(vec![], Some(next))));
        if key != Some(2) {
            assert!(now(jobs.next()).is_none());
            jobs = jobs.reject_repeated_keys();
        }
    }
    assert!(matches!(
        now(jobs.next()).unwrap(),
        Some(Err(JobsError::RepeatedKey))
    ));
    assert_eq!(manifests.calls(), [None, Some(1), Some(2)]);
    assert!(now(jobs.next()).unwrap().is_none());
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryRepeatedKey);
    assert_eq!(report.discovery.calls, 3);
    assert!(report.discovery.failed.is_none());

    let mut resumed = manifest_jobs(
        report.discovery.resume.unwrap(),
        options(5, 10, OnError::Stop),
        |key| {
            assert_eq!(key, Some(1));
            ready(Ok::<_, Infallible>(Manifest::new(vec![9], None)))
        },
        |id| ready(Ok::<_, Infallible>(id)),
    )
    .reject_repeated_keys();
    assert_eq!(now(resumed.next()).unwrap().unwrap().unwrap().value, 9);
    assert!(now(resumed.next()).unwrap().is_none());
    assert_eq!(resumed.finish().outcome, Outcome::Exhausted);
}

#[test]
fn discovery_and_job_caps_precede_repeat_refusal_and_exact_exhaustion_stays_success() {
    for cap in [1, 2] {
        let mut jobs = manifest_jobs(
            Resume::At(1),
            options(cap, 10, OnError::Stop),
            |key| ready(Ok::<_, Infallible>(Manifest::<u32, _>::new(vec![], key))),
            |id| ready(Ok::<_, Infallible>(id)),
        )
        .reject_repeated_keys();
        if cap == 1 {
            assert!(now(jobs.next()).unwrap().is_none());
            assert_eq!(jobs.finish().outcome, Outcome::DiscoveryBounded);
        } else {
            assert!(matches!(
                now(jobs.next()).unwrap(),
                Some(Err(JobsError::RepeatedKey))
            ));
            assert!(now(jobs.next()).unwrap().is_none());
            assert_eq!(jobs.finish().outcome, Outcome::DiscoveryRepeatedKey);
        }
    }
    let mut jobs = manifest_jobs(
        Resume::At(1),
        options(5, 1, OnError::Stop),
        |key| ready(Ok::<_, Infallible>(Manifest::new(vec![7], key))),
        |id| ready(Ok::<_, Infallible>(id)),
    )
    .reject_repeated_keys();
    assert_eq!(now(jobs.next()).unwrap().unwrap().unwrap().value, 7);
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(jobs.finish().outcome, Outcome::Bounded);

    let mut jobs = manifest_jobs(
        Resume::<u32>::Start,
        options(1, 1, OnError::Stop),
        |_| ready(Ok::<_, Infallible>(Manifest::new(vec![7], None))),
        |id| ready(Ok::<_, Infallible>(id)),
    )
    .reject_repeated_keys();
    assert_eq!(now(jobs.next()).unwrap().unwrap().unwrap().value, 7);
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(jobs.finish().outcome, Outcome::Exhausted);
}

#[test]
fn stop_cancels_admitted_jobs_and_retains_queued_descriptors_on_refusal() {
    let downloads = Server::<u32, Result<u32, Infallible>>::new();
    let calls = Cell::new(0);
    let mut jobs = manifest_jobs(
        Resume::At(1),
        options(5, 10, OnError::Stop),
        |key| {
            calls.set(calls.get() + 1);
            ready(Ok::<_, Infallible>(Manifest::new(vec![10, 11, 12], key)))
        },
        |id| downloads.call(id),
    )
    .reject_repeated_keys();
    assert!(matches!(
        now(jobs.next()).unwrap(),
        Some(Err(JobsError::RepeatedKey))
    ));
    assert!(downloads.dropped(&10) && downloads.dropped(&11));
    assert!(now(jobs.next()).unwrap().is_none());
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryRepeatedKey);
    assert_eq!(report.queued, [12]);
    assert_eq!(
        report
            .unfinished
            .iter()
            .map(|job| (job.key, job.value))
            .collect::<Vec<_>>(),
        [(10, Unfinished::Cancelled), (11, Unfinished::Cancelled),]
    );
    assert_eq!(report.discovery.calls, 1);
    assert_eq!(report.discovery.resume, Some(Resume::At(1)));
    assert!(report.discovery.failed.is_none());
    assert_eq!(report.progress.jobs.failed, 0);
    assert_eq!(calls.get(), 1);
}

#[test]
fn continue_finishes_known_jobs_and_early_finish_is_stopped() {
    for stop_early in [false, true] {
        let downloads = Server::<u32, Result<u32, Infallible>>::new();
        let mut jobs = manifest_jobs(
            Resume::At(1),
            options(5, 10, OnError::Continue),
            |key| ready(Ok::<_, Infallible>(Manifest::new(vec![10, 11, 12], key))),
            |id| downloads.call(id),
        )
        .reject_repeated_keys();
        assert!(matches!(
            now(jobs.next()).unwrap(),
            Some(Err(JobsError::RepeatedKey))
        ));
        assert!(!downloads.dropped(&10) && !downloads.dropped(&11));
        if !stop_early {
            let mut prefix = Vec::new();
            for id in [10, 11, 12] {
                if !downloads.calls().contains(&id) {
                    assert!(now(jobs.next()).is_none());
                }
                downloads.open(&id, Ok(id));
                prefix.push(now(jobs.next()).unwrap().unwrap().unwrap().value);
            }
            assert_eq!(prefix, [10, 11, 12]);
            assert!(now(jobs.next()).unwrap().is_none());
        }
        let report = jobs.finish();
        assert_eq!(
            report.outcome,
            if stop_early {
                Outcome::Stopped
            } else {
                Outcome::DiscoveryRepeatedKey
            }
        );
        assert_eq!(report.discovery.calls, 1);
        assert_eq!(report.discovery.resume, Some(Resume::At(1)));
        assert!(report.discovery.failed.is_none());
        assert!(report.discovery.repeated_key);
        if stop_early {
            assert_eq!(report.queued, [12]);
            assert!(downloads.dropped(&10) && downloads.dropped(&11));
        } else {
            assert_eq!(report.progress.jobs.succeeded, 3);
            assert!(report.queued.is_empty() && report.unfinished.is_empty());
        }
    }
}

#[test]
fn default_manifest_keys_need_no_hash_or_equality_and_add_no_clones() {
    struct Key<'a>(&'a Cell<usize>);
    impl Clone for Key<'_> {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0)
        }
    }
    let clones = Cell::new(0);
    let calls = RefCell::new(Vec::new());
    let mut jobs = manifest_jobs(
        Resume::At(Key(&clones)),
        options(2, 10, OnError::Stop),
        |key| {
            calls.borrow_mut().push(());
            ready(Ok::<_, Infallible>(Manifest::<u32, _>::new(vec![], key)))
        },
        |id| ready(Ok::<_, Infallible>(id)),
    );
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(clones.get(), 2); // The two existing factory-argument copies only.
    assert_eq!(jobs.finish().outcome, Outcome::DiscoveryBounded);
}
