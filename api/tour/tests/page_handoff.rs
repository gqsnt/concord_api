//! Executable page preparation and tour pagination contracts.
#![allow(dead_code)]

use concord::__private::{Call, Prepared};

// Preserve the exact page preparation signature.
#[allow(clippy::type_complexity)]
fn build_page<C: Call>(
    call: C,
) -> Result<Prepared<C::Value, C::Problem>, concord::Error<C::Problem>> {
    call.clone().prepare()
}

fn send_page<C: Call>(
    prepared: Prepared<C::Value, C::Problem>,
) -> concord::Sending<concord::Answer<C::Value>, C::Problem> {
    // An engine can inspect the finished request and its marks at this boundary.
    let _ = (
        &prepared.run,
        &prepared.request,
        &prepared.secrets,
        prepared.idempotent,
    );
    prepared.send()
}

#[tokio::test(start_paused = true)]
async fn c10_10_tour_calls_run_the_generic_prepare_handoff() {
    let script = Script::new();
    script.answer(200).body("[]");
    script.answer(200).body(r#"{"data":[],"next_cursor":null}"#);
    script.answer(200).body("[]");
    let tasks = tour::Tasks::new().with_transport(script.clone());
    let feed = tour::Feed::new().with_transport(script.clone());
    let riot = tour::Riot::new("key").with_transport(script.clone());
    assert!(
        send_page::<tour::tasks::List>(build_page(tasks.list()).unwrap())
            .await
            .unwrap()
            .value()
            .is_empty()
    );
    assert!(
        send_page::<tour::feed::List>(build_page(feed.list()).unwrap())
            .await
            .unwrap()
            .value()
            .data
            .is_empty()
    );
    assert!(send_page::<tour::riot::matches::Ids>(
        build_page(riot.matches(tour::Europe).ids(tour::Puuid::from("p"))).unwrap()
    )
    .await
    .unwrap()
    .value()
    .is_empty());
    assert_eq!(script.requests().len(), 3);
}

use concord::error::{Class, Kept, Kind};
use concord_testkit::Script;
use std::time::Duration;

#[tokio::test(start_paused = true)]
/// Also proves C8.7.
async fn c8_4_riot_counting_pager_builds_each_request_with_current_secrets() {
    let script = Script::new();
    script.answer(200).body(r#"["one","two"]"#);
    script.answer(200).body(r#"["three"]"#);
    let key = concord::Secret::from("old");
    let riot = tour::Riot::new(&key).with_transport(script.clone());
    let mut items = riot
        .matches(tour::Europe)
        .ids(tour::Puuid::from("p"))
        .count(2)
        .pages()
        .items();
    assert!(script.requests().is_empty());
    assert_eq!(items.try_next().await.unwrap().unwrap().to_string(), "one");
    assert_eq!(items.try_next().await.unwrap().unwrap().to_string(), "two");
    assert_eq!(script.requests().len(), 1);
    key.replace("new");
    assert_eq!(
        items.try_next().await.unwrap().unwrap().to_string(),
        "three"
    );
    assert!(items.try_next().await.unwrap().is_none());
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("x-riot-token"), Some("old"));
    assert_eq!(requests[1].header("x-riot-token"), Some("new"));
    assert!(requests[1].url().contains("start=2"));
}

#[tokio::test(start_paused = true)]
/// Also proves C8.6.
async fn c8_4_feed_pagers_follow_cursors_and_relative_links_through_empty_pages() {
    for variant in 0..3 {
        let script = Script::new();
        script
            .answer(200)
            .header("link", r#"<../other/next?page=2#part>; rel="next""#)
            .body(r#"{"data":[],"next_cursor":"next"}"#);
        script
            .answer(200)
            .body(r#"{"data":[{"id":"file","name":"kept"}],"next_cursor":null}"#);
        let feed = tour::Feed::new().with_transport(script.clone());
        let pages = match variant {
            0 => feed.list().pages(),
            1 => feed.by_cursor().pages(),
            _ => feed.linked().pages(),
        };
        let mut items = pages.items();
        assert_eq!(items.try_next().await.unwrap().unwrap().name, "kept");
        assert!(items.try_next().await.unwrap().is_none());
        let requests = script.requests();
        assert_eq!(requests.len(), 2);
        if variant == 2 {
            assert_eq!(
                requests[1].url(),
                "https://feed.example.com/other/next?page=2"
            );
        } else {
            assert!(requests[1].url().contains("starting_after=next"));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c8_4_files_search_pages_by_changed_body_bytes() {
    let script = Script::new();
    script
        .answer(200)
        .body(r#"{"data":[],"next_cursor":"next"}"#);
    script
        .answer(200)
        .body(r#"{"data":[{"id":"file","name":"found"}],"next_cursor":null}"#);
    let files = tour::Files::new("key").with_transport(script.clone());
    // Search currently has no paginate statement in tour. Exercise its prepared
    // POST with the ordinary engine and its existing cursor setter.
    let mut pages = concord::__private::pages(files.search("needle"), |call, answer| {
        answer
            .value()
            .next_cursor()
            .map(|cursor| call.cursor(cursor))
    });
    assert!(pages.try_next().await.unwrap().unwrap().data.is_empty());
    assert_eq!(
        pages.try_next().await.unwrap().unwrap().data[0].name,
        "found"
    );
    assert!(pages.try_next().await.unwrap().is_none());
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].body(), requests[1].body());
    assert!(std::str::from_utf8(requests[1].body().unwrap())
        .unwrap()
        .contains("next"));
}

#[tokio::test(start_paused = true)]
async fn c10_10_page_preparation_sends_the_same_request_and_keeps_the_full_answer() {
    let script = Script::new();
    script
        .answer(201)
        .header("x-kept", "yes")
        .body(r#"{"data":[],"next_cursor":null}"#);
    script
        .answer(201)
        .header("x-kept", "yes")
        .body(r#"{"data":[],"next_cursor":null}"#);
    let call = tour::Feed::new()
        .with_transport(script.clone())
        .list()
        .starting_after("cursor");
    let prepared = build_page(call.clone()).unwrap();
    let url = prepared.request.url().clone();
    let answer = send_page::<tour::feed::List>(prepared).await.unwrap();
    assert_eq!(answer.status(), 201);
    assert_eq!(answer.headers().get("x-kept"), Some("yes"));
    assert!(answer.value().data.is_empty());
    assert_eq!(script.requests()[0].url(), url.as_str());
    let answer = call.send_full().await.unwrap();
    assert_eq!(answer.status(), 201);
    assert_eq!(answer.headers().get("x-kept"), Some("yes"));
    assert!(answer.value().data.is_empty());
}

#[tokio::test(start_paused = true)]
async fn c7_6_generated_read_body_watches_silence_and_accepts_continuing_pieces() {
    for stalled in [false, true] {
        let script = Script::new();
        if stalled {
            script
                .answer(200)
                .body("a")
                .pause(Duration::from_secs(2))
                .body("b");
        } else {
            script
                .answer(200)
                .body("a")
                .pause(Duration::from_millis(750))
                .body("b")
                .pause(Duration::from_millis(750))
                .body("c");
        }
        let files = tour::Files::new("key")
            .with_transport(script.clone())
            .with_idle_timeout(Duration::from_secs(1));
        let result = files.readme("repo").await;
        if stalled {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), Kind::Transport);
            assert_eq!(error.class(), Some(Class::Timeout));
            // A33: "After a head, while a body is read: `transport error: the body was silent for 30 s after status 200 (<hint>), in <endpoint>`."
            assert_eq!(error.to_string(), "transport error: the body was silent for 1 s after status 200 (with_idle_timeout changes it), in files.readme");
        } else {
            assert_eq!(result.unwrap(), "abc");
        }
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_generated_refusal_body_preserves_known_outcome_when_silence_ends_it() {
    for stalled in [false, true] {
        let script = Script::new();
        if stalled {
            script
                .answer(403)
                .body("a")
                .pause(Duration::from_secs(2))
                .body("b");
        } else {
            script
                .answer(403)
                .body("a")
                .pause(Duration::from_millis(750))
                .body("b")
                .pause(Duration::from_millis(750))
                .body("c");
        }
        let files = tour::Files::new("key")
            .with_transport(script.clone())
            .with_idle_timeout(Duration::from_secs(1));
        let error = files.readme("repo").await.unwrap_err();
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.to_string(), "refused: status 403, in files.readme");
        assert_eq!(
            error.kept(),
            Some(if stalled {
                Kept::Interrupted
            } else {
                Kept::Complete
            })
        );
        assert_eq!(
            error.body(),
            Some(if stalled {
                b"a".as_slice()
            } else {
                b"abc".as_slice()
            })
        );
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_generated_byte_stream_retains_its_silence_clock_after_handover() {
    for stalled in [false, true] {
        let script = Script::new();
        if stalled {
            script
                .answer(200)
                .body("a")
                .pause(Duration::from_secs(2))
                .body("b");
        } else {
            script
                .answer(200)
                .body("a")
                .pause(Duration::from_millis(750))
                .body("b")
                .pause(Duration::from_millis(750))
                .body("c");
        }
        let files = tour::Files::new("key")
            .with_transport(script.clone())
            .with_idle_timeout(Duration::from_secs(1));
        let mut body = files
            .download(tour::FileId::from("file"))
            .with_idle_timeout(Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(body.try_next().await.unwrap().unwrap(), "a");
        if stalled {
            let error = body.try_next().await.unwrap_err();
            assert_eq!(error.class(), Some(Class::Timeout));
            // A33: "After a head, while a body is read: `transport error: the body was silent for 30 s after status 200 (<hint>), in <endpoint>`."
            assert_eq!(error.to_string(), "transport error: the body was silent for 1 s after status 200 (with_idle_timeout changes it), in files.download");
        } else {
            assert_eq!(body.try_next().await.unwrap().unwrap(), "b");
            assert_eq!(body.try_next().await.unwrap().unwrap(), "c");
        }
        assert!(body.try_next().await.unwrap().is_none());
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_a42_a_plain_http_response_uses_the_request_url_for_relative_links() {
    use concord::transport::{Transport, TransportError, TryInfo};
    use std::sync::{Arc, Mutex};
    struct Plain(Arc<Mutex<Vec<String>>>);
    impl Plain {
        fn response(&self, request: &concord::reqwest::Request) -> concord::reqwest::Response {
            let mut seen = self.0.lock().unwrap();
            seen.push(request.url().to_string());
            let mut response = concord::http::Response::new(r#"{"data":[],"next_cursor":null}"#);
            if seen.len() == 1 {
                response
                    .headers_mut()
                    .insert("link", "<?page=2>; rel=\"next\"".parse().unwrap());
            }
            response.into()
        }
    }
    impl Transport for Plain {
        async fn send(
            &self,
            request: concord::reqwest::Request,
            _: TryInfo,
        ) -> Result<concord::reqwest::Response, TransportError> {
            Ok(self.response(&request))
        }
    }
    struct MovePath;
    impl concord::Middleware for MovePath {
        async fn handle(
            &self,
            mut request: concord::reqwest::Request,
            info: TryInfo,
            next: concord::Next<'_>,
        ) -> Result<concord::reqwest::Response, TransportError> {
            if request.url().path() == "/linked-items" {
                request.url_mut().set_path("/answers/current");
            }
            next.send(request, info).await
        }
    }
    for moved in [false, true] {
        for fake in [false, true] {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let transport = Plain(seen.clone());
            let feed = if fake {
                tour::Feed::new().with_transport(concord::fake(move |request| {
                    Ok(transport.response(request))
                }))
            } else {
                tour::Feed::new().with_transport(transport)
            };
            let feed = if moved {
                feed.with_middleware(MovePath)
            } else {
                feed
            };
            let mut pages = feed.linked().pages();
            assert!(pages.try_next().await.unwrap().is_some());
            assert!(pages.try_next().await.unwrap().is_some());
            assert!(pages.try_next().await.unwrap().is_none());
            assert_eq!(
                *seen.lock().unwrap(),
                if moved {
                    [
                        "https://feed.example.com/answers/current",
                        "https://feed.example.com/answers/current?page=2",
                    ]
                } else {
                    [
                        "https://feed.example.com/linked-items",
                        "https://feed.example.com/linked-items?page=2",
                    ]
                }
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_a44_stream_silence_runs_between_polls_but_try_next_resets_on_drop() {
    use futures_core::Stream;
    use std::{pin::Pin, task::Poll, time::Duration};
    for stream_form in [false, true] {
        let script = Script::new();
        script
            .answer(200)
            .body("a")
            .pause(Duration::from_secs(8))
            .body("b");
        let files = tour::Files::new("k").with_transport(script);
        let mut stream = files
            .download(tour::FileId::from("f"))
            .with_idle_timeout(Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(stream.try_next().await.unwrap().unwrap(), "a");
        if stream_form {
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(
                    Pin::new(&mut stream).poll_next(cx).is_pending()
                ))
                .await
            );
        } else {
            let mut waiting = Box::pin(stream.try_next());
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(
                    std::future::Future::poll(waiting.as_mut(), cx).is_pending()
                ))
                .await
            );
        }
        tokio::time::advance(Duration::from_secs(6)).await;
        if stream_form {
            let error = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
                .await
                .unwrap()
                .unwrap_err();
            assert!(error.is_timeout());
        } else {
            assert_eq!(stream.try_next().await.unwrap().unwrap(), "b");
        }
    }
}
