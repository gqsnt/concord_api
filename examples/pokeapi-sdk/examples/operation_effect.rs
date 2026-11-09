//! Offline caller: the simple case keeps its existing shape.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_pokeapi::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = TestServer::start(|request: &Recorded| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/pokemon/clefairy/");
        Reply::json(200, include_str!("../fixtures/first_use/clefairy.json"))
    })
    .await;
    let api = Client::new(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
    )
    .with_base_url(server.url())?;

    let response = api.pokemon("clefairy").send().await?;
    println!("{}: {}", response.value.id, response.value.name);

    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    Ok(())
}
