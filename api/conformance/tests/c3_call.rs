//! C3, one call: the request built once, a copy for each try, `blocked`, the way out through
//! the middlewares, a try as the whole exchange, what is final, the one `Run`, and
//! `read_response` (docs/contracts.md C3; docs/engine.md sections 2 and 3; docs/rules.md
//! rules 18 and 20).
//!
//! C3.8 (the hidden `__private::exchange`) has no behaviour of its own that a caller can
//! see: every test of a send here goes through it.

mod common;

use common::engine::*;
use common::stream;
use concord::error::{Class, Kind};
use concord::transport::TransportError;
use concord::Retry;
use concord_testkit::Script;
use http::HeaderValue;
use tokio::time::Instant;

// C3.1 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_1_an_edit_runs_once_for_a_call_and_its_change_is_on_every_try() {
    let script = Script::new();
    script.answer(503);
    script.answer(503);
    json(&script, 200, TASK);
    let count = Count::default();
    let edits = count.clone();

    let task = success(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .edit_request(move |request| {
                edits.add();
                request
                    .headers_mut()
                    .insert("x-edit", HeaderValue::from_static("1"));
            })
            .await,
    );

    assert_eq!(task.id, 7);
    assert_eq!(count.get(), 1, "the edit runs once for the call");
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    for (n, request) in requests.iter().enumerate() {
        assert_eq!(request.header("x-edit"), Some("1"), "try {}", n + 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c3_1_edits_run_in_the_order_written() {
    let script = Script::new();
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .get(7)
            .edit_request(|request| {
                request
                    .headers_mut()
                    .insert("x-order", HeaderValue::from_static("first"));
            })
            .edit_request(|request| {
                let before = request.headers()["x-order"].to_str().unwrap().to_owned();
                request.headers_mut().insert(
                    "x-order",
                    HeaderValue::from_str(&format!("{before},second")).unwrap(),
                );
            })
            .await,
    );

    assert_eq!(script.requests()[0].header("x-order"), Some("first,second"));
}

#[tokio::test(start_paused = true)]
async fn c3_1_an_edit_runs_once_for_send_raw_send_full_and_send_if_none_match() {
    // C3.1: "`.edit_request(f)` runs once on each request that is built: once for a call".
    // Each way of sending builds the request once, also when the policy tries again.
    let ways: [(&str, Way); 3] = [
        ("send_raw()", |call| {
            Box::pin(async move { call.send_raw().await.map(drop) })
        }),
        ("send_full()", |call| {
            Box::pin(async move { call.send_full().await.map(drop) })
        }),
        ("send_if_none_match(etag)", |call| {
            Box::pin(async move { call.send_if_none_match("\"v1\"").await.map(drop) })
        }),
    ];
    for (way, send) in ways {
        let script = Script::new();
        script.answer(503);
        json(&script, 200, TASK);
        let count = Count::default();
        let edits = count.clone();
        let call = tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .edit_request(move |_| edits.add());

        success(send(call).await);

        assert_eq!(script.requests().len(), 2, "{way}: two tries");
        assert_eq!(count.get(), 1, "{way}: the edit runs once for the call");
    }
}

/// A send of `tasks.get`, its value left out.
type Sent = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), concord::Error>>>>;

/// A way of sending `tasks.get`.
type Way = fn(tour::tasks::Get) -> Sent;

// C3.2 ------------------------------------------------------------------------------

/// `Files` with a policy that sends a `PUT` marked `idempotent false;` again after a `503`
/// (C4.1, by its status): only the copy of C3.2 can then stop another try.
fn upload_retry(script: &Script) -> tour::Files {
    files(script).with_retry(Retry::standard().not_applied_on(503))
}

#[tokio::test(start_paused = true)]
async fn c3_2_each_try_sends_a_copy_of_the_same_bytes() {
    let script = Script::new();
    script.answer(503);
    script.answer(200);

    success(
        upload_retry(&script)
            .upload("a.txt", concord::Body::new("abc", "text/plain"))
            .await,
    );

    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.body(), Some(&b"abc"[..]));
    }
}

#[tokio::test(start_paused = true)]
async fn c3_2_a_stream_body_gets_one_try() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();

    let error = failure(upload_retry(&script).upload("a.txt", stream("abc")).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    let requests = script.requests();
    assert_eq!(requests.len(), 1, "a stream is sent once");
    assert_eq!(requests[0].upload().bytes(), b"abc");
    assert_eq!(script.unused(), 1);
}

// C3.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_3_a_middleware_that_blocks_ends_the_tries() {
    let script = Script::new();
    json(&script, 200, TASK);
    let count = Count::default();

    let error = failure(
        tasks(&script)
            .with_middleware(Blocks {
                count: count.clone(),
            })
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Blocked));
    assert_eq!(count.get(), 1, "no other try");
    assert_eq!(error.tries(), 1);
    assert!(
        script.requests().is_empty(),
        "nothing reached the transport"
    );
}

#[tokio::test(start_paused = true)]
async fn c3_3_a_transport_that_blocks_ends_the_tries() {
    let script = Script::new();
    script.fail(TransportError::blocked());
    json(&script, 200, TASK);

    let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

    assert_eq!(error.class(), Some(Class::Blocked));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c3_3_blocked_after_a_refusal_is_the_result_not_the_refusal() {
    // D244 withdrew "the result is the error of the try before".
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_middleware(BlocksFrom(2))
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Blocked));
    assert_eq!(error.status(), None);
    assert_eq!(error.tries(), 2);
    assert_eq!(script.requests().len(), 1);
}

// C3.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_4_the_callers_middlewares_then_the_descriptions_then_the_transport_and_back() {
    let script = Script::new();
    script.answer(200).body("read me");
    let log = Log::default();

    let text = success(
        files(&script)
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .with_middleware(Logs {
                log: log.clone(),
                name: "b",
            })
            .readme("repo")
            .await,
    );

    assert_eq!(text, "read me");
    assert_eq!(
        log.events(),
        [
            "a out 1 files.readme",
            "a sees x-signature: false",
            "b out 1 files.readme",
            "b sees x-signature: false",
            "b back",
            "a back",
        ]
    );
    let request = &script.requests()[0];
    assert_eq!(request.header("x-a"), Some("1"));
    assert_eq!(request.header("x-b"), Some("1"));
    assert!(
        request.headers().contains_key("x-signature"),
        "the author's signer is the last before the transport"
    );
}

#[tokio::test(start_paused = true)]
async fn c3_4_with_transport_set_before_with_middleware_is_still_under_it() {
    let script = Script::new();
    script.answer(200).body("read me");
    let log = Log::default();

    success(
        tour::Files::new("key")
            .with_transport(script.clone())
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .readme("repo")
            .await,
    );

    assert_eq!(log.events().len(), 3);
    assert_eq!(script.requests()[0].header("x-a"), Some("1"));
}

#[tokio::test(start_paused = true)]
async fn c3_4_the_descriptions_middlewares_run_in_the_order_written_and_back() {
    // C3.4: "then through those of the description, in the order written, then to the
    // transport [...] The answer comes back the same way." `api/tour` has no description with
    // two middlewares: the client is made as generated code makes it (`add_middleware`, in
    // the order of the description), with a caller's middleware added after.
    let script = Script::new();
    script.answer(200);
    let log = Log::default();
    let core = concord::__private::Core::<()>::default()
        .add_middleware(Logs {
            log: log.clone(),
            name: "first",
        })
        .add_middleware(Logs {
            log: log.clone(),
            name: "second",
        })
        .with_middleware(Logs {
            log: log.clone(),
            name: "caller",
        });
    let request = concord::reqwest::Request::new(
        concord::reqwest::Method::GET,
        "https://example.test/a".parse().unwrap(),
    );

    let answer = core
        .next(&script)
        .send(request, concord::transport::TryInfo::new("tasks.get", 1))
        .await;

    assert!(answer.is_ok());
    assert_eq!(
        log.events(),
        [
            "caller out 1 tasks.get",
            "caller sees x-signature: false",
            "first out 1 tasks.get",
            "first sees x-signature: false",
            "second out 1 tasks.get",
            "second sees x-signature: false",
            "second back",
            "first back",
            "caller back",
        ]
    );
}

// C3.5 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_5_a_body_that_breaks_while_it_is_read_is_the_outcome_of_its_try() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"id":"#)
        .cut();
    json(&script, 200, TASK);

    let task = success(tasks(&script).with_retry(Retry::standard()).get(7).await);

    assert_eq!(task.id, 7);
    assert_eq!(
        script.requests().len(),
        2,
        "the broken body made another try"
    );
}

#[tokio::test(start_paused = true)]
async fn c3_5_with_read_stream_the_try_ends_at_the_head() {
    // D273: "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    let script = Script::new();
    script.answer(200).body("ab").cut();
    script.answer(200).body("never");

    let mut stream = success(
        files(&script)
            .with_retry(Retry::standard())
            .download(tour::FileId::from("f"))
            .await,
    );

    assert_eq!(
        stream_success(stream.try_next().await).as_deref(),
        Some(&b"ab"[..])
    );
    let error: concord::read::StreamError = stream_failure(stream.try_next().await);
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert_eq!(script.requests().len(), 1, "no other try after the head");
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c3_5_for_send_raw_the_try_ends_at_the_head() {
    let script = Script::new();
    script.answer(200).body("ab").cut();
    json(&script, 200, TASK);

    let response = success(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .send_raw()
            .await,
    );

    assert_eq!(response.status(), 200);
    assert!(
        response.bytes().await.is_err(),
        "the caller reads the break"
    );
    assert_eq!(script.requests().len(), 1);
}

// C3.6 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_6_a_mismatch_is_final() {
    let script = Script::new();
    json(&script, 200, "not json");
    json(&script, 200, TASK);
    let asked = Count::default();
    let rule = asked.clone();

    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard().when(move |_| {
                rule.add();
                Some(SECOND)
            }))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(asked.get(), 0, "a rule is not asked about a final outcome");
}

#[tokio::test(start_paused = true)]
async fn c3_6_a_build_error_is_final() {
    let script = Script::new();
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_base("not a url")
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Build);
    assert!(script.requests().is_empty());
}

/// A policy whose rule gives a wait for every outcome it is asked about.
fn always() -> Retry {
    Retry::standard().when(|_| Some(SECOND))
}

#[tokio::test(start_paused = true)]
async fn c3_6_for_send_raw_a_head_below_400_is_final_like_a_value() {
    for status in [200, 302, 399] {
        let script = Script::new();
        script.answer(status);
        script.answer(200);

        let response = success(tasks(&script).with_retry(always()).get(7).send_raw().await);

        assert_eq!(response.status(), status);
        assert_eq!(script.requests().len(), 1, "status {status}");
    }
}

#[tokio::test(start_paused = true)]
async fn c3_6_c4_7_for_send_raw_a_head_of_exactly_400_is_looked_at_by_the_rules() {
    // C3.6: "a head of `400` or above is looked at by the policy, as a refusal is". The
    // standard table does not hold 400 (C4.2): a `when` rule asks for another try with no
    // wait of its own (C4.7), so only the random part of C4.3 is waited, up to 1 s.
    let script = Script::new();
    script.answer(400);
    script.answer(200);
    let retry = Retry::standard().when(|tried| {
        tried
            .status()
            .filter(|status| status.as_u16() == 400)
            .map(|_| std::time::Duration::ZERO)
    });
    let start = Instant::now();

    let response = success(tasks(&script).with_retry(retry).get(7).send_raw().await);

    assert_eq!(response.status(), 200);
    assert_eq!(script.requests().len(), 2, "400 is not final");
    assert!(start.elapsed() <= SECOND, "{:?}", start.elapsed());
}

#[tokio::test(start_paused = true)]
async fn c3_6_a_head_below_400_through_send_is_looked_at_by_the_rule() {
    // The same 302, sent with `send()`: a Refused error, which the rule may try again.
    let script = Script::new();
    script.answer(302);
    json(&script, 200, TASK);

    success(tasks(&script).with_retry(always()).get(7).await);

    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c3_6_for_send_raw_a_head_of_400_or_above_is_looked_at_by_the_policy() {
    let script = Script::new();
    script.answer(503);
    script.answer(200);
    let response = success(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .send_raw()
            .await,
    );
    assert_eq!(response.status(), 200);
    assert_eq!(script.requests().len(), 2);

    // When the tries run out, the last head is the result, unchanged.
    let script = Script::new();
    for _ in 0..3 {
        script.answer(503).header("x-try", "same");
    }
    let response = success(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .send_raw()
            .await,
    );
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers()["x-try"], "same");
    assert_eq!(script.requests().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn c3_6_for_send_raw_a_request_that_may_not_be_repeated_is_sent_once() {
    // C3.6: "a head of `400` or above is looked at by the policy, as a refusal is"; the
    // policy's gate is C4.1: an unmarked `POST` may not be repeated.
    let script = Script::new();
    script.answer(503);
    script.answer(200);
    let response = success(
        tasks(&script)
            .with_retry(Retry::standard())
            .create(new_task())
            .send_raw()
            .await,
    );
    assert_eq!(response.status(), 503, "the first head is the result");
    assert_eq!(script.requests().len(), 1);

    // A `POST` with `idempotent;` may: the same policy sends it again.
    let script = Script::new();
    script.answer(503);
    script.answer(200);
    let response = success(
        shapes(&script)
            .with_retry(Retry::standard())
            .again(1)
            .send_raw()
            .await,
    );
    assert_eq!(response.status(), 200);
    assert_eq!(script.requests().len(), 2);
}

// C3.9 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c3_9_no_request_is_handed_to_the_transport_after_timeout_has_ended() {
    let script = Script::new();
    json(&script, 200, TASK);
    let start = Instant::now();

    let error = failure(
        tasks(&script)
            .with_middleware(Waits {
                before: secs(100),
                after: secs(0),
            })
            .with_retry(Retry::standard())
            .get(7)
            .with_timeout(secs(60))
            .await,
    );

    assert_eq!(start.elapsed(), secs(60));
    assert!(error.is_timeout());
    assert_eq!(
        error.tries(),
        1,
        "a try is counted when it is handed to the middlewares"
    );
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c3_9_a_timeout_of_zero_hands_no_request_to_the_transport() {
    // Building is not a wait and finishes; then the instant has passed.
    let script = Script::new();
    json(&script, 200, TASK);

    let error = failure(tasks(&script).get(7).with_timeout(secs(0)).await);

    assert!(error.is_timeout());
    assert!(script.requests().is_empty());
}

// C3.10 -----------------------------------------------------------------------------

#[tokio::test]
async fn c3_10_read_response_reads_an_answer_that_came_from_anywhere() {
    let call = tour::Tasks::new().get(7);
    let task = success(
        call.read_response(response(200, &[("content-type", "application/json")], TASK))
            .await,
    );
    assert_eq!(task.id, 7);
}

#[tokio::test]
async fn c3_10_read_response_uses_the_arms_and_the_problem_of_the_endpoint() {
    let riot = tour::Riot::new("key");

    let none = success(
        riot.account(tour::Europe)
            .by_riot_id("Faker", "KR1")
            .read_response(response(404, &[], ""))
            .await,
    );
    assert!(none.is_none());

    let error = failure(
        riot.matches(tour::Europe)
            .get(tour::MatchId::from("EUW1_1"))
            .read_response(response(
                429,
                &[("content-type", "application/json")],
                PROBLEM,
            ))
            .await,
    );
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(429));
    assert_eq!(error.problem().unwrap().status.message, "slow down");
}

#[tokio::test]
async fn c3_10_read_response_has_no_retry_and_its_error_has_no_tries() {
    let call = tour::Tasks::new().with_retry(Retry::standard()).get(7);
    let error = failure(call.read_response(response(503, &[], "busy")).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 0);
    assert_eq!(error.endpoint(), "tasks.get");
}

#[tokio::test(start_paused = true)]
async fn c3_10_read_response_has_no_clock() {
    // A body that is silent for longer than every clock that the client sets.
    let source = Script::new();
    source
        .answer(200)
        .header("content-type", "application/json")
        .pause(secs(100))
        .body(TASK);
    let request = concord::reqwest::Request::new(
        concord::reqwest::Method::GET,
        "http://tasks.test/7".parse().unwrap(),
    );
    let answer = concord::Transport::send(
        &source,
        request,
        concord::transport::TryInfo::new("source", 1),
    )
    .await
    .unwrap();
    let start = Instant::now();

    let call = tour::Tasks::new()
        .with_idle_timeout(secs(1))
        .with_timeout(secs(2))
        .get(7);
    let task = success(call.read_response(answer).await);

    assert_eq!(task.id, 7);
    assert_eq!(start.elapsed(), secs(100));
}
