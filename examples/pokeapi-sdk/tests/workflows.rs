//! Public consumer workflows, entirely against the loopback server.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_pokeapi::Client;

fn response(body: String) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json")
        .body(body)
}
fn api(server: &TestServer) -> Client {
    Client::new(reqwest::Client::builder().no_proxy().build().unwrap())
        .with_base_url(format!("http://{}/api/v2", server.addr()))
        .unwrap()
}

#[tokio::test]
async fn read_named_pokemon() {
    let server =
        TestServer::start(|_: &Recorded| response(include_str!("../fixtures/pokemon.json").into()))
            .await;
    let pokemon = api(&server).pokemon("clefairy").await.unwrap().value;
    assert_eq!(
        (pokemon.id, pokemon.name.as_str(), pokemon.height),
        (35, "clefairy", 6)
    );
    assert_eq!(server.single().target, "/api/v2/pokemon/clefairy/");
    server.shutdown().await;
}

#[tokio::test]
async fn change_limit_follow_next_and_fetch_detail() {
    let server = TestServer::start(|r: &Recorded| {
        let body = match r.target.as_str() {
            "/api/v2/pokemon/?limit=2&offset=0" => include_str!("../fixtures/page-first.json"),
            "/api/v2/pokemon/?offset=1&limit=1" => include_str!("../fixtures/page-last.json"),
            "/api/v2/pokemon/?limit=1&offset=2" => include_str!("../fixtures/page-empty.json"),
            "/api/v2/pokemon/35/" => include_str!("../fixtures/pokemon.json"),
            _ => panic!("unexpected target {}", r.target),
        };
        response(body.replace("HOST", &r.host))
    })
    .await;
    let api = api(&server);
    let first = api.list(Some(2), Some(0)).await.unwrap().value;
    let next = first.next.as_deref().unwrap();
    let last = api.follow(next).await.unwrap().value;
    assert!(last.next.is_none());
    let selected = api.pokemon_url(&last.results[0].url).await.unwrap().value;
    assert_eq!(selected.id, 35);
    let empty = api.list(Some(1), Some(2)).await.unwrap().value;
    assert!(empty.results.is_empty() && empty.next.is_none());
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [
            "/api/v2/pokemon/?limit=2&offset=0",
            "/api/v2/pokemon/?offset=1&limit=1",
            "/api/v2/pokemon/35/",
            "/api/v2/pokemon/?limit=1&offset=2"
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn follow_to_empty_end_and_keep_prefix_on_failure() {
    let server = TestServer::start(|r: &Recorded| match r.target.as_str() {
        "/api/v2/pokemon/?limit=1" => {
            response(include_str!("../fixtures/page-first.json").replace("HOST", &r.host))
        }
        "/api/v2/pokemon/?offset=1&limit=1" => {
            response(include_str!("../fixtures/page-empty.json").replace("HOST", &r.host))
        }
        _ => Reply::new(503)
            .header("content-type", "text/plain")
            .body("unavailable"),
    })
    .await;
    let api = api(&server);
    let first = api.list(Some(1), None).await.unwrap().value;
    let final_page = api
        .follow(first.next.as_deref().unwrap())
        .await
        .unwrap()
        .value;
    assert!(final_page.results.is_empty() && final_page.next.is_none());
    let failed_url = format!("http://{}/api/v2/pokemon/?limit=1&offset=50", server.addr());
    let failure = api.follow(&failed_url).await.unwrap_err();
    assert!(matches!(
        failure,
        concord_sdk_pokeapi::Error::Concord { meta: Some(_), .. }
    ));
    assert_eq!(first.results[0].name, "clefairy");
    server.shutdown().await;
}
