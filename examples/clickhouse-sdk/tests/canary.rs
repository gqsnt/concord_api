//! Secrets: distinct Basic username/password and header username/password; Basic values
//! travel in Authorization (decoded by the positive control), header values in
//! X-ClickHouse-User/Key. Bound query values are also withheld by Query/Param Debug.
//! Select and streamed insert cover all seven cases. Insert accepts arbitrary drained response
//! bytes/media on success; those cases assert rendering safety rather than inventing a decoder.
//! Excludes native reqwest, explicit SQL/parameter/message/header accessors and clear rows.
//! Private wire client has no public scope/Prepared. The header-auth username
//! (`X-ClickHouse-User`) is a credential input of `Credentials::headers`, so it carries a canary
//! like the key; the header-mode credential and client Debug are checked below. No wire
//! exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

// Every credential is echoed in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_clickhouse::{
    ClickHouse, Credentials, Error, Identifier, OutputFormat, Query, Table, stream,
};
use std::convert::Infallible;

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Some(meta) = error.meta() {
        c.assert_debug_absent("error metadata", meta);
    }
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Server(e) => {
            c.assert_debug_absent("server exception", e);
            c.assert_display_absent("server exception", e);
        }
        Error::Read(e) => {
            c.assert_debug_absent("read failure", e);
            c.assert_display_absent("read failure", e);
            c.assert_debug_absent("reader error", e.read_error());
            c.assert_error_absent("reader sources", e.read_error());
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("clickhouse");
    let user = c.issue();
    let password = c.issue();
    let header_user = c.issue();
    let header_password = c.issue();
    let parameter = c.issue();
    let param = concord_sdk_clickhouse::Param::from(parameter.as_str());
    c.assert_debug_absent("Param", &param);
    for case in 0..7 {
        let echo = password.clone();
        let server = TestServer::start(move |r: &concord_example_testserver::Recorded| {
            let success = if r.method == "GET" { "{\"n\":1}\n" } else { "" };
            let problem = format!("Code: 516. DB::Exception: {echo} (AUTHENTICATION_FAILED)");
            let response = reply(case, success, &problem, &echo)
                .header("x-clickhouse-format", "JSONEachRow")
                .header("x-clickhouse-exception-tag", "canarytag");
            // JSONEachRow has its own content type, unlike the default JSON helper.
            if case == 0 || case == 6 {
                Reply::text(200, "application/x-ndjson", success)
                    .header("x-clickhouse-format", "JSONEachRow")
                    .header("x-clickhouse-exception-tag", "canarytag")
                    .header("x-request-id", &echo)
            } else {
                response
            }
        })
        .await;
        let credentials = Credentials::basic(&user, &password).unwrap();
        c.assert_debug_absent("Basic credentials", &credentials);
        let api = ClickHouse::new(http(), credentials)
            .with_base_url(&if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let query = Query::new("SELECT {secret:String} AS n")
            .bind("secret", parameter.as_str())
            .unwrap();
        c.assert_debug_absent("query", &query);
        let read = api.select(query, OutputFormat::JsonEachRow);
        let write = api.insert(Table::new(Identifier::new("canary_table").unwrap()));
        c.assert_debug_absent("select handle", &read);
        c.assert_debug_absent("insert handle", &write);
        let result = read.rows::<serde_json::Value>().await;
        c.assert_debug_absent("open rows Result", &result);
        let result = match result {
            Ok(rows) => {
                c.assert_debug_absent("Rows", &rows);
                c.assert_debug_absent("row metadata", rows.meta());
                rows.collect().await
            }
            Err(e) => Err(e),
        };
        c.assert_debug_absent("collected Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(collected) => c.assert_debug_absent("collection report", &collected),
            Err(e) => error(&mut c, &e),
        }
        let result = write
            .json_each_row(stream::from_iter([Ok::<_, Infallible>(
                serde_json::json!({"n":1}),
            )]))
            .await;
        c.assert_debug_absent("insert Result", &result);
        match result {
            Ok(receipt) => {
                assert!(matches!(case, 0 | 3 | 4));
                c.assert_debug_absent("insert receipt", &receipt);
                c.assert_debug_absent("insert metadata", receipt.meta());
            }
            Err(e) => {
                assert_ne!(case, 0);
                error(&mut c, &e);
            }
        }
        if case == 0 {
            let credentials = Credentials::headers(&header_user, &header_password).unwrap();
            let headers = ClickHouse::new(http(), credentials)
                .with_base_url(&server.url())
                .unwrap();
            headers
                .select(Query::new("SELECT 1"), OutputFormat::JsonEachRow)
                .rows::<serde_json::Value>()
                .await
                .unwrap()
                .collect()
                .await
                .unwrap();
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!(
        "clickhouse: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn header_credentials_and_client_withhold_user_and_key() {
    let mut c = Canaries::new("clickhouse");
    let user = c.issue();
    let key = c.issue();
    let credentials = Credentials::headers(&user, &key).unwrap();
    c.assert_debug_absent("Credentials::Headers Debug", &credentials);
    let api = ClickHouse::new(http(), credentials);
    c.assert_debug_absent("ClickHouse Debug with header credentials", &api);
}
