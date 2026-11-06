//! Non-advancing continuations (OPUS-2): a provider that answers a continued request with the
//! locator it was sent. Each adapter refuses the first repeat with
//! `ProtocolErrorKind::LocatorDidNotAdvance`, so the chain ends there rather than at the budget
//! (50 here); the requests are counted on the loopback server.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::bulk_query::{BulkQueryJob, ResultSets};
use concord_example_salesforce_sdk::ids::{
    AccessToken, ApiVersion, InstanceUrl, QueryJobId, SObjectName,
};
use concord_example_salesforce_sdk::model::Account;
use concord_example_salesforce_sdk::query::QueryPages;
use concord_example_salesforce_sdk::soql::{FieldPath, Soql};
use concord_example_salesforce_sdk::{OrgContext, ProtocolErrorKind, SfError};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{
    Budget, JobOptions, JobsError, Options, Outcome, PagesError, Resume, manifest_jobs, sequential,
};
use serde::Deserialize;

const HOST: &str = "loop.my.salesforce.test";
const TOKEN: &str = "00DW0000SECRET!loop";
const JOB: &str = "750A00000000001AAA";

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
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

fn protocol_kind(error: &SfError) -> Option<&ProtocolErrorKind> {
    match error {
        SfError::Protocol(p) => Some(p.kind()),
        _ => None,
    }
}

/// `query?q=` and every `query/{locator}` answer `nextRecordsUrl` = the same locator.
#[tokio::test]
async fn query_locator_that_does_not_advance_is_refused() {
    let (server, ctx) = start(|r: &Recorded| {
        if r.path().starts_with("/services/data/v68.0/query") {
            Reply::json(
                200,
                r#"{"totalSize":9,"done":false,
                    "nextRecordsUrl":"/services/data/v68.0/query/01gA0000000AbCdEAA-2",
                    "records":[{"attributes":{"type":"Account",
                        "url":"/services/data/v68.0/sobjects/Account/001A00000000001AAA"},
                        "Id":"001A00000000001AAA","Name":"Acme"}]}"#,
            )
        } else {
            Reply::new(404)
        }
    })
    .await;
    let soql = Soql::select(
        SObjectName::new("Account").unwrap(),
        [
            FieldPath::parse("Id").unwrap(),
            FieldPath::parse("Name").unwrap(),
        ],
    )
    .build()
    .unwrap();
    let pages = QueryPages::<Account>::new(&ctx, soql);
    let mut chain = sequential(Resume::Start, budget(50), |key| pages.fetch(key));
    let mut records = 0;
    let mut errors = Vec::new();
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => records += page.items.len(),
            Err(error) => errors.push(error),
        }
    }
    let report = chain.finish();
    assert_eq!(records, 1, "the repeated batch's records are not returned");
    assert_eq!(errors.len(), 1);
    assert_eq!(
        protocol_kind(&errors[0]),
        Some(&ProtocolErrorKind::LocatorDidNotAdvance)
    );
    assert_eq!(
        errors[0].to_string(),
        "query: protocol error: the next locator is the one just requested"
    );
    assert_eq!(report.outcome, Outcome::Failed);
    let targets: Vec<String> = server
        .requests()
        .iter()
        .map(|r| r.path().to_owned())
        .collect();
    assert_eq!(
        targets,
        [
            "/services/data/v68.0/query",
            "/services/data/v68.0/query/01gA0000000AbCdEAA-2"
        ]
    );
    server.shutdown().await;
}

/// The ordinary listing (`QueryPages::into_pages`): the same non-advancing provider ends the
/// traversal with a typed error that names the batch to retry, and never with `Ok(None)`; a
/// provider that reaches `done: true` ends with `Ok(None)`.
#[tokio::test]
async fn ordinary_query_listing_ends_only_at_done() {
    let record = r#"{"attributes":{"type":"Account",
        "url":"/services/data/v68.0/sobjects/Account/001A00000000001AAA"},
        "Id":"001A00000000001AAA","Name":"Acme"}"#;
    let (server, ctx) = start(move |r: &Recorded| match (r.path(), r.query()) {
        ("/services/data/v68.0/query", Some(q)) if q.contains("Contact") => Reply::json(
            200,
            r#"{"totalSize":0,"done":false,
                "nextRecordsUrl":"/services/data/v68.0/query/01gA0000000AbCdEAA-2","records":[]}"#,
        ),
        ("/services/data/v68.0/query", _) => Reply::json(
            200,
            format!(
                r#"{{"totalSize":2,"done":false,
                    "nextRecordsUrl":"/services/data/v68.0/query/01gA0000000AbCdEAA-1",
                    "records":[{record}]}}"#
            ),
        ),
        ("/services/data/v68.0/query/01gA0000000AbCdEAA-1", _) => Reply::json(
            200,
            format!(r#"{{"totalSize":2,"done":true,"records":[{record}]}}"#),
        ),
        // Repeats its own locator.
        ("/services/data/v68.0/query/01gA0000000AbCdEAA-2", _) => Reply::json(
            200,
            r#"{"totalSize":0,"done":false,
                "nextRecordsUrl":"/services/data/v68.0/query/01gA0000000AbCdEAA-2","records":[]}"#,
        ),
        _ => Reply::new(404),
    })
    .await;
    let select = |object: &str| {
        Soql::select(
            SObjectName::new(object).unwrap(),
            [FieldPath::parse("Id").unwrap()],
        )
        .build()
        .unwrap()
    };

    let mut pages = QueryPages::<Account>::new(&ctx, select("Account")).into_pages(Resume::Start);
    let mut records = 0;
    while let Some(page) = pages.next_page().await.unwrap() {
        records += page.items.len();
    }
    assert_eq!(records, 2);

    // A statement of another object; its records (none) do not matter, the chain does.
    let mut pages = QueryPages::<Account>::new(&ctx, select("Contact")).into_pages(Resume::Start);
    assert!(pages.next_page().await.unwrap().is_some());
    let error = pages.next_page().await.unwrap_err();
    let PagesError::Provider { error, failed, .. } = error else {
        panic!("expected the provider's error, got {error:?}");
    };
    assert_eq!(
        protocol_kind(&error),
        Some(&ProtocolErrorKind::LocatorDidNotAdvance)
    );
    assert!(matches!(failed, Resume::At(_)), "the batch to retry");
    assert!(
        matches!(pages.next_page().await, Err(PagesError::Ended { .. })),
        "an ended listing is not an exhausted one"
    );
    server.shutdown().await;
}

#[derive(Debug, Deserialize)]
struct Row {
    #[serde(rename = "Id")]
    _id: String,
}

/// Every result set answers `Sforce-Locator` = the same locator.
#[tokio::test]
async fn result_set_locator_that_does_not_advance_is_refused() {
    let (server, ctx) = start(|r: &Recorded| {
        if r.path().ends_with("/results") {
            Reply::text(
                200,
                "text/csv; charset=UTF-8",
                "\"Id\"\n\"001A00000000001AAA\"\n",
            )
            .header("sforce-locator", "MTAwMDA")
            .header("sforce-numberofrecords", "1")
        } else {
            Reply::new(404)
        }
    })
    .await;
    let dialect = BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf);
    let job = BulkQueryJob::attach(&ctx, QueryJobId::new(JOB).unwrap(), dialect);
    let sets = ResultSets::<Row>::new(&job, &[FieldPath::parse("Id").unwrap()], None).unwrap();
    let mut chain = sequential(Resume::Start, budget(50), |key| sets.fetch(key));
    let mut rows = 0;
    let mut errors = Vec::new();
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => rows += page.items.len(),
            Err(error) => errors.push(error),
        }
    }
    assert_eq!(rows, 1);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        protocol_kind(&errors[0]),
        Some(&ProtocolErrorKind::LocatorDidNotAdvance)
    );
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    let queries: Vec<Option<String>> = server
        .requests()
        .iter()
        .map(|r| r.query().map(str::to_owned))
        .collect();
    assert_eq!(queries, [None, Some("locator=MTAwMDA".to_owned())]);
    server.shutdown().await;
}

/// Every result-pages manifest names itself as the next manifest: discovery fails on the first
/// repeat, within a discovery budget of 50.
#[tokio::test]
async fn result_manifest_locator_that_does_not_advance_is_refused() {
    let (server, ctx) = start(|r: &Recorded| {
        if r.path().ends_with("/resultPages") {
            Reply::json(
                200,
                format!(
                    r#"{{"resultPages":[{{"resultUrl":"/services/data/v68.0/jobs/query/{JOB}/results?locator=QUFB"}}],
                        "done":false,
                        "nextRecordsUrl":"/services/data/v68.0/jobs/query/{JOB}/resultPages?locator=YcApWm4N"}}"#
                ),
            )
        } else {
            Reply::new(404)
        }
    })
    .await;
    let dialect = BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf);
    let job = BulkQueryJob::attach(&ctx, QueryJobId::new(JOB).unwrap(), dialect);
    let manifests = job.result_manifests();
    let options = JobOptions::new(budget(50), Options::new(budget(50)));
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |key| manifests.fetch(key),
        |part| async move { Ok::<u64, SfError>(part.ordinal) },
    );
    let mut parts = Vec::new();
    let mut discovery_errors = Vec::new();
    while let Some(next) = jobs.next().await {
        match next {
            Ok(done) => parts.push(done.value),
            Err(JobsError::Discovery(error)) => discovery_errors.push(error),
            Err(JobsError::Job(job)) => panic!("job {:?} failed", job.key),
        }
    }
    let report = jobs.finish();
    assert_eq!(discovery_errors.len(), 1);
    assert_eq!(
        protocol_kind(&discovery_errors[0]),
        Some(&ProtocolErrorKind::LocatorDidNotAdvance)
    );
    assert_eq!(report.outcome, Outcome::DiscoveryFailed);
    assert_eq!(report.discovery.calls, 2);
    assert!(
        parts.len() <= 1,
        "only the first manifest's part: {parts:?}"
    );
    let queries: Vec<Option<String>> = server
        .requests()
        .iter()
        .map(|r| r.query().map(str::to_owned))
        .collect();
    assert_eq!(queries, [None, Some("locator=YcApWm4N".to_owned())]);
    server.shutdown().await;
}
