//! C9, the transport and the middlewares: what really sends, a fake, what a transport gets,
//! `with_transport`, `TryInfo`, `blocked`, `Middleware` and `Next` (docs/reference/contracts.md C9;
//! docs/internals/engine.md, "The way out: middleware, then the transport"; docs/reference/rules.md rules 21 and 26).

mod common;

use common::engine::*;
use concord::error::{Class, Kind};
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{reqwest, Retry};
use concord_testkit::server::Server;
use concord_testkit::Script;
use http::HeaderValue;
use std::sync::Mutex;
use tokio::time::Instant;

/// A transport of the caller's own: the task, after a delay of virtual time.
struct Delayed(std::time::Duration);

impl Transport for Delayed {
    async fn send(
        &self,
        _request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        tokio::time::sleep(self.0).await;
        Ok(response(200, &[("content-type", "application/json")], TASK))
    }
}

// C9.1 ------------------------------------------------------------------------------

#[tokio::test]
async fn c9_1_a_reqwest_client_is_a_transport() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    let task = success(
        tour::Tasks::new()
            .with_transport(client)
            .with_base(format!("{}/api/tasks", server.url()))
            .get(7)
            .await,
    );

    assert_eq!(task.id, 7);
    assert_eq!(server.requests()[0].target(), "/api/tasks/7");
}

#[tokio::test(start_paused = true)]
async fn c9_1_a_type_of_the_caller_is_a_transport() {
    let task = success(
        tour::Tasks::new()
            .with_transport(Delayed(secs(0)))
            .get(7)
            .await,
    );
    assert_eq!(task.id, 7);
}

// C9.2 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c9_2_a_fake_gives_a_whole_answer() {
    let fake = concord::fake(|request| {
        assert_eq!(request.url().path(), "/api/tasks/7");
        Ok(concord::reply_with(
            200,
            &[("content-type", "application/json")],
            TASK,
        ))
    });
    let task = success(tour::Tasks::new().with_transport(fake).get(7).await);
    assert_eq!(task.id, 7);

    let fake = concord::fake(|_| Ok(concord::reply(404, "gone")));
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    assert!(error.is_status(404));
    assert_eq!(error.body(), Some(&b"gone"[..]));
}

#[tokio::test(start_paused = true)]
async fn c9_2_a_fake_gives_a_failure_before_any_head() {
    let fake = concord::fake(|_| Err(TransportError::connect()));
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
}

/// A body in pieces, from an answer of a `Script`: `script` shapes it.
async fn pieces(shape: impl FnOnce(&Script)) -> reqwest::Body {
    let source = Script::new();
    shape(&source);
    let request = reqwest::Request::new(reqwest::Method::GET, "http://body.test/".parse().unwrap());
    let answer = source
        .send(request, TryInfo::new("body.source", 1))
        .await
        .unwrap();
    reqwest::Body::wrap_stream(answer.bytes_stream())
}

#[tokio::test(start_paused = true)]
async fn c9_2_a_fake_gives_a_body_in_pieces_where_a_piece_fails_or_never_comes() {
    // A piece that fails.
    let body = Mutex::new(Some(
        pieces(|script| {
            script.answer(200).body(r#"{"id":7,"#).cut();
        })
        .await,
    ));
    let fake = concord::fake(move |_| {
        let body = body.lock().unwrap().take().expect("one try");
        Ok(concord::reply(200, body))
    });
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    assert_eq!(error.class(), Some(Class::Body));

    // A piece that never comes.
    let body = Mutex::new(Some(
        pieces(|script| {
            script.answer(200).body(r#"{"id":7,"#).stall();
        })
        .await,
    ));
    let fake = concord::fake(move |_| {
        let body = body.lock().unwrap().take().expect("one try");
        Ok(concord::reply(200, body))
    });
    let start = Instant::now();
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(30));
}

#[tokio::test(start_paused = true)]
async fn c9_2_an_answer_after_a_delay_is_a_type_of_the_caller() {
    let start = Instant::now();
    let task = success(
        tour::Tasks::new()
            .with_transport(Delayed(secs(5)))
            .get(7)
            .await,
    );
    assert_eq!(task.id, 7);
    assert_eq!(start.elapsed(), secs(5));
}

// C9.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c9_3_a_transport_gets_each_try_after_the_edits_and_the_middlewares() {
    let script = Script::new();
    script.answer(503);
    script.answer(503);
    json(&script, 200, TASK);
    let log = Log::default();

    success(
        tasks(&script)
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .with_retry(Retry::standard())
            .get(7)
            .edit_request(|request| {
                request
                    .headers_mut()
                    .insert("x-edit", HeaderValue::from_static("1"));
            })
            .await,
    );

    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    for (n, request) in requests.iter().enumerate() {
        assert_eq!(request.header("x-edit"), Some("1"));
        assert_eq!(request.header("x-a"), Some("1"));
        assert_eq!(request.info().number(), n as u32 + 1);
        assert_eq!(request.info().endpoint(), "tasks.get");
    }
}

#[tokio::test(start_paused = true)]
async fn c9_3_the_engine_sees_one_try_whatever_a_middleware_sends_inside_it() {
    let script = Script::new();
    script.answer(200);
    script.answer(503);
    script.answer(200);
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .with_middleware(SendsTwice)
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );

    let numbers: Vec<_> = script
        .requests()
        .iter()
        .map(|request| request.info().number())
        .collect();
    assert_eq!(numbers, [1, 1, 2, 2]);
}

// C9.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c9_4_the_last_with_transport_wins() {
    let script = Script::new();
    json(&script, 200, TASK);

    let task = success(
        tour::Tasks::new()
            .with_transport(FailsWith(Class::Other))
            .with_transport(script.clone())
            .get(7)
            .await,
    );

    assert_eq!(task.id, 7);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c9_4_with_transport_replaces_what_sends_under_the_middlewares() {
    let script = Script::new();
    script.answer(200).body("text");

    success(
        tour::Files::new("key")
            .with_middleware(Logs {
                log: Log::default(),
                name: "a",
            })
            .with_transport(FailsWith(Class::Other))
            .with_transport(script.clone())
            .readme("repo")
            .await,
    );

    let request = &script.requests()[0];
    assert_eq!(request.header("x-a"), Some("1"), "the caller's middleware");
    assert!(
        request.headers()["x-signature"].is_sensitive(),
        "the author's signer"
    );
}

// C9.5 ------------------------------------------------------------------------------

#[test]
fn c9_5_a_transport_error_says_its_class_and_try_info_is_clone() {
    assert_eq!(TransportError::connect().class(), Class::Connect);
    assert_eq!(TransportError::timeout().class(), Class::Timeout);
    assert_eq!(TransportError::blocked().class(), Class::Blocked);
    assert_eq!(TransportError::new(Class::Body).class(), Class::Body);

    let info = TryInfo::new("tasks.get", 1);
    let copy = info.clone();
    assert_eq!(copy.endpoint(), "tasks.get");
    assert_eq!(copy.number(), 1);
}

#[tokio::test(start_paused = true)]
async fn c9_5_try_info_names_the_endpoint_as_an_error_prints_it_and_counts_from_1() {
    let script = Script::new();
    script.answer(503);
    script.answer(503);
    let log = Log::default();

    let error = failure(
        riot(&script)
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .with_retry(Retry::standard().tries(2))
            .matches(tour::Europe)
            .get(tour::MatchId::from("EUW1_1"))
            .await,
    );

    let outs: Vec<_> = log
        .events()
        .into_iter()
        .filter(|event| event.contains(" out "))
        .collect();
    assert_eq!(
        outs,
        ["a out 1 riot.matches.get", "a out 2 riot.matches.get"]
    );
    assert_eq!(error.endpoint(), "riot.matches.get");
    assert!(error.to_string().contains(", in riot.matches.get"));
}

#[tokio::test(start_paused = true)]
async fn c9_5_blocked_is_a_failure_of_class_blocked_and_the_result() {
    let script = Script::new();
    script.fail(TransportError::blocked());
    script.answer(200);

    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard().not_applied_on(503))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Blocked));
    assert_eq!(script.unused(), 1);
}

// C9.6 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c9_6_next_send_may_be_called_again_inside_one_try() {
    let script = Script::new();
    script.answer(401);
    json(&script, 200, TASK);

    let task = success(tasks(&script).with_middleware(Renews).get(7).await);

    assert_eq!(task.id, 7);
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("authorization"), Some("Bearer old"));
    assert_eq!(requests[1].header("authorization"), Some("Bearer new"));
    assert!(requests.iter().all(|request| request.info().number() == 1));
}

#[tokio::test(start_paused = true)]
async fn c9_6_next_send_may_not_be_called_at_all() {
    let script = Script::new();
    let error = failure(
        tasks(&script)
            .with_middleware(Blocks::default())
            .get(7)
            .await,
    );
    assert_eq!(error.class(), Some(Class::Blocked));
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c9_6_a_middleware_runs_for_every_try_and_the_callers_come_first() {
    let script = Script::new();
    script.answer(503);
    script.answer(200).body("text");
    let log = Log::default();

    success(
        files(&script)
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .with_retry(Retry::standard())
            .readme("repo")
            .await,
    );

    assert_eq!(
        log.events(),
        [
            "a out 1 files.readme",
            "a sees x-signature: false",
            "a back",
            "a out 2 files.readme",
            "a sees x-signature: false",
            "a back",
        ]
    );
    for request in script.requests() {
        assert!(request.headers().contains_key("x-signature"));
    }
}

#[test]
fn c9_6_request_and_into_request_give_the_request_before_any_middleware() {
    let files = tour::Files::new("key").with_middleware(Logs {
        log: Log::default(),
        name: "a",
    });

    let request = files.readme("repo").request().expect("a request");
    assert!(!request.headers().contains_key("x-signature"));
    assert!(!request.headers().contains_key("x-a"));
    assert!(request.headers().contains_key("authorization"));

    let request = files.readme("repo").into_request().expect("a request");
    assert!(!request.headers().contains_key("x-signature"));
    assert!(!request.headers().contains_key("x-a"));
}

// C8.6: an answer with no URL of its own ------------------------------------

/// The URL that `tasks.get(7)` sends to.
fn task_url() -> String {
    tour::Tasks::new()
        .get(7)
        .request()
        .expect("a request")
        .url()
        .to_string()
}

/// A middleware that notes, for each try, the URL of its request and the URL of the answer
/// that `next.send` gave, before anything else looks at that answer.
#[derive(Clone, Default)]
struct SeesUrls(std::sync::Arc<Mutex<Vec<(String, String)>>>);

impl concord::Middleware for SeesUrls {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: concord::Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let sent = request.url().to_string();
        let answer = next.send(request, info).await?;
        self.0
            .lock()
            .expect("the notes")
            .push((sent, answer.url().to_string()));
        Ok(answer)
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_a_middleware_sees_the_request_url_on_an_answer_that_has_none() {
    // C8.6: "an answer with no URL of its own, as a transport of the caller, a `fake` or a
    // `tower` service makes it from an `http::Response`, is taken to come from the URL of
    // its request". A middleware gets the answer of `next.send` with that URL already.
    let fake = concord::fake(|_| {
        Ok(concord::reply_with(
            200,
            &[("content-type", "application/json")],
            TASK,
        ))
    });
    let seen = SeesUrls::default();
    success(
        tour::Tasks::new()
            .with_transport(fake)
            .with_middleware(seen.clone())
            .get(7)
            .await,
    );

    // A transport of the caller's own, whose answer is made from an `http::Response`.
    let own = SeesUrls::default();
    success(
        tour::Tasks::new()
            .with_transport(Delayed(SECOND))
            .with_middleware(own.clone())
            .get(7)
            .await,
    );

    for notes in [seen, own] {
        let notes = notes.0.lock().expect("the notes").clone();
        assert_eq!(notes, [(task_url(), task_url())]);
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_an_answer_whose_url_only_looks_like_the_placeholder_keeps_it() {
    // C8.6: only an answer with no URL of its own takes the URL of its request. An answer
    // that carries a URL keeps it, also when that URL has the placeholder's host.
    use reqwest::ResponseBuilderExt as _;
    for own in [
        "http://no.url.provided.local/x",
        "https://no.url.provided.local/",
        "http://no.url.provided.local:8080/",
        "https://tasks.example.com/elsewhere",
    ] {
        let url: reqwest::Url = own.parse().expect("a URL");
        let fake = concord::fake(move |_| {
            let answer = http::Response::builder()
                .status(200)
                .url(url.clone())
                .body(reqwest::Body::from(""))
                .expect("an answer");
            Ok(reqwest::Response::from(answer))
        });
        let seen = SeesUrls::default();
        let response = success(
            tour::Tasks::new()
                .with_transport(fake)
                .with_middleware(seen.clone())
                .get(7)
                .send_raw()
                .await,
        );
        assert_eq!(response.url().as_str(), own);
        let notes = seen.0.lock().expect("the notes").clone();
        assert_eq!(notes, [(task_url(), own.to_owned())]);
    }

    // The placeholder itself is no URL of its own.
    let fake = concord::fake(|_| Ok(concord::reply(200, "")));
    let response = success(
        tour::Tasks::new()
            .with_transport(fake)
            .get(7)
            .send_raw()
            .await,
    );
    assert_eq!(response.url().as_str(), task_url());
}
