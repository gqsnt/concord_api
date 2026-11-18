#![cfg(feature = "json")]
//! An API that reports its errors inside a `200`, such as `{"ok": false, "error": "…"}`.
//! An arm that decodes the body itself fails with the decoder's error, and the call's error
//! names the location of the fault, as it does under a plain `read Json;`.

use concord::bytes::Bytes;
use concord::error::{Kept, Kind};
use concord::page::{Call, Slot};
use concord::{fake, reply, Answer, Binary, Fail, Json};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Deserialize, PartialEq)]
struct Posted {
    channel: String,
    kind: MessageKind,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum MessageKind {
    Message,
}

#[derive(Debug, Deserialize)]
struct Refusal {
    error: String,
}

/// Counts by a name that the server chooses.
#[derive(Debug, Deserialize)]
struct Tally {
    #[allow(dead_code)] // PANIC-SAFETY: Only decoded, to fail; no panic lint is relaxed.
    counts: HashMap<String, u32>,
}

/// A page whose cursor the API sends as JSON text: `"next": "{\"at\": 2}"`.
#[derive(Debug, Deserialize)]
struct Batch {
    items: Vec<u32>,
    next: Option<String>,
}

/// What a pager that can fail returns.
type Next<C> = Result<Option<C>, Fail<<C as Call>::Problem>>;

/// `paginate by_token(cursor);`: the page that the cursor of this page names.
fn by_token<C, S>(cursor: S) -> impl Fn(C, &Answer<C::Value>) -> Next<C> + Send + 'static
where
    C: Call<Value = Batch>,
    S: Slot<C, Value = String> + Send + 'static,
{
    #[derive(Deserialize)]
    struct Token {
        at: u32,
    }
    move |call, answer| match &answer.value().next {
        None => Ok(None),
        Some(next) => {
            let token: Token = Json::decode(next).map_err(Fail::from_read)?;
            Ok(Some(cursor.set(call, token.at.to_string())))
        }
    }
}

/// The arm of every 2xx: `ok` says whether the body is the value or the problem.
fn envelope<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<Refusal>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    if head.ok {
        Json::decode(&body).map_err(Fail::from_read)
    } else {
        let problem: Refusal = Json::decode(&body).map_err(Fail::from_read)?;
        Err(problem)?
    }
}

concord::api! {
    client Plain {
        base "https://api.test";
        read Json;
        problem Refusal;
        GET post() -> Posted { path "post"; }
        GET taken() -> Result<Posted, Refusal> { path "taken"; status { 409 { body } => Err(body) } }
    }
}

concord::api! {
    client Wrapped {
        base "https://api.test";
        read Binary;
        problem Refusal read Json;
        status { 2xx { body } => envelope(body)? }
        GET post() -> Posted { path "post"; }
        GET tally() -> Tally { path "tally"; }
        GET list(cursor?: String) -> Batch {
            path "list";
            query { cursor; }
            paginate by_token(cursor);
        }
        GET taken() -> Result<Posted, Refusal> {
            path "taken";
            status {
                override 2xx { body } => Ok(envelope(body)?),
                409 { body } => Err(Json::decode(&body).map_err(Fail::from_read)?),
            }
        }
    }
}

fn sources(error: &dyn std::error::Error) -> Vec<String> {
    let mut found = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        found.push(cause.to_string());
        source = cause.source();
    }
    found
}

#[tokio::test]
async fn c5_8_an_arm_that_decodes_fails_with_the_error_of_read_json() {
    const BODY: &str = r#"{"ok": true, "channel": 5, "kind": "message"}"#;
    let plain = Plain::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let plain = plain.post().await.unwrap_err();
    let wrapped = wrapped.post().await.unwrap_err();

    assert_eq!(
        plain.to_string(),
        "mismatch: status 200, channel could not be decoded, in plain.post"
    );
    assert_eq!(
        wrapped.to_string(),
        "mismatch: status 200, channel could not be decoded, in wrapped.post"
    );
    assert_eq!(wrapped.kind(), Kind::Mismatch);
    assert_eq!(plain.reason(), None);
    assert_eq!(wrapped.reason(), None);
    assert_eq!(sources(&plain), ["channel could not be decoded"]);
    assert_eq!(sources(&wrapped), sources(&plain));
    assert_eq!(wrapped.body(), Some(BODY.as_bytes()));
    assert_eq!(wrapped.kept(), Some(Kept::Complete));
    let details = wrapped.details().to_string();
    assert!(
        details
            .contains("reader: invalid type: integer `5`, expected a string at line 1 column 25"),
        "{details}"
    );
    // The same lines, but the bytes are named for what Concord knows of them: what the
    // decoder was given, which it cannot verify to be this response's body.
    assert_eq!(
        details.replace("wrapped.post", "plain.post"),
        plain
            .details()
            .to_string()
            .replace("\nbody (", "\ndecoder input (")
    );
}

#[tokio::test]
async fn c5_8_a_problem_inside_a_200_is_refused_with_the_problem() {
    let wrapped = Wrapped::new().with_transport(fake(|_| {
        Ok(reply(
            200,
            r#"{"ok": true, "channel": "C1", "kind": "message"}"#,
        ))
    }));
    assert_eq!(
        wrapped.post().await.unwrap(),
        Posted {
            channel: "C1".to_owned(),
            kind: MessageKind::Message
        }
    );
    let wrapped = Wrapped::new().with_transport(fake(|_| {
        Ok(reply(200, r#"{"ok": false, "error": "channel_not_found"}"#))
    }));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(200));
    assert_eq!(
        error.problem().map(|problem| problem.error.as_str()),
        Some("channel_not_found")
    );
    assert_eq!(error.to_string(), "refused: status 200, in wrapped.post");
    // The problem was read into a value: the error keeps no bytes and no decoder message.
    assert_eq!(error.body(), None);
    assert_eq!(error.kept(), None);
    assert!(error.source().is_none());
    assert!(!error.details().to_string().contains("reader:"));
}

#[tokio::test]
async fn c5_8_an_envelope_without_ok_is_a_mismatch_with_the_decoder_message() {
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, r#"{"channel": "C1"}"#))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body could not be decoded, in wrapped.post"
    );
    assert_eq!(error.reason(), None);
    assert_eq!(sources(&error), ["the body could not be decoded"]);
    assert_eq!(error.body(), Some(br#"{"channel": "C1"}"#.as_slice()));
    assert!(error
        .details()
        .to_string()
        .contains("reader: missing field `ok`"));
}

#[tokio::test]
async fn c5_3_an_arm_that_decodes_on_another_status_is_refused_with_the_bytes() {
    const BODY: &str = r#"{"error": 7}"#;
    let plain = Plain::new().with_transport(fake(|_| Ok(reply(409, BODY))));
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(409, BODY))));
    let plain = plain.taken().await.unwrap_err();
    let wrapped = wrapped.taken().await.unwrap_err();
    for (error, endpoint) in [(&plain, "plain.taken"), (&wrapped, "wrapped.taken")] {
        assert_eq!(error.kind(), Kind::Refused);
        assert!(error.is_status(409));
        assert!(error.problem().is_none());
        assert_eq!(error.body(), Some(BODY.as_bytes()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert!(error.source().is_none());
        assert_eq!(
            error.to_string(),
            format!("refused: status 409, in {endpoint}")
        );
    }
    assert_eq!(
        wrapped
            .details()
            .to_string()
            .replace("wrapped.taken", "plain.taken"),
        plain
            .details()
            .to_string()
            .replace("\nbody (", "\ndecoder input (")
    );
}

#[tokio::test]
async fn c5_8_the_message_and_the_causes_of_an_arm_that_decodes_hold_no_value() {
    const BODY: &str = r#"{"ok": true, "channel": "C1", "kind": "PRIVATE_SENTINEL"}"#;
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, kind could not be decoded, in wrapped.post"
    );
    let mut cause: Option<&dyn std::error::Error> = Some(&error);
    while let Some(source) = cause {
        assert!(!format!("{source} {source:?} {source:#?}").contains("PRIVATE_SENTINEL"));
        cause = source.source();
    }
    assert!(error.details().to_string().contains("PRIVATE_SENTINEL"));
}

#[tokio::test]
async fn c5_6_an_arm_that_decodes_keeps_at_most_64_kib_of_the_bytes() {
    let pad = "x".repeat(70 * 1024);
    let body = format!(r#"{{"ok": true, "channel": 5, "kind": "message", "pad": "{pad}"}}"#);
    let wrapped = Wrapped::new().with_transport(fake(move |_| Ok(reply(200, body.clone()))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, channel could not be decoded, in wrapped.post"
    );
    assert_eq!(error.kept(), Some(Kept::Cut));
    assert_eq!(error.body().map(<[u8]>::len), Some(64 * 1024));
}

#[tokio::test]
async fn c5_8_keys_chosen_by_the_server_reach_neither_the_message_nor_the_causes() {
    const BODY: &str = r#"{"ok": true, "counts": {"PRIVATE_KEY": "PRIVATE_VALUE"}}"#;
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let error = wrapped.tally().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, counts[key] could not be decoded, in wrapped.tally"
    );
    assert_eq!(error.reason(), None);
    assert_eq!(sources(&error), ["counts[key] could not be decoded"]);
    let mut shown = format!("{error} {error:?} {error:#?}");
    let mut cause = error.source();
    while let Some(source) = cause {
        shown.push_str(&format!(" {source} {source:?} {source:#?}"));
        cause = source.source();
    }
    for private in ["PRIVATE_KEY", "PRIVATE_VALUE"] {
        assert!(!shown.contains(private), "{shown}");
    }
    assert!(error.details().to_string().contains("PRIVATE_VALUE"));
}

#[tokio::test]
async fn c8_4_a_pager_that_fails_with_a_decoder_error_gives_the_page_then_the_error_then_the_end() {
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = sent.clone();
    let wrapped = Wrapped::new().with_transport(fake(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(reply(
            200,
            r#"{"ok": true, "items": [1, 2], "next": "{\"at\": \"two\"}"}"#,
        ))
    }));
    let mut pages = wrapped.list().pages();
    let page = pages.try_next().await.unwrap().unwrap();
    assert_eq!(page.items, [1, 2]);
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, at could not be decoded, in wrapped.list"
    );
    assert_eq!(error.reason(), None);
    assert_eq!(sources(&error), ["at could not be decoded"]);
    assert_eq!(error.body(), Some(br#"{"at": "two"}"#.as_slice()));
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(sent.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c5_8_a_body_that_is_not_json_fails_in_the_first_pass_where_its_syntax_breaks() {
    // The value does not fit at `channel`, and the JSON breaks later. `read Json;` meets the
    // value first; the arm's first pass, which reads only `ok`, skips the value and meets the
    // broken syntax. The guide says when the two places are the same.
    const BODY: &str = r#"{"ok": true, "channel": 5, "kind": "message", "x": [1, }"#;
    let plain = Plain::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let plain = plain.post().await.unwrap_err();
    let wrapped = wrapped.post().await.unwrap_err();
    assert_eq!(
        plain.to_string(),
        "mismatch: status 200, channel could not be decoded, in plain.post"
    );
    assert_eq!(
        wrapped.to_string(),
        "mismatch: status 200, [key] could not be decoded, in wrapped.post"
    );
    assert!(wrapped
        .details()
        .to_string()
        .contains("reader: expected value at line 1 column 56"));
}

/// A decoder's error saved from one call, for a later call to return.
static SAVED: std::sync::Mutex<Vec<concord::read::ReadError>> = std::sync::Mutex::new(Vec::new());

fn save<T: DeserializeOwned>(body: Bytes) -> Result<u32, Fail<Refusal>> {
    if let Err(error) = Json::decode::<T>(&body) {
        if let Ok(mut saved) = SAVED.lock() {
            saved.push(error);
        }
    }
    Ok(0)
}

fn replay<T, P>() -> Result<T, Fail<P>> {
    match SAVED.lock().ok().and_then(|mut saved| saved.pop()) {
        Some(error) => Err(Fail::from_read(error)),
        None => Err(concord::mismatch("nothing was saved")),
    }
}

/// `paginate replayed();`: a pager that returns the saved error.
fn replayed<C: Call>() -> impl Fn(C, &Answer<C::Value>) -> Next<C> + Send + 'static {
    |_, _| replay()
}

#[derive(Debug, Deserialize)]
struct Count {
    #[allow(dead_code)] // PANIC-SAFETY: Only decoded, to fail; no panic lint is relaxed.
    count: u32,
}

concord::api! {
    client Saved {
        base "https://api.test";
        read Binary;
        problem Refusal read Json;
        GET first() -> u32 { path "first"; status { 2xx { body } => save::<Count>(body)? } }
        GET later() -> u32 {
            path "later";
            status { 202 => replay()?, 409 => replay()?, 2xx => 7 }
        }
        GET pages() -> Vec<u32> {
            path "pages";
            read Json;
            paginate replayed();
        }
    }
}

#[tokio::test]
async fn c5_8_bytes_saved_from_another_call_are_shown_as_decoder_input() {
    const OLD: &str = r#"{"ok": true, "count": "OLD"}"#;
    let json = [("content-type", "application/json")];
    let saved = Saved::new().with_transport(fake(move |request| {
        Ok(match request.url().path() {
            "/first" => reply(200, OLD),
            "/pages" => concord::reply_with(200, &json, "[1]"),
            _ if request.headers().contains_key("x-conflict") => {
                concord::reply_with(409, &json, "{}")
            }
            _ => concord::reply_with(202, &json, "{}"),
        })
    }));
    let mut errors = Vec::new();
    for _ in 0..3 {
        saved.first().await.unwrap();
    }
    errors.push(saved.later().await.unwrap_err());
    errors.push(
        saved
            .later()
            .edit_request(|request| {
                request
                    .headers_mut()
                    .insert("x-conflict", concord::http::HeaderValue::from_static("1"));
            })
            .await
            .unwrap_err(),
    );
    let mut pages = saved.pages().pages();
    assert_eq!(*pages.try_next().await.unwrap().unwrap(), [1]);
    errors.push(pages.try_next().await.unwrap_err());
    let expected = [
        (
            Kind::Mismatch,
            202,
            "mismatch: status 202, count could not be decoded, in saved.later",
        ),
        (Kind::Refused, 409, "refused: status 409, in saved.later"),
        (
            Kind::Mismatch,
            200,
            "mismatch: status 200, count could not be decoded, in saved.pages",
        ),
    ];
    for (error, (kind, status, text)) in errors.iter().zip(expected) {
        assert_eq!(error.kind(), kind);
        assert!(error.is_status(status));
        assert_eq!(error.to_string(), text);
        let details = error.details().to_string();
        assert!(!details.contains("\nbody ("), "{details}");
        assert!(
            details.contains(&format!(
                "\ndecoder input (complete, {} bytes): {OLD}",
                OLD.len()
            )),
            "{details}"
        );
        assert_eq!(error.body(), Some(OLD.as_bytes()));
        assert_eq!(error.kept(), Some(Kept::Complete));
    }
}

#[tokio::test]
async fn c5_8_a_saved_content_type_remark_is_not_shown_beside_another_response() {
    use concord::read::{Incoming, Reader};
    // The loose reader saw `text/html`; the response that the arm runs on says JSON.
    let html = concord::reply_with(200, &[("content-type", "text/html")], r#"{"count": "x"}"#);
    let failure = <Json as Reader<Count>>::read(&Json, Incoming::new(html))
        .await
        .unwrap_err();
    let error = concord::__private::arm::<u32, Refusal>(
        Err(Fail::from_read(failure)),
        concord::http::StatusCode::ACCEPTED,
        &concord::Headers::from(
            concord::reply_with(202, &[("content-type", "application/json")], "")
                .headers()
                .clone(),
        ),
        &concord::transport::TryInfo::new("saved.later", 1),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 202, count could not be decoded, in saved.later"
    );
    assert!(!error
        .details()
        .to_string()
        .contains("unexpected content type"));
}
