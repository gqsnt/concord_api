//! Generic call code can send any endpoint of the tour's Riot client.

use concord_testkit::Script;
use std::time::Instant;
use tour_gen::{files, riot, Europe, Puuid, Riot};

async fn timed<E: riot::Endpoint>(call: riot::Call<E>) -> riot::Result<E::Value> {
    let started = Instant::now();
    let result = call.send().await;
    let _elapsed = started.elapsed();
    result
}

#[tokio::test]
async fn c1_11_one_function_sends_two_endpoints() -> riot::Result<()> {
    let wire = Script::new();
    wire.answer(200)
        .body(r#"{"puuid":"p-1","gameName":"Ada","tagLine":"EUW"}"#);
    wire.answer(200).body(r#"["EUW1_10"]"#);
    let riot = Riot::new("test-key").with_transport(wire.clone());

    let account = timed(riot.account(Europe).by_riot_id("Ada", "EUW")).await?;
    assert_eq!(
        account.map(|account| account.game_name).as_deref(),
        Some("Ada")
    );

    let ids = timed(riot.matches(Europe).ids(Puuid::from("p-1")).count(3)).await?;
    assert_eq!(ids.len(), 1);
    let requests = wire.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .any(|request| request.url().ends_with("/by-riot-id/Ada/EUW")));
    assert!(requests
        .iter()
        .any(|request| request.url().ends_with("/by-puuid/p-1/ids?count=3")));
    Ok(())
}

#[test]
fn c8_8_all_unpaged_files_calls_can_be_awaited() {
    use std::future::IntoFuture;

    fn assert_awaitable<E: files::Endpoint>()
    where
        files::Call<E>: IntoFuture<Output = files::Result<E::Value>>,
    {
    }

    assert_awaitable::<files::Download>();
    assert_awaitable::<files::Exists>();
    assert_awaitable::<files::Grant>();
    assert_awaitable::<files::Readme>();
    assert_awaitable::<files::Search>();
    assert_awaitable::<files::Upload>();
}
