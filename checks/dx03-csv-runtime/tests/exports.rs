//! Wire-level tests against the loopback service.

use std::time::Duration;

use concord::{
    ErrorKind,
    bytes::Bytes,
    reqwest::{self, StatusCode},
    stream::{OpenErrorKind, ReadErrorKind},
};
use concord_check_dx03_csv_runtime::{
    ExportError, JobDescriptor, ReportRow, client, open_export, upload_export,
};
use concord_example_testserver::{Body, Reply, TestServer, paused};

fn job(header: bool, delimiter: &str, columns: &[&str]) -> JobDescriptor {
    JobDescriptor {
        header,
        delimiter: delimiter.into(),
        columns: columns.iter().map(|s| (*s).to_owned()).collect(),
    }
}
fn row(id: u64, amount: u64) -> ReportRow {
    ReportRow { id, amount }
}

#[tokio::test]
async fn fixed_report_decodes_and_rejects_contradictory_header() {
    let server = TestServer::start(|request| match request.path() {
        "/reports/ok" => Reply::text(
            200,
            "text/csv; header=present",
            "id;amount\r\n7;45\r\n8;90\r\n",
        ),
        _ => Reply::text(200, "text/csv; header=absent", "id;amount\n7;45\n"),
    })
    .await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    assert_eq!(
        api.report("ok").send().await.unwrap(),
        vec![row(7, 45), row(8, 90)]
    );
    let error = api.report("contradiction").send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert!(error.codec_error().is_some());
    let response = error.into_response().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        b"id;amount\n7;45\n"
    );
    assert_eq!(server.requests()[0].method, "GET");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn runtime_reader_delivers_first_row_before_remaining_body_and_keeps_metadata() {
    let server = TestServer::start(|_| {
        Reply::text(200, "text/csv; header=absent", "")
            .header("x-export-id", "exp-42")
            .header("x-request-id", "req-7")
            .with_body(Body::Chunks(vec![
                (Duration::ZERO, Bytes::from_static(b"35|1\n")),
                (Duration::from_secs(2), Bytes::from_static(b"90|2\n")),
            ]))
    })
    .await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    let descriptor: JobDescriptor =
        serde_json::from_str(r#"{"header":false,"delimiter":"|","columns":["amount","id"]}"#)
            .unwrap();
    let mut export = open_export(&api, "job-1", &descriptor).await.unwrap();
    assert_eq!(export.metadata.status, StatusCode::OK);
    assert_eq!(export.metadata.export_id.as_ref().unwrap(), "exp-42");
    assert_eq!(export.metadata.request_id.as_ref().unwrap(), "req-7");
    // Clock frozen while the call runs: the bound fails only a call that waits for something
    // that never comes (then reported as stuck).
    let first = tokio::time::timeout(Duration::from_secs(1), paused::frozen(export.rows.next()))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first, row(1, 35));
    assert_eq!(export.rows.next().await.unwrap().unwrap(), row(2, 90));
    assert!(export.rows.next().await.is_none());
    assert_eq!(server.single().path(), "/jobs/job-1/export");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn refused_response_stays_unread_and_inspectable() {
    let server = TestServer::start(|_| {
        Reply::json(404, r#"{"error":"job missing"}"#).header("x-request-id", "req-404")
    })
    .await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    let Err(ExportError::Open(error)) =
        open_export(&api, "missing", &job(false, "|", &["id", "amount"])).await
    else {
        panic!("expected OpenError")
    };
    assert_eq!(error.kind(), OpenErrorKind::Status);
    assert_eq!(error.operation(), Some("Exports.download"));
    assert_eq!(error.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(error.response().headers()["x-request-id"], "req-404");
    let json: serde_json::Value =
        serde_json::from_slice(&error.into_response().bytes().await.unwrap()).unwrap();
    assert_eq!(json["error"], "job missing");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn read_failures_report_delivered_rows() {
    let server = TestServer::start(|request| match request.path() {
        "/jobs/broken/export" => {
            Reply::text(200, "text/csv; header=present", "").with_body(Body::FailAfter(vec![
                Bytes::from_static(b"id|amount\n1|35\n2|90\n"),
            ]))
        }
        "/jobs/bad-row/export" => {
            Reply::text(200, "text/csv; header=present", "id|amount\n1|35\n2|bad\n")
        }
        path => panic!("unexpected export path: {path}"),
    })
    .await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    let descriptor = job(true, "|", &["id", "amount"]);
    for (id, count, kind) in [
        ("broken", 2, ReadErrorKind::Transport),
        ("bad-row", 1, ReadErrorKind::Decode),
    ] {
        let mut export = open_export(&api, id, &descriptor).await.unwrap();
        for _ in 0..count {
            export.rows.next().await.unwrap().unwrap();
        }
        let error = export.rows.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.position().records, count);
        assert_eq!(error.operation(), Some("Exports.download"));
        assert!(export.rows.next().await.is_none());
        assert_eq!(export.metadata.status, StatusCode::OK);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn upload_matches_runtime_dialect_on_wire() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    upload_export(
        &api,
        "first",
        &job(false, "|", &["id", "amount"]),
        vec![row(1, 35), row(2, 90)],
    )
    .await
    .unwrap();
    upload_export(
        &api,
        "second",
        &job(true, ";", &["id", "amount"]),
        vec![row(7, 45)],
    )
    .await
    .unwrap();
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path(), "/jobs/first/export");
    assert_eq!(
        requests[0].header_all("content-type"),
        vec!["text/csv; header=absent"]
    );
    assert_eq!(requests[0].body, b"1|35\r\n2|90\r\n");
    assert_eq!(
        requests[1].header_all("content-type"),
        vec!["text/csv; header=present"]
    );
    assert_eq!(requests[1].body, b"id;amount\r\n7;45\r\n");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn invalid_runtime_options_fail_before_request() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let api = client(reqwest::Client::new(), &server.url()).unwrap();
    assert!(matches!(
        open_export(&api, "x", &job(false, "||", &["id", "amount"])).await,
        Err(ExportError::Delimiter)
    ));
    assert!(matches!(
        upload_export(&api, "x", &job(false, "\n", &["id", "amount"]), vec![]).await,
        Err(ExportError::Config(_))
    ));
    assert!(server.requests().is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}
