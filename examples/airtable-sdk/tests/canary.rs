//! Secrets (`src/lib.rs:185-194`): the `#[sensitive]` bearer token (`Client::new`, line 279,
//! sent as `Authorization: Bearer`), the `#[sensitive]` `filterByFormula` query value
//! (`ListOptions::filter_by_formula`) and the `#[sensitive]` `offset` query value (the resume
//! offset of `list_page` and `list_pages`). Each has its own canary. Read-only SDK: `list_page`
//! and the `list_pages` traversal (resumed at the offset) cover the seven scenarios; the server
//! echoes the token into headers, the provider error message (withheld by `ProviderError`'s
//! `Debug`) and every non-success body. The client, response, metadata, page, `Resume`,
//! traversal, `PagesError`, `Incomplete`, `Error` and its sources are formatted.
//! Not covered: native reqwest values; the generated `AirtableApi` (crate-private, so no public
//! endpoint or `Prepared`); decoded models returned in clear (`Record::fields`, and
//! `RecordPage::offset`, a clear public field with derived `Debug`, so the success page carries
//! no offset instead of echoing the canary). `ListOptions` Debug withholding the formula is
//! checked separately below. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

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

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(403, problem),
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

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_airtable::pagination::Resume;
use concord_sdk_airtable::{Client, Error, ListOptions};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    c.assert_debug_absent("provider error", &error.provider());
    match error {
        Error::Concord { source, .. } => concord_error(c, source),
        Error::Http {
            body_error: Some(e),
            ..
        } => concord_error(c, e),
        _ => {}
    }
}

fn options(filter: &str) -> ListOptions {
    ListOptions {
        filter_by_formula: Some(filter.to_owned()),
        fields: vec!["Name".into()],
        page_size: Some(10),
        max_records: Some(20),
        view: Some("Grid view".into()),
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("airtable");
    let token = c.issue();
    let filter = c.issue();
    let offset = c.issue();
    let cap = NonZeroUsize::new(2).unwrap();
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |_| {
            let success = r#"{"records":[{"id":"recA","createdTime":"2026-11-06T00:00:00.000Z","fields":{"Name":"A"}}]}"#;
            let problem = format!(
                r#"{{"error":{{"type":"INVALID_PERMISSIONS_OR_MODEL_NOT_FOUND","message":"{echo}"}}}}"#
            );
            reply(case, success, &problem, &echo)
        })
        .await;
        let api = Client::new(http(), &token)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let result = api
            .list_page("appBase", "Tasks", &options(&filter), Some(&offset))
            .await;
        c.assert_debug_absent("page Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("response", &response);
                c.assert_debug_absent("record page", &response.value);
                c.assert_debug_absent("metadata", &response.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let from = Resume::At(offset.clone());
        c.assert_debug_absent("Resume", &from);
        let mut pages = api.list_pages("appBase", "Tasks", options(&filter), from.clone(), cap);
        c.assert_debug_absent("traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("traversal page Result", &page);
        match page {
            Ok(page) => {
                assert_eq!(case, 0);
                let page = page.unwrap();
                c.assert_debug_absent("traversal page", &page);
                c.assert_debug_absent("traversal page metadata", &page.meta);
                c.assert_debug_absent("exhausted traversal", &pages.next_page().await);
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                c.assert_debug_absent("failed position", &e.resume());
                error(&mut c, e.provider_error().unwrap());
                let after = pages.next_page().await.unwrap_err();
                c.assert_debug_absent("ended traversal", &after);
                c.assert_display_absent("ended traversal", &after);
            }
        }
        let collected = api
            .list_pages("appBase", "Tasks", options(&filter), from, cap)
            .items()
            .collect_complete()
            .await;
        c.assert_debug_absent("collected Result", &collected);
        if let Err(incomplete) = &collected {
            c.assert_debug_absent("Incomplete", incomplete);
            c.assert_display_absent("Incomplete", incomplete);
            c.assert_debug_absent("Incomplete error", &incomplete.error);
            error(&mut c, incomplete.error.provider_error().unwrap());
        }
        assert_eq!(collected.is_ok(), case == 0);
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "airtable: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn list_options_debug_withholds_the_formula() {
    let mut c = Canaries::new("airtable");
    let filter = c.issue();
    c.assert_debug_absent("ListOptions", &options(&filter));
}
