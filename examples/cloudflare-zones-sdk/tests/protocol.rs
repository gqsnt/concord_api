//! Wire-level behaviour against a loopback server: request shapes, every error class, malformed
//! and oversized bodies, the 429 policy and redaction. Fixtures: see `PROVENANCE.md`.

mod support;

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_cloudflare_zones::{
    AccountId, Cloudflare, Direction, EnvelopeProblem, Error, RateLimitHeaderError, RecordId,
    RecordOrder, RecordType, RecordsPerPage, RetryAfter, SendPolicy, ZoneId, ZoneOrder, ZoneStatus,
    ZoneType, ZonesPerPage, concord,
};
use support::{ACCOUNT, RECORD, TOKEN, ZONE, client, client_with, fixture, json, ok};

fn page(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Request shapes

#[tokio::test]
async fn list_zones_request_shape() {
    let server = TestServer::start(|_| ok("zones_page1.json")).await;
    let cf = client(&server);
    let response = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_name("contains:example")
        .set_status(ZoneStatus::Active)
        .set_order(ZoneOrder::AccountId)
        .set_direction(Direction::Desc)
        .set_page(page(2))
        .set_per_page(ZonesPerPage::MAX)
        .send()
        .await
        .unwrap();
    assert_eq!(response.result.len(), 5);

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        format!(
            "/client/v4/zones?account.id={ACCOUNT}&name=contains%3Aexample&status=active\
             &order=account.id&direction=desc&page=2&per_page=50"
        )
    );
    assert_eq!(
        request.header("authorization"),
        Some(&*format!("Bearer {TOKEN}"))
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), None);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn optional_filters_are_absent_until_set() {
    let server = TestServer::start(|_| ok("zones_page1.json")).await;
    client(&server).zones().list().send().await.unwrap();
    assert_eq!(server.single().target, "/client/v4/zones");
}

#[tokio::test]
async fn zone_details_request_shape_and_path_encoding() {
    let server = TestServer::start(|_| ok("zone_details.json")).await;
    let cf = client(&server);
    cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap();
    // An identifier is one path segment: `/` and `?` are data, not structure.
    cf.zone(ZoneId::from("a/b?c")).get().send().await.unwrap();
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            format!("/client/v4/zones/{ZONE}"),
            "/client/v4/zones/a%2Fb%3Fc".to_owned()
        ]
    );
    // An identifier that cannot be a segment is refused before anything is sent.
    let error = cf.zone(ZoneId::from("..")).get().send().await.unwrap_err();
    assert_eq!(error.concord_kind(), Some(ErrorKind::Build));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn dns_records_request_shapes() {
    let server = TestServer::start(|r| {
        if r.path().ends_with("/dns_records") {
            ok("dns_records_page1.json")
        } else {
            ok("dns_record_details.json")
        }
    })
    .await;
    let zone = client(&server).zone(ZoneId::from(ZONE));
    zone.dns_records()
        .list()
        .set_record_type(RecordType::Aaaa)
        .set_proxied(false)
        .set_order(RecordOrder::Ttl)
        .set_direction(Direction::Asc)
        .set_page(page(1))
        .set_per_page(RecordsPerPage::new(100).unwrap())
        .send()
        .await
        .unwrap();
    zone.dns_record(RecordId::from(RECORD))
        .get()
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        format!(
            "/client/v4/zones/{ZONE}/dns_records?type=AAAA&proxied=false&order=ttl\
             &direction=asc&page=1&per_page=100"
        )
    );
    assert_eq!(
        requests[1].target,
        format!("/client/v4/zones/{ZONE}/dns_records/{RECORD}")
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.header("authorization"),
            Some(&*format!("Bearer {TOKEN}"))
        );
    }
}

#[test]
fn page_sizes_follow_each_endpoint() {
    assert_eq!(ZonesPerPage::new(4), None);
    assert_eq!(ZonesPerPage::new(5).map(ZonesPerPage::get), Some(5));
    assert_eq!(ZonesPerPage::new(50), Some(ZonesPerPage::MAX));
    assert_eq!(ZonesPerPage::new(51), None);
    assert_eq!(RecordsPerPage::new(0), None);
    assert_eq!(RecordsPerPage::new(1).map(RecordsPerPage::get), Some(1));
    assert_eq!(RecordsPerPage::new(5_000_000), Some(RecordsPerPage::MAX));
    assert_eq!(RecordsPerPage::new(5_000_001), None);
}

#[test]
fn header_names_are_valid() {
    Cloudflare::validate_header_names().unwrap();
}

#[tokio::test]
async fn an_invalid_token_is_refused_before_sending() {
    let server = TestServer::start(|_| ok("zone_details.json")).await;
    for bad in ["", "two words", "line\nbreak", "caf\u{e9}"] {
        let error = Cloudflare::connect(concord::reqwest::Client::new(), bad).unwrap_err();
        assert!(matches!(error, Error::Credential(_)), "{bad:?}");
        assert!(!error.to_string().contains(bad) || bad.is_empty());
    }
    assert!(server.requests().is_empty());
}

// ---------------------------------------------------------------------------------------------
// success: false inside HTTP 2xx

#[tokio::test]
async fn success_false_on_2xx_is_a_provider_error_with_its_errors() {
    let server = TestServer::start(|_| ok("failure_success_false.json")).await;
    let cf = client(&server);
    for error in [
        cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err(),
        cf.zones().list().send().await.unwrap_err(),
        cf.zone(ZoneId::from(ZONE))
            .dns_records()
            .list()
            .send()
            .await
            .unwrap_err(),
        cf.zone(ZoneId::from(ZONE))
            .dns_record(RecordId::from(RECORD))
            .get()
            .send()
            .await
            .unwrap_err(),
    ] {
        let Error::Provider(provider) = &error else {
            panic!("expected a provider error, got {error:?}");
        };
        assert_eq!(provider.status(), 200);
        let codes: Vec<_> = provider.errors().iter().map(|e| e.code).collect();
        assert_eq!(codes, [1001, 1002]);
        assert_eq!(
            provider.errors()[1]
                .source
                .as_ref()
                .unwrap()
                .pointer
                .as_deref(),
            Some("/zone_id")
        );
        assert_eq!(provider.messages()[0].code, 1100);
        assert!(provider.has_code(1002));
        assert_eq!(provider.attempts(), 1);
        let shown = error.to_string();
        assert!(
            shown.contains("HTTP 200") && shown.contains("success=false"),
            "{shown}"
        );
        assert!(shown.contains("[1001, 1002]"), "{shown}");
        // Display carries codes, not provider text.
        assert!(!shown.contains("synthetic refusal"), "{shown}");
    }
}

#[tokio::test]
async fn success_false_wins_over_an_unexpected_result_shape() {
    let server = TestServer::start(|_| ok("failure_unexpected_result.json")).await;
    let cf = client(&server);
    let list = cf.zones().list().send().await.unwrap_err();
    let get = cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err();
    for error in [list, get] {
        let provider = error.provider().expect("a provider error");
        assert_eq!(provider.document().unwrap().codes(), [1003]);
    }
}

#[tokio::test]
async fn the_raw_envelope_keeps_the_failure_for_native_callers() {
    use concord_sdk_cloudflare_zones::wire::Single;
    let server = TestServer::start(|_| ok("failure_success_false.json")).await;
    // `prepare()?.send()` is Concord's own handling: the declared wire envelope, unpoliced.
    let envelope = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap();
    let Single::Failure(document) = envelope else {
        panic!("expected the failure envelope");
    };
    assert_eq!(document.codes(), [1001, 1002]);
}

#[tokio::test]
async fn success_without_result_is_not_an_empty_result() {
    let server = TestServer::start(|_| ok("success_without_result.json")).await;
    let cf = client(&server);
    for error in [
        cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err(),
        cf.zones().list().send().await.unwrap_err(),
    ] {
        let Error::Envelope(envelope) = &error else {
            panic!("expected an envelope error, got {error:?}");
        };
        assert_eq!(envelope.problem(), EnvelopeProblem::MissingResult);
        assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    }
}

#[tokio::test]
async fn success_is_authoritative_over_errors_beside_it() {
    // The documented 200 examples carry `errors` beside `success: true`.
    let server = TestServer::start(|_| ok("zone_details.json")).await;
    let response = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(response.result.name, "zeta.example.com");
    assert_eq!(response.errors[0].code, 1000);
    assert_eq!(response.messages[0].code, 1000);
    assert_eq!(response.meta.status(), 200);
}

#[tokio::test]
async fn undocumented_values_are_kept_not_refused() {
    let server = TestServer::start(|_| ok("undocumented_values.json")).await;
    let zone = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap()
        .result;
    assert_eq!(
        zone.status,
        Some(ZoneStatus::Other("synthetic-status".into()))
    );
    assert_eq!(
        zone.zone_type,
        Some(ZoneType::Other("synthetic-type".into()))
    );
    assert_eq!(zone.account.id, None);
    assert!(zone.name_servers.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Non-2xx statuses

#[tokio::test]
async fn a_non_2xx_envelope_is_a_provider_error() {
    let server = TestServer::start(|_| json(403, "error_403.json")).await;
    let error = client(&server).zones().list().send().await.unwrap_err();
    let Error::Provider(provider) = &error else {
        panic!("expected a provider error, got {error:?}");
    };
    assert_eq!(provider.status(), 403);
    assert_eq!(provider.document().unwrap().codes(), [1004]);
    assert!(error.to_string().contains("HTTP 403"));
    assert!(!error.to_string().contains("success=false"));
}

#[tokio::test]
async fn an_unreadable_error_body_keeps_the_status() {
    let server = TestServer::start(|r| match r.path() {
        "/client/v4/zones" => Reply::text(502, "text/html", "<html>Bad gateway</html>"),
        p if p.ends_with("/empty") => Reply::new(500),
        _ => Reply::json(400, "{\"success\": false, \"errors\": ["),
    })
    .await;
    let cf = client(&server);

    let error = cf.zones().list().send().await.unwrap_err();
    let provider = error.provider().unwrap();
    assert_eq!(provider.status(), 502);
    assert_eq!(provider.document().unwrap_err().kind(), ErrorKind::Metadata);
    assert!(provider.errors().is_empty());
    assert!(error.to_string().contains("error document unreadable"));

    let error = cf
        .zone(ZoneId::from("empty"))
        .get()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.provider().unwrap().status(), 500);
    assert_eq!(
        error.provider().unwrap().document().unwrap_err().kind(),
        ErrorKind::Metadata
    );

    let error = cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err();
    assert_eq!(error.provider().unwrap().status(), 400);
    assert_eq!(
        error.provider().unwrap().document().unwrap_err().kind(),
        ErrorKind::Decode
    );
}

#[tokio::test]
async fn an_oversized_error_body_is_bounded() {
    let server = TestServer::start(|_| {
        let mut body = br#"{"success":false,"errors":[],"messages":[],"padding":""#.to_vec();
        body.resize(body.len() + 70 * 1024, b'x');
        body.extend_from_slice(b"\"}");
        Reply::json(400, body)
    })
    .await;
    let error = client(&server).zones().list().send().await.unwrap_err();
    let provider = error.provider().unwrap();
    assert_eq!(provider.status(), 400);
    assert_eq!(provider.document().unwrap_err().kind(), ErrorKind::Limit);
}

// ---------------------------------------------------------------------------------------------
// Malformed, truncated, oversized and wrong-media 2xx bodies

#[tokio::test]
async fn a_2xx_that_is_not_json_is_a_metadata_error() {
    let server = TestServer::start(|r| match r.path() {
        "/client/v4/zones" => Reply::text(200, "text/html", "<html>login</html>"),
        _ => Reply::new(200).body(fixture("zone_details.json")), // no content-type
    })
    .await;
    let cf = client(&server);
    for error in [
        cf.zones().list().send().await.unwrap_err(),
        cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err(),
    ] {
        assert_eq!(error.concord_kind(), Some(ErrorKind::Metadata), "{error:?}");
        assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    }
}

#[tokio::test]
async fn a_malformed_or_incomplete_envelope_is_a_decode_error() {
    let server = TestServer::start(|r| match r.path() {
        "/client/v4/zones" => Reply::json(200, "{\"success\": true, \"result\": [}"),
        p if p.ends_with("/no-success") => Reply::json(200, "{\"result\": {}}"),
        p if p.ends_with("/bad-zone") => {
            Reply::json(200, "{\"success\": true, \"result\": {\"name\": \"x\"}}")
        }
        _ => Reply::json(
            200,
            "{\"success\": true, \"result\": [], \"result_info\": 7}",
        ),
    })
    .await;
    let cf = client(&server);
    for error in [
        cf.zones().list().send().await.unwrap_err(),
        cf.zone(ZoneId::from("no-success"))
            .get()
            .send()
            .await
            .unwrap_err(),
        cf.zone(ZoneId::from("bad-zone"))
            .get()
            .send()
            .await
            .unwrap_err(),
        cf.zone(ZoneId::from(ZONE))
            .dns_records()
            .list()
            .send()
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error.concord_kind(), Some(ErrorKind::Decode), "{error:?}");
        assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    }
}

#[tokio::test]
async fn a_truncated_body_is_a_transport_error_after_the_head() {
    let server = TestServer::start(|_| {
        let prefix = Bytes::from_static(br#"{"success": true, "result": [{"id": "1a0"#);
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![prefix]))
    })
    .await;
    let error = client(&server).zones().list().send().await.unwrap_err();
    assert_eq!(
        error.concord_kind(),
        Some(ErrorKind::Transport),
        "{error:?}"
    );
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
}

#[tokio::test]
async fn an_oversized_success_body_is_refused_at_the_limit() {
    let server = TestServer::start(|_| {
        let total = concord_sdk_cloudflare_zones::SUCCESS_BODY_LIMIT + 1;
        let mut sent = 0usize;
        let generate = Box::new(move || {
            if sent >= total {
                return None;
            }
            let chunk = (total - sent).min(64 * 1024);
            sent += chunk;
            Some(Bytes::from(vec![b' '; chunk]))
        });
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(generate, None))
    })
    .await;
    let error = client(&server).zones().list().send().await.unwrap_err();
    assert_eq!(error.concord_kind(), Some(ErrorKind::Limit), "{error:?}");
}

#[tokio::test]
async fn no_response_is_a_transport_error_without_a_head() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.concord_kind(), Some(ErrorKind::Transport));
    assert_eq!(error.status(), None);
}

// ---------------------------------------------------------------------------------------------
// Rate limits: 429, Retry-After, global and service limit items

fn rate_limited(retry_after: Option<&str>) -> Reply {
    let reply = json(429, "error_429.json")
        .header("ratelimit", r#""default";r=0;t=30"#)
        .header("ratelimit-policy", r#""default";q=1200;w=300"#);
    match retry_after {
        Some(value) => reply.header("retry-after", value),
        None => reply,
    }
}

#[tokio::test]
async fn a_429_is_rate_limited_with_its_delay_and_items() {
    let server = TestServer::start(|_| rate_limited(Some("30"))).await;
    let error = client(&server).zones().list().send().await.unwrap_err();
    let Error::RateLimited(limited) = &error else {
        panic!("expected a rate-limit error, got {error:?}");
    };
    assert_eq!(limited.status(), 429);
    assert_eq!(
        limited.retry_after(),
        RetryAfter::After(Duration::from_secs(30))
    );
    assert_eq!(limited.document().unwrap().codes(), [1005]);
    let limits = limited.meta().rate_limits().unwrap();
    assert_eq!(limits.limit("default").unwrap().remaining, Some(0));
    assert_eq!(limits.policies[0].quota, Some(1200));
    assert_eq!(limits.policies[0].window, Some(Duration::from_secs(300)));
    assert_eq!(limited.attempts(), 1);
    assert!(error.to_string().contains("retry after 30s"), "{error}");
    // The default policy sends once.
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn global_and_service_limits_stay_separate_items() {
    let server = TestServer::start(|_| {
        ok("zone_details.json")
            .header("ratelimit", r#""default";r=1150;t=240, "dns";r=0;t=7"#)
            .header(
                "ratelimit-policy",
                r#""default";q=1200;w=300, "dns";q=10;w=10"#,
            )
    })
    .await;
    let response = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap();
    let limits = response.meta.rate_limits().unwrap();
    let names: Vec<_> = limits.limits.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["default", "dns"]);
    let exhausted: Vec<_> = limits.exhausted().map(|l| l.name.as_str()).collect();
    assert_eq!(exhausted, ["dns"]);
    assert_eq!(
        limits.limit("default").unwrap().reset,
        Some(Duration::from_secs(240))
    );
    assert_eq!(limits.policies.len(), 2);
    assert_eq!(response.meta.retry_after(), RetryAfter::Absent);
}

#[tokio::test]
async fn a_malformed_rate_limit_header_does_not_fail_the_call() {
    let server =
        TestServer::start(|_| ok("zone_details.json").header("ratelimit", "default;r=-3")).await;
    let response = client(&server)
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.meta.rate_limits(),
        Err(RateLimitHeaderError::Ratelimit)
    );
}

fn retrying() -> SendPolicy {
    SendPolicy::default().retry_rate_limited(NonZeroUsize::new(3).unwrap(), Duration::from_secs(5))
}

#[tokio::test]
async fn the_opt_in_policy_honours_retry_after_on_reads() {
    let script =
        concord_example_testserver::Script::new([rate_limited(Some("0")), ok("zone_details.json")]);
    let server = TestServer::start(move |_| script.next()).await;
    let response = client_with(&server, retrying())
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(response.result.id.as_str(), ZONE);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, requests[1].target);
    assert_eq!(
        requests[1].header("authorization"),
        Some(&*format!("Bearer {TOKEN}"))
    );
}

#[tokio::test]
async fn the_opt_in_policy_stops_at_its_bounds() {
    // A delay above the maximum, a missing delay and an invalid one are returned at once.
    for header in [Some("120"), None, Some("soon")] {
        let server = TestServer::start(move |_| rate_limited(header)).await;
        let error = client_with(&server, retrying()).zones().list().send().await;
        let Err(Error::RateLimited(limited)) = error else {
            panic!("expected a rate-limit error");
        };
        assert_eq!(limited.attempts(), 1, "{header:?}");
        assert_eq!(server.requests().len(), 1);
    }
    // Every attempt rate limited: the last 429 after three requests.
    let server = TestServer::start(|_| rate_limited(Some("0"))).await;
    let error = client_with(&server, retrying()).zones().list().send().await;
    let Err(Error::RateLimited(limited)) = error else {
        panic!("expected a rate-limit error");
    };
    assert_eq!(limited.attempts(), 3);
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn the_opt_in_policy_repeats_nothing_but_a_429() {
    let server = TestServer::start(|r| match r.path() {
        "/client/v4/zones" => json(503, "error_403.json").header("retry-after", "0"),
        _ => ok("failure_success_false.json").header("retry-after", "0"),
    })
    .await;
    let cf = client_with(&server, retrying());
    assert_eq!(
        cf.zones()
            .list()
            .send()
            .await
            .unwrap_err()
            .status()
            .unwrap(),
        503
    );
    assert!(matches!(
        cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err(),
        Error::Provider(_)
    ));
    assert_eq!(server.requests().len(), 2);
}

// ---------------------------------------------------------------------------------------------
// Redaction

#[tokio::test]
async fn the_token_never_appears_in_debug_or_display() {
    let server = TestServer::start(|r| match r.path() {
        "/client/v4/zones" => json(403, "error_403.json")
            .header("set-cookie", "__cf_bm=synthetic-cookie-secret; path=/"),
        _ => ok("zone_details.json").header("set-cookie", "__cf_bm=synthetic-cookie-secret"),
    })
    .await;
    let cf = client(&server);
    let mut shown = vec![
        format!("{cf:?}"),
        format!("{:?}", cf.zone(ZoneId::from(ZONE))),
        format!(
            "{:?}",
            cf.zone(ZoneId::from(ZONE))
                .dns_record(RecordId::from(RECORD))
                .get()
        ),
        format!(
            "{:?}",
            cf.zones().list().set_account_id(AccountId::from(ACCOUNT))
        ),
        format!("{:?}", cf.zone(ZoneId::from(ZONE)).get().prepare().unwrap()),
    ];
    assert!(
        shown[4].contains("authorization: <redacted>"),
        "{}",
        shown[4]
    );

    let error = cf.zones().list().send().await.unwrap_err();
    shown.push(format!("{error}"));
    shown.push(format!("{error:?}"));
    let response = cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap();
    shown.push(format!("{response:?}"));
    assert!(shown.last().unwrap().contains("set-cookie"));

    // A transport failure (nothing listens on the port any more).
    let url = server.url();
    drop(server);
    let offline = Cloudflare::connect(concord::reqwest::Client::new(), TOKEN)
        .unwrap()
        .with_base_url(url)
        .unwrap();
    let error = offline
        .zone(ZoneId::from(ZONE))
        .get()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.concord_kind(), Some(ErrorKind::Transport));
    shown.push(format!("{error}"));
    shown.push(format!("{error:?}"));

    for text in &shown {
        assert!(!text.contains(TOKEN), "token in {text}");
        assert!(
            !text.contains("synthetic-cookie-secret"),
            "cookie in {text}"
        );
    }
}

#[tokio::test]
async fn the_authorization_header_is_flagged_sensitive() {
    let cf = Cloudflare::connect(concord::reqwest::Client::new(), TOKEN).unwrap();
    let prepared = cf.zones().list().prepare().unwrap();
    let value = &prepared.request().headers()["authorization"];
    assert!(value.is_sensitive());
    assert_eq!(value, &*format!("Bearer {TOKEN}"));
}
