//! SF-B ingest sub-scenario, offline against the loopback server: public encoder, bounded spool
//! of known length, job create → upload → close → poll → result files. Command:
//! `cargo test -p concord-example-salesforce-app --test sf_b_ingest_round_trip`.

mod support;

use std::num::NonZeroUsize;

use concord::stream::from_iter;
use concord_example_salesforce_app::ingest::{
    CONTACT_INSERT_COLUMNS, ContactInsert, IngestError, IngestInput, ingest_round_trip,
};
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, JobState, LineEnding};
use concord_example_salesforce_sdk::csv::{CsvError, FieldText, IngestValue};
use concord_example_salesforce_sdk::{LinkErrorKind, SfError};
use concord_example_testserver::{Recorded, Reply};
use support::*;

const DIALECT: BulkDialect = BulkDialect::new(ColumnDelimiter::Semicolon, LineEnding::Crlf);

fn input() -> IngestInput {
    IngestInput {
        dialect: DIALECT,
        upload_limit: 1024 * 1024,
        max_polls: NonZeroUsize::new(5).unwrap(),
        max_result_rows: 100,
    }
}

fn rows() -> Vec<ContactInsert> {
    vec![
        ContactInsert {
            last_name: FieldText::new("O'Brien; Jr").unwrap(),
            first_name: IngestValue::text("Zoë").unwrap(),
            email: IngestValue::Null,
            loyalty_tier: IngestValue::text("Gold").unwrap(),
            description: IngestValue::text("said \"hi\"\nthen left").unwrap(),
        },
        ContactInsert {
            last_name: FieldText::new("Dury").unwrap(),
            first_name: IngestValue::Keep,
            email: IngestValue::Keep,
            loyalty_tier: IngestValue::Keep,
            description: IngestValue::Keep,
        },
        ContactInsert {
            last_name: FieldText::new("Hopper").unwrap(),
            first_name: IngestValue::text("Grace").unwrap(),
            email: IngestValue::text("not-an-email").unwrap(),
            loyalty_tier: IngestValue::Keep,
            description: IngestValue::Keep,
        },
    ]
}

const EXPECTED_BODY: &str = "LastName;FirstName;Email;Loyalty_Tier__c;Description\r\n\
\"O'Brien; Jr\";Zoë;#N/A;Gold;\"said \"\"hi\"\"\nthen left\"\r\n\
Dury;;;;\r\n\
Hopper;Grace;not-an-email;;\r\n";

fn ingest_routes(router: &Router, open_doc: String) {
    let job = |rest: &str| api(&format!("jobs/ingest/{INGEST_JOB}{rest}"));
    router
        .on(ORG_A, "POST", &api("jobs/ingest"), json(open_doc))
        .on(ORG_A, "PUT", &job("/batches"), |_: &Recorded| {
            Reply::new(201)
        })
        .on(
            ORG_A,
            "PATCH",
            &job(""),
            json(fixture("ingest/job_upload_complete.json")),
        )
        .on(
            ORG_A,
            "GET",
            &job(""),
            json(fixture("ingest/job_in_progress.json")),
        )
        .on(
            ORG_A,
            "GET",
            &job(""),
            json(fixture("ingest/job_complete.json")),
        )
        .on(ORG_A, "GET", &job("/successfulResults/"), |_: &Recorded| {
            Reply::text(
                200,
                "text/csv; charset=UTF-8",
                fixture("ingest/successful_results.csv"),
            )
        })
        .on(ORG_A, "GET", &job("/failedResults/"), |_: &Recorded| {
            Reply::text(
                200,
                "text/csv; charset=UTF-8",
                fixture("ingest/failed_results.csv"),
            )
        })
        .on(
            ORG_A,
            "GET",
            &job("/unprocessedrecords/"),
            |_: &Recorded| {
                Reply::text(
                    200,
                    "text/csv; charset=UTF-8",
                    fixture("ingest/unprocessed_records.csv"),
                )
            },
        );
}

#[tokio::test]
async fn ingest_spooled_upload_round_trip() {
    let router = Router::new();
    ingest_routes(&router, fixture("ingest/job_open.json"));
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);
    let source = from_iter(rows().into_iter().map(Ok::<_, std::io::Error>));
    let report = ingest_round_trip(&ctx, &input(), source, |_| std::future::ready(()))
        .await
        .unwrap();

    assert_eq!(report.state, JobState::JobComplete);
    assert_eq!(report.upload.rows, 3);
    assert_eq!(report.upload.buffered_bytes, EXPECTED_BODY.len());
    assert_eq!(report.processed, Some(3));
    assert_eq!(report.failed_count, Some(1));
    assert_eq!(report.successful.rows.len(), 2);
    let first = &report.successful.rows[0];
    assert_eq!(first.id.as_ref().unwrap().as_str(), "003A00000000101AAA");
    assert_eq!(first.created, Some(true));
    assert_eq!(first.field("LastName"), Some("O'Brien; Jr"));
    assert_eq!(first.field("Email"), Some("#N/A"));
    assert_eq!(first.field("Description"), Some("said \"hi\"\nthen left"));
    assert_eq!(report.failed.rows.len(), 1);
    let failed = &report.failed.rows[0];
    assert_eq!(failed.id, None, "a failed insert has an empty sf__Id");
    assert!(
        failed
            .error_unredacted()
            .unwrap()
            .starts_with("INVALID_EMAIL_ADDRESS")
    );
    assert!(report.unprocessed.rows.is_empty());
    let shown = format!("{report:?}");
    assert!(
        !shown.contains("not-an-email") && !shown.contains("Brien"),
        "{shown}"
    );

    // Wire.
    let requests = server.requests();
    assert_eq!(router.unrouted(), 0);
    assert_org_isolation(&requests);
    let create = requests.iter().find(|r| r.method == "POST").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"object":"Contact","operation":"insert","contentType":"CSV",
            "columnDelimiter":"SEMICOLON","lineEnding":"CRLF"})
    );
    let upload = requests.iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(upload.header("content-type"), Some("text/csv"));
    assert!(!upload.chunked, "Salesforce path: one body of known length");
    assert_eq!(
        upload.header("content-length"),
        Some(EXPECTED_BODY.len().to_string().as_str())
    );
    assert_eq!(upload.body_text(), EXPECTED_BODY);
    let close = requests.iter().find(|r| r.method == "PATCH").unwrap();
    assert_eq!(close.body_text(), r#"{"state":"UploadComplete"}"#);
    let order: Vec<&str> = requests.iter().map(|r| r.method.as_str()).collect();
    assert_eq!(&order[..3], ["POST", "PUT", "PATCH"]);
    for r in requests
        .iter()
        .filter(|r| r.path().ends_with('/') && r.method == "GET")
    {
        assert_eq!(r.header("accept"), Some("text/csv"));
    }
    assert_eq!(CONTACT_INSERT_COLUMNS.len(), 5);
    server.shutdown().await;
}

#[tokio::test]
async fn ingest_encoding_failures_create_no_job() {
    let router = Router::new();
    ingest_routes(&router, fixture("ingest/job_open.json"));
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);

    // A failing source.
    let failing = from_iter(
        rows()
            .into_iter()
            .map(Ok)
            .chain(std::iter::once(Err(std::io::Error::other("source secret")))),
    );
    let err = ingest_round_trip(&ctx, &input(), failing, |_| std::future::ready(()))
        .await
        .unwrap_err();
    assert!(
        matches!(err, IngestError::Encode(CsvError::Source(_))),
        "{err:?}"
    );
    assert!(!format!("{err} {err:?}").contains("source secret"));

    // Over the upload limit.
    let mut small = input();
    small.upload_limit = 64;
    let source = from_iter(rows().into_iter().map(Ok::<_, std::io::Error>));
    let err = ingest_round_trip(&ctx, &small, source, |_| std::future::ready(()))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IngestError::Encode(CsvError::UploadTooLarge { limit: 64 })
    ));

    assert!(server.requests().is_empty(), "nothing reached the org");
    server.shutdown().await;
}

#[tokio::test]
async fn ingest_content_url_is_validated_before_upload() {
    let wrong_job = fixture("ingest/job_open.json").replace(
        &format!("ingest/{INGEST_JOB}/batches"),
        "ingest/750A00000000009AAA/batches",
    );
    let foreign = fixture("ingest/job_open.json").replace(
        "\"services/data/",
        &format!("\"https://{}/services/data/", ORG_B.host),
    );
    for (doc, kind) in [
        (wrong_job, LinkErrorKind::WrongJob),
        (foreign, LinkErrorKind::ForeignOrigin),
    ] {
        let router = Router::new();
        ingest_routes(&router, doc);
        let server = router.start().await;
        let ctx = ctx(ORG_A, &server);
        let source = from_iter(rows().into_iter().map(Ok::<_, std::io::Error>));
        let err = ingest_round_trip(&ctx, &input(), source, |_| std::future::ready(()))
            .await
            .unwrap_err();
        match err {
            IngestError::Create(SfError::Link(l)) => assert_eq!(l.kind(), kind),
            other => panic!("{other:?}"),
        }
        assert!(!server.requests().iter().any(|r| r.method == "PUT"));
        server.shutdown().await;
    }
}
