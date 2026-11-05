//! Wire fixtures authored for the Stage 1 downstream consumer.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use concord_check_dx03_fanout_sink::stage1::{AccountRow, ExportError, collect_accounts};
use concord_example_salesforce_sdk::{
    OrgContext, ProtocolErrorKind, SfError,
    ids::{AccessToken, ApiVersion, InstanceUrl, QueryJobId, RecordId},
};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use serde_json::{Value, json};

const JOB: &str = "750000000000001AAA";
const JOB_PATH: &str = "/services/data/v68.0/jobs/query/750000000000001AAA";
const RESULTS_PATH: &str = "/services/data/v68.0/jobs/query/750000000000001AAA/results";
const HEADER: &str = "Id,Name,NumberOfEmployees\n";

fn job_info(delimiter: &str, ending: &str) -> Value {
    json!({
        "id": JOB,
        "operation": "query",
        "object": "Account",
        "state": "JobComplete",
        "apiVersion": 68.0,
        "contentType": "CSV",
        "columnDelimiter": delimiter,
        "lineEnding": ending
    })
}

fn info_reply(info: Value) -> Reply {
    Reply::json(200, serde_json::to_vec(&info).unwrap())
}

fn csv_reply(body: &str, next: &str, count: u64) -> Reply {
    Reply::text(200, "text/csv; charset=UTF-8; header=present", body)
        .header("Sforce-Locator", next)
        .header("Sforce-NumberOfRecords", count.to_string())
}

fn context(server: &TestServer) -> OrgContext {
    OrgContext::new(
        concord::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        InstanceUrl::parse(&server.url()).unwrap(),
        ApiVersion::PINNED,
        AccessToken::new("fixture-token").unwrap(),
    )
    .unwrap()
}

fn job_id() -> QueryJobId {
    QueryJobId::new(JOB).unwrap()
}

fn row(id: &str, name: &str, employees: Option<u64>) -> AccountRow {
    AccountRow {
        id: RecordId::new(id).unwrap(),
        name: name.to_owned(),
        number_of_employees: employees,
    }
}

async fn scripted(replies: Vec<Reply>) -> (TestServer, Script) {
    let script = Script::new(replies);
    let responses = script.clone();
    let server = TestServer::start(move |_| responses.next()).await;
    (server, script)
}

async fn assert_chain(server: TestServer, script: Script, locators: &[Option<&str>]) {
    let requests = server.requests();
    assert_eq!(requests.len(), 1 + locators.len());
    assert_eq!(requests[0].target, JOB_PATH);
    assert_eq!(requests[0].header("accept"), Some("application/json"));
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.header("authorization"),
            Some("Bearer fixture-token")
        );
        assert!(request.body.is_empty());
    }
    for (request, locator) in requests[1..].iter().zip(locators) {
        assert_eq!(request.path(), RESULTS_PATH);
        assert_eq!(request.query_all("maxRecords"), ["10000"]);
        assert_eq!(
            request.query_all("locator"),
            locator
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            request.query_pairs().len(),
            1 + usize::from(locator.is_some())
        );
    }
    assert_eq!(script.remaining(), 0);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn collects_two_sets_in_order_using_the_jobs_pipe_crlf_dialect() {
    let first_finished = Arc::new(AtomicBool::new(false));
    let first_finished_in_body = first_finished.clone();
    let first_csv = concat!(
        "\u{feff}Id|Name|NumberOfEmployees\r\n",
        "001000000000001AAA|\"Café | \"\"North\"\"\r\nBranch\"|42\r\n",
        "001000000000002AAA|\"  Padded  \"|\r\n"
    );
    // Arbitrary transport boundaries split UTF-8, quotes, CRLF and records.
    let mut chunks = first_csv
        .as_bytes()
        .chunks(7)
        .map(|bytes| bytes.to_vec().into())
        .collect::<Vec<_>>()
        .into_iter();
    let first_body = Body::Generate(
        Box::new(move || {
            let chunk = chunks.next();
            if chunk.is_none() {
                first_finished_in_body.store(true, Ordering::SeqCst);
            }
            chunk
        }),
        Some(Duration::from_millis(1)),
    );
    let script = Script::new([
        info_reply(job_info("PIPE", "CRLF")),
        csv_reply("", "part_2==", 2).with_body(first_body),
        csv_reply(
            "Id|Name|NumberOfEmployees\r\n001000000000003AAA|Équipe 😀|\"\"\r\n",
            "null",
            1,
        ),
    ]);
    let responses = script.clone();
    let server = TestServer::start(move |request| {
        if !request.query_all("locator").is_empty() {
            assert!(
                first_finished.load(Ordering::SeqCst),
                "next request preceded first body EOF"
            );
        }
        responses.next()
    })
    .await;

    let export = collect_accounts(&context(&server), job_id()).await.unwrap();

    assert_eq!(export.result_requests, 2);
    assert_eq!(
        export.rows,
        [
            row("001000000000001AAA", "Café | \"North\"\r\nBranch", Some(42)),
            row("001000000000002AAA", "  Padded  ", None),
            row("001000000000003AAA", "Équipe 😀", None),
        ]
    );
    assert_chain(server, script, &[None, Some("part_2==")]).await;
}

#[tokio::test]
async fn empty_set_with_a_locator_does_not_end_the_chain() {
    let (server, script) = scripted(vec![
        info_reply(job_info("COMMA", "LF")),
        csv_reply(HEADER, "after_empty", 0),
        csv_reply(
            "Id,Name,NumberOfEmployees\n001000000000001AAA,Last,0\n",
            "null",
            1,
        ),
    ])
    .await;

    let export = collect_accounts(&context(&server), job_id()).await.unwrap();

    assert_eq!(export.result_requests, 2);
    assert_eq!(export.rows, [row("001000000000001AAA", "Last", Some(0))]);
    assert_chain(server, script, &[None, Some("after_empty")]).await;
}

#[tokio::test]
async fn header_only_last_set_is_a_complete_empty_export() {
    let (server, script) = scripted(vec![
        info_reply(job_info("COMMA", "LF")),
        csv_reply(HEADER, "null", 0),
    ])
    .await;

    let export = collect_accounts(&context(&server), job_id()).await.unwrap();

    assert!(export.rows.is_empty());
    assert_eq!(export.result_requests, 1);
    assert_chain(server, script, &[None]).await;
}

#[tokio::test]
async fn later_http_failure_preserves_the_typed_salesforce_error() {
    let (server, script) = scripted(vec![
        info_reply(job_info("COMMA", "LF")),
        csv_reply(
            "Id,Name,NumberOfEmployees\n001000000000001AAA,First,1\n",
            "failed_part",
            1,
        ),
        Reply::json(
            500,
            serde_json::to_vec(&json!([{
                "errorCode": "UNKNOWN_EXCEPTION",
                "message": "fixture result retrieval failed"
            }]))
            .unwrap(),
        ),
    ])
    .await;

    let error = collect_accounts(&context(&server), job_id())
        .await
        .unwrap_err();

    let ExportError::Salesforce(SfError::Api(failure)) = error else {
        panic!("expected a typed provider failure, got {error:?}");
    };
    assert_eq!(
        failure.status(),
        concord::reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(failure.has_error_code("UNKNOWN_EXCEPTION"));
    assert_eq!(failure.errors().len(), 1);
    assert_chain(server, script, &[None, Some("failed_part")]).await;
}

#[tokio::test]
async fn later_csv_decode_failure_is_typed_and_stops_before_its_next_locator() {
    let (server, script) = scripted(vec![
        info_reply(job_info("COMMA", "LF")),
        csv_reply(HEADER, "bad_csv", 0),
        csv_reply(
            "Id,Name,NumberOfEmployees\n001000000000001AAA,Bad,not-a-number\n",
            "must_not_fetch",
            1,
        ),
    ])
    .await;

    let error = collect_accounts(&context(&server), job_id())
        .await
        .unwrap_err();

    let ExportError::Salesforce(SfError::Read(failure)) = error else {
        panic!("expected a typed CSV read failure, got {error:?}");
    };
    assert_eq!(failure.kind(), concord::stream::ReadErrorKind::Decode);
    assert_chain(server, script, &[None, Some("bad_csv")]).await;
}

#[tokio::test]
async fn later_count_mismatch_is_a_typed_protocol_failure() {
    let (server, script) = scripted(vec![
        info_reply(job_info("COMMA", "LF")),
        csv_reply(HEADER, "wrong_count", 0),
        csv_reply(
            "Id,Name,NumberOfEmployees\n001000000000001AAA,Only,1\n",
            "null",
            2,
        ),
    ])
    .await;

    let error = collect_accounts(&context(&server), job_id())
        .await
        .unwrap_err();

    let ExportError::Salesforce(SfError::Protocol(failure)) = error else {
        panic!("expected a typed count mismatch, got {error:?}");
    };
    assert!(matches!(
        failure.kind(),
        ProtocolErrorKind::RecordCountMismatch {
            declared: 2,
            parsed: 1
        }
    ));
    assert_chain(server, script, &[None, Some("wrong_count")]).await;
}

#[tokio::test]
async fn invalid_job_metadata_is_refused_before_requesting_results() {
    for (field, value) in [
        ("state", json!("InProgress")),
        ("id", json!("750000000000002AAA")),
        ("apiVersion", json!(67.0)),
        ("columnDelimiter", Value::Null),
        ("lineEnding", Value::Null),
    ] {
        let mut info = job_info("COMMA", "LF");
        info[field] = value;
        let (server, script) = scripted(vec![info_reply(info)]).await;

        let error = collect_accounts(&context(&server), job_id())
            .await
            .unwrap_err();

        match (field, error) {
            ("state", ExportError::Salesforce(SfError::Protocol(failure))) => {
                assert!(matches!(failure.kind(), ProtocolErrorKind::JobNotComplete));
            }
            ("id", ExportError::Salesforce(SfError::Protocol(failure))) => {
                assert!(matches!(failure.kind(), ProtocolErrorKind::JobIdChanged));
            }
            ("apiVersion", ExportError::Salesforce(SfError::Protocol(failure))) => {
                assert!(matches!(
                    failure.kind(),
                    ProtocolErrorKind::ApiVersionChanged
                ));
            }
            ("columnDelimiter" | "lineEnding", ExportError::MissingDialect) => {}
            (_, error) => panic!("unexpected error for {field}: {error:?}"),
        }
        assert_chain(server, script, &[]).await;
    }
}
