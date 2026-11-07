//! Secrets: the bearer token (`Intercom::with_token`, `src/lib.rs:106`; `#[sensitive]
//! header(AUTHORIZATION)`, `src/lib.rs:74`), the `#[sensitive]` list cursor `starting_after`
//! (`src/lib.rs:89`) and contact id of `get` (`src/lib.rs:96`), and the values the README says
//! SDK `Debug` withholds ("credentials, cursors, contact IDs, contact data, provider messages and
//! header values", README "Failure semantics"): the search email and search cursor
//! (`EmailSearch`, `src/models.rs:38`), the create email and name (`NewContact`,
//! `src/models.rs:99`), and the provider continuation `pages.next.starting_after` (response-only;
//! `CursorNext`, `src/models.rs:172`). Each has its own canary. The token travels in
//! `Authorization`, the list cursors in the query, the id in the path, the search and create
//! values in the JSON bodies; the provider cursor reaches the decoder from the first list page
//! and travels as the second page's query. The server echoes the token into headers, the error
//! document (`message`, `data`, `request_id`: `ErrorDocument` Debug is withheld), every contact
//! field (`Contact` Debug is withheld) and every malformed, wrong-media and truncated body; the
//! search response echoes the search cursor back as its `next` (withheld by `CursorPages`).
//! Read (`get`) and write (`create`) cover the seven scenarios through the SDK `send()` and the
//! native `Prepared`; list, search, a list traversal and a search traversal that ends with
//! `Error::InvalidCursor` run beside them. A token refused by `with_token` and the generated
//! `IntercomConfig` with an unflagged value are checked too. Not covered: native reqwest values,
//! explicit unredacted access (`ResponseMeta::headers`, public model fields such as
//! `Contact::email`, `ErrorItem::message`, `ErrorDocument::request_id`, `Page::next`, raw
//! Concord error bodies). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, num::NonZeroUsize, time::Duration};

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

// Echo credentials in headers and in diagnostics whose formatting is withheld.
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

use concord::reqwest::header::HeaderValue;
use concord_sdk_intercom::{
    ContactPages, EmailSearch, Error, Intercom, IntercomConfig, NewContact, PageSize, Response,
    Role, pagination::Resume,
};
use std::time::SystemTime;

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error status", &error.status());
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_debug_absent("provider metadata", &p.meta);
            c.assert_debug_absent("provider document", &p.document);
            c.assert_debug_absent("rate limits", &p.rate_limits());
            c.assert_debug_absent("retry after", &p.retry_after(SystemTime::UNIX_EPOCH));
            match &p.document {
                Ok(document) => {
                    c.assert_debug_absent("error items", &document.errors);
                    c.assert_debug_absent("error code", &document.errors[0].code);
                }
                Err(e) => concord_error(c, e),
            }
        }
        Error::InvalidCursor(meta) => c.assert_debug_absent("invalid cursor metadata", meta),
        _ => {}
    }
}

fn sent<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<Response<T>, Error>) {
    c.assert_debug_absent(label, result);
    match result {
        Ok(response) => {
            c.assert_debug_absent("decoded value", &response.value);
            c.assert_debug_absent("response metadata", &response.meta);
        }
        Err(e) => error(c, e),
    }
}

async fn traverse(c: &mut Canaries, mut pages: ContactPages) -> usize {
    c.assert_debug_absent("traversal", &pages);
    let mut read = 0;
    loop {
        let page = pages.next_page().await;
        c.assert_debug_absent("traversal page Result", &page);
        match page {
            Ok(Some(page)) => {
                c.assert_debug_absent("contacts", &page.items);
                c.assert_debug_absent("page metadata", &page.meta);
                read += 1;
            }
            Ok(None) => return read,
            Err(e) => {
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                c.assert_debug_absent("resume", &e.resume());
                error(c, e.provider_error().unwrap());
                c.assert_debug_absent("ended traversal", &pages.next_page().await);
                return read;
            }
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("intercom");
    let token = c.issue();
    let list_cursor = c.issue();
    let id = c.issue();
    let email = format!("{}@example.test", c.issue());
    let search_cursor = c.issue();
    let new_email = format!("{}@example.test", c.issue());
    let new_name = c.issue();
    let provider_cursor = c.issue();
    let search = || EmailSearch::new(&email).with_starting_after(Some(search_cursor.clone()));
    let contact = || NewContact {
        email: new_email.clone(),
        name: Some(new_name.clone()),
        role: Some(Role::Lead),
    };
    c.assert_debug_absent("email search", &search());
    c.assert_debug_absent("new contact", &contact());
    let config = IntercomConfig {
        authorization: HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    };
    c.assert_debug_absent("generated config (value not flagged)", &config);
    c.assert_debug_absent("client from config", &Intercom::new(http(), config));
    let cap = NonZeroUsize::new(4).unwrap();
    for case in 0..7 {
        let (echo, next, repeat) = (
            token.clone(),
            provider_cursor.clone(),
            search_cursor.clone(),
        );
        let server = TestServer::start(move |r: &Recorded| {
            let one = format!(
                r#"{{"type":"contact","id":"{echo}","role":"lead","external_id":"{echo}","email":"{echo}","name":"{echo}"}}"#
            );
            let success = match (r.method.as_str(), r.path()) {
                ("GET", "/contacts") if r.query_all("starting_after") == [next.clone()] => {
                    format!(r#"{{"type":"list","data":[{one}],"total_count":2,"pages":null}}"#)
                }
                ("GET", "/contacts") => format!(
                    r#"{{"type":"list","data":[{one}],"total_count":2,"pages":{{"next":{{"starting_after":"{next}"}}}}}}"#
                ),
                ("POST", "/contacts/search") => format!(
                    r#"{{"type":"list","data":[{one}],"total_count":1,"pages":{{"next":{{"starting_after":"{repeat}"}}}}}}"#
                ),
                _ => one,
            };
            let problem = format!(
                r#"{{"type":"error.list","request_id":"{echo}","errors":[{{"code":"unauthorized","message":"{echo}","data":{{"echo":"{echo}"}}}}]}}"#
            );
            reply(case, &success, &problem, &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api = Intercom::with_token(http(), &token)
            .unwrap()
            .with_base_url(base)
            .unwrap();
        let contacts = api.contacts();
        let read = contacts.get(id.clone());
        let write = contacts.create(contact());
        let list = contacts.list().set_starting_after(list_cursor.clone());
        let find = contacts.search(search());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("contacts scope", &contacts);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("list endpoint", &list);
        c.assert_debug_absent("search endpoint", &find);
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        c.assert_debug_absent("list Prepared", &list.clone().prepare().unwrap());
        c.assert_debug_absent("search Prepared", &find.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        sent(&mut c, "read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            // Withholding keeps the diagnosis: status, stage and operation name.
            match case {
                1 => {
                    assert_eq!(e.status().map(|s| s.as_u16()), Some(401));
                    assert_eq!(e.to_string(), "Intercom returned HTTP 401");
                }
                3 => {
                    let Error::Concord(inner) = e else {
                        panic!("{e}")
                    };
                    assert_eq!(inner.kind(), concord::ErrorKind::Decode);
                    assert_eq!(inner.endpoint(), Some("Intercom.contacts.get"));
                    assert_eq!(e.status().map(|s| s.as_u16()), Some(200));
                }
                _ => {}
            }
        }
        let result = write.send().await;
        sent(&mut c, "write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let result = list.clone().send().await;
        sent(&mut c, "list Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let result = find.clone().send().await;
        sent(&mut c, "search Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let pages = traverse(&mut c, list.pages(Resume::Start, cap)).await;
        assert_eq!(pages, if case == 0 { 2 } else { 0 });
        let repeated = find.pages(Resume::At(search_cursor.clone()), cap);
        assert_eq!(traverse(&mut c, repeated).await, 0);
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    let e = PageSize::new(0).unwrap_err();
    error(&mut c, &e);
    assert!(c.checked() >= 250);
    println!(
        "intercom: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_token_never_prints() {
    let mut c = Canaries::new("intercom");
    let token = format!("{}\r\nx-injected: 1", c.issue());
    let e = Intercom::with_token(http(), &token).unwrap_err();
    c.assert_debug_absent("credential error", &e);
    c.assert_error_absent("credential error and sources", &e);
}
