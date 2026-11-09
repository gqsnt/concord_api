//! Secret: the API token (`#[sensitive] header(AUTHORIZATION)`, `src/lib.rs:64`), passed to
//! `Cloudflare::connect` (`src/lib.rs:150`) or built into `CloudflareConfig` with
//! `concord_auth::bearer`, sent as `Authorization: Bearer`. One canary; the server echoes it into
//! headers (`ResponseMeta` prints names only) and every malformed, wrong-media and truncated
//! body. Read (`zone(..).get()`) and write (`dns_records().create(..)`) cover the seven
//! scenarios through the SDK `send()` and the native `Prepared`, with a `zones().list()` walk
//! beside them. Client, config, policy, scopes, endpoints, `Prepared`, responses, metadata,
//! rate limits, page walks, `PagesError`, the SDK `Error`, its `ProviderError`/`EnvelopeError`
//! and Concord errors are formatted. A token refused by `connect` gets its own canary in a
//! separate test (it never travels).
//! Not covered: native reqwest values and the clear provider envelope (`ResponseInfo::message`
//! is a public field with derived `Debug`, so the 4xx/5xx documents carry a synthetic message,
//! not a canary). No wire exemptions in the main test.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(403, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) -> Result<T, concord::Error<D>> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error.widen()),
    }
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_cloudflare_zones::{
    Cloudflare, CloudflareConfig, Error, NewDnsRecord, RecordTtl, RecordType, ResponseMeta,
    SendPolicy, ZoneId,
};

fn metadata(c: &mut Canaries, meta: &ResponseMeta) {
    c.assert_debug_absent("metadata", meta);
    c.assert_debug_absent("rate limits", &meta.rate_limits());
    c.assert_debug_absent("retry after", &meta.retry_after());
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Provider(p) | Error::RateLimited(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_display_absent("provider failure", p);
            c.assert_debug_absent("provider document", &p.document());
            metadata(c, p.meta());
            if let Err(e) = p.document() {
                concord_error(c, e);
            }
        }
        Error::Envelope(e) => {
            c.assert_debug_absent("envelope failure", e);
            metadata(c, e.meta());
        }
        Error::Concord(e) | Error::CreateUncertain(e) => concord_error(c, e),
        _ => {}
    }
}

fn record() -> NewDnsRecord {
    NewDnsRecord::new(
        RecordType::A,
        "new.example.com",
        "192.0.2.42",
        RecordTtl::AUTOMATIC,
        Some(true),
    )
    .unwrap()
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("cloudflare-zones");
    let token = c.issue();
    let zone_id = ZoneId::from("1a00000000000000000000000000000f");
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &concord_example_testserver::Recorded| {
            let success = if r.method == "POST" {
                include_str!("../fixtures/dns_record_created.json")
            } else if r.path() == "/client/v4/zones" {
                r#"{"success":true,"errors":[],"messages":[],"result":[],"result_info":{"page":1,"per_page":20,"count":0,"total_count":0,"total_pages":1}}"#
            } else {
                include_str!("../fixtures/zone_details.json")
            };
            reply(case, success, include_str!("../fixtures/error_403.json"), &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let config = CloudflareConfig {
            authorization: concord_sdk_cloudflare_zones::concord_auth::bearer(&token).unwrap(),
            policy: SendPolicy::default(),
        };
        c.assert_debug_absent("config", &config);
        c.assert_debug_absent("policy", &config.policy);
        let configured = Cloudflare::new(http(), config)
            .with_base_url(&base)
            .unwrap();
        c.assert_debug_absent("configured client", &configured);
        let api = Cloudflare::connect(http(), &token)
            .unwrap()
            .with_base_url(&base)
            .unwrap();
        let zone = api.zone(zone_id.clone());
        let records = zone.dns_records();
        let read = zone.get();
        let write = records.create(record());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("zone scope", &zone);
        c.assert_debug_absent("records scope", &records);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("read response", &response);
                c.assert_debug_absent("zone model", &response.result);
                metadata(&mut c, &response.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("write response", &response);
                metadata(&mut c, &response.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let mut pages = api.zones().list().pages();
        c.assert_debug_absent("zone walk", &pages);
        match pages.next_page().await {
            Ok(page) => {
                assert_eq!(case, 0);
                let page = page.unwrap();
                c.assert_debug_absent("zone page", &page);
                c.assert_debug_absent("page metadata", &page.meta);
                metadata(&mut c, &page.meta.meta);
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "cloudflare-zones: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_token_never_prints() {
    let mut c = Canaries::new("cloudflare-zones");
    let token = format!("{} with spaces", c.issue());
    let e = Cloudflare::connect(http(), &token).unwrap_err();
    assert!(matches!(e, Error::Credential(_)));
    c.assert_debug_absent("refused credential", &e);
    c.assert_error_absent("refused credential and sources", &e);
}
