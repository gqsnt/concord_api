//! Fresh-consumer application for `concord-sdk-digitalocean`, written from the README and
//! rustdoc only. Two caller tasks (one per workflow) plus the failures the slice must exercise,
//! all against `concord-example-testserver` on loopback. No real DigitalOcean call is made.

use concord_sdk_digitalocean::concord::{ErrorKind, FieldError};
use concord_sdk_digitalocean::concord_pagination::{Incomplete, PagesError, Resume};
use concord_sdk_digitalocean::{
    Action, DigitalOcean, Droplet, Error, HttpClass, NextLink, PageNumber, PageSize, ProviderError,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};
use std::num::NonZeroUsize;

/// Synthetic token in the personal-access-token shape (`dop_v1_` prefix).
const TOKEN: &str = "dop_v1_first_use_synthetic_token_7f3a";
const BEARER: &str = "Bearer dop_v1_first_use_synthetic_token_7f3a";
const WEB_02: u64 = 3_164_494;

// ---------------------------------------------------------------------------------------------
// The application: two caller tasks.
// ---------------------------------------------------------------------------------------------

/// What the operator dashboard shows about one droplet.
#[derive(Debug)]
struct DropletReport {
    listed: usize,
    id: u64,
    status: String,
    region: String,
    public_ipv4: Option<String>,
    quota_remaining: Option<u64>,
}

/// What the operator dashboard shows about one droplet action.
#[derive(Debug)]
struct ActionReport {
    listed: usize,
    id: u64,
    kind: String,
    status: String,
    completed_at: Option<String>,
}

#[derive(Debug)]
enum AppError {
    /// The droplet listing stopped early; the prefix read so far is kept.
    DropletListing(Box<Incomplete<Droplet, Error, NextLink>>),
    /// The action listing stopped early; the prefix read so far is kept.
    ActionListing(Box<Incomplete<Action, Error, NextLink>>),
    /// The complete listing does not contain what the operator asked for.
    NotListed,
    /// A single retrieval failed.
    Call(Error),
}

impl From<Error> for AppError {
    fn from(error: Error) -> Self {
        AppError::Call(error)
    }
}

/// Caller task 1: list every droplet (following `links.pages.next`), select one by name, then
/// retrieve it to report its current state.
async fn inspect_droplet(api: &DigitalOcean, name: &str) -> Result<DropletReport, AppError> {
    let page_size = PageSize::new(2)?;
    let max_pages = NonZeroUsize::new(5).expect("nonzero");
    let droplets = api
        .droplet_pages(page_size, max_pages)
        .items()
        .collect_complete()
        .await
        .map_err(|incomplete| AppError::DropletListing(Box::new(incomplete)))?;
    let listed = droplets.len();
    let selected = droplets
        .iter()
        .find(|d| d.name == name)
        .ok_or(AppError::NotListed)?;

    let response = api.droplet(selected.id).get().send().await?;
    let droplet = response.value.droplet;
    let public_ipv4 = droplet
        .networks
        .v4
        .iter()
        .find(|n| n.kind.as_deref() == Some("public"))
        .and_then(|n| n.ip_address.clone());
    Ok(DropletReport {
        listed,
        id: droplet.id,
        status: droplet.status,
        region: droplet.region.slug,
        public_ipv4,
        quota_remaining: response.meta.remaining().ok().flatten(),
    })
}

/// Caller task 2: list the selected droplet's action history, select the first action still
/// in progress, then retrieve that action to report its current state.
async fn inspect_running_action(
    api: &DigitalOcean,
    droplet_id: u64,
) -> Result<ActionReport, AppError> {
    let page_size = PageSize::new(2)?;
    let max_pages = NonZeroUsize::new(5).expect("nonzero");
    let actions = api
        .action_pages(droplet_id, page_size, max_pages)
        .items()
        .collect_complete()
        .await
        .map_err(|incomplete| AppError::ActionListing(Box::new(incomplete)))?;
    let listed = actions.len();
    let running = actions
        .iter()
        .find(|a| a.status == "in-progress")
        .ok_or(AppError::NotListed)?;

    let response = api.droplet(droplet_id).action(running.id).send().await?;
    let action = response.value.action;
    Ok(ActionReport {
        listed,
        id: action.id,
        kind: action.kind,
        status: action.status,
        completed_at: action.completed_at,
    })
}

// ---------------------------------------------------------------------------------------------
// Fixtures: realistic DigitalOcean-shaped documents (synthetic values).
// ---------------------------------------------------------------------------------------------

fn droplet(id: u64, name: &str, status: &str, region: &str, public_ip: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "memory": 2048,
        "vcpus": 2,
        "disk": 60,
        "locked": false,
        "status": status,
        "kernel": null,
        "created_at": "2026-10-30T14:12:31Z",
        "features": ["monitoring", "ipv6", "private_networking"],
        "backup_ids": [],
        "next_backup_window": null,
        "snapshot_ids": [],
        "image": {
            "id": 191_457_505,
            "name": "24.04 (LTS) x64",
            "distribution": "Ubuntu",
            "slug": "ubuntu-24-04-x64",
            "public": true
        },
        "volume_ids": [],
        "size": {
            "slug": "s-2vcpu-2gb",
            "memory": 2048,
            "vcpus": 2,
            "disk": 60,
            "transfer": 3.0,
            "price_monthly": 18.0,
            "price_hourly": 0.02679,
            "regions": [region],
            "available": true,
            "description": "Basic"
        },
        "size_slug": "s-2vcpu-2gb",
        "networks": {
            "v4": [
                {"ip_address": "10.114.0.7", "netmask": "255.255.240.0", "type": "private"},
                {"ip_address": public_ip, "netmask": "255.255.240.0",
                 "gateway": "203.0.113.1", "type": "public"}
            ],
            "v6": [
                {"ip_address": "2001:db8:0:1::10", "netmask": 64,
                 "gateway": "2001:db8:0:1::1", "type": "public"}
            ]
        },
        "region": {
            "name": "Frankfurt 1",
            "slug": region,
            "features": ["backups", "ipv6", "metadata"],
            "available": true,
            "sizes": ["s-2vcpu-2gb"]
        },
        "tags": ["web", "env:prod"],
        "vpc_uuid": "5a4981aa-9653-4bd1-bef5-d6bff52042e4"
    })
}

fn action(id: u64, status: &str, kind: &str, completed_at: Option<&str>) -> Value {
    json!({
        "id": id,
        "status": status,
        "type": kind,
        "started_at": "2026-11-07T09:15:02Z",
        "completed_at": completed_at,
        "resource_id": WEB_02,
        "resource_type": "droplet",
        "region": {"name": "Frankfurt 1", "slug": "fra1", "available": true},
        "region_slug": "fra1"
    })
}

/// A 200 JSON reply with DigitalOcean's quota headers.
fn ok(body: Value, remaining: u64) -> Reply {
    Reply::json(200, body.to_string())
        .header("ratelimit-limit", "5000")
        .header("ratelimit-remaining", remaining.to_string())
        .header("ratelimit-reset", "1762507800")
}

fn problem(status: u16, id: &str, message: &str) -> Reply {
    Reply::json(
        status,
        json!({"id": id, "message": message, "request_id": "4d9d8375-3c56-4925-a3e7-eb137fed17e9"})
            .to_string(),
    )
}

fn not_found() -> Reply {
    problem(
        404,
        "not_found",
        "The resource you requested could not be found.",
    )
}

/// The base a fixture uses for absolute `links.pages` URLs: its own origin, as the client saw it.
fn origin(req: &Recorded) -> String {
    format!("http://{}", req.host)
}

fn droplets_page_one(next: &str, last: &str) -> Value {
    json!({
        "droplets": [
            droplet(3_164_444, "web-01", "active", "fra1", "203.0.113.24"),
            droplet(WEB_02, "web-02", "active", "fra1", "203.0.113.25"),
        ],
        "links": {"pages": {"next": next, "last": last}},
        "meta": {"total": 3}
    })
}

fn droplets_page_two(base: &str) -> Value {
    json!({
        "droplets": [droplet(3_164_512, "worker-01", "off", "ams3", "198.51.100.40")],
        "links": {"pages": {
            "first": format!("{base}/v2/droplets?page=1&per_page=2"),
            "prev": format!("{base}/v2/droplets?page=1&per_page=2")
        }},
        "meta": {"total": 3}
    })
}

/// The happy-path fixture for both workflows, routed by raw request target.
fn provider(req: &Recorded) -> Reply {
    let base = origin(req);
    match (req.method.as_str(), req.target.as_str()) {
        ("GET", "/v2/droplets?per_page=2") => ok(
            droplets_page_one(
                &format!("{base}/v2/droplets?page=2&per_page=2"),
                &format!("{base}/v2/droplets?page=2&per_page=2"),
            ),
            4998,
        ),
        ("GET", "/v2/droplets?page=2&per_page=2") => ok(droplets_page_two(&base), 4997),
        ("GET", "/v2/droplets/3164494") => ok(
            json!({"droplet": droplet(WEB_02, "web-02", "active", "fra1", "203.0.113.25")}),
            4996,
        ),
        ("GET", "/v2/droplets/3164494/actions?per_page=2") => ok(
            json!({
                "actions": [
                    action(2_711_100_301, "completed", "power_on", Some("2026-11-07T09:15:40Z")),
                    action(2_711_100_302, "completed", "snapshot", Some("2026-11-07T09:21:13Z")),
                ],
                "links": {"pages": {
                    "next": format!("{base}/v2/droplets/3164494/actions?page=2&per_page=2"),
                    "last": format!("{base}/v2/droplets/3164494/actions?page=2&per_page=2")
                }},
                "meta": {"total": 3}
            }),
            4995,
        ),
        ("GET", "/v2/droplets/3164494/actions?page=2&per_page=2") => ok(
            json!({
                "actions": [action(2_711_100_377, "in-progress", "reboot", None)],
                "links": {"pages": {
                    "first": format!("{base}/v2/droplets/3164494/actions?page=1&per_page=2"),
                    "prev": format!("{base}/v2/droplets/3164494/actions?page=1&per_page=2")
                }},
                "meta": {"total": 3}
            }),
            4994,
        ),
        ("GET", "/v2/droplets/3164494/actions/2711100377") => ok(
            json!({"action": action(2_711_100_377, "in-progress", "reboot", None)}),
            4993,
        ),
        _ => not_found(),
    }
}

fn targets(server: &TestServer) -> Vec<String> {
    server.requests().into_iter().map(|r| r.target).collect()
}

/// Every request this SDK sends: a bodiless GET carrying the bearer token, JSON accept, no
/// referer, to the fixture's own host.
fn assert_ordinary_get(req: &Recorded, server: &TestServer) {
    assert_eq!(req.method, "GET");
    assert_eq!(req.header("authorization"), Some(BEARER));
    assert_eq!(req.header("accept"), Some("application/json"));
    assert_eq!(req.header("referer"), None);
    assert_eq!(req.header("content-type"), None);
    assert_eq!(req.host, server.url().trim_start_matches("http://"));
    assert!(req.body.is_empty());
    assert_eq!(req.body_len, 0);
}

fn provider_error(error: &Error) -> &ProviderError {
    match error {
        Error::Provider(provider) => provider,
        other => panic!("expected an HTTP failure, got {other:?}"),
    }
}

fn assert_redacted(rendered: &str) {
    assert!(!rendered.contains("dop_v1_"), "token leaked: {rendered}");
    assert!(
        !rendered.contains("synthetic_token"),
        "token leaked: {rendered}"
    );
}

// ---------------------------------------------------------------------------------------------
// Workflow 1: list droplets with links.pages, then retrieve a droplet.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn workflow_1_lists_every_page_then_retrieves_the_selected_droplet() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let report = inspect_droplet(&api, "web-02").await.unwrap();
    assert_eq!(
        report.listed, 3,
        "both pages were read; the last has no next link"
    );
    assert_eq!(report.id, WEB_02);
    assert_eq!(report.status, "active");
    assert_eq!(report.region, "fra1");
    assert_eq!(report.public_ipv4.as_deref(), Some("203.0.113.25"));
    assert_eq!(report.quota_remaining, Some(4996));

    // Wire: first page without `page`, the next link verbatim, then the droplet.
    assert_eq!(
        targets(&server),
        [
            "/v2/droplets?per_page=2",
            "/v2/droplets?page=2&per_page=2",
            "/v2/droplets/3164494",
        ]
    );
    for req in server.requests() {
        assert_ordinary_get(&req, &server);
    }
}

#[tokio::test]
async fn workflow_1_retrieval_exposes_typed_fields_and_quota_metadata() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let response = api.droplet(WEB_02).get().send().await.unwrap();
    assert_eq!(response.meta.status().as_u16(), 200);
    let operation = response.meta.operation().expect("declared endpoint");
    assert_eq!(operation.name(), "DigitalOcean.droplet.get");
    assert_eq!(operation.method(), Some("GET"));
    assert_eq!(response.meta.limit(), Ok(Some(5000)));
    assert_eq!(response.meta.remaining(), Ok(Some(4996)));
    assert_eq!(response.meta.reset_epoch_seconds(), Ok(Some(1_762_507_800)));
    assert_eq!(response.meta.retry_after_seconds(), Ok(None));

    let droplet = &response.value.droplet;
    assert_eq!((droplet.memory, droplet.vcpus, droplet.disk), (2048, 2, 60));
    assert_eq!(droplet.size_slug, "s-2vcpu-2gb");
    assert_eq!(droplet.created_at, "2026-10-30T14:12:31Z");
    assert_eq!(droplet.region.name, "Frankfurt 1");
    assert_eq!(droplet.tags, ["web", "env:prod"]);
    let private = &droplet.networks.v4[0];
    assert_eq!(private.kind.as_deref(), Some("private"));
    assert_eq!(
        private.gateway, None,
        "a private interface without a gateway decodes"
    );
    assert_eq!(droplet.networks.v6[0].netmask, Some(64));

    // Formatting the wrapper does not print the decoded document.
    let rendered = format!("{response:?}");
    assert!(!rendered.contains("web-02"), "{rendered}");
}

#[tokio::test]
async fn workflow_1_page_metadata_is_available_per_page() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let mut pages = api.droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(5).unwrap());
    let first = pages.next_page().await.unwrap().expect("first page");
    assert_eq!(first.items.len(), 2);
    assert!(first.next.is_some());
    assert_eq!(first.meta.remaining(), Ok(Some(4998)));
    assert_eq!(
        first.meta.operation().map(|o| o.name()),
        Some("DigitalOcean.droplets.list")
    );
    let second = pages.next_page().await.unwrap().expect("second page");
    assert_eq!(second.items.len(), 1);
    assert!(
        second.next.is_none(),
        "no next link: this was the last page"
    );
    assert_eq!(second.meta.remaining(), Ok(Some(4997)));
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "Ok(None) is completion"
    );
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "and stays completion"
    );
    assert_eq!(server.requests().len(), 2, "completion sends nothing more");
}

#[tokio::test]
async fn workflow_1_missing_next_link_means_completion() {
    // Every shape in which DigitalOcean has no further page: no `links` (20 or fewer objects),
    // empty `links`, empty `pages`, null `next`, and only backward relations. A `meta.total`
    // larger than what was returned never invents a page.
    let shapes: Vec<Value> = vec![
        json!({"droplets": [droplet(1, "a", "active", "nyc3", "192.0.2.1")], "meta": {"total": 1}}),
        json!({"droplets": [droplet(1, "a", "active", "nyc3", "192.0.2.1")], "links": {}, "meta": {"total": 1}}),
        json!({"droplets": [droplet(1, "a", "active", "nyc3", "192.0.2.1")], "links": {"pages": {}}, "meta": {"total": 1}}),
        json!({"droplets": [droplet(1, "a", "active", "nyc3", "192.0.2.1")], "links": {"pages": {"next": null}}, "meta": {"total": 1}}),
        json!({"droplets": [droplet(1, "a", "active", "nyc3", "192.0.2.1")],
               "links": {"pages": {"first": "http://127.0.0.1:9/v2/droplets?page=1",
                                   "prev": "http://127.0.0.1:9/v2/droplets?page=1"}},
               "meta": {"total": 50}}),
        json!({"droplets": [], "meta": {"total": 0}}),
    ];
    for (index, shape) in shapes.into_iter().enumerate() {
        let body = shape.to_string();
        let server = TestServer::start(move |_req: &Recorded| {
            ok(serde_json::from_str(&body).unwrap(), 4000)
        })
        .await;
        let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();
        let droplets = api
            .droplet_pages(PageSize::new(20).unwrap(), NonZeroUsize::new(3).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap_or_else(|e| panic!("shape {index}: {e:?}"));
        assert!(droplets.len() <= 1, "shape {index}");
        assert_eq!(
            targets(&server),
            ["/v2/droplets?per_page=20"],
            "shape {index}"
        );
    }
}

/// Builds a `links.pages.next` value from the fixture's own origin.
type LinkFrom = Box<dyn Fn(&str) -> String + Send + Sync>;

#[tokio::test]
async fn workflow_1_untrusted_next_host_never_receives_the_bearer_token() {
    // A second loopback server stands in for an attacker-controlled host named by a response.
    let untrusted =
        TestServer::start(|_req: &Recorded| Reply::json(200, json!({"droplets": []}).to_string()))
            .await;
    let untrusted_origin = untrusted.url();
    let untrusted_port = untrusted.addr().port();

    // Each case names page 2 somewhere else than this collection on this origin.
    let cases: Vec<(&str, LinkFrom)> = vec![
        (
            "other port",
            Box::new(move |_| format!("{untrusted_origin}/v2/droplets?page=2&per_page=2")),
        ),
        (
            "other host name",
            Box::new(|base: &str| {
                base.replace("127.0.0.1", "localhost") + "/v2/droplets?page=2&per_page=2"
            }),
        ),
        (
            "https scheme",
            Box::new(|base: &str| {
                base.replace("http://", "https://") + "/v2/droplets?page=2&per_page=2"
            }),
        ),
        (
            "other collection",
            Box::new(|base: &str| format!("{base}/v2/images?page=2&per_page=2")),
        ),
        (
            "user info",
            Box::new(|base: &str| {
                base.replace("http://", "http://dop_v1_x:y@") + "/v2/droplets?page=2&per_page=2"
            }),
        ),
        (
            "fragment",
            Box::new(|base: &str| format!("{base}/v2/droplets?page=2&per_page=2#x")),
        ),
        (
            "relative",
            Box::new(|_| "/v2/droplets?page=2&per_page=2".to_owned()),
        ),
        ("empty", Box::new(|_| String::new())),
        (
            "backslash",
            Box::new(|base: &str| format!("{base}/v2\\droplets?page=2")),
        ),
    ];

    for (case, next) in cases {
        let server = TestServer::start(move |req: &Recorded| {
            let base = origin(req);
            let link = next(&base);
            ok(droplets_page_one(&link, &link), 4998)
        })
        .await;
        let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

        let incomplete = api
            .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(5).unwrap())
            .items()
            .collect_complete()
            .await
            .expect_err(case);
        // The stop is a typed failure, not completion. OBSERVED (mismatch with my prediction,
        // see FIRST_USE.md): the page that carried the unsafe link fails as a whole, so its two
        // valid droplets are not in the prefix and the reported retry point is that same page.
        assert!(incomplete.collected.is_empty(), "{case}");
        assert!(
            matches!(incomplete.error.provider_error(), Some(Error::UnsafeNext)),
            "{case}: {:?}",
            incomplete.error
        );
        assert!(
            matches!(
                incomplete.error,
                PagesError::Provider {
                    failed: Resume::Start,
                    ..
                }
            ),
            "{case}: {:?}",
            incomplete.error
        );
        assert_eq!(
            targets(&server),
            ["/v2/droplets?per_page=2"],
            "{case}: nothing followed"
        );
        assert_redacted(&format!("{incomplete:?} {incomplete}"));
        assert!(!format!("{incomplete:?} {incomplete}").contains(&untrusted_port.to_string()));
    }
    assert!(
        untrusted.requests().is_empty(),
        "the untrusted host saw no request at all"
    );

    // Workaround a caller has for the lost page: the ordinary single-page call still returns the
    // page's records, with the raw (unvalidated, untrusted) link left as a plain string.
    let untrusted_origin = untrusted.url();
    let server = TestServer::start(move |_req: &Recorded| {
        let link = format!("{untrusted_origin}/v2/droplets?page=2&per_page=2");
        ok(droplets_page_one(&link, &link), 4998)
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();
    let page = api
        .droplets()
        .list()
        .set_per_page(PageSize::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.value.droplets.len(), 2);
    let raw_next = page
        .value
        .links
        .as_ref()
        .and_then(|l| l.pages.as_ref())
        .and_then(|p| p.next.as_deref());
    assert!(raw_next.is_some_and(|next| next.starts_with(&untrusted.url())));
    assert!(untrusted.requests().is_empty());

    // Control: the stand-in host does record what reaches it (and would see the token), so its
    // empty request log above is evidence, not an artefact of the fixture.
    let direct = DigitalOcean::for_fixture(TOKEN, &untrusted.url()).unwrap();
    direct.droplets().list().send().await.unwrap();
    assert_eq!(untrusted.single().header("authorization"), Some(BEARER));
}

#[tokio::test]
async fn workflow_1_unsafe_link_on_a_later_page_keeps_only_earlier_pages() {
    let untrusted =
        TestServer::start(|_req: &Recorded| Reply::json(200, json!({"droplets": []}).to_string()))
            .await;
    let foreign = format!("{}/v2/droplets?page=3&per_page=2", untrusted.url());
    let server = TestServer::start(move |req: &Recorded| match req.target.as_str() {
        "/v2/droplets?page=2&per_page=2" => ok(
            json!({
                "droplets": [droplet(3_164_512, "worker-01", "off", "ams3", "198.51.100.40")],
                "links": {"pages": {"next": foreign.clone()}},
                "meta": {"total": 5}
            }),
            4997,
        ),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let incomplete = api
        .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    // OBSERVED: page one is the prefix; page two's own record (worker-01) is dropped with it.
    let names: Vec<&str> = incomplete
        .collected
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, ["web-01", "web-02"]);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::UnsafeNext)
    ));
    assert!(matches!(
        incomplete.error,
        PagesError::Provider {
            failed: Resume::At(_),
            ..
        }
    ));
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(server.requests().len(), 2);
    assert!(untrusted.requests().is_empty());
}

#[tokio::test]
async fn workflow_1_redirect_to_untrusted_host_is_an_http_failure() {
    let untrusted =
        TestServer::start(|_req: &Recorded| Reply::json(200, json!({"droplets": []}).to_string()))
            .await;
    let location = format!("{}/v2/droplets?page=2&per_page=2", untrusted.url());
    let server = TestServer::start(move |req: &Recorded| match req.target.as_str() {
        "/v2/droplets?page=2&per_page=2" => Reply::new(302).header("location", location.clone()),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let incomplete = api
        .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let failure = provider_error(incomplete.error.provider_error().expect("page failure"));
    assert_eq!(failure.meta.status().as_u16(), 302);
    assert_eq!(failure.class(), HttpClass::Other);
    assert!(
        failure.document.is_err(),
        "a bodiless redirect has no error document"
    );
    assert!(untrusted.requests().is_empty(), "redirect not followed");
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn workflow_1_droplet_deleted_between_list_and_get() {
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets/3164494" => not_found(),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let error = inspect_droplet(&api, "web-02").await.unwrap_err();
    let AppError::Call(Error::Provider(failure)) = &error else {
        panic!("expected the retrieval to fail, got {error:?}");
    };
    assert_eq!(failure.class(), HttpClass::NotFound);
    assert_eq!(
        failure.meta.operation().map(|o| o.name()),
        Some("DigitalOcean.droplet.get")
    );
    assert_eq!(server.requests().len(), 3);

    // Selecting a droplet that is not listed is the application's own outcome.
    assert!(matches!(
        inspect_droplet(&api, "db-01").await,
        Err(AppError::NotListed)
    ));
}

#[tokio::test]
async fn workflow_1_page_cap_with_more_pages_is_bounded_not_complete() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let incomplete = api
        .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(
        matches!(incomplete.error, PagesError::Bounded { .. }),
        "{:?}",
        incomplete.error
    );
    assert!(
        incomplete.error.resume().is_some(),
        "the first unread page is reported"
    );
    assert_eq!(server.requests().len(), 1);

    // Item by item: the cap is an error, and every later call says the traversal ended.
    let mut items = api
        .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(1).unwrap())
        .items();
    assert!(items.next().await.unwrap().is_some());
    assert!(items.next().await.unwrap().is_some());
    assert!(matches!(
        items.next().await,
        Err(PagesError::Bounded { .. })
    ));
    assert!(matches!(items.next().await, Err(PagesError::Ended { .. })));
    assert_eq!(server.requests().len(), 2);

    // A cap reached exactly on the last page is completion.
    let all = api
        .droplet_pages(PageSize::new(2).unwrap(), NonZeroUsize::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn workflow_1_rate_limited_page_keeps_prefix_and_is_not_replayed() {
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets?page=2&per_page=2" => {
            problem(429, "too_many_requests", "API Rate limit exceeded.")
                .header("ratelimit-limit", "5000")
                .header("ratelimit-remaining", "0")
                .header("ratelimit-reset", "1762507800")
                .header("retry-after", "30")
        }
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let error = inspect_droplet(&api, "worker-01").await.unwrap_err();
    let AppError::DropletListing(incomplete) = error else {
        panic!("expected an incomplete listing, got {error:?}");
    };
    let names: Vec<&str> = incomplete
        .collected
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, ["web-01", "web-02"]);
    let failure = provider_error(incomplete.error.provider_error().unwrap());
    assert_eq!(failure.class(), HttpClass::RateLimited);
    assert_eq!(failure.meta.status().as_u16(), 429);
    assert_eq!(failure.meta.retry_after_seconds(), Ok(Some(30)));
    assert_eq!(failure.meta.remaining(), Ok(Some(0)));
    let document = failure.document.as_ref().expect("typed error document");
    assert_eq!(document.id, "too_many_requests");
    assert_eq!(
        document.request_id.as_deref(),
        Some("4d9d8375-3c56-4925-a3e7-eb137fed17e9")
    );
    assert_eq!(
        targets(&server),
        ["/v2/droplets?per_page=2", "/v2/droplets?page=2&per_page=2"],
        "one attempt per page, no automatic replay"
    );
    let rendered = format!("{incomplete:?} {incomplete}");
    assert!(!rendered.contains("API Rate limit exceeded"), "{rendered}");
}

#[tokio::test]
async fn workflow_1_retrieval_failures_keep_status_authoritative() {
    let server = TestServer::start(|req: &Recorded| match req.path() {
        "/v2/droplets/401" => problem(401, "unauthorized", "Unable to authenticate you"),
        "/v2/droplets/403" => problem(
            403,
            "forbidden",
            "You do not have access for the attempted action.",
        ),
        "/v2/droplets/404" => not_found(),
        "/v2/droplets/500" => {
            Reply::text(500, "text/html", "<html><body>Bad gateway</body></html>")
        }
        "/v2/droplets/503" => Reply::new(503),
        "/v2/droplets/502" => problem(
            502,
            "not_found",
            "The resource you requested could not be found.",
        ),
        _ => not_found(),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    for (id, class, typed) in [
        (401u64, HttpClass::Unauthorized, Some("unauthorized")),
        (403, HttpClass::Forbidden, Some("forbidden")),
        (404, HttpClass::NotFound, Some("not_found")),
        (500, HttpClass::Server, None),
        (503, HttpClass::Server, None),
        // The document contradicts the status; the status decides the class.
        (502, HttpClass::Server, Some("not_found")),
    ] {
        let error = api.droplet(id).get().send().await.unwrap_err();
        let failure = provider_error(&error);
        assert_eq!(failure.class(), class, "{id}");
        assert_eq!(failure.meta.status().as_u16(), id as u16);
        match typed {
            Some(expected) => {
                let document = failure.document.as_ref().expect("typed document");
                assert_eq!(document.id, expected);
                assert!(!document.message.is_empty());
            }
            None => assert!(
                failure.document.is_err(),
                "{id}: unreadable document kept as Err"
            ),
        }
        let rendered = format!("{error} {error:?}");
        assert_redacted(&rendered);
        assert!(!rendered.contains("Unable to authenticate"), "{rendered}");
        assert!(!rendered.contains("Bad gateway"), "{rendered}");
    }
    assert_eq!(server.requests().len(), 6, "no 401/5xx replay");
}

#[tokio::test]
async fn workflow_1_malformed_success_bodies_are_concord_errors() {
    let server = TestServer::start(|req: &Recorded| match req.path() {
        "/v2/droplets/1" => Reply::json(200, r#"{"droplet": {"id": 1, "name": "trunc"#),
        "/v2/droplets/2" => Reply::json(200, r#"{"droplet": 3}"#),
        "/v2/droplets/3" => Reply::text(200, "text/html", "<html>login portal</html>"),
        "/v2/droplets/4" => Reply::json(
            200,
            json!({"droplet": {"id": 4, "name": "no-region"}}).to_string(),
        ),
        _ => not_found(),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    for (id, expected) in [
        (1u64, ErrorKind::Decode),
        (2, ErrorKind::Decode),
        (3, ErrorKind::Metadata),
        (4, ErrorKind::Decode),
    ] {
        let error = api.droplet(id).get().send().await.unwrap_err();
        let Error::Concord(concord) = &error else {
            panic!("{id}: expected a Concord failure, got {error:?}");
        };
        assert_eq!(concord.kind(), expected, "{id}");
        assert_eq!(concord.head().map(|h| h.status().as_u16()), Some(200));
        assert_redacted(&format!("{error} {error:?}"));
        assert!(!format!("{error} {error:?}").contains("login portal"));
    }
}

#[tokio::test]
async fn workflow_1_quota_headers_are_parsed_strictly() {
    let server = TestServer::start(|req: &Recorded| {
        let body = json!({"droplet": droplet(WEB_02, "web-02", "active", "fra1", "203.0.113.25")});
        match req.path() {
            "/v2/droplets/1" => Reply::json(200, body.to_string()),
            _ => Reply::json(200, body.to_string())
                .header("ratelimit-limit", "5000")
                .header("ratelimit-limit", "5000")
                .header("ratelimit-remaining", "lots")
                .header("ratelimit-reset", "99999999999999999999999"),
        }
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let missing = api.droplet(1u64).get().send().await.unwrap();
    assert_eq!(missing.meta.limit(), Ok(None));
    assert_eq!(missing.meta.retry_after_seconds(), Ok(None));

    let odd = api.droplet(2u64).get().send().await.unwrap();
    assert_eq!(
        odd.value.droplet.id, WEB_02,
        "bad metadata does not fail the call"
    );
    assert!(matches!(odd.meta.limit(), Err(FieldError::Repeated)));
    assert!(matches!(odd.meta.remaining(), Err(FieldError::Invalid)));
    assert!(matches!(
        odd.meta.reset_epoch_seconds(),
        Err(FieldError::Overflow)
    ));
}

#[tokio::test]
async fn workflow_1_invalid_inputs_fail_before_any_request() {
    let server = TestServer::start(provider).await;
    let url = server.url();
    let port = server.addr().port();

    assert!(matches!(
        DigitalOcean::for_fixture("dop_v1_bad\ntoken", &url),
        Err(Error::Credential)
    ));
    assert!(matches!(
        DigitalOcean::connect("dop_v1_bad\r\ntoken"),
        Err(Error::Credential)
    ));
    assert!(matches!(DigitalOcean::connect(""), Err(Error::Credential)));
    for origin in [
        format!("http://localhost:{port}"),
        format!("https://127.0.0.1:{port}"),
        format!("http://127.0.0.1:{port}/v2"),
        format!("http://user:pw@127.0.0.1:{port}"),
        format!("http://192.0.2.10:{port}"),
        "https://api.digitalocean.com".to_owned(),
    ] {
        assert!(
            matches!(
                DigitalOcean::for_fixture(TOKEN, &origin),
                Err(Error::FixtureOrigin)
            ),
            "{origin}"
        );
    }
    assert!(matches!(PageSize::new(0), Err(Error::PageSize)));
    assert!(matches!(PageSize::new(201), Err(Error::PageSize)));
    assert!(PageSize::new(200).is_ok());
    assert!(matches!(PageNumber::new(0), Err(Error::PageNumber)));
    assert!(server.requests().is_empty());

    // The production client builds requests for the real host but is only inspected here.
    let production = DigitalOcean::connect(TOKEN).unwrap();
    let prepared = production.droplet(WEB_02).get().prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.digitalocean.com/v2/droplets/3164494"
    );
    let authorization = prepared.request().headers().get("authorization").unwrap();
    assert!(authorization.is_sensitive());
    assert_redacted(&format!("{production:?} {prepared:?}"));
}

// ---------------------------------------------------------------------------------------------
// Workflow 2: list the selected droplet's actions, then retrieve one action.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn workflow_2_lists_action_history_then_retrieves_the_running_action() {
    let server = TestServer::start(provider).await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    // The application chains both workflows: select the droplet, then inspect its actions.
    let droplet = inspect_droplet(&api, "web-02").await.unwrap();
    let report = inspect_running_action(&api, droplet.id).await.unwrap();
    assert_eq!(report.listed, 3);
    assert_eq!(report.id, 2_711_100_377);
    assert_eq!(report.kind, "reboot", "JSON `type` arrives as `kind`");
    assert_eq!(report.status, "in-progress");
    assert_eq!(report.completed_at, None, "null while unfinished");

    assert_eq!(
        targets(&server)[3..],
        [
            "/v2/droplets/3164494/actions?per_page=2",
            "/v2/droplets/3164494/actions?page=2&per_page=2",
            "/v2/droplets/3164494/actions/2711100377",
        ]
    );
    for req in server.requests() {
        assert_ordinary_get(&req, &server);
    }
}

#[tokio::test]
async fn workflow_2_single_page_call_and_action_metadata() {
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets/3164494/actions?page=2&per_page=50" => ok(
            json!({
                "actions": [action(2_711_100_377, "in-progress", "reboot", None)],
                "links": {"pages": {"prev": format!("{}/v2/droplets/3164494/actions?page=1&per_page=50", origin(req))}},
                "meta": {"total": 51}
            }),
            4100,
        ),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    // The ordinary single-page call with explicit page parameters.
    let page = api
        .droplet(WEB_02)
        .actions()
        .set_page(PageNumber::new(2).unwrap())
        .set_per_page(PageSize::new(50).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.value.actions.len(), 1);
    assert_eq!(page.value.meta.as_ref().map(|m| m.total), Some(51));
    let next = page
        .value
        .links
        .as_ref()
        .and_then(|l| l.pages.as_ref())
        .and_then(|p| p.next.as_ref());
    assert!(next.is_none(), "only prev: this is the last page");
    assert_eq!(
        page.meta.operation().map(|o| o.name()),
        Some("DigitalOcean.droplet.actions")
    );
    assert_eq!(
        targets(&server),
        ["/v2/droplets/3164494/actions?page=2&per_page=50"]
    );

    let retrieved = api
        .droplet(WEB_02)
        .action(2_711_100_377u64)
        .send()
        .await
        .unwrap();
    let action = &retrieved.value.action;
    assert_eq!(action.resource_id, Some(WEB_02));
    assert_eq!(action.resource_type.as_deref(), Some("droplet"));
    assert_eq!(action.region_slug.as_deref(), Some("fra1"));
    assert_eq!(
        action.region.as_ref().map(|r| r.slug.as_str()),
        Some("fra1")
    );
    assert_eq!(action.started_at, "2026-11-07T09:15:02Z");
    assert_eq!(
        retrieved.meta.operation().map(|o| o.name()),
        Some("DigitalOcean.droplet.action")
    );
    assert_eq!(retrieved.meta.remaining(), Ok(Some(4993)));
}

#[tokio::test]
async fn workflow_2_next_link_to_another_droplets_actions_is_refused() {
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets/3164494/actions?per_page=2" => ok(
            json!({
                "actions": [
                    action(2_711_100_301, "completed", "power_on", Some("2026-11-07T09:15:40Z")),
                    action(2_711_100_302, "completed", "snapshot", Some("2026-11-07T09:21:13Z")),
                ],
                "links": {"pages": {"next": format!("{}/v2/droplets/999/actions?page=2&per_page=2", origin(req))}}
            }),
            4900,
        ),
        _ => provider(req),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let error = inspect_running_action(&api, WEB_02).await.unwrap_err();
    let AppError::ActionListing(incomplete) = error else {
        panic!("expected an incomplete action listing, got {error:?}");
    };
    // OBSERVED (prediction mismatch): the page carrying the foreign link fails whole.
    assert!(incomplete.collected.is_empty());
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::UnsafeNext)
    ));
    assert_eq!(
        targets(&server),
        ["/v2/droplets/3164494/actions?per_page=2"]
    );
}

#[tokio::test]
async fn workflow_2_empty_page_continues_and_server_error_keeps_prefix() {
    let server = TestServer::start(|req: &Recorded| {
        let base = origin(req);
        match req.target.as_str() {
            "/v2/droplets/3164494/actions?per_page=2" => ok(
                json!({
                    "actions": [
                        action(2_711_100_301, "completed", "power_on", Some("2026-11-07T09:15:40Z")),
                        action(2_711_100_302, "errored", "resize", Some("2026-11-07T09:30:00Z")),
                    ],
                    "links": {"pages": {"next": format!("{base}/v2/droplets/3164494/actions?page=2&per_page=2")}},
                    "meta": {"total": 6}
                }),
                4800,
            ),
            "/v2/droplets/3164494/actions?page=2&per_page=2" => ok(
                json!({
                    "actions": [],
                    "links": {"pages": {"next": format!("{base}/v2/droplets/3164494/actions?page=3&per_page=2")}},
                    "meta": {"total": 6}
                }),
                4799,
            ),
            "/v2/droplets/3164494/actions?page=3&per_page=2" => {
                Reply::text(500, "text/html", "<html>upstream connect error</html>")
            }
            _ => not_found(),
        }
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    let error = inspect_running_action(&api, WEB_02).await.unwrap_err();
    let AppError::ActionListing(incomplete) = error else {
        panic!("expected an incomplete action listing, got {error:?}");
    };
    let ids: Vec<u64> = incomplete.collected.iter().map(|a| a.id).collect();
    assert_eq!(ids, [2_711_100_301, 2_711_100_302]);
    let failure = provider_error(incomplete.error.provider_error().unwrap());
    assert_eq!(failure.class(), HttpClass::Server);
    assert!(failure.document.is_err());
    assert_eq!(
        server.requests().len(),
        3,
        "the empty page was followed, the 500 not replayed"
    );
    assert!(!format!("{incomplete:?} {incomplete}").contains("upstream connect error"));
}

#[tokio::test]
async fn workflow_2_malformed_first_page_and_missing_action() {
    let server = TestServer::start(|req: &Recorded| match req.target.as_str() {
        "/v2/droplets/42/actions?per_page=2" => {
            Reply::json(200, r#"{"actions": [{"id": 1, "status": "comp"#)
        }
        "/v2/droplets/43/actions?per_page=2" => Reply::json(200, r#"{"actions": {"id": 1}}"#),
        _ => not_found(),
    })
    .await;
    let api = DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap();

    for droplet_id in [42u64, 43] {
        let error = inspect_running_action(&api, droplet_id).await.unwrap_err();
        let AppError::ActionListing(incomplete) = error else {
            panic!("expected an incomplete action listing, got {error:?}");
        };
        assert!(incomplete.collected.is_empty());
        let Some(Error::Concord(concord)) = incomplete.error.provider_error() else {
            panic!("expected a decode failure, got {:?}", incomplete.error);
        };
        assert_eq!(concord.kind(), ErrorKind::Decode);
    }

    let error = api.droplet(WEB_02).action(1u64).send().await.unwrap_err();
    let failure = provider_error(&error);
    assert_eq!(failure.class(), HttpClass::NotFound);
    assert_eq!(
        failure.document.as_ref().map(|p| p.id.as_str()).ok(),
        Some("not_found")
    );
    assert_eq!(
        targets(&server).last().map(String::as_str),
        Some("/v2/droplets/3164494/actions/1")
    );
}
