//! SH-B end to end: the bulk export workflow against a loopback shop (GraphQL fixtures of
//! `tests/fixtures/sh_b`) and a second loopback server standing in for the signed-URL storage
//! host (reached by name through `reqwest::ClientBuilder::resolve`).
//!
//! `cargo test -p concord-example-shopify-app --test sh_b_bulk_export`

mod support;

use std::num::NonZeroU32;
use std::time::Duration;

use concord::bytes::Bytes;
use concord::stream::ReadErrorKind;
use concord_example_shopify_app::bulk_export::{
    BoundedSink, ExportOutcome, ExportRecord, ExportReport, ExportRequest, ExportStart,
    JobCreationOptIn, MalformedLines, PollWait, RecordSink, RequestStage, SinkControl,
    export_catalog, signature_refused,
};
use concord_example_shopify_sdk::ops::bulk::{BulkOperationErrorCode, BulkOperationStatus};
use concord_example_shopify_sdk::ops::catalog::ProductStatus;
use concord_example_shopify_sdk::scalars::BulkOperationId;
use concord_example_shopify_sdk::{CatalogExport, SearchQuery, ShopContext};
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer, generated_records};
use support::*;

const SIGNATURE: &str = "X-Goog-Signature=5yn7h371c51gn47ur3";

fn selection() -> CatalogExport {
    CatalogExport::products()
        .filter(
            SearchQuery::new()
                .term("vendor", "O'Brien \"Hats\"")
                .term("title", "Café"),
        )
        .with_variants(true)
}

fn submit() -> ExportStart {
    ExportStart::Submit(selection(), JobCreationOptIn::development_shop_confirmed())
}

fn attach() -> ExportStart {
    ExportStart::Attach(BulkOperationId::parse("gid://shopify/BulkOperation/9001").unwrap())
}

fn request(start: ExportStart) -> ExportRequest {
    ExportRequest {
        start,
        max_polls: NonZeroU32::new(5).unwrap(),
        max_line_bytes: 1024,
        malformed: MalformedLines::Stop,
    }
}

/// The storage host: `/bulk/9001.jsonl` and `/bulk/9001-partial.jsonl`.
async fn storage(
    full: impl Fn() -> Reply + Send + Sync + 'static,
    partial: impl Fn() -> Reply + Send + Sync + 'static,
) -> TestServer {
    TestServer::start(move |r: &Recorded| match r.path() {
        "/bulk/9001.jsonl" => full(),
        "/bulk/9001-partial.jsonl" => partial(),
        _ => Reply::new(404),
    })
    .await
}

fn jsonl(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", "application/jsonl")
        .with_body(body)
}

/// The shop server: shop-one answers `BulkRunQuery` with `run` and status polls from `statuses`
/// in order; shop-two always reports an empty completed operation.
async fn shops(storage_port: u16, run: &'static str, statuses: &[&'static str]) -> TestServer {
    let script = Script::new(
        statuses
            .iter()
            .map(|name| graphql_fixture(name, Some(storage_port))),
    );
    TestServer::start(move |r: &Recorded| {
        let shop = match shop_of(r) {
            Ok(shop) => shop,
            Err(reply) => return reply,
        };
        match (shop, operation(r).as_str()) {
            ("one", "BulkRunQuery") => graphql_fixture(run, Some(storage_port)),
            ("one", "BulkOperationStatus") => script.next(),
            ("two", "BulkOperationStatus") => {
                graphql_fixture("sh_b/status_completed_empty.json", None)
            }
            _ => Reply::json(500, "{}"),
        }
    })
    .await
}

fn chunked(bytes: &[u8], size: usize) -> Body {
    Body::Chunks(
        bytes
            .chunks(size)
            .map(|c| (Duration::ZERO, Bytes::copy_from_slice(c)))
            .collect(),
    )
}

async fn run(
    shop: &ShopContext,
    request: &ExportRequest,
    sink: &mut dyn RecordSink,
) -> (ExportReport, Vec<PollWait>) {
    let mut waits = Vec::new();
    let report = export_catalog(shop, request, sink, |wait| {
        waits.push(wait);
        std::future::ready(())
    })
    .await;
    (report, waits)
}

/// `(kind, id tail, parent tail)` of each record.
fn shape(records: &[ExportRecord]) -> Vec<(&'static str, String, Option<String>)> {
    records
        .iter()
        .map(|r| match r {
            ExportRecord::Product { id, .. } => ("product", id.tail().to_owned(), None),
            ExportRecord::Variant { id, product_id, .. } => (
                "variant",
                id.tail().to_owned(),
                Some(product_id.tail().to_owned()),
            ),
        })
        .collect()
}

fn expected_shape() -> Vec<(&'static str, String, Option<String>)> {
    [
        ("product", "101", None),
        ("product", "102", None),
        ("variant", "1101", Some("101")),
        ("product", "103", None),
        ("variant", "1102", Some("101")),
        ("variant", "1201", Some("102")),
        ("variant", "1301", Some("103")),
    ]
    .into_iter()
    .map(|(k, i, p)| (k, i.to_owned(), p.map(str::to_owned)))
    .collect()
}

#[tokio::test]
async fn sh_b_completed_export_streams_normalized_records_from_arbitrary_chunks() {
    let export = fixture_bytes("sh_b/export.jsonl");
    // Chunk sizes that split records, CRLF-free LF boundaries and multi-byte UTF-8 (é, —, 👕, 日本語).
    for size in [1, 2, 3, 5, 7, 64, export.len()] {
        let body = export.clone();
        let store = storage(move || jsonl(chunked(&body, size)), || Reply::new(404)).await;
        let server = shops(
            store.addr().port(),
            "sh_b/run_created.json",
            &["sh_b/status_running.json", "sh_b/status_completed.json"],
        )
        .await;
        let sdk = sdk(&server, Some(&store));
        let mut sink = BoundedSink::new(100);
        let (report, waits) = run(&shop_one(&sdk), &request(submit()), &mut sink).await;

        assert!(
            matches!(report.outcome, ExportOutcome::Complete),
            "{size}: {report:?}"
        );
        assert_eq!(shape(&sink.records), expected_shape(), "chunk size {size}");
        assert_eq!(
            (report.delivered.products, report.delivered.variants),
            (3, 4)
        );
        assert!(report.submitted);
        assert_eq!(report.operation.as_ref().unwrap().tail(), "9001");
        assert_eq!(report.polls, 2);
        assert_eq!(report.status, Some(BulkOperationStatus::Completed));
        assert_eq!(
            (report.object_count, report.file_size),
            (Some(7), Some(1234))
        );
        assert_eq!(report.position.unwrap().records, 7);
        assert_eq!(report.position.unwrap().bytes, export.len() as u64);
        assert!(report.peak_retained_bytes <= 1025);
        assert_eq!(report.cost.responses, 3);
        // The caller's wait ran before each poll, with the progress seen so far.
        let seen: Vec<_> = waits
            .iter()
            .map(|w| (w.poll, w.status.clone(), w.object_count))
            .collect();
        assert_eq!(
            seen,
            [
                (1, Some(BulkOperationStatus::Created), Some(0)),
                (2, Some(BulkOperationStatus::Running), Some(3)),
            ]
        );
        if size == 7 {
            let ExportRecord::Product { title, .. } = &sink.records[0] else {
                panic!()
            };
            assert_eq!(title, "Café crème — tasse 👕");
            let ExportRecord::Product {
                vendor,
                status,
                updated_at,
                ..
            } = &sink.records[1]
            else {
                panic!()
            };
            assert_eq!(vendor, "O'Brien \"Hats\"");
            assert_eq!(*status, ProductStatus::Active);
            assert_eq!(updated_at.as_str(), "2026-10-20T09:30:00Z");
            let ExportRecord::Variant {
                sku,
                inventory_quantity,
                tracked,
                price,
                ..
            } = &sink.records[4]
            else {
                panic!()
            };
            assert_eq!(
                (sku.as_deref(), *inventory_quantity, *tracked),
                (None, None, false)
            );
            assert_eq!(price.as_str(), "19.90");
        }

        // The shop: the mutation carries the selection's document as a JSON string variable,
        // the filter inside it as a GraphQL string literal.
        let shop_requests = server.requests();
        assert_eq!(
            shop_requests.iter().map(operation).collect::<Vec<_>>(),
            ["BulkRunQuery", "BulkOperationStatus", "BulkOperationStatus"]
        );
        assert!(
            shop_requests
                .iter()
                .all(|r| r.header_all("x-shopify-access-token") == [TOKEN_ONE])
        );
        let document = variable(&shop_requests[0], "query").unwrap();
        assert_eq!(document, selection().document());
        assert!(
            document.starts_with(
                r#"{ products(query: "vendor:\"O'Brien \\\"Hats\\\"\" title:Café") {"#
            ),
            "{document}"
        );
        assert_eq!(
            variable(&shop_requests[1], "id").as_deref(),
            Some("gid://shopify/BulkOperation/9001")
        );

        // The storage host: the signed URL verbatim, no Shopify credential.
        let download = store.single();
        assert_eq!(download.method, "GET");
        assert!(download.host.starts_with(STORAGE_HOST));
        assert!(download.target.ends_with(SIGNATURE));
        assert_eq!(download.header("x-shopify-access-token"), None);

        let shown = format!("{report:?}");
        for secret in [SIGNATURE, TOKEN_ONE, "Café", "O'Brien"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
        assert_eq!(server.shutdown().await.open_after, 0);
        assert_eq!(store.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn sh_b_empty_completion_downloads_nothing_and_shops_stay_separate() {
    let store = storage(|| Reply::new(500), || Reply::new(500)).await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed_empty.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    for shop in [shop_one(&sdk), shop_two(&sdk)] {
        let mut sink = BoundedSink::new(10);
        let (report, waits) = run(&shop, &request(attach()), &mut sink).await;
        assert!(
            matches!(report.outcome, ExportOutcome::EmptyCompleted),
            "{report:?}"
        );
        assert!(report.outcome.is_complete());
        assert!(!report.submitted);
        assert_eq!(report.polls, 1);
        assert!(waits.is_empty(), "an attached operation is polled at once");
        assert_eq!(report.object_count, Some(0));
        assert!(report.position.is_none() && sink.records.is_empty());
    }
    assert!(store.requests().is_empty(), "no url: nothing to download");
    let hosts: Vec<_> = server
        .requests()
        .iter()
        .map(|r| {
            (
                r.host.split(':').next().unwrap().to_owned(),
                r.header_all("x-shopify-access-token").join(","),
            )
        })
        .collect();
    assert_eq!(
        hosts,
        [
            ("shop-one.myshopify.com".to_owned(), TOKEN_ONE.to_owned()),
            ("shop-two.myshopify.com".to_owned(), TOKEN_TWO.to_owned()),
        ]
    );
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_failed_job_streams_partial_output_and_says_it_is_partial() {
    let partial = fixture_bytes("sh_b/export_partial.jsonl");
    let store = storage(|| Reply::new(404), move || jsonl(chunked(&partial, 11))).await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &[
            "sh_b/status_failed_partial.json",
            "sh_b/status_failed_no_data.json",
        ],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let shop = shop_one(&sdk);

    let mut sink = BoundedSink::new(10);
    let (report, _) = run(&shop, &request(attach()), &mut sink).await;
    assert!(
        matches!(
            report.outcome,
            ExportOutcome::Partial {
                error_code: Some(BulkOperationErrorCode::Timeout)
            }
        ),
        "{report:?}"
    );
    assert!(!report.outcome.is_complete());
    assert!(report.partial_output);
    assert_eq!(
        shape(&sink.records),
        expected_shape()
            .into_iter()
            .filter(|s| s.1 == "101" || s.1 == "1101")
            .collect::<Vec<_>>()
    );
    assert_eq!(store.single().path(), "/bulk/9001-partial.jsonl");

    let mut sink = BoundedSink::new(10);
    let (report, _) = run(&shop, &request(attach()), &mut sink).await;
    assert!(matches!(
        report.outcome,
        ExportOutcome::FailedWithoutData {
            error_code: Some(BulkOperationErrorCode::AccessDenied)
        }
    ));
    assert!(sink.records.is_empty());
    assert_eq!(store.requests().len(), 1);
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_expired_signed_url_is_refused_before_its_body_is_read() {
    let store = storage(
        || {
            Reply::text(
                403,
                "application/xml; charset=UTF-8",
                fixture("sh_b/expired_signature.xml"),
            )
        },
        || Reply::new(404),
    )
    .await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let mut sink = BoundedSink::new(10);
    let (report, _) = run(&shop_one(&sdk), &request(attach()), &mut sink).await;

    assert_eq!(report.delivered.products + report.delivered.variants, 0);
    let shown = format!("{report:?}");
    assert!(
        shown.contains("403") && !shown.contains(SIGNATURE),
        "{shown}"
    );
    let ExportOutcome::DownloadRefused { error } = report.outcome else {
        panic!("{shown}")
    };
    assert!(signature_refused(&error));
    assert_eq!(error.operation(), Some("shopify: bulk result"));
    assert!(!error.to_string().contains(SIGNATURE));
    // The refused response was kept unread: the caller can still look at the storage error.
    let body = error.into_response().text().await.unwrap();
    assert_eq!(body, fixture("sh_b/expired_signature.xml"));
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_malformed_lines_stop_the_export_or_are_skipped_within_a_bound() {
    let cases = [
        (MalformedLines::Stop, 1, 0, Some(ReadErrorKind::Format)),
        (
            MalformedLines::Skip { max: 1 },
            2,
            1,
            Some(ReadErrorKind::Decode),
        ),
        (MalformedLines::Skip { max: 2 }, 3, 2, None),
    ];
    for (policy, delivered, skipped, interrupted) in cases {
        let body = fixture_bytes("sh_b/export_malformed.jsonl");
        let store = storage(move || jsonl(chunked(&body, 13)), || Reply::new(404)).await;
        let server = shops(
            store.addr().port(),
            "sh_b/run_created.json",
            &["sh_b/status_completed.json"],
        )
        .await;
        let sdk = sdk(&server, Some(&store));
        let mut sink = BoundedSink::new(10);
        let mut request = request(attach());
        request.malformed = policy;
        let (report, _) = run(&shop_one(&sdk), &request, &mut sink).await;

        assert_eq!(sink.records.len(), delivered, "{policy:?}");
        assert_eq!(report.skipped_lines, skipped, "{policy:?}");
        match (interrupted, &report.outcome) {
            (Some(kind), ExportOutcome::Interrupted { error }) => {
                assert_eq!(error.kind(), kind, "{policy:?}");
                // Diagnostics: kind, position and labels only, no line content.
                let text = format!("{error} {error:?}");
                assert!(
                    !text.contains("cut") && !text.contains("not selected"),
                    "{text}"
                );
            }
            (None, ExportOutcome::Complete) => {}
            (_, other) => panic!("{policy:?}: {other:?}"),
        }
        server.shutdown().await;
        store.shutdown().await;
    }
}

#[tokio::test]
async fn sh_b_a_skipped_error_right_before_the_end_is_never_reported_as_complete() {
    let export = fixture("sh_b/export.jsonl");
    let lines: Vec<&str> = export.lines().collect();
    // A byte-order mark is a terminal Format error; a malformed last line is an isolated one. The
    // reader reports both the same way, then ends: neither may become a complete export.
    let bom = format!("\u{feff}{}\n{}\n", lines[0], lines[1]).into_bytes();
    let last = format!("{}\n{}\n{{\"id\":", lines[0], lines[1]).into_bytes();
    for (body, delivered) in [(bom, 0), (last, 2)] {
        let store = storage(move || jsonl(chunked(&body, 9)), || Reply::new(404)).await;
        let server = shops(
            store.addr().port(),
            "sh_b/run_created.json",
            &["sh_b/status_completed.json"],
        )
        .await;
        let sdk = sdk(&server, Some(&store));
        let mut sink = BoundedSink::new(10);
        let mut request = request(attach());
        request.malformed = MalformedLines::Skip { max: 5 };
        let (report, _) = run(&shop_one(&sdk), &request, &mut sink).await;
        assert_eq!(sink.records.len(), delivered);
        assert_eq!(report.skipped_lines, 1);
        let ExportOutcome::Interrupted { error } = &report.outcome else {
            panic!("{report:?}")
        };
        assert_eq!(error.kind(), ReadErrorKind::Format);
        server.shutdown().await;
        store.shutdown().await;
    }
}

#[tokio::test]
async fn sh_b_an_oversized_line_ends_the_stream_with_a_limit_error() {
    let export = fixture("sh_b/export.jsonl");
    let lines: Vec<&str> = export.lines().collect();
    let oversized = format!(
        r#"{{"id":"gid://shopify/Product/199","title":"{}","handle":"h","status":"ACTIVE","vendor":"v","updatedAt":"2026-10-20T09:30:00Z"}}"#,
        "x".repeat(4096)
    );
    let body = format!("{}\n{}\n{oversized}\n{}\n", lines[0], lines[1], lines[3]).into_bytes();
    let store = storage(move || jsonl(chunked(&body, 500)), || Reply::new(404)).await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let mut sink = BoundedSink::new(10);
    let mut request = request(attach());
    request.malformed = MalformedLines::Skip { max: 10 }; // an over-long line is never skipped
    let (report, _) = run(&shop_one(&sdk), &request, &mut sink).await;

    assert_eq!(sink.records.len(), 2);
    let ExportOutcome::Interrupted { error } = &report.outcome else {
        panic!("{report:?}")
    };
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(error.position().records, 2);
    assert!(
        report.peak_retained_bytes <= 1025,
        "{}",
        report.peak_retained_bytes
    );
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_consumer_cancellation_stops_reading_and_closes_the_download() {
    let line = |i: u64| {
        format!(
            r#"{{"id":"gid://shopify/Product/{i}","title":"Generated {i}","handle":"g-{i}","status":"ACTIVE","vendor":"Synthetic","updatedAt":"2026-10-20T09:30:00Z"}}"#
        )
        .into_bytes()
        .into_iter()
        .chain(*b"\n")
        .collect::<Vec<u8>>()
    };
    let total: u64 = (0..1_000_000u64).map(|i| line(i).len() as u64).sum();
    let store = storage(
        move || {
            jsonl(Body::Generate(
                generated_records(1_000_000, 16 * 1024, line),
                None,
            ))
        },
        || Reply::new(404),
    )
    .await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let mut sink = BoundedSink::new(5);
    let (report, _) = run(&shop_one(&sdk), &request(attach()), &mut sink).await;

    assert!(
        matches!(report.outcome, ExportOutcome::SinkStopped),
        "{report:?}"
    );
    assert_eq!(sink.records.len(), 5);
    assert_eq!(report.position.unwrap().records, 5);
    // The response was dropped: the server sees the connection close long before the body ends.
    assert!(
        store
            .wait_until(Duration::from_secs(10), |s| s.live_connections() == 0
                && s.active_bodies() == 0)
            .await
    );
    assert!(
        store.body_bytes_sent() < total / 10,
        "{} of {total}",
        store.body_bytes_sent()
    );
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_a_transport_error_after_records_is_reported_with_its_position() {
    let export = fixture_bytes("sh_b/export.jsonl");
    let cut = export
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(1)
        .unwrap()
        .0
        + 1
        + 20;
    let prefix = Bytes::copy_from_slice(&export[..cut]);
    let store = storage(
        move || jsonl(Body::FailAfter(vec![prefix.clone()])),
        || Reply::new(404),
    )
    .await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let mut sink = BoundedSink::new(10);
    let (report, _) = run(&shop_one(&sdk), &request(attach()), &mut sink).await;

    assert_eq!(
        sink.records.len(),
        2,
        "the complete records before the failure stay delivered"
    );
    let ExportOutcome::Interrupted { error } = &report.outcome else {
        panic!("{report:?}")
    };
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.position().records, 2);
    assert!(!format!("{error} {error:?}").contains(SIGNATURE));
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_a_rejected_submission_creates_no_job_and_polls_nothing() {
    let store = storage(|| Reply::new(500), || Reply::new(500)).await;
    let server = shops(store.addr().port(), "sh_b/run_in_progress.json", &[]).await;
    let sdk = sdk(&server, Some(&store));
    let mut sink = BoundedSink::new(10);
    let (report, waits) = run(&shop_one(&sdk), &request(submit()), &mut sink).await;

    let ExportOutcome::Rejected { user_errors } = &report.outcome else {
        panic!("{report:?}")
    };
    assert_eq!(
        user_errors[0].code.as_deref(),
        Some("OPERATION_IN_PROGRESS")
    );
    assert!(report.operation.is_none() && !report.submitted && waits.is_empty());
    assert_eq!(server.requests().len(), 1);
    assert!(!format!("{report:?}").contains("already in progress"));
    server.shutdown().await;
    store.shutdown().await;
}

#[tokio::test]
async fn sh_b_polling_is_bounded_and_a_throttled_poll_is_a_reported_failure() {
    let store = storage(|| Reply::new(500), || Reply::new(500)).await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &[
            "sh_b/status_running.json",
            "sh_b/status_running.json",
            "sh_b/status_running.json",
            "sh_b/status_throttled.json",
        ],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let shop = shop_one(&sdk);
    let mut request = request(attach());
    request.max_polls = NonZeroU32::new(2).unwrap();

    let mut sink = BoundedSink::new(10);
    let (report, waits) = run(&shop, &request, &mut sink).await;
    assert!(
        matches!(report.outcome, ExportOutcome::StillRunning),
        "{report:?}"
    );
    assert_eq!((report.polls, waits.len()), (2, 1));
    assert_eq!(report.object_count, Some(3));
    assert_eq!(
        report.operation.as_ref().unwrap().tail(),
        "9001",
        "kept to attach again later"
    );

    let (report, _) = run(&shop, &request, &mut sink).await;
    let ExportOutcome::RequestFailed { stage, failure } = &report.outcome else {
        panic!("{report:?}")
    };
    assert_eq!(*stage, RequestStage::Status);
    assert!(failure.is_throttled());
    assert_eq!(report.cost.not_executed, 1);
    assert_eq!(report.cost.last.unwrap().currently_available, 0.5);
    server.shutdown().await;
    store.shutdown().await;
}

/// Counts records and remembers how much of the body the server had sent when the first one
/// arrived; keeps no record.
struct Probe<'a> {
    storage: &'a TestServer,
    first_at: Option<u64>,
    records: u64,
}

impl RecordSink for Probe<'_> {
    fn accept(&mut self, _record: ExportRecord) -> SinkControl {
        if self.first_at.is_none() {
            self.first_at = Some(self.storage.body_bytes_sent());
        }
        self.records += 1;
        SinkControl::Continue
    }
}

#[tokio::test]
async fn sh_b_a_large_generated_export_is_streamed_with_a_bounded_line_buffer() {
    const RECORDS: u64 = 40_000;
    let line = |i: u64| {
        let text = if i.is_multiple_of(2) {
            format!(
                r#"{{"id":"gid://shopify/Product/{i}","title":"Généré {i}","handle":"g-{i}","status":"ACTIVE","vendor":"Synthetic","updatedAt":"2026-10-20T09:30:00Z"}}"#
            )
        } else {
            format!(
                r#"{{"id":"gid://shopify/ProductVariant/{i}","title":"Default Title","sku":"G-{i}","price":"1.00","inventoryQuantity":{i},"inventoryItem":{{"id":"gid://shopify/InventoryItem/{i}","tracked":true}},"__parentId":"gid://shopify/Product/{}"}}"#,
                i - 1
            )
        };
        let mut bytes = text.into_bytes();
        bytes.push(b'\n');
        bytes
    };
    let total: u64 = (0..RECORDS).map(|i| line(i).len() as u64).sum();
    let store = storage(
        move || {
            jsonl(Body::Generate(
                generated_records(RECORDS, 16 * 1024, line),
                Some(Duration::from_millis(1)),
            ))
        },
        || Reply::new(404),
    )
    .await;
    let server = shops(
        store.addr().port(),
        "sh_b/run_created.json",
        &["sh_b/status_completed.json"],
    )
    .await;
    let sdk = sdk(&server, Some(&store));
    let mut probe = Probe {
        storage: &store,
        first_at: None,
        records: 0,
    };
    let started = std::time::Instant::now();
    let (report, _) = run(&shop_one(&sdk), &request(attach()), &mut probe).await;
    let elapsed = started.elapsed();

    assert!(
        matches!(report.outcome, ExportOutcome::Complete),
        "{report:?}"
    );
    assert_eq!(probe.records, RECORDS);
    assert_eq!(
        (report.delivered.products, report.delivered.variants),
        (RECORDS / 2, RECORDS / 2)
    );
    assert_eq!(report.position.unwrap().bytes, total);
    let first_at = probe.first_at.unwrap();
    assert!(
        first_at < total / 4,
        "first record after {first_at} of {total} bytes"
    );
    // The reader's own buffer stayed within one line, whatever the body size.
    assert!(
        report.peak_retained_bytes <= 1025,
        "{}",
        report.peak_retained_bytes
    );
    eprintln!(
        "sh_b large export: {RECORDS} records, {total} bytes, first record after {first_at} body bytes sent, \
         peak retained line buffer {} bytes, {elapsed:?} (loopback, 16 KiB chunks, 1 ms per chunk)",
        report.peak_retained_bytes
    );
    server.shutdown().await;
    store.shutdown().await;
}
