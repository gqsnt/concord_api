//! C5, the answer: the arm before the body, the simple rule, an arm that binds `body`, a
//! reader and its limit, what an error keeps, a known failure that stays, `send_full`, a
//! stream, the readers, a conditional send (docs/reference/contracts.md C5; docs/reference/rules.md rules 11 to
//! 15; docs/internals/engine.md, "One try").
//!
//! Table 5 of docs/reference/cases.md is in `c5_table5.rs`.

mod common;

use bytes::Bytes;
use common::engine::*;
use concord::error::{Class, Kept, Kind};
use concord::read::{Incoming, Place, ReadError, Reader};
use concord::MIB;
use concord_testkit::server::Server;
use concord_testkit::Script;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::Instant;

fn match_get(script: &Script) -> tour::riot::matches::Get {
    riot(script)
        .matches(tour::Europe)
        .get(tour::MatchId::from("EUW1_1"))
}

fn by_riot_id(script: &Script) -> tour::riot::account::ByRiotId {
    riot(script)
        .account(tour::Europe)
        .by_riot_id("Faker", "KR1")
}

/// A problem, as the API of `Shapes` sends it.
const SHAPES_PROBLEM: &str = r#"{"code":"slow","message":"slow down"}"#;

/// A reader of the caller's own, with a limit of 4 bytes.
#[derive(Clone, Copy)]
struct FourBytes;

impl Reader<Bytes> for FourBytes {
    async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
        incoming.bytes(4).await
    }
}

/// A reader of the caller's own that refuses every body at this place.
#[derive(Clone)]
struct RefusesAt(Place);

impl Reader<()> for RefusesAt {
    async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
        let body = incoming.bytes(1024).await?;
        Err(ReadError::not_the_type(body).at(self.0.clone()))
    }
}

/// A body that notes when its allocation is freed.
struct Watched(&'static [u8], Arc<AtomicBool>);

impl AsRef<[u8]> for Watched {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.1.store(true, Ordering::SeqCst);
    }
}

// C5.1 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_1_the_status_picks_the_arm_before_any_byte_of_the_body_is_read() {
    // A body that never ends: reading it would wait for the clock for silence.
    let script = Script::new();
    script.answer(404).body("x").stall();
    let start = Instant::now();

    let found = success(files(&script).exists(tour::DocId::from("a")).await);

    assert!(!found);
    assert_eq!(start.elapsed(), secs(0));
}

#[tokio::test(start_paused = true)]
async fn c5_1_an_exact_status_wins_over_a_class() {
    // `override 404 => None` under the client's `4xx { body } => …`: the 404 is the exact
    // arm's, and its body (not a problem) is never read.
    let script = Script::new();
    script.answer(404).body("not a problem");
    assert!(success(by_riot_id(&script).await).is_none());

    // Another 4xx is the class arm's.
    let script = Script::new();
    json(&script, 400, PROBLEM);
    let error = failure(by_riot_id(&script).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(400));
    assert!(error.problem().is_some());
}

// C5.2 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_2_a_2xx_that_no_arm_takes_is_the_body_and_any_other_status_is_refused() {
    for status in [200, 201, 203, 299] {
        let script = Script::new();
        json(&script, status, TASK);
        assert_eq!(success(tasks(&script).get(7).await).id, 7, "{status}");
    }
    for status in [302, 400, 404, 500, 599] {
        let script = Script::new();
        json(&script, status, TASK);
        let error = failure(tasks(&script).get(7).await);
        assert_eq!(error.kind(), Kind::Refused, "{status}");
        assert!(error.is_status(status), "{status}");
    }
}

#[tokio::test(start_paused = true)]
async fn c5_2_an_option_after_the_arrow_reads_t_and_gives_some() {
    let script = Script::new();
    json(&script, 200, ACCOUNT);
    let account = success(by_riot_id(&script).await).expect("Some");
    assert_eq!(account.tag_line, "KR1");

    // A `200` whose body is `null` is not a `None`.
    let script = Script::new();
    json(&script, 200, "null");
    assert_eq!(failure(by_riot_id(&script).await).kind(), Kind::Mismatch);
}

#[tokio::test(start_paused = true)]
async fn c5_2_a_body_of_no_bytes_is_the_readers_affair() {
    let script = Script::new();
    script.answer(204);
    assert_eq!(
        failure(tasks(&script).get(7).await).kind(),
        Kind::Mismatch,
        "Json"
    );

    for status in [200, 204] {
        let script = Script::new();
        script.answer(status);
        assert_eq!(success(files(&script).readme("repo").await), "", "Text");
    }
}

#[tokio::test(start_paused = true)]
async fn c5_2_with_no_arrow_a_2xx_gives_unit_and_its_body_is_ignored() {
    let script = Script::new();
    script.answer(200).body("not what the API promised");
    success(tasks(&script).delete(7).await);
}

// C5.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_3_an_arm_that_binds_body_reads_it_with_the_reader_in_reach() {
    // `shapes.refusal` reads `4xx { body } => Err::<_, ShapesProblem>(body)?` with `Json`.
    let script = Script::new();
    json(&script, 403, SHAPES_PROBLEM);
    let error = failure(shapes(&script).refusal(1).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.problem().unwrap().code, "slow");

    // "When an arm on a status that is not a 2xx binds `body` and that body does not read, the call is Refused".
    let script = Script::new();
    json(&script, 403, r#"{"other":1}"#);
    let error = failure(shapes(&script).refusal(1).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(403));
    assert!(error.problem().is_none());
    assert_eq!(
        error.body(),
        Some(&br#"{"other":1}"#[..]),
        "the bytes are kept"
    );
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("content-type")),
        Some("application/json")
    );
}

// C5.4 ------------------------------------------------------------------------------

const LARGER: &str =
    "mismatch: status 200, the body is larger than the limit of the reader, in tasks.get";

#[tokio::test(start_paused = true)]
async fn c5_4_an_announced_size_over_the_limit_is_a_mismatch_before_anything_is_read() {
    let script = Script::new();
    // The head says more than 10 MiB, and no byte ever comes.
    script
        .answer(200)
        .header("content-type", "application/json")
        .declare_length(10 * MIB as u64 + 1)
        .stall();
    let start = Instant::now();

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.to_string(), LARGER);
    assert_eq!(start.elapsed(), secs(0), "nothing was read");
}

#[tokio::test(start_paused = true)]
async fn c5_4_bytes_are_counted_as_they_come_and_the_limit_is_10_mib() {
    let piece = Bytes::from(vec![b'a'; MIB]);
    let answer = |script: &Script, extra: &'static [u8]| {
        let mut answer = script.answer(200);
        for _ in 0..10 {
            answer = answer.body(piece.clone());
        }
        let _ = answer.body(extra);
    };

    let script = Script::new();
    answer(&script, b"");
    assert_eq!(success(files(&script).readme("repo").await).len(), 10 * MIB);

    let script = Script::new();
    answer(&script, b"a");
    let error = failure(files(&script).readme("repo").await);
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body is larger than the limit of the reader, in files.readme"
    );
}

#[tokio::test(start_paused = true)]
async fn c5_4_a_body_that_breaks_is_a_transport_error_that_keeps_the_status_and_the_headers() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .header("x-a", "1")
        .body(r#"{"id":7,"#)
        .cut();

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
    assert_eq!(error.headers().unwrap().get("x-a"), Some("1"));
    assert_eq!(
        error.to_string(),
        "transport error: the body broke after status 200, in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c5_4_a_body_that_ends_before_its_announced_size_is_never_a_shorter_value() {
    // The first bytes are a whole text already: a shorter value is possible, and refused.
    let script = Script::new();
    script.answer(200).declare_length(100).body("short");

    let error = failure(files(&script).readme("repo").await);

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
}

#[tokio::test]
async fn c5_4_a_body_that_ends_early_on_the_wire() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .declare_length(100)
        .body(r#"{"id":7,"#);
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let error = failure(
        tour::Tasks::new()
            .with_transport(http)
            .with_base(format!("{}/api/tasks", server.url()))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
    assert_eq!(
        error.to_string(),
        "transport error: the body broke after status 200, in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c5_4_the_limit_of_10_mib_is_10_times_1024_times_1024_bytes() {
    // C5.11: `Text` and `Json` read "10 MiB"; lib: "1024 × 1024 bytes". The oracle is
    // written here, not taken from the crate.
    const LIMIT: usize = 10 * 1024 * 1024;
    assert_eq!(concord::MIB, 1_048_576);
    let body = |script: &Script, size: usize| {
        let mut answer = script.answer(200);
        let mut left = size;
        while left > 0 {
            let piece = left.min(1 << 20);
            answer = answer.body(vec![b'a'; piece]);
            left -= piece;
        }
    };

    let script = Script::new();
    body(&script, LIMIT);
    assert_eq!(success(files(&script).readme("repo").await).len(), LIMIT);

    let script = Script::new();
    body(&script, LIMIT + 1);
    assert_eq!(
        failure(files(&script).readme("repo").await).kind(),
        Kind::Mismatch
    );
}

#[tokio::test(start_paused = true)]
async fn c5_4_a_chunk_that_would_pass_the_limit_is_not_kept() {
    // C5.4: "A chunk that would pass the limit is not kept." A reader with a limit of 4 bytes
    // gets `ab`, then `cdef`.
    let script = Script::new();
    script.answer(200).body("ab").body("cdef").stall();
    let core = concord::__private::Core::default().with_transport(script);

    let error = failure(prepared_get(core, FourBytes).send().await);
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.body(), Some(&b"ab"[..]));
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body is larger than the limit of the reader, in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c5_4_a_content_length_is_the_announced_size_also_for_a_fake() {
    // "\"Announced size\" is the `Content-Length` header of the answer when it
    // has one [...] This holds for a `fake`". A body that ends before it: class Body.
    let fake = concord::fake(|_| {
        Ok(concord::reply_with(
            200,
            &[("content-length", "100"), ("x-a", "1")],
            "short",
        ))
    });
    let error = failure(
        tour::Files::new("key")
            .with_transport(fake)
            .readme("repo")
            .await,
    );
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
    assert_eq!(error.headers().and_then(|h| h.get("x-a")), Some("1"));

    // One over the limit of the reader: a Mismatch before a byte is read.
    let fake = concord::fake(|_| {
        Ok(concord::reply_with(
            200,
            &[("content-length", "10485761")],
            "short",
        ))
    });
    let error = failure(
        tour::Files::new("key")
            .with_transport(fake)
            .readme("repo")
            .await,
    );
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body is larger than the limit of the reader, in files.readme"
    );
    assert_eq!(error.kept(), None, "no byte was kept");
}

/// A body of one frame that holds no byte, then its end.
struct OneEmptyFrame(bool);

impl futures_core::Stream for OneEmptyFrame {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.0 {
            std::task::Poll::Ready(None)
        } else {
            self.0 = true;
            std::task::Poll::Ready(Some(Ok(Bytes::new())))
        }
    }
}

/// A transport whose every answer says `content-length: 1`, then gives one empty frame and
/// ends: on the wire, a connection closed cleanly one byte early.
fn one_empty_frame() -> concord::__private::Core<()> {
    concord::__private::Core::default().with_transport(concord::fake(|_| {
        let answer = http::Response::builder()
            .status(200)
            .header("content-length", "1")
            .body(reqwest::Body::wrap_stream(OneEmptyFrame(false)))
            .expect("an answer");
        Ok(reqwest::Response::from(answer))
    }))
}

#[tokio::test(start_paused = true)]
async fn c5_4_an_empty_frame_counts_for_no_byte_of_the_announced_size() {
    // C5.4: "Bytes are counted as they come"; "A body that ends before its announced
    // size is a Transport error of class Body, never a shorter value." A frame of no byte
    // brings none: after it, one announced byte is still missing.
    const SHORT: &str = "transport error: the body broke after status 200, in tasks.get";
    let errors = [
        (
            "Json",
            failure(
                prepared_get::<serde_json::Value, _>(one_empty_frame(), concord::Json)
                    .send()
                    .await,
            ),
        ),
        (
            "Text",
            failure(
                prepared_get::<String, _>(one_empty_frame(), concord::Text)
                    .send()
                    .await,
            ),
        ),
        (
            "Binary",
            failure(
                prepared_get::<Bytes, _>(one_empty_frame(), concord::Binary)
                    .send()
                    .await,
            ),
        ),
    ];
    for (reader, error) in errors {
        assert_eq!(error.kind(), Kind::Transport, "{reader}");
        assert_eq!(error.class(), Some(Class::Body), "{reader}");
        assert!(error.is_status(200), "{reader}");
        assert_eq!(error.to_string(), SHORT, "{reader}");
    }

    // `Stream`: the call ends at the head; the stream gives the empty frame or not,
    // then the short-body error, never its end.
    let mut stream = success(
        prepared_get::<concord::ByteStream, _>(one_empty_frame(), concord::read::Stream)
            .send()
            .await,
    )
    .into_value();
    let error = loop {
        match stream.try_next().await {
            Ok(Some(piece)) => assert!(piece.is_empty(), "no byte was sent"),
            Ok(None) => panic!("the stream ended one announced byte early with no error"),
            Err(error) => break error,
        }
    };
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
    assert_eq!(error.to_string(), SHORT);
}

// C5.5 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_5_a_body_that_no_arm_names_cannot_change_the_result() {
    let script = Script::new();
    script.answer(200).body("par").cut();
    success(tasks(&script).delete(7).await);

    let script = Script::new();
    script.answer(204).body("par").stall();
    let start = Instant::now();
    success(tasks(&script).delete(7).await);
    assert_eq!(start.elapsed(), secs(0), "the body is not waited for");

    let script = Script::new();
    script.answer(200).body("par").cut();
    assert!(success(files(&script).exists(tour::DocId::from("a")).await));
}

// C5.6 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_6_a_refusal_keeps_a_small_body_whole() {
    let script = Script::new();
    script.answer(404).body("gone");

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(error.body(), Some(&b"gone"[..]));
    assert_eq!(error.kept(), Some(Kept::Complete));
}

#[tokio::test(start_paused = true)]
async fn c5_6_a_refusal_keeps_at_most_64_kib_and_problem_needs_a_complete_body() {
    // A problem of 100 KiB: it would decode, but only 64 KiB are kept.
    let long = format!(
        r#"{{"status":{{"message":"{}","status_code":503}}}}"#,
        "m".repeat(100 * 1024)
    );
    let script = Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .body(long.clone());

    let error = failure(match_get(&script).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.body().unwrap().len(), 64 * 1024);
    assert_eq!(error.body().unwrap(), &long.as_bytes()[..64 * 1024]);
    assert_eq!(error.kept(), Some(Kept::Cut));
    assert!(error.problem().is_none());
}

#[tokio::test(start_paused = true)]
async fn c5_6_problem_is_tried_from_a_complete_body_and_never_makes_another_error() {
    let script = Script::new();
    json(&script, 503, PROBLEM);
    let error = failure(match_get(&script).await);
    assert_eq!(error.problem().unwrap().status.message, "slow down");

    let script = Script::new();
    json(&script, 503, "<html>busy</html>");
    let error = failure(match_get(&script).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(503));
    assert!(error.problem().is_none());
    assert_eq!(error.body(), Some(&b"<html>busy</html>"[..]));
}

#[tokio::test(start_paused = true)]
async fn c5_6_with_no_problem_line_problem_is_always_none() {
    let script = Script::new();
    json(&script, 404, PROBLEM);
    let error = failure(tasks(&script).get(7).await);
    assert!(error.problem().is_none());
}

#[tokio::test(start_paused = true)]
async fn c5_6_a_mismatch_keeps_the_body_and_problem_is_not_tried() {
    // The body would decode as the problem type: a body is never read to see what it is.
    let script = Script::new();
    json(&script, 200, PROBLEM);

    let error = failure(match_get(&script).await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.body(), Some(PROBLEM.as_bytes()));
    assert_eq!(error.kept(), Some(Kept::Complete));
    assert!(error.problem().is_none());
}

#[tokio::test(start_paused = true)]
async fn c5_6_a_refusal_and_a_mismatch_keep_64_kib_whole_and_cut_after() {
    // C5.6: "a Refused error and a Mismatch keep up to 64 KiB of the body"; `kept()` says
    // complete or cut.
    const KIB64: usize = 64 * 1024;
    for size in [40 * 1024, KIB64 - 1, KIB64, KIB64 + 1] {
        let body = vec![b'x'; size];
        let kept = size.min(KIB64);
        let state = if size > KIB64 {
            Kept::Cut
        } else {
            Kept::Complete
        };

        let script = Script::new();
        script.answer(404).body(body.clone());
        let error = failure(tasks(&script).get(7).await);
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.body(), Some(&body[..kept]), "refusal of {size} bytes");
        assert_eq!(error.kept(), Some(state), "refusal of {size} bytes");

        let script = Script::new();
        script.answer(200).body(body.clone());
        let error = failure(tasks(&script).get(7).await);
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(
            error.body(),
            Some(&body[..kept]),
            "mismatch of {size} bytes"
        );
        assert_eq!(error.kept(), Some(state), "mismatch of {size} bytes");
    }
}

#[test]
fn c5_6_a_mismatch_keeps_its_bytes_in_a_buffer_of_its_own() {
    // C5.6: "a Refused error and a Mismatch keep up to 64 KiB of the body, in a buffer of
    // their own". A reader hands over a part of the transport's allocation: the error does
    // not keep that allocation.
    let freed = Arc::new(AtomicBool::new(false));
    let body = Bytes::from_owner(Watched(b"not json at all", freed.clone()));

    let error = ReadError::not_the_type(body.slice(..8));
    drop(body);

    assert!(
        freed.load(Ordering::SeqCst),
        "the error keeps the transport's allocation"
    );
    drop(error);
}

// C5.7 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_7_a_refusal_whose_body_breaks_is_still_that_refusal() {
    let script = Script::new();
    script.answer(404).header("x-a", "1").body("par").cut();

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(404));
    assert_eq!(error.class(), None);
    assert_eq!(error.body(), Some(&b"par"[..]));
    assert_eq!(error.kept(), Some(Kept::Interrupted));
    assert_eq!(error.to_string(), "refused: status 404, in tasks.get");
}

#[tokio::test(start_paused = true)]
async fn c5_7_a_refusal_whose_body_never_ends_is_still_that_refusal() {
    let script = Script::new();
    script.answer(404).body("par").stall();
    let start = Instant::now();

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(
        start.elapsed(),
        secs(30),
        "the clock for silence ended the read"
    );
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(404));
    assert_eq!(error.body(), Some(&b"par"[..]));
    assert_eq!(error.kept(), Some(Kept::Interrupted));
}

#[tokio::test]
async fn c5_7_a_refusal_whose_body_ends_early_on_the_wire() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(404)
        .declare_length(100)
        .body("par");
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let error = failure(
        tour::Tasks::new()
            .with_transport(http)
            .with_base(format!("{}/api/tasks", server.url()))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(404));
    assert_eq!(error.body(), Some(&b"par"[..]));
    assert_eq!(error.kept(), Some(Kept::Interrupted));
}

// C5.8 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_8_question_mark_on_the_apis_problem_in_an_arm_gives_a_refused_error_with_no_bytes() {
    // `shapes.refusal`: `4xx { body } => Err::<_, ShapesProblem>(body)?`.
    let script = Script::new();
    json(&script, 429, SHAPES_PROBLEM);

    let error = failure(shapes(&script).refusal(1).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(429));
    assert_eq!(error.problem().unwrap().message, "slow down");
    assert_eq!(
        error.body(),
        None,
        "it keeps the typed problem and no bytes"
    );
    assert_eq!(error.reason(), None);
}

// C5.9 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_9_send_full_takes_the_same_path_and_keeps_the_head() {
    let script = Script::new();
    script
        .answer(201)
        .header("content-type", "application/json")
        .header("x-request-id", "r-1")
        .body(TASK);

    let answer = success(tasks(&script).get(7).send_full().await);

    assert_eq!(answer.status(), 201);
    assert_eq!(answer.headers().get("x-request-id"), Some("r-1"));
    assert_eq!(answer.value().id, 7);

    let script = Script::new();
    script.answer(404);
    let error = failure(tasks(&script).get(7).send_full().await);
    assert_eq!(error.to_string(), "refused: status 404, in tasks.get");
}

// C5.10 -----------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_10_a_success_ends_the_try_at_the_head_and_the_body_is_handed_over_unread() {
    // "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    let script = Script::new();
    script
        .answer(200)
        .header("x-a", "1")
        .body("a")
        .pause(secs(10))
        .body("b");
    let start = Instant::now();

    let mut stream = success(files(&script).download(tour::FileId::from("f")).await);

    assert_eq!(start.elapsed(), secs(0), "the call ends at the head");
    assert_eq!(stream.status(), 200);
    assert_eq!(stream.headers().get("x-a"), Some("1"));
    assert_eq!(
        stream_success(stream.try_next().await).as_deref(),
        Some(&b"a"[..])
    );
    assert_eq!(
        stream_success(stream.try_next().await).as_deref(),
        Some(&b"b"[..])
    );
    assert_eq!(stream_success(stream.try_next().await), None);
    assert_eq!(start.elapsed(), secs(10));
}

#[tokio::test(start_paused = true)]
async fn c5_10_a_refusal_is_read_like_any_refusal() {
    let script = Script::new();
    script.answer(404).body("gone");

    let error = failure(files(&script).download(tour::FileId::from("f")).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.body(), Some(&b"gone"[..]));
    assert_eq!(error.kept(), Some(Kept::Complete));
}

#[tokio::test(start_paused = true)]
async fn c5_10_what_fails_later_is_a_transport_error_from_the_stream() {
    // "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    let script = Script::new();
    script.answer(200).header("x-a", "1").body("a").cut();

    let mut stream = success(files(&script).download(tour::FileId::from("f")).await);

    assert_eq!(
        stream_success(stream.try_next().await).as_deref(),
        Some(&b"a"[..])
    );
    let error: concord::read::StreamError = stream_failure(stream.try_next().await);
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(error.is_status(200));
    assert_eq!(error.headers().unwrap().get("x-a"), Some("1"));
    assert_eq!(error.endpoint(), "files.download");
    assert_eq!(error.tries(), 1);
    assert_eq!(
        error.to_string(),
        "transport error: the body broke after status 200, in files.download"
    );
}

// C5.11 -----------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_11_json_reads_a_2xx_whatever_its_label_says() {
    for label in [None, Some("text/plain"), Some("application/problem+json")] {
        let script = Script::new();
        {
            let mut answer = script.answer(200);
            if let Some(label) = label {
                answer = answer.header("content-type", label);
            }
            let _ = answer.body(TASK);
        }
        assert_eq!(success(tasks(&script).get(7).await).id, 7, "{label:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c5_11_json_skips_a_utf8_byte_order_mark() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(format!("\u{feff}{TASK}"));
    assert_eq!(success(tasks(&script).get(7).await).id, 7);
}

#[tokio::test(start_paused = true)]
async fn c5_11_a_body_that_does_not_decode_prints_the_place_and_never_the_value() {
    let script = Script::new();
    json(
        &script,
        200,
        r#"[{"id":1,"title":"a","done":false},{"id":"value-9d1c","title":"b","done":false}]"#,
    );

    let error = failure(tasks(&script).list().await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, [1].id is not what the type says, in tasks.list"
    );
    assert_eq!(error.reason(), None);
    let source = std::error::Error::source(&error).expect("a cause that Concord makes");
    let mut chain = source.to_string();
    let mut next = source.source();
    while let Some(cause) = next {
        chain.push_str(&cause.to_string());
        next = cause.source();
    }
    assert!(!chain.contains("value-9d1c"), "{chain}");
    assert!(!format!("{error:?}").contains("value-9d1c"));
}

#[tokio::test(start_paused = true)]
async fn c5_11_a_place_prints_key_and_variant_and_holds_at_most_32_pieces() {
    // C5.11: "`.key()` for a key of a map, printed `[key]`; `.variant()` for a tag of an enum
    // that was read, printed `[variant]`; 32 pieces at most".
    assert_eq!(
        Place::root().field("items").key().variant().to_string(),
        "items[key][variant]"
    );
    let long = (0..40).fold(Place::root(), |place, i| place.index(i));
    let first_32: String = (0..32).map(|i| format!("[{i}]")).collect();
    assert_eq!(long.to_string(), first_32);
    let place = (0..31)
        .fold(Place::root(), |place, i| place.index(i))
        .variant();
    let kept: String = (0..31).map(|i| format!("[{i}]")).collect::<String>() + "[variant]";
    assert_eq!(place.to_string(), kept, "the 32nd piece is kept");

    // The line of the Mismatch prints the place that the reader gave.
    for (place, printed) in [
        (Place::root().variant(), "[variant]".to_owned()),
        (long, first_32),
    ] {
        let script = Script::new();
        script.answer(200).body("{}");
        let core = concord::__private::Core::default().with_transport(script);
        let result = prepared_get(core, RefusesAt(place)).send().await;
        assert_eq!(
            failure(result).to_string(),
            format!("mismatch: status 200, {printed} is not what the type says, in tasks.get")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c5_11_a_failed_decode_says_when_the_label_was_not_the_readers() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "text/html")
        .body("<html>maintenance</html>");

    let error = failure(tasks(&script).get(7).await);

    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body is not what the type says and its content type is not the reader's, in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c5_11_text_refuses_bytes_that_are_not_utf8() {
    let script = Script::new();
    script.answer(200).body(&b"\xff\xfe"[..]);

    let error = failure(files(&script).readme("repo").await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.reason(), None);
}

// C5.12 -----------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c5_12_send_if_none_match_writes_the_header_and_a_304_is_none_with_its_headers() {
    let script = Script::new();
    script
        .answer(304)
        .header("etag", "\"v2\"")
        .header("cache-control", "max-age=60");

    let answer = success(tasks(&script).get(7).send_if_none_match("\"v1\"").await);

    assert_eq!(script.requests()[0].header("if-none-match"), Some("\"v1\""));
    assert_eq!(answer.status(), 304);
    assert_eq!(answer.headers().get("etag"), Some("\"v2\""));
    assert_eq!(answer.headers().get("cache-control"), Some("max-age=60"));
    assert!(answer.into_value().is_none());
}

#[tokio::test(start_paused = true)]
async fn c5_12_any_other_answer_is_read_as_by_send_full_and_is_some() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .header("etag", "\"v2\"")
        .body(TASK);
    let answer = success(tasks(&script).get(7).send_if_none_match("\"v1\"").await);
    assert_eq!(answer.headers().get("etag"), Some("\"v2\""));
    assert_eq!(answer.into_value().unwrap().id, 7);

    // A HEAD: what its arm gives, in `Some`.
    let script = Script::new();
    script.answer(304);
    let answer = success(
        files(&script)
            .exists(tour::DocId::from("a"))
            .send_if_none_match("\"v1\"")
            .await,
    );
    assert!(answer.into_value().is_none());
    let script = Script::new();
    script.answer(200);
    let answer = success(
        files(&script)
            .exists(tour::DocId::from("a"))
            .send_if_none_match("\"v1\"")
            .await,
    );
    assert_eq!(answer.into_value(), Some(true));
}

#[tokio::test(start_paused = true)]
async fn c5_12_the_header_is_written_before_the_edits_of_edit_request() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let script = Script::new();
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .get(7)
            .edit_request(move |request| {
                let value = request
                    .headers_mut()
                    .remove("if-none-match")
                    .map(|value| value.to_str().unwrap().to_owned());
                record.lock().unwrap().push(value);
            })
            .send_if_none_match("\"v1\"")
            .await,
    );

    assert_eq!(*seen.lock().unwrap(), [Some("\"v1\"".to_owned())]);
    assert_eq!(script.requests()[0].header("if-none-match"), None);
}
