//! Caller workflows using only the public SDK contract, with independently authored fixtures.
//! Existing first_use.rs is unchanged. All traffic is loopback; live calls NOT_RUN.

use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_cloudflare_zones::{
    AccountId, Cloudflare, Error, NewDnsRecord, RecordTtl, RecordType, SendPolicy, ZoneId,
    concord::reqwest, http_client,
};
use serde_json::{Value, json};

const ZONE: &str = "0123456789abcdef0123456789abcdef";
const RECORD: &str = "fedcba9876543210fedcba9876543210";

fn created() -> Value {
    json!({"id": RECORD, "type": "AAAA", "name": "api.example.com",
        "content": "2001:db8::42", "ttl": 300, "proxied": false})
}

fn reply(result: Value) -> Reply {
    Reply::json(
        200,
        json!({"success": true, "errors": [], "messages": [], "result": result}).to_string(),
    )
}

fn new_record() -> NewDnsRecord {
    NewDnsRecord::new(
        RecordType::Aaaa,
        "api.example.com",
        "2001:db8::42",
        RecordTtl::new(300).unwrap(),
        Some(false),
    )
    .unwrap()
}

#[tokio::test]
async fn caller_selects_account_zone_creates_then_fetches_the_created_record() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (request.method.as_str(), request.path()) {
            ("GET", "/client/v4/zones") => Reply::json(200, json!({
                "success": true, "errors": [], "messages": [],
                "result": [{"id": ZONE, "name": "example.com"}],
                "result_info": {"page": 1, "total_pages": 1}
            }).to_string()),
            ("GET", p) if p == format!("/client/v4/zones/{ZONE}") => reply(json!({"id": ZONE, "name": "example.com"})),
            ("POST", p) if p == format!("/client/v4/zones/{ZONE}/dns_records") => {
                assert_eq!(serde_json::from_slice::<Value>(&request.body).unwrap(), json!({
                    "type": "AAAA", "name": "api.example.com", "content": "2001:db8::42", "ttl": 300, "proxied": false
                }));
                reply(created())
            }
            ("GET", p) if p == format!("/client/v4/zones/{ZONE}/dns_records/{RECORD}") => reply(created()),
            _ => Reply::text(500, "text/plain", "Unexpected fixture request"),
        }
    }).await;
    let cf = Cloudflare::connect(http_client().unwrap(), "create-consumer-fixture-token")
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    let zones = cf
        .zones()
        .list()
        .set_account_id(AccountId::from("fixture-account"))
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let zone = cf
        .zone(zones[0].id.clone())
        .get()
        .send()
        .await
        .unwrap()
        .result;
    let created = cf
        .zone(zone.id.clone())
        .dns_records()
        .create(new_record())
        .send()
        .await
        .unwrap()
        .result;
    assert_eq!(created.id.as_str(), RECORD);
    let fetched = cf
        .zone(zone.id)
        .dns_record(created.id.clone())
        .get()
        .send()
        .await
        .unwrap()
        .result;
    assert_eq!(fetched, created);
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        ["GET", "GET", "POST", "GET"]
    );
    assert_eq!(requests[0].query_all("account.id"), ["fixture-account"]);
    server.shutdown().await;
}

#[tokio::test]
async fn caller_handles_record_exists_as_a_provider_refusal() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(
            200,
            json!({"success": false, "errors": [
            {"code": 81057, "message": "The record already exists."}
        ], "messages": [], "result": null})
            .to_string(),
        )
    })
    .await;
    let cf = Cloudflare::connect(http_client().unwrap(), "create-consumer-fixture-token")
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    match cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .create(new_record())
        .send()
        .await
    {
        Err(Error::Provider(refusal)) => {
            assert_eq!(refusal.status(), 200);
            assert_eq!(refusal.errors()[0].message, "The record already exists.");
            assert!(refusal.has_code(81057));
        }
        other => panic!("expected a typed provider refusal: {other:?}"),
    }
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn caller_reconciles_a_committed_but_timed_out_create_by_listing_every_page() {
    let committed = Arc::new(AtomicBool::new(false));
    let state = committed.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        if request.method == "POST" {
            assert!(
                !state.swap(true, Ordering::SeqCst),
                "create must never be replayed"
            );
            return reply(created()).delay(Duration::from_secs(30));
        }
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.path(),
            format!("/client/v4/zones/{ZONE}/dns_records")
        );
        assert!(state.load(Ordering::SeqCst));
        let page = request.query_all("page")[0].parse::<u32>().unwrap();
        assert!((1..=2).contains(&page));
        Reply::json(
            200,
            json!({"success": true, "errors": [], "messages": [],
                "result": if page == 1 { json!([]) } else { json!([created()]) },
                "result_info": {"page": page, "total_pages": 2}
            })
            .to_string(),
        )
    })
    .await;
    let http = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();
    let cf = Cloudflare::connect(http, "create-consumer-fixture-token")
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
        .set_policy(
            SendPolicy::default()
                .retry_rate_limited(NonZeroUsize::new(3).unwrap(), Duration::from_secs(1)),
        );
    let error = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .create(new_record())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::CreateUncertain(_)), "{error:?}");
    let records = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .list()
        .set_record_type(RecordType::Aaaa)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let matching: Vec<_> = records
        .iter()
        .filter(|record| {
            record.name == "api.example.com"
                && record.record_type == RecordType::Aaaa
                && record.content.as_deref() == Some("2001:db8::42")
        })
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].id.as_str(), RECORD);
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "GET", "GET"]
    );
    server.shutdown().await;
}
