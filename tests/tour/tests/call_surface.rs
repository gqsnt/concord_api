//! Client-generic code exercised against both independent implementations.
use tour::{riot, Europe, MatchId, Puuid, Riot};

async fn timed<E: riot::Endpoint>(call: riot::Call<E>) -> riot::Result<E::Value> {
    let start = std::time::Instant::now();
    let result = call.send().await;
    let _elapsed = start.elapsed();
    result
}

#[tokio::test]
async fn c1_11_timed_accepts_paged_and_unpaged_endpoints_with_distinct_values() -> riot::Result<()>
{
    let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
        let body = if request.url().path().ends_with("/ids") {
            r#"["EUW1_1"]"#
        } else {
            assert!(request.url().path().ends_with("/accounts/me"));
            r#"{"puuid":"p-1","gameName":"Faker","tagLine":"KR1"}"#
        };
        Ok(concord::reply(200, body))
    }));
    let ids: Vec<MatchId> = timed(riot.matches(Europe).ids(Puuid::from("p-1"))).await?;
    assert_eq!(
        ids.into_iter().map(|id| id.0).collect::<Vec<_>>(),
        ["EUW1_1"]
    );
    let account = timed(riot.account(Europe).me("player-token")).await?;
    assert_eq!(account.puuid.0, "p-1");
    Ok(())
}
