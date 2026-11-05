//! Single-operation wire tests: every declared operation's method, path, query, headers and body
//! as reqwest sends them, against the loopback server; and redaction of the generated handles.
//! Expectations come from the Bulk API 2.0 and REST API documentation (see `PROVIDER.md`).

use concord::bytes::Bytes;
use concord::reqwest;
use concord_example_salesforce_sdk::bulk::{
    BulkDialect, ColumnDelimiter, CreateIngestJob, CreateQueryJob, IngestOperation, JobStateChange,
    LineEnding,
};
use concord_example_salesforce_sdk::ids::{
    AccessToken, ApiVersion, BatchSize, FieldList, FieldName, IngestJobId, InstanceUrl, MaxRecords,
    QueryJobId, QueryLocator, RecordId, ResultLocator, SObjectName,
};
use concord_example_salesforce_sdk::soql::{FieldPath, Soql};
use concord_example_salesforce_sdk::{OrgContext, SfError};
use concord_example_testserver::{Recorded, Reply, TestServer};

const HOST: &str = "wire.my.salesforce.test";
const TOKEN: &str = "00DW0000SECRET!wire";

async fn setup() -> (TestServer, OrgContext) {
    let server = TestServer::start(|r: &Recorded| {
        // Any JSON endpoint gets a document it can decode; CSV endpoints a CSV body.
        let path = r.path();
        if path.ends_with("/describe/") {
            Reply::json(200, r#"{"name":"Account","fields":[{"name":"Id","type":"id"}]}"#)
        } else if path.contains("/sobjects/Account/") {
            Reply::json(
                200,
                r#"{"attributes":{"type":"Account"},"Id":"001A00000000001AAA","Name":"Acme"}"#,
            )
        } else if path.contains("/sobjects/Thing__c/") {
            Reply::json(200, r#"{"attributes":{"type":"Thing__c"},"Name":"x"}"#)
        } else if path.contains("/v68.0/query") {
            Reply::json(200, r#"{"totalSize":0,"done":true,"records":[]}"#)
        } else if path.ends_with("/resultPages") {
            Reply::json(200, r#"{"resultPages":[],"done":true}"#)
        } else if path.contains("/jobs/query") && !path.ends_with("/results") {
            Reply::json(
                200,
                r#"{"id":"750A00000000001AAA","operation":"query","state":"JobComplete","apiVersion":68.0}"#,
            )
        } else if path.contains("/jobs/ingest") && r.method == "PUT" {
            Reply::new(201)
        } else if path.contains("/jobs/ingest") && !path.ends_with('/') {
            Reply::json(
                200,
                r#"{"id":"750A00000000002AAA","operation":"insert","state":"Open","apiVersion":"68.0"}"#,
            )
        } else {
            Reply::text(200, "text/csv; charset=UTF-8", "\"Id\"\n")
        }
    })
    .await;
    let http = reqwest::Client::builder()
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

fn last(server: &TestServer) -> Recorded {
    server.requests().pop().unwrap()
}

fn common(r: &Recorded, method: &str, path: &str) {
    assert_eq!(r.method, method);
    assert_eq!(r.path(), format!("/services/data/v68.0/{path}"));
    assert_eq!(
        r.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert!(r.host.starts_with(HOST));
}

fn soql() -> Soql {
    Soql::select(
        SObjectName::new("Account").unwrap(),
        [
            FieldPath::parse("Id").unwrap(),
            FieldPath::parse("Name").unwrap(),
        ],
    )
    .build()
    .unwrap()
}

#[tokio::test]
async fn rest_operations_on_the_wire() {
    let (server, ctx) = setup().await;
    let api = ctx.api();
    let id = RecordId::new("001A00000000001AAA").unwrap();
    let fields = FieldList::new([
        FieldName::new("Id").unwrap(),
        FieldName::new("Name").unwrap(),
    ])
    .unwrap();

    api.accounts().describe().send().await.unwrap();
    let r = last(&server);
    common(&r, "GET", "sobjects/Account/describe/");
    assert_eq!(r.header("accept"), Some("application/json"));

    api.accounts()
        .get(id.clone(), fields.clone())
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "GET", "sobjects/Account/001A00000000001AAA");
    assert_eq!(r.query(), Some("fields=Id%2CName"));

    let thing = SObjectName::new("Thing__c").unwrap();
    api.sobject(thing.clone())
        .get(id.clone())
        .send()
        .await
        .unwrap();
    common(
        &last(&server),
        "GET",
        "sobjects/Thing__c/001A00000000001AAA",
    );
    assert_eq!(last(&server).query(), None, "all fields without ?fields=");
    api.sobject(thing).describe().send().await.unwrap();
    common(&last(&server), "GET", "sobjects/Thing__c/describe/");

    api.query().accounts(soql()).send().await.unwrap();
    let r = last(&server);
    common(&r, "GET", "query");
    assert_eq!(r.query_all("q"), ["SELECT Id, Name FROM Account"]);
    assert_eq!(r.header("sforce-query-options"), None);

    api.query()
        .set_batch_size(BatchSize::new(500).unwrap())
        .contacts(soql())
        .send()
        .await
        .unwrap();
    assert_eq!(
        last(&server).header("sforce-query-options"),
        Some("batchSize=500")
    );

    let locator = QueryLocator::new("01gA0000000AbCdEAA-2000").unwrap();
    api.query()
        .more_dynamic(locator.clone())
        .send()
        .await
        .unwrap();
    common(&last(&server), "GET", "query/01gA0000000AbCdEAA-2000");
    // Concord's own diagnostics withhold the locator and the token.
    let prepared = api.query().more_accounts(locator).prepare().unwrap();
    let shown = format!("{prepared:?} {api:?} {ctx:?}");
    assert!(
        !shown.contains("01gA0000000AbCdEAA") && !shown.contains("SECRET"),
        "{shown}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn bulk_query_operations_on_the_wire() {
    let (server, ctx) = setup().await;
    let api = ctx.api();
    let dialect = BulkDialect::new(ColumnDelimiter::Tab, LineEnding::Crlf);
    api.bulk_query()
        .create(CreateQueryJob::new(&soql(), dialect))
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "POST", "jobs/query");
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&r.body).unwrap(),
        serde_json::json!({"operation":"query","query":"SELECT Id, Name FROM Account",
            "contentType":"CSV","columnDelimiter":"TAB","lineEnding":"CRLF"})
    );

    let job = api
        .bulk_query()
        .job(QueryJobId::new("750A00000000001AAA").unwrap());
    job.info().send().await.unwrap();
    common(&last(&server), "GET", "jobs/query/750A00000000001AAA");
    job.abort(JobStateChange::ABORTED).send().await.unwrap();
    let r = last(&server);
    common(&r, "PATCH", "jobs/query/750A00000000001AAA");
    assert_eq!(r.body_text(), r#"{"state":"Aborted"}"#);

    job.result_pages().send().await.unwrap();
    let r = last(&server);
    common(&r, "GET", "jobs/query/750A00000000001AAA/resultPages");
    assert_eq!(r.query(), None);
    job.result_pages()
        .set_locator(ResultLocator::new("YcApWm4N").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(last(&server).query(), Some("locator=YcApWm4N"));

    job.results()
        .set_locator(ResultLocator::new("MTAwMDA").unwrap())
        .set_max_records(MaxRecords::new(50_000).unwrap())
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "GET", "jobs/query/750A00000000001AAA/results");
    assert_eq!(r.query(), Some("locator=MTAwMDA&maxRecords=50000"));
    assert_eq!(r.header("accept"), Some("text/csv"));
    server.shutdown().await;
}

#[tokio::test]
async fn ingest_operations_on_the_wire() {
    let (server, ctx) = setup().await;
    let api = ctx.api();
    let dialect = BulkDialect::new(ColumnDelimiter::Caret, LineEnding::Lf);
    api.ingest()
        .create(
            CreateIngestJob::new(
                SObjectName::new("Contact").unwrap(),
                IngestOperation::Upsert,
                dialect,
            )
            .external_id(FieldName::new("External_Id__c").unwrap()),
        )
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "POST", "jobs/ingest");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&r.body).unwrap(),
        serde_json::json!({"object":"Contact","operation":"upsert","contentType":"CSV",
            "columnDelimiter":"CARET","lineEnding":"LF","externalIdFieldName":"External_Id__c"})
    );
    let job = api
        .ingest()
        .job(IngestJobId::new("750A00000000002AAA").unwrap());
    job.info().send().await.unwrap();
    common(&last(&server), "GET", "jobs/ingest/750A00000000002AAA");
    job.set_state(JobStateChange::UPLOAD_COMPLETE)
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "PATCH", "jobs/ingest/750A00000000002AAA");
    assert_eq!(r.body_text(), r#"{"state":"UploadComplete"}"#);
    job.upload(Bytes::from_static(b"LastName\nX\n"))
        .send()
        .await
        .unwrap();
    let r = last(&server);
    common(&r, "PUT", "jobs/ingest/750A00000000002AAA/batches");
    assert_eq!(r.header("content-type"), Some("text/csv"));
    assert_eq!(r.header("content-length"), Some("11"));
    assert!(!r.chunked);
    for (op, path) in [
        (job.successful_results().send().await, "successfulResults/"),
        (job.failed_results().send().await, "failedResults/"),
        (
            job.unprocessed_records().send().await,
            "unprocessedrecords/",
        ),
    ] {
        op.unwrap();
        let _ = path;
    }
    let requests = server.requests();
    let tail: Vec<_> = requests[requests.len() - 3..]
        .iter()
        .map(|r| r.path().to_owned())
        .collect();
    assert_eq!(
        tail,
        [
            "/services/data/v68.0/jobs/ingest/750A00000000002AAA/successfulResults/",
            "/services/data/v68.0/jobs/ingest/750A00000000002AAA/failedResults/",
            "/services/data/v68.0/jobs/ingest/750A00000000002AAA/unprocessedrecords/",
        ]
    );
    assert!(
        requests[requests.len() - 3..]
            .iter()
            .all(|r| r.header("accept") == Some("text/csv"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn status_errors_become_salesforce_error_documents() {
    let server = TestServer::start(|_| {
        Reply::json(
            404,
            r#"[{"errorCode":"NOT_FOUND","message":"The requested resource does not exist"}]"#,
        )
    })
    .await;
    let http = reqwest::Client::builder()
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
    let error = ctx.api().accounts().describe().send().await.unwrap_err();
    let error = SfError::from_concord(error).await;
    let api = error.api().unwrap();
    assert_eq!(api.status().as_u16(), 404);
    assert!(error.has_error_code("NOT_FOUND"));
    assert_eq!(api.operation(), Some("SalesforceApi.accounts.describe"));
    assert_eq!(
        error.to_string(),
        "SalesforceApi.accounts.describe: Salesforce answered status 404 [NOT_FOUND]"
    );
    assert!(!format!("{error:?}").contains("requested resource"));
    server.shutdown().await;
}
