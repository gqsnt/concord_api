//! Destinations are chosen before credentials; rotations keep the same origin.
use concord_sdk_digitalocean::{Destination, DigitalOcean, DigitalOceanConfig, Error};
use concord_testserver::{Reply, TestServer};

#[test]
fn destinations_reject_external_fixture_routes() {
    for input in [
        "https://api.digitalocean.com",
        "http://evil.example",
        "http://localhost",
        "http://127.0.0.1:0",
        "http://127.0.0.1/path",
        "http://user:secret@127.0.0.1",
        "http://127.0.0.1?secret",
        "http://127.0.0.1#secret",
    ] {
        assert!(
            matches!(Destination::for_fixture(input), Err(Error::FixtureOrigin)),
            "{input}"
        );
    }
    assert!(Destination::for_fixture("http://[::1]:8080").is_ok());
    let api = DigitalOcean::new(
        concord_sdk_digitalocean::concord::reqwest::Client::new(),
        DigitalOceanConfig {
            destination: Destination::production(),
            authorization: concord_auth::bearer("old").unwrap(),
        },
    );
    assert_eq!(
        api.droplets()
            .list()
            .prepare()
            .unwrap()
            .request()
            .url()
            .as_str(),
        "https://api.digitalocean.com/v2/droplets"
    );
}

#[tokio::test]
async fn rotation_preserves_destination_and_earlier_request_snapshot() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let api = DigitalOcean::for_fixture("old", &server.url()).unwrap();
    let before = api.droplet(1).get();
    let rotated = api.set_authorization(concord_auth::bearer("fresh").unwrap());
    before.prepare().unwrap().send_raw().await.unwrap();
    rotated
        .droplet(1)
        .get()
        .prepare()
        .unwrap()
        .send_raw()
        .await
        .unwrap();
    let wire = server.requests();
    assert_eq!(wire.len(), 2);
    assert_eq!(wire[0].target, "/v2/droplets/1");
    assert_eq!(wire[1].target, wire[0].target);
    assert_eq!(wire[0].header("authorization"), Some("Bearer old"));
    assert_eq!(wire[1].header("authorization"), Some("Bearer fresh"));
    server.shutdown().await;
}
