//! Consumer application for the maintenance additions (tag-filtered droplets, droplet
//! snapshots, resumed traversals), written against the README and rustdoc only. Everything
//! runs against `concord-example-testserver` on loopback; no real DigitalOcean call is made.

use concord_sdk_digitalocean::concord_pagination::{Incomplete, PagesError, Resume};
use concord_sdk_digitalocean::{
    DigitalOcean, Droplet, Error, HttpClass, NextLink, PageSize, Snapshot, TagName,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};

const TOKEN: &str = "dop_v1_maintenance_synthetic_token_51c0";
const BEARER: &str = "Bearer dop_v1_maintenance_synthetic_token_51c0";

// ---------------------------------------------------------------------------------------------
// The application: a nightly backup audit of the droplets tagged for production.
// ---------------------------------------------------------------------------------------------

/// The newest image of each audited droplet.
#[derive(Debug, PartialEq)]
struct BackupLine {
    droplet: String,
    newest_backup: Option<String>,
    snapshots: usize,
}

#[derive(Debug)]
enum AuditError {
    Droplets(Box<Incomplete<Droplet, Error, NextLink>>),
    Snapshots(u64, Box<Incomplete<Snapshot, Error, NextLink>>),
}

/// A run reads at most `budget` pages of the tagged listing, then carries on from where the
/// previous run stopped (the position the traversal reported), keeping what it already read.
struct TaggedListing {
    tag: TagName,
    from: Resume<NextLink>,
    droplets: Vec<Droplet>,
    done: bool,
}

impl TaggedListing {
    fn new(tag: &str) -> Result<Self, Error> {
        Ok(Self {
            tag: TagName::new(tag)?,
            from: Resume::Start,
            droplets: Vec::new(),
            done: false,
        })
    }

    /// One budgeted run. `Ok` after the listing's last page or after spending the budget.
    async fn run(&mut self, api: &DigitalOcean, budget: usize) -> Result<(), AuditError> {
        let size = PageSize::new(2).expect("valid size");
        let budget = NonZeroUsize::new(budget).expect("nonzero budget");
        let from = std::mem::replace(&mut self.from, Resume::Start);
        let pages = api
            .tagged_droplet_pages(&self.tag, size, budget)
            .starting_at(from);
        match pages.items().collect_complete().await {
            Ok(rest) => {
                self.droplets.extend(rest);
                self.done = true;
                Ok(())
            }
            Err(mut stop) => {
                self.droplets.append(&mut stop.collected);
                self.from = stop.error.resume().cloned().unwrap_or(Resume::Start);
                match stop.error {
                    PagesError::Bounded { .. } => Ok(()),
                    _ => Err(AuditError::Droplets(Box::new(stop))),
                }
            }
        }
    }
}

async fn audit(api: &DigitalOcean, droplets: &[Droplet]) -> Result<Vec<BackupLine>, AuditError> {
    let size = PageSize::new(2).expect("valid size");
    let cap = NonZeroUsize::new(5).expect("nonzero");
    let mut lines = Vec::new();
    for droplet in droplets {
        let snapshots = api
            .snapshot_pages(droplet.id, size, cap)
            .items()
            .collect_complete()
            .await
            .map_err(|stop| AuditError::Snapshots(droplet.id, Box::new(stop)))?;
        let newest_backup = snapshots
            .iter()
            .filter(|s| s.kind == "backup")
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|s| s.name.clone());
        lines.push(BackupLine {
            droplet: droplet.name.clone(),
            newest_backup,
            snapshots: snapshots.len(),
        });
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------------------------
// Fixtures: DigitalOcean-shaped documents with synthetic values.
// ---------------------------------------------------------------------------------------------

fn droplet(id: u64, name: &str) -> Value {
    json!({
        "id": id, "name": name, "memory": 2048, "vcpus": 2, "disk": 60, "locked": false,
        "status": "active", "created_at": "2026-10-30T14:12:31Z", "size_slug": "s-2vcpu-2gb",
        "region": {"name": "Frankfurt 1", "slug": "fra1", "available": true},
        "networks": {"v4": [{"ip_address": "10.114.0.7", "netmask": "255.255.240.0",
                             "type": "private"}], "v6": []},
        "tags": ["env:prod", "web"], "snapshot_ids": [], "backup_ids": []
    })
}

fn image(id: u64, name: &str, kind: &str, created_at: &str) -> Value {
    json!({
        "id": id, "name": name, "type": kind, "created_at": created_at,
        "regions": ["fra1"], "min_disk_size": 60, "size_gigabytes": 4.72
    })
}

fn ok(body: Value) -> Reply {
    Reply::json(200, body.to_string())
        .header("ratelimit-limit", "5000")
        .header("ratelimit-remaining", "4321")
        .header("ratelimit-reset", "1762507800")
}

fn origin(req: &Recorded) -> String {
    format!("http://{}", req.host)
}

/// Three production droplets on two pages; droplet 11's images span two pages.
fn provider(req: &Recorded) -> Reply {
    let base = origin(req);
    match req.target.as_str() {
        "/v2/droplets?per_page=2&tag_name=env%3Aprod" => ok(json!({
            "droplets": [droplet(11, "web-01"), droplet(12, "web-02")],
            "links": {"pages": {
                "next": format!("{base}/v2/droplets?page=2&per_page=2&tag_name=env%3Aprod"),
                "last": format!("{base}/v2/droplets?page=2&per_page=2&tag_name=env%3Aprod")
            }},
            "meta": {"total": 3}
        })),
        "/v2/droplets?page=2&per_page=2&tag_name=env%3Aprod" => ok(json!({
            "droplets": [droplet(13, "web-03")],
            "links": {"pages": {
                "first": format!("{base}/v2/droplets?page=1&per_page=2&tag_name=env%3Aprod"),
                "prev": format!("{base}/v2/droplets?page=1&per_page=2&tag_name=env%3Aprod")
            }},
            "meta": {"total": 3}
        })),
        "/v2/droplets/11/snapshots?per_page=2" => ok(json!({
            "snapshots": [
                image(901, "web-01-before-upgrade", "snapshot", "2026-10-01T08:00:00Z"),
                image(902, "web-01-2026-11-05", "backup", "2026-11-05T03:00:00Z"),
            ],
            "links": {"pages": {
                "next": format!("{base}/v2/droplets/11/snapshots?page=2&per_page=2")
            }},
            "meta": {"total": 3}
        })),
        "/v2/droplets/11/snapshots?page=2&per_page=2" => ok(json!({
            "snapshots": [image(903, "web-01-2026-11-06", "backup", "2026-11-06T03:00:00Z")],
            "links": {},
            "meta": {"total": 3}
        })),
        "/v2/droplets/12/snapshots?per_page=2" => ok(json!({
            "snapshots": [image(904, "web-02-2026-11-06", "backup", "2026-11-06T03:10:00Z")],
            "links": {},
            "meta": {"total": 1}
        })),
        "/v2/droplets/13/snapshots?per_page=2" => ok(json!({
            "snapshots": [], "links": {}, "meta": {"total": 0}
        })),
        _ => Reply::json(
            404,
            json!({"id": "not_found", "message": "The resource you requested could not be found."})
                .to_string(),
        ),
    }
}

fn targets(server: &TestServer) -> Vec<String> {
    server.requests().into_iter().map(|r| r.target).collect()
}

fn assert_ordinary_get(server: &TestServer) {
    for req in server.requests() {
        assert_eq!(req.method, "GET");
        assert_eq!(req.header("authorization"), Some(BEARER));
        assert_eq!(req.header("accept"), Some("application/json"));
        assert_eq!(req.body_len, 0);
    }
}

fn expected_report() -> Vec<BackupLine> {
    vec![
        BackupLine {
            droplet: "web-01".into(),
            newest_backup: Some("web-01-2026-11-06".into()),
            snapshots: 3,
        },
        BackupLine {
            droplet: "web-02".into(),
            newest_backup: Some("web-02-2026-11-06".into()),
            snapshots: 1,
        },
        BackupLine {
            droplet: "web-03".into(),
            newest_backup: None,
            snapshots: 0,
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// The audit, in one run and in budgeted runs.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn audit_lists_tagged_droplets_then_their_snapshots() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut listing = TaggedListing::new("env:prod").unwrap();
    listing.run(&api, 10).await.unwrap();
    assert!(listing.done);
    let report = audit(&api, &listing.droplets).await.unwrap();
    assert_eq!(report, expected_report());
    assert_eq!(
        targets(&server),
        [
            "/v2/droplets?per_page=2&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=2&tag_name=env%3Aprod",
            "/v2/droplets/11/snapshots?per_page=2",
            "/v2/droplets/11/snapshots?page=2&per_page=2",
            "/v2/droplets/12/snapshots?per_page=2",
            "/v2/droplets/13/snapshots?per_page=2",
        ]
    );
    assert_ordinary_get(&server);
}

#[tokio::test]
async fn budgeted_runs_resume_where_the_previous_run_stopped() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut listing = TaggedListing::new("env:prod").unwrap();
    listing.run(&api, 1).await.unwrap();
    assert!(!listing.done, "the budget ended the run, not the listing");
    assert_eq!(listing.droplets.len(), 2);
    listing.run(&api, 1).await.unwrap();
    assert!(listing.done);
    assert_eq!(listing.droplets.len(), 3);
    // The resumed run asked for page two with the same tag; nothing was read twice.
    assert_eq!(
        targets(&server),
        [
            "/v2/droplets?per_page=2&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=2&tag_name=env%3Aprod",
        ]
    );
    assert_eq!(
        audit(&api, &listing.droplets).await.unwrap(),
        expected_report()
    );
}

#[tokio::test]
async fn rate_limited_page_is_retried_by_the_next_run_with_the_tag() {
    let limited = AtomicBool::new(true);
    let server = TestServer::start(move |req: &Recorded| {
        if req.query_all("page") == ["2"] && limited.swap(false, Ordering::SeqCst) {
            Reply::json(
                429,
                json!({"id": "too_many_requests", "message": "API Rate limit exceeded."})
                    .to_string(),
            )
            .header("retry-after", "30")
        } else {
            provider(req)
        }
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut listing = TaggedListing::new("env:prod").unwrap();
    let AuditError::Droplets(stop) = listing.run(&api, 10).await.unwrap_err() else {
        panic!("expected the droplet listing to stop");
    };
    let Some(Error::Provider(failure)) = stop.error.provider_error() else {
        panic!("expected an HTTP failure, got {:?}", stop.error);
    };
    assert_eq!(failure.class(), HttpClass::RateLimited);
    assert_eq!(failure.meta.retry_after_seconds(), Ok(Some(30)));
    assert_eq!(listing.droplets.len(), 2, "the first page is kept");
    assert!(matches!(listing.from, Resume::At(_)), "the failed page");

    listing.run(&api, 10).await.unwrap();
    assert!(listing.done);
    assert_eq!(listing.droplets.len(), 3);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].target, requests[2].target,
        "the same page, retried"
    );
    assert_eq!(requests[2].query_all("tag_name"), ["env:prod"]);
}

// ---------------------------------------------------------------------------------------------
// Failures the additions must exercise.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_invalid_tag_never_reaches_the_provider() {
    let server = TestServer::start(provider).await;
    for tag in ["", "env: prod", "env:prod\n", "\t"] {
        assert!(
            matches!(TaggedListing::new(tag), Err(Error::TagName)),
            "{tag:?}"
        );
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_next_link_that_loses_the_tag_is_refused_not_widened() {
    // An untagged next page would hand the audit every droplet in the account.
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets?per_page=2&tag_name=env%3Aprod" => ok(json!({
            "droplets": [droplet(11, "web-01"), droplet(12, "web-02")],
            "links": {"pages": {"next": format!("{}/v2/droplets?page=2&per_page=2", origin(req))}}
        })),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut listing = TaggedListing::new("env:prod").unwrap();
    let AuditError::Droplets(stop) = listing.run(&api, 10).await.unwrap_err() else {
        panic!("expected the droplet listing to stop");
    };
    assert!(matches!(
        stop.error.provider_error(),
        Some(Error::UnsafeNext)
    ));
    // As for the other listings, the page carrying the unsafe link fails as a whole.
    assert!(listing.droplets.is_empty());
    assert_eq!(
        targets(&server),
        ["/v2/droplets?per_page=2&tag_name=env%3Aprod"]
    );
}

#[tokio::test]
async fn an_untrusted_snapshot_link_never_receives_the_token() {
    let untrusted =
        TestServer::start(|_: &Recorded| Reply::json(200, json!({"snapshots": []}).to_string()))
            .await;
    let foreign = format!(
        "{}/v2/droplets/11/snapshots?page=2&per_page=2",
        untrusted.url()
    );
    let server = TestServer::start(move |req: &Recorded| match req.target.as_str() {
        "/v2/droplets/11/snapshots?per_page=2" => ok(json!({
            "snapshots": [image(901, "web-01-before-upgrade", "snapshot", "2026-10-01T08:00:00Z")],
            "links": {"pages": {"next": foreign.clone()}}
        })),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let droplets = vec![serde_json::from_value(droplet(11, "web-01")).unwrap()];
    let AuditError::Snapshots(id, stop) = audit(&api, &droplets).await.unwrap_err() else {
        panic!("expected the snapshot listing to stop");
    };
    assert_eq!(id, 11);
    assert!(matches!(
        stop.error.provider_error(),
        Some(Error::UnsafeNext)
    ));
    let rendered = format!("{stop:?} {stop}");
    assert!(!rendered.contains("dop_v1_") && !rendered.contains(&untrusted.url()));
    assert!(
        untrusted.requests().is_empty(),
        "the other host saw nothing"
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_saved_position_is_only_good_for_its_own_listing() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut listing = TaggedListing::new("env:prod").unwrap();
    listing.run(&api, 1).await.unwrap();
    let Resume::At(position) = listing.from.clone() else {
        panic!("a bounded run keeps a position");
    };
    // The same position offered to another tag or to the unfiltered listing is refused.
    let staging = TagName::new("env:staging").unwrap();
    let size = PageSize::new(2).unwrap();
    let cap = NonZeroUsize::new(5).unwrap();
    for mut other in [
        api.tagged_droplet_pages(&staging, size, cap)
            .starting_at(Resume::At(position.clone())),
        api.droplet_pages(size, cap)
            .starting_at(Resume::At(position)),
    ] {
        let error = other.next_page().await.unwrap_err();
        assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
    }
    assert_eq!(server.requests().len(), 1, "refused positions send nothing");
}
