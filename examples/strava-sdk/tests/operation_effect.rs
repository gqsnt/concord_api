//! Operation effects survive the public carriers.
mod support;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_strava::{
    ActivityUpdate,
    concord::{Endpoint, ErrorKind},
};

fn prepared_effect<T, E: Endpoint<T, concord_sdk_strava::ErrorDocument>>(
    endpoint: E,
    expected: bool,
) {
    let declared = E::operation();
    assert_eq!(declared.is_read(), expected);
    let prepared = endpoint.prepare().unwrap();
    let carried = prepared.operation().unwrap();
    assert_eq!(carried, declared);
    assert_eq!(carried.is_read(), expected);
    let (_, _, decoder) = prepared.into_parts();
    assert_eq!(decoder.operation().unwrap().is_read(), expected);
}

#[test]
fn preparation_and_native_escape_keep_the_effect_without_sending() {
    let api = support::api("http://127.0.0.1:1/");
    prepared_effect(api.activities().get(7), true);
    prepared_effect(
        api.activities()
            .update(7, ActivityUpdate::new().name("new name".into())),
        false,
    );
    prepared_effect(api.uploads().create(support::file()), false);
}

async fn received_and_decode_error_effect<
    T: std::fmt::Debug,
    E: Endpoint<T, concord_sdk_strava::ErrorDocument>,
>(
    endpoint: E,
    expected: bool,
) {
    let received = endpoint.prepare().unwrap().receive().await.unwrap();
    assert_eq!(received.operation().unwrap().is_read(), expected);
    assert_eq!(received.meta().operation().unwrap().is_read(), expected);
    let error = received.decode().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert_eq!(error.operation().unwrap().is_read(), expected);
    assert_eq!(
        error.head_meta().unwrap().operation().unwrap().is_read(),
        expected
    );
}

#[tokio::test]
async fn read_and_write_effects_survive_receiving_and_body_failure() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, "{")).await;
    let api = support::api(&server.url());
    received_and_decode_error_effect(api.activities().get(7), true).await;
    received_and_decode_error_effect(
        api.activities()
            .update(7, ActivityUpdate::new().name("new name".into())),
        false,
    )
    .await;
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
