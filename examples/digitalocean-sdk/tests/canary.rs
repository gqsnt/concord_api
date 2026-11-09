//! Secrets: the bearer token (`DigitalOcean::connect` / `for_fixture`, `src/lib.rs:58,71`;
//! `#[sensitive] header(AUTHORIZATION)`, `src/lib.rs:24`) and the provider continuation
//! `links.pages.next` (response-only: `PageLinks`, `Links` and `NextLink` withhold it,
//! `src/models.rs:125-146`, `src/pagination.rs:12`; README "Failure semantics": "pagination keys
//! withhold ... followed URLs"). Each has its own canary. The token travels in `Authorization`;
//! the continuation canary sits in the query of a next link that reaches the decoder and travels
//! as the followed page's target. The server echoes the token into headers (including the
//! strictly parsed `ratelimit-*` and `retry-after` fields), the error document (`id`, `message`,
//! `request_id`: `Problem` Debug is withheld) and every malformed, wrong-media and truncated body.
//! The SDK is read-only: `droplet(id).get()` covers the seven scenarios through the SDK `send()`
//! and the native `Prepared`; `droplets().list()`, a droplet traversal (and its resumption from
//! a reported `NextLink`) and an action traversal whose next link leaves the origin
//! (`Error::UnsafeNext`, carrying the same continuation canary) run beside it. A token refused by
//! `connect` and the generated `DigitalOceanConfig` with an unflagged value are checked too.
//! Not covered: native reqwest values; explicit field access (`Problem` fields, model fields,
//! `PageLinks::next`); decoded models carry no canary (the README documents model fields as
//! disclosed, and `TagName` is an unmarked filter printed by design). No wire exemptions.

use concord_check_canary::Canaries;
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

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
    concord_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
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
        .header("ratelimit-limit", echo)
        .header("retry-after", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord::reqwest::header::HeaderValue;
use concord_sdk_digitalocean::{
    DigitalOcean, DigitalOceanConfig, Error, PageSize, Response, ResponseMeta, Traversal,
    concord_pagination::Resume,
};

fn metadata(c: &mut Canaries, meta: &ResponseMeta) {
    c.assert_debug_absent("metadata", meta);
    c.assert_debug_absent("operation", &meta.operation());
    c.assert_debug_absent("rate limit", &meta.limit());
    c.assert_debug_absent("rate remaining", &meta.remaining());
    c.assert_debug_absent("rate reset", &meta.reset_epoch_seconds());
    c.assert_debug_absent("retry after", &meta.retry_after_seconds());
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_debug_absent("provider document", &p.document);
            c.assert_debug_absent("status class", &p.class());
            metadata(c, &p.meta);
            if let Err(e) = &p.document {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

fn sent<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<Response<T>, Error>) {
    c.assert_debug_absent(label, result);
    match result {
        Ok(response) => {
            c.assert_debug_absent("decoded value", &response.value);
            metadata(c, &response.meta);
        }
        Err(e) => error(c, e),
    }
}

// Reads pages until the end or a failure; returns the pages read and the last reported key.
async fn traverse<T>(
    c: &mut Canaries,
    mut pages: Traversal<T>,
) -> (usize, Option<Resume<concord_sdk_digitalocean::NextLink>>) {
    c.assert_debug_absent("traversal", &pages);
    let mut read = 0;
    loop {
        let page = pages.next_page().await;
        c.assert_debug_absent("traversal page Result", &page);
        match page {
            Ok(Some(page)) => {
                c.assert_debug_absent("next link", &page.next);
                metadata(c, &page.meta);
                read += 1;
            }
            Ok(None) => return (read, None),
            Err(e) => {
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                c.assert_debug_absent("resume", &e.resume());
                error(c, e.provider_error().unwrap());
                c.assert_debug_absent("ended traversal", &pages.next_page().await);
                return (read, e.resume().cloned());
            }
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("digitalocean");
    let token = c.issue();
    let continuation = c.issue();
    let config = DigitalOceanConfig {
        destination: concord_sdk_digitalocean::Destination::production(),
        authorization: HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    };
    c.assert_debug_absent("generated config (value not flagged)", &config);
    c.assert_debug_absent("client from config", &DigitalOcean::new(http(), config));
    let size = PageSize::new(1).unwrap();
    let cap = NonZeroUsize::new(3).unwrap();
    for case in 0..7 {
        let (echo, next) = (token.clone(), continuation.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let origin = format!("http://{}", r.host);
            let success = match r.path() {
                "/v2/droplets/7" => include_str!("../fixtures/droplet.json").to_owned(),
                "/v2/droplets" if r.query_all("cursor") == [next.clone()] => {
                    include_str!("../fixtures/droplets-last.json").replace("__ORIGIN__", &origin)
                }
                "/v2/droplets" => include_str!("../fixtures/droplets-first.json").replace(
                    "__ORIGIN__/v2/droplets?page=2&per_page=1\",",
                    &format!("{origin}/v2/droplets?page=2&per_page=1&cursor={next}\","),
                ),
                _ => format!(
                    r#"{{"actions":[],"links":{{"pages":{{"next":"http://198.51.100.7/v2/droplets/7/actions?cursor={next}"}}}}}}"#
                ),
            };
            let problem =
                format!(r#"{{"id":"{echo}","message":"{echo}","request_id":"{echo}"}}"#);
            reply(case, &success, &problem, &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api = DigitalOcean::for_fixture(&token, &base).unwrap();
        let connected = DigitalOcean::connect(&token).unwrap();
        let droplet = api.droplet(7);
        let read = droplet.get();
        let list = api.droplets().list().set_per_page(size);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("production client", &connected);
        c.assert_debug_absent("droplets scope", &api.droplets());
        c.assert_debug_absent("droplet scope", &droplet);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("list endpoint", &list);
        c.assert_debug_absent("list Prepared", &list.clone().prepare().unwrap());
        c.assert_debug_absent(
            "production Prepared",
            &connected.droplet(7).get().prepare().unwrap(),
        );
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        sent(&mut c, "read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            // Withholding keeps the diagnosis: status, class, stage and operation name.
            match (case, e) {
                (1, Error::Provider(p)) => {
                    assert_eq!(p.meta.status().as_u16(), 401);
                    assert!(e.to_string().contains("401"));
                    assert!(e.to_string().contains("Unauthorized"));
                }
                (3, Error::Concord(inner)) => {
                    assert_eq!(inner.kind(), concord::ErrorKind::Decode);
                    assert_eq!(inner.endpoint(), Some("DigitalOcean.droplet.get"));
                }
                (1 | 3, _) => panic!("unexpected error kind: {e}"),
                _ => {}
            }
        }
        let result = list.send().await;
        sent(&mut c, "list Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let (read, _) = traverse(&mut c, api.droplet_pages(size, cap)).await;
        assert_eq!(read, if case == 0 { 2 } else { 0 });
        if case == 0 {
            // Resume from the first page's reported key: the followed link is checked again.
            let mut first = api.droplet_pages(size, NonZeroUsize::MIN);
            first.next_page().await.unwrap().unwrap();
            let bounded = first.next_page().await.unwrap_err();
            c.assert_debug_absent("bounded traversal", &bounded);
            c.assert_display_absent("bounded traversal", &bounded);
            let from = bounded.resume().cloned().unwrap();
            let resumed = api.droplet_pages(size, cap).starting_at(from);
            assert_eq!(traverse(&mut c, resumed).await.0, 1);
            let (read, at) = traverse(&mut c, api.action_pages(7, size, cap)).await;
            assert_eq!(read, 0);
            assert!(matches!(at, Some(Resume::Start)));
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    let e = PageSize::new(0).unwrap_err();
    error(&mut c, &e);
    assert!(c.checked() >= 250);
    println!(
        "digitalocean: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_token_never_prints() {
    let mut c = Canaries::new("digitalocean");
    let token = format!("{}\r\nx-injected: 1", c.issue());
    let e = DigitalOcean::connect(&token).unwrap_err();
    assert!(matches!(e, Error::Credential));
    error(&mut c, &e);
    let origin = format!("http://{}@127.0.0.1:9", c.issue());
    let e = DigitalOcean::for_fixture("token", &origin).unwrap_err();
    assert!(matches!(e, Error::FixtureOrigin));
    error(&mut c, &e);
}
