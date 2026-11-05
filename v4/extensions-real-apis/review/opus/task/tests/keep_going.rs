//! The keep-going audit on loopback, over the GH-A synthetic fixtures of
//! `examples/github-app/tests/fixtures/synthetic` (read, not modified).
//!
//! Listing (filters `state=all&labels=bug&since=..&per_page=2`): page 1 = issue #11 and pull
//! request #12, page 2 = empty with `next`, page 3 = issue #13. #11 has 2 + 1 comments on two
//! pages, #12 details and 1 + 1 reviews, #13 1 + 1 comments.

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord::reqwest;
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::params::{Labels, PerPage, StateFilter, Timestamp};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};
use concord_example_testserver::{Recorded, Reply, TestServer};
use opus_review_task::*;

const API_HOST: &str = "api.github.test";
const TOKEN: &str = "ghp_SYNTHETIC_TOKEN_OPUS_000000000000000";
const ISSUES_QUERY: &str = "state=all&labels=bug&since=2026-10-01T00%3A00%3A00Z&per_page=2";

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../examples/github-app/tests/fixtures/synthetic/",
            $name
        ))
    };
}

#[derive(Default)]
struct Faults {
    issues_page_2: AtomicUsize,
    comments_13_page_2: AtomicUsize,
    comments_11_page_2: AtomicUsize,
    pull_12: AtomicUsize,
    reviews_12_page_2: AtomicUsize,
    /// The listing changed: page 1 now starts with #13.
    changed_listing: AtomicBool,
    /// Delay every reply (to observe overlapping requests).
    delay_ms: AtomicUsize,
    served: AtomicUsize,
    /// (arrival, path) of every request.
    arrivals: Mutex<Vec<(Instant, String)>>,
}

fn fail(counter: &AtomicUsize) -> bool {
    counter
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}

fn json(body: &str, n: usize) -> Reply {
    Reply::json(200, body).header("x-github-request-id", format!("REQ:{n:04}"))
}

fn error500() -> Reply {
    Reply::json(500, fixture!("error-500.json")).header("x-github-request-id", "REQ:FAIL")
}

fn stand_in(faults: Arc<Faults>) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |r: &Recorded| {
        let n = faults.served.fetch_add(1, Ordering::SeqCst);
        faults
            .arrivals
            .lock()
            .unwrap()
            .push((Instant::now(), r.path().to_owned()));
        let o = format!("http://{}", r.host);
        let issues = format!("/repositories/101/issues?{ISSUES_QUERY}");
        let reply = match (r.path(), r.query().unwrap_or("")) {
            ("/repos/octo-org/hello-world/issues", q) if q == ISSUES_QUERY => {
                if faults.changed_listing.load(Ordering::SeqCst) {
                    json(fixture!("issues-a-3.json"), n)
                } else {
                    json(fixture!("issues-a-1.json"), n)
                        .header("link", format!("<{o}{issues}&page=2>; rel=\"next\""))
                }
            }
            ("/repositories/101/issues", q) if q == format!("{ISSUES_QUERY}&page=2") => {
                if fail(&faults.issues_page_2) {
                    error500()
                } else {
                    json(fixture!("issues-a-2.json"), n)
                        .header("link", format!("<{o}{issues}&page=3>; rel=\"next\""))
                }
            }
            ("/repositories/101/issues", q) if q == format!("{ISSUES_QUERY}&page=3") => {
                json(fixture!("issues-a-3.json"), n)
            }
            ("/repos/octo-org/hello-world/issues/11/comments", "") => {
                json(fixture!("comments-a-11-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/issues/11/comments?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/issues/11/comments", "page=2") => {
                if fail(&faults.comments_11_page_2) {
                    error500()
                } else {
                    json(fixture!("comments-a-11-2.json"), n)
                }
            }
            ("/repos/octo-org/hello-world/pulls/12", "") => {
                if fail(&faults.pull_12) {
                    error500()
                } else {
                    json(fixture!("pull-a-12.json"), n)
                }
            }
            ("/repos/octo-org/hello-world/pulls/12/reviews", "") => {
                json(fixture!("reviews-a-12-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/pulls/12/reviews?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/pulls/12/reviews", "page=2") => {
                if fail(&faults.reviews_12_page_2) {
                    error500()
                } else {
                    json(fixture!("reviews-a-12-2.json"), n)
                }
            }
            ("/repos/octo-org/hello-world/issues/13/comments", "") => {
                json(fixture!("comments-a-13-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/issues/13/comments?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/issues/13/comments", "page=2") => {
                if fail(&faults.comments_13_page_2) {
                    error500()
                } else {
                    json(fixture!("comments-a-13-2.json"), n)
                }
            }
            _ => Reply::json(404, r#"{"message":"Not Found","status":"404"}"#),
        };
        match faults.delay_ms.load(Ordering::SeqCst) {
            0 => reply,
            ms => reply.delay(Duration::from_millis(ms as u64)),
        }
    }
}

fn github(api: SocketAddr) -> GitHubContext {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(redirect_policy())
        .resolve(API_HOST, api)
        .build()
        .unwrap();
    GitHubContext::new(
        http,
        GitHubConfig {
            token: TOKEN.to_owned(),
        },
    )
    .with_base_url(format!("http://{API_HOST}:{}", api.port()))
    .unwrap()
}

fn filter() -> IssueFilter {
    IssueFilter {
        state: Some(StateFilter::All),
        labels: Some(Labels::new(["bug"]).unwrap()),
        since: Some(Timestamp::parse("2026-10-01T00:00:00Z").unwrap()),
        per_page: Some(PerPage::new(2).unwrap()),
        ..IssueFilter::default()
    }
}

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn input(from: Option<StoredPosition>) -> KeepGoingInput {
    KeepGoingInput {
        filter: filter(),
        issue_pages: nz(10),
        child_pages: nz(10),
        from,
        max_child_failures: None,
    }
}

/// A durable store: records, the last position, and failures, written together per item.
#[derive(Default)]
struct Store {
    records: Vec<ItemRecord>,
    position: Option<Option<StoredPosition>>,
    failures: Vec<StoredFailure>,
    stop_after: Option<usize>,
}

impl AuditSink for Store {
    fn processed(
        &mut self,
        record: ItemRecord,
        after: Option<StoredPosition>,
        failure: Option<&StoredFailure>,
    ) -> ControlFlow<()> {
        self.records.push(record);
        self.position = Some(after);
        if let Some(f) = failure {
            self.failures.push(f.clone());
        }
        match self.stop_after {
            Some(n) if self.records.len() >= n => ControlFlow::Break(()),
            _ => ControlFlow::Continue(()),
        }
    }
}

fn paths(server: &TestServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .map(|r| match r.query() {
            Some(q) if q.starts_with("page=") => format!("{}?{q}", r.path()),
            Some(q) => match q.rsplit_once("&page=") {
                Some((_, p)) => format!("{}?page={p}", r.path()),
                None => r.path().to_owned(),
            },
            None => r.path().to_owned(),
        })
        .collect()
}

#[tokio::test]
async fn child_failure_does_not_stop_and_repair_fetches_only_the_failed_page() {
    let faults = Arc::new(Faults::default());
    faults.comments_13_page_2.store(1, Ordering::SeqCst);
    faults.pull_12.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let mut store = Store::default();
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    assert!(
        matches!(report.end, RunEnd::CompleteWithChildFailures),
        "{:?}",
        report.end
    );
    assert_eq!(report.processed, 3);
    let numbers: Vec<u64> = store.records.iter().map(ItemRecord::number).collect();
    assert_eq!(numbers, [11, 12, 13]);
    assert_eq!(
        store.records[0],
        ItemRecord::Issue {
            number: 11,
            comments: 3,
            state: ChildState::Complete
        }
    );
    assert_eq!(
        store.records[1],
        ItemRecord::PullRequest {
            number: 12,
            merged: None,
            reviews: 0,
            state: ChildState::Failed
        }
    );
    assert_eq!(
        store.records[2],
        ItemRecord::Issue {
            number: 13,
            comments: 1,
            state: ChildState::Failed
        }
    );
    // Failed keys, persistable, with their restart points.
    assert_eq!(store.failures.len(), 2);
    assert_eq!(store.failures[0].number, 12);
    assert_eq!(store.failures[0].kind, ChildKind::PullDetails);
    assert_eq!(store.failures[0].restart_url, None);
    assert_eq!(store.failures[1].number, 13);
    assert_eq!(store.failures[1].kind, ChildKind::Comments);
    assert!(
        store.failures[1]
            .restart_url
            .as_deref()
            .unwrap()
            .ends_with("/repositories/101/issues/13/comments?page=2")
    );
    assert_eq!(report.failures.len(), 2);
    assert_eq!(report.failures[1].error.status().map(|s| s.as_u16()), Some(500));
    // The final position: everything processed.
    assert_eq!(store.position, Some(None));

    // Repair pass in a "new process": only the persisted failures.
    let before = server.requests().len();
    let repaired = repair(&repo, store.failures.clone(), nz(10)).await;
    assert!(repaired.still_failed.is_empty(), "{:?}", repaired.still_failed);
    assert_eq!(repaired.outcome, concord_pagination::Outcome::Exhausted);
    assert_eq!(
        repaired.repaired,
        [
            ItemRecord::PullRequest {
                number: 12,
                merged: Some(false),
                reviews: 2,
                state: ChildState::Complete
            },
            ItemRecord::Issue {
                number: 13,
                comments: 1,
                state: ChildState::Complete
            },
        ]
    );
    let repair_paths = paths(&server)[before..].to_vec();
    assert_eq!(
        repair_paths,
        [
            "/repos/octo-org/hello-world/pulls/12",
            "/repos/octo-org/hello-world/pulls/12/reviews",
            "/repositories/101/pulls/12/reviews?page=2",
            "/repositories/101/issues/13/comments?page=2",
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn repair_that_fails_again_reports_the_new_restart_point() {
    let faults = Arc::new(Faults::default());
    faults.pull_12.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let mut store = Store::default();
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    assert!(matches!(report.end, RunEnd::CompleteWithChildFailures));
    assert_eq!(store.failures.len(), 1);

    // During repair the details succeed but the reviews' page 2 fails.
    faults.reviews_12_page_2.store(1, Ordering::SeqCst);
    let repaired = repair(&repo, store.failures.clone(), nz(10)).await;
    assert_eq!(
        repaired.outcome,
        concord_pagination::Outcome::ExhaustedWithFailures
    );
    assert_eq!(repaired.still_failed.len(), 1);
    let again = &repaired.still_failed[0].stored;
    assert_eq!(again.kind, ChildKind::Reviews);
    assert!(
        again
            .restart_url
            .as_deref()
            .unwrap()
            .ends_with("/repositories/101/pulls/12/reviews?page=2")
    );
    // Second repair from the new restart point fetches only the failed review page.
    let before = server.requests().len();
    let repaired = repair(&repo, vec![again.clone()], nz(10)).await;
    assert!(repaired.still_failed.is_empty());
    assert_eq!(
        paths(&server)[before..].to_vec(),
        ["/repositories/101/pulls/12/reviews?page=2"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn stop_after_one_item_then_resume_from_the_persisted_position() {
    let faults = Arc::new(Faults::default());
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let mut store = Store {
        stop_after: Some(1),
        ..Store::default()
    };
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    let RunEnd::Stopped { position } = &report.end else {
        panic!("{:?}", report.end)
    };
    let position = position.clone().expect("more items remain");
    assert_eq!(
        position,
        StoredPosition {
            page_url: None,
            skip: 1,
            last_number: Some(11)
        }
    );
    assert_eq!(store.position, Some(Some(position.clone())));

    // A new run (new context too) from the stored strings.
    let github = self::github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let mut resumed = Store::default();
    let before = server.requests().len();
    let report = keep_going_audit(&repo, input(Some(position)), &mut resumed).await;
    assert!(matches!(report.end, RunEnd::Complete), "{:?}", report.end);
    let numbers: Vec<u64> = resumed.records.iter().map(ItemRecord::number).collect();
    assert_eq!(numbers, [12, 13], "no item processed twice");
    // Page 1 replayed once; #11's comments not fetched again.
    let resumed_paths = paths(&server)[before..].to_vec();
    assert_eq!(resumed_paths[0], "/repos/octo-org/hello-world/issues");
    assert!(!resumed_paths.iter().any(|p| p.contains("/issues/11/")));

    // Stopping after the last item of a page stores the next page (a page boundary).
    let mut store = Store {
        stop_after: Some(2),
        ..Store::default()
    };
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    let RunEnd::Stopped { position } = &report.end else {
        panic!("{:?}", report.end)
    };
    let position = position.clone().unwrap();
    assert_eq!(position.skip, 0);
    assert!(position.page_url.as_deref().unwrap().ends_with("&page=2"));
    server.shutdown().await;
}

#[tokio::test]
async fn changed_listing_is_refused_on_resume() {
    let faults = Arc::new(Faults::default());
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let stored = StoredPosition {
        page_url: None,
        skip: 1,
        last_number: Some(11),
    };
    faults.changed_listing.store(true, Ordering::SeqCst);
    let mut store = Store::default();
    let report = keep_going_audit(&repo, input(Some(stored.clone())), &mut store).await;
    match &report.end {
        RunEnd::Failed {
            error: AuditError::Stale { expected, found },
            position,
        } => {
            assert_eq!((*expected, *found), (11, Some(13)));
            assert_eq!(position.as_ref(), Some(&stored), "position unchanged");
        }
        other => panic!("{other:?}"),
    }
    assert!(store.records.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn parent_failure_stops_with_a_page_position_and_resumes() {
    let faults = Arc::new(Faults::default());
    faults.issues_page_2.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let mut store = Store::default();
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    let RunEnd::Failed { error, position } = &report.end else {
        panic!("{:?}", report.end)
    };
    assert!(matches!(error, AuditError::GitHub(_)));
    let position = position.clone().unwrap();
    assert_eq!(position.skip, 0);
    assert!(position.page_url.as_deref().unwrap().ends_with("&page=2"));
    // The durable store already holds the same position (written after #12).
    assert_eq!(store.position, Some(Some(StoredPosition { last_number: Some(12), ..position.clone() })));

    let mut resumed = Store::default();
    let report = keep_going_audit(&repo, input(Some(position)), &mut resumed).await;
    assert!(matches!(report.end, RunEnd::Complete));
    assert_eq!(resumed.records.iter().map(ItemRecord::number).collect::<Vec<_>>(), [13]);
    server.shutdown().await;
}

#[tokio::test]
async fn child_failure_limit_stops_after_the_item_that_reached_it() {
    let faults = Arc::new(Faults::default());
    faults.comments_11_page_2.store(1, Ordering::SeqCst);
    faults.pull_12.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let mut store = Store::default();
    let report = keep_going_audit(
        &repo,
        KeepGoingInput {
            max_child_failures: Some(nz(2)),
            ..input(None)
        },
        &mut store,
    )
    .await;
    let RunEnd::Stopped { position } = &report.end else {
        panic!("{:?}", report.end)
    };
    assert_eq!(report.failures.len(), 2);
    let position = position.clone().unwrap();
    assert!(position.page_url.as_deref().unwrap().ends_with("&page=2"));
    assert_eq!(store.records.len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn persisted_child_link_restored_into_another_parent_is_not_refused() {
    // Not a requirement of the task: probes the continuation binding of a *persisted* link.
    let faults = Arc::new(Faults::default());
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    // A failure of #13's comments page 2, persisted, but replayed as #11's.
    let wrong = StoredFailure {
        number: 11,
        kind: ChildKind::Comments,
        restart_url: Some(format!(
            "http://{API_HOST}:{}/repositories/101/issues/13/comments?page=2",
            server.addr().port()
        )),
    };
    let before = server.requests().len();
    let repaired = repair(&repo, vec![wrong], nz(10)).await;
    // Observed: accepted and sent; #13's comment is attributed to #11.
    assert!(repaired.still_failed.is_empty());
    assert_eq!(
        repaired.repaired,
        [ItemRecord::Issue {
            number: 11,
            comments: 1,
            state: ChildState::Complete
        }]
    );
    assert_eq!(
        paths(&server)[before..].to_vec(),
        ["/repositories/101/issues/13/comments?page=2"]
    );
    server.shutdown().await;
}

/// The alternative written with `manifest_jobs`: it works functionally, but the manifest call
/// runs beside the job window, so a "serial" GitHub caller sends overlapping requests even with a
/// window of one.
#[tokio::test]
async fn manifest_jobs_variant_overlaps_discovery_with_jobs() {
    let faults = Arc::new(Faults::default());
    faults.comments_13_page_2.store(1, Ordering::SeqCst);
    faults.delay_ms.store(40, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let run = via_jobs::run(&repo, &filter(), nz(10)).await;
    assert_eq!(
        run.records.iter().map(ItemRecord::number).collect::<Vec<_>>(),
        [11, 12, 13]
    );
    assert_eq!(run.failed, [(13, false)]);
    assert_eq!(run.outcome, concord_pagination::Outcome::ExhaustedWithFailures);
    // The checkpoint after the first record: no single persistable position; queued descriptors
    // plus the next manifest.
    let cp = run.first_checkpoint.unwrap();
    println!(
        "first checkpoint: admitted={} unfinished={} queued={:?} discovery={:?}",
        cp.jobs.admitted,
        cp.jobs.unfinished.len(),
        cp.queued,
        cp.discovery
    );

    // Overlap: a request that arrived less than the reply delay after the previous one.
    let arrivals = faults.arrivals.lock().unwrap().clone();
    let mut overlaps = Vec::new();
    for pair in arrivals.windows(2) {
        let gap = pair[1].0.duration_since(pair[0].0);
        if gap < Duration::from_millis(30) {
            overlaps.push(format!("{} || {} ({gap:?})", pair[0].1, pair[1].1));
        }
    }
    println!("requests: {}", arrivals.len());
    for o in &overlaps {
        println!("overlap: {o}");
    }
    assert!(!overlaps.is_empty(), "expected discovery to overlap a job");
    server.shutdown().await;
}

/// Baseline for the overlap measurement: the hand-written traversal is strictly serial.
#[tokio::test]
async fn hand_written_traversal_is_serial() {
    let faults = Arc::new(Faults::default());
    faults.delay_ms.store(40, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let mut store = Store::default();
    let report = keep_going_audit(&repo, input(None), &mut store).await;
    assert!(matches!(report.end, RunEnd::Complete));
    let arrivals = faults.arrivals.lock().unwrap().clone();
    for pair in arrivals.windows(2) {
        assert!(pair[1].0.duration_since(pair[0].0) >= Duration::from_millis(35));
    }
    server.shutdown().await;
}
