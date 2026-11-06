//! Independent application consumer. Predictions and provenance are in FIRST_USE.md.

use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_pokeapi::{Client, Error, Pokemon, PokemonPage, Response};
use serde_json::{Value, json};

const CLEFAIRY: &[u8] = include_bytes!("../fixtures/first_use/clefairy.json");
const NOT_FOUND: &[u8] = include_bytes!("../fixtures/first_use/not-found.txt");
const UNAVAILABLE: &[u8] = include_bytes!("../fixtures/first_use/unavailable.json");
const INCOMPLETE: &[u8] = include_bytes!("../fixtures/first_use/incomplete-detail.json");

fn local(server: &TestServer) -> Client {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .expect("build local transport");
    Client::new(http)
        .with_base_url(format!("{}/api/v2", server.url()))
        .expect("accept loopback API base")
}

fn json_reply(status: u16, bytes: impl AsRef<[u8]>) -> Reply {
    Reply::json(status, bytes).header("x-fixture", "first-use")
}

fn checked_error(
    failure: Error,
    kind: ErrorKind,
    status: u16,
    expected_url: &str,
) -> concord::Error {
    let Error::Concord { error, meta } = failure else {
        panic!("expected a Concord error, got {failure:?}");
    };
    assert_eq!(error.kind(), kind);
    let meta = meta.expect("received head must survive failure");
    assert_eq!(meta.status.as_u16(), status);
    assert_eq!(meta.headers["x-fixture"], "first-use");
    assert_eq!(meta.url.as_str(), expected_url);
    if kind == ErrorKind::Status {
        assert_eq!(error.status().unwrap().as_u16(), status);
    } else {
        assert_eq!(error.status(), None);
    }
    error
}

async fn finish(server: TestServer, expected_targets: &[&str]) {
    let requests = server.requests();
    assert_eq!(requests.len(), expected_targets.len(), "{requests:#?}");
    for (request, target) in requests.iter().zip(expected_targets) {
        assert_eq!(request.method, "GET");
        assert_eq!(&request.target, target);
        assert_eq!(request.host, server.addr().to_string());
        assert_eq!(request.header_all("accept"), ["application/json"]);
        assert_eq!(request.header("authorization"), None);
        assert_eq!(request.header("cookie"), None);
        assert!(request.body.is_empty());
        assert_eq!(request.body_len, 0);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_named_pokemon_has_typed_fields_extra_json_and_response_head() {
    let server = TestServer::start(|_| json_reply(200, CLEFAIRY)).await;
    let api = local(&server);

    let response = api.pokemon("clefairy").await.expect("named detail");
    let pokemon = response.value;
    assert_eq!(pokemon.id, 35);
    assert_eq!(pokemon.name, "clefairy");
    assert_eq!(pokemon.base_experience, Some(113));
    assert_eq!((pokemon.height, pokemon.weight, pokemon.order), (6, 75, 56));
    assert!(pokemon.is_default);
    assert_eq!(
        pokemon.extra["abilities"][0]["ability"]["name"],
        "friend-guard"
    );
    assert_eq!(pokemon.extra["types"][0]["type"]["name"], "fairy");
    assert!(pokemon.extra["sprites"]["front_default"].is_null());
    assert!(!pokemon.extra.contains_key("id"));
    assert_eq!(response.meta.status, reqwest::StatusCode::OK);
    assert_eq!(response.meta.headers["x-fixture"], "first-use");
    assert_eq!(
        response.meta.url.as_str(),
        format!("{}/api/v2/pokemon/clefairy/", server.url())
    );

    finish(server, &["/api/v2/pokemon/clefairy/"]).await;
}

#[tokio::test]
async fn a_numeric_id_and_nullable_experience_use_the_same_public_entry_point() {
    let mut fixture: Value = serde_json::from_slice(CLEFAIRY).unwrap();
    fixture["base_experience"] = Value::Null;
    let bytes = serde_json::to_vec(&fixture).unwrap();
    let server = TestServer::start(move |_| json_reply(200, &bytes)).await;
    let response = local(&server).pokemon("35").await.unwrap();

    assert_eq!(response.value.name, "clefairy");
    assert_eq!(response.value.id, 35);
    assert_eq!(response.value.base_experience, None);
    finish(server, &["/api/v2/pokemon/35/"]).await;
}

#[tokio::test]
async fn a_not_found_preserves_the_unstandardized_error_body() {
    let server = TestServer::start(|_| {
        Reply::text(404, "text/plain; charset=utf-8", NOT_FOUND).header("x-fixture", "first-use")
    })
    .await;
    let failure = local(&server).pokemon("missing-pokemon").await.unwrap_err();
    let error = checked_error(
        failure,
        ErrorKind::Status,
        404,
        &format!("{}/api/v2/pokemon/missing-pokemon/", server.url()),
    );
    // SDK Error has no into_response(): recover the public inner Concord error first.
    let response = error.into_response().expect("unread rejected response");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(response.bytes().await.unwrap().as_ref(), NOT_FOUND);
    finish(server, &["/api/v2/pokemon/missing-pokemon/"]).await;
}

#[tokio::test]
async fn a_wrong_success_content_type_keeps_the_unread_body() {
    let server = TestServer::start(|_| {
        Reply::text(200, "text/plain", CLEFAIRY).header("x-fixture", "first-use")
    })
    .await;
    let failure = local(&server).pokemon("clefairy").await.unwrap_err();
    let error = checked_error(
        failure,
        ErrorKind::Metadata,
        200,
        &format!("{}/api/v2/pokemon/clefairy/", server.url()),
    );
    assert_eq!(
        error.response().unwrap().headers()["content-type"],
        "text/plain"
    );
    assert_eq!(
        error
            .into_response()
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        CLEFAIRY
    );
    finish(server, &["/api/v2/pokemon/clefairy/"]).await;
}

#[tokio::test]
async fn a_incomplete_json_in_a_complete_http_body_is_a_decode_failure() {
    let server = TestServer::start(|_| json_reply(200, INCOMPLETE)).await;
    let failure = local(&server).pokemon("clefairy").await.unwrap_err();
    let error = checked_error(
        failure,
        ErrorKind::Decode,
        200,
        &format!("{}/api/v2/pokemon/clefairy/", server.url()),
    );
    assert!(error.codec_error().is_some());
    assert!(
        error.response().is_none(),
        "decoded body was already consumed"
    );
    assert_eq!(error.into_response().unwrap_err().kind(), ErrorKind::Decode);
    finish(server, &["/api/v2/pokemon/clefairy/"]).await;
}

#[tokio::test]
async fn a_complete_json_does_not_hide_an_unfinished_chunked_transfer() {
    let server = TestServer::start(|_| {
        json_reply(200, b"").with_body(Body::FailAfter(vec![CLEFAIRY.into()]))
    })
    .await;
    let failure = local(&server).pokemon("clefairy").await.unwrap_err();
    let error = checked_error(
        failure,
        ErrorKind::Transport,
        200,
        &format!("{}/api/v2/pokemon/clefairy/", server.url()),
    );
    assert!(error.transport().is_some());
    assert!(error.response().is_none());
    finish(server, &["/api/v2/pokemon/clefairy/"]).await;
}

#[tokio::test]
async fn a_disconnect_before_headers_has_no_response_metadata_or_retry() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let failure = local(&server).pokemon("clefairy").await.unwrap_err();
    let Error::Concord { error, meta } = failure else {
        panic!("expected transport error, got {failure:?}");
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(meta.is_none());
    assert_eq!(error.status(), None);
    assert!(error.response().is_none());
    assert!(error.transport().is_some());
    finish(server, &["/api/v2/pokemon/clefairy/"]).await;
}

// Application-owned traversal state. A failed call never replaces a saved page.
#[derive(Debug)]
enum Stop {
    Complete,
    PageLimit { next: String },
    Failed(Error),
}

#[derive(Debug)]
struct Selection {
    pages: Vec<Response<PokemonPage>>,
    details: Vec<Response<Pokemon>>,
    stop: Stop,
}

async fn select_named(api: &Client, name: &str, max_pages: usize) -> Selection {
    assert!(max_pages > 0);
    let mut selection = Selection {
        pages: Vec::new(),
        details: Vec::new(),
        stop: Stop::Complete,
    };
    let mut next: Option<String> = None;
    loop {
        let result = match next.as_deref() {
            Some(url) => api.follow(url).await,
            None => api.list(Some(2), Some(0)).await,
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                selection.stop = Stop::Failed(error);
                return selection;
            }
        };
        next = response.value.next.clone();
        selection.pages.push(response);
        for resource in &selection.pages.last().unwrap().value.results {
            if resource.name == name {
                match api.pokemon_url(&resource.url).await {
                    Ok(detail) => selection.details.push(detail),
                    Err(error) => {
                        selection.stop = Stop::Failed(error);
                        return selection;
                    }
                }
            }
        }
        let Some(url) = next.as_ref() else {
            return selection;
        };
        if selection.pages.len() == max_pages {
            selection.stop = Stop::PageLimit { next: url.clone() };
            return selection;
        }
    }
}

// Construct wire JSON independently of the SDK's deserialized model types.
fn page_reply(count: u64, next: Option<String>, previous: Option<String>, results: Value) -> Reply {
    json_reply(
        200,
        serde_json::to_vec(&json!({
            "count": count, "next": next, "previous": previous, "results": results
        }))
        .unwrap(),
    )
}

#[tokio::test]
async fn b_follow_empty_intermediate_page_and_fetch_only_selected_detail() {
    let server = TestServer::start(|request| {
        let base = format!("http://{}/api/v2/pokemon/", request.host);
        match request.target.as_str() {
            "/api/v2/pokemon/?limit=2&offset=0" => page_reply(
                3,
                Some(format!("{base}?offset=2&limit=1")),
                None,
                json!([
                    {"name":"bulbasaur", "url":format!("{base}1/")},
                    {"name":"ivysaur", "url":format!("{base}2/")}
                ]),
            ),
            "/api/v2/pokemon/?offset=2&limit=1" => page_reply(
                3,
                Some(format!("{base}?limit=3&offset=3")),
                Some(format!("{base}?limit=2&offset=0")),
                json!([]),
            ),
            "/api/v2/pokemon/?limit=3&offset=3" => page_reply(
                3,
                None,
                Some(format!("{base}?offset=2&limit=1")),
                json!([{"name":"clefairy", "url":format!("{base}35/")}]),
            ),
            "/api/v2/pokemon/35/" => json_reply(200, CLEFAIRY),
            _ => json_reply(500, UNAVAILABLE),
        }
    })
    .await;
    // Hitting the bound exactly is still complete when the final next is null.
    let selection = select_named(&local(&server), "clefairy", 3).await;
    assert!(matches!(selection.stop, Stop::Complete));
    assert_eq!(selection.pages.len(), 3);
    assert_eq!(selection.pages[0].value.count, 3);
    assert_eq!(selection.pages[0].value.previous, None);
    assert!(selection.pages[1].value.results.is_empty());
    assert!(selection.pages[1].value.next.is_some());
    assert_eq!(selection.pages[2].value.next, None);
    assert_eq!(
        selection.pages[2].value.previous.as_deref(),
        Some(selection.pages[1].meta.url.as_str())
    );
    let listed_names: Vec<_> = selection
        .pages
        .iter()
        .flat_map(|response| response.value.results.iter().map(|r| r.name.as_str()))
        .collect();
    assert_eq!(listed_names, ["bulbasaur", "ivysaur", "clefairy"]);
    assert_eq!(selection.details.len(), 1);
    assert_eq!(selection.details[0].value.id, 35);
    assert_eq!(selection.details[0].value.name, "clefairy");
    for page in &selection.pages {
        assert_eq!(page.meta.status, reqwest::StatusCode::OK);
        assert_eq!(page.meta.headers["x-fixture"], "first-use");
    }
    assert_eq!(
        selection.details[0].meta.url.as_str(),
        format!("{}/api/v2/pokemon/35/", server.url())
    );
    finish(
        server,
        &[
            "/api/v2/pokemon/?limit=2&offset=0",
            "/api/v2/pokemon/?offset=2&limit=1",
            "/api/v2/pokemon/?limit=3&offset=3",
            "/api/v2/pokemon/35/",
        ],
    )
    .await;
}

#[tokio::test]
async fn b_provider_defaults_and_an_empty_final_page_require_no_followup() {
    let server = TestServer::start(|_| page_reply(0, None, None, json!([]))).await;
    let api = local(&server);
    let default_page = api.list(None, None).await.unwrap();
    assert_eq!(default_page.value.count, 0);
    assert!(default_page.value.results.is_empty());
    assert_eq!(default_page.value.next, None);
    assert_eq!(default_page.value.previous, None);

    let selection = select_named(&api, "clefairy", 1).await;
    assert!(matches!(selection.stop, Stop::Complete));
    assert_eq!(selection.pages.len(), 1);
    assert!(selection.details.is_empty());
    finish(
        server,
        &["/api/v2/pokemon/", "/api/v2/pokemon/?limit=2&offset=0"],
    )
    .await;
}

#[tokio::test]
async fn b_failed_next_page_keeps_prior_page_detail_and_error_document() {
    let server = TestServer::start(|request| {
        let base = format!("http://{}/api/v2/pokemon/", request.host);
        match request.target.as_str() {
            "/api/v2/pokemon/?limit=2&offset=0" => page_reply(
                2,
                Some(format!("{base}?offset=1&limit=1")),
                None,
                json!([{"name":"clefairy", "url":format!("{base}35/")}]),
            ),
            "/api/v2/pokemon/35/" => json_reply(200, CLEFAIRY),
            "/api/v2/pokemon/?offset=1&limit=1" => {
                json_reply(503, UNAVAILABLE).header("retry-after", "7")
            }
            _ => json_reply(500, UNAVAILABLE),
        }
    })
    .await;
    let selection = select_named(&local(&server), "clefairy", 5).await;
    assert_eq!(selection.pages.len(), 1);
    assert_eq!(selection.pages[0].value.results[0].name, "clefairy");
    assert_eq!(selection.details.len(), 1);
    assert_eq!(selection.details[0].value.id, 35);
    let next = selection.pages[0].value.next.as_deref().unwrap();
    let Stop::Failed(failure) = selection.stop else {
        panic!("a rejected continuation must not be reported as complete");
    };
    let error = checked_error(failure, ErrorKind::Status, 503, next);
    assert_eq!(error.response().unwrap().headers()["retry-after"], "7");
    let body = error.into_response().unwrap().bytes().await.unwrap();
    assert_eq!(body.as_ref(), UNAVAILABLE);
    // This is our synthetic failure document, not a promised PokéAPI error schema.
    let document: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(document["retryable"], true);
    finish(
        server,
        &[
            "/api/v2/pokemon/?limit=2&offset=0",
            "/api/v2/pokemon/35/",
            "/api/v2/pokemon/?offset=1&limit=1",
        ],
    )
    .await;
}

#[tokio::test]
async fn b_selected_detail_failure_keeps_the_list_and_is_not_completion() {
    let server = TestServer::start(|request| match request.target.as_str() {
        "/api/v2/pokemon/?limit=2&offset=0" => page_reply(
            1,
            None,
            None,
            json!([{
                "name":"clefairy",
                "url":format!("http://{}/api/v2/pokemon/35/", request.host)
            }]),
        ),
        "/api/v2/pokemon/35/" => {
            Reply::text(404, "text/plain", NOT_FOUND).header("x-fixture", "first-use")
        }
        _ => json_reply(500, UNAVAILABLE),
    })
    .await;
    let selection = select_named(&local(&server), "clefairy", 2).await;
    assert_eq!(selection.pages.len(), 1);
    assert_eq!(selection.pages[0].value.next, None);
    assert!(selection.details.is_empty());
    let Stop::Failed(failure) = selection.stop else {
        panic!("list exhaustion does not mean selected detail succeeded");
    };
    let error = checked_error(
        failure,
        ErrorKind::Status,
        404,
        &selection.pages[0].value.results[0].url,
    );
    assert_eq!(
        error
            .into_response()
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        NOT_FOUND
    );
    finish(
        server,
        &["/api/v2/pokemon/?limit=2&offset=0", "/api/v2/pokemon/35/"],
    )
    .await;
}

#[tokio::test]
async fn b_truncated_next_page_keeps_progress_without_publishing_partial_json() {
    let server = TestServer::start(|request| {
        let base = format!("http://{}/api/v2/pokemon/", request.host);
        match request.target.as_str() {
            "/api/v2/pokemon/?limit=2&offset=0" => page_reply(
                2,
                Some(format!("{base}?offset=1&limit=1")),
                None,
                json!([{"name":"clefairy", "url":format!("{base}35/")}]),
            ),
            "/api/v2/pokemon/35/" => json_reply(200, CLEFAIRY),
            "/api/v2/pokemon/?offset=1&limit=1" => {
                let bytes = serde_json::to_vec(&json!({
                    "count":2, "next":null, "previous":format!("{base}?limit=2&offset=0"),
                    "results":[{"name":"ditto", "url":format!("{base}132/")}]
                }))
                .unwrap();
                json_reply(200, b"").with_body(Body::FailAfter(vec![bytes.into()]))
            }
            _ => json_reply(500, UNAVAILABLE),
        }
    })
    .await;
    let selection = select_named(&local(&server), "clefairy", 5).await;
    assert_eq!(selection.pages.len(), 1);
    assert_eq!(selection.details.len(), 1);
    assert_eq!(selection.details[0].value.name, "clefairy");
    let Stop::Failed(failure) = selection.stop else {
        panic!("a page with unfinished HTTP framing must fail");
    };
    let error = checked_error(
        failure,
        ErrorKind::Transport,
        200,
        selection.pages[0].value.next.as_deref().unwrap(),
    );
    assert!(error.response().is_none());
    finish(
        server,
        &[
            "/api/v2/pokemon/?limit=2&offset=0",
            "/api/v2/pokemon/35/",
            "/api/v2/pokemon/?offset=1&limit=1",
        ],
    )
    .await;
}

#[tokio::test]
async fn b_application_page_bound_stops_a_cycle_with_a_continuation() {
    let server = TestServer::start(|request| {
        page_reply(
            1,
            Some(format!(
                "http://{}/api/v2/pokemon/?offset=0&limit=2",
                request.host
            )),
            None,
            json!([]),
        )
    })
    .await;
    let selection = select_named(&local(&server), "clefairy", 2).await;
    assert_eq!(selection.pages.len(), 2);
    assert!(selection.details.is_empty());
    let Stop::PageLimit { next } = selection.stop else {
        panic!("the local page bound is distinct from failure and exhaustion");
    };
    assert_eq!(
        next,
        format!("{}/api/v2/pokemon/?offset=0&limit=2", server.url())
    );
    assert_eq!(
        selection.pages[1].value.next.as_deref(),
        Some(next.as_str())
    );
    finish(
        server,
        &[
            "/api/v2/pokemon/?limit=2&offset=0",
            "/api/v2/pokemon/?offset=0&limit=2",
        ],
    )
    .await;
}

#[tokio::test]
async fn b_invalid_continuation_and_detail_urls_are_rejected_without_io() {
    // A second loopback server makes the foreign-origin check offline even if it regresses.
    let server = TestServer::start(|_| json_reply(500, UNAVAILABLE)).await;
    let foreign = TestServer::start(|_| json_reply(500, UNAVAILABLE)).await;
    let api = local(&server);
    let base = server.url();
    let invalid_lists = [
        format!("{}/api/v2/pokemon/?offset=2&limit=1", foreign.url()),
        format!("{base}/api/v2/ability/?offset=2&limit=1"),
        format!("{base}/api/v2/pokemon/35/"),
        format!("{base}/api/v2/pokemon/?offset=2#fragment"),
        format!("http://synthetic-user@{}/api/v2/pokemon/", server.addr()),
        "/api/v2/pokemon/?offset=2".to_owned(),
        "not a URL".to_owned(),
    ];
    for url in invalid_lists {
        assert!(
            matches!(api.follow(&url).await, Err(Error::InvalidUrl(_))),
            "{url}"
        );
    }
    let invalid_details = [
        format!("{}/api/v2/pokemon/35/", foreign.url()),
        format!("{base}/api/v2/pokemon-species/35/"),
        format!("{base}/api/v2/pokemon/"),
        format!("{base}/api/v2/pokemon/35/encounters"),
        format!("{base}/api/v2/pokemon/35/#fragment"),
        format!("http://synthetic-user@{}/api/v2/pokemon/35/", server.addr()),
        "/api/v2/pokemon/35/".to_owned(),
        "not a URL".to_owned(),
    ];
    for url in invalid_details {
        assert!(
            matches!(api.pokemon_url(&url).await, Err(Error::InvalidUrl(_))),
            "{url}"
        );
    }
    finish(server, &[]).await;
    finish(foreign, &[]).await;
}
