//! Consumer of the added controls, independent of tests/first_use.rs.
use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_fred::{
    AggregationMethod, Fred, FredConfig, Frequency, ObservationLimit, ObservationOptions, Units,
    concord_pagination::Resume, http_client, pages,
};

fn transformed<E: ObservationOptions>(endpoint: E) -> E {
    endpoint
        .set_units(Units::PercentChange)
        .set_frequency(Frequency::Annual)
        .set_aggregation_method(AggregationMethod::EndOfPeriod)
}

#[tokio::test]
async fn selected_series_controls_survive_pagination_and_format_changes() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/fred/series/search" => Reply::json(200, include_bytes!("../fixtures/search-0.json")),
        "/fred/series" => Reply::json(200, include_bytes!("../fixtures/metadata.json")),
        "/fred/series/observations" => match r.query_all("file_type").first().map(String::as_str) {
            Some("json") => Reply::json(200, include_bytes!("../fixtures/observations-0.json")),
            Some("csv") => Reply::text(
                200,
                "application/zip",
                include_bytes!("../fixtures/observations.zip"),
            ),
            None if r.query_all("offset") == ["2"] => Reply::text(
                200,
                "text/xml",
                include_bytes!("../fixtures/observations-2.xml"),
            ),
            None => Reply::text(
                200,
                "text/xml",
                include_bytes!("../fixtures/observations-0.xml"),
            ),
            other => panic!("unexpected format {other:?}"),
        },
        other => panic!("unexpected path {other}"),
    })
    .await;
    let api = Fred::new(
        http_client().unwrap(),
        FredConfig {
            api_key: "local-consumer-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let found = api.search_json("synthetic activity").send().await.unwrap();
    let selected = api.series(&found.seriess[0].id);
    let metadata = selected.metadata_json().send().await.unwrap();
    assert_eq!(metadata.seriess[0], found.seriess[0]);

    let template =
        transformed(selected.observations()).set_limit(ObservationLimit::new(2).unwrap());
    let mut walk = pages(
        template.clone(),
        Resume::Start,
        NonZeroUsize::new(1).unwrap(),
    );
    let first = walk.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].value, "2.5000");
    assert_eq!(first.items[1].value, ".");
    let bounded = walk.next_page().await.unwrap_err();
    let resume = bounded.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(2));
    let mut resumed = pages(template, resume, NonZeroUsize::new(2).unwrap());
    assert_eq!(
        resumed.next_page().await.unwrap().unwrap().items[0].value,
        "NA"
    );
    assert!(resumed.next_page().await.unwrap().is_none());

    let json = transformed(selected.observations_json())
        .send()
        .await
        .unwrap();
    assert_eq!(json.observations, first.items);
    let export = transformed(selected.export_csv()).send().await.unwrap();
    assert_eq!(
        export.into_csv(),
        include_bytes!("../fixtures/observations.csv")
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    for r in &requests[..2] {
        for parameter in ["units", "frequency", "aggregation_method"] {
            assert!(r.query_all(parameter).is_empty());
        }
    }
    for r in &requests[2..] {
        assert_eq!(r.query_all("series_id"), ["SYN_A"]);
        assert_eq!(r.query_all("units"), ["pch"]);
        assert_eq!(r.query_all("frequency"), ["a"]);
        assert_eq!(r.query_all("aggregation_method"), ["eop"]);
    }
    assert_eq!(requests[2].query_all("offset"), ["0"]);
    assert_eq!(requests[3].query_all("offset"), ["2"]);
    assert_eq!(requests[4].query_all("file_type"), ["json"]);
    assert_eq!(requests[5].query_all("file_type"), ["csv"]);
    server.shutdown().await;
}
