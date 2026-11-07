//! Secret input: FredConfig.api_key (#[sensitive] query, src/lib.rs:78), sent in every
//! request. A successful ZIP fixture also puts the key in CSV/README bytes to check
//! CsvExport through its real decoder. Provider Problem.error_message and response headers echo that canary only in
//! renderers promising to withhold them. Read-only SDK; all seven failure/success scenarios.
//! Excludes native reqwest, message_unredacted/source_unredacted/body_unredacted, and
//! clear data models. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_sdk_fred::concord_pagination::Resume;
use concord_sdk_fred::{Error, Fred, FredConfig, pages};
use std::{
    io::{Cursor, Write},
    num::NonZeroUsize,
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn csv_export(echo: &str) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("SYN_A.csv", SimpleFileOptions::default())
        .unwrap();
    write!(zip, "observation_date,SYN_A\n2026-01-01,{echo}\n").unwrap();
    zip.start_file("README.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(echo.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(h) => {
            c.assert_debug_absent("HTTP failure", h);
            c.assert_debug_absent("HTTP metadata", &h.meta);
            c.assert_debug_absent("provider document", &h.document);
            c.assert_debug_absent("retry information", &h.retry_after);
            if let Err(e) = &h.document {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("fred");
    let key = c.issue();
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |r: &Recorded| {
            if case == 0 && r.query_all("file_type") == ["csv"] {
                return Reply::text(200, "application/zip", csv_export(&echo));
            }
            let problem = format!(r#"{{"error_code":400,"error_message":"{echo}"}}"#);
            reply(
                case,
                "application/json",
                include_str!("../fixtures/observations-0.json"),
                &problem,
                &echo,
            )
        })
        .await;
        let config = FredConfig {
            api_key: key.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Fred::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let series = api.series("SYN_A");
        let op = series.observations_json();
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("series scope", &series);
        c.assert_debug_absent("read endpoint", &op);
        receive(&mut c, op.clone().prepare().unwrap()).await;
        let result = op.clone().send().await;
        c.assert_debug_absent("SDK Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
            if case == 1 {
                assert!(e.to_string().contains("400"));
                let Error::Http(h) = &e else {
                    panic!("HTTP failure expected")
                };
                assert_eq!(h.document.as_ref().unwrap().message_unredacted(), key);
            }
        }
        let mut walk = pages(op, Resume::Start, NonZeroUsize::MIN);
        c.assert_debug_absent("observation traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            let op = series.export_csv();
            c.assert_debug_absent("CSV export endpoint", &op);
            receive(&mut c, op.clone().prepare().unwrap()).await;
            let export = op.send().await.unwrap();
            c.assert_debug_absent("CSV export wrapper", &export);
            assert!(String::from_utf8_lossy(export.csv()).contains(&key));
            assert_eq!(export.readme().unwrap(), key.as_bytes());
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let e = Fred::new(
        http(),
        FredConfig {
            api_key: key.clone(),
        },
    )
    .with_base_url(format!("http://user:{key}@invalid.test"))
    .unwrap_err();
    concord_error(&mut c, &e);
    assert!(c.checked() >= 150);
    println!("fred: {} renderings; positive control passed", c.checked());
}
