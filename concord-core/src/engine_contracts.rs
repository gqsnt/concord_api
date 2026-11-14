use crate::__private::{arm, read_body, Core, Draft, Idempotent, Options, Piece, Run};
use crate::error::{Class, Kept, Kind};
use crate::read::{ByteStream, Incoming, ReadError, Reader, StreamError};
use crate::transport::{TransportError, TryInfo};
use crate::{bytes::Bytes, Answer, Error, Headers, Retry, Transport, KIB};
use futures_util::StreamExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct Binary(usize);
impl Reader<Bytes> for Binary {
    async fn read(&self, body: Incoming) -> Result<Bytes, ReadError> {
        body.bytes(self.0).await
    }
}

struct Reject;
impl Reader<()> for Reject {
    async fn read(&self, body: Incoming) -> Result<(), ReadError> {
        Err(ReadError::not_the_type(body.bytes(100 * KIB).await?).label_was_not_expected())
    }
}

fn draft() -> Draft {
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[Piece::Lit("http://fixture.invalid/")]);
    draft
}

fn info() -> TryInfo {
    TryInfo::new("client.get", 1)
}

fn broken(status: u16) -> reqwest::Response {
    crate::reply_with(
        status,
        &[("x-id", "one")],
        reqwest::Body::wrap_stream(futures_util::stream::iter([
            Ok(Bytes::from_static(b"prefix")),
            Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
        ])),
    )
}

#[tokio::test(start_paused = true)]
async fn c5_3_a_failed_non_2xx_body_binding_is_refused_with_bounded_evidence() {
    for (status, body) in [
        (302, "".to_owned()),
        (429, "bad".to_owned()),
        (503, "x".repeat(100 * KIB)),
    ] {
        let error: Error<String> = read_body(
            &Reject,
            crate::reply_with(status, &[("x-id", "one")], body.clone()),
            &info(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), Kind::Refused);
        assert!(error.is_status(status));
        assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        assert_eq!(
            error.body(),
            Some(body.as_bytes().get(..body.len().min(64 * KIB)).unwrap())
        );
        assert_eq!(
            error.kept(),
            Some(if body.len() > 64 * KIB {
                Kept::Cut
            } else {
                Kept::Complete
            })
        );
        assert!(error.problem().is_none());
        assert!(std::error::Error::source(&error).is_none());
    }
    let error: Error<String> = read_body(&Binary(100), broken(429), &info())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.body(), Some(b"prefix".as_slice()));
    assert_eq!(error.kept(), Some(Kept::Interrupted));
    assert!(error.problem().is_none());
}

#[tokio::test(start_paused = true)]
async fn c5_3_a_reader_that_declines_before_reading_still_keeps_refusal_evidence() {
    struct Strict;
    impl Reader<Bytes> for Strict {
        async fn read(&self, _: Incoming) -> Result<Bytes, ReadError> {
            Err(ReadError::wrong_content_type())
        }
    }
    for limit in [None, Some(0)] {
        let response = crate::reply_with(429, &[("x-id", "one")], "actual body");
        let error: Error<String> = match limit {
            None => read_body(&Strict, response, &info()).await,
            Some(limit) => read_body(&Binary(limit), response, &info()).await,
        }
        .unwrap_err();
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.body(), Some(b"actual body".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert!(error.problem().is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c5_3_successful_bindings_and_explicit_arm_failures_keep_their_meaning() {
    let value: Result<_, Error<String>> =
        read_body(&Binary(100), crate::reply(429, "valid"), &info()).await;
    assert_eq!(value.unwrap(), "valid");
    let error: Error<String> = read_body(&Reject, crate::reply(200, "bad"), &info())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    let error: Error<String> = read_body(&Binary(100), broken(200), &info())
        .await
        .unwrap_err();
    assert_eq!(error.class(), Some(Class::Body));
    for failure in [
        crate::mismatch("author"),
        Headers::default().require("missing").unwrap_err(),
    ] {
        let error = arm::<(), String>(
            Err(failure),
            http::StatusCode::TOO_MANY_REQUESTS,
            &Headers::default(),
            &info(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
    }
}

#[tokio::test(start_paused = true)]
async fn c4_2_standard_retry_retries_429_whatever_its_body() {
    for mode in 0..5 {
        let tries = Arc::new(AtomicUsize::new(0));
        let count = tries.clone();
        let core = Core::<()>::default()
            .with_retry(Retry::standard())
            .with_transport(crate::fake(move |_| {
                if count.fetch_add(1, Ordering::SeqCst) != 0 {
                    return Ok(crate::reply(200, "ok"));
                }
                Ok(match mode {
                    0 => crate::reply(429, ""),
                    1 => crate::reply(429, "malformed"),
                    2 => crate::reply(429, "x".repeat(100 * KIB)),
                    3 => broken(429),
                    _ => crate::reply(
                        429,
                        reqwest::Body::wrap_stream(futures_util::stream::pending::<
                            Result<Bytes, std::io::Error>,
                        >()),
                    ),
                })
            }));
        let options = Options::default();
        let run = Run::start(&core, &options, "client.get");
        let answer = draft()
            .prepare(
                run,
                core,
                options,
                Idempotent::ByMethod,
                |response, info| async move {
                    let status = response.status();
                    let headers = Headers::from(response.headers().clone());
                    if !status.is_success() {
                        read_body(&Reject, response, &info).await?;
                    }
                    Ok(Answer::new(status, headers, ()))
                },
            )
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), http::StatusCode::OK);
        assert_eq!(tries.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn c5_7_a_bound_refusal_survives_total_clock_cancellation() {
    let core = Core::<()>::default()
        .with_timeout(Duration::from_secs(1))
        .with_transport(crate::fake(|_| {
            Ok(crate::reply_with(
                429,
                &[("x-id", "one")],
                reqwest::Body::wrap_stream(
                    futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(
                        b"prefix",
                    ))])
                    .chain(futures_util::stream::pending()),
                ),
            ))
        }));
    let options = Options::default();
    let run = Run::start(&core, &options, "client.get");
    let error = draft()
        .prepare(
            run,
            core,
            options,
            Idempotent::ByMethod,
            |response, info| async move {
                let _: Bytes = read_body(&Binary(100), response, &info).await?;
                Ok(Answer::new(http::StatusCode::OK, Headers::default(), ()))
            },
        )
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(429));
    assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
    assert_eq!(error.body(), Some(b"prefix".as_slice()));
    assert_eq!(error.kept(), Some(Kept::Interrupted));
}

#[tokio::test(start_paused = true)]
async fn c5_10_stream_error_converts_with_question_mark_for_every_problem_type() {
    async fn consume<P>(mut stream: ByteStream) -> Result<(), Error<P>> {
        while stream.try_next().await?.is_some() {}
        Ok(())
    }
    let stream = Incoming::new(broken(201)).with_info(&info()).stream();
    // A non-Send, non-Debug problem also requires no conversion bounds.
    let error = consume::<std::rc::Rc<()>>(stream).await.unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(201));
    assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
    assert_eq!(error.endpoint(), "client.get");
    assert_eq!(error.tries(), 1);
    assert!(error.problem().is_none());

    let mut stream = Incoming::new(broken(201)).with_info(&info()).stream();
    assert!(stream.try_next().await.unwrap().is_some());
    let error: StreamError = stream.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert_eq!(error.status(), Some(http::StatusCode::CREATED));
    assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
    assert_eq!(error.endpoint(), "client.get");
    assert_eq!(error.tries(), 1);
    assert!(!error.is_timeout());
    assert!(stream.try_next().await.unwrap().is_none());
    let error: ReadError = error.into();
    let error: Error<String> = error.into_error(http::StatusCode::OK, Headers::default(), &info());
    assert!(error.is_status(201));
    assert_eq!(error.class(), Some(Class::Body));
}

struct Silent;
impl Transport for Silent {
    async fn send(
        &self,
        _: reqwest::Request,
        _: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_idle_text_names_the_phase_and_the_winning_setting() {
    for (client, description, call, seconds, hint) in [
        (None, None, None, 30, "with_idle_timeout changes it"),
        (Some(4), None, None, 4, "with_idle_timeout changes it"),
        (
            Some(4),
            Some(2),
            None,
            2,
            "with_idle_timeout on the call changes it",
        ),
        (Some(4), Some(2), Some(1), 1, "with_idle_timeout changes it"),
        (
            None,
            Some(30),
            None,
            30,
            "with_idle_timeout on the call changes it",
        ),
    ] {
        for after_head in [false, true] {
            let mut core = Core::<()>::default();
            if let Some(seconds) = client {
                core = core.with_idle_timeout(Duration::from_secs(seconds));
            }
            core = if after_head {
                core.with_transport(crate::fake(|_| {
                    Ok(crate::reply_with(
                        200,
                        &[("x-id", "one")],
                        reqwest::Body::wrap_stream(futures_util::stream::pending::<
                            Result<Bytes, std::io::Error>,
                        >()),
                    ))
                }))
            } else {
                core.with_transport(Silent)
            };
            let mut options = Options::default();
            if let Some(seconds) = call {
                options.with_idle_timeout(Duration::from_secs(seconds));
            }
            let run = Run::start(&core, &options, "client.get");
            // As generated code does: the description runs after the Run starts.
            if let Some(seconds) = description {
                options.description_idle_timeout(Duration::from_secs(seconds));
            }
            let error = draft()
                .prepare(
                    run,
                    core,
                    options,
                    Idempotent::ByMethod,
                    |response, info| async move {
                        let status = response.status();
                        let headers = response.headers().clone().into();
                        let bytes = read_body(&Binary(100), response, &info).await?;
                        Ok(Answer::new(status, headers, bytes))
                    },
                )
                .unwrap()
                .send()
                .await
                .unwrap_err();
            let note = if after_head {
                format!("the body was silent for {seconds} s after status 200")
            } else {
                format!("no answer for {seconds} s")
            };
            assert_eq!(
                error.to_string(),
                format!("transport error: {note} ({hint}), in client.get")
            );
            assert_eq!(error.class(), Some(Class::Timeout));
            assert_eq!(error.status(), after_head.then_some(http::StatusCode::OK));
            assert_eq!(
                error.headers().and_then(|h| h.get("x-id")),
                after_head.then_some("one")
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c5_4_an_announced_excess_keeps_no_body_or_state_and_never_polls() {
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    let response = crate::reply_with(
        200,
        &[("content-length", "100")],
        reqwest::Body::wrap_stream(futures_util::stream::poll_fn(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::<Option<Result<Bytes, std::io::Error>>>::Pending
        })),
    );
    let error: Error = read_body(&Binary(10), response, &info()).await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.kept(), None);
    assert_eq!(error.body(), None);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn c6_3_details_print_the_complete_cut_and_interrupted_body_in_words() {
    for (body, kept, text) in [
        (vec![b'x'; 18], Kept::Complete, "body (complete, 18 bytes)"),
        (vec![b'x'; 64 * KIB], Kept::Cut, "body (cut at 64 KiB)"),
        (
            vec![b'x'; 120],
            Kept::Interrupted,
            "body (interrupted, 120 bytes)",
        ),
    ] {
        let error: Error = Error::refused(
            "client.get",
            1,
            http::StatusCode::BAD_REQUEST,
            Headers::default(),
            None,
        )
        .with_body(&body, kept);
        assert!(error.details().to_string().contains(text));
        assert!(!error.details().to_string().contains("Some("));
    }
}

#[tokio::test(start_paused = true)]
async fn c5_2_empty_decode_names_emptiness_and_only_a_present_wrong_label() {
    for (body, label, note) in [
        ("", None, "the body is empty"),
        (
            "",
            Some("text/plain"),
            "the body is empty and its content type is not the reader's",
        ),
        ("bad", None, "the body is not what the type says"),
        (
            "bad",
            Some("text/plain"),
            "the body is not what the type says and its content type is not the reader's",
        ),
    ] {
        let headers: Vec<_> = label
            .map(|label| ("content-type", label))
            .into_iter()
            .collect();
        let error: Error = read_body(&Reject, crate::reply_with(204, &headers, body), &info())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("mismatch: status 204, {note}, in client.get")
        );
    }
}

#[test]
fn c2_10_an_invalid_replacement_base_names_with_base_only_on_that_failure() {
    let core = Core::<()>::default().with_base("not a base");
    let error = draft()
        .finish(&core, &Options::default(), "client.get")
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build error: the base is not a URL that can be used (with_base), in client.get"
    );
    let mut bad = Draft::new(http::Method::GET);
    bad.base(&[Piece::Lit("not a base")]);
    let error = bad
        .finish(&Core::<()>::default(), &Options::default(), "client.get")
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build error: the base is not a URL that can be used, in client.get"
    );
}
