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

/// What `info.endpoint()` reported for each try, in order.
#[derive(Clone, Default)]
struct Reported(std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>);

impl concord::Middleware for Reported {
    async fn handle(
        &self,
        _request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        _next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(info.endpoint());
        Ok(concord::reply(200, "[]"))
    }
}

// A generic key for limits and logs, read from the endpoint's type alone.
fn key<E: riot::Endpoint>(_: &riot::Call<E>) -> &'static str {
    E::NAME
}

#[tokio::test]
async fn c6_4_c9_5_endpoint_name_is_the_text_that_try_info_reports() {
    use tour::{tasks, Tasks};

    let reported = Reported::default();
    let tasks = Tasks::new().with_middleware(reported.clone());
    tasks.list().await.unwrap();
    let riot = Riot::new("test-key").with_middleware(reported.clone());
    let ids = riot.matches(Europe).ids(Puuid::from("p-1"));
    assert_eq!(key(&ids), "riot.matches.ids");
    ids.send().await.unwrap();

    assert_eq!(<tasks::List as tasks::Endpoint>::NAME, "tasks.list");
    assert_eq!(
        <riot::matches::Ids as riot::Endpoint>::NAME,
        "riot.matches.ids"
    );
    assert_eq!(
        *reported.0.lock().unwrap(),
        [
            <tasks::List as tasks::Endpoint>::NAME,
            <riot::matches::Ids as riot::Endpoint>::NAME,
        ]
    );
}
