//! The unchanged declaration and KeepHead SendWith policy: both GETs read, no caller changes.
use concord::{Endpoint, reqwest};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_pokeapi::{Client, PokeApiList, PokeApiPokemon, Pokemon, PokemonPage, Response};

#[test]
fn an_unannotated_client_gets_read_descriptors() {
    for operation in [
        <PokeApiPokemon as Endpoint<Pokemon>>::operation(),
        <PokeApiList as Endpoint<PokemonPage>>::operation(),
    ] {
        assert_eq!(operation.method(), Some("GET"));
        assert!(operation.is_read());
    }
}

#[tokio::test]
async fn the_existing_sendwith_policy_keeps_its_output_and_dispatch_count() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, include_str!("../fixtures/first_use/clefairy.json"))
    })
    .await;
    let api = Client::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let endpoint = api.pokemon("clefairy");
    assert!(
        endpoint
            .clone()
            .prepare()
            .unwrap()
            .operation()
            .unwrap()
            .is_read()
    );
    let response: Response<Pokemon> = endpoint.send().await.unwrap();
    assert_eq!(response.value.name, "clefairy");
    assert_eq!(response.meta.status.as_u16(), 200);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
