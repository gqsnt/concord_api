//! The application's HTTP policy keeps the key on Riot's origin: a redirect to another origin is
//! never followed. Two loopback origins and the fixture key; no real credential.

mod support;

use concord::ErrorKind;
use concord::reqwest::{Client, ClientBuilder};
use concord_example_riot::*;
use support::{KEY, Reply, Server};

/// The routed Riot hosts resolved to `origin`, with the given policy applied (as `riot_http` does
/// for the other tests, minus its own timeout).
fn client(origin: &Server, policy: impl FnOnce(ClientBuilder) -> ClientBuilder) -> Riot {
    let builder = Client::builder()
        .no_proxy()
        .resolve("americas.api.riotgames.com", origin.addr);
    let port = origin.addr.port();
    Riot::new(
        policy(builder).build().expect("client"),
        RiotConfig { key: KEY.into() },
    )
    .with_base_url(format!("http://api.riotgames.com:{port}"))
    .unwrap()
}

/// Riot's host answers `302` to a URL on a second origin, which records what reaches it.
async fn origins() -> (Server, Server) {
    let elsewhere = Server::reply(Reply::json(200, r#"{"puuid":"x"}"#)).await;
    let location = format!("{}/collect", elsewhere.url());
    let origin = Server::reply(Reply::new(302).header("location", &location)).await;
    (origin, elsewhere)
}

#[tokio::test]
async fn send_reports_the_redirect_and_the_second_origin_receives_nothing() {
    let (origin, elsewhere) = origins().await;
    let error = client(&origin, http_policy)
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .send()
        .await
        .unwrap_err();
    // Not followed: an ordinary status error, holding the unread 302.
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(302));
    assert!(!format!("{error} {error:?}").contains(KEY));
    let response = error.into_response().unwrap();
    assert!(response.headers().contains_key("location"));
    assert_eq!(origin.requests().len(), 1);
    assert_eq!(origin.single().header("x-riot-token"), Some(KEY));
    assert!(
        elsewhere.requests().is_empty(),
        "{:#?}",
        elsewhere.requests()
    );
}

#[tokio::test]
async fn send_raw_returns_the_302_unfollowed() {
    let (origin, elsewhere) = origins().await;
    let response = client(&origin, http_policy)
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .prepare()
        .unwrap()
        .send_raw()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 302);
    assert_eq!(origin.requests().len(), 1);
    assert!(elsewhere.requests().is_empty());
}

/// The hazard the policy removes: reqwest's default policy follows the redirect and, because
/// `X-Riot-Token` is not on its cross-origin strip list, sends the key to the second origin.
/// (If reqwest ever strips it, this control fails and the comment on `http_policy` must change.)
#[tokio::test]
async fn control_the_default_policy_forwards_the_key_to_the_second_origin() {
    let (origin, elsewhere) = origins().await;
    let account = client(&origin, |b| b)
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .send()
        .await
        .unwrap();
    assert_eq!(account.puuid, "x".into());
    let forwarded = elsewhere.single();
    assert_eq!(forwarded.path(), "/collect");
    assert_eq!(forwarded.header("x-riot-token"), Some(KEY));
}

/// The live client is built from the same policy and refuses plain HTTP. reqwest checks the scheme
/// before any name lookup or connection; the host is a reserved `.invalid` name all the same.
#[tokio::test]
async fn the_live_client_refuses_plain_http() {
    let (origin, elsewhere) = origins().await;
    let port = origin.addr.port();
    let riot = Riot::new(live_http_client().unwrap(), RiotConfig { key: KEY.into() })
        .with_base_url(format!("http://api.riotgames.invalid:{port}"))
        .unwrap();
    let error = riot
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(origin.requests().is_empty());
    assert!(elsewhere.requests().is_empty());
}
