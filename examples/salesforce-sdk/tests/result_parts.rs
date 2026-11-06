//! The validated result-part reader ([`ResultPartReader`]) against loopback fixtures: a part's
//! completion exists only after its validated end of input. Zero-row parts, exact and absent
//! `Sforce-NumberOfRecords`, a count mismatch after valid rows, an early stop, a `maxRecords`
//! limit, a provider error document on a non-2xx part, and the collected chain ([`ResultSets`])
//! reading through the same adapter.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord::stream::ReadErrorKind;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::bulk_query::{
    BulkQueryJob, NextResultSet, ResultPartReader, ResultSets,
};
use concord_example_salesforce_sdk::ids::{
    AccessToken, ApiVersion, InstanceUrl, MaxRecords, QueryJobId, ResultLocator,
};
use concord_example_salesforce_sdk::soql::FieldPath;
use concord_example_salesforce_sdk::{OrgContext, ProtocolErrorKind, ResultLink, SfError};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, Resume, sequential};
use serde::Deserialize;

const HOST: &str = "parts.my.salesforce.test";
const TOKEN: &str = "00DW0000SECRET!parts";
const JOB: &str = "750A00000000001AAA";
const RESULTS: &str = "/services/data/v68.0/jobs/query/750A00000000001AAA/results";
const HEADER: &str = "\"Id\",\"Name\"\n";
const OPERATION: &str = "SalesforceApi.bulk_query.job.results";

#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
}

async fn start(
    respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
) -> (TestServer, OrgContext) {
    let server = TestServer::start(respond).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .resolve(HOST, server.addr())
        .build()
        .unwrap();
    let ctx = OrgContext::new(
        http,
        InstanceUrl::parse(&format!("http://{HOST}:{}", server.addr().port())).unwrap(),
        ApiVersion::PINNED,
        AccessToken::new(TOKEN).unwrap(),
    )
    .unwrap();
    (server, ctx)
}

fn csv(body: &str) -> Reply {
    Reply::text(200, "text/csv; charset=UTF-8", body)
}

fn rows(n: usize) -> String {
    let mut body = HEADER.to_owned();
    for i in 0..n {
        body.push_str(&format!("\"001A0000000000{i}AAA\",\"Name {i}\"\n"));
    }
    body
}

fn reader(ctx: &OrgContext) -> ResultPartReader<Row> {
    let job = BulkQueryJob::attach(
        ctx,
        QueryJobId::new(JOB).unwrap(),
        BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf),
    );
    let columns = [
        FieldPath::parse("Id").unwrap(),
        FieldPath::parse("Name").unwrap(),
    ];
    ResultPartReader::new(&job, &columns).unwrap()
}

fn link(locator: &str, max_records: Option<u32>) -> ResultLink {
    ResultLink {
        locator: Some(ResultLocator::new(locator).unwrap()),
        max_records: max_records.map(|n| MaxRecords::new(n).unwrap()),
    }
}

fn locator(request: &Recorded) -> String {
    assert_eq!(request.path(), RESULTS);
    request.query_all("locator").pop().unwrap_or_default()
}

/// Every part shape the provider can answer, by locator.
fn respond(request: &Recorded) -> Reply {
    match locator(request).as_str() {
        "zero" => csv(HEADER).header("sforce-numberofrecords", "0"),
        "exact" => csv(&rows(3)).header("sforce-numberofrecords", "3"),
        "absent" => csv(&rows(2)),
        "zero-absent" => csv(HEADER),
        "mismatch" => csv(&rows(2)).header("sforce-numberofrecords", "3"),
        "over" => csv(&rows(3)).header("sforce-numberofrecords", "3"),
        "bad-count" => csv(&rows(1)).header("sforce-numberofrecords", "many"),
        "gone" => Reply::json(
            404,
            r#"[{"errorCode":"NOT_FOUND","message":"The requested resource does not exist"}]"#,
        ),
        "json" => Reply::json(200, r#"{"not":"csv"}"#),
        other => panic!("unexpected locator {other}"),
    }
}

async fn read_all(reader: &ResultPartReader<Row>, at: ResultLink) -> (Vec<Row>, Option<SfError>) {
    let mut part = reader.open(&at).await.unwrap();
    let mut delivered = Vec::new();
    while let Some(row) = part.next().await {
        match row {
            Ok(row) => delivered.push(row),
            Err(error) => {
                assert!(part.next().await.is_none(), "an error ends the part");
                let incomplete = part.finish().unwrap_err();
                let SfError::Incomplete(incomplete) = incomplete else {
                    panic!("finish after an error is Incomplete: {incomplete:?}");
                };
                assert!(incomplete.failed());
                return (delivered, Some(error));
            }
        }
    }
    let complete = part.finish().unwrap();
    assert_eq!(complete.records, delivered.len() as u64);
    (delivered, None)
}

#[tokio::test]
async fn zero_row_parts_complete_with_and_without_a_declared_count() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    for (at, declared) in [("zero", Some(0)), ("zero-absent", None)] {
        let mut part = reader.open(&link(at, None)).await.unwrap();
        assert_eq!(part.declared_records(), declared);
        assert!(part.next().await.is_none());
        let complete = part.finish().unwrap();
        assert_eq!(complete.records, 0);
        assert_eq!(complete.declared_records, declared);
        assert_eq!(complete.bytes, HEADER.len() as u64);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn exact_and_absent_counts_complete() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    let mut part = reader.open(&link("exact", None)).await.unwrap();
    assert_eq!(part.operation(), OPERATION);
    // A parallel result URL's response need not carry `Sforce-Locator`.
    assert_eq!(part.next_set(), &NextResultSet::Unknown);
    let mut delivered = Vec::new();
    while let Some(row) = part.next().await {
        delivered.push(row.unwrap());
    }
    assert_eq!(delivered[2].name, "Name 2");
    let complete = part.finish().unwrap();
    assert_eq!((complete.records, complete.declared_records), (3, Some(3)));
    assert_eq!(complete.bytes, rows(3).len() as u64);

    let (delivered, error) = read_all(&reader, link("absent", None)).await;
    assert!(error.is_none(), "an absent count is permitted on a part");
    assert_eq!(delivered.len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_count_mismatch_after_valid_rows_is_a_typed_failure_and_never_completes() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    let (delivered, error) = read_all(&reader, link("mismatch", None)).await;
    assert_eq!(delivered.len(), 2, "the valid rows were delivered first");
    let Some(SfError::Protocol(error)) = error else {
        panic!("expected a count mismatch, got {error:?}");
    };
    assert_eq!(
        error.kind(),
        &ProtocolErrorKind::RecordCountMismatch {
            declared: 3,
            parsed: 2
        }
    );
    assert_eq!(error.operation(), OPERATION);

    // A malformed count is refused before any row.
    let error = reader.open(&link("bad-count", None)).await.unwrap_err();
    let SfError::Protocol(error) = error else {
        panic!("{error:?}")
    };
    assert_eq!(error.kind(), &ProtocolErrorKind::RecordCountHeaderInvalid);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn stopping_early_or_dropping_is_not_a_completion() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    let mut part = reader.open(&link("exact", None)).await.unwrap();
    part.next().await.unwrap().unwrap();
    let error = part.finish().unwrap_err();
    let SfError::Incomplete(incomplete) = &error else {
        panic!("{error:?}")
    };
    assert!(!incomplete.failed(), "stopped, not failed");
    assert_eq!(incomplete.records(), 1);
    assert_eq!(incomplete.operation(), OPERATION);
    assert_eq!(
        error.to_string(),
        format!("{OPERATION}: result part incomplete (not read to its end, 1 records delivered)")
    );

    // Opened and dropped without reading: nothing completes, the connection is released.
    let part = reader.open(&link("exact", None)).await.unwrap();
    drop(part);
    // Every row read but the end not observed yet: still not complete.
    let mut part = reader.open(&link("exact", None)).await.unwrap();
    for _ in 0..3 {
        part.next().await.unwrap().unwrap();
    }
    assert!(matches!(part.finish(), Err(SfError::Incomplete(_))));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn the_links_max_records_is_a_limit_not_a_completion() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    let (delivered, error) = read_all(&reader, link("over", Some(2))).await;
    assert_eq!(delivered.len(), 2);
    let Some(SfError::Read(error)) = error else {
        panic!("expected a limit, got {error:?}");
    };
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(error.operation(), Some(OPERATION));
    let requested = server.requests().pop().unwrap();
    assert_eq!(requested.query_all("maxRecords"), ["2"]);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_non_2xx_part_is_the_provider_error_document_and_a_refused_head_stays_unread() {
    let (server, ctx) = start(respond).await;
    let reader = reader(&ctx);
    let error = reader.open(&link("gone", None)).await.unwrap_err();
    let api = error.api().expect("the provider's error document");
    assert_eq!(api.status().as_u16(), 404);
    assert!(error.has_error_code("NOT_FOUND"));
    assert_eq!(api.operation(), Some(OPERATION));
    assert_eq!(
        error.to_string(),
        format!("{OPERATION}: Salesforce answered status 404 [NOT_FOUND]")
    );

    let error = reader.open(&link("json", None)).await.unwrap_err();
    let SfError::Open(open) = error else {
        panic!("{error:?}")
    };
    assert_eq!(open.operation(), Some(OPERATION));
    assert_eq!(
        open.into_response().text().await.unwrap(),
        r#"{"not":"csv"}"#
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// The collected chain reads each set through the same adapter: a set whose count disagrees
/// fails after its rows were parsed, and no page of it is returned.
#[tokio::test]
async fn result_sets_use_the_same_validated_adapter() {
    let (server, ctx) = start(|request: &Recorded| match locator(request).as_str() {
        "" => csv(&rows(2))
            .header("sforce-locator", "second")
            .header("sforce-numberofrecords", "2"),
        "second" => csv(&rows(1))
            .header("sforce-locator", "null")
            .header("sforce-numberofrecords", "2"),
        other => panic!("unexpected locator {other}"),
    })
    .await;
    let job = BulkQueryJob::attach(
        &ctx,
        QueryJobId::new(JOB).unwrap(),
        BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf),
    );
    let columns = [
        FieldPath::parse("Id").unwrap(),
        FieldPath::parse("Name").unwrap(),
    ];
    let sets = ResultSets::<Row>::new(&job, &columns, None).unwrap();
    let budget = Budget::new(NonZeroUsize::new(10).unwrap());
    let mut chain = sequential(Resume::Start, budget, |key| sets.fetch(key));
    let first = chain.next().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.meta.declared_records, Some(2));
    assert_eq!(first.meta.bytes, rows(2).len() as u64);
    let error = chain.next().await.unwrap().unwrap_err();
    let SfError::Protocol(error) = error else {
        panic!("{error:?}")
    };
    assert_eq!(
        error.kind(),
        &ProtocolErrorKind::RecordCountMismatch {
            declared: 2,
            parsed: 1
        }
    );
    assert!(chain.next().await.is_none());
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    assert_eq!(server.shutdown().await.open_after, 0);
}
