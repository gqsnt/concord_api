//! C12.11–C12.14: repeatable uploads through the public facade.
use concord::transport::{TransportError, TryInfo};
use concord::{
    bytes::Bytes,
    error::{Class, Kind},
    http, reqwest, Body, Middleware, Next, Retry, Transport,
};
use concord_testkit::Script;
use futures_core::Stream;
use std::{
    collections::VecDeque,
    error::Error,
    future::{poll_fn, Future},
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::{sleep, Instant};
type BoxError = Box<dyn Error + Send + Sync>;

concord::api! {
    client Uploads {
        base "https://uploads.example.test";
        PUT put(data: Body, length?: u64) { headers { "content-length" = length; } body data; }
        PUT typed(data: Body) { headers { "content-type" = "description/type"; } body data; }
        PUT counted(data: Body, builds: Arc<AtomicUsize>) { body declared_body(data, builds); }
        POST post(data: Body) { body data; }
        PUT forbidden(data: Body) { idempotent false; body data; }
        GET conditional(data: Body) { body data; }
        GET page(data: Body) -> String { read concord::Text; body data; paginate |call, _| Some(call); }
    }
}
fn declared_body(data: &Body, builds: &Arc<AtomicUsize>) -> Body {
    builds.fetch_add(1, Ordering::SeqCst);
    data.clone()
}
struct Chunks {
    parts: VecDeque<Result<Bytes, io::Error>>,
    stall: bool,
    drops: Arc<AtomicUsize>,
}
impl Stream for Chunks {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(piece) = self.parts.pop_front() {
            Poll::Ready(Some(piece))
        } else if self.stall {
            Poll::Pending
        } else {
            Poll::Ready(None)
        }
    }
}
impl Drop for Chunks {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn chunks(parts: &[&'static [u8]], drops: Arc<AtomicUsize>) -> Chunks {
    Chunks {
        parts: parts
            .iter()
            .map(|part| Ok(Bytes::from_static(part)))
            .collect(),
        stall: false,
        drops,
    }
}
fn body(opens: &Arc<AtomicUsize>) -> Body {
    let opens = opens.clone();
    Body::repeatable(move || {
        opens.fetch_add(1, Ordering::SeqCst);
        async { Ok::<_, io::Error>(chunks(&[b"abc", b"def"], Arc::default())) }
    })
}
fn counts(script: &Script) -> Vec<u32> {
    script
        .requests()
        .iter()
        .map(|r| r.info().number())
        .collect()
}
async fn read(request: &mut reqwest::Request) -> Result<Vec<u8>, BoxError> {
    let body = request.body_mut().take().unwrap();
    let stream = reqwest::Response::from(http::Response::new(body)).bytes_stream();
    let mut stream = std::pin::pin!(stream);
    let mut bytes = Vec::new();
    while let Some(piece) = poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
        bytes.extend_from_slice(&piece?);
    }
    Ok(bytes)
}
fn original_io(error: &(dyn Error + 'static), kind: io::ErrorKind) -> bool {
    let mut next = Some(error);
    while let Some(error) = next {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == kind)
        {
            return true;
        }
        next = error.source();
    }
    false
}
async fn pending_for<F: Future>(work: Pin<&mut F>, duration: Duration) {
    tokio::select! {biased; _=work=>panic!("operation unexpectedly finished"), _=sleep(duration)=>{}}
}
#[tokio::test(start_paused = true)]
async fn c12_11_wrapping_writer_and_description_keep_content_type_precedence() {
    let script = Script::new();
    script.answer(200).read_upload();
    script.answer(200).read_upload();
    let body = concord::gzip(Body::new("abc", "text/plain; charset=utf-8")).unwrap();
    assert_eq!(body.get_content_type(), Some("text/plain; charset=utf-8"));
    Uploads::new()
        .with_transport(script.clone())
        .typed(body.clone())
        .await
        .unwrap();
    assert_eq!(
        script.requests()[0].header("content-type"),
        Some("description/type")
    );
    Uploads::new()
        .with_transport(script.clone())
        .put(body)
        .edit_request(|r| {
            r.headers_mut().insert(
                "content-type",
                http::HeaderValue::from_static("override/type"),
            );
        })
        .await
        .unwrap();
    assert_eq!(
        script.requests()[1].header("content-type"),
        Some("override/type")
    );
    assert_eq!(counts(&script), [1, 1]);
}
#[tokio::test(start_paused = true)]
async fn c12_12_retries_reopen_from_zero_without_rebuilding_the_body_or_editing_twice() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::new(AtomicUsize::new(0));
    let edits = Arc::new(AtomicUsize::new(0));
    let seen = edits.clone();
    Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .length(6)
        .edit_request(move |r| {
            seen.fetch_add(1, Ordering::SeqCst);
            assert!(r.body().is_some());
            assert!(r.body().unwrap().as_bytes().is_none());
            r.headers_mut()
                .insert("x-upload", http::HeaderValue::from_static("kept"));
        })
        .with_retry(Retry::standard())
        .await
        .unwrap();
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(edits.load(Ordering::SeqCst), 1);
    for request in script.requests() {
        assert_eq!(request.upload().bytes(), b"abcdef");
        assert_eq!(request.header("x-upload"), Some("kept"));
    }
}
#[tokio::test(start_paused = true)]
async fn c12_12_body_expressions_run_once_and_raw_and_conditional_sends_retain_the_plan() {
    for raw in [true, false] {
        let script = Script::new();
        script.answer(503).read_upload();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let api = Uploads::new()
            .with_transport(script.clone())
            .with_retry(Retry::standard());
        if raw {
            api.put(body(&opens)).send_raw().await.unwrap();
        } else {
            api.conditional(body(&opens))
                .send_if_none_match("tag")
                .await
                .unwrap();
            assert_eq!(script.requests()[0].header("if-none-match"), Some("tag"));
        }
        assert_eq!(counts(&script), [1, 2]);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_12_permission_gate_still_refuses_unmarked_post_and_explicit_false_put() {
    for false_put in [true, false] {
        let script = Script::new();
        script.fail(TransportError::connect()).read_upload();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let api = Uploads::new()
            .with_transport(script.clone())
            .with_retry(Retry::standard());
        let error = if false_put {
            api.forbidden(body(&opens)).await.unwrap_err()
        } else {
            api.post(body(&opens)).await.unwrap_err()
        };
        assert_eq!(error.class(), Some(Class::Connect));
        assert_eq!(counts(&script), [1]);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
    }
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    Uploads::new()
        .with_transport(script.clone())
        .forbidden(body(&opens))
        .with_retry(Retry::standard().not_applied_on(503))
        .await
        .unwrap();
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}
#[tokio::test(start_paused = true)]
async fn c12_12_multipart_reopens_its_parts_with_one_boundary_and_identical_bytes() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let multipart = concord::multipart()
        .text("caption", "snapshot")
        .file(
            "file",
            "snapshot.bin",
            "application/octet-stream",
            body(&opens),
        )
        .finish()
        .unwrap();
    let label = multipart.get_content_type().unwrap().to_owned();
    Uploads::new()
        .with_transport(script.clone())
        .put(multipart)
        .with_retry(Retry::standard())
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests[0].upload().bytes(), requests[1].upload().bytes());
    for request in &requests {
        assert_eq!(request.header("content-type"), Some(label.as_str()));
        assert!(request.upload().bytes().windows(6).any(|s| s == b"abcdef"));
    }
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    let once = Body::stream(chunks(&[b"once"], Arc::default()));
    let multipart = concord::multipart()
        .file("f", "f", "text/plain", once)
        .finish()
        .unwrap();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(multipart)
        .with_retry(Retry::standard())
        .await
        .unwrap_err();
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.unused(), 1);
}
#[tokio::test(start_paused = true)]
async fn c12_12_two_cloned_calls_sent_concurrently_get_independent_cursors() {
    let script = Script::new();
    script.answer(200).read_upload_paced(Duration::from_secs(1));
    script.answer(200).read_upload();
    let opens = Arc::default();
    let call = Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens));
    let (a, b) = tokio::join!(call.clone().send(), call.send());
    a.unwrap();
    b.unwrap();
    assert_eq!(counts(&script), [1, 1]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    for request in script.requests() {
        assert_eq!(request.upload().bytes(), b"abcdef");
    }
}
struct Half {
    script: Script,
    first: Arc<Mutex<Vec<u8>>>,
}
impl Transport for Half {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        if info.number() == 1 {
            let body = request.body_mut().take().unwrap();
            let stream = reqwest::Response::from(http::Response::new(body)).bytes_stream();
            let mut stream = Box::pin(stream);
            let first = poll_fn(|cx| stream.as_mut().poll_next(cx))
                .await
                .unwrap()
                .unwrap();
            *self.first.lock().unwrap() = first.to_vec();
            *request.body_mut() = Some(reqwest::Body::wrap_stream(stream));
        }
        self.script.send(request, info).await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_12_half_sent_body_restarts_at_its_first_byte() {
    let script = Script::new();
    script.fail(TransportError::connect());
    script.answer(200).read_upload();
    let first = Arc::new(Mutex::new(Vec::new()));
    let opens = Arc::default();
    Uploads::new()
        .with_transport(Half {
            script: script.clone(),
            first: first.clone(),
        })
        .put(body(&opens))
        .with_retry(Retry::standard())
        .await
        .unwrap();
    assert_eq!(*first.lock().unwrap(), b"abc");
    assert_eq!(script.requests()[1].upload().bytes(), b"abcdef");
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}
#[tokio::test(start_paused = true)]
async fn c12_13_second_open_failure_is_other_with_original_cause_and_attempt_two() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::new(AtomicUsize::new(0));
    let seen = opens.clone();
    let body = Body::repeatable(move || {
        let n = seen.fetch_add(1, Ordering::SeqCst);
        async move {
            if n == 1 {
                Err(io::Error::new(io::ErrorKind::NotFound, "private-path"))
            } else {
                Ok(chunks(&[b"abc"], Arc::default()))
            }
        }
    });
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(body)
        .with_retry(Retry::standard())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(error.tries(), 2);
    assert!(original_io(&error, io::ErrorKind::NotFound));
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(script.unused(), 1);
    assert!(!format!("{error:?} {error}").contains("private-path"));
}
struct Lies {
    script: Script,
}
impl Transport for Lies {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let _ = read(&mut request).await;
        self.script.send(request, info).await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_body_error_wins_over_connection_failure_or_success_reported_by_transport() {
    for reports_success in [false, true] {
        let script = Script::new();
        if reports_success {
            script.answer(200);
        } else {
            script.fail(TransportError::connect());
        }
        script.answer(200);
        let body = Body::repeatable(|| async {
            let mut source = chunks(&[b"abc"], Arc::default());
            source.parts.push_back(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-source",
            )));
            Ok::<_, io::Error>(source)
        });
        let error = Uploads::new()
            .with_transport(Lies {
                script: script.clone(),
            })
            .put(body)
            .with_retry(Retry::standard())
            .await
            .unwrap_err();
        assert_eq!(error.class(), Some(Class::Other));
        assert!(original_io(&error, io::ErrorKind::PermissionDenied));
        assert_eq!(error.tries(), 1);
        assert_eq!(counts(&script), [1]);
        assert_eq!(script.unused(), 1);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_short_long_boundary_and_edited_zero_lengths_fail_as_body_errors() {
    for length in [0, 3, 7] {
        let script = Script::new();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let error = Uploads::new()
            .with_transport(script.clone())
            .put(body(&opens))
            .edit_request(move |r| {
                r.headers_mut().insert(
                    "content-length",
                    http::HeaderValue::from_str(&length.to_string()).unwrap(),
                );
            })
            .await
            .unwrap_err();
        assert_eq!(error.class(), Some(Class::Other));
        assert!(original_io(&error, io::ErrorKind::InvalidData));
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        if length == 0 {
            // A caller gets the body error on attempt one, before any transport
            // handover: a declared empty upload cannot silently discard bytes.
            assert_eq!(error.tries(), 1);
            assert!(script.requests().is_empty());
        } else {
            assert_eq!(counts(&script), [1]);
            if length == 3 {
                assert!(script.requests()[0].upload().bytes().is_empty());
            }
        }
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_edits_see_an_unreadable_placeholder_and_cannot_replace_or_remove_it() {
    for remove in [false, true] {
        let script = Script::new();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let later = Arc::new(AtomicUsize::new(0));
        let seen = later.clone();
        let call = Uploads::new()
            .with_transport(script.clone())
            .put(body(&opens))
            .edit_request(move |r| {
                assert!(concord::sign::body_bytes(r).is_err());
                *r.body_mut() = if remove {
                    None
                } else {
                    Some("replacement".into())
                };
            })
            .edit_request(move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
            });
        let error = call.await.unwrap_err();
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.tries(), 0);
        assert!(error.source().is_none());
        assert!(error
            .to_string()
            .contains("an edit replaced or removed a repeatable body"));
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        assert_eq!(later.load(Ordering::SeqCst), 0);
        assert!(script.requests().is_empty());
    }
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .edit_request(|r| {
            if r.body().is_none() {
                r.headers_mut()
                    .insert("content-length", http::HeaderValue::from_static("0"));
            }
        })
        .await
        .unwrap();
    assert_eq!(script.requests()[0].upload().bytes(), b"abcdef");
    assert_eq!(script.requests()[0].header("content-length"), None);
}
#[tokio::test(start_paused = true)]
async fn c12_14_request_and_consuming_exports_are_lazy_and_legacy_stream_inspection_still_refuses()
{
    let opens = Arc::default();
    let edits = Arc::new(AtomicUsize::new(0));
    let seen = edits.clone();
    let call = Uploads::new().put(body(&opens)).edit_request(move |r| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert!(r.body().is_some());
        r.headers_mut()
            .insert("x-edit", http::HeaderValue::from_static("kept"));
    });
    let mut a = call.request().unwrap();
    let mut b = call.clone().into_request().unwrap();
    assert!(a.try_clone().is_none());
    assert!(b.try_clone().is_none());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(edits.load(Ordering::SeqCst), 2);
    assert_eq!(read(&mut a).await.unwrap(), b"abcdef");
    assert_eq!(read(&mut b).await.unwrap(), b"abcdef");
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    drop(call);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    let once = Body::stream(chunks(&[b"x"], Arc::default()));
    let call = Uploads::new().put(once.clone());
    assert!(call
        .request()
        .unwrap_err()
        .to_string()
        .contains("request() cannot take a one-shot body stream"));
    assert!(once.into_stream().is_ok());
}
#[tokio::test(start_paused = true)]
async fn c12_14_same_repeatable_page_twice_is_a_mismatch_before_opening_the_second() {
    let script = Script::new();
    script.answer(200).read_upload().body("page");
    script.answer(200).read_upload().body("page");
    let opens = Arc::default();
    let mut pages = Uploads::new()
        .with_transport(script.clone())
        .page(body(&opens))
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some("page".into()));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(counts(&script), [1]);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(script.unused(), 1);
}
struct Consume;
impl Middleware for Consume {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let _ = read(&mut request).await;
        next.send(request, info).await
    }
}
fn stalled(opens: &Arc<AtomicUsize>) -> Body {
    let opens = opens.clone();
    Body::repeatable(move || {
        opens.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<Result<Chunks, io::Error>>()
    })
}
#[tokio::test(start_paused = true)]
async fn c12_14_pending_open_uses_transport_idle_and_total_but_middleware_has_no_default_total() {
    for total in [None, Some(2)] {
        let script = Script::new();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let call = Uploads::new()
            .with_transport(script.clone())
            .put(stalled(&opens));
        let start = Instant::now();
        let error = if let Some(seconds) = total {
            call.with_timeout(Duration::from_secs(seconds))
                .await
                .unwrap_err()
        } else {
            call.await.unwrap_err()
        };
        assert!(error.is_timeout());
        assert_eq!(
            Instant::now() - start,
            Duration::from_secs(total.unwrap_or(30))
        );
        assert_eq!(counts(&script), [1]);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
    }
    let script = Script::new();
    script.answer(200);
    let opens = Arc::default();
    let api = Uploads::new()
        .with_transport(script.clone())
        .with_middleware(Consume);
    let mut sending = Box::pin(api.put(stalled(&opens)).send());
    pending_for(sending.as_mut(), Duration::from_secs(35)).await;
    drop(sending);
    assert!(script.requests().is_empty());
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    let start = Instant::now();
    let error = api
        .put(stalled(&opens))
        .with_timeout(Duration::from_secs(2))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(Instant::now() - start, Duration::from_secs(2));
    assert!(script.requests().is_empty());
}
#[tokio::test(start_paused = true)]
async fn c12_14_chunks_reset_transport_idle_and_opening_is_covered_by_clock_ties() {
    let script = Script::new();
    script
        .answer(200)
        .read_upload_paced(Duration::from_secs(20));
    let opens = Arc::default();
    let start = Instant::now();
    Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .await
        .unwrap();
    assert!(Instant::now() - start >= Duration::from_secs(40));
    assert_eq!(counts(&script), [1]);
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(stalled(&opens))
        .with_timeout(Duration::from_secs(30))
        .await
        .unwrap_err();
    assert!(error.to_string().contains(" (with_timeout)"));
    assert!(error.is_timeout());
    assert_eq!(counts(&script), [1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_drop_before_poll_between_attempts_and_during_open_stream_stops_work() {
    let opens = Arc::new(AtomicUsize::new(0));
    drop(Uploads::new().put(body(&opens)));
    drop(Uploads::new().put(body(&opens)).send());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    let script = Script::new();
    script.answer(503).read_upload().header("retry-after", "10");
    script.answer(200).read_upload();
    let api = Uploads::new().with_transport(script.clone());
    let mut sending = Box::pin(api.put(body(&opens)).with_retry(Retry::standard()).send());
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    drop(sending);
    tokio::time::advance(Duration::from_secs(20)).await;
    assert_eq!(counts(&script), [1]);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    let script = Script::new();
    script.never().read_upload();
    let drops = Arc::new(AtomicUsize::new(0));
    let seen = drops.clone();
    let body = Body::repeatable(move || {
        let mut source = chunks(&[b"a"], seen.clone());
        source.stall = true;
        async move { Ok::<_, io::Error>(source) }
    });
    let mut sending = Box::pin(
        Uploads::new()
            .with_transport(script.clone())
            .put(body)
            .send(),
    );
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(sending);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&script), [1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_limit_does_not_open_waiters_and_releases_raw_refusal_before_retry() {
    use concord::limit::Limit;
    let script = Script::new();
    script
        .answer(503)
        .read_upload()
        .body("unread nonempty refusal")
        .header("retry-after", "2");
    script.answer(200).read_upload();
    let opens = Arc::new(AtomicUsize::new(0));
    let seen = opens.clone();
    let source = Body::repeatable(move || {
        let n = seen.fetch_add(1, Ordering::SeqCst);
        async move {
            if n == 1 {
                Err(io::Error::new(io::ErrorKind::NotFound, "open two"))
            } else {
                Ok(chunks(&[b"abc"], Arc::default()))
            }
        }
    });
    let api = Uploads::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().max_in_flight(1));
    let error = api
        .put(source)
        .with_retry(Retry::standard())
        .with_timeout(Duration::from_secs(10))
        .send_raw()
        .await
        .unwrap_err();
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    let script = Script::new();
    script.never();
    script.answer(200).read_upload();
    let api = Uploads::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().max_in_flight(1));
    let mut first = Box::pin(api.put(Body::from("first")).send());
    pending_for(first.as_mut(), Duration::from_secs(1)).await;
    let opens = Arc::default();
    let mut peer = Box::pin(api.put(body(&opens)).send());
    pending_for(peer.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&script), [1]);
    drop(first);
    peer.await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&script), [1, 1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_unsigned_signer_signs_the_actual_stream_on_each_attempt() {
    use concord::sign::{CanonicalPath, Credentials, SigV4};
    let base: concord::Base = "https://uploads.example.test".parse().unwrap();
    let signer = SigV4::new(
        &base,
        Credentials::new("access", "secret"),
        "eu-west-1",
        "s3",
        CanonicalPath::Preserve,
    )
    .allow_unsigned_payload();
    let script = Script::new();
    script.answer(503).read_upload().header("retry-after", "2");
    script.answer(200).read_upload();
    let opens = Arc::default();
    Uploads::new()
        .with_transport(script.clone())
        .with_middleware(signer)
        .put(body(&opens))
        .with_retry(Retry::standard())
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    for request in &requests {
        assert_eq!(
            request.header("x-amz-content-sha256"),
            Some("UNSIGNED-PAYLOAD")
        );
        assert!(request
            .header("authorization")
            .unwrap()
            .contains("AWS4-HMAC-SHA256"));
        assert_eq!(request.upload().bytes(), b"abcdef");
    }
    assert_ne!(
        requests[0].header("x-amz-date"),
        requests[1].header("x-amz-date")
    );
}

struct Retained(Arc<Mutex<Option<reqwest::Request>>>);
impl Transport for Retained {
    async fn send(
        &self,
        request: reqwest::Request,
        _: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        *self.0.lock().unwrap() = Some(request);
        std::future::pending().await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_dropped_call_prevents_a_retained_transport_from_opening_later() {
    let retained = Arc::new(Mutex::new(None));
    let opens = Arc::default();
    let mut sending = Box::pin(
        Uploads::new()
            .with_transport(Retained(retained.clone()))
            .put(body(&opens))
            .send(),
    );
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(sending);
    let mut request = retained.lock().unwrap().take().unwrap();
    assert!(read(&mut request).await.unwrap().is_empty());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}
#[tokio::test(start_paused = true)]
async fn c12_12_description_expression_is_retained_and_custom_rules_can_retry_other() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let builds = Arc::new(AtomicUsize::new(0));
    Uploads::new()
        .with_transport(script.clone())
        .counted(body(&opens), builds.clone())
        .with_retry(Retry::standard())
        .await
        .unwrap();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(counts(&script), [1, 2]);
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::new(AtomicUsize::new(0));
    let seen = opens.clone();
    let source = Body::repeatable(move || {
        let n = seen.fetch_add(1, Ordering::SeqCst);
        async move {
            if n == 1 {
                Err(io::Error::new(io::ErrorKind::NotFound, "open two"))
            } else {
                Ok(chunks(&[b"same"], Arc::default()))
            }
        }
    });
    Uploads::new()
        .with_transport(script.clone())
        .put(source)
        .with_retry(
            Retry::standard()
                .when(|tried| (tried.class() == Some(Class::Other)).then_some(Duration::ZERO)),
        )
        .await
        .unwrap();
    assert_eq!(counts(&script), [1, 2, 3]);
    assert_eq!(opens.load(Ordering::SeqCst), 3);
    assert_eq!(
        script.requests()[0].upload().bytes(),
        script.requests()[2].upload().bytes()
    );
}
#[tokio::test(start_paused = true)]
async fn c12_14_exports_have_no_clocks_and_delayed_open_uses_transport_time() {
    let opens = Arc::default();
    let mut request = Uploads::new()
        .put(stalled(&opens))
        .with_timeout(Duration::from_secs(1))
        .with_idle_timeout(Duration::from_secs(1))
        .request()
        .unwrap();
    let mut reading = Box::pin(read(&mut request));
    pending_for(reading.as_mut(), Duration::from_secs(35)).await;
    drop(reading);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    let opens = Arc::new(AtomicUsize::new(0));
    let seen = opens.clone();
    let source = Body::repeatable(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        async {
            sleep(Duration::from_secs(20)).await;
            Ok::<_, io::Error>(chunks(&[b"abc", b"def"], Arc::default()))
        }
    });
    let script = Script::new();
    script
        .answer(200)
        .read_upload_paced(Duration::from_secs(20));
    let start = Instant::now();
    Uploads::new()
        .with_transport(script.clone())
        .put(source)
        .await
        .unwrap();
    assert_eq!(Instant::now() - start, Duration::from_secs(60));
    assert_eq!(counts(&script), [1]);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}
#[tokio::test(start_paused = true)]
async fn c12_14_transfer_and_header_edits_preserve_the_same_plan() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let edits = Arc::new(AtomicUsize::new(0));
    let seen = edits.clone();
    let call = Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .edit_request(move |r| {
            seen.fetch_add(1, Ordering::SeqCst);
            r.headers_mut()
                .insert("x-returned", http::HeaderValue::from_static("kept"));
        })
        .with_retry(Retry::standard());
    let url = "https://uploads.example.test/returned?part=1";
    let call = concord::transfer::at(call, url).unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(edits.load(Ordering::SeqCst), 0);
    call.await.unwrap();
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(edits.load(Ordering::SeqCst), 1);
    for request in script.requests() {
        assert_eq!(request.url(), url);
        assert_eq!(request.header("x-returned"), Some("kept"));
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_malformed_and_conflicting_final_lengths_fail_before_opening() {
    for length in ["", "invalid", "18446744073709551616", "6,6"] {
        let opens = Arc::default();
        let script = Script::new();
        script.answer(200).read_upload();
        let error = Uploads::new()
            .with_transport(script.clone())
            .put(body(&opens))
            .edit_request(move |r| {
                r.headers_mut()
                    .insert("content-length", http::HeaderValue::from_static(length));
            })
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.tries(), 0);
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        assert!(script.requests().is_empty());
    }
    let opens = Arc::default();
    let error = Uploads::new()
        .put(body(&opens))
        .length(6)
        .edit_request(|r| {
            r.headers_mut()
                .append("content-length", http::HeaderValue::from_static("7"));
        })
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_14_edit_wrapping_the_placeholder_is_rejected_even_when_it_retains_it() {
    let opens = Arc::default();
    let script = Script::new();
    script.answer(200).read_upload();
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .edit_request(|r| {
            let body = r.body_mut().take().unwrap();
            let stream = reqwest::Response::from(http::Response::new(body)).bytes_stream();
            *r.body_mut() = Some(reqwest::Body::wrap_stream(stream));
        })
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert!(script.requests().is_empty());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}

struct NeverAfterRead(Script);
impl Transport for NeverAfterRead {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let _ = read(&mut request).await;
        self.0.send(request, info).await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_an_observed_body_error_ends_an_attempt_even_if_transport_never_returns() {
    let script = Script::new();
    script.never();
    let source = Body::repeatable(|| async {
        Err::<Chunks, _>(io::Error::new(io::ErrorKind::NotFound, "private-file"))
    });
    let start = Instant::now();
    let error = Uploads::new()
        .with_transport(NeverAfterRead(script.clone()))
        .put(source)
        .with_timeout(Duration::from_secs(100))
        .with_retry(Retry::standard())
        .await
        .unwrap_err();
    assert_eq!(error.class(), Some(Class::Other));
    assert!(original_io(&error, io::ErrorKind::NotFound));
    assert_eq!(Instant::now(), start);
    assert_eq!(counts(&script), [1]);
}

struct PendingOutput;
impl Stream for PendingOutput {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_drop_also_cancels_unopened_input_retained_by_a_mapping_callback() {
    let input = Arc::new(Mutex::new(None));
    let saved = input.clone();
    let opens = Arc::default();
    let mapped = body(&opens).map_stream(move |stream| {
        *saved.lock().unwrap() = Some(stream);
        Box::pin(PendingOutput)
    });
    let script = Script::new();
    script.never().read_upload();
    let client = Uploads::new().with_transport(script.clone());
    let mut sending = Box::pin(client.put(mapped).send());
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(sending);
    let mut retained = input.lock().unwrap().take().unwrap();
    assert!(poll_fn(|cx| retained.as_mut().poll_next(cx))
        .await
        .is_none());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&script), [1]);
}

concord::api! {
    client CleanupChecks {
        base "https://cleanup.example.test";
        GET empty() {}
        PUT upload(data: Body) { body data; }
    }
}
struct HoldAdmittedResponse;
impl Middleware for HoldAdmittedResponse {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let response = next.send(request, info).await?;
        std::future::pending::<()>().await;
        Ok(response)
    }
}
fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    let waker = std::task::Waker::noop();
    future.poll(&mut Context::from_waker(waker))
}
async fn ordinary_attempt_releases_limit_before_rules(kind: u8) {
    let script = Script::new();
    script
        .answer(200)
        .body("permit stays with this unread body");
    script.answer(200);
    let limit = concord::limit::Limit::new().max_in_flight(1);
    let peer = CleanupChecks::new()
        .with_transport(script.clone())
        .with_middleware(limit.clone())
        .empty()
        .send();
    let peer = Arc::new(Mutex::new(Box::pin(peer)));
    let seen = peer.clone();
    let rules = Arc::new(AtomicUsize::new(0));
    let calls = rules.clone();
    let retry = Retry::none().tries(2).when(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            matches!(
                poll_once(seen.lock().unwrap().as_mut()),
                Poll::Ready(Ok(()))
            ),
            "the failed attempt still holds its Limit permit during retry evaluation"
        );
        Some(Duration::from_secs(10))
    });
    let api = CleanupChecks::new()
        .with_transport(script.clone())
        .with_middleware(HoldAdmittedResponse)
        .with_middleware(limit)
        .with_retry(retry)
        .with_timeout(Duration::from_secs(2));
    let error = match kind {
        0 => api.empty().await.unwrap_err(),
        1 => api.upload(Body::from("bytes")).await.unwrap_err(),
        _ => api
            .upload(Body::stream(chunks(&[b"once"], Arc::default())))
            .await
            .unwrap_err(),
    };
    assert!(error.is_timeout());
    if kind == 2 {
        assert_eq!(rules.load(Ordering::SeqCst), 0);
        assert!(matches!(
            poll_once(peer.lock().unwrap().as_mut()),
            Poll::Ready(Ok(()))
        ));
    } else {
        assert_eq!(rules.load(Ordering::SeqCst), 1);
    }
    assert_eq!(counts(&script), [1, 1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_no_body_attempt_drops_its_limit_permit_before_retry_rules() {
    ordinary_attempt_releases_limit_before_rules(0).await;
}
#[tokio::test(start_paused = true)]
async fn c12_14_bytes_attempt_drops_its_limit_permit_before_retry_rules() {
    ordinary_attempt_releases_limit_before_rules(1).await;
}
#[tokio::test(start_paused = true)]
async fn c12_14_one_shot_attempt_drops_its_limit_permit_without_consulting_retry_rules() {
    ordinary_attempt_releases_limit_before_rules(2).await;
}

struct PollInputOnDrop(Option<concord::body::BodyStream>);
impl Stream for PollInputOnDrop {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}
impl Drop for PollInputOnDrop {
    fn drop(&mut self) {
        if let Some(input) = &mut self.0 {
            let waker = std::task::Waker::noop();
            let _ = input.as_mut().poll_next(&mut Context::from_waker(waker));
        }
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_cancellation_is_marked_before_a_destructor_polls_saved_input() {
    let opens = Arc::default();
    let source = body(&opens).map_stream(|input| Box::pin(PollInputOnDrop(Some(input))));
    let script = Script::new();
    script.never().read_upload();
    let mut sending = Box::pin(
        Uploads::new()
            .with_transport(script.clone())
            .put(source)
            .send(),
    );
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(sending);
    assert_eq!(
        opens.load(Ordering::SeqCst),
        0,
        "cleanup opened saved input before cancellation was marked"
    );
    assert_eq!(counts(&script), [1]);
}
#[tokio::test(start_paused = true)]
async fn c12_13_zero_length_is_checked_before_an_unreading_transport() {
    let script = Script::new();
    script.answer(200);
    let opens = Arc::default();
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .length(0)
        .await
        .unwrap_err();
    assert_eq!(error.class(), Some(Class::Other));
    assert!(original_io(&error, io::ErrorKind::InvalidData));
    assert_eq!(error.tries(), 1);
    assert!(script.requests().is_empty());
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}

struct RacingError {
    entered: std::sync::mpsc::Sender<()>,
    release: Arc<(Mutex<bool>, std::sync::Condvar)>,
}
impl Stream for RacingError {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.entered.send(()).unwrap();
        let (lock, changed) = &*self.release;
        let mut ready = lock.lock().unwrap();
        while !*ready {
            ready = changed.wait(ready).unwrap();
        }
        Poll::Ready(Some(Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "racing source",
        ))))
    }
}
struct EarlyResponse {
    script: Script,
    entered: Mutex<std::sync::mpsc::Receiver<()>>,
    upload: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl Transport for EarlyResponse {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let body = request.body_mut().take().unwrap();
        *self.upload.lock().unwrap() = Some(std::thread::spawn(move || {
            let stream = reqwest::Response::from(http::Response::new(body)).bytes_stream();
            let mut stream = Box::pin(stream);
            let waker = std::task::Waker::noop();
            assert!(matches!(
                stream.as_mut().poll_next(&mut Context::from_waker(waker)),
                Poll::Ready(Some(Err(_)))
            ));
        }));
        self.entered.lock().unwrap().recv().unwrap();
        self.script.send(request, info).await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_completion_waits_for_an_in_progress_upload_poll_and_keeps_its_error() {
    let script = Script::new();
    script.answer(200);
    let (entered, receiver) = std::sync::mpsc::channel();
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let saved = release.clone();
    let source = Body::repeatable(move || {
        let stream = RacingError {
            entered: entered.clone(),
            release: saved.clone(),
        };
        async { Ok::<_, io::Error>(stream) }
    });
    let transport = Arc::new(EarlyResponse {
        script: script.clone(),
        entered: Mutex::new(receiver),
        upload: Mutex::new(None),
    });
    let mut sending = Box::pin(
        Uploads::new()
            .with_transport(transport.clone())
            .put(source)
            .send(),
    );
    let first = poll_once(sending.as_mut());
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    transport
        .upload
        .lock()
        .unwrap()
        .take()
        .unwrap()
        .join()
        .unwrap();
    assert!(
        first.is_pending(),
        "success escaped while the upload poll was still in progress"
    );
    let error = sending.await.unwrap_err();
    assert_eq!(error.class(), Some(Class::Other));
    assert!(original_io(&error, io::ErrorKind::PermissionDenied));
    assert_eq!(counts(&script), [1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_a_forwarding_placeholder_wrapper_still_sends_the_declared_repeatable_body() {
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .edit_request(|r| {
            let placeholder = r.body_mut().take().unwrap();
            *r.body_mut() = Some(reqwest::Body::wrap(placeholder));
        })
        .await
        .unwrap();
    assert_eq!(counts(&script), [1]);
    assert_eq!(script.requests()[0].upload().bytes(), b"abcdef");
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    script.answer(200).read_upload();
    let error = Uploads::new()
        .with_transport(script.clone())
        .put(body(&opens))
        .edit_request(|request| {
            let placeholder = request.body_mut().take().unwrap();
            *request.body_mut() = Some(reqwest::Body::wrap(placeholder));
        })
        .edit_request(|request| *request.body_mut() = Some("replacement".into()))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(counts(&script), [1]);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(script.unused(), 1);
}
concord::api! {
    client DistinctPages {
        base "https://pages.example.test";
        GET next(opens: Arc<AtomicUsize>) -> String {
            body new_page_source(opens);
            read concord::Text;
            paginate |call, _| Some(call);
        }
    }
}
fn new_page_source(opens: &Arc<AtomicUsize>) -> Body {
    body(opens)
}
#[tokio::test(start_paused = true)]
async fn c12_14_distinct_factories_with_identical_page_heads_are_not_a_loop() {
    let script = Script::new();
    script.answer(200).body("one").read_upload();
    script.answer(200).body("two").read_upload();
    let opens = Arc::<AtomicUsize>::default();
    let mut pages = DistinctPages::new()
        .with_transport(script.clone())
        .next(opens.clone())
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some("one".to_owned()));
    assert_eq!(pages.try_next().await.unwrap(), Some("two".to_owned()));
    assert_eq!(counts(&script), [1, 1]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn c12_14_failed_upload_releases_its_limit_permit_before_the_retry_wait() {
    let script = Script::new();
    script.never().read_upload();
    script.answer(200);
    let limit = concord::limit::Limit::new().max_in_flight(1);
    let peer = CleanupChecks::new()
        .with_transport(script.clone())
        .with_middleware(limit.clone())
        .empty()
        .send();
    let peer = Mutex::new(Box::pin(peer));
    let rules = Arc::new(AtomicUsize::new(0));
    let calls = rules.clone();
    let retry = Retry::none().tries(2).when(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            matches!(
                poll_once(peer.lock().unwrap().as_mut()),
                Poll::Ready(Ok(()))
            ),
            "the failed upload still holds its permit before waiting"
        );
        Some(Duration::from_secs(10))
    });
    let source =
        Body::repeatable(|| async { Err::<Chunks, _>(io::Error::other("opening failed")) });
    let mut sending = Box::pin(
        Uploads::new()
            .with_transport(NeverAfterRead(script.clone()))
            .with_middleware(limit)
            .put(source)
            .with_retry(retry)
            .send(),
    );
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(rules.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&script), [1, 1]);
    drop(sending);
}

fn swap_body<C, A, F>(
    alternate: A,
    file: F,
) -> impl Fn(C, &concord::Answer<C::Value>) -> Option<C> + Send + 'static
where
    C: concord::page::Call,
    A: concord::page::Slot<C, Value = Body>,
    F: concord::page::Slot<C, Value = Body>,
{
    move |call, _| {
        let body = alternate.get(&call)?.clone();
        Some(file.set(call, body))
    }
}
concord::api! {
    client BodyCases {
        base "https://body-cases.example.test";
        PUT send(data: Body, length?: u64) { headers { "content-length" = length; } body data; }
        PUT typed(data: Body) { headers { "content-type" = "description/type"; } body data; }
        GET swapping(alternate: Body, file?: Body) -> String {
            body file.clone().unwrap_or_else(|| alternate.clone());
            read concord::Text;
            paginate swap_body(alternate, file);
        }
    }
}
#[tokio::test(start_paused = true)]
async fn c12_11_converted_text_bytes_and_concat_send_no_implicit_content_type() {
    let script = Script::new();
    for body in [
        Body::from("text"),
        Body::from(String::from("text")),
        Body::from(vec![1, 2]),
        Body::from(Bytes::from_static(b"bytes")),
        Body::concat([Body::new("a", "ignored/type")]),
    ] {
        script.answer(200).read_upload();
        BodyCases::new()
            .with_transport(script.clone())
            .send(body)
            .await
            .unwrap();
    }
    assert_eq!(counts(&script), [1, 1, 1, 1, 1]);
    for request in script.requests() {
        assert_eq!(request.header("content-type"), None);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_11_an_invalid_stored_label_fails_build_unless_the_description_overrides_it() {
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let source = body(&opens).content_type("invalid\nlabel");
    assert_eq!(source.get_content_type(), Some("invalid\nlabel"));
    let error = BodyCases::new()
        .with_transport(script.clone())
        .send(source.clone())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert!(script.requests().is_empty());
    assert!(!format!("{error} {error:?}").contains("invalid\nlabel"));
    BodyCases::new()
        .with_transport(script.clone())
        .typed(source)
        .await
        .unwrap();
    assert_eq!(counts(&script), [1]);
    assert_eq!(
        script.requests()[0].header("content-type"),
        Some("description/type")
    );
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}
#[tokio::test(start_paused = true)]
async fn c12_13_empty_lookahead_chunks_are_skipped_until_eof_or_excess_bytes() {
    for excess in [false, true] {
        let script = Script::new();
        script.answer(200).read_upload();
        let source = Body::repeatable(move || async move {
            let mut parts = chunks(&[b"abc", b"", b""], Arc::default());
            if excess {
                parts.parts.push_back(Ok(Bytes::from_static(b"x")));
            }
            Ok::<_, io::Error>(parts)
        });
        let result = BodyCases::new()
            .with_transport(script.clone())
            .send(source)
            .length(3)
            .await;
        if excess {
            let error = result.unwrap_err();
            assert_eq!(error.class(), Some(Class::Other));
            assert_eq!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidData
            );
            assert!(script.requests()[0].upload().bytes().is_empty());
        } else {
            result.unwrap();
            assert_eq!(script.requests()[0].upload().bytes(), b"abc");
        }
        assert_eq!(counts(&script), [1]);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_a_single_oversized_chunk_is_rejected_whole_before_any_of_it_is_sent() {
    let script = Script::new();
    script.answer(200).read_upload();
    let later = Arc::default();
    let source =
        Body::repeatable(|| async { Ok::<_, io::Error>(chunks(&[b"abcdef"], Arc::default())) });
    let error = BodyCases::new()
        .with_transport(script.clone())
        .send(Body::concat([source, body(&later)]))
        .length(3)
        .await
        .unwrap_err();
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(script.requests()[0].upload().bytes().is_empty());
    assert_eq!(later.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&script), [1]);
}
#[tokio::test(start_paused = true)]
async fn c12_14_reading_the_inspected_request_body_does_not_spend_the_call() {
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let call = BodyCases::new()
        .with_transport(script.clone())
        .send(body(&opens));
    let mut request = call.request().unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(read(&mut request).await.unwrap(), b"abcdef");
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    call.await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(counts(&script), [1]);
    assert_eq!(script.requests()[0].upload().bytes(), b"abcdef");
}
#[tokio::test(start_paused = true)]
async fn c12_12_two_occurrences_of_the_same_repeatable_source_open_independently_on_each_attempt() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let source = body(&opens);
    BodyCases::new()
        .with_transport(script.clone())
        .send(Body::concat([source.clone(), source]))
        .with_retry(Retry::standard())
        .await
        .unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 4);
    assert_eq!(counts(&script), [1, 2]);
    for request in script.requests() {
        assert_eq!(request.upload().bytes(), b"abcdefabcdef");
    }
}
fn mime_headers() -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "content-type",
        http::HeaderValue::from_static("application/octet-stream"),
    );
    headers
}
#[tokio::test(start_paused = true)]
async fn c12_12_mixed_and_related_multipart_repeat_with_one_boundary_and_identical_bytes() {
    for related in [false, true] {
        let script = Script::new();
        script.answer(503).read_upload();
        script.answer(200).read_upload();
        let opens = Arc::default();
        let parts = [(mime_headers(), body(&opens))];
        let source = if related {
            concord::multipart_related(parts)
        } else {
            concord::multipart_mixed(parts)
        }
        .unwrap();
        let label = source.get_content_type().unwrap().to_owned();
        assert!(source.is_repeatable());
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        BodyCases::new()
            .with_transport(script.clone())
            .send(source)
            .with_retry(Retry::standard())
            .await
            .unwrap();
        assert_eq!(counts(&script), [1, 2]);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
        let requests = script.requests();
        assert_eq!(requests[0].upload().bytes(), requests[1].upload().bytes());
        assert!(requests[0]
            .upload()
            .bytes()
            .windows(6)
            .any(|bytes| bytes == b"abcdef"));
        for request in &requests {
            assert_eq!(request.header("content-type"), Some(label.as_str()));
        }
        let once = Body::stream(chunks(&[b"once"], Arc::default()));
        let parts = [(mime_headers(), once.clone())];
        let error = if related {
            concord::multipart_related(parts)
        } else {
            concord::multipart_mixed(parts)
        }
        .unwrap_err();
        assert!(matches!(
            error,
            concord::MimeWriteError::Stream { part: 1, .. }
        ));
        assert!(once.into_stream().is_ok());
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_repeatable_mime_parts_check_the_fixed_boundary_on_each_opening() {
    for related in [false, true] {
        let script = Script::new();
        script.answer(200).read_upload();
        script.answer(200).read_upload();
        let opens = Arc::new(AtomicUsize::new(0));
        let opened = opens.clone();
        let source = Body::repeatable(move || {
            opened.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, io::Error>(chunks(
                    &[b"payload-concord-00000000000000000000000000000000"],
                    Arc::default(),
                ))
            }
        });
        let parts = [(mime_headers(), source)];
        let body = if related {
            concord::multipart_related(parts)
        } else {
            concord::multipart_mixed(parts)
        }
        .unwrap();
        let error = BodyCases::new()
            .with_transport(script.clone())
            .send(body)
            .with_retry(Retry::none().tries(2).when(|_| Some(Duration::ZERO)))
            .await
            .unwrap_err();
        assert_eq!(error.class(), Some(Class::Other));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(error.tries(), 2);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
        assert_eq!(counts(&script), [1, 2]);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_the_final_length_from_an_edit_is_used_on_every_attempt() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let edits = Arc::new(AtomicUsize::new(0));
    let edited = edits.clone();
    BodyCases::new()
        .with_transport(script.clone())
        .send(body(&opens))
        .length(10)
        .edit_request(move |request| {
            edited.fetch_add(1, Ordering::SeqCst);
            request
                .headers_mut()
                .insert("content-length", http::HeaderValue::from_static("6"));
        })
        .with_retry(Retry::standard())
        .await
        .unwrap();
    assert_eq!(counts(&script), [1, 2]);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(edits.load(Ordering::SeqCst), 1);
    for request in script.requests() {
        assert_eq!(request.header("content-length"), Some("6"));
        assert_eq!(request.upload().bytes(), b"abcdef");
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_a_pager_can_swap_sources_with_the_same_head_and_only_the_repeated_source_loops() {
    let script = Script::new();
    script.answer(200).body("one").read_upload();
    script.answer(200).body("two").read_upload();
    let first = Arc::default();
    let second = Arc::default();
    let mut pages = BodyCases::new()
        .with_transport(script.clone())
        .swapping(body(&second))
        .file(body(&first))
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some("one".into()));
    assert_eq!(pages.try_next().await.unwrap(), Some("two".into()));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(counts(&script), [1, 1]);
    assert_eq!(first.load(Ordering::SeqCst), 1);
    assert_eq!(second.load(Ordering::SeqCst), 1);
}
#[tokio::test(start_paused = true)]
async fn c12_12_a_public_mapping_writer_runs_once_per_opening_and_length_counts_its_framing() {
    // An author's writer uses only the public BodyStream interface.
    let maps = Arc::new(AtomicUsize::new(0));
    let mapped = maps.clone();
    let opens = Arc::default();
    let source = body(&opens).map_stream(move |input| {
        mapped.fetch_add(1, Ordering::SeqCst);
        struct Framed {
            prefix: bool,
            input: concord::body::BodyStream,
            end: bool,
        }
        impl Stream for Framed {
            type Item = Result<Bytes, BoxError>;
            fn poll_next(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                if !self.prefix {
                    self.prefix = true;
                    return Poll::Ready(Some(Ok(Bytes::from_static(b"<"))));
                }
                match self.input.as_mut().poll_next(cx) {
                    Poll::Ready(None) if !self.end => {
                        self.end = true;
                        Poll::Ready(Some(Ok(Bytes::from_static(b">"))))
                    }
                    result => result,
                }
            }
        }
        Box::pin(Framed {
            prefix: false,
            input,
            end: false,
        })
    });
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let call = BodyCases::new()
        .with_transport(script.clone())
        .send(source)
        .length(8)
        .with_retry(Retry::standard());
    drop(call.request().unwrap());
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    call.await.unwrap();
    assert_eq!(maps.load(Ordering::SeqCst), 2);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(counts(&script), [1, 2]);
    for request in script.requests() {
        assert_eq!(request.upload().bytes(), b"<abcdef>");
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_zero_length_opening_is_after_limit_admission_and_inside_transport_clocks() {
    for total in [None, Some(2)] {
        let script = Script::new();
        script.answer(200);
        let opens = Arc::default();
        let call = BodyCases::new()
            .with_transport(script.clone())
            .send(stalled(&opens))
            .length(0);
        let start = Instant::now();
        let error = if let Some(seconds) = total {
            call.with_timeout(Duration::from_secs(seconds))
                .await
                .unwrap_err()
        } else {
            call.await.unwrap_err()
        };
        assert!(error.is_timeout());
        assert_eq!(error.tries(), 1);
        assert_eq!(
            Instant::now() - start,
            Duration::from_secs(total.unwrap_or(30))
        );
        assert!(script.requests().is_empty());
        assert_eq!(opens.load(Ordering::SeqCst), 1);
    }
    let script = Script::new();
    script.never();
    script.answer(200);
    let limit = concord::limit::Limit::new().max_in_flight(1);
    let mut first = Box::pin(
        CleanupChecks::new()
            .with_transport(script.clone())
            .with_middleware(limit.clone())
            .empty()
            .send(),
    );
    pending_for(first.as_mut(), Duration::from_secs(1)).await;
    let opens = Arc::default();
    let mut zero = Box::pin(
        BodyCases::new()
            .with_transport(script.clone())
            .with_middleware(limit)
            .send(stalled(&opens))
            .length(0)
            .with_timeout(Duration::from_secs(5))
            .send(),
    );
    pending_for(zero.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(first);
    assert!(zero.await.unwrap_err().is_timeout());
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&script), [1]);
}

#[tokio::test(start_paused = true)]
async fn c12_13_an_empty_zero_length_source_is_checked_before_one_unreading_transport_handover() {
    let script = Script::new();
    script.answer(200);
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let source = Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async { Ok::<_, io::Error>(chunks(&[b"", b""], Arc::default())) }
    });
    let call = BodyCases::new()
        .with_transport(script.clone())
        .send(source)
        .length(0);
    drop(call.request().unwrap());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    call.await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(counts(&script), [1]);
    assert_eq!(script.requests()[0].header("content-length"), Some("0"));
}

#[tokio::test(start_paused = true)]
async fn c12_14_zero_length_opening_ready_at_the_total_deadline_never_reaches_transport() {
    let script = Script::new();
    script.answer(200);
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let source = Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async {
            sleep(Duration::from_secs(2)).await;
            Ok::<_, io::Error>(chunks(&[], Arc::default()))
        }
    });
    let start = Instant::now();
    let error = BodyCases::new()
        .with_transport(script.clone())
        .send(source)
        .length(0)
        .with_timeout(Duration::from_secs(2))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(error.tries(), 1);
    assert_eq!(Instant::now() - start, Duration::from_secs(2));
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert!(
        script.requests().is_empty(),
        "an expired call reached the transport after validation"
    );
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_14_zero_length_opening_ready_at_the_idle_deadline_never_reaches_transport() {
    let script = Script::new();
    script.answer(200);
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let source = Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async {
            sleep(Duration::from_secs(2)).await;
            Ok::<_, io::Error>(chunks(&[], Arc::default()))
        }
    });
    let start = Instant::now();
    let error = BodyCases::new()
        .with_transport(script.clone())
        .send(source)
        .length(0)
        .with_idle_timeout(Duration::from_secs(2))
        .with_timeout(Duration::from_secs(5))
        .await
        .unwrap_err();
    assert_eq!(error.tries(), 1);
    assert!(error.is_timeout());
    assert_eq!(
        error.to_string(),
        "transport error: no response for 2 s (with_idle_timeout changes it), in body_cases.send"
    );
    assert_eq!(Instant::now() - start, Duration::from_secs(2));
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert!(script.requests().is_empty());
    assert_eq!(script.unused(), 1);
}

#[derive(Debug)]
struct NamedUploadFailure;
impl std::fmt::Display for NamedUploadFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("the content-length does not match the body length")
    }
}
impl Error for NamedUploadFailure {}
#[tokio::test(start_paused = true)]
async fn c12_13_core_body_failures_are_documented_and_author_types_identify_provenance() {
    let rustdoc = include_str!("../../../concord-core/src/body/repeatable.rs");
    for row in [
        "| Length mismatch | `InvalidData` | `the content-length does not match the body length` |",
        "| Caught panic | `Other` | `the repeatable body panicked` |",
    ] {
        assert!(
            rustdoc.contains(row),
            "missing body-failure contract: {row}"
        );
    }
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200).read_upload();
    }
    let opens = Arc::default();
    let length = BodyCases::new()
        .with_transport(script.clone())
        .send(body(&opens))
        .length(3)
        .await
        .unwrap_err();
    let panic = Body::repeatable(|| -> std::future::Ready<Result<Chunks, io::Error>> {
        panic!("author panic")
    });
    let panic = BodyCases::new()
        .with_transport(script.clone())
        .send(panic)
        .await
        .unwrap_err();
    for (error, kind, text) in [
        (
            &length,
            io::ErrorKind::InvalidData,
            "the content-length does not match the body length",
        ),
        (&panic, io::ErrorKind::Other, "the repeatable body panicked"),
    ] {
        assert_eq!(error.class(), Some(Class::Other));
        assert_eq!(error.tries(), 1);
        let cause = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(cause.kind(), kind);
        assert_eq!(cause.to_string(), text);
    }
    let author_io = Body::repeatable(|| async {
        Err::<Chunks, _>(io::Error::new(
            io::ErrorKind::InvalidData,
            "the content-length does not match the body length",
        ))
    });
    let author_io = BodyCases::new()
        .with_transport(script.clone())
        .send(author_io)
        .await
        .unwrap_err();
    let cause = author_io
        .source()
        .unwrap()
        .downcast_ref::<io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), io::ErrorKind::InvalidData);
    assert_eq!(cause.to_string(), length.source().unwrap().to_string());
    let author_typed = Body::repeatable(|| async {
        Err::<std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, NamedUploadFailure>> + Send>>, _>(
            NamedUploadFailure,
        )
    });
    let author_typed = BodyCases::new()
        .with_transport(script.clone())
        .send(author_typed)
        .await
        .unwrap_err();
    assert!(author_typed
        .source()
        .unwrap()
        .downcast_ref::<NamedUploadFailure>()
        .is_some());
    assert!(author_typed
        .source()
        .unwrap()
        .downcast_ref::<io::Error>()
        .is_none());
    assert_eq!(counts(&script), [1, 1, 1, 1]);
}

type SavedBodyStream = Arc<Mutex<Option<concord::body::BodyStream>>>;
struct PollSavedInputOnCleanup(SavedBodyStream);
impl Drop for PollSavedInputOnCleanup {
    fn drop(&mut self) {
        if let Some(input) = self.0.lock().unwrap().as_mut() {
            let _ = input
                .as_mut()
                .poll_next(&mut Context::from_waker(std::task::Waker::noop()));
        }
    }
}
struct KeepRootDuringCleanup {
    script: Script,
    root: SavedBodyStream,
    input: SavedBodyStream,
}
impl Transport for KeepRootDuringCleanup {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let root = request.body_mut().take().unwrap();
        let mut output =
            Body::stream(reqwest::Response::from(http::Response::new(root)).bytes_stream())
                .into_stream()
                .unwrap();
        assert!(output
            .as_mut()
            .poll_next(&mut Context::from_waker(std::task::Waker::noop()))
            .is_pending());
        *self.root.lock().unwrap() = Some(output);
        let _cleanup = PollSavedInputOnCleanup(self.input.clone());
        self.script.send(request, info).await
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_outer_cancellation_precedes_cleanup_when_the_root_body_is_retained_elsewhere() {
    let script = Script::new();
    script.never();
    let root = Arc::new(Mutex::new(None));
    let input = Arc::new(Mutex::new(None));
    let saved = input.clone();
    let opens = Arc::default();
    let source = body(&opens).map_stream(move |stream| {
        *saved.lock().unwrap() = Some(stream);
        Box::pin(PendingOutput)
    });
    let transport = Arc::new(KeepRootDuringCleanup {
        script: script.clone(),
        root: root.clone(),
        input: input.clone(),
    });
    let mut sending = Box::pin(
        BodyCases::new()
            .with_transport(transport)
            .send(source)
            .send(),
    );
    pending_for(sending.as_mut(), Duration::from_secs(1)).await;
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(sending);
    assert!(
        root.lock().unwrap().is_some(),
        "the root must outlive attempt cleanup for this witness"
    );
    assert_eq!(
        opens.load(Ordering::SeqCst),
        0,
        "the transport destructor opened input before outer cancellation"
    );
    assert_eq!(counts(&script), [1]);
    root.lock().unwrap().take();
    input.lock().unwrap().take();
}
#[test]
fn c12_14_repeatable_docs_state_the_placeholder_and_composition_work_boundaries() {
    let docs = include_str!("../../../concord-core/src/body/repeatable.rs");
    for text in [
        "Reading the placeholder is a Build error.",
        "A replacement body's `size_hint` and `Drop` can run during building;",
        "Descent through maps and terminal destruction have no separate work limit:",
    ] {
        assert!(docs.contains(text), "missing runtime boundary: {text}");
    }
}
#[tokio::test(start_paused = true)]
async fn c12_14_reading_and_restoring_the_placeholder_still_fails_before_any_attempt() {
    let script = Script::new();
    script.answer(200).read_upload();
    let opens = Arc::default();
    let error = BodyCases::new()
        .with_transport(script.clone())
        .send(body(&opens))
        .edit_request(|request| {
            let original = request.body_mut().take().unwrap();
            let mut response = reqwest::Response::from(http::Response::new(original));
            let read = poll_once(Box::pin(response.chunk()).as_mut());
            assert!(matches!(read, Poll::Ready(Err(_))));
            let response: http::Response<reqwest::Body> = response.into();
            *request.body_mut() = Some(response.into_body());
        })
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert!(script.requests().is_empty());
}
#[tokio::test(start_paused = true)]
async fn c12_13_mime_framing_is_exact_and_second_open_failure_stops_before_trailers_and_later_parts(
) {
    for related in [false, true] {
        let script = Script::new();
        script.answer(503).read_upload();
        script.answer(200).read_upload();
        script.answer(200).read_upload();
        let opens = Arc::new(AtomicUsize::new(0));
        let opened = opens.clone();
        let later = Arc::default();
        let part = Body::repeatable(move || {
            let attempt = opened.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 1 {
                    Err(io::Error::new(io::ErrorKind::NotFound, "second opening"))
                } else {
                    Ok(chunks(&[b"abc"], Arc::default()))
                }
            }
        });
        let parts = [(mime_headers(), part), (mime_headers(), body(&later))];
        let mime = if related {
            concord::multipart_related(parts)
        } else {
            concord::multipart_mixed(parts)
        }
        .unwrap();
        let label = mime.get_content_type().unwrap().to_owned();
        let boundary = label.split("boundary=").nth(1).unwrap();
        let header = format!("--{boundary}\r\ncontent-type: application/octet-stream\r\n\r\n");
        let complete = format!("{header}abc\r\n{header}abcdef\r\n--{boundary}--\r\n");
        let error = BodyCases::new()
            .with_transport(script.clone())
            .send(mime)
            .with_retry(Retry::standard())
            .await
            .unwrap_err();
        assert_eq!(error.class(), Some(Class::Other));
        assert_eq!(error.tries(), 2);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(counts(&script), [1, 2]);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
        assert_eq!(later.load(Ordering::SeqCst), 1);
        assert_eq!(script.requests()[0].upload().bytes(), complete.as_bytes());
        assert_eq!(script.requests()[1].upload().bytes(), header.as_bytes());
        assert_eq!(script.unused(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_13_core_and_multipart_failure_lists_pin_the_boundary_scanner_source() {
    fn rustdoc(source: &str) -> String {
        source
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("///"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    let core = rustdoc(include_str!("../../../concord-core/src/body/repeatable.rs"));
    assert!(core.contains("The core's own body failures have an `io::Error` as their source:"));
    let form = rustdoc(include_str!("../../../concord-extras/src/multipart.rs"));
    let mime = rustdoc(include_str!("../../../concord-extras/src/body/mime.rs"));
    let messages = include_str!("../../../docs/reference/messages.md");
    let text = "multipart boundary occurred in a stream part";
    for docs in [form.as_str(), mime.as_str(), messages] {
        assert!(docs.contains(text), "missing scanner failure: {text}");
    }
    assert!(form.contains("`io::ErrorKind::Other`"));
    assert!(mime.contains("`io::ErrorKind::InvalidData`"));
    assert!(messages.contains("`Other` for form-data, `InvalidData` for mixed/related"));
    for mode in 0..3 {
        let boundary = Arc::new(Mutex::new(String::new()));
        let value = boundary.clone();
        let part = Body::repeatable(move || {
            let value = value.lock().unwrap().clone();
            async move {
                Ok::<_, io::Error>(Chunks {
                    parts: [Ok(Bytes::from(value))].into(),
                    stall: false,
                    drops: Arc::default(),
                })
            }
        });
        let data = if mode == 0 {
            concord::multipart()
                .file("file", "data", "text/plain", part)
                .finish()
                .unwrap()
        } else {
            let mut headers = http::HeaderMap::new();
            headers.insert("content-type", http::HeaderValue::from_static("text/plain"));
            if mode == 1 {
                concord::multipart_mixed([(headers, part)])
            } else {
                concord::multipart_related([(headers, part)])
            }
            .unwrap()
        };
        *boundary.lock().unwrap() = data
            .get_content_type()
            .unwrap()
            .split_once("boundary=")
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let script = Script::new();
        script.answer(200).read_upload();
        let error = BodyCases::new()
            .with_transport(script.clone())
            .send(data)
            .await
            .unwrap_err();
        assert_eq!(error.class(), Some(Class::Other));
        assert_eq!(error.tries(), 1);
        let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(
            source.kind(),
            if mode == 0 {
                io::ErrorKind::Other
            } else {
                io::ErrorKind::InvalidData
            }
        );
        assert_eq!(source.to_string(), text);
        assert_eq!(counts(&script), [1]);
        assert_eq!(script.unused(), 0);
    }
}

async fn same_page_after_regrouping(first: Body, next: Body, opens: &AtomicUsize, wire: &[u8]) {
    let script = Script::new();
    script.answer(200).read_upload().body("one");
    script.answer(200).read_upload().body("duplicate");
    let mut pages = BodyCases::new()
        .with_transport(script.clone())
        .swapping(next)
        .file(first)
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some("one".into()));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.tries(), 1);
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(counts(&script), [1]);
    assert_eq!(script.requests()[0].upload().bytes(), wire);
    assert_eq!(script.unused(), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_singleton_concat_of_the_same_source_is_the_same_page() {
    let opens = Arc::default();
    let source = body(&opens);
    same_page_after_regrouping(source.clone(), Body::concat([source]), &opens, b"abcdef").await;
}

#[tokio::test(start_paused = true)]
async fn c12_14_nested_concats_of_the_same_source_are_the_same_page() {
    let opens = Arc::default();
    let source = body(&opens);
    same_page_after_regrouping(
        Body::concat([Body::concat([source.clone()])]),
        Body::concat([Body::concat([Body::concat([source])])]),
        &opens,
        b"abcdef",
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_buffered_singleton_beside_a_source_is_the_same_page() {
    let opens = Arc::default();
    let source = body(&opens);
    same_page_after_regrouping(
        Body::concat([Body::from("prefix:"), source.clone()]),
        Body::concat([Body::concat([Body::from("prefix:")]), source]),
        &opens,
        b"prefix:abcdef",
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn c12_14_regrouped_adjacent_bytes_beside_a_source_are_the_same_page() {
    let opens = Arc::default();
    let source = body(&opens);
    same_page_after_regrouping(
        Body::concat([Body::from("pre"), Body::from("fix:"), source.clone()]),
        Body::concat([
            Body::concat([Body::from("pre"), Body::from("fix:")]),
            source,
        ]),
        &opens,
        b"prefix:abcdef",
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn c12_14_distinct_maps_of_the_same_source_are_distinct_pages() {
    let opens = Arc::default();
    let source = body(&opens);
    let maps = Arc::new(AtomicUsize::new(0));
    let mapped = || {
        let maps = maps.clone();
        source.clone().map_stream(move |input| {
            maps.fetch_add(1, Ordering::SeqCst);
            input
        })
    };
    let script = Script::new();
    script.answer(200).read_upload().body("one");
    script.answer(200).read_upload().body("two");
    script.answer(200).read_upload().body("duplicate");
    let mut pages = BodyCases::new()
        .with_transport(script.clone())
        .swapping(mapped())
        .file(mapped())
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some("one".into()));
    assert_eq!(pages.try_next().await.unwrap(), Some("two".into()));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.tries(), 1);
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(counts(&script), [1, 1]);
    assert_eq!(script.unused(), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(maps.load(Ordering::SeqCst), 2);
    for request in script.requests() {
        assert_eq!(request.upload().bytes(), b"abcdef");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_14_flattening_preserves_byte_values_and_source_order() {
    for order in [false, true] {
        let opens = Arc::default();
        let source = body(&opens);
        let (first, next, first_wire, next_wire) = if order {
            let other = body(&opens);
            (
                Body::concat([source.clone(), other.clone()]),
                Body::concat([other, source]),
                b"abcdefabcdef".as_slice(),
                b"abcdefabcdef".as_slice(),
            )
        } else {
            (
                Body::concat([Body::from("a"), source.clone()]),
                Body::concat([Body::from("b"), source]),
                b"aabcdef".as_slice(),
                b"babcdef".as_slice(),
            )
        };
        let script = Script::new();
        script.answer(200).read_upload().body("one");
        script.answer(200).read_upload().body("two");
        script.answer(200).read_upload().body("duplicate");
        let mut pages = BodyCases::new()
            .with_transport(script.clone())
            .swapping(next)
            .file(first)
            .pages();
        assert_eq!(pages.try_next().await.unwrap(), Some("one".into()));
        assert_eq!(pages.try_next().await.unwrap(), Some("two".into()));
        let error = pages.try_next().await.unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.tries(), 1);
        assert!(pages.try_next().await.unwrap().is_none());
        assert_eq!(counts(&script), [1, 1]);
        assert_eq!(script.requests()[0].upload().bytes(), first_wire);
        assert_eq!(script.requests()[1].upload().bytes(), next_wire);
        assert_eq!(script.unused(), 1);
        assert_eq!(opens.load(Ordering::SeqCst), if order { 4 } else { 2 });
    }
}
