//! Offline connected workflow tests.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_nasa_neows::{NeoWs, NeoWsConfig};

fn client(server: &TestServer) -> NeoWs {
    NeoWs::new(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        NeoWsConfig {
            api_key: "synthetic-secret".into(),
        },
    )
    .with_base_url(format!("{}/neo/rest/v1", server.url()))
    .unwrap()
}

#[tokio::test]
async fn feed_then_lookup() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/neo/rest/v1/feed" => Reply::json(200, include_str!("../fixtures/feed.json")),
        "/neo/rest/v1/neo/3542519" => {
            Reply::json(200, include_str!("../fixtures/lookup-feed.json"))
        }
        _ => Reply::new(404),
    })
    .await;
    let api = client(&server);
    let neo = api.neows();
    let feed = neo.feed("2015-09-07", "2015-09-08").send().await.unwrap();
    assert_eq!(feed.element_count, 1);
    let id = &feed.near_earth_objects["2015-09-07"][0].id;
    let detail = neo.lookup(id).send().await.unwrap();
    assert_eq!(detail.id, *id);
    assert_eq!(detail.name, "Synthetic Alpha");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn browse_then_inspect_and_key_failure() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/neo/rest/v1/neo/browse" => Reply::json(200, include_str!("../fixtures/browse.json")),
        "/neo/rest/v1/neo/2000433" if r.query_all("api_key") == ["synthetic-secret"] => {
            Reply::json(200, include_str!("../fixtures/lookup-browse.json"))
        }
        _ => Reply::json(403, include_str!("../fixtures/key-error.json")),
    })
    .await;
    let api = client(&server);
    let neo = api.neows();
    let page = neo.browse().set_page(0).set_size(1).send().await.unwrap();
    assert_eq!(page.page.total_pages, 2);
    let id = &page.near_earth_objects[0].id;
    let detail = neo.lookup(id).send().await.unwrap();
    assert_eq!(detail.name, "Synthetic Eros");
    assert_eq!(&detail.id, id);
    let bad_api = api.set_api_key("invalid-secret");
    let err = bad_api.neows().lookup(id).send().await.unwrap_err();
    assert_eq!(err.status.unwrap().as_u16(), 403);
    assert!(!format!("{err:?} {err}").contains("invalid-secret"));
    assert!(!format!("{err:?} {err}").contains("synthetic-secret"));
    assert_eq!(
        server.requests()[2].query_all("api_key"),
        ["invalid-secret"]
    );
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}
