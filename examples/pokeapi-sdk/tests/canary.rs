//! No secret input: PokéAPI needs no authentication, the declaration has no `#[sensitive]`
//! value (`src/lib.rs:70-99`) and no constructor takes a credential. The most sensitive-looking
//! values it accepts are passwords in URL user info, which are refused before any request and
//! documented as never printed: a `follow` URL and a `pokemon_url` URL
//! (`Error::InvalidUrl`, `src/lib.rs:120-122`) and a `with_base_url` base (Concord's build
//! error). Each gets a distinct canary; the server echoes the `follow` canary into every body it
//! sends (error documents, malformed, wrong media, truncated), whose formatting is withheld.
//! Read-only SDK: `pokemon` and `list`/`follow` cover the seven scenarios, generated and native
//! `Prepared` sends. Wire: the three canaries never travel by design (refused before sending),
//! so they are exempt from the positive control, and the test asserts no request carried them.
//! A separate control canary in an ordinary sent value (the Pokémon name, a path segment that
//! `Prepared` and the final URL show by design) proves the wire check sees requests; it is only
//! checked on the wire. Not covered: native reqwest values, and the SDK's `ResponseMeta`
//! (`headers` and `url` are documented clear public fields with derived `Debug`, so no canary
//! is echoed into response headers).

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
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

// The echo goes into bodies only: their formatting is withheld everywhere.
fn reply(case: usize, success: &str, echo: &str) -> Reply {
    match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::text(404, "text/plain", format!("Not Found {echo}")),
        2 => Reply::json(500, format!(r#"{{"detail":"{echo}"}}"#)),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    }
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
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_pokeapi::{Client, Error, UrlRejection};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord { error, meta } => {
            concord_error(c, error);
            c.assert_debug_absent("SDK error metadata", meta);
        }
        Error::InvalidUrl(reason) => {
            c.assert_debug_absent("URL rejection", reason);
            c.assert_display_absent("URL rejection", reason);
        }
    }
}

#[tokio::test]
async fn refused_urls_never_print() {
    let mut c = Canaries::new("pokeapi");
    let follow_password = c.issue();
    let detail_password = c.issue();
    let base_password = c.issue();
    let mut control = Canaries::new("pokeapi-control");
    let name = control.issue();
    for case in 0..7 {
        let echo = follow_password.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.path().ends_with("/pokemon/") {
                format!(
                    r#"{{"count":2,"next":"http://{}/api/v2/pokemon/?offset=1&limit=1","previous":null,"results":[]}}"#,
                    r.host
                )
            } else {
                include_str!("../fixtures/pokemon.json").into()
            };
            reply(case, &success, &echo)
        })
        .await;
        let origin = if case == 6 { refused() } else { server.url() };
        let base = format!("{origin}/api/v2");
        let api = Client::new(http()).with_base_url(&base).unwrap();
        let read = api.pokemon(name.clone());
        let list = api.list().set_limit(1).set_offset(0);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("list endpoint", &list);
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        c.assert_debug_absent("list Prepared", &list.clone().prepare().unwrap());
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("read response", &response);
                c.assert_debug_absent("Pokemon model", &response.value);
            }
            Err(e) => error(&mut c, &e),
        }
        let result = list.send().await;
        c.assert_debug_absent("list Result", &result);
        match result {
            Ok(response) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("list page", &response.value);
                let next = response.value.next.clone().unwrap();
                let followed = api.follow(&next).await;
                c.assert_debug_absent("followed Result", &followed);
                assert!(followed.is_ok());
            }
            Err(e) => error(&mut c, &e),
        }
        // User info is refused before anything is sent, with the reason only.
        let with_password = format!(
            "http://user:{follow_password}@{}/api/v2/pokemon/?offset=1&limit=1",
            origin.trim_start_matches("http://")
        );
        let refused_follow = api.follow(&with_password).await;
        c.assert_debug_absent("refused follow Result", &refused_follow);
        let e = refused_follow.unwrap_err();
        assert!(matches!(e, Error::InvalidUrl(UrlRejection::Userinfo)));
        error(&mut c, &e);
        let detail = format!(
            "http://user:{detail_password}@{}/api/v2/pokemon/clefairy/",
            origin.trim_start_matches("http://")
        );
        let refused_detail = api.pokemon_url(&detail).await;
        c.assert_debug_absent("refused pokemon_url Result", &refused_detail);
        let e = refused_detail.unwrap_err();
        assert!(matches!(e, Error::InvalidUrl(UrlRejection::Userinfo)));
        error(&mut c, &e);
        let bad_base = format!(
            "http://user:{base_password}@{}/api/v2",
            origin.trim_start_matches("http://")
        );
        let refused_base = Client::new(http()).with_base_url(&bad_base);
        let e = refused_base.unwrap_err();
        concord_error(&mut c, &e);
        // The three canaries never travel: no recorded request carries one.
        let requests = server.requests();
        for request in &requests {
            let wire = concord_check_canary::wire_text(request);
            for canary in c.issued() {
                assert!(!wire.contains(canary.as_str()), "{canary} was sent");
            }
        }
        if case == 0 {
            control.assert_on_wire(&requests);
            assert_eq!(requests.len(), 4, "read, native read, list and follow only");
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 150);
    println!(
        "pokeapi: {} renderings; positive control passed (control canary on the wire; the three user-info canaries never sent)",
        c.checked()
    );
}
