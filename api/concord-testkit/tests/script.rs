//! The tests of `Script`, through the `Transport` trait, with the clock paused.

use bytes::Bytes;
use concord_core::error::Class;
use concord_core::reqwest;
use concord_core::transport::{Transport, TransportError, TryInfo};
use concord_testkit::script::UploadState;
use concord_testkit::Script;
use hyper::body::Frame;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{timeout, Instant};

fn get(url: &str) -> reqwest::Request {
    reqwest::Request::new(reqwest::Method::GET, url.parse().unwrap())
}

fn info(number: u32) -> TryInfo {
    TryInfo::new("tasks.list", number)
}

const SECOND: Duration = Duration::from_secs(1);

#[tokio::test(start_paused = true)]
async fn an_answer_comes_after_its_virtual_time() {
    let script = Script::new();
    script
        .answer(200)
        .after(30 * SECOND)
        .header("content-type", "application/json")
        .header("x-many", "1")
        .header("x-many", "2")
        .body("[]");

    let start = Instant::now();
    let answer = script
        .send(get("http://api.test/tasks"), info(1))
        .await
        .unwrap();
    assert_eq!(start.elapsed(), 30 * SECOND);
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.headers()["content-type"], "application/json");
    let many: Vec<_> = answer.headers().get_all("x-many").iter().collect();
    assert_eq!(many, ["1", "2"]);
    assert_eq!(answer.content_length(), Some(2));
    assert_eq!(answer.text().await.unwrap(), "[]");
    assert_eq!(start.elapsed(), 30 * SECOND);
}

#[tokio::test(start_paused = true)]
async fn tries_play_in_order_and_record_the_try_and_the_instant() {
    let script = Script::new();
    let given = script.clone();
    script.fail(TransportError::connect()).after(2 * SECOND);
    script.answer(503).after(SECOND);
    script.answer(200);
    assert_eq!(script.unused(), 3);

    let start = Instant::now();
    let error = given
        .send(get("http://api.test/a"), info(1))
        .await
        .unwrap_err();
    assert_eq!(error.class(), Class::Connect);
    let answer = given.send(get("http://api.test/a"), info(2)).await.unwrap();
    assert_eq!(answer.status(), 503);

    let mut post = reqwest::Request::new(
        reqwest::Method::POST,
        "http://api.test/a?x=1".parse().unwrap(),
    );
    post.headers_mut()
        .insert("authorization", "Bearer t".parse().unwrap());
    *post.body_mut() = Some("the body".into());
    let answer = given.send(post, info(3)).await.unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(script.unused(), 0);

    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let numbers: Vec<u32> = requests.iter().map(|r| r.info().number()).collect();
    assert_eq!(numbers, [1, 2, 3]);
    assert!(requests.iter().all(|r| r.info().endpoint() == "tasks.list"));
    let at: Vec<Duration> = requests.iter().map(|r| r.at() - start).collect();
    assert_eq!(at, [Duration::ZERO, 2 * SECOND, 3 * SECOND]);
    assert_eq!(requests[0].method(), reqwest::Method::GET);
    assert_eq!(requests[0].body(), None);
    assert_eq!(requests[2].method(), reqwest::Method::POST);
    assert_eq!(requests[2].url(), "http://api.test/a?x=1");
    assert_eq!(requests[2].header("authorization"), Some("Bearer t"));
    assert_eq!(requests[2].headers().len(), 1);
    assert_eq!(requests[2].body(), Some(&b"the body"[..]));
}

#[tokio::test(start_paused = true)]
async fn a_try_with_nothing_scripted_gets_599() {
    let script = Script::new();
    let answer = script.send(get("http://api.test/"), info(4)).await.unwrap();
    assert_eq!(answer.status(), 599);
    assert_eq!(
        answer.text().await.unwrap(),
        "concord-testkit: nothing scripted for try 4 of tasks.list\n"
    );
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_failure_of_each_class() {
    let script = Script::new();
    script.fail(TransportError::timeout());
    script.fail(TransportError::blocked()).after(SECOND);
    let start = Instant::now();
    let first = script
        .send(get("http://api.test/"), info(1))
        .await
        .unwrap_err();
    assert_eq!(first.class(), Class::Timeout);
    assert_eq!(start.elapsed(), Duration::ZERO);
    let second = script
        .send(get("http://api.test/"), info(2))
        .await
        .unwrap_err();
    assert_eq!(second.class(), Class::Blocked);
    assert_eq!(start.elapsed(), SECOND);
}

#[tokio::test(start_paused = true)]
async fn a_try_that_never_answers() {
    let script = Script::new();
    script.never();
    let start = Instant::now();
    let result =
        tokio::time::timeout(3600 * SECOND, script.send(get("http://api.test/"), info(1))).await;
    assert!(result.is_err(), "the try answered");
    assert_eq!(start.elapsed(), 3600 * SECOND);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn body_pieces_come_after_their_virtual_pauses() {
    let script = Script::new();
    script
        .answer(200)
        .after(SECOND)
        .body("a")
        .pause(5 * SECOND)
        .body("b")
        .pause(10 * SECOND)
        .body("c");

    let start = Instant::now();
    let mut answer = script.send(get("http://api.test/"), info(1)).await.unwrap();
    assert_eq!(start.elapsed(), SECOND);
    assert_eq!(answer.content_length(), Some(3));
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "a");
    assert_eq!(start.elapsed(), SECOND);
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "b");
    assert_eq!(start.elapsed(), 6 * SECOND);
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "c");
    assert_eq!(start.elapsed(), 16 * SECOND);
    assert_eq!(answer.chunk().await.unwrap(), None);
}

#[tokio::test(start_paused = true)]
async fn a_body_that_stalls_after_its_pieces() {
    let script = Script::new();
    script.answer(200).body("a").stall();
    let mut answer = script.send(get("http://api.test/"), info(1)).await.unwrap();
    assert_eq!(answer.content_length(), None);
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "a");
    let start = Instant::now();
    let next = tokio::time::timeout(60 * SECOND, answer.chunk()).await;
    assert!(next.is_err(), "the body went on: {next:?}");
    assert_eq!(start.elapsed(), 60 * SECOND);
}

#[tokio::test(start_paused = true)]
async fn a_body_that_is_cut_after_its_pieces() {
    let script = Script::new();
    script.answer(200).body("a").pause(SECOND).cut();
    let mut answer = script.send(get("http://api.test/"), info(1)).await.unwrap();
    let start = Instant::now();
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "a");
    assert!(answer.chunk().await.is_err());
    assert_eq!(start.elapsed(), SECOND);
}

#[test]
#[should_panic(expected = "is not a header name")]
fn a_bad_header_is_a_mistake_in_the_script() {
    let script = Script::new();
    let _ = script.answer(200).header("bad name", "x");
}

#[test]
#[should_panic(expected = "1000 is not a status code")]
fn a_bad_status_is_a_mistake_in_the_script() {
    let _ = Script::new().answer(1000);
}

#[tokio::test(start_paused = true)]
async fn an_answer_has_the_url_of_its_request() {
    let script = Script::new();
    script
        .answer(200)
        .header("link", "<?page=2>; rel=next")
        .body("[]");
    let answer = script
        .send(get("http://api.test/v1/items?page=1"), info(1))
        .await
        .unwrap();
    assert_eq!(answer.url().as_str(), "http://api.test/v1/items?page=1");
    let next = answer.url().join("?page=2").unwrap();
    assert_eq!(next.as_str(), "http://api.test/v1/items?page=2");
}

#[tokio::test(start_paused = true)]
async fn a_declared_length_that_the_body_does_not_meet_fails_at_the_early_end() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-length", "1000000")
        .body("tiny");
    script.answer(200).declare_length(10).body("tiny");
    script.answer(200).declare_length(2).body("abcd");

    let answer = script.send(get("http://api.test/"), info(1)).await.unwrap();
    assert_eq!(answer.headers()["content-length"], "1000000");
    assert_eq!(answer.content_length(), Some(1_000_000));
    assert!(
        answer.bytes().await.is_err(),
        "a short body is not a success"
    );

    let mut answer = script.send(get("http://api.test/"), info(2)).await.unwrap();
    assert_eq!(answer.headers()["content-length"], "10");
    assert_eq!(answer.content_length(), Some(10));
    assert_eq!(answer.chunk().await.unwrap().unwrap(), "tiny");
    assert!(answer.chunk().await.is_err(), "it fails at the early end");

    // As on the wire: the bytes after the declared length are not part of the body.
    let answer = script.send(get("http://api.test/"), info(3)).await.unwrap();
    assert_eq!(answer.content_length(), Some(2));
    assert_eq!(answer.text().await.unwrap(), "ab");
}

/// An upload: its pieces (an `Err` is a failure of the stream), how often it was pulled,
/// and whether it was dropped.
struct Upload {
    pieces: VecDeque<Result<&'static str, &'static str>>,
    pulls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Upload {
    fn post(
        pieces: &[Result<&'static str, &'static str>],
    ) -> (reqwest::Request, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let pulls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let upload = Upload {
            pieces: pieces.iter().copied().collect(),
            pulls: Arc::clone(&pulls),
            drops: Arc::clone(&drops),
        };
        let mut request =
            reqwest::Request::new(reqwest::Method::POST, "http://api.test/up".parse().unwrap());
        *request.body_mut() = Some(reqwest::Body::wrap(upload));
        (request, pulls, drops)
    }
}

impl hyper::body::Body for Upload {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, std::io::Error>>> {
        self.pulls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(match self.pieces.pop_front() {
            Some(Ok(piece)) => Some(Ok(Frame::data(Bytes::from_static(piece.as_bytes())))),
            Some(Err(error)) => Some(Err(std::io::Error::other(error))),
            None => None,
        })
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn the_upload_is_read_when_the_script_says_so_with_virtual_pauses() {
    let script = Script::new();
    script
        .answer(201)
        .read_upload_paced(2 * SECOND)
        .after(SECOND);
    script.answer(200).read_upload();
    script.answer(200);

    let (request, pulls, _) = Upload::post(&[Ok("a"), Ok("b"), Ok("c")]);
    let start = Instant::now();
    let answer = script.send(request, info(1)).await.unwrap();
    assert_eq!(answer.status(), 201);
    assert_eq!(start.elapsed(), 7 * SECOND, "three pauses, then the head");
    assert_eq!(pulls.load(Ordering::SeqCst), 4, "three pieces and the end");
    let record = &script.requests()[0];
    assert_eq!(record.upload().bytes(), b"abc");
    assert_eq!(record.upload().pieces(), 3);
    assert_eq!(record.upload().state(), &UploadState::Ended);

    // A body in memory is one piece.
    let mut post = get("http://api.test/up");
    *post.body_mut() = Some("the body".into());
    script.send(post, info(2)).await.unwrap();
    let record = &script.requests()[1];
    assert_eq!(record.upload().bytes(), b"the body");
    assert_eq!(record.upload().pieces(), 1);

    // Not told: not read.
    let (request, pulls, _) = Upload::post(&[Ok("a")]);
    script.send(request, info(3)).await.unwrap();
    assert_eq!(pulls.load(Ordering::SeqCst), 0);
    assert_eq!(script.requests()[2].upload().state(), &UploadState::Unread);
}

#[tokio::test(start_paused = true)]
async fn a_failing_upload_fails_the_try_and_is_recorded() {
    let script = Script::new();
    script.answer(200).read_upload().body("success");
    let (request, _, _) = Upload::post(&[Ok("a"), Err("the upload broke")]);
    let error = script.send(request, info(1)).await.unwrap_err();
    assert_eq!(error.class(), Class::Other);
    let record = &script.requests()[0];
    assert_eq!(record.upload().bytes(), b"a");
    match record.upload().state() {
        UploadState::Failed(text) => assert!(text.contains("the upload broke"), "{text}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn never_keeps_the_upload_unread_unless_told() {
    let script = Script::new();
    script.never();
    script.never().read_upload();

    let (request, pulls, drops) = Upload::post(&[Ok("a"), Ok("b")]);
    let mut try_one = Box::pin(script.send(request, info(1)));
    assert!(timeout(3600 * SECOND, &mut try_one).await.is_err());
    assert_eq!(pulls.load(Ordering::SeqCst), 0, "unread");
    assert_eq!(drops.load(Ordering::SeqCst), 0, "kept while the try waits");
    assert_eq!(script.requests()[0].upload().state(), &UploadState::Unread);
    drop(try_one);
    assert_eq!(drops.load(Ordering::SeqCst), 1);

    let (request, _, drops) = Upload::post(&[Ok("a"), Ok("b")]);
    let mut try_two = Box::pin(script.send(request, info(2)));
    assert!(timeout(3600 * SECOND, &mut try_two).await.is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let record = &script.requests()[1];
    assert_eq!(record.upload().bytes(), b"ab");
    assert_eq!(record.upload().state(), &UploadState::Ended);
}
