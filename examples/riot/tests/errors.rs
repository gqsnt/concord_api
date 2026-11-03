//! Non-success responses (Riot promises no JSON body for them), rate limiting, and the key never
//! appearing in Concord's output.

mod support;

use concord::ErrorKind;
use concord_example_riot::*;
use support::{KEY, riot};
use support::{Reply, Server, fixture};

#[tokio::test]
async fn non_success_bodies_are_kept_unread() {
    let cases = [
        (
            403,
            Reply::new(403)
                .header("content-type", "application/json;charset=utf-8")
                .body(fixture("error_403.json")),
            "Forbidden",
        ),
        (404, Reply::new(404), ""),
        (
            503,
            Reply::new(503)
                .header("content-type", "text/html")
                .body(fixture("error_503.html")),
            "503 Service",
        ),
    ];
    for (status, reply, text) in cases {
        let server = Server::reply(reply).await;
        let error = riot(&server)
            .account(AccountRoute::Americas)
            .by_riot_id("Fixture Player", "EUW")
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Status);
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status));
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains(KEY), "{shown}");
        let body = error.into_response().unwrap().text().await.unwrap();
        assert!(body.contains(text), "{body}");
    }
}

#[tokio::test]
async fn a_429_stops_the_workflow_with_retry_after() {
    let server = Server::reply(
        Reply::new(429)
            .header("retry-after", "7")
            .header("x-rate-limit-type", "application"),
    )
    .await;
    let stop = player_summary(
        &riot(&server),
        "Fixture Player",
        "EUW",
        AccountRoute::Europe,
        Platform::Euw1,
        MatchRoute::Europe,
        3,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            stop,
            Stop::RateLimited {
                retry_after: Some(7)
            }
        ),
        "{stop:?}"
    );
    // No retry: one request, then the workflow stopped.
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_429_without_retry_after_still_stops() {
    let server = Server::reply(Reply::new(429)).await;
    let stop = player_summary(
        &riot(&server),
        "x",
        "y",
        AccountRoute::Asia,
        Platform::Kr,
        MatchRoute::Asia,
        1,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(stop, Stop::RateLimited { retry_after: None }),
        "{stop:?}"
    );
}

/// A 200 that is not JSON (a proxy's HTML page) is refused by `Json`'s head check before its body
/// is read, and stays readable; a body labelled with Riot's JSON type that is not JSON is a decode
/// error. Neither prints the key.
#[tokio::test]
async fn non_json_success_responses_are_refused_or_fail_to_decode() {
    let page = Reply::new(200)
        .header("content-type", "text/html")
        .body("<html>not json</html>");
    let server = Server::reply(page).await;
    let error = riot(&server)
        .platform(Platform::Na1)
        .rotation()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert!(!format!("{error} {error:?}").contains(KEY));
    let page = error.into_response().unwrap().text().await.unwrap();
    assert_eq!(page, "<html>not json</html>");

    let mislabelled = Reply::new(200)
        .header("content-type", "application/json;charset=utf-8")
        .body("<html>not json</html>");
    let server = Server::reply(mislabelled).await;
    let error = riot(&server)
        .platform(Platform::Na1)
        .rotation()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(!format!("{error} {error:?}").contains(KEY));
}

#[test]
fn the_key_and_access_tokens_are_withheld_from_debug() {
    let client = Riot::new(
        concord::reqwest::Client::new(),
        RiotConfig { key: KEY.into() },
    );
    let shown = format!("{client:?}");
    assert!(
        !shown.contains(KEY) && shown.contains("<redacted>"),
        "{shown}"
    );
    let prepared = client
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .prepare()
        .unwrap();
    let shown = format!("{prepared:?}");
    assert!(!shown.contains(KEY), "{shown}");
    // Positive control: the key is really sent, with the native sensitivity flag.
    let header = &prepared.request().headers()["x-riot-token"];
    assert_eq!(header.as_bytes(), KEY.as_bytes());
    assert!(header.is_sensitive());

    let me = client
        .account(AccountRoute::Europe)
        .me("synthetic-access-token");
    assert!(
        !format!("{me:?}").contains("synthetic-access-token"),
        "{me:?}"
    );
    let prepared = me.prepare().unwrap();
    assert!(!format!("{prepared:?}").contains("synthetic-access-token"));
    assert!(prepared.request().headers()["authorization"].is_sensitive());
}
