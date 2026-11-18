//! C6.2: what the errors of `concord-extras`, and the causes of a failed repeatable body,
//! print. The page `docs/reference/messages.md` lists them in tables, from "The cause behind a
//! failed repeatable body" to section 3; each printed text there is compared, in the page's
//! order, with `to_string()` of an error that is constructed, or produced through the public
//! surface where its variant cannot be constructed outside its crate.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use concord::auth::{
    CredentialStore, Exchange, RenewalError, Rotating, RotationError, Stored, TokenPair,
};
use concord::bytes::Bytes;
use concord::http::{HeaderMap, HeaderValue};
use concord::limit::{Limit, LimitError};
use concord::read::{RecordError, RecordStream, Sse, SseEvent};
use concord::sign::SignError;
use concord::transfer::TransferError;
use concord::{Base, Body, BracketFormError, GzipError, JsonLinesError, MultipartError};
use concord_testkit::Script;
use futures_core::Stream;
use std::error::Error;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

const PAGE: &str = include_str!("../../../docs/reference/messages.md");

concord::api! {
    client Uploads {
        base "https://uploads.example.test";
        PUT put(data: Body, length?: u64) { headers { "content-length" = length; } body data; }
    }
}

concord::api! {
    client Events {
        base "https://events.example.test";
        GET watch() -> RecordStream<SseEvent> { read Sse::new(64); }
        GET small() -> RecordStream<SseEvent> { read Sse::new(4); }
    }
}

concord::api! {
    client Data {
        base "https://data.example.test";
        GET get() -> String { path "data"; read concord::Text; }
    }
}

/// The printed texts of the tables, in the page's order.
fn documented() -> Vec<String> {
    let start = PAGE
        .find("**The cause behind a failed repeatable body.**")
        .unwrap();
    let end = start + PAGE[start..].find("\n## 3.").unwrap();
    PAGE[start..end]
        .lines()
        .filter(|line| line.starts_with("| ") && !line.starts_with("| Failure"))
        .map(|line| {
            let row = line.trim_end().strip_suffix(" |").unwrap();
            let cell = &row[row.rfind(" | ").unwrap() + 3..];
            cell.strip_prefix('`')
                .and_then(|cell| cell.strip_suffix('`'))
                .unwrap_or_else(|| panic!("not one code span: {line}"))
                .to_owned()
        })
        .collect()
}

/// One chunk, then the end.
struct Once(Option<Bytes>);

impl Stream for Once {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.take().map(Ok))
    }
}

/// The text of the first `io::Error` in the chain of `source()`.
fn io_cause(error: &(dyn Error + 'static)) -> String {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(io) = error.downcast_ref::<io::Error>() {
            return io.to_string();
        }
        next = error.source();
    }
    panic!("no io::Error in the chain of {error}")
}

async fn repeatable_texts() -> Vec<String> {
    let mut texts = Vec::new();
    let script = Script::new();
    script.answer(200).read_upload();
    script.answer(200).read_upload();
    let uploads = Uploads::new().with_transport(script.clone());

    let panics = Body::repeatable(|| -> std::future::Ready<Result<Once, io::Error>> {
        panic!("the opening fails")
    });
    let error = uploads.put(panics).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "transport error: the body of the request failed, in uploads.put"
    );
    texts.push(io_cause(&error));

    let short = Body::repeatable(|| {
        std::future::ready(Ok::<_, io::Error>(Once(Some(Bytes::from_static(b"abc")))))
    });
    let error = uploads.put(short).length(5).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "transport error: the body of the request failed, in uploads.put"
    );
    texts.push(io_cause(&error));
    texts
}

fn sign_texts() -> Vec<String> {
    let invalid = HeaderValue::from_str("a\nb").unwrap_err();
    let not_text = HeaderValue::from_bytes(b"\xff")
        .unwrap()
        .to_str()
        .unwrap_err();
    [
        SignError::StreamingBody,
        SignError::InsecureUnsignedPayload,
        SignError::WrongOrigin,
        SignError::TooLarge,
        SignError::InvalidRequest,
        SignError::InvalidCredentials,
        SignError::from(invalid),
        SignError::from(not_text),
        SignError::InvalidTime,
        SignError::failed(io::Error::other("private cause")),
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

fn cause() -> Arc<dyn Error + Send + Sync> {
    Arc::new(io::Error::other("private cause"))
}

fn renewal_texts() -> Vec<String> {
    [
        RenewalError::Failed(cause()),
        RenewalError::TimedOut(cause()),
        RenewalError::Panicked,
        RenewalError::InvalidHeader(Arc::new(HeaderValue::from_str("a\nb").unwrap_err())),
        RenewalError::Expired,
        RenewalError::WrongOrigin,
        RenewalError::InvalidPolicy,
        RenewalError::TimeOverflow,
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// A store that holds an expired pair, saves the marker and fails every later save.
#[derive(Clone, Default)]
struct FailingStore(Arc<Mutex<u32>>);

impl CredentialStore for FailingStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        Ok(Stored::Ready(TokenPair::expires_in(
            "old",
            "r0",
            Duration::ZERO,
        )))
    }
    async fn save(&self, _record: Stored) -> Result<(), RenewalError> {
        let mut saves = self.0.lock().unwrap();
        *saves += 1;
        if *saves == 1 {
            Ok(())
        } else {
            Err(RenewalError::failed(io::Error::other("disk full")))
        }
    }
}

async fn unsaved_text() -> String {
    let base: Base = "https://data.example.test".parse().unwrap();
    let (auth, driver) = Rotating::new(&base, FailingStore::default(), |_: Exchange| async {
        Ok(TokenPair::expires_in("a1", "r1", Duration::from_secs(300)))
    });
    let script = Script::new();
    script.answer(200).body("ok");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(auth);
    let (_, returned) = tokio::join!(
        async {
            let result = api.get().await;
            drop(api);
            result
        },
        driver
    );
    let error = returned.unwrap_err();
    assert!(matches!(error, RotationError::Unsaved { .. }), "{error:?}");
    error.to_string()
}

async fn rotation_texts() -> Vec<String> {
    let mut texts: Vec<String> = [
        RotationError::Load(RenewalError::Panicked),
        RotationError::Save(RenewalError::Panicked),
        RotationError::Renewal(RenewalError::Panicked),
        RotationError::InvalidRefreshToken,
        RotationError::NeedsReauthorization,
        RotationError::NotPolled,
        RotationError::Stopped,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    texts.push(unsaved_text().await);
    texts
}

async fn limit_texts() -> Vec<String> {
    let mut texts: Vec<String> = [
        LimitError::InvalidPolicy,
        LimitError::TooManyKeys,
        LimitError::CostExceedsCapacity,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().max_hold(Duration::from_secs(1)));
    assert!(api.get().await.unwrap_err().is_status(429));
    let error = api.get().await.unwrap_err();
    let held = LimitError::find(&error).unwrap();
    assert!(matches!(held, LimitError::MaxHold { .. }), "{held:?}");
    texts.push(held.to_string());
    texts
}

/// A record that cannot be serialized when it is `Fails(true)`.
struct Fails(bool);

impl serde::Serialize for Fails {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0 {
            Err(serde::ser::Error::custom("private cause"))
        } else {
            serializer.serialize_u8(1)
        }
    }
}

fn typed(content_type: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static(content_type));
    headers
}

fn writer_texts() -> Vec<String> {
    let one_shot = Body::stream(Once(Some(Bytes::from_static(b"abc"))));
    let first = (typed("application/json"), Body::from("{}"));
    vec![
        BracketFormError::Shape.to_string(),
        GzipError::Stream.to_string(),
        JsonLinesError::TooLarge.to_string(),
        concord::json_lines([Fails(false), Fails(false), Fails(true)], 1024)
            .unwrap_err()
            .to_string(),
        concord::multipart_mixed(Vec::new())
            .unwrap_err()
            .to_string(),
        concord::multipart_related([first.clone(), (typed("text/plain"), one_shot)])
            .unwrap_err()
            .to_string(),
        concord::multipart_related([first, (HeaderMap::new(), Body::from("abc"))])
            .unwrap_err()
            .to_string(),
        MultipartError::Name.to_string(),
        MultipartError::ContentType(HeaderValue::from_str("a\nb").unwrap_err()).to_string(),
        MultipartError::Randomness.to_string(),
    ]
}

/// The first error of the stream of events of this response.
async fn first_record_error(script: &Script, small: bool) -> RecordError {
    let events = Events::new().with_transport(script.clone());
    let mut stream = if small {
        events.small().await.unwrap()
    } else {
        events.watch().await.unwrap()
    };
    loop {
        match stream.try_next().await {
            Ok(Some(_)) => continue,
            Ok(None) => panic!("the end, and no error"),
            Err(error) => return error,
        }
    }
}

async fn record_texts() -> Vec<String> {
    let script = Script::new();
    script.answer(200).body("data: one\n\ndata: tw").cut();
    script.answer(200).body("data: hello\n\n");
    script.answer(200).body(&b"data: \xff\n\n"[..]);
    let broken = first_record_error(&script, false).await;
    assert!(matches!(broken, RecordError::Stream(_)), "{broken:?}");
    let large = first_record_error(&script, true).await;
    assert!(matches!(large, RecordError::TooLarge { .. }), "{large:?}");
    let not_utf8 = first_record_error(&script, false).await;
    assert!(matches!(not_utf8, RecordError::Utf8 { .. }), "{not_utf8:?}");
    let decode = SseEvent::new("not json").json::<u8>().unwrap_err();
    [broken, large, not_utf8, decode]
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn transfer_texts() -> Vec<String> {
    [
        TransferError::InvalidUrl(cause()),
        TransferError::InvalidUri(cause()),
        TransferError::NotAbsolute,
        TransferError::UnsupportedScheme,
        TransferError::ForbiddenUrlPart,
        TransferError::ChangedUrl,
        TransferError::EmptyQuery,
        TransferError::UrlTooLong,
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

#[tokio::test(start_paused = true)]
async fn c6_2_every_text_of_the_extras_and_of_a_repeatable_body_is_on_the_messages_page() {
    let mut printed = repeatable_texts().await;
    printed.extend(sign_texts());
    printed.extend(renewal_texts());
    printed.extend(rotation_texts().await);
    printed.extend(limit_texts().await);
    printed.extend(writer_texts());
    printed.extend(record_texts().await);
    printed.extend(transfer_texts());
    let documented = documented();
    assert_eq!(documented.len(), 54);
    for (index, (page, code)) in documented.iter().zip(&printed).enumerate() {
        assert_eq!(page, code, "row {} of the tables", index + 1);
    }
    assert_eq!(documented, printed);
}

/// The acronyms that may start a text in capitals.
const ACRONYMS: [&str; 2] = ["JSON", "MIME"];

/// The capitals and the punctuation of every text on the page: no capital at the start
/// unless the first word is one of `ACRONYMS`, no final period, and no semicolon. It checks
/// no words: advice written in another form would pass.
#[tokio::test(start_paused = true)]
async fn c6_2_every_text_of_the_extras_starts_in_lower_case_and_holds_no_period_or_semicolon() {
    let mut printed = repeatable_texts().await;
    printed.extend(sign_texts());
    printed.extend(renewal_texts());
    printed.extend(rotation_texts().await);
    printed.extend(limit_texts().await);
    printed.extend(writer_texts());
    printed.extend(record_texts().await);
    printed.extend(transfer_texts());
    for text in &printed {
        let first = text.split(' ').next().unwrap();
        assert!(
            ACRONYMS.contains(&first) || !first.starts_with(|c: char| c.is_uppercase()),
            "a capital at the start: {text}"
        );
        assert!(!text.ends_with('.'), "a final period: {text}");
        assert!(!text.contains(';'), "a semicolon: {text}");
    }
}
