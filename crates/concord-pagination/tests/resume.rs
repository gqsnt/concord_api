//! Lossless report decomposition (`JobsReport::into_resume`, `IndependentReport::into_resume`):
//! every part of a report built by the real engines survives, with non-`Clone` results and
//! errors, and the derived rerun view keeps retained, attempted, queued and undiscovered work
//! apart.

mod support;

use std::num::NonZeroUsize;

use concord_pagination::{
    Budget, IndependentReport, JobOptions, JobsError, JobsReport, Keyed, Manifest, OnError,
    Options, Outcome, Resume, Unfinished, first_page_then, independent, manifest_jobs,
};
use support::{Server, now};

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn s(text: &str) -> String {
    text.to_owned()
}

/// A job receipt: neither `Clone` nor `Debug`.
struct Receipt(&'static str);

/// A job error: neither `Clone` nor `Debug`.
struct JobError(&'static str);

type ManifestReply = Result<Manifest<String, u32>, &'static str>;
type JobReply = Result<Receipt, JobError>;

fn manifest(urls: &[&str], next: Option<u32>) -> ManifestReply {
    Ok(Manifest::new(urls.iter().map(|url| s(url)).collect(), next))
}

fn states<K: Clone>(entries: &[Keyed<K, Unfinished>]) -> Vec<(u64, K, Unfinished)> {
    entries
        .iter()
        .map(|entry| (entry.index, entry.key.clone(), entry.value))
        .collect()
}

/// Ordered `Continue` jobs over two manifests, finished early, leaving at once: an emitted
/// failure (j1), cancelled jobs (j2, j5), a retained receipt (j3), an unemitted error (j4),
/// queued descriptors (j6, j7) and an undiscovered remainder after a failed second manifest.
fn jobs_with_every_kind_of_work() -> JobsReport<String, u32, Receipt, JobError> {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(100)))
            .window(n(4))
            .on_error(OnError::Continue),
    )
    .backlog(n(8));
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    manifests.open(
        &None,
        manifest(&["j1", "j2", "j3", "j4", "j5", "j6", "j7"], Some(2)),
    );
    assert!(now(jobs.next()).is_none());
    assert_eq!(downloads.call_count(), 4);
    assert_eq!(manifests.call_count(), 2); // the second manifest is in flight

    downloads.open(&s("j1"), Err(JobError("j1 broke")));
    let Some(Some(Err(JobsError::Job(failed)))) = now(jobs.next()) else {
        panic!("j1's failure is emitted");
    };
    assert_eq!((failed.key.as_str(), failed.value.0), ("j1", "j1 broke"));

    manifests.open(&Some(2), Err("manifest 2 failed"));
    let Some(Some(Err(JobsError::Discovery(error)))) = now(jobs.next()) else {
        panic!("the discovery failure is emitted");
    };
    assert_eq!(error, "manifest 2 failed");
    assert_eq!(downloads.call_count(), 5); // j5 took j1's slot

    downloads.open(&s("j3"), Ok(Receipt("shard:j3")));
    downloads.open(&s("j4"), Err(JobError("j4 broke")));
    assert!(now(jobs.next()).is_none()); // both wait behind the stalled j2
    let progress = jobs.progress().jobs;
    assert_eq!((progress.in_flight, progress.retained), (2, 2));
    jobs.finish()
}

#[test]
fn jobs_into_resume_keeps_every_part_of_the_report() {
    let report = jobs_with_every_kind_of_work();
    assert_eq!(report.outcome, Outcome::Stopped);
    let progress = report.progress;
    let unfinished = states(&report.unfinished);
    assert_eq!(
        unfinished,
        [
            (0, s("j1"), Unfinished::Failed),
            (1, s("j2"), Unfinished::Cancelled),
            (2, s("j3"), Unfinished::Retained),
            (3, s("j4"), Unfinished::Failed),
            (4, s("j5"), Unfinished::Cancelled),
        ]
    );

    let resume = report.into_resume();

    // Outcome and every counter.
    assert_eq!(resume.outcome, Outcome::Stopped);
    assert_eq!(resume.progress, progress);
    let jobs = resume.progress.jobs;
    assert_eq!(
        (
            jobs.admitted,
            jobs.emitted,
            jobs.succeeded,
            jobs.failed,
            jobs.cancelled,
            jobs.retained
        ),
        (5, 1, 0, 2, 2, 2)
    );
    assert_eq!(
        (resume.progress.discovery_calls, resume.progress.queued),
        (2, 2)
    );

    // Keep: the retained receipt, moved.
    assert_eq!(
        resume
            .retained
            .iter()
            .map(|done| (done.index, done.key.as_str(), done.value.0))
            .collect::<Vec<_>>(),
        [(2, "j3", "shard:j3")]
    );
    // The unemitted typed error, moved.
    assert_eq!(
        resume
            .unemitted_errors
            .iter()
            .map(|failed| (failed.index, failed.key.as_str(), failed.value.0))
            .collect::<Vec<_>>(),
        [(3, "j4", "j4 broke")]
    );
    // Attempted work to run again, with its classification; the retained job is not in it.
    assert_eq!(
        states(&resume.rerun),
        [
            (0, s("j1"), Unfinished::Failed),
            (1, s("j2"), Unfinished::Cancelled),
            (3, s("j4"), Unfinished::Failed),
            (4, s("j5"), Unfinished::Cancelled),
        ]
    );
    // Lossless: the rerun entries and the retained receipts are exactly the report's unfinished
    // entries.
    let mut rebuilt = states(&resume.rerun);
    rebuilt.extend(
        resume
            .retained
            .iter()
            .map(|done| (done.index, done.key.clone(), Unfinished::Retained)),
    );
    rebuilt.sort_by_key(|entry| entry.0);
    assert_eq!(rebuilt, unfinished);

    // Known, never attempted.
    assert_eq!(resume.queued, [s("j6"), s("j7")]);
    // Known work in discovery order, apart from the undiscovered remainder.
    assert_eq!(
        resume.known().map(String::as_str).collect::<Vec<_>>(),
        ["j1", "j2", "j4", "j5", "j6", "j7"]
    );
    assert_eq!(resume.undiscovered(), Some(&Resume::At(2)));
    // The whole discovery report.
    assert_eq!(resume.discovery.calls, 2);
    assert_eq!(resume.discovery.resume, Some(Resume::At(2)));
    assert_eq!(resume.discovery.failed, Some(Resume::At(2)));
}

#[test]
fn jobs_into_rerun_is_unchanged_and_agrees_with_into_resume() {
    let (retained, rerun) = jobs_with_every_kind_of_work().into_rerun();
    assert_eq!(
        retained
            .iter()
            .map(|done| (done.key.as_str(), done.value.0))
            .collect::<Vec<_>>(),
        [("j3", "shard:j3")]
    );
    assert_eq!(rerun, ["j1", "j2", "j4", "j5", "j6", "j7"]);

    let resume = jobs_with_every_kind_of_work().into_resume();
    assert_eq!(resume.known().cloned().collect::<Vec<_>>(), rerun);
}

/// Ordered `Continue` keys finished early: an emitted failure (0), cancelled keys (1, 4), a
/// retained result (2), an unemitted error (3) and the plan's rest (5..10).
fn independent_with_every_kind_of_work()
-> IndependentReport<u32, std::ops::Range<u32>, Receipt, JobError> {
    let server = Server::<u32, JobReply>::new();
    let mut pages = independent(
        0..10,
        Options::new(Budget::new(n(100)))
            .window(n(4))
            .on_error(OnError::Continue),
        |key| server.call(key),
    );
    assert!(now(pages.next()).is_none());
    server.open(&0, Err(JobError("0 broke")));
    let Some(Some(Err(failed))) = now(pages.next()) else {
        panic!("key 0's failure is emitted");
    };
    assert_eq!((failed.key, failed.value.0), (0, "0 broke"));
    server.open(&2, Ok(Receipt("r2")));
    server.open(&3, Err(JobError("3 broke")));
    assert!(now(pages.next()).is_none());
    assert_eq!(server.call_count(), 5);
    pages.finish()
}

#[test]
fn independent_into_resume_keeps_every_part_of_the_report() {
    let report = independent_with_every_kind_of_work();
    let progress = report.progress;
    let unfinished = states(&report.unfinished);
    assert_eq!(
        unfinished,
        [
            (0, 0, Unfinished::Failed),
            (1, 1, Unfinished::Cancelled),
            (2, 2, Unfinished::Retained),
            (3, 3, Unfinished::Failed),
            (4, 4, Unfinished::Cancelled),
        ]
    );

    let resume = report.into_resume();
    assert_eq!(resume.outcome, Outcome::Stopped);
    assert_eq!(resume.progress, progress);
    assert_eq!(
        (
            progress.admitted,
            progress.emitted,
            progress.failed,
            progress.cancelled,
            progress.retained
        ),
        (5, 1, 2, 2, 2)
    );
    assert_eq!(
        resume
            .retained
            .iter()
            .map(|done| (done.index, done.key, done.value.0))
            .collect::<Vec<_>>(),
        [(2, 2, "r2")]
    );
    assert_eq!(
        resume
            .unemitted_errors
            .iter()
            .map(|failed| (failed.index, failed.key, failed.value.0))
            .collect::<Vec<_>>(),
        [(3, 3, "3 broke")]
    );
    assert_eq!(
        states(&resume.rerun),
        [
            (0, 0, Unfinished::Failed),
            (1, 1, Unfinished::Cancelled),
            (3, 3, Unfinished::Failed),
            (4, 4, Unfinished::Cancelled),
        ]
    );
    let mut rebuilt = states(&resume.rerun);
    rebuilt.extend(
        resume
            .retained
            .iter()
            .map(|done| (done.index, done.key, Unfinished::Retained)),
    );
    rebuilt.sort_by_key(|entry| entry.0);
    assert_eq!(rebuilt, unfinished);
    assert_eq!(
        resume.rest.map(Iterator::collect::<Vec<_>>),
        Some(vec![5, 6, 7, 8, 9])
    );

    // `into_rerun` is unchanged.
    let (retained, keys) = independent_with_every_kind_of_work().into_rerun();
    assert_eq!(retained.len(), 1);
    assert_eq!(keys.collect::<Vec<_>>(), [0, 1, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn a_failed_first_page_leaves_no_plan_and_the_resume_says_so() {
    let mut pages = first_page_then(
        1_u32,
        Options::new(Budget::new(n(10))),
        |_: u32| std::future::ready(Err::<Receipt, _>(JobError("first page failed"))),
        |_: &Receipt| Ok::<_, JobError>(2..5),
    );
    assert!(matches!(now(pages.next()), Some(Some(Err(_)))));
    assert!(now(pages.next()).unwrap().is_none());
    let resume = pages.finish().into_resume();
    assert_eq!(resume.outcome, Outcome::DiscoveryFailed);
    assert_eq!(states(&resume.rerun), [(0, 1, Unfinished::Failed)]);
    assert!(resume.rest.is_none()); // unknown, not empty
    assert!(resume.retained.is_empty() && resume.unemitted_errors.is_empty());
}

#[test]
fn resumes_never_print_keys_or_values() {
    let resume = jobs_with_every_kind_of_work().into_resume();
    let text = format!("{resume:?}");
    assert!(!text.contains("j1") && !text.contains("shard") && !text.contains("broke"));
    let resume = independent_with_every_kind_of_work().into_resume();
    let text = format!("{resume:?}");
    assert!(!text.contains("r2") && !text.contains("broke"));
}
