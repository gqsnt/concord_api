#![allow(missing_docs)]

mod common;

use concord_example_testserver::TestServer;
use concord_sdk_frankfurter::{Date, Decimal, Error, Rate};

use common::{HISTORICAL, LATEST, assert_wire, client, json};

#[tokio::test]
async fn workflow_a_read_latest_pair_with_exact_decimal_and_observation_date() {
    let server = TestServer::start(|_| json(200, LATEST)).await;
    let observation = client(&server)
        .latest_pair("EUR", "USD")
        .send()
        .await
        .unwrap();
    assert_eq!(observation.rate.base, "EUR");
    assert_eq!(observation.rate.quote, "USD");
    assert_eq!(observation.rate.date, "2026-03-25".parse::<Date>().unwrap());
    assert_eq!(observation.rate.rate, "1.1568".parse::<Decimal>().unwrap());
    assert_wire(&server.single(), "/v2/rate/EUR/USD");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn workflow_b_read_historical_pair_without_substituting_the_requested_date() {
    // This synthetic carried observation checks that a caller gets the provider's date.
    let server = TestServer::start(|_| json(200, HISTORICAL)).await;
    let requested: Date = "2024-01-16".parse().unwrap();
    let observation = client(&server)
        .historical_pair("EUR", "USD", requested)
        .send()
        .await
        .unwrap();
    assert_eq!(observation.rate.date, "2024-01-15".parse::<Date>().unwrap());
    assert_eq!(observation.rate.rate, "1.089".parse::<Decimal>().unwrap());
    assert_eq!(
        (
            observation.rate.base.as_str(),
            observation.rate.quote.as_str()
        ),
        ("EUR", "USD")
    );
    assert_wire(&server.single(), "/v2/rate/EUR/USD?date=2024-01-16");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn optional_filter_and_unknown_fields_keep_existing_rate_callers_working() {
    let server =
        TestServer::start(|_| json(200, include_bytes!("../fixtures/extended.json"))).await;
    let api = client(&server);
    let original = api.latest_pair("EUR", "USD");
    let plain = original.clone().send().await.unwrap();
    let filtered = original.set_providers("ECB").send().await.unwrap();
    let historical = api
        .historical_pair("EUR", "USD", "2026-03-25".parse::<Date>().unwrap())
        .set_providers("ECB")
        .send()
        .await
        .unwrap();
    assert_eq!(plain.rate, filtered.rate);
    assert_eq!(plain.rate, historical.rate);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_wire(&requests[0], "/v2/rate/EUR/USD");
    assert_wire(&requests[1], "/v2/rate/EUR/USD?providers=ECB");
    assert_wire(
        &requests[2],
        "/v2/rate/EUR/USD?date=2026-03-25&providers=ECB",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn unknown_provider_attribution_preserves_existing_rate_callers() {
    let server =
        TestServer::start(|_| json(200, include_bytes!("../fixtures/provider-attribution.json")))
            .await;
    let api = client(&server);
    // Exhaustive construction also checks that existing four-field Rate callers compile.
    let expected = Rate {
        date: "2026-03-25".parse().unwrap(),
        base: "EUR".into(),
        quote: "USD".into(),
        rate: "1.1568".parse().unwrap(),
    };
    let latest = api.latest_pair("EUR", "USD");
    assert_eq!(latest.clone().send().await.unwrap().rate, expected);
    assert_eq!(
        latest
            .clone()
            .set_providers("ECB")
            .send()
            .await
            .unwrap()
            .rate,
        expected
    );
    // Concord's own path (`prepare()?.send()`: `Rate` and `concord::Error`), which the
    // generated `send()` was before the client declared `send SendPolicy;`.
    assert_eq!(latest.prepare().unwrap().send().await.unwrap(), expected);

    let historical = api.historical_pair("EUR", "USD", expected.date);
    assert_eq!(historical.clone().send().await.unwrap().rate, expected);
    assert_eq!(
        historical.prepare().unwrap().send().await.unwrap(),
        expected
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    for (request, target) in requests.iter().zip([
        "/v2/rate/EUR/USD",
        "/v2/rate/EUR/USD?providers=ECB",
        "/v2/rate/EUR/USD",
        "/v2/rate/EUR/USD?date=2026-03-25",
        "/v2/rate/EUR/USD?date=2026-03-25",
    ]) {
        assert_wire(request, target);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn historical_no_data_is_a_typed_failure_not_a_zero_rate() {
    let server =
        TestServer::start(|_| json(404, include_bytes!("../fixtures/not-found.json"))).await;
    let result = client(&server)
        .historical_pair("EUR", "ABC", "1999-01-04".parse::<Date>().unwrap())
        .send()
        .await;
    let Error::Provider(error) = result.unwrap_err() else {
        panic!("expected provider error")
    };
    assert_eq!(error.meta.status().as_u16(), 404);
    assert_eq!(error.document.unwrap().message, "no data found");
    assert_wire(&server.single(), "/v2/rate/EUR/ABC?date=1999-01-04");
    assert_eq!(server.shutdown().await.open_after, 0);
}
