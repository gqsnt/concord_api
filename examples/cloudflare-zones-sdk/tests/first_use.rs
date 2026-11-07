//! Independent caller tasks, written from README and public rustdoc only.
//! Every response is synthetic; no live Cloudflare request is permitted.

use std::{collections::VecDeque, num::NonZeroUsize, sync::Mutex, time::Duration};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_cloudflare_zones::{
    AccountId, Cloudflare, EnvelopeProblem, Error, RecordId, RecordType, RecordsPerPage,
    ResponseMeta, RetryAfter, ZoneId, ZoneStatus, ZonesPerPage,
    concord::ErrorKind,
    concord_pagination::{Outcome, PagesError},
    http_client,
};
use serde_json::{Value, json};

const ACCOUNT: &str = "023e105f4ecef8ad9ca31a8372d0c353";
const ZONE: &str = "023e105f4ecef8ad9ca31a8372d0c350";
const RECORD: &str = "372e67954025e0ba6aaa6d586b9e0b59";
const TOKEN: &str = "first-use-fixture-token-KEEP-PRIVATE";
const COOKIE: &str = "session=first-use-cookie-KEEP-PRIVATE";

fn zone(index: u8) -> Value {
    json!({
        "id": format!("023e105f4ecef8ad9ca31a8372d0c35{index}"),
        "name": if index == 0 { "example.com".to_owned() } else { format!("site{index}.example") },
        "status": "active", "type": "full", "paused": false,
        "account": {"id": ACCOUNT, "name": "Example Infrastructure"},
        "name_servers": ["ada.ns.cloudflare.com", "bob.ns.cloudflare.com"],
        "development_mode": 0, "created_on": "2024-01-01T05:20:00Z"
    })
}

fn record(index: u8) -> Value {
    json!({
        "id": if index == 0 { RECORD.to_owned() } else { format!("372e67954025e0ba6aaa6d586b9e0b5{index}") },
        "name": if index == 0 { "www.example.com" } else { "api.example.com" },
        "type": "A", "content": if index == 0 { "192.0.2.10" } else { "192.0.2.11" },
        "ttl": 1, "proxied": true, "proxiable": true,
        "comment": "Synthetic first-use fixture", "tags": ["owner:platform"],
        "created_on": "2024-01-01T05:20:00.12345Z"
    })
}

fn success(result: Value) -> Value {
    json!({"success": true, "result": result, "errors": [], "messages": []})
}

fn page(items: Vec<Value>, number: u32, total_pages: u32, per_page: u32) -> Value {
    let count = items.len();
    let mut body = success(json!(items));
    // Deliberately unrelated to the filtered result count: never an end condition.
    body["result_info"] = json!({
        "page": number, "per_page": per_page, "count": count,
        "total_count": 2000, "total_pages": total_pages
    });
    body
}

fn refusal() -> Value {
    json!({
        "success": false,
        // A refused list must not attempt to decode this as a Vec.
        "result": {"unexpected": "shape"},
        "errors": [
            {"code": 10000, "message": "Authentication error"},
            {"code": 9109, "message": "Insufficient permission"}
        ],
        "messages": [{"code": 1001, "message": "Check the token's zone scope"}]
    })
}

fn reply(status: u16, body: Value) -> Reply {
    Reply::json(status, body.to_string())
        .header("Ratelimit", "\"global\";r=1198;t=300, \"dns\";r=0;t=15")
        .header(
            "Ratelimit-Policy",
            "\"global\";q=1200;w=300, \"dns\";q=100;w=60",
        )
        .header("CF-Ray", "8dfeed1234567890-CDG")
        .header("Set-Cookie", COOKIE)
}

async fn fixture(replies: Vec<Reply>) -> (TestServer, Cloudflare) {
    let replies = Mutex::new(VecDeque::from(replies));
    let server = TestServer::start(move |_request: &Recorded| -> Reply {
        replies.lock().unwrap().pop_front().unwrap_or_else(|| {
            Reply::json(500, r#"{"success":false,"errors":[{"code":1000,"message":"unexpected extra request"}]}"#)
        })
    })
    .await;
    let cf = Cloudflare::connect(http_client().unwrap(), TOKEN)
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    (server, cf)
}

fn zones_query(page: u32) -> String {
    format!("/client/v4/zones?account.id={ACCOUNT}&page={page}&per_page=5")
}

fn records_query(page: u32) -> String {
    format!("/client/v4/zones/{ZONE}/dns_records?type=A&page={page}&per_page=1")
}

fn assert_requests(server: &TestServer, expected: &[String]) {
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        expected.len(),
        "no hidden retry or page request"
    );
    for (request, target) in requests.iter().zip(expected) {
        assert_eq!(request.method, "GET");
        assert_eq!(request.host, server.addr().to_string());
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        assert_eq!(request.path(), path);
        let mut actual = request.query_pairs();
        let mut wanted: Vec<_> = query
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (key, value) = part.split_once('=').unwrap();
                (key.to_owned(), value.to_owned())
            })
            .collect();
        actual.sort();
        wanted.sort();
        assert_eq!(actual, wanted);
        assert_eq!(
            request.header_all("authorization"),
            [format!("Bearer {TOKEN}")]
        );
        assert_eq!(request.header("accept"), Some("application/json"));
        assert_eq!(request.header("x-auth-email"), None);
        assert_eq!(request.header("x-auth-key"), None);
        assert!(request.body.is_empty());
        assert_eq!(request.body_len, 0);
        assert!(!request.target.contains(TOKEN));
    }
}

fn assert_meta(meta: &ResponseMeta, status: u16, operation: &str) {
    assert_eq!(meta.status().as_u16(), status);
    let identity = meta.operation().unwrap();
    assert_eq!(identity.name(), operation);
    assert_eq!(identity.method(), Some("GET"));
    assert_eq!(
        meta.headers().get("cf-ray").unwrap(),
        "8dfeed1234567890-CDG"
    );
    let limits = meta.rate_limits().unwrap();
    assert_eq!(
        limits
            .limits
            .iter()
            .map(|limit| limit.name.as_str())
            .collect::<Vec<_>>(),
        ["global", "dns"]
    );
    assert_eq!(limits.limit("global").unwrap().remaining, Some(1198));
    assert_eq!(
        limits.limit("dns").unwrap().reset,
        Some(Duration::from_secs(15))
    );
    assert_eq!(
        limits
            .exhausted()
            .map(|limit| limit.name.as_str())
            .collect::<Vec<_>>(),
        ["dns"]
    );
    assert_eq!(limits.policies.len(), 2);
    assert_eq!(limits.policies[0].name, "global");
    assert_eq!(limits.policies[0].quota, Some(1200));
    assert_eq!(limits.policies[0].window, Some(Duration::from_secs(300)));
    assert_eq!(limits.policies[1].name, "dns");
    assert_eq!(limits.policies[1].quota, Some(100));
    assert_eq!(limits.policies[1].window, Some(Duration::from_secs(60)));
    assert!(!format!("{meta:?}").contains(COOKIE));
}

fn assert_provider(error: &Error, status: u16, operation: &str) {
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    assert_eq!(error.status().unwrap().as_u16(), status);
    let provider = error.provider().unwrap();
    assert_eq!(
        provider
            .errors()
            .iter()
            .map(|info| info.code)
            .collect::<Vec<_>>(),
        [10000, 9109]
    );
    assert_eq!(provider.errors()[0].message, "Authentication error");
    assert_eq!(provider.messages()[0].code, 1001);
    assert_eq!(
        provider.messages()[0].message,
        "Check the token's zone scope"
    );
    assert!(provider.document().is_ok());
    assert_eq!(provider.attempts(), 1);
    assert_meta(provider.meta(), status, operation);
    assert!(!format!("{error:?} {error}").contains(TOKEN));
}

// Each caller also handles detail failures through its normal `.get().send()` path.
async fn detail_failures(dns: bool) {
    enum Expected {
        Provider(u16),
        Unreadable,
        RateLimited,
        Concord(ErrorKind),
        MissingResult,
    }
    let cases = [
        (reply(200, refusal()), Expected::Provider(200)),
        (reply(403, refusal()), Expected::Provider(403)),
        (
            Reply::text(503, "text/html", "<html>upstream unavailable</html>"),
            Expected::Unreadable,
        ),
        (
            reply(429, refusal()).header("Retry-After", "3"),
            Expected::RateLimited,
        ),
        (Reply::json(200, "{"), Expected::Concord(ErrorKind::Decode)),
        (
            Reply::text(200, "text/html", "<html>not an API envelope</html>"),
            Expected::Concord(ErrorKind::Metadata),
        ),
        (
            reply(200, json!({"success": true})),
            Expected::MissingResult,
        ),
        (
            reply(200, success(json!({"id": 42}))),
            Expected::Concord(ErrorKind::Decode),
        ),
    ];
    let (replies, expectations): (Vec<_>, Vec<_>) = cases.into_iter().unzip();
    let (server, cf) = fixture(replies).await;
    let operation = if dns {
        "Cloudflare.zone.dns_record.get"
    } else {
        "Cloudflare.zone.get"
    };
    let path = if dns {
        format!("/client/v4/zones/{ZONE}/dns_records/{RECORD}")
    } else {
        format!("/client/v4/zones/{ZONE}")
    };
    let count = expectations.len();
    for expected in expectations {
        let result = if dns {
            cf.zone(ZoneId::from(ZONE))
                .dns_record(RecordId::from(RECORD))
                .get()
                .send()
                .await
                .map(|_| ())
        } else {
            cf.zone(ZoneId::from(ZONE)).get().send().await.map(|_| ())
        };
        let error = result.unwrap_err();
        assert!(!format!("{error:?} {error}").contains(TOKEN));
        match expected {
            Expected::Provider(status) => assert_provider(&error, status, operation),
            Expected::Unreadable => {
                assert!(matches!(error, Error::Provider(_)));
                assert_eq!(error.status().unwrap().as_u16(), 503);
                assert_eq!(
                    error.provider().unwrap().document().unwrap_err().kind(),
                    ErrorKind::Metadata
                );
            }
            Expected::RateLimited => {
                assert!(matches!(error, Error::RateLimited(_)));
                assert_eq!(error.status().unwrap().as_u16(), 429);
                let provider = error.provider().unwrap();
                assert_eq!(
                    provider.retry_after(),
                    RetryAfter::After(Duration::from_secs(3))
                );
                assert_eq!(provider.attempts(), 1);
                assert_eq!(provider.errors()[0].code, 10000);
                assert_meta(provider.meta(), 429, operation);
            }
            Expected::Concord(kind) => {
                assert_eq!(error.status().unwrap().as_u16(), 200);
                assert_eq!(error.concord_kind(), Some(kind));
            }
            Expected::MissingResult => {
                assert_eq!(error.status().unwrap().as_u16(), 200);
                let Error::Envelope(envelope) = error else {
                    panic!("expected envelope failure")
                };
                assert_eq!(envelope.problem(), EnvelopeProblem::MissingResult);
                assert_meta(envelope.meta(), 200, operation);
            }
        }
    }
    assert_requests(&server, &vec![path; count]);
    server.shutdown().await;
}

#[tokio::test]
async fn account_zones_then_selected_zone() {
    let first = page((0..5).map(zone).collect(), 1, 2, 5);
    let second = page(vec![zone(5)], 2, 2, 5);
    let mut detail = success(zone(0));
    // The success flag remains authoritative even with a non-fatal errors entry.
    detail["errors"] = json!([{"code": 1000, "message": "Synthetic advisory"}]);
    let (server, cf) = fixture(vec![
        reply(200, first),
        reply(200, second),
        reply(200, detail),
    ])
    .await;
    let listing = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(ZonesPerPage::new(5).unwrap());
    for debug in [
        format!("{cf:?}"),
        format!("{:?}", cf.zones()),
        format!("{listing:?}"),
    ] {
        assert!(!debug.contains(TOKEN));
    }
    let mut pages = listing.pages();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 5);
    assert_eq!(first.next.unwrap().get(), 2);
    assert_eq!(first.meta.page.get(), 1);
    assert_meta(&first.meta.meta, 200, "Cloudflare.zones.list");
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next.is_none());
    assert_eq!(second.meta.result_info.total_count, Some(2000));
    assert_meta(&second.meta.meta, 200, "Cloudflare.zones.list");
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 2);
    assert_eq!(report.progress.succeeded, 2);
    assert_eq!(report.progress.failed, 0);
    assert!(report.resume.is_none());

    let selected = &first.items[0];
    let detail = cf.zone(selected.id.clone()).get().send().await.unwrap();
    assert_eq!(detail.result.id, selected.id);
    assert_eq!(detail.result.name, "example.com");
    assert_eq!(detail.result.status, Some(ZoneStatus::Active));
    assert_eq!(detail.result.account.id, Some(AccountId::from(ACCOUNT)));
    assert_eq!(detail.errors[0].code, 1000);
    assert_meta(&detail.meta, 200, "Cloudflare.zone.get");
    assert_requests(
        &server,
        &[
            zones_query(1),
            zones_query(2),
            format!("/client/v4/zones/{ZONE}"),
        ],
    );
    server.shutdown().await;

    // A failed page preserves the complete earlier page and supports explicit recovery.
    let (server, cf) = fixture(vec![
        reply(200, page((0..5).map(zone).collect(), 1, 2, 5)),
        reply(200, refusal()),
        reply(200, page(vec![zone(5)], 2, 2, 5)),
    ])
    .await;
    let listing = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(ZonesPerPage::new(5).unwrap());
    let incomplete = listing
        .clone()
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete
            .collected
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        [
            "example.com",
            "site1.example",
            "site2.example",
            "site3.example",
            "site4.example"
        ]
    );
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = incomplete.error
    else {
        panic!("a refused page must not complete successfully")
    };
    assert_provider(&error, 200, "Cloudflare.zones.list");
    assert_eq!(failed.key().unwrap().get(), 2);
    assert_eq!(progress.admitted, 2);
    assert_eq!(progress.succeeded, 1);
    assert_eq!(progress.failed, 1);
    let recovered = listing
        .pages_from(failed)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].name, "site5.example");
    assert_requests(&server, &[zones_query(1), zones_query(2), zones_query(2)]);
    server.shutdown().await;

    // A safety cap is explicitly incomplete, and never requests the next page.
    let (server, cf) = fixture(vec![reply(200, page((0..5).map(zone).collect(), 1, 2, 5))]).await;
    let incomplete = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(ZonesPerPage::new(5).unwrap())
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 5);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(incomplete.error.resume().unwrap().key().unwrap().get(), 2);
    assert_requests(&server, &[zones_query(1)]);
    server.shutdown().await;

    // The page envelope is optional for a direct send, but required for a complete walk.
    let (server, cf) = fixture(vec![
        reply(200, success(json!([]))),
        reply(200, success(json!([]))),
        reply(200, page(vec![zone(0)], 9, 9, 5)),
    ])
    .await;
    let listing = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(ZonesPerPage::new(5).unwrap());
    let direct = listing.clone().send().await.unwrap();
    assert!(direct.result.is_empty());
    assert!(direct.result_info.is_none());
    assert_meta(&direct.meta, 200, "Cloudflare.zones.list");
    let mut walk = listing.clone().pages();
    let error = walk.next_page().await.unwrap_err();
    let Some(Error::Envelope(envelope)) = error.provider_error() else {
        panic!("missing page info")
    };
    assert_eq!(envelope.problem(), EnvelopeProblem::MissingPageInfo);
    assert!(matches!(
        walk.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(walk.finish().outcome, Outcome::Failed);
    let incomplete = listing.pages().collect_complete().await.unwrap_err();
    assert!(incomplete.collected.is_empty());
    let Some(Error::Envelope(envelope)) = incomplete.error.provider_error() else {
        panic!("wrong page")
    };
    assert!(
        matches!(envelope.problem(), EnvelopeProblem::PageMismatch { requested, returned: 9 } if requested.get() == 1)
    );
    assert_requests(
        &server,
        &[
            format!("/client/v4/zones?account.id={ACCOUNT}&per_page=5"),
            zones_query(1),
            zones_query(1),
        ],
    );
    server.shutdown().await;

    // Credential validation and path construction fail before any request is sent.
    let (server, cf) = fixture(vec![]).await;
    for token in ["", "contains a space", "contains\na-newline"] {
        let error = Cloudflare::connect(http_client().unwrap(), token).unwrap_err();
        assert!(matches!(error, Error::Credential(_)));
        assert!(error.status().is_none());
        if !token.is_empty() {
            assert!(!format!("{error:?} {error}").contains(token));
        }
    }
    let error = cf.zone(ZoneId::from("..")).get().send().await.unwrap_err();
    assert_eq!(error.concord_kind(), Some(ErrorKind::Build));
    assert!(error.status().is_none());
    assert!(!format!("{error:?} {error}").contains(TOKEN));
    assert_requests(&server, &[]);
    server.shutdown().await;
    detail_failures(false).await;
}

#[tokio::test]
async fn selected_zone_records_then_selected_record() {
    let mut first = page(vec![record(0)], 1, 3, 1);
    first["messages"] = json!([{"code": 1001, "message": "Synthetic page notice"}]);
    let (server, cf) = fixture(vec![
        reply(200, success(zone(0))),
        reply(200, first),
        reply(200, page(vec![], 2, 3, 1)),
        reply(200, page(vec![record(1)], 3, 3, 1)),
        reply(200, success(record(0))),
    ])
    .await;
    let selected_zone = cf
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap()
        .result;
    let scope = cf.zone(selected_zone.id.clone());
    let listing = scope
        .dns_records()
        .list()
        .set_record_type(RecordType::A)
        .set_per_page(RecordsPerPage::new(1).unwrap());
    for debug in [format!("{scope:?}"), format!("{listing:?}")] {
        assert!(!debug.contains(TOKEN));
    }
    // Collect pages here because this caller needs each response's metadata.
    let pages = listing.pages().collect_complete().await.unwrap();
    assert_eq!(pages.len(), 3);
    assert!(
        pages[1].items.is_empty(),
        "an empty intermediate page must not finish the walk"
    );
    assert_eq!(pages[0].meta.messages[0].message, "Synthetic page notice");
    for (index, page) in pages.iter().enumerate() {
        assert_eq!(page.meta.page.get() as usize, index + 1);
        assert_eq!(page.meta.total_pages, 3);
        assert_meta(&page.meta.meta, 200, "Cloudflare.zone.dns_records.list");
    }
    let records: Vec<_> = pages.into_iter().flat_map(|page| page.items).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].record_type, RecordType::A);
    assert_eq!(records[1].content.as_deref(), Some("192.0.2.11"));
    let endpoint = cf
        .zone(selected_zone.id)
        .dns_record(records[0].id.clone())
        .get();
    assert!(!format!("{endpoint:?}").contains(TOKEN));
    let detail = endpoint.send().await.unwrap();
    assert_eq!(detail.result.id, records[0].id);
    assert_eq!(detail.result.name, "www.example.com");
    assert_eq!(detail.result.content.as_deref(), Some("192.0.2.10"));
    assert!(detail.result.ttl_is_automatic());
    assert_eq!(detail.result.proxied, Some(true));
    assert_eq!(detail.result.tags, ["owner:platform"]);
    assert_eq!(
        detail.result.created_on.as_deref(),
        Some("2024-01-01T05:20:00.12345Z")
    );
    assert_meta(&detail.meta, 200, "Cloudflare.zone.dns_record.get");
    assert_requests(
        &server,
        &[
            format!("/client/v4/zones/{ZONE}"),
            records_query(1),
            records_query(2),
            records_query(3),
            format!("/client/v4/zones/{ZONE}/dns_records/{RECORD}"),
        ],
    );
    server.shutdown().await;

    let (server, cf) = fixture(vec![
        reply(200, page(vec![record(0)], 1, 2, 1)),
        reply(200, refusal()),
        reply(200, page(vec![record(1)], 2, 2, 1)),
    ])
    .await;
    let listing = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .list()
        .set_record_type(RecordType::A)
        .set_per_page(RecordsPerPage::new(1).unwrap());
    let incomplete = listing
        .clone()
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, RecordId::from(RECORD));
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = incomplete.error
    else {
        panic!("DNS refusal must retain the prefix and error")
    };
    assert_provider(&error, 200, "Cloudflare.zone.dns_records.list");
    assert_eq!(failed.key().unwrap().get(), 2);
    assert_eq!(progress.succeeded, 1);
    assert_eq!(progress.failed, 1);
    let recovered = listing
        .pages_from(failed)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].content.as_deref(), Some("192.0.2.11"));
    assert_requests(
        &server,
        &[records_query(1), records_query(2), records_query(2)],
    );
    server.shutdown().await;

    // A direct send exposes its endpoint-specific page envelope. Bad limit syntax is
    // an observability error only, and does not discard an otherwise useful result.
    let (server, cf) = fixture(vec![
        Reply::json(200, page(vec![record(0)], 1, 1, 1).to_string())
            .header("Ratelimit", "synthetic invalid header"),
    ])
    .await;
    let direct = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .list()
        .set_record_type(RecordType::A)
        .set_per_page(RecordsPerPage::new(1).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(direct.result.len(), 1);
    assert_eq!(direct.result_info.unwrap().total_pages, Some(1));
    assert!(direct.meta.rate_limits().is_err());
    assert_requests(
        &server,
        &[format!(
            "/client/v4/zones/{ZONE}/dns_records?type=A&per_page=1"
        )],
    );
    server.shutdown().await;
    detail_failures(true).await;
}
