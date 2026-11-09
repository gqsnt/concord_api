//! Create regressions against synthetic loopback fixtures only.

mod support;

use std::{num::NonZeroUsize, time::Duration};

use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_cloudflare_zones::{
    Cloudflare, Error, NewDnsRecord, RecordInputError, RecordTtl, RecordType, RetryAfter,
    SendPolicy, ZoneId,
    concord::{ErrorKind, reqwest},
};
use serde_json::{Value, json};
use support::{TOKEN, ZONE, client, fixture, ok};

fn input() -> NewDnsRecord {
    NewDnsRecord::new(
        RecordType::A,
        "new.example.com",
        "192.0.2.42",
        RecordTtl::AUTOMATIC,
        Some(true),
    )
    .unwrap()
}

fn retrying() -> SendPolicy {
    SendPolicy::default().retry_rate_limited(NonZeroUsize::new(3).unwrap(), Duration::from_secs(1))
}

#[tokio::test]
async fn create_returns_the_created_record_and_exact_json_body() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        ok("dns_record_created.json")
            .header("ratelimit", "\"global\";r=1199;t=300, \"dns\";r=9;t=60")
    })
    .await;
    let cf = client(&server);
    let endpoint = cf.zone(ZoneId::from(ZONE)).dns_records().create(input());
    assert!(!format!("{endpoint:?}").contains(TOKEN));
    assert!(!format!("{:?}", endpoint.prepare().unwrap()).contains(TOKEN));
    let response = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .create(input())
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.result.id.as_str(),
        "2b00000000000000000000000000000c"
    );
    assert_eq!(response.result.record_type, RecordType::A);
    assert_eq!(response.result.name, "new.example.com");
    assert_eq!(response.result.content.as_deref(), Some("192.0.2.42"));
    assert_eq!(response.result.ttl, 1);
    assert_eq!(response.result.proxied, Some(true));
    assert_eq!(
        response.meta.operation().unwrap().name(),
        "Cloudflare.zone.dns_records.create"
    );
    assert_eq!(response.meta.operation().unwrap().method(), Some("POST"));
    assert_eq!(response.meta.rate_limits().unwrap().limits.len(), 2);
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.target,
        format!("/client/v4/zones/{ZONE}/dns_records")
    );
    assert_eq!(request.host, server.addr().to_string());
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap(),
        json!({
            "type": "A", "name": "new.example.com", "content": "192.0.2.42", "ttl": 1, "proxied": true
        })
    );
    server.shutdown().await;
}

#[tokio::test]
async fn duplicate_on_http_success_is_a_typed_provider_error_and_never_retried() {
    for status in [200, 201, 400] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(status, fixture("dns_record_exists.json")).header("retry-after", "0")
        })
        .await;
        let error = client(&server)
            .set_policy(retrying())
            .zone(ZoneId::from(ZONE))
            .dns_records()
            .create(input())
            .send()
            .await
            .unwrap_err();
        let Error::Provider(provider) = &error else {
            panic!("{error:?}");
        };
        assert_eq!(provider.status().as_u16(), status);
        assert!(provider.has_code(81057));
        assert_eq!(provider.errors()[0].message, "The record already exists.");
        assert_eq!(
            provider.messages()[0].message,
            "Synthetic duplicate create refusal"
        );
        assert_eq!(provider.attempts(), 1);
        assert_eq!(server.requests().len(), 1);
        assert!(!format!("{error} {error:?}").contains(TOKEN));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn even_a_retryable_429_never_replays_a_create() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(429, fixture("error_429.json")).header("retry-after", "0")
    })
    .await;
    let error = client(&server)
        .set_policy(retrying())
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .create(input())
        .send()
        .await
        .unwrap_err();
    let Error::RateLimited(provider) = error else {
        panic!("{error:?}");
    };
    assert_eq!(provider.retry_after(), RetryAfter::After(Duration::ZERO));
    assert_eq!(provider.attempts(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

// Paused clock: the 500 ms timeout fires only once the server holds the create, after writing its
// head when it stalls the body, so it ends the body and never the wait for the head.
#[tokio::test(start_paused = true)]
async fn timeout_before_headers_or_during_success_body_is_uncertain_and_sent_once() {
    for after_headers in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            if after_headers {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::Stall)
            } else {
                ok("dns_record_created.json").delay(Duration::from_secs(30))
            }
        })
        .await;
        let timeout = Duration::from_millis(500);
        let http = reqwest::Client::builder()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .unwrap();
        let cf = Cloudflare::connect(http, TOKEN)
            .unwrap()
            .with_base_url(server.url())
            .unwrap()
            .set_policy(retrying());
        let zone = cf.zone(ZoneId::from(ZONE));
        let records = zone.dns_records();
        let create = records.create(input()).send();
        let error = if after_headers {
            paused::time_out_in_body(&server, |_| true, timeout, create).await
        } else {
            paused::time_out(&server, |_| true, timeout, create).await
        }
        .unwrap_err();
        let Error::CreateUncertain(cause) = &error else {
            panic!("{error:?}");
        };
        assert!(cause.transport().unwrap().is_timeout());
        assert_eq!(error.concord_kind(), Some(ErrorKind::Transport));
        assert_eq!(
            error.status().map(|status| status.as_u16()),
            after_headers.then_some(200)
        );
        assert!(error.to_string().contains("reconcile"));
        assert!(!format!("{error} {error:?}").contains(TOKEN));
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.single().method, "POST");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unusable_success_or_connection_loss_requires_reconciliation_without_retry() {
    let script = concord_example_testserver::Script::new([
        Reply::hang_up(),
        Reply::json(200, "{"),
        Reply::text(200, "text/html", "<html>proxy</html>"),
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![
                concord_sdk_cloudflare_zones::concord::bytes::Bytes::from_static(b"{"),
            ])),
    ]);
    let server = TestServer::start(move |_: &Recorded| -> Reply { script.next() }).await;
    let cf = client(&server).set_policy(retrying());
    for kind in [
        ErrorKind::Transport,
        ErrorKind::Decode,
        ErrorKind::Metadata,
        ErrorKind::Transport,
    ] {
        let error = cf
            .zone(ZoneId::from(ZONE))
            .dns_records()
            .create(input())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::CreateUncertain(_)), "{error:?}");
        assert_eq!(error.concord_kind(), Some(kind));
    }
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_create_target_and_body_are_refused_before_network_io() {
    let server = TestServer::start(|_: &Recorded| -> Reply { ok("dns_record_created.json") }).await;
    let cf = client(&server);
    let error = cf
        .zone(ZoneId::from("x".repeat(33)))
        .dns_records()
        .create(input())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Input(RecordInputError::ZoneId)),
        "{error:?}"
    );
    assert!(error.status().is_none());
    for zone in ["", ".."] {
        let error = cf
            .zone(ZoneId::from(zone))
            .dns_records()
            .create(input())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Concord(_)));
        assert_eq!(error.concord_kind(), Some(ErrorKind::Build));
    }
    // The usual caller's `?` propagates validation failures before constructing the endpoint.
    async fn invalid_body(cf: &Cloudflare) -> Result<(), Error> {
        let body = NewDnsRecord::new(
            RecordType::A,
            "example.com",
            "::1",
            RecordTtl::AUTOMATIC,
            None,
        )?;
        cf.zone(ZoneId::from(ZONE))
            .dns_records()
            .create(body)
            .send()
            .await?;
        Ok(())
    }
    assert!(matches!(
        invalid_body(&cf).await,
        Err(Error::Input(RecordInputError::Content))
    ));
    assert!(server.requests().is_empty());
    // Count the decoded identifier, not its percent-encoded path length; reads remain permissive.
    let encoded_id = "a/".repeat(16);
    cf.zone(ZoneId::from(encoded_id))
        .dns_records()
        .create(input())
        .send()
        .await
        .unwrap();
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[test]
fn create_input_boundaries_and_type_specific_rules() {
    for value in [0, 2, 29, 30, 59, 86401, u32::MAX] {
        assert_eq!(RecordTtl::new(value), Err(RecordInputError::Ttl));
    }
    for value in [1, 60, 86400] {
        assert!(RecordTtl::new(value).is_ok());
    }
    for value in [1, 30, 59, 60, 86400] {
        assert!(RecordTtl::enterprise(value).is_ok());
    }
    for value in [0, 2, 29, 86401] {
        assert_eq!(RecordTtl::enterprise(value), Err(RecordInputError::Ttl));
    }
    let make = |kind, name: &str, content: &str, proxy| {
        NewDnsRecord::new(kind, name, content, RecordTtl::AUTOMATIC, proxy)
    };
    let max_name = std::array::from_fn::<_, 4, _>(|_| "x".repeat(63)).join(".");
    assert_eq!(max_name.len(), 255);
    assert!(make(RecordType::A, &max_name, "192.0.2.1", None).is_ok());
    for name in [
        "",
        "a..example",
        "-bad.example",
        "bad-.example",
        "bad name",
        "café.example",
        "example.*",
        "*.*.example",
        &"x".repeat(64),
        &(max_name + "x"),
    ] {
        assert_eq!(
            make(RecordType::A, name, "192.0.2.1", None),
            Err(RecordInputError::Name)
        );
    }
    for name in [
        "*.example.com",
        "_acme-challenge.example.com",
        "xn--caf-dma.example.",
    ] {
        assert!(make(RecordType::A, name, "192.0.2.1", None).is_ok());
    }
    for (kind, content) in [
        (RecordType::A, "2001:db8::1"),
        (RecordType::Aaaa, "192.0.2.1"),
        (RecordType::A, "999.0.0.1"),
        (RecordType::Cname, "https://example.com"),
        (RecordType::Cname, "EXAMPLE.COM."),
        (RecordType::Ns, ""),
        (RecordType::Ptr, "bad host"),
        (RecordType::Txt, "unquoted"),
        (RecordType::Txt, "\"unterminated"),
        (RecordType::Txt, "\"bad\\999\""),
        (RecordType::Txt, "\"a\"\"b\""),
    ] {
        assert_eq!(
            make(kind, "example.com", content, None),
            Err(RecordInputError::Content)
        );
    }
    for (kind, content, proxy) in [
        (RecordType::Aaaa, "2001:db8::1", Some(true)),
        (RecordType::Cname, "target.example.com", Some(false)),
        (RecordType::Ns, "ns.example.com", None),
        (RecordType::Ptr, "host.example.com", None),
        (RecordType::Txt, "\"one\" \"two\"", None),
        (RecordType::Txt, "\"\"", Some(false)),
        (RecordType::Txt, "\"a\\032b\"", None),
    ] {
        let body = make(kind, "example.com", content, proxy).unwrap();
        let body = serde_json::to_value(body).unwrap();
        assert_eq!(body["content"], content);
        assert_eq!(body.get("proxied").cloned(), proxy.map(Value::Bool));
    }
    assert!(
        make(
            RecordType::Txt,
            "example.com",
            &format!("\"{}\"", "x".repeat(300)),
            None
        )
        .is_ok()
    );
    assert_eq!(
        make(RecordType::Txt, "example.com", "\"text\"", Some(true)),
        Err(RecordInputError::Proxied)
    );
    for kind in [
        RecordType::Mx,
        RecordType::Srv,
        RecordType::Other("FUTURE".into()),
    ] {
        assert_eq!(
            make(kind, "example.com", "content", None),
            Err(RecordInputError::UnsupportedType)
        );
    }
}
