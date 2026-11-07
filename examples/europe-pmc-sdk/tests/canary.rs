//! No credentials/authentication exist. Withheld inputs are the sensitive search Query and
//! JSON/XML cursors (src/lib.rs:79,83,88). Source/PublicationId are public publication
//! identifiers: their own Debug redacts (a blanket rule for the SDK's text types,
//! src/input.rs:41), but the declaration leaves the publication scope's path values unmarked
//! and the README's withholding promise for generated handles names search expressions,
//! cursors, header values and bodies only, so generated scope Debug and prepared URLs show them
//! (kept for diagnosis; not an I2 leak). The main test uses distinct query/JSON-cursor/XML-cursor
//! canaries.
//! Search JSON and XML cover all seven scenarios. Success fixtures echo the query into every
//! withheld publication/reference text field and cursor fields, reaching both decoders.
//! Publication/reference scopes and traversals are checked too. No writes; no wire exemptions.
//! Excludes native reqwest, explicit raw body/header/as_str access and typed Display values:
//! Query/Cursor/Source/PublicationId explicitly document Display as the clear wire value.
use concord_check_canary::Canaries;
use concord_testserver::{Body, Recorded, Reply, TestServer};
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

// Echo only into withheld diagnostics, never clear success-model fields.
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(status, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(status, format!("{{invalid {echo}")),
        4 => Reply::text(status, "text/html", echo),
        5 => Reply::json(status, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response.header("x-canary", echo).header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_europe_pmc::concord_pagination::Resume;
use concord_sdk_europe_pmc::{
    Cursor, Error, EuropePmc, PageSize, PublicationId, Query, SearchFormat, Source,
};
use std::num::NonZeroUsize;

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Some(e) = error.call_error() {
        concord_error(c, e);
    }
    if let Error::Http(e) = error {
        c.assert_debug_absent("HTTP failure", e);
        c.assert_debug_absent("HTTP failure metadata", &e.meta);
        if let Err(e) = &e.body {
            concord_error(c, e);
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("epmc");
    let query = Query::new(c.issue()).unwrap();
    let json_cursor = Cursor::new(c.issue()).unwrap();
    let xml_cursor = Cursor::new(c.issue()).unwrap();
    let source = Source::new("MED").unwrap();
    let id = PublicationId::new("1").unwrap();
    c.assert_debug_absent("Query", &query);
    c.assert_debug_absent("JSON Cursor", &json_cursor);
    c.assert_debug_absent("XML Cursor", &xml_cursor);
    c.assert_debug_absent("Source", &source);
    c.assert_debug_absent("PublicationId", &id);
    for case in 0..7 {
        let (echo, response_source, response_id, response_json_cursor, response_xml_cursor) = (
            query.as_str().to_owned(),
            source.clone(),
            id.clone(),
            json_cursor.as_str().to_owned(),
            xml_cursor.as_str().to_owned(),
        );
        let server = TestServer::start(move |r: &Recorded| {
            let record = format!(r#"{{"id":"{response_id}","source":"{response_source}","title":"{echo}","authorString":"{echo}","journalTitle":"{echo}","journalAbbreviation":"{echo}","pubYear":"{echo}","doi":"{echo}","abstractText":"{echo}"}}"#);
            let xml = r.query_all("format") == ["xml"];
            let success = if r.path().ends_with("/references") {
                format!(r#"{{"hitCount":1,"referenceList":{{"reference":[{record}]}}}}"#)
            } else if r.path().ends_with("/citations") {
                format!(r#"{{"hitCount":1,"citationList":{{"citation":[{record}]}}}}"#)
            } else if xml {
                format!("<responseWrapper><hitCount>1</hitCount><nextCursorMark>{response_xml_cursor}</nextCursorMark><resultList><result><id>{response_id}</id><source>{response_source}</source><title>{echo}</title><authorString>{echo}</authorString><journalTitle>{echo}</journalTitle><pubYear>{echo}</pubYear><doi>{echo}</doi><abstractText>{echo}</abstractText></result></resultList></responseWrapper>")
            } else {
                format!(r#"{{"hitCount":1,"nextCursorMark":"{response_json_cursor}","resultList":{{"result":[{record}]}}}}"#)
            };
            let response = if xml && matches!(case, 0 | 3 | 5 | 6) {
                match case {
                    3 => Reply::text(200, "application/xml", format!("<invalid>{echo}")),
                    5 => Reply::text(200, "application/xml", "").with_body(Body::FailAfter(vec![format!("<incomplete>{echo}").into()])),
                    _ => Reply::text(200, "application/xml", success),
                }
            } else {
                reply(case, 200, &success, &format!(r#"{{"errMsg":"{echo}"}}"#), &echo)
            };
            response.header("x-private", &echo)
        }).await;
        let api = EuropePmc::new(http())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.search(query.clone(), PageSize::default());
        let json = scope.json().set_cursor(json_cursor.clone());
        let xml = scope.xml().set_cursor(xml_cursor.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("search scope", &scope);
        c.assert_debug_absent("JSON endpoint", &json);
        c.assert_debug_absent("XML endpoint", &xml);
        c.assert_debug_absent("JSON Prepared", &json.clone().prepare().unwrap());
        c.assert_debug_absent("XML Prepared", &xml.clone().prepare().unwrap());
        for native in [
            receive(&mut c, json.clone().prepare().unwrap()).await,
            receive(&mut c, xml.clone().prepare().unwrap()).await,
        ] {
            c.assert_debug_absent("native Result", &native);
            assert_eq!(native.is_ok(), case == 0);
            if let Err(e) = native {
                concord_error(&mut c, &e);
            }
        }
        for (response, expected_cursor) in [
            (json.send().await, json_cursor.as_str()),
            (xml.send().await, xml_cursor.as_str()),
        ] {
            c.assert_debug_absent("SDK Result", &response);
            assert_eq!(response.is_ok(), case == 0);
            match response {
                Ok(page) => {
                    c.assert_debug_absent("SearchPage", &page);
                    c.assert_debug_absent("decoded response cursor", &page.next_cursor_mark);
                    assert_eq!(
                        page.next_cursor_mark.as_ref().unwrap().as_str(),
                        expected_cursor
                    );
                    c.assert_debug_absent("publications", &page.result_list.result);
                    assert_eq!(
                        page.result_list.result[0].title.as_deref(),
                        Some(query.as_str())
                    );
                }
                Err(e) => {
                    error(&mut c, &e);
                    if case == 1 {
                        assert_eq!(e.status().unwrap().as_u16(), 401);
                    }
                }
            }
        }
        let mut pages = scope
            .pages(
                SearchFormat::Json,
                Resume::Start,
                NonZeroUsize::new(2).unwrap(),
            )
            .unwrap();
        c.assert_debug_absent("search traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("search traversal page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError and sources", &e);
            c.assert_display_absent("PagesError", &e);
            error(&mut c, e.provider_error().unwrap());
        }
        if case == 0 {
            let publication = api.publication(source.clone(), id.clone());
            let references = publication.references(PageSize::default());
            c.assert_debug_absent("publication scope", &publication);
            c.assert_debug_absent("references scope", &references);
            let citations = publication.citations();
            c.assert_debug_absent("citations endpoint", &citations);
            c.assert_debug_absent("citations Result", &citations.send().await.unwrap());
            let references = references.json();
            c.assert_debug_absent("references endpoint", &references);
            let response = references.send().await.unwrap();
            c.assert_debug_absent("references page", &response);
            c.assert_debug_absent("references", &response.reference_list.reference);
            assert_eq!(
                response.reference_list.reference[0].title.as_deref(),
                Some(query.as_str())
            );
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "europe-pmc: {} renderings; positive control passed",
        c.checked()
    );
}
