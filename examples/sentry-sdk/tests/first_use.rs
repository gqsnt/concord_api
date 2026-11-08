//! Fresh-consumer application: two caller tasks against a loopback Sentry fixture.
//!
//! Workflow 1: list an organization's issues by cursor and retrieve a chosen issue.
//! Workflow 2: page an issue's events with the Link header and retrieve one event.
//! Written from README + rustdoc only; every response is synthetic (no provider calls).

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use concord_sdk_sentry::concord::ErrorKind;
use concord_sdk_sentry::{
    Error, HttpClass, PagesError, Resume, SendPolicy, Sentry, Settings, http_client,
};
use concord_testserver::{Recorded, Reply, TestServer};

const TOKEN: &str = "sntryu_3f9c2a7d41e84b5f9a0c6d2e8b1f4a7c3e5d9b0a2c4f6e8d1b3a5c7e9f0d2b4";
const ISSUE: &str = "4567891230";
const QUERY: &str = "is:unresolved assigned:me";

// ---------------------------------------------------------------- fixture helpers

fn api(server: &TestServer, policy: SendPolicy) -> Sentry {
    Sentry::for_deployment(
        http_client().expect("transport"),
        server.url(),
        TOKEN,
        policy,
    )
    .expect("loopback base url")
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Typical quota headers of a successful Sentry response.
fn with_rates(reply: Reply) -> Reply {
    reply
        .header("X-Sentry-Rate-Limit-Limit", "40")
        .header("X-Sentry-Rate-Limit-Remaining", "39")
        .header("X-Sentry-Rate-Limit-Reset", (epoch_secs() + 1).to_string())
        .header("X-Sentry-Rate-Limit-ConcurrentLimit", "25")
        .header("X-Sentry-Rate-Limit-ConcurrentRemaining", "24")
}

/// Sentry-style Link header: both directions, each with a `results` flag and a `cursor`.
/// Issue links echo the request's own non-cursor query, as Sentry does.
fn issue_link(r: &Recorded, prev: &str, next: &str, more: bool) -> String {
    let kept: Vec<&str> = r
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("cursor="))
        .collect();
    let base = format!("http://{}{}?{}", r.host, r.path(), kept.join("&"));
    format!(
        "<{base}&cursor={prev}>; rel=\"previous\"; results=\"false\"; cursor=\"{prev}\", \
         <{base}&cursor={next}>; rel=\"next\"; results=\"{more}\"; cursor=\"{next}\""
    )
}

/// The documentation's event-list form: `?&cursor=...` without the other parameters.
fn event_link(r: &Recorded, prev: &str, next: &str, more: bool) -> String {
    let base = format!("http://{}{}", r.host, r.path());
    format!(
        "<{base}?&cursor={prev}>; rel=\"previous\"; results=\"false\"; cursor=\"{prev}\", \
         <{base}?&cursor={next}>; rel=\"next\"; results=\"{more}\"; cursor=\"{next}\""
    )
}

fn issue_json(id: &str, short: &str, title: &str) -> String {
    format!(
        r#"{{"id":"{id}","shortId":"{short}","title":"{title}","culprit":"app/routes/checkout.ts in handler",
        "permalink":"https://acme.sentry.io/issues/{id}/","level":"error","status":"unresolved",
        "substatus":"ongoing","isPublic":false,"platform":"javascript",
        "project":{{"id":"4504","name":"backend","slug":"backend"}},"type":"error","count":"152",
        "userCount":37,"firstSeen":"2026-10-30T08:12:44.123000Z","lastSeen":"2026-11-06T22:41:09Z"}}"#
    )
}

fn event_summary(storage_id: &str, event_id: &str, title: &str) -> String {
    format!(
        r#"{{"id":"{storage_id}","eventID":"{event_id}","groupID":"{ISSUE}","title":"{title}",
        "message":"","platform":"javascript","dateCreated":"2026-11-06T22:41:09Z",
        "event.type":"error","tags":[{{"key":"environment","value":"production"}}],"user":null}}"#
    )
}

fn event_detail(storage_id: &str, event_id: &str) -> String {
    format!(
        r#"{{"id":"{storage_id}","eventID":"{event_id}","groupID":"{ISSUE}","projectID":"4504",
        "title":"TypeError: Cannot read properties of undefined (reading 'id')","platform":"javascript",
        "dateCreated":"2026-11-06T22:41:09Z","size":4211,"type":"error",
        "entries":[{{"type":"exception","data":{{"values":[{{"type":"TypeError",
        "value":"Cannot read properties of undefined (reading 'id')"}}]}}}},
        {{"type":"breadcrumbs","data":{{"values":[]}}}}],
        "tags":[{{"key":"release","value":"backend@2026.11.6"}}],"contexts":{{}}}}"#
    )
}

fn problem(status: u16, detail: &str) -> Reply {
    Reply::json(status, format!(r#"{{"detail":"{detail}"}}"#))
}

fn cursor_of(r: &Recorded) -> Option<String> {
    r.query_all("cursor").into_iter().next()
}

/// Debug/Display output that must never contain caller secrets or provider content.
fn assert_withheld(text: &str, what: &str) {
    for secret in [TOKEN, "1730937600000:2:0", "assigned:me", "TypeError"] {
        assert!(!text.contains(secret), "{what} leaked {secret:?}: {text}");
    }
}

// ================================================================ workflow 1

/// List unresolved issues assigned to me, two per page, until Sentry says `results="false"`,
/// then retrieve the issue whose short id is BACKEND-7Q.
#[tokio::test]
async fn workflow1_issues_by_cursor_then_retrieve_chosen_issue() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path() == format!("/api/0/organizations/acme/issues/{ISSUE}/") {
            return with_rates(Reply::json(
                200,
                issue_json(ISSUE, "BACKEND-7Q", "TypeError: checkout"),
            ));
        }
        assert_eq!(r.path(), "/api/0/organizations/acme/issues/");
        let (body, prev, next, more) = match cursor_of(r).as_deref() {
            None => (
                format!(
                    "[{},{}]",
                    issue_json("4567891201", "BACKEND-7A", "ValueError: a"),
                    issue_json("4567891202", "BACKEND-7B", "KeyError: b")
                ),
                "1730937600000:0:1",
                "1730937600000:2:0",
                true,
            ),
            Some("1730937600000:2:0") => (
                format!(
                    "[{},{}]",
                    issue_json(ISSUE, "BACKEND-7Q", "TypeError: checkout"),
                    issue_json("4567891203", "BACKEND-7C", "OSError: c")
                ),
                "1730937600000:2:1",
                "1730937600000:4:0",
                true,
            ),
            // Last page: Sentry still returns a next cursor, flagged results="false".
            Some("1730937600000:4:0") => (
                format!(
                    "[{}]",
                    issue_json("4567891204", "BACKEND-7D", "IndexError: d")
                ),
                "1730937600000:4:1",
                "1730937600000:6:0",
                false,
            ),
            Some(other) => panic!("unexpected cursor {other}"),
        };
        with_rates(Reply::json(200, body)).header("Link", issue_link(r, prev, next, more))
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let organization = api.organization("acme");

    let mut pages = organization
        .issues()
        .set_query(QUERY)
        .set_limit(2)
        .pages(Resume::Start, NonZeroUsize::new(10).unwrap());
    let mut seen = Vec::new();
    while let Some(page) = pages.next_page().await.expect("issue page") {
        assert_eq!(page.meta.status(), 200);
        seen.extend(page.items);
    }
    // Completion is explicit and sticky.
    assert!(pages.next_page().await.expect("still exhausted").is_none());
    let short_ids: Vec<&str> = seen.iter().map(|i| i.short_id.as_str()).collect();
    assert_eq!(
        short_ids,
        [
            "BACKEND-7A",
            "BACKEND-7B",
            "BACKEND-7Q",
            "BACKEND-7C",
            "BACKEND-7D"
        ]
    );

    // Wire: three list requests, the cursor projected into the declared operation with the
    // same filters; the results="false" cursor 1730937600000:6:0 is never requested.
    let list: Vec<Recorded> = server.requests();
    assert_eq!(list.len(), 3);
    eprintln!(
        "observed list targets: {:?}",
        list.iter().map(|r| r.target.clone()).collect::<Vec<_>>()
    );
    for (r, cursor) in list
        .iter()
        .zip([None, Some("1730937600000:2:0"), Some("1730937600000:4:0")])
    {
        assert_eq!(r.method, "GET");
        assert_eq!(r.path(), "/api/0/organizations/acme/issues/");
        assert_eq!(r.query_all("limit"), ["2"]);
        assert_eq!(r.query_all("query"), [QUERY]);
        assert_eq!(cursor_of(r).as_deref(), cursor);
        assert_eq!(
            r.header("authorization"),
            Some(format!("Bearer {TOKEN}").as_str())
        );
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(r.body.is_empty());
    }

    // Retrieve the chosen issue.
    let chosen = seen
        .iter()
        .find(|i| i.short_id == "BACKEND-7Q")
        .expect("chosen issue");
    let detail = organization
        .issue(&chosen.id)
        .get()
        .send()
        .await
        .expect("issue detail");
    assert_eq!(detail.value.id, ISSUE);
    assert_eq!(detail.value.status, "unresolved");
    assert_eq!(detail.value.title, "TypeError: checkout");
    assert_eq!(detail.meta.status(), 200);
    assert_eq!(detail.next_cursor, None);
    assert_eq!(detail.retry.retry.attempts, 1);
    assert_eq!(detail.rates.limit, Some(40));
    assert_eq!(detail.rates.remaining, Some(39));
    assert_eq!(detail.rates.concurrent_limit, Some(25));
    assert_eq!(detail.rates.concurrent_remaining, Some(24));
    assert!(detail.rates.reset.is_some());
    let get = server.requests().pop().unwrap();
    assert_eq!(
        get.target,
        format!("/api/0/organizations/acme/issues/{ISSUE}/")
    );
    assert_eq!(
        get.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );

    // Diagnostics withhold token, cursor, filter and provider content.
    assert_withheld(&format!("{detail:?}"), "Response Debug");
    assert_withheld(&format!("{:?}", detail.value), "Issue Debug");
    assert_withheld(&format!("{api:?}"), "client Debug");
    assert_withheld(
        &format!(
            "{:?}",
            organization
                .issues()
                .set_query(QUERY)
                .set_cursor("1730937600000:2:0")
        ),
        "request Debug",
    );
    server.shutdown().await;
}

/// One page through `send()`: the continuation is a cursor, not a URL; a capped traversal
/// reports `Bounded` with a resume cursor and a new traversal continues from it.
#[tokio::test]
async fn workflow1_single_page_cursor_and_bounded_resume() {
    let server = TestServer::start(|r: &Recorded| {
        let (body, prev, next, more) = match cursor_of(r).as_deref() {
            None => (
                format!("[{}]", issue_json("1", "API-1", "a")),
                "1730937600000:0:1",
                "1730937600000:2:0",
                true,
            ),
            Some("1730937600000:2:0") => (
                format!("[{}]", issue_json("2", "API-2", "b")),
                "1730937600000:2:1",
                "1730937600000:4:0",
                true,
            ),
            Some("1730937600000:4:0") => (
                format!("[{}]", issue_json("3", "API-3", "c")),
                "1730937600000:4:1",
                "1730937600000:6:0",
                false,
            ),
            Some(other) => panic!("unexpected cursor {other}"),
        };
        Reply::json(200, body).header("Link", issue_link(r, prev, next, more))
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let organization = api.organization("acme");

    // Single page: next_cursor is the results="true" continuation.
    let first = organization
        .issues()
        .set_query("")
        .send()
        .await
        .expect("first page");
    assert_eq!(first.next_cursor.as_deref(), Some("1730937600000:2:0"));
    assert_eq!(
        first.rates,
        Default::default(),
        "no quota headers: all None"
    );
    let second = organization
        .issues()
        .set_query("")
        .maybe_cursor(first.next_cursor.clone())
        .send()
        .await
        .expect("second page");
    assert_eq!(second.value[0].short_id, "API-2");
    assert_eq!(
        server.requests()[1].query_all("query"),
        [""],
        "empty query lists all statuses"
    );

    // Bounded traversal: one page allowed, more exists.
    let mut capped = organization
        .issues()
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap());
    let page = capped.next_page().await.expect("page").expect("first page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next.as_deref(), Some("1730937600000:2:0"));
    let resume = match capped.next_page().await {
        Err(PagesError::Bounded { resume, .. }) => resume,
        other => panic!(
            "expected Bounded, got {:?}",
            other.map(|p| p.map(|p| p.items.len()))
        ),
    };
    assert_eq!(resume, Resume::At("1730937600000:2:0".to_owned()));
    assert!(
        matches!(capped.next_page().await, Err(PagesError::Ended { .. })),
        "a bound is not the end"
    );
    let requests_before = server.requests().len();

    // Resume later with the same organization, query and page size.
    let mut rest = organization
        .issues()
        .pages(resume, NonZeroUsize::new(5).unwrap());
    let mut ids = Vec::new();
    while let Some(page) = rest.next_page().await.expect("resumed page") {
        ids.extend(page.items.into_iter().map(|i| i.id));
    }
    assert_eq!(ids, ["2", "3"]);
    let resumed = &server.requests()[requests_before];
    assert_eq!(cursor_of(resumed).as_deref(), Some("1730937600000:2:0"));
    assert_eq!(
        resumed.query_all("query"),
        Vec::<String>::new(),
        "omitted query keeps Sentry's default"
    );
    assert_eq!(resumed.query_all("limit"), ["100"]);
    server.shutdown().await;
}

/// Status, provider-document, body and credential failures of the issue calls.
#[tokio::test]
async fn workflow1_failures_are_typed_and_bounded() {
    // Invalid token: nothing is built or sent.
    for bad in ["", "sntryu_abc\r\nX-Injected: 1", "two words"] {
        let err =
            Sentry::with_token(http_client().unwrap(), bad, SendPolicy::default()).unwrap_err();
        assert!(matches!(err, Error::Credential), "{bad:?}: {err:?}");
    }

    // 401: typed problem document, no retry, token withheld from diagnostics.
    let server = TestServer::start(|_: &Recorded| problem(401, "Invalid token")).await;
    let err = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    let Error::Provider(p) = &err else {
        panic!("expected Provider, got {err:?}")
    };
    assert_eq!(p.class, HttpClass::Unauthorized);
    assert_eq!(p.meta.status(), 401);
    assert_eq!(
        p.document.as_ref().expect("typed document").detail,
        "Invalid token"
    );
    assert_eq!(p.retry.retry.attempts, 1);
    assert_eq!(server.requests().len(), 1);
    assert_withheld(&format!("{err} {err:?}"), "401 error");
    eprintln!("observed 401: display={err} debug={err:?}");
    server.shutdown().await;

    // 404 on the chosen issue: single attempt.
    let server =
        TestServer::start(|_: &Recorded| problem(404, "The requested resource does not exist"))
            .await;
    let err = api(&server, SendPolicy::default())
        .organization("acme")
        .issue("999")
        .get()
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Provider(p) if p.class == HttpClass::NotFound && p.retry.retry.attempts == 1),
        "{err:?}"
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // 400 (bad search syntax) and 403 (missing scope): single attempt each.
    for (status, class) in [(400, HttpClass::BadRequest), (403, HttpClass::Forbidden)] {
        let server =
            TestServer::start(move |_: &Recorded| problem(status, "Error parsing search query"))
                .await;
        let err = api(&server, SendPolicy::default())
            .organization("acme")
            .issues()
            .set_query("is:")
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Provider(p) if p.class == class && p.retry.retry.attempts == 1),
            "{status}: {err:?}"
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }

    // A Retry-After beyond the local scheduling deadline is not slept through.
    let server = TestServer::start(|_: &Recorded| {
        problem(429, "Too many requests").header("Retry-After", "5")
    })
    .await;
    let settings = Settings {
        timeout: Duration::from_millis(500),
        ..Settings::default()
    };
    let started = Instant::now();
    let err = api(&server, SendPolicy::new(settings).unwrap())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    let elapsed = started.elapsed();
    eprintln!(
        "observed deadline: elapsed={elapsed:?} requests={} display={err} debug={err:?}",
        server.requests().len()
    );
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    // Observed: RetryStopped (no HttpClass, no problem document), the 429 head retained.
    // StopReason is not re-exported by the SDK, so it is compared through Debug.
    let Error::RetryStopped { report, meta } = &err else {
        panic!("expected RetryStopped, got {err:?}")
    };
    assert_eq!(format!("{:?}", report.retry.stop), "Deadline");
    assert_eq!(report.retry.attempts, 1);
    assert_eq!(meta.as_ref().map(|m| m.status().as_u16()), Some(429));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // 429 with Retry-After: 1, then success: the server minimum is honoured.
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let server = TestServer::start(move |_: &Recorded| {
        if h.fetch_add(1, Ordering::SeqCst) == 0 {
            problem(429, "You are attempting to use this endpoint too frequently. Limit is 40 requests in 1 seconds")
                .header("Retry-After", "1")
                .header("X-Sentry-Rate-Limit-Limit", "40")
                .header("X-Sentry-Rate-Limit-Remaining", "0")
                .header("X-Sentry-Rate-Limit-Reset", epoch_secs().to_string())
        } else {
            Reply::json(200, "[]")
        }
    })
    .await;
    let started = Instant::now();
    let ok = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .send()
        .await
        .expect("retried 429");
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_eq!(ok.retry.retry.attempts, 2);
    assert!(
        ok.retry.retry.waits[0] >= Duration::from_secs(1),
        "{:?}",
        ok.retry.retry.waits
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;

    // 500 is a server error but not retried.
    let server = TestServer::start(|_: &Recorded| problem(500, "Internal Error")).await;
    let err = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Provider(p) if p.class == HttpClass::Server && p.retry.retry.attempts == 1),
        "{err:?}"
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // 502 HTML from a proxy: retried up to three attempts, status survives the unreadable document.
    let server = TestServer::start(|_: &Recorded| {
        Reply::text(502, "text/html", "<html><body>Bad Gateway</body></html>")
    })
    .await;
    let err = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    let Error::Provider(p) = &err else {
        panic!("expected Provider, got {err:?}")
    };
    assert_eq!(
        (p.class, p.meta.status().as_u16()),
        (HttpClass::Server, 502)
    );
    assert!(p.document.is_err());
    assert_eq!(p.retry.retry.attempts, 3);
    assert_eq!(server.requests().len(), 3);
    eprintln!(
        "observed 502: waits={:?} stop={:?} display={err}",
        p.retry.retry.waits, p.retry.retry.stop
    );
    server.shutdown().await;

    // Successful status, bad bodies.
    type ReplyFn = fn() -> Reply;
    let bodies: [(&str, ReplyFn, ErrorKind); 5] = [
        (
            "over 1 MiB",
            || {
                let one = issue_json("4567891230", "BACKEND-7Q", "TypeError: checkout");
                let many = vec![one; 1_100_000 / 500];
                Reply::json(200, format!("[{}]", many.join(",")))
            },
            ErrorKind::Limit,
        ),
        (
            "truncated",
            || Reply::json(200, r#"[{"id":"1","shortId":"#),
            ErrorKind::Decode,
        ),
        (
            "wrong shape",
            || Reply::json(200, r#"{"detail":"not a list"}"#),
            ErrorKind::Decode,
        ),
        ("empty", || Reply::json(200, ""), ErrorKind::Decode),
        (
            "html",
            || Reply::text(200, "text/html", "<html></html>"),
            ErrorKind::Metadata,
        ),
    ];
    for (label, reply, kind) in bodies {
        let server = TestServer::start(move |_: &Recorded| reply()).await;
        let err = api(&server, SendPolicy::default())
            .organization("acme")
            .issues()
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Concord(e) if e.kind() == kind),
            "{label}: {err:?}"
        );
        eprintln!("observed body [{label}]: display={err}");
        assert_eq!(
            server.requests().len(),
            1,
            "{label}: body failures are not replayed"
        );
        server.shutdown().await;
    }

    // Invalid quota metadata on a 2xx.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, "[]").header("X-Sentry-Rate-Limit-Remaining", "plenty")
    })
    .await;
    let err = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::RateMetadata { meta, .. } if meta.status() == 200),
        "{err:?}"
    );
    eprintln!("observed rate metadata: display={err}");
    server.shutdown().await;
}

/// Link semantics that must not be continued silently.
#[tokio::test]
async fn workflow1_invalid_links_are_pagination_errors() {
    type LinkFn = fn(&Recorded) -> String;
    let cases: [(&str, LinkFn); 6] = [
        ("missing results flag", |r| {
            format!(
                "<http://{}{}?&cursor=1730937600000:2:0>; rel=\"next\"; cursor=\"1730937600000:2:0\"",
                r.host,
                r.path()
            )
        }),
        ("invalid results flag", |r| {
            format!(
                "<http://{}{}?&cursor=1730937600000:2:0>; rel=\"next\"; results=\"yes\"; cursor=\"1730937600000:2:0\"",
                r.host,
                r.path()
            )
        }),
        ("other origin", |r| {
            format!(
                "<https://sentry.io{}?&cursor=1730937600000:2:0>; rel=\"next\"; results=\"true\"; cursor=\"1730937600000:2:0\"",
                r.path()
            )
        }),
        ("changed filter", |r| {
            format!(
                "<http://{}{}?query=is%3Aresolved&cursor=1730937600000:2:0>; rel=\"next\"; results=\"true\"; cursor=\"1730937600000:2:0\"",
                r.host,
                r.path()
            )
        }),
        ("conflicting cursors", |r| {
            format!(
                "<http://{}{}?&cursor=1730937600000:2:0>; rel=\"next\"; results=\"true\"; cursor=\"1730937600000:9:0\"",
                r.host,
                r.path()
            )
        }),
        ("two next links", |r| {
            format!(
                "<http://{h}{p}?&cursor=1730937600000:2:0>; rel=\"next\"; results=\"true\"; cursor=\"1730937600000:2:0\", \
                     <http://{h}{p}?&cursor=1730937600000:4:0>; rel=\"next\"; results=\"true\"; cursor=\"1730937600000:4:0\"",
                h = r.host,
                p = r.path()
            )
        }),
    ];
    for (label, link) in cases {
        let server =
            TestServer::start(move |r: &Recorded| Reply::json(200, "[]").header("Link", link(r)))
                .await;
        let err = api(&server, SendPolicy::default())
            .organization("acme")
            .issues()
            .set_query(QUERY)
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Pagination { meta, .. } if meta.status() == 200),
            "{label}: {err:?}"
        );
        if let Error::Pagination { reason, .. } = &err {
            eprintln!("observed pagination [{label}]: reason={reason:?} display={err}");
        }
        assert_withheld(&format!("{err} {err:?}"), label);
        server.shutdown().await;
    }

    // Immediate cursor loop: the next link names the cursor just requested.
    let server = TestServer::start(|r: &Recorded| {
        let c = cursor_of(r).unwrap_or_else(|| "1730937600000:2:0".to_owned());
        Reply::json(200, "[]").header("Link", issue_link(r, "1730937600000:0:1", &c, true))
    })
    .await;
    let mut pages = api(&server, SendPolicy::default())
        .organization("acme")
        .issues()
        .pages(Resume::Start, NonZeroUsize::new(5).unwrap());
    assert!(pages.next_page().await.expect("first page").is_some());
    match pages.next_page().await {
        Err(PagesError::Provider {
            error: Error::Pagination { .. },
            failed,
            ..
        }) => {
            assert_eq!(failed, Resume::At("1730937600000:2:0".to_owned()));
        }
        other => panic!(
            "expected a cursor-loop error, got {:?}",
            other.map(|p| p.map(|p| p.items.len()))
        ),
    }
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

// ================================================================ workflow 2

/// Page an issue's events (one 503 retried inside a page, one empty page that continues),
/// stop at `results="false"` although it carries a cursor, then retrieve one event by eventID.
#[tokio::test]
async fn workflow2_events_follow_link_until_results_false_then_retrieve_event() {
    let retried = Arc::new(AtomicUsize::new(0));
    let r2 = retried.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let events = format!("/api/0/organizations/acme/issues/{ISSUE}/events/");
        if let Some(id) = r
            .path()
            .strip_prefix(events.as_str())
            .filter(|rest| !rest.is_empty())
        {
            let id = id.trim_end_matches('/');
            return if id == "9fac2ceed9344f2bbfdd1fdacb0ed9b1" {
                with_rates(Reply::json(200, event_detail("8812", id)))
            } else {
                problem(404, "Event not found")
            };
        }
        assert_eq!(r.path(), events);
        match cursor_of(r).as_deref() {
            None => Reply::json(
                200,
                format!(
                    "[{},{}]",
                    event_summary(
                        "8810",
                        "1d3cfa6b0d6e4a14b0a7e3c2f5b9e801",
                        "TypeError: first"
                    ),
                    event_summary(
                        "8811",
                        "6c1f0e5bb3e94d8a9a2f7d4c0e8b1a22",
                        "TypeError: second"
                    )
                ),
            )
            .header("Link", event_link(r, "0:0:1", "0:2:0", true)),
            Some("0:2:0") if r2.fetch_add(1, Ordering::SeqCst) == 0 => {
                Reply::text(503, "text/plain", "upstream unavailable")
            }
            // Empty page with results="true": not the end.
            Some("0:2:0") => {
                Reply::json(200, "[]").header("Link", event_link(r, "0:2:1", "0:4:0", true))
            }
            Some("0:4:0") => Reply::json(
                200,
                format!(
                    "[{}]",
                    event_summary(
                        "8812",
                        "9fac2ceed9344f2bbfdd1fdacb0ed9b1",
                        "TypeError: checkout"
                    )
                ),
            )
            .header("Link", event_link(r, "0:4:1", "0:6:0", false)),
            Some(other) => panic!("unexpected cursor {other}"),
        }
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let issue = api.organization("acme").issue(ISSUE);

    let mut pages = issue
        .events()
        .set_per_page(2)
        .pages(Resume::Start, NonZeroUsize::new(10).unwrap());
    let mut events = Vec::new();
    let mut page_sizes = Vec::new();
    while let Some(page) = pages.next_page().await.expect("event page") {
        page_sizes.push(page.items.len());
        events.extend(page.items);
    }
    assert!(pages.next_page().await.expect("sticky end").is_none());
    assert_eq!(page_sizes, [2, 0, 1]);
    assert_eq!(
        pages.progress().succeeded,
        3,
        "logical pages, retries not counted"
    );

    // Wire: 4 requests (one retry); the results=false cursor 0:6:0 is never requested;
    // per_page from the template survives links that omit it.
    let requests = server.requests();
    let cursors: Vec<Option<String>> = requests.iter().map(cursor_of).collect();
    assert_eq!(
        cursors,
        [
            None,
            Some("0:2:0".into()),
            Some("0:2:0".into()),
            Some("0:4:0".into())
        ]
    );
    for r in &requests {
        assert_eq!(r.query_all("per_page"), ["2"]);
        assert_eq!(r.query_all("full"), Vec::<String>::new());
        assert_eq!(
            r.header("authorization"),
            Some(format!("Bearer {TOKEN}").as_str())
        );
    }

    // Retrieve the chosen event by its eventID, not its storage id.
    let chosen = events
        .iter()
        .find(|e| e.title == "TypeError: checkout")
        .expect("chosen event");
    assert_ne!(chosen.id, chosen.event_id);
    assert!(chosen.entries.is_empty(), "summaries omit entries");
    let event = issue
        .event(&chosen.event_id)
        .send()
        .await
        .expect("event detail");
    assert_eq!(event.value.event_id, "9fac2ceed9344f2bbfdd1fdacb0ed9b1");
    assert_eq!(event.value.group_id, ISSUE);
    assert_eq!(event.value.entries.len(), 2);
    assert_eq!(event.value.entries[0]["type"], "exception");
    assert_eq!(event.retry.retry.attempts, 1);
    assert_eq!(event.rates.concurrent_limit, Some(25));
    assert_eq!(
        server.requests().last().unwrap().target,
        format!(
            "/api/0/organizations/acme/issues/{ISSUE}/events/9fac2ceed9344f2bbfdd1fdacb0ed9b1/"
        )
    );
    assert_withheld(&format!("{event:?} {:?}", event.value), "event Debug");

    // Unknown event id: 404, single attempt.
    let before = server.requests().len();
    let err = issue
        .event("00000000000000000000000000000000")
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Provider(p) if p.class == HttpClass::NotFound && p.retry.retry.attempts == 1),
        "{err:?}"
    );
    assert_eq!(server.requests().len(), before + 1);
    server.shutdown().await;
}

/// A failed page keeps the prefix with the caller and names the page to retry.
#[tokio::test]
async fn workflow2_failed_page_keeps_prefix_and_resumes() {
    let failures = Arc::new(AtomicUsize::new(0));
    let f = failures.clone();
    let server = TestServer::start(move |r: &Recorded| match cursor_of(r).as_deref() {
        None => Reply::json(200, format!("[{}]", event_summary("1", "e1", "first")))
            .header("Link", event_link(r, "0:0:1", "0:1:0", true)),
        Some("0:1:0") if f.fetch_add(1, Ordering::SeqCst) == 0 => problem(500, "Internal Error"),
        Some("0:1:0") => Reply::json(200, format!("[{}]", event_summary("2", "e2", "second")))
            .header("Link", event_link(r, "0:1:1", "0:2:0", false)),
        Some(other) => panic!("unexpected cursor {other}"),
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let issue = api.organization("acme").issue(ISSUE);
    let cap = NonZeroUsize::new(10).unwrap();

    let mut collected = Vec::new();
    let mut pages = issue.events().pages(Resume::Start, cap);
    let failed = loop {
        match pages.next_page().await {
            Ok(Some(page)) => collected.extend(page.items),
            Ok(None) => panic!("a 500 is not the end"),
            Err(e) => {
                let (error, failed, progress) = e.into_provider_error().expect("provider failure");
                assert!(
                    matches!(&error, Error::Provider(p) if p.class == HttpClass::Server),
                    "{error:?}"
                );
                assert_eq!(progress.succeeded, 1);
                break failed;
            }
        }
    };
    assert_eq!(collected.len(), 1);
    assert_eq!(failed, Resume::At("0:1:0".to_owned()));

    let mut retry = issue.events().pages(failed, cap);
    while let Some(page) = retry.next_page().await.expect("resumed") {
        collected.extend(page.items);
    }
    let ids: Vec<&str> = collected.iter().map(|e| e.event_id.as_str()).collect();
    assert_eq!(ids, ["e1", "e2"]);
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

/// Timeline of request arrivals at the server and how long each head was held.
#[derive(Clone, Default)]
struct Timeline(Arc<Mutex<Vec<(Instant, Duration, String)>>>);

impl Timeline {
    fn record(&self, r: &Recorded, hold: Duration) -> usize {
        let mut v = self.0.lock().unwrap();
        v.push((Instant::now(), hold, r.path().to_owned()));
        v.len() - 1
    }
    fn get(&self) -> Vec<(Instant, Duration, String)> {
        self.0.lock().unwrap().clone()
    }
}

/// Two callers fetch events concurrently; the first attempt is rate limited (429). Its retry
/// must wait for the endpoint's concurrency lease, which the other caller holds while its
/// response head is delayed 400ms. Control: with two local slots the retry overlaps.
async fn retry_vs_concurrency(
    concurrency: usize,
) -> (Vec<(Instant, Duration, String)>, [usize; 2]) {
    let timeline = Timeline::default();
    let t = timeline.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let id = r
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        let seen = t.0.lock().unwrap().len();
        match seen {
            0 => {
                t.record(r, Duration::ZERO);
                problem(429, "Too many concurrent requests").header("Retry-After", "0")
            }
            1 => {
                t.record(r, Duration::from_millis(400));
                Reply::json(200, event_detail("s", &id)).delay(Duration::from_millis(400))
            }
            _ => {
                t.record(r, Duration::ZERO);
                Reply::json(200, event_detail("s", &id))
            }
        }
    })
    .await;
    let settings = Settings {
        concurrency: NonZeroUsize::new(concurrency).unwrap(),
        ..Settings::default()
    };
    let api = api(&server, SendPolicy::new(settings).expect("settings"));
    let a = {
        let issue = api.organization("acme").issue(ISSUE);
        tokio::spawn(async move { issue.event("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").send().await })
    };
    let b = {
        let issue = api.organization("acme").issue(ISSUE);
        tokio::spawn(async move { issue.event("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").send().await })
    };
    let a = a.await.unwrap().expect("caller A");
    let b = b.await.unwrap().expect("caller B");
    server.shutdown().await;
    (
        timeline.get(),
        [a.retry.retry.attempts, b.retry.retry.attempts],
    )
}

#[tokio::test]
async fn workflow2_retry_waits_for_concurrency_lease() {
    let (events, attempts) = retry_vs_concurrency(1).await;
    eprintln!(
        "observed lease timeline (ms from first): {:?}",
        events
            .iter()
            .map(|e| (e.0 - events[0].0).as_millis())
            .collect::<Vec<_>>()
    );
    assert_eq!(events.len(), 3);
    let mut sorted = attempts;
    sorted.sort();
    assert_eq!(sorted, [1, 2], "one caller retried once");
    assert_eq!(
        events[2].2, events[0].2,
        "the third request is the retry of the first"
    );
    // No overlap at the server: each request arrives after the previous head was released.
    for w in events.windows(2) {
        assert!(
            w[1].0 >= w[0].0 + w[0].1,
            "overlap: {:?} after {:?}",
            w[1].0 - w[0].0,
            w[0].1
        );
    }

    // Control: two local slots let the retry run while the other caller's head is held.
    let (events, _) = retry_vs_concurrency(2).await;
    eprintln!(
        "observed control timeline (ms from first): {:?}",
        events
            .iter()
            .map(|e| (e.0 - events[0].0).as_millis())
            .collect::<Vec<_>>()
    );
    assert_eq!(events.len(), 3);
    assert!(
        events[2].0 < events[1].0 + events[1].1,
        "control should overlap"
    );
}

/// `X-Sentry-Rate-Limit-ConcurrentLimit: 1` caps local concurrency below Settings (4).
async fn reported_limit(header: bool) -> Vec<(Instant, Duration, String)> {
    let timeline = Timeline::default();
    let t = timeline.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let id = r
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        let hold = if id == "warmup" {
            Duration::ZERO
        } else {
            Duration::from_millis(300)
        };
        t.record(r, hold);
        let reply = Reply::json(200, event_detail("s", &id)).delay(hold);
        if header {
            reply
                .header("X-Sentry-Rate-Limit-ConcurrentLimit", "1")
                .header("X-Sentry-Rate-Limit-ConcurrentRemaining", "0")
        } else {
            reply
        }
    })
    .await;
    let settings = Settings {
        concurrency: NonZeroUsize::new(4).unwrap(),
        ..Settings::default()
    };
    let api = api(&server, SendPolicy::new(settings).unwrap());
    let issue = api.organization("acme").issue(ISSUE);
    let warm = issue.event("warmup").send().await.expect("warm-up");
    assert_eq!(warm.rates.concurrent_limit, header.then_some(1));
    let tasks: Vec<_> = ["c1", "c2"]
        .into_iter()
        .map(|id| {
            let issue = issue.clone();
            tokio::spawn(async move { issue.event(id).send().await })
        })
        .collect();
    for t in tasks {
        t.await.unwrap().expect("concurrent read");
    }
    server.shutdown().await;
    timeline.get()
}

#[tokio::test]
async fn workflow2_reported_concurrent_limit_caps_local_concurrency() {
    let capped = reported_limit(true).await;
    eprintln!(
        "observed reported-limit timeline (ms from first): {:?}",
        capped
            .iter()
            .map(|e| (e.0 - capped[0].0).as_millis())
            .collect::<Vec<_>>()
    );
    assert_eq!(capped.len(), 3);
    assert!(
        capped[2].0 >= capped[1].0 + capped[1].1,
        "reported limit 1 must serialize"
    );
    let free = reported_limit(false).await;
    assert!(
        free[2].0 < free[1].0 + free[1].1,
        "control: 4 local slots overlap"
    );
}

/// Frequency is separate from concurrency: Remaining 0 delays the next call on that endpoint
/// until the absolute UTC reset.
#[tokio::test]
async fn workflow2_frequency_remaining_zero_waits_for_reset() {
    let reset = epoch_secs() + 2;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let a = arrivals.clone();
    let server = TestServer::start(move |r: &Recorded| {
        a.lock().unwrap().push(SystemTime::now());
        let id = r
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        Reply::json(200, event_detail("s", &id))
            .header("X-Sentry-Rate-Limit-Limit", "40")
            .header("X-Sentry-Rate-Limit-Remaining", "0")
            .header("X-Sentry-Rate-Limit-Reset", reset.to_string())
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let issue = api.organization("acme").issue(ISSUE);
    let first = issue.event("e1").send().await.expect("first");
    assert_eq!(
        (first.rates.remaining, first.rates.reset),
        (Some(0), Some(reset))
    );
    issue
        .event("e2")
        .send()
        .await
        .expect("second, after the reset");
    let arrivals = arrivals.lock().unwrap().clone();
    let second = arrivals[1].duration_since(UNIX_EPOCH).unwrap();
    eprintln!(
        "observed frequency: second request {second:?}, reset {reset}s, gap {:?}",
        arrivals[1].duration_since(arrivals[0]).unwrap()
    );
    assert!(
        second + Duration::from_millis(50) >= Duration::from_secs(reset),
        "sent {second:?} before reset {reset}"
    );
    server.shutdown().await;
}
