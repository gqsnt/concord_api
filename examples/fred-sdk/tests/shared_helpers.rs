//! Adoption checks for the shared media validator and declared finite body cap.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_fred::{BODY_LIMIT, Fred, FredConfig, concord};

fn api(server: &TestServer) -> Fred {
    Fred::new(
        concord_sdk_fred::http_client().unwrap(),
        FredConfig {
            api_key: "fixture-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

#[tokio::test]
async fn every_finite_format_keeps_its_limit_on_prepared_sends() {
    let server = TestServer::start(|request: &Recorded| {
        let media = match request.query_all("file_type").first().map(String::as_str) {
            Some("json") => "application/json",
            Some("csv") => "application/zip",
            _ => "text/xml",
        };
        Reply::text(200, media, vec![b' '; BODY_LIMIT + 1])
    })
    .await;
    let api = api(&server);
    let series = api.series("fixture");
    macro_rules! bounded {
        ($operation:expr) => {{
            let operation = $operation;
            assert_eq!(
                operation.clone().prepare().unwrap().into_parts().2.limit(),
                Some(BODY_LIMIT)
            );
            let error = operation.prepare().unwrap().send().await.unwrap_err();
            assert_eq!(error.kind(), concord::ErrorKind::Limit);
        }};
    }
    bounded!(api.search("fixture"));
    bounded!(api.search_json("fixture"));
    bounded!(series.metadata());
    bounded!(series.metadata_json());
    bounded!(series.observations());
    bounded!(series.observations_json());
    bounded!(series.export_csv());
    assert_eq!(server.requests().len(), 7);
    server.shutdown().await;
}

#[tokio::test]
async fn xml_zip_and_problem_use_the_shared_rfc_parameter_parser() {
    let server = TestServer::start(|request: &Recorded| {
        if request.query_all("search_text") == ["problem"] {
            Reply::text(
                400,
                "text/xml ; charset=\"utf-8\"; profile=x",
                "<error code=\"400\" message=\"fixture\"/>",
            )
        } else if request.query_all("file_type") == ["csv"] {
            Reply::text(
                200,
                "application/zip ; charset=\"utf-8\"; profile=x",
                include_bytes!("../fixtures/observations.zip"),
            )
        } else {
            Reply::text(
                200,
                "text/xml ; charset=\"utf-8\"; profile=x",
                include_bytes!("../fixtures/search-0.xml"),
            )
        }
    })
    .await;
    let api = api(&server);
    assert!(api.search("fixture").send().await.is_ok());
    assert!(api.series("fixture").export_csv().send().await.is_ok());
    let concord_sdk_fred::Error::Http(error) = api.search("problem").send().await.unwrap_err()
    else {
        panic!("expected provider error");
    };
    assert!(error.document.is_ok());
    server.shutdown().await;
}
