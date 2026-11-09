//! Secrets: the API key (`Mailgun::local`/`connect`, `src/lib.rs:101-106`; `#[sensitive]
//! header(AUTHORIZATION)`, `src/lib.rs:64`, sent as `Basic api:<key>`); the `#[sensitive]`
//! event filters `message-id`, `recipient` and `tags` (`src/lib.rs:86-90`); every message
//! field `TextEmail`'s `Debug` withholds (from, to, subject, text, an `o:tag`, a `v:` variable)
//! and the attachment's filename and bytes (`Attachment`); the provider continuation
//! (`EventCursor`, `Paging`: "Debug never renders them", `src/events.rs:97,239`). Each has its
//! own canary. The cursor comes from the server's `paging.next` and travels as the next page's
//! path. The server echoes the key into headers, the provider `message` (withheld by
//! `Problem`), the success payloads (withheld by `Accepted`, `EventPage`, `Event`) and every
//! malformed, wrong-media and truncated body. Read (`events`) and write (`send_message`) cover
//! the seven scenarios through the generated `send()` and the native `Prepared`; the success
//! case also walks `pages`. A key refused by `local` gets its own canary in a separate test.
//! Not covered: native reqwest values; explicit clear accessors (`MessageId::as_str`, public
//! model fields such as `Problem::message`, `Event::recipient`, `Paging::next`). No wire
//! exemptions in the main test.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

fn refused() -> String {
    concord_example_testserver::refused_url()
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
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord::reqwest::header::HeaderValue;
use concord_sdk_mailgun::{
    Attachment, Domain, Error, EventRange, Mailgun, MailgunConfig, MessageId, Metadata, TextEmail,
};

fn metadata(c: &mut Canaries, meta: &Metadata) {
    c.assert_debug_absent("metadata", meta);
    c.assert_debug_absent("rate limit", &meta.rate_limit());
    c.assert_debug_absent("rate remaining", &meta.rate_remaining());
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) | Error::AmbiguousSend(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_debug_absent("provider document", &p.document);
            c.assert_debug_absent("status class", &p.class());
            metadata(c, &p.meta);
            if let Err(e) = &p.document {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

struct Message {
    from: String,
    to: String,
    subject: String,
    text: String,
    filename: String,
    bytes: String,
    tag: String,
    variable: String,
}

impl Message {
    // `TextEmail` and `Attachment` are single-use: build one per send.
    fn email(&self) -> TextEmail {
        let attachment = Attachment::bytes(
            &self.filename,
            "text/plain",
            self.bytes.clone().into_bytes(),
        )
        .unwrap();
        TextEmail::new(
            format!("Sender <{}@mg.example.test>", self.from),
            vec![format!("{}@example.test", self.to)],
            &self.subject,
            &self.text,
            attachment,
        )
        .unwrap()
        .tag(&self.tag)
        .unwrap()
        .variable("ref", self.variable.clone())
        .unwrap()
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("mailgun");
    let key = c.issue();
    let message_id = MessageId::new(format!("<{}@mg.example.test>", c.issue())).unwrap();
    let recipient = format!("{}@example.test", c.issue());
    let tags = c.issue();
    let cursor = c.issue();
    let message = Message {
        from: c.issue(),
        to: c.issue(),
        subject: c.issue(),
        text: c.issue(),
        filename: format!("{}.txt", c.issue()),
        bytes: c.issue(),
        tag: c.issue(),
        variable: c.issue(),
    };
    c.assert_debug_absent("message id", &message_id);
    c.assert_debug_absent("email", &message.email());
    let range = EventRange::new(1700000000, 1700000100).unwrap();
    let cap = NonZeroUsize::new(3).unwrap();
    for case in 0..7 {
        let (echo, cursor) = (key.clone(), cursor.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let success = match (r.method.as_str(), r.path()) {
                ("POST", _) => format!(r#"{{"id":"<{echo}@mg.example.test>","message":"{echo}"}}"#),
                ("GET", "/v3/mg.example.test/events") => format!(
                    r#"{{"items":[{{"id":"{echo}","event":"accepted","timestamp":1700000001.0,"recipient":"{echo}","tags":["{echo}"]}}],"paging":{{"next":"http://{host}/v3/mg.example.test/events/{cursor}?limit=2","previous":"http://{host}/v3/mg.example.test/events/{echo}"}}}}"#,
                    host = r.host
                ),
                _ => format!(
                    r#"{{"items":[],"paging":{{"next":"http://{host}/v3/mg.example.test/events/{echo}","previous":"http://{host}/v3/mg.example.test/events/{echo}"}}}}"#,
                    host = r.host
                ),
            };
            reply(case, &success, &format!(r#"{{"message":"{echo}"}}"#), &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let config = MailgunConfig {
            origin: concord::BaseUrl::from_static("https://api.mailgun.net"),
            authorization: HeaderValue::from_str(&format!("Basic {key}")).unwrap(),
        };
        c.assert_debug_absent("config (value not flagged)", &config);
        c.assert_debug_absent(
            "configured client",
            &Mailgun::new(Default::default(), config),
        );
        let api = Mailgun::local(&base, &key, Duration::from_secs(2)).unwrap();
        let domain = api.domain(Domain::new("mg.example.test").unwrap());
        let read = domain
            .events(range)
            .set_message_id(message_id.clone())
            .set_recipient(recipient.clone())
            .set_tags(tags.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("domain scope", &domain);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &domain.send_message(message.email()));
        c.assert_debug_absent(
            "write Prepared",
            &domain.send_message(message.email()).prepare(),
        );
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.clone().send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("event page", &response.value);
                c.assert_debug_absent("events", &response.value.items);
                c.assert_debug_absent("paging", &response.value.paging);
                metadata(&mut c, &response.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let result = domain.send_message(message.email()).send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("acceptance", &response.value);
                c.assert_debug_absent("accepted message id", &response.value.id);
                metadata(&mut c, &response.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let mut pages = read.pages(cap).unwrap();
        c.assert_debug_absent("traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("traversal page Result", &page);
        match page {
            Ok(page) => {
                assert_eq!(case, 0);
                let page = page.unwrap();
                c.assert_debug_absent("traversal page", &page);
                c.assert_debug_absent("cursor", &page.next);
                let last = pages.next_page().await.unwrap().unwrap();
                c.assert_debug_absent("last page", &last);
                c.assert_debug_absent("exhausted", &pages.next_page().await);
                c.assert_on_wire(&server.requests());
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "mailgun: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_key_never_prints() {
    let mut c = Canaries::new("mailgun");
    let key = format!(" {} ", c.issue());
    let e = Mailgun::local("http://127.0.0.1:9", &key, Duration::from_secs(2)).unwrap_err();
    error(&mut c, &e);
}
