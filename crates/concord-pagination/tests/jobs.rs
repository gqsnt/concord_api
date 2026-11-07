//! Manifest discovery and streamed jobs (P04, P06, P08, contract §3, §4 and §6): discovery and
//! backlog bounds, slots held through a stalled body, completed-but-unemitted receipts,
//! discovery-failure policy, keyed completion order, and job-owned staged output.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::num::NonZeroUsize;
use std::rc::Rc;

use concord_pagination::{
    Budget, JobOptions, JobsError, Manifest, OnError, Options, Order, Outcome, Resume, Unfinished,
    manifest_jobs,
};
use support::{Server, now};

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn job_options(discovery: usize, jobs: usize, window: usize) -> JobOptions {
    JobOptions::new(
        Budget::new(n(discovery)),
        Options::new(Budget::new(n(jobs))).window(n(window)),
    )
}

type ManifestReply = Result<Manifest<String, u32>, &'static str>;
type JobReply = Result<String, &'static str>;

fn manifest(urls: &[&str], next: Option<u32>) -> ManifestReply {
    Ok(Manifest::new(
        urls.iter().map(|url| (*url).to_owned()).collect(),
        next,
    ))
}

fn receipt(url: &str) -> JobReply {
    Ok(format!("shard:{url}"))
}

fn s(url: &str) -> String {
    url.to_owned()
}

fn states(unfinished: &[concord_pagination::Keyed<String, Unfinished>]) -> Vec<(&str, Unfinished)> {
    unfinished
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value))
        .collect()
}

#[test]
fn the_manifest_chain_feeds_parallel_jobs_within_window_plus_one() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 2),
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    let owned = || manifests.active() + downloads.active();

    assert_eq!(manifests.call_count(), 0, "construction performs no call");
    assert!(now(jobs.next()).is_none());
    assert_eq!(manifests.calls(), [None]);

    manifests.open(&None, manifest(&["a", "b", "c"], Some(1)));
    assert!(now(jobs.next()).is_none());
    // Two jobs in the window; the backlog (c) is under its bound, so the next manifest overlaps.
    assert_eq!(downloads.calls(), [s("a"), s("b")]);
    assert_eq!(manifests.calls(), [None, Some(1)]);
    assert_eq!(owned(), 3);
    assert_eq!(jobs.progress().queued, 1);

    downloads.open(&s("b"), receipt("b"));
    downloads.open(&s("a"), receipt("a"));
    let mut receipts = Vec::new();
    while let Some(Some(outcome)) = now(jobs.next()) {
        receipts.push(outcome.unwrap().value);
        assert!(owned() <= 3);
    }
    assert_eq!(receipts, ["shard:a", "shard:b"]);
    assert_eq!(downloads.calls(), [s("a"), s("b"), s("c")]);

    manifests.open(&Some(1), manifest(&["d"], None));
    assert!(now(jobs.next()).is_none());
    assert_eq!(downloads.calls(), [s("a"), s("b"), s("c"), s("d")]);
    downloads.open(&s("d"), receipt("d"));
    downloads.open(&s("c"), receipt("c"));
    while let Some(Some(outcome)) = now(jobs.next()) {
        receipts.push(outcome.unwrap().value);
    }
    assert_eq!(receipts, ["shard:a", "shard:b", "shard:c", "shard:d"]);
    assert!(now(jobs.next()).unwrap().is_none());

    assert!(downloads.peak() <= 2);
    assert_eq!(manifests.peak(), 1);
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.discovery.calls, 2);
    assert!(report.discovery.resume.is_none());
    assert!(report.unfinished.is_empty() && report.queued.is_empty());
}

#[test]
fn the_discovery_budget_counts_empty_manifests_and_ends_a_repeating_chain() {
    let mut calls = Vec::new();
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(5, 100, 2),
        |key: Option<u32>| {
            calls.push(key);
            // Empty, and always the same next manifest.
            ready(manifest(&[], Some(7)))
        },
        |url: String| ready(receipt(&url)),
    );
    assert!(now(jobs.next()).unwrap().is_none());
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryBounded);
    assert!(report.outcome.is_discovery_incomplete());
    assert_eq!(report.discovery.calls, 5);
    assert_eq!(report.discovery.resume, Some(Resume::At(7)));
    assert_eq!(calls, [None, Some(7), Some(7), Some(7), Some(7)]);
}

#[test]
fn empty_manifests_in_the_middle_of_the_chain_continue() {
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 2),
        |key: Option<u32>| {
            ready(match key {
                None => manifest(&[], Some(1)),
                Some(1) => manifest(&["a"], Some(2)),
                _ => manifest(&[], None),
            })
        },
        |url: String| ready(receipt(&url)),
    );
    let mut receipts = Vec::new();
    while let Some(outcome) = now(jobs.next()).unwrap() {
        receipts.push(outcome.unwrap().value);
    }
    assert_eq!(receipts, ["shard:a"]);
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.discovery.calls, 3);
}

#[test]
fn the_descriptor_backlog_bounds_discovery() {
    let discovery_calls = Cell::new(0u32);
    let downloads = Server::<String, JobReply>::new();
    // An endless chain of manifests with three descriptors each.
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(1_000, 1_000, 1).backlog(n(2)),
        |key: Option<u32>| {
            discovery_calls.set(discovery_calls.get() + 1);
            let m = key.unwrap_or(0);
            ready(Ok::<_, &str>(Manifest::new(
                (0..3).map(|i| format!("m{m}-{i}")).collect(),
                Some(m + 1),
            )))
        },
        |url| downloads.call(url),
    );
    for _ in 0..5 {
        assert!(now(jobs.next()).is_none());
    }
    // One manifest: one job admitted (window 1), two queued, which fills the backlog.
    assert_eq!(discovery_calls.get(), 1);
    assert_eq!(jobs.progress().queued, 2);
    assert_eq!(downloads.calls(), [s("m0-0")]);

    for step in 0..20 {
        let current = downloads.calls().last().cloned().unwrap();
        downloads.open(&current, receipt(&current));
        assert!(now(jobs.next()).unwrap().is_some());
        assert!(now(jobs.next()).is_none());
        // Documented bound: backlog - 1 + one manifest's descriptors.
        assert!(jobs.progress().queued <= 2 - 1 + 3, "step {step}");
        assert!(downloads.active() <= 1);
    }
    // 21 jobs admitted from manifests of 3: discovery ran only as the backlog drained.
    assert!(
        discovery_calls.get() <= 8,
        "{} manifest calls",
        discovery_calls.get()
    );
}

#[test]
fn a_job_keeps_its_slot_while_its_body_stalls_after_immediate_headers() {
    let headers = Cell::new(0usize);
    let bodies = Server::<String, JobReply>::new();
    let urls = ["r0", "r1", "r2", "r3", "r4"];
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 2),
        |_: Option<u32>| ready(manifest(&urls, None)),
        |url: String| {
            let body = bodies.call(url);
            let headers = &headers;
            async move {
                // The response head is available at once...
                ready(()).await;
                headers.set(headers.get() + 1);
                // ...but the body (parse + sink through EOF) stalls.
                body.await
            }
        },
    );
    for _ in 0..10 {
        assert!(now(jobs.next()).is_none());
    }
    assert_eq!(headers.get(), 2);
    assert_eq!(bodies.calls(), [s("r0"), s("r1")]);
    assert_eq!(jobs.progress().jobs.in_flight, 2);

    // Finishing one body releases one slot.
    bodies.open(&s("r0"), receipt("r0"));
    assert_eq!(now(jobs.next()).unwrap().unwrap().unwrap().key, "r0");
    assert!(now(jobs.next()).is_none());
    assert_eq!(headers.get(), 3);
    assert_eq!(bodies.call_count(), 3);
    assert_eq!(bodies.peak(), 2);
}

/// Contract §6: jobs 3–4 finished their sinks before a later failure stopped ordered output
/// behind job 2. Their receipts are handed back, not hidden behind a restart point.
#[test]
fn completed_unemitted_receipts_are_handed_back_after_a_later_failure() {
    let downloads = Server::<String, JobReply>::new();
    let urls = ["j1", "j2", "j3", "j4", "j5", "j6", "j7"];
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 4).backlog(n(8)),
        |_: Option<u32>| ready(manifest(&urls, None)),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    downloads.open(&s("j1"), receipt("j1"));
    assert_eq!(now(jobs.next()).unwrap().unwrap().unwrap().key, "j1");
    assert!(now(jobs.next()).is_none());
    assert_eq!(downloads.call_count(), 5);
    downloads.open(&s("j3"), receipt("j3"));
    downloads.open(&s("j4"), receipt("j4"));
    assert!(now(jobs.next()).is_none());
    // Two receipts wait behind j2 and still occupy their slots: j6 is not admitted.
    let progress = jobs.progress().jobs;
    assert_eq!((progress.in_flight, progress.retained), (2, 2));
    assert_eq!(downloads.call_count(), 5);
    downloads.open(&s("j5"), Err("parse error at record 12"));

    let Some(Some(Err(JobsError::Job(failed)))) = now(jobs.next()) else {
        panic!("the job failure is returned while j2 is stalled");
    };
    assert_eq!(
        (failed.key.as_str(), failed.value),
        ("j5", "parse error at record 12")
    );
    assert!(downloads.dropped(&s("j2")));
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(downloads.call_count(), 5);

    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.restart().map(String::as_str), Some("j2"));
    assert_eq!(
        report.failed().map(String::as_str).collect::<Vec<_>>(),
        ["j5"]
    );
    assert_eq!(
        states(&report.unfinished),
        [
            ("j2", Unfinished::Cancelled),
            ("j3", Unfinished::Retained),
            ("j4", Unfinished::Retained),
            ("j5", Unfinished::Failed),
        ]
    );
    assert_eq!(report.queued, [s("j6"), s("j7")]);
    assert!(report.discovery.resume.is_none());
    let (receipts, rerun) = report.into_rerun();
    assert_eq!(
        receipts
            .iter()
            .map(|receipt| (receipt.key.as_str(), receipt.value.as_str()))
            .collect::<Vec<_>>(),
        [("j3", "shard:j3"), ("j4", "shard:j4")]
    );
    assert_eq!(rerun, [s("j2"), s("j5"), s("j6"), s("j7")]);
}

/// Policy, stop mode: a discovery failure cancels the admitted jobs.
#[test]
fn a_discovery_failure_in_stop_mode_cancels_jobs_and_keeps_known_and_undiscovered_apart() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 2),
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    manifests.open(&None, manifest(&["a", "b", "c"], Some(1)));
    assert!(now(jobs.next()).is_none());
    manifests.open(&Some(1), Err("manifest 1 broke"));

    let Some(Some(Err(JobsError::Discovery(error)))) = now(jobs.next()) else {
        panic!("the discovery failure is returned");
    };
    assert_eq!(error, "manifest 1 broke");
    assert!(downloads.dropped(&s("a")) && downloads.dropped(&s("b")));
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(manifests.call_count(), 2);
    assert_eq!(downloads.call_count(), 2);

    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryFailed);
    // Attempted known work, known unattempted work, undiscovered work: three separate parts.
    assert_eq!(
        states(&report.unfinished),
        [("a", Unfinished::Cancelled), ("b", Unfinished::Cancelled)]
    );
    assert_eq!(report.queued, [s("c")]);
    assert_eq!(report.discovery.failed, Some(Resume::At(1)));
    assert_eq!(report.discovery.resume, Some(Resume::At(1)));
    assert_eq!(report.discovery.calls, 2);
}

/// Policy, continue mode: a discovery failure lets admitted and queued known jobs finish.
#[test]
fn a_discovery_failure_in_continue_mode_finishes_known_work() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(100)))
            .window(n(2))
            .on_error(OnError::Continue),
    );
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    manifests.open(&None, manifest(&["a", "b", "c"], Some(1)));
    assert!(now(jobs.next()).is_none());
    manifests.open(&Some(1), Err("manifest 1 broke"));
    assert!(matches!(
        now(jobs.next()),
        Some(Some(Err(JobsError::Discovery("manifest 1 broke"))))
    ));
    assert!(!downloads.dropped(&s("a")) && !downloads.dropped(&s("b")));

    let mut receipts = Vec::new();
    for url in ["a", "b", "c"] {
        while !downloads.calls().contains(&s(url)) {
            assert!(now(jobs.next()).is_none());
        }
        downloads.open(&s(url), receipt(url));
        receipts.push(now(jobs.next()).unwrap().unwrap().unwrap().key);
    }
    assert_eq!(receipts, ["a", "b", "c"]);
    assert!(now(jobs.next()).unwrap().is_none());
    assert_eq!(
        manifests.call_count(),
        2,
        "no manifest is guessed after the failure"
    );

    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::DiscoveryFailed);
    assert!(!report.is_success());
    assert!(report.unfinished.is_empty() && report.queued.is_empty());
    assert_eq!(report.progress.jobs.succeeded, 3);
    assert_eq!(report.discovery.failed, Some(Resume::At(1)));
}

/// Precedence (4.4 review SOL-6): stopping with `finish` while continue-mode jobs still run after
/// a discovery failure reports `Stopped`, and the discovery failure stays in `discovery.failed`.
#[test]
fn finish_after_a_discovery_failure_reports_stopped_and_keeps_the_failure() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(100)))
            .window(n(2))
            .on_error(OnError::Continue),
    );
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    manifests.open(&None, manifest(&["a"], Some(1)));
    assert!(now(jobs.next()).is_none());
    manifests.open(&Some(1), Err("manifest 1 broke"));
    assert!(matches!(
        now(jobs.next()),
        Some(Some(Err(JobsError::Discovery("manifest 1 broke"))))
    ));
    // Job `a` is still running (never answered) when the caller stops.
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.discovery.failed, Some(Resume::At(1)));
    assert_eq!(states(&report.unfinished), [("a", Unfinished::Cancelled)]);
    assert!(downloads.dropped(&s("a")));
}

#[test]
fn jobs_support_keyed_completion_order_and_continue_mode() {
    let downloads = Server::<String, JobReply>::new();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(100)))
            .window(n(4))
            .order(Order::Completion)
            .on_error(OnError::Continue),
    );
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |_: Option<u32>| ready(manifest(&["a", "b", "c", "d"], None)),
        |url| downloads.call(url),
    );
    assert!(now(jobs.next()).is_none());
    let mut seen = Vec::new();
    for (url, reply) in [
        ("d", receipt("d")),
        ("b", Err("b broke")),
        ("a", receipt("a")),
        ("c", receipt("c")),
    ] {
        downloads.open(&s(url), reply);
        match now(jobs.next()).unwrap().unwrap() {
            Ok(done) => seen.push((done.index, done.key, true)),
            Err(JobsError::Job(failed)) => seen.push((failed.index, failed.key, false)),
            Err(JobsError::Discovery(_)) => unreachable!(),
            Err(JobsError::RepeatedKey) => unreachable!(),
        }
    }
    assert_eq!(
        seen,
        [
            (3, s("d"), true),
            (1, s("b"), false),
            (0, s("a"), true),
            (2, s("c"), true)
        ]
    );
    assert!(now(jobs.next()).unwrap().is_none());
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::ExhaustedWithFailures);
    assert_eq!(report.restart().map(String::as_str), Some("b"));
}

#[test]
fn the_job_budget_bounds_jobs_and_stops_discovery() {
    let discovery_calls = Cell::new(0u32);
    let job_calls = Cell::new(0u32);
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(100, 3, 2),
        |key: Option<u32>| {
            discovery_calls.set(discovery_calls.get() + 1);
            let m = key.unwrap_or(0);
            ready(Ok::<_, &str>(Manifest::new(
                vec![format!("m{m}a"), format!("m{m}b")],
                Some(m + 1),
            )))
        },
        |url: String| {
            job_calls.set(job_calls.get() + 1);
            ready(receipt(&url))
        },
    );
    let mut done = 0;
    while let Some(outcome) = now(jobs.next()).unwrap() {
        outcome.unwrap();
        done += 1;
    }
    assert_eq!(done, 3);
    assert_eq!(job_calls.get(), 3);
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.discovery.calls, 2);
    assert_eq!(discovery_calls.get(), 2);
    assert_eq!(report.queued, [s("m1b")]);
    assert_eq!(report.discovery.resume, Some(Resume::At(2)));
}

#[test]
fn dropping_jobs_drops_the_manifest_call_and_every_job() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    {
        let mut jobs = manifest_jobs(
            Resume::Start,
            job_options(10, 100, 2),
            |key| manifests.call(key),
            |url| downloads.call(url),
        );
        assert!(now(jobs.next()).is_none());
        manifests.open(&None, manifest(&["a", "b", "c"], Some(1)));
        assert!(now(jobs.next()).is_none());
        assert_eq!(manifests.active() + downloads.active(), 3);
    }
    assert_eq!(manifests.active() + downloads.active(), 0);
    assert!(manifests.dropped(&Some(1)));
    assert_eq!((manifests.call_count(), downloads.call_count()), (2, 2));
}

#[test]
fn resuming_discovery_starts_at_the_reported_manifest() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let mut jobs = manifest_jobs(
        Resume::At(4),
        job_options(10, 100, 2),
        |key| manifests.call(key),
        |url: String| ready(receipt(&url)),
    );
    assert!(now(jobs.next()).is_none());
    assert_eq!(manifests.calls(), [Some(4)]);
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.discovery.resume, Some(Resume::At(4)));
    assert!(report.discovery.failed.is_none());
}

// --- job-owned output: a job stages its shard and publishes it only at EOF; a parse error or a
// --- cancellation removes the staged shard deterministically. The library only drops futures.

#[derive(Default)]
struct Store {
    staged: BTreeMap<String, Vec<u32>>,
    published: BTreeMap<String, Vec<u32>>,
}

/// A staged shard: removed on drop unless committed.
struct Staged {
    store: Rc<RefCell<Store>>,
    name: String,
    committed: bool,
}

impl Staged {
    fn new(store: &Rc<RefCell<Store>>, name: &str) -> Self {
        store
            .borrow_mut()
            .staged
            .insert(name.to_owned(), Vec::new());
        Self {
            store: Rc::clone(store),
            name: name.to_owned(),
            committed: false,
        }
    }

    fn write(&self, record: u32) {
        let mut store = self.store.borrow_mut();
        store.staged.get_mut(&self.name).unwrap().push(record);
    }

    fn commit(mut self) -> usize {
        let mut store = self.store.borrow_mut();
        let records = store.staged.remove(&self.name).unwrap();
        let count = records.len();
        store.published.insert(self.name.clone(), records);
        self.committed = true;
        count
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.committed {
            self.store.borrow_mut().staged.remove(&self.name);
        }
    }
}

/// A body arrives as a first chunk at once, then the rest when the test releases it.
fn shard_job(
    store: &Rc<RefCell<Store>>,
    rest: support::Response<Result<Vec<u32>, &'static str>>,
    url: String,
) -> impl Future<Output = Result<usize, &'static str>> + use<> {
    let store = Rc::clone(store);
    async move {
        let shard = Staged::new(&store, &url);
        for record in [1, 2] {
            shard.write(record);
        }
        for record in rest.await? {
            shard.write(record);
        }
        Ok(shard.commit())
    }
}

#[test]
fn a_job_owns_its_staged_shard_through_success_error_and_cancellation() {
    let store = Rc::new(RefCell::new(Store::default()));
    let bodies = Server::<String, Result<Vec<u32>, &'static str>>::new();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(100)))
            .window(n(3))
            .order(Order::Completion)
            .on_error(OnError::Continue),
    );
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |_: Option<u32>| ready(manifest(&["ok", "bad", "slow"], None)),
        |url: String| shard_job(&store, bodies.call(url.clone()), url),
    );
    assert!(now(jobs.next()).is_none());
    // Every job has written its first chunk to its own staged shard.
    assert_eq!(store.borrow().staged.len(), 3);

    bodies.open(&s("ok"), Ok(vec![3]));
    assert_eq!(now(jobs.next()).unwrap().unwrap().unwrap().value, 3);
    // A source/parse error halfway through: the partial shard is not published.
    bodies.open(&s("bad"), Err("truncated body"));
    assert!(matches!(
        now(jobs.next()),
        Some(Some(Err(JobsError::Job(_))))
    ));
    {
        let store = store.borrow();
        assert_eq!(store.published.keys().collect::<Vec<_>>(), ["ok"]);
        assert_eq!(store.staged.keys().collect::<Vec<_>>(), ["slow"]);
    }

    // Cancellation: finishing drops the slow job, whose guard removes its staged shard.
    let report = jobs.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(
        states(&report.unfinished),
        [("bad", Unfinished::Failed), ("slow", Unfinished::Cancelled)]
    );
    let store = store.borrow();
    assert!(store.staged.is_empty());
    assert_eq!(store.published["ok"], [1, 2, 3]);
}

#[tokio::test(flavor = "current_thread")]
async fn real_wakeups_drive_discovery_and_jobs() {
    let manifests = Server::<Option<u32>, ManifestReply>::new();
    let downloads = Server::<String, JobReply>::new();
    let mut jobs = manifest_jobs(
        Resume::Start,
        job_options(10, 100, 2),
        |key| manifests.call(key),
        |url| downloads.call(url),
    );
    let consumer = async {
        let mut keys = Vec::new();
        while let Some(outcome) = jobs.next().await {
            keys.push(outcome.unwrap().key);
        }
        keys
    };
    let driver = async {
        let mut opened_manifests = Vec::new();
        let mut opened_jobs = Vec::new();
        while opened_jobs.len() < 4 {
            tokio::task::yield_now().await;
            for key in manifests.calls() {
                if !opened_manifests.contains(&key) {
                    opened_manifests.push(key);
                    let reply = match key {
                        None => manifest(&["a", "b", "c"], Some(1)),
                        _ => manifest(&["d"], None),
                    };
                    manifests.open(&key, reply);
                }
            }
            // Release the most recent job first.
            if let Some(url) = downloads
                .calls()
                .into_iter()
                .rev()
                .find(|url| !opened_jobs.contains(url))
            {
                downloads.open(&url, receipt(&url));
                opened_jobs.push(url);
            }
        }
    };
    let (keys, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(consumer, driver)
    })
    .await
    .expect("a lost wakeup would hang here");
    assert_eq!(keys, ["a", "b", "c", "d"]);
}
