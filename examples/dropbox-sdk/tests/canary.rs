//! Secrets: ClientConfig.token -> bearer header; member and root namespace -> sensitive
//! selection headers; provider cursor -> list_folder_continue JSON body. Each has its own
//! canary. All seven scenarios cover the read-only listing; a successful continuation sends
//! the cursor too. Context, cursor, config, metadata, errors and page wrappers are formatted.
//! Excludes native reqwest, explicit raw accessors and clear provider entry/error values.
//! The transport module, scope, endpoint and Prepared are private and cannot be constructed
//! through the public SDK. Root/namespace Debug is checked in separate regression tests.
//! No wire exemptions.

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

use concord_sdk_dropbox::{
    Client, ClientConfig, Error, ListFolderOptions, NamespaceId, PathRoot, RequestContext,
};

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("dropbox");
    let token = c.issue();
    let member = c.issue();
    let root = c.issue();
    let cursor = c.issue();
    let namespace = c.issue();
    for case in 0..7 {
        let echo = token.clone();
        let next = cursor.clone();
        let server = TestServer::start(move |_| {
            let success = format!(r#"{{"entries":[],"cursor":"{next}","has_more":false}}"#);
            let problem = format!(r#"{{"error_summary":"{echo}","error":{{".tag":"path","path":{{".tag":"not_found"}}}}}}"#);
            reply(case, &success, &problem, &echo)
        }).await;
        let config = ClientConfig {
            token: token.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Client::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let context = RequestContext::new(&member, PathRoot::root(&root).unwrap()).unwrap();
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("context", &context);
        let result = api
            .list_folder(context.clone(), ListFolderOptions::default())
            .await;
        c.assert_debug_absent("SDK Result", &result);
        match result {
            Ok(response) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("response", &response);
                c.assert_debug_absent("metadata", &response.meta);
                c.assert_debug_absent("cursor", &response.value.cursor);
                let continued = api
                    .list_folder_continue(&response.value.cursor)
                    .await
                    .unwrap();
                c.assert_debug_absent("continued response", &continued);
                let mut pages = api.list_folder_pages(context, ListFolderOptions::default());
                let page = pages.next_page().await.unwrap().unwrap();
                c.assert_debug_absent("pagination page", &page);
                c.assert_debug_absent("page report", &page.meta);
                let context =
                    RequestContext::new(&member, PathRoot::namespace(&namespace).unwrap()).unwrap();
                c.assert_debug_absent("namespace context", &context);
                let response = api
                    .list_folder(context, ListFolderOptions::default())
                    .await
                    .unwrap();
                c.assert_debug_absent("namespace response", &response);
                c.assert_on_wire(&server.requests());
            }
            Err(error) => {
                assert_ne!(case, 0);
                c.assert_debug_absent("SDK error", &error);
                c.assert_error_absent("SDK error and sources", &error);
                if let Some(meta) = error.meta() {
                    c.assert_debug_absent("error metadata", meta);
                }
                match &error {
                    Error::Concord { error, .. } => concord_error(&mut c, error),
                    Error::Http(http) => {
                        c.assert_debug_absent("HTTP failure", http);
                        c.assert_debug_absent("ErrorBody", &http.body);
                        if let Some(e) = http.body.read_error() {
                            concord_error(&mut c, e);
                        }
                    }
                    _ => {}
                }
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!(
        "dropbox: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn root_debug_withholds_sensitive_header_value() {
    let mut c = Canaries::new("dropbox");
    let root = PathRoot::root(c.issue()).unwrap();
    c.assert_debug_absent("PathRoot Debug", &root);
}

#[test]
fn namespace_debug_withholds_sensitive_header_value() {
    let mut c = Canaries::new("dropbox");
    let namespace = NamespaceId::new(c.issue()).unwrap();
    c.assert_debug_absent("NamespaceId Debug", &namespace);
}
