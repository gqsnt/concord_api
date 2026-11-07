//! Finite endpoint limits also apply outside the provider send policy.

mod common;

use common::{client, filter};
use concord_sdk_usgs_earthquake_catalog::{EventId, SUCCESS_BODY_LIMIT, concord::ErrorKind};
use concord_testserver::{Recorded, Reply, TestServer};

#[tokio::test]
async fn prepared_geojson_and_event_sends_are_bounded() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(200, vec![b' '; SUCCESS_BODY_LIMIT + 1]))
            .await;
    let api = client(&server);
    let query = api.catalog().query(&filter(3));
    let event = api.catalog().event(EventId::new("fixture").unwrap());
    assert_eq!(
        query.geojson().prepare().unwrap().into_parts().2.limit(),
        Some(SUCCESS_BODY_LIMIT)
    );
    assert_eq!(
        event.clone().prepare().unwrap().into_parts().2.limit(),
        Some(SUCCESS_BODY_LIMIT)
    );
    assert_eq!(query.csv().prepare().unwrap().into_parts().2.limit(), None);
    assert_eq!(
        query
            .geojson()
            .prepare()
            .unwrap()
            .send()
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Limit
    );
    assert_eq!(
        event.prepare().unwrap().send().await.unwrap_err().kind(),
        ErrorKind::Limit
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
