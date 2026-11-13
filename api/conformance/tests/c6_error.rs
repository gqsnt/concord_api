//! C6, the error: its kind, each accessor, `source()`, the printed line, `details()`, the
//! name of an endpoint, its size (docs/contracts.md C6; docs/rules.md rule 15;
//! docs/messages.md section 2).
//!
//! Each text of docs/messages.md section 2 that a call without pages can produce with the
//! endpoints of `api/tour` is compared as a whole line in
//! `c6_2_the_texts_of_messages_section_2`. The others need an endpoint that `api/tour` does
//! not have, or pages.

mod common;

use common::engine::*;
use common::stream;
use concord::error::{Class, Kept, Kind};
use concord::page::Call as _;
use concord::transport::TransportError;
use concord::Retry;
use concord_testkit::Script;

/// The texts of an error and of every source below it, joined.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut next = error.source();
    while let Some(cause) = next {
        text.push_str(" / ");
        text.push_str(&cause.to_string());
        next = cause.source();
    }
    text
}

fn match_get(script: &Script) -> tour::riot::matches::Get {
    riot(script)
        .matches(tour::Europe)
        .get(tour::MatchId::from("EUW1_1"))
}

// C6.1 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c6_1_the_accessors_of_a_refused_error() {
    let script = Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .header("retry-after", "7")
        .header("x-a", "1")
        .body(PROBLEM);

    let error = failure(match_get(&script).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.endpoint(), "riot.matches.get");
    assert_eq!(error.status(), Some(http::StatusCode::SERVICE_UNAVAILABLE));
    assert!(error.is_status(503));
    assert!(!error.is_status(500));
    assert_eq!(error.headers().unwrap().get("x-a"), Some("1"));
    assert_eq!(error.body(), Some(PROBLEM.as_bytes()));
    assert_eq!(error.kept(), Some(Kept::Complete));
    assert_eq!(error.problem().unwrap().status.message, "slow down");
    assert_eq!(error.tries(), 1);
    assert_eq!(error.class(), None);
    assert!(!error.is_timeout());
    assert_eq!(error.retry_after(), Some(secs(7)));
    assert_eq!(error.reason(), None);
    assert!(std::error::Error::source(&error).is_none());
}

#[tokio::test(start_paused = true)]
async fn c6_1_into_problem_and_map_problem() {
    let script = Script::new();
    json(&script, 503, PROBLEM);
    let error = failure(match_get(&script).await);
    let problem = error.into_problem().expect("the problem");
    assert_eq!(problem.status.status_code, 429);

    let script = Script::new();
    json(&script, 503, PROBLEM);
    let mapped =
        failure(match_get(&script).await).map_problem(|problem| problem.status.status_code);
    assert_eq!(mapped.problem(), Some(&429));
    assert!(mapped.is_status(503));
    assert_eq!(mapped.endpoint(), "riot.matches.get");
    assert_eq!(mapped.kind(), Kind::Refused);
    assert_eq!(
        mapped.to_string(),
        "refused: status 503, in riot.matches.get"
    );

    // With no problem, the error comes back whole.
    let script = Script::new();
    json(&script, 503, "busy");
    let back = match failure(match_get(&script).await).into_problem() {
        Ok(_) => panic!("there is no problem"),
        Err(back) => back,
    };
    assert!(back.is_status(503));
    assert_eq!(back.body(), Some(&b"busy"[..]));
}

#[tokio::test(start_paused = true)]
async fn c6_1_the_accessors_of_a_transport_error_and_its_source() {
    let error = failure(
        tour::Tasks::new()
            .with_transport(FailsWith(Class::Other))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(error.status(), None);
    assert!(error.headers().is_none());
    assert_eq!(error.body(), None);
    assert_eq!(error.kept(), None);
    assert!(error.problem().is_none());
    assert_eq!(error.tries(), 1);
    assert_eq!(error.reason(), None);
    assert!(!error.is_timeout());
    let source = std::error::Error::source(&error).expect("the source that the transport gave");
    assert_eq!(chain(source), CALLER_TEXT);

    let script = Script::new();
    script.never();
    let error = failure(tasks(&script).get(7).await);
    assert!(error.is_timeout());
    assert_eq!(error.class(), Some(Class::Timeout));
}

#[tokio::test(start_paused = true)]
async fn c6_1_a_middleware_gives_the_source_of_a_blocked_error() {
    let script = Script::new();
    let error = failure(
        tasks(&script)
            .with_middleware(Blocks::default())
            .get(7)
            .await,
    );
    let source = std::error::Error::source(&error).expect("the source that the middleware gave");
    assert_eq!(chain(source), CALLER_TEXT);
}

#[tokio::test(start_paused = true)]
async fn c6_1_an_error_that_concord_makes_has_no_source() {
    let script = Script::new();
    let error = failure(tasks(&script).with_base("not a url").get(7).await);
    assert_eq!(error.kind(), Kind::Build);
    assert!(std::error::Error::source(&error).is_none());

    let error = failure(files(&script).readme("..").await);
    assert!(std::error::Error::source(&error).is_none());
}

#[tokio::test]
async fn c6_1_the_source_of_a_failure_of_the_http_library_holds_no_url() {
    let (_bound, port) = refusing_port();
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let error = failure(
        tour::Tasks::new()
            .with_transport(http)
            .with_base(format!("http://127.0.0.1:{port}/api/tasks"))
            .get(987_654_321)
            .await,
    );

    assert_eq!(error.class(), Some(Class::Connect));
    let source = std::error::Error::source(&error).expect("the error of the HTTP library");
    let text = chain(source);
    assert!(!text.contains("987654321"), "{text}");
    assert!(!text.contains("/api/tasks"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn c6_1_map_problem_keeps_everything_but_the_problem() {
    // C6.1: `map_problem(f)` changes the problem; the error is otherwise the same error.
    let script = Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .header("retry-after", "7")
        .header("x-a", "1")
        .body(PROBLEM);
    let error = failure(match_get(&script).await);
    let before = (
        error.kind(),
        error.endpoint(),
        error.status(),
        error.body().map(<[u8]>::to_vec),
        error.kept(),
        error.tries(),
        error.class(),
        error.retry_after(),
        error.reason().map(str::to_owned),
        error.to_string(),
    );
    let mapped = error.map_problem(|problem| problem.status.status_code);
    let after = (
        mapped.kind(),
        mapped.endpoint(),
        mapped.status(),
        mapped.body().map(<[u8]>::to_vec),
        mapped.kept(),
        mapped.tries(),
        mapped.class(),
        mapped.retry_after(),
        mapped.reason().map(str::to_owned),
        mapped.to_string(),
    );
    assert_eq!(after, before);
    assert_eq!(before.3.as_deref(), Some(PROBLEM.as_bytes()));
    assert_eq!(mapped.headers().and_then(|h| h.get("x-a")), Some("1"));
    assert_eq!(mapped.problem(), Some(&429));

    // With no problem: the body and its state stay.
    let script = Script::new();
    script.answer(404).header("x-a", "1").body("evidence");
    let mapped = failure(tasks(&script).get(7).await).map_problem(|()| 1u32);
    assert_eq!(mapped.body(), Some(&b"evidence"[..]));
    assert_eq!(mapped.kept(), Some(Kept::Complete));
    assert_eq!(mapped.headers().and_then(|h| h.get("x-a")), Some("1"));
    assert!(mapped.problem().is_none());
}

#[test]
fn c6_1_a_reqwest_error_about_a_status_keeps_no_source() {
    // ANSWERS.md A12: "A `reqwest` error that is about a status or about decoding keeps no
    // source." Its text holds the reason phrase of the answer.
    let library = concord::reply(418, "").error_for_status().unwrap_err();
    assert!(library.is_status());
    let failure = TransportError::from(library);
    assert!(std::error::Error::source(&failure).is_none());
}

#[tokio::test(start_paused = true)]
async fn c6_1_through_a_call_a_reqwest_error_about_a_status_keeps_no_source() {
    // The same, as a transport of the caller's own gives it with `?`.
    let fake = concord::fake(|_| {
        let library = concord::reply(418, "").error_for_status().unwrap_err();
        Err(TransportError::from(library))
    });
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    assert_eq!(error.kind(), Kind::Transport);
    assert!(
        std::error::Error::source(&error).is_none(),
        "{}",
        chain(&error)
    );
    let details = error.details().to_string();
    assert!(
        !details.contains("418") && !details.contains("teapot"),
        "{details}"
    );
}

// C6.2 ------------------------------------------------------------------------------

/// A cell: the printed line of what `send` gives, as a whole line.
fn text<T: Send + 'static>(
    row: &'static str,
    expected: &'static str,
    send: impl std::future::Future<Output = Result<T, concord::Error>> + Send + 'static,
) -> Cell {
    cell(row, "printed text", async move {
        match send.await {
            Ok(_) => Err(format!("a value, not {expected:?}")),
            Err(error) => {
                let line = error.to_string();
                check(line == expected, || format!("{line:?}"))
            }
        }
    })
}

/// The same, for Riot.
fn riot_text<T: Send + 'static>(
    row: &'static str,
    expected: &'static str,
    send: impl std::future::Future<Output = Result<T, concord::Error<tour::RiotProblem>>>
        + Send
        + 'static,
) -> Cell {
    cell(row, "printed text", async move {
        match send.await {
            Ok(_) => Err(format!("a value, not {expected:?}")),
            Err(error) => {
                let line = error.to_string();
                check(line == expected, || format!("{line:?}"))
            }
        }
    })
}

#[tokio::test(start_paused = true)]
async fn c6_2_the_texts_of_messages_section_2() {
    let mut cells = Vec::new();

    let script = Script::new();
    // A37: "A `with_base` that is no base: `build error: the base is not a URL that can be used (with_base), in <endpoint>`."
    cells.push(text(
        "Invalid base, port or changed parsed host",
        "build error: the base is not a URL that can be used (with_base), in tasks.get",
        tasks(&script).with_base("not a url").get(7).send(),
    ));

    for repo in ["", ".", ".."] {
        cells.push(text(
            "An empty path value, or a finished segment that is . or ..",
            "build error: input repo gives an empty or dot path segment, in files.readme",
            files(&script).readme(repo).send(),
        ));
    }

    cells.push(riot_text(
        "Invalid final header value",
        "build error: the value of header authorization is not a legal header value, in riot.account.me",
        riot(&script).account(tour::Europe).me("tok\nen").send(),
    ));

    let upload = files(&script).upload("a.bin", stream("abc"));
    cells.push(cell(
        "request() on a call whose body is a one-shot stream",
        "printed text",
        async move {
            let line = match upload.request() {
                Ok(_) => return Err("a request".to_owned()),
                Err(error) => error.to_string(),
            };
            check(
                line == "build error: the body is a stream; into_request takes it, in files.upload",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(cell(
        "A body that is a one-shot stream, sent a second time",
        "printed text",
        async {
            let script = Script::new();
            script.answer(200).read_upload();
            script.answer(200).read_upload();
            let call = files(&script).upload("a.bin", stream("abc"));
            let again = call.clone();
            if let Err(error) = call.await {
                return Err(format!("the first send: {error}"));
            }
            let line = match again.await {
                Ok(()) => return Err("a second send".to_owned()),
                Err(error) => error.to_string(),
            };
            check(
                line == "build error: the body is a stream that was already sent, in files.upload",
                || format!("{line:?}"),
            )?;
            check(script.requests().len() == 1, || {
                "nothing more was sent".to_owned()
            })
        },
    ));

    cells.push(text(
        "A middleware or a transport that will not send",
        "transport error: blocked before it was sent, in tasks.get",
        tasks(&script)
            .with_middleware(Blocks::default())
            .get(7)
            .send(),
    ));

    cells.push(cell(
        "A status with no arm, not a 2xx",
        "printed text",
        async {
            let script = Script::new();
            script.answer(404);
            let line = failure(tasks(&script).get(7).await).to_string();
            check(line == "refused: status 404, in tasks.get", || {
                format!("{line:?}")
            })
        },
    ));

    cells.push(cell("The same, after tries", "printed text", async {
        let script = Script::new();
        for _ in 0..3 {
            json(&script, 429, PROBLEM);
        }
        let result = riot(&script)
            .with_retry(Retry::standard())
            .matches(tour::Europe)
            .ids(tour::Puuid::from("p-1"))
            .send()
            .await;
        let line = failure(result).to_string();
        check(
            line == "refused: status 429, in riot.matches.ids, after 3 tries",
            || format!("{line:?}"),
        )
    }));

    cells.push(cell("A body larger than its limit", "printed text", async {
        let script = Script::new();
        script
            .answer(200)
            .declare_length(10 * concord::MIB as u64 + 1)
            .stall();
        let line = failure(tasks(&script).get(7).await).to_string();
        check(
            line == "mismatch: status 200, the body is larger than the limit of the reader, in tasks.get",
            || format!("{line:?}"),
        )
    }));

    cells.push(cell("A body that does not decode", "printed text", async {
        let script = Script::new();
        json(
            &script,
            200,
            r#"[{"id":1,"title":"a","done":false},{"id":2,"title":"b","done":false},{"id":3,"title":"c","done":false},{"id":4,"title":"d","done":"no"}]"#,
        );
        let line = failure(tasks(&script).list().await).to_string();
        check(
            line == "mismatch: status 200, [3].done is not what the type says, in tasks.list",
            || format!("{line:?}"),
        )
    }));

    cells.push(cell(
        "A body that does not read, when its label was not the reader's either",
        "printed text",
        async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "text/html")
                .body("<html></html>");
            let line = failure(tasks(&script).get(7).await).to_string();
            check(
                line == "mismatch: status 200, the body is not what the type says and its content type is not the reader's, in tasks.get",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(cell(
        "A body that breaks or ends early",
        "printed text",
        async {
            let script = Script::new();
            script.answer(200).body("a").cut();
            let mut stream = success(files(&script).download(tour::FileId::from("f")).await);
            let _ = stream.try_next().await;
            // D273: "`ByteStream::try_next()` gives a `concord::read::StreamError`".
            let line = stream_failure(stream.try_next().await).to_string();
            check(
                line == "transport error: the body broke after status 200, in files.download",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(text(
        "A URL given to at_url that leaves the origin that the call builds",
        "build error: the URL is at another origin than the call, in tasks.get",
        tasks(&script)
            .get(7)
            .at_url("https://elsewhere.example/api/tasks/7")
            .send(),
    ));

    cells.push(cell(
        "A connection that failed (no word of the closed list fits)",
        "printed text",
        async {
            let script = Script::new();
            script.fail(TransportError::connect());
            let line = failure(tasks(&script).get(7).await).to_string();
            check(
                line == "transport error: no connection, in tasks.get",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(cell(
        "A connection that failed, after tries",
        "printed text",
        async {
            let script = Script::new();
            for _ in 0..3 {
                script.fail(TransportError::connect());
            }
            let line =
                failure(tasks(&script).with_retry(Retry::standard()).get(7).await).to_string();
            check(
                line == "transport error: no connection, in tasks.get, after 3 tries",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(cell(
        "Silence for longer than idle_timeout",
        "printed text",
        async {
            let script = Script::new();
            script.never();
            let line = failure(tasks(&script).get(7).await).to_string();
            check(
                line == "transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get",
                || format!("{line:?}"),
            )
        },
    ));

    cells.push(cell(
        "The whole call longer than timeout",
        "printed text",
        async {
            let script = Script::new();
            script.never();
            let result = tasks(&script)
                .get(7)
                .with_idle_timeout(secs(120))
                .with_timeout(secs(60))
                .await;
            let line = failure(result).to_string();
            check(
                line == "transport error: the call took longer than 60 s (with_timeout), in tasks.get",
                || format!("{line:?}"),
            )
        },
    ));

    run_cells("messages, section 2", cells).await;
}

#[tokio::test]
async fn c6_2_a_connection_that_was_refused() {
    let (_bound, port) = refusing_port();
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let error = failure(
        tour::Tasks::new()
            .with_transport(http)
            .with_base(format!("http://127.0.0.1:{port}/api/tasks"))
            .get(7)
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(
        error.to_string(),
        "transport error: connection refused, in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c6_2_no_printed_line_holds_text_of_the_callers_code_or_a_value() {
    let script = Script::new();
    let blocked = failure(
        tasks(&script)
            .with_middleware(Blocks::default())
            .get(7)
            .await,
    );
    let other = failure(
        tour::Tasks::new()
            .with_transport(FailsWith(Class::Other))
            .get(7)
            .await,
    );
    let script = Script::new();
    script
        .answer(404)
        .header("x-private", "header-value-31aa")
        .body("body-value-77e0");
    let refused = failure(tasks(&script).get(7).await);

    for error in [blocked, other, refused] {
        for line in [
            error.to_string(),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            for value in [
                CALLER_TEXT,
                "header-value-31aa",
                "body-value-77e0",
                "example.com",
            ] {
                assert!(!line.contains(value), "{line}");
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_debug_shows_the_names_and_needs_no_bound_on_the_problem() {
    struct NoDebug;
    let script = Script::new();
    script.answer(404).body("body-value-77e0");
    let error: concord::Error<NoDebug> =
        failure(tasks(&script).get(7).await).map_problem(|()| NoDebug);

    let debug = format!("{error:?}");

    assert!(debug.contains("tasks.get"), "{debug}");
    assert!(debug.contains("404"), "{debug}");
    assert!(!debug.contains("body-value-77e0"), "{debug}");
}

/// A key of a map of the user's, which no printed line may hold.
const MAP_KEY: &str = "private-map-key-3c9e";

/// A JSON value nested `levels` deep: over 128, it cannot be written (A49).
fn nested(levels: usize) -> serde_json::Value {
    let mut value = serde_json::Value::Null;
    for _ in 0..levels {
        value = serde_json::Value::Array(vec![value]);
    }
    value
}

/// A map of the user's whose `serde` name is the hidden name of the engine's `Entries`.
#[derive(serde::Serialize)]
#[serde(rename = "__concord_body_entries")]
struct LooksLikeEntries(std::collections::BTreeMap<String, serde_json::Value>);

/// `POST tasks.create` with `body json { filters; }`, written as the macro writes a body of
/// entries (as `api/tour` writes `files.search`), with `filters` this value.
fn body_of_entries<T: serde::Serialize>(filters: &T) -> concord::Error {
    use concord::__private::{AdaptValue as _, ValueRef};
    let value = ValueRef(filters);
    let mut entries = concord::body::Entries::new();
    entries.__push("filters", (&value).adapt());
    let mut draft = common::draft(http::Method::POST, "https://tasks.example.com/api/tasks");
    draft.body(concord::json(entries));
    match common::finish(draft, "tasks.create") {
        Ok(_) => panic!("a value nested 200 deep is written"),
        Err(error) => error,
    }
}

#[test]
fn c6_2_a58_a_key_of_a_users_map_is_never_printed() {
    // ANSWERS.md A58: "\"input <name>\" in a Build error is always a name from the
    // description. A key of a map of the user, the name of a type of the user or of a `serde`
    // rename is data: it is never printed. A value that fails inside a map of an entry is
    // reported with the name of the entry." (C6.2)
    let plain = std::collections::BTreeMap::from([(MAP_KEY.to_owned(), nested(200))]);
    let renamed = LooksLikeEntries(plain.clone());
    for (shape, error) in [
        ("a map", body_of_entries(&plain)),
        ("a map named as Entries", body_of_entries(&renamed)),
    ] {
        assert_eq!(error.kind(), Kind::Build, "{shape}");
        assert_eq!(
            error.to_string(),
            common::cannot_be_written("filters", "body", "tasks.create"),
            "{shape}"
        );
        let printed = format!("{error} {error:?} {}", chain(&error));
        assert!(!printed.contains(MAP_KEY), "{shape}: {printed}");
        assert!(!printed.contains("__concord"), "{shape}: {printed}");
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_a_timeout_that_a_transport_reported_names_no_clock() {
    let script = Script::new();
    script.fail(TransportError::timeout());

    let error = failure(tasks(&script).get(7).await);

    assert!(error.is_timeout());
    let line = error.to_string();
    assert!(line.starts_with("transport error: "), "{line}");
    assert!(line.ends_with(", in tasks.get"), "{line}");
    for clock in ["30 s", "with_idle_timeout", "with_timeout"] {
        assert!(!line.contains(clock), "{line}");
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_a_clock_length_prints_its_decimal_part() {
    // ANSWERS.md A20: "A clock length prints in seconds, with a decimal part only when it has
    // one (`30 s`, `1.5 s`, `0.125 s`)."
    for (millis, printed) in [(125, "0.125"), (1500, "1.5"), (2000, "2")] {
        let script = Script::new();
        script.never();
        let error = failure(
            tasks(&script)
                .get(7)
                .with_idle_timeout(std::time::Duration::from_millis(millis))
                .await,
        );
        assert_eq!(
            error.to_string(),
            format!("transport error: no answer for {printed} s (with_idle_timeout changes it), in tasks.get")
        );

        let script = Script::new();
        script.never();
        let error = failure(
            tasks(&script)
                .get(7)
                .with_timeout(std::time::Duration::from_millis(millis))
                .await,
        );
        assert_eq!(
            error.to_string(),
            format!("transport error: the call took longer than {printed} s (with_timeout), in tasks.get")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_the_reason_of_an_arm_is_never_printed() {
    // C6.2: "Never text that came from the author's or the caller's code." `shapes.create`:
    // `3xx => Err(concord::mismatch("a redirect is not a receipt"))?`. The reason is behind
    // `reason()` (C6.1) and `details()` (C6.3) only.
    let script = Script::new();
    script.answer(302).header("location", "/elsewhere");

    let error = failure(shapes(&script).create(item()).await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.reason(), Some("a redirect is not a receipt"));
    assert_eq!(
        error.to_string(),
        "mismatch: status 302, the status arm refused the answer, in shapes.create"
    );
    for printed in [
        error.to_string(),
        format!("{error:?}"),
        std::error::Error::source(&error)
            .map(chain)
            .unwrap_or_default(),
    ] {
        assert!(!printed.contains("redirect is not"), "{printed}");
    }
    assert!(error
        .details()
        .to_string()
        .contains("a redirect is not a receipt"));
}

// C6.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c6_3_details_prints_the_line_the_headers_and_the_kept_body() {
    let script = Script::new();
    script
        .answer(404)
        .header("x-trace", "t-1")
        .body(&b"gone-4b2 a\xffb"[..]);

    let error = failure(tasks(&script).get(7).await);
    let details = error.details().to_string();

    assert!(details.contains(&error.to_string()), "{details}");
    assert!(details.contains("x-trace"), "{details}");
    assert!(details.contains("t-1"), "{details}");
    assert!(details.contains("gone-4b2 a\u{fffd}b"), "{details}");
}

#[tokio::test(start_paused = true)]
async fn c6_3_details_prints_the_typed_problem_with_its_debug() {
    let script = Script::new();
    json(&script, 503, PROBLEM);

    let error = failure(match_get(&script).await);
    let details = error.details().to_string();

    assert!(details.contains("slow down"), "{details}");
}

#[tokio::test(start_paused = true)]
async fn c6_3_details_prints_the_source_with_its_chain() {
    let error = failure(
        tour::Tasks::new()
            .with_transport(FailsWith(Class::Other))
            .get(7)
            .await,
    );
    let details = error.details().to_string();
    assert!(details.contains(CALLER_TEXT), "{details}");
}

#[tokio::test(start_paused = true)]
async fn c6_3_details_prints_the_state_of_the_kept_body() {
    // C6.3: "the kept body as text [...] with its state"; ANSWERS.md A35: "`body (complete,
    // 18 bytes)`, `body (cut at 64 KiB)`, `body (interrupted, 120 bytes)`".
    let script = Script::new();
    script.answer(404).body("gone, for 18 bytes");
    let details = failure(tasks(&script).get(7).await).details().to_string();
    assert!(details.contains("body (complete, 18 bytes)"), "{details}");

    let script = Script::new();
    script.answer(404).body(vec![b'x'; 64 * 1024 + 1]);
    let details = failure(tasks(&script).get(7).await).details().to_string();
    assert!(details.contains("body (cut at 64 KiB)"), "{details}");

    let script = Script::new();
    script.answer(404).body(vec![b'x'; 120]).cut();
    let details = failure(tasks(&script).get(7).await).details().to_string();
    assert!(
        details.contains("body (interrupted, 120 bytes)"),
        "{details}"
    );
}

/// An error of a caller's transport whose own cause has a cause.
#[derive(Debug)]
struct Outer(std::io::Error);

impl std::fmt::Display for Outer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("outer-cause-3e1")
    }
}

impl std::error::Error for Outer {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[tokio::test(start_paused = true)]
async fn c6_3_details_prints_every_cause_of_the_chain() {
    // C6.3: "the `source()` error with its own chain".
    let fake = concord::fake(|_| {
        Err(TransportError::new(Class::Other)
            .with_source(Outer(std::io::Error::other("inner-cause-9b2"))))
    });
    let error = failure(tour::Tasks::new().with_transport(fake).get(7).await);
    let details = error.details().to_string();
    assert!(details.contains("outer-cause-3e1"), "{details}");
    assert!(details.contains("inner-cause-9b2"), "{details}");
}

// C6.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c6_4_the_name_of_an_endpoint_is_the_tree_joined_with_dots() {
    let script = Script::new();
    for _ in 0..4 {
        script.answer(500);
    }
    let names = [
        failure(tasks(&script).get(7).await).endpoint(),
        failure(match_get(&script).await).endpoint(),
        failure(
            riot(&script)
                .account(tour::Europe)
                .by_riot_id("Faker", "KR1")
                .await,
        )
        .endpoint(),
        failure(files(&script).download(tour::FileId::from("f")).await).endpoint(),
    ];
    assert_eq!(
        names,
        [
            "tasks.get",
            "riot.matches.get",
            "riot.account.by_riot_id",
            "files.download"
        ]
    );
    let seen: Vec<_> = script
        .requests()
        .iter()
        .map(|request| request.info().endpoint())
        .collect();
    assert_eq!(seen, names, "info.endpoint() gives the same text");
}

// C6.5 ------------------------------------------------------------------------------

#[test]
fn c6_5_an_error_is_one_pointer_wide() {
    assert_eq!(
        std::mem::size_of::<concord::Error>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<concord::Error<tour::RiotProblem>>(),
        std::mem::size_of::<usize>()
    );
}

// A75 -------------------------------------------------------------------------------

/// The heads that a client cannot read (ANSWERS.md A75: "not HTTP, too many or too large
/// headers, two lengths").
fn heads_that_cannot_be_read() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("not HTTP", b"not HTTP at all\r\n\r\n".to_vec()),
        (
            "two lengths",
            b"HTTP/1.1 200 OK\r\ncontent-length: 1\r\ncontent-length: 2\r\n\r\nxx".to_vec(),
        ),
        (
            "too many headers",
            format!("HTTP/1.1 200 OK\r\n{}\r\n", "x-h: a\r\n".repeat(200)).into_bytes(),
        ),
        (
            "a header too large",
            format!("HTTP/1.1 200 OK\r\nx-big: {}\r\n\r\n", "a".repeat(1 << 20)).into_bytes(),
        ),
    ]
}

/// `Shapes` on this base, through Concord's own client, with the standard rule.
fn shapes_on(base: &str) -> tour::Shapes {
    tour::Shapes::new()
        .with_transport(concord::Http::with(|builder| builder.no_proxy()).unwrap())
        .with_base(base)
        .with_retry(Retry::standard())
}

#[tokio::test]
async fn c6_2_c4_2_a75_a_head_that_cannot_be_read_is_class_other_and_not_tried_again() {
    // ANSWERS.md A75: "when the server's head arrived and cannot be read (not HTTP, too many
    // or too large headers, two lengths), the error is `transport error: the head of the
    // answer could not be read` [...] class Other: the standard rule does not try [it]
    // again". `shapes.again` is a `POST` marked `idempotent`: the standard rule would try a
    // connection that failed again.
    for (case, head) in heads_that_cannot_be_read() {
        let (base, connections) = raw_server(Some(head));

        let error = failure(shapes_on(&base).again(1).await);

        assert_eq!(error.kind(), Kind::Transport, "{case}");
        assert_eq!(error.class(), Some(Class::Other), "{case}");
        assert_eq!(
            error.to_string(),
            "transport error: the head of the answer could not be read, in shapes.again",
            "{case}"
        );
        assert_eq!(error.tries(), 1, "{case}");
        assert_eq!(connections.get(), 1, "{case}");
        let source = std::error::Error::source(&error).expect("the error of the HTTP library");
        let library = source
            .downcast_ref::<reqwest::Error>()
            .expect("a reqwest::Error");
        assert_eq!(library.url(), None, "{case}");
        let printed = format!("{error:?} {}", chain(&error));
        assert!(!printed.contains("127.0.0.1"), "{case}: {printed}");
        assert!(!printed.contains("again/1"), "{case}: {printed}");
    }
}

#[tokio::test]
async fn c6_2_c4_2_a75_a_request_body_that_fails_is_class_other_and_keeps_its_source() {
    // ANSWERS.md A75: "when the caller's own body stream fails while it is sent, it is
    // `transport error: the body of the request failed`, with the stream's error as the
    // source. [...] class Other: the standard rule does not try [it] again".
    let (base, connections) = raw_server(None);

    let error = failure(
        shapes_on(&base)
            .upload(concord::Body::stream(BreaksAfterOnePiece(0)).content_type("text/plain"))
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(
        error.to_string(),
        "transport error: the body of the request failed, in shapes.upload"
    );
    assert_eq!(error.tries(), 1);
    assert_eq!(connections.get(), 1);
    // The source is kept: the stream's own error is in the chain, with its text.
    let mut cause = std::error::Error::source(&error);
    let mut kept = false;
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<std::io::Error>() {
            kept |= error.to_string() == CALLER_TEXT;
        }
        cause = error.source();
    }
    assert!(
        kept,
        "the stream's error is not in the chain: {}",
        chain(&error)
    );
    // Nothing of the caller's text in the line.
    for line in [
        error.to_string(),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        assert!(!line.contains(CALLER_TEXT), "{line}");
    }
}
