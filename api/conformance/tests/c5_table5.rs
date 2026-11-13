//! Table 5 of docs/cases.md, "The answer": each kind of status, for each kind of endpoint
//! and each way of sending. Every cell is run; a cell that fails names its row and column.
//!
//! The columns are endpoints of `api/tour`:
//!
//! | Column of the table | Endpoint |
//! |---|---|
//! | No arrow | `tasks.delete`; `shapes.remove` for an arm, in `c5_arms.rs` |
//! | `-> T` | `tasks.get`; `riot.matches.get` for an arm (`4xx { body } => Err(body)?`) |
//! | `-> Option<T>` | `riot.account.by_riot_id` (`override 404 => None`) |
//! | `-> Result<T, E>` | in `c5_arms.rs`, on `shapes.outcome` and `shapes.settled` |
//! | `HEAD`, no arrow | in `c5_arms.rs`, on `shapes.ping` and `shapes.wrapped.touch` |
//! | `HEAD` with an arrow | `files.exists` (`2xx => true, 404 => false`); `shapes.present` and `shapes.wrapped.holds` in `c5_arms.rs` |
//! | `read Stream` | `files.download`; `shapes.part` and `shapes.sealed.file` in `c5_arms.rs` |
//! | `send_raw()`, `read_response`, `send_if_none_match(etag)` | `tasks.get`, or the endpoint that has the arm of the row |
//!
//! The cells that need the fifth client, `Shapes`, are in `c5_arms.rs`, with the row "A 2xx
//! with an arm from above that binds `body`" (`shapes.wrapped`, and `shapes.sealed` for
//! `read Stream`: `unpack` takes an `Envelope`, which a stream is not). The two files hold 68
//! of the 70 cells. The two others are the macro's, and are not run here: "A 2xx with an arm
//! from above that binds `body`" under no arrow (message 39) and under `-> Result<T, E>`
//! (message 46) are refused at compile time; they are cases of the macro's catalogue.

mod common;

use common::engine::*;
use concord::error::{Kept, Kind};
use concord_testkit::Script;

const NO_ARROW: &str = "No arrow";
const T: &str = "-> T";
const OPTION: &str = "-> Option<T>";
const HEAD_ARROW: &str = "HEAD with an arrow";
const STREAM: &str = "read Stream";
const RAW: &str = "send_raw()";
const READ: &str = "read_response";
const IF_NONE_MATCH: &str = "send_if_none_match(etag)";

const WITH_ARM: &str = "A 2xx with an arm";
const WITHOUT_ARM: &str = "A 2xx without an arm";
const NO_CONTENT: &str = "A 204";
const REDIRECT: &str = "A 3xx";
const ERROR_WITH_ARM: &str = "A 4xx or 5xx with an arm";
const ERROR_WITHOUT_ARM: &str = "A 4xx or 5xx without an arm";

/// A Refused error with this status.
fn refused<T, E>(result: Result<T, concord::Error<E>>, status: u16) -> Result<(), String> {
    let error = match result {
        Ok(_) => return Err(format!("a value, not a Refused error with status {status}")),
        Err(error) => error,
    };
    is(&error, Kind::Refused, None)?;
    check(error.is_status(status), || {
        format!("status {:?}, not {status}", error.status())
    })
}

/// A Mismatch.
fn mismatch<T, E>(result: Result<T, concord::Error<E>>) -> Result<(), String> {
    match result {
        Ok(_) => Err("a value, not a Mismatch".to_owned()),
        Err(error) => is(&error, Kind::Mismatch, None),
    }
}

/// A value that holds.
fn holds<T, E>(
    result: Result<T, concord::Error<E>>,
    what: &str,
    test: impl FnOnce(T) -> bool,
) -> Result<(), String> {
    match result {
        Ok(value) => check(test(value), || format!("the value is not {what}")),
        Err(error) => Err(format!(
            "expected {what}, got {}",
            outcome::<(), E>(&Err(error))
        )),
    }
}

/// What a stream gave, its error as any `concord::Error` (D273).
fn piece<T>(result: Result<T, concord::read::StreamError>) -> Result<T, concord::Error> {
    result.map_err(concord::Error::from)
}

fn europe(script: &Script) -> tour::riot::Account {
    riot(script).account(tour::Europe)
}

fn match_get(script: &Script) -> tour::riot::matches::Get {
    riot(script)
        .matches(tour::Europe)
        .get(tour::MatchId::from("EUW1_1"))
}

fn exists(script: &Script) -> tour::files::Exists {
    files(script).exists(tour::DocId::from("readme"))
}

fn download(script: &Script) -> tour::files::Download {
    files(script).download(tour::FileId::from("f"))
}

fn a_2xx_with_an_arm() -> Vec<Cell> {
    vec![
        cell(WITH_ARM, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(200);
            holds(exists(&script).await, "true", |found| found)
        }),
        cell(WITH_ARM, RAW, async {
            let script = Script::new();
            script.answer(200).header("x-a", "1");
            holds(exists(&script).send_raw().await, "the response", |r| {
                r.status() == 200 && r.headers()["x-a"] == "1"
            })
        }),
        cell(WITH_ARM, READ, async {
            let call = tour::Files::new("key").exists(tour::DocId::from("readme"));
            holds(
                call.read_response(response(200, &[], "")).await,
                "true",
                |found| found,
            )
        }),
        cell(WITH_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            script.answer(200);
            holds(
                exists(&script).send_if_none_match("\"v1\"").await,
                "Some(true)",
                |answer| answer.into_value() == Some(true),
            )
        }),
    ]
}

fn a_2xx_without_an_arm() -> Vec<Cell> {
    vec![
        cell(WITHOUT_ARM, NO_ARROW, async {
            let script = Script::new();
            script.answer(200).body("anything at all");
            holds(tasks(&script).delete(7).await, "()", |()| true)
        }),
        cell(WITHOUT_ARM, T, async {
            let script = Script::new();
            json(&script, 200, TASK);
            holds(tasks(&script).get(7).await, "the task", |task| task.id == 7)
        }),
        cell(WITHOUT_ARM, OPTION, async {
            let script = Script::new();
            json(&script, 200, ACCOUNT);
            holds(
                europe(&script).by_riot_id("Faker", "KR1").await,
                "Some(account)",
                |account| account.is_some_and(|account| account.game_name == "Faker"),
            )?;
            // A body that is `null` is not a `None`.
            let script = Script::new();
            json(&script, 200, "null");
            mismatch(europe(&script).by_riot_id("Faker", "KR1").await)
        }),
        cell(WITHOUT_ARM, STREAM, async {
            let script = Script::new();
            script.answer(200).body("ab");
            let mut stream = match download(&script).await {
                Ok(stream) => stream,
                Err(error) => return Err(format!("{error}")),
            };
            holds(piece(stream.try_next().await), "the bytes", |chunk| {
                chunk.as_deref() == Some(&b"ab"[..])
            })
        }),
        cell(WITHOUT_ARM, RAW, async {
            let script = Script::new();
            script.answer(200).body("not json at all");
            let response = match tasks(&script).get(7).send_raw().await {
                Ok(response) => response,
                Err(error) => return Err(format!("{error}")),
            };
            check(response.status() == 200, || "not the status".to_owned())?;
            let body = response.bytes().await.map_err(|e| e.to_string())?;
            check(&body[..] == b"not json at all", || {
                "nothing was read: the body is the caller's".to_owned()
            })
        }),
        cell(WITHOUT_ARM, READ, async {
            let call = tour::Tasks::new().get(7);
            holds(
                call.read_response(response(200, &[("content-type", "application/json")], TASK))
                    .await,
                "the task",
                |task| task.id == 7,
            )
        }),
        cell(WITHOUT_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            json(&script, 200, TASK);
            holds(
                tasks(&script).get(7).send_if_none_match("\"v1\"").await,
                "Some(task)",
                |answer| answer.status() == 200 && answer.into_value().is_some_and(|t| t.id == 7),
            )
        }),
    ]
}

fn a_204() -> Vec<Cell> {
    vec![
        cell(NO_CONTENT, NO_ARROW, async {
            let script = Script::new();
            script.answer(204);
            holds(tasks(&script).delete(7).await, "()", |()| true)
        }),
        cell(NO_CONTENT, T, async {
            // `Json` gives a Mismatch for a body of no bytes; `Text` an empty text.
            let script = Script::new();
            script.answer(204);
            mismatch(tasks(&script).get(7).await)?;
            let script = Script::new();
            script.answer(204);
            holds(
                files(&script).readme("repo").await,
                "an empty text",
                |text| text.is_empty(),
            )
        }),
        cell(NO_CONTENT, OPTION, async {
            let script = Script::new();
            script.answer(204);
            mismatch(europe(&script).by_riot_id("Faker", "KR1").await)
        }),
        cell(NO_CONTENT, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(204);
            holds(exists(&script).await, "true (the arm 2xx)", |found| found)
        }),
        cell(NO_CONTENT, STREAM, async {
            let script = Script::new();
            script.answer(204);
            let mut stream = match download(&script).await {
                Ok(stream) => stream,
                Err(error) => return Err(format!("{error}")),
            };
            holds(piece(stream.try_next().await), "the end", |chunk| {
                chunk.is_none()
            })
        }),
        cell(NO_CONTENT, RAW, async {
            let script = Script::new();
            script.answer(204);
            holds(
                tasks(&script).get(7).send_raw().await,
                "the response",
                |r| r.status() == 204,
            )
        }),
        cell(NO_CONTENT, READ, async {
            let call = tour::Tasks::new().get(7);
            mismatch(call.read_response(response(204, &[], "")).await)
        }),
        cell(NO_CONTENT, IF_NONE_MATCH, async {
            let script = Script::new();
            script.answer(204);
            mismatch(tasks(&script).get(7).send_if_none_match("\"v1\"").await)
        }),
    ]
}

fn a_3xx() -> Vec<Cell> {
    vec![
        cell(REDIRECT, NO_ARROW, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(tasks(&script).delete(7).await, 302)
        }),
        cell(REDIRECT, T, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(tasks(&script).get(7).await, 302)
        }),
        cell(REDIRECT, OPTION, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(europe(&script).by_riot_id("Faker", "KR1").await, 302)
        }),
        cell(REDIRECT, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(exists(&script).await, 302)
        }),
        cell(REDIRECT, STREAM, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(download(&script).await, 302)
        }),
        cell(REDIRECT, RAW, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            holds(
                tasks(&script).get(7).send_raw().await,
                "the response",
                |r| r.status() == 302,
            )
        }),
        cell(REDIRECT, READ, async {
            let call = tour::Tasks::new().get(7);
            let result = call
                .read_response(response(302, &[("location", "/elsewhere")], ""))
                .await;
            let tries = result.as_ref().err().map(concord::Error::tries);
            refused(result, 302)?;
            check(tries == Some(0), || format!("tries {tries:?}, not 0"))
        }),
        cell(REDIRECT, IF_NONE_MATCH, async {
            let script = Script::new();
            script
                .answer(304)
                .header("etag", "\"v2\"")
                .header("cache-control", "max-age=60");
            holds(
                tasks(&script).get(7).send_if_none_match("\"v1\"").await,
                "None, with the headers",
                |answer| {
                    answer.status() == 304
                        && answer.headers().get("etag") == Some("\"v2\"")
                        && answer.headers().get("cache-control") == Some("max-age=60")
                        && answer.into_value().is_none()
                },
            )?;
            let script = Script::new();
            script.answer(302);
            refused(
                tasks(&script).get(7).send_if_none_match("\"v1\"").await,
                302,
            )
        }),
    ]
}

fn a_4xx_or_5xx_with_an_arm() -> Vec<Cell> {
    vec![
        cell(ERROR_WITH_ARM, T, async {
            // `4xx { body } => Err::<_, RiotProblem>(body)?`: a Refused error with the problem.
            let script = Script::new();
            json(&script, 404, PROBLEM);
            let result = match_get(&script).await;
            let message = result
                .as_ref()
                .err()
                .and_then(|error| error.problem())
                .map(|problem| problem.status.message.clone());
            refused(result, 404)?;
            check(message.as_deref() == Some("slow down"), || {
                format!("the problem is {message:?}")
            })
        }),
        cell(ERROR_WITH_ARM, OPTION, async {
            let script = Script::new();
            script.answer(404);
            holds(
                europe(&script).by_riot_id("Faker", "KR1").await,
                "None",
                |account| account.is_none(),
            )
        }),
        cell(ERROR_WITH_ARM, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(404);
            holds(exists(&script).await, "false", |found| !found)
        }),
        cell(ERROR_WITH_ARM, RAW, async {
            let script = Script::new();
            script.answer(404);
            holds(exists(&script).send_raw().await, "the response", |r| {
                r.status() == 404
            })
        }),
        cell(ERROR_WITH_ARM, READ, async {
            let call = tour::Riot::new("key")
                .account(tour::Europe)
                .by_riot_id("Faker", "KR1");
            holds(
                call.read_response(response(404, &[], "")).await,
                "None",
                |account| account.is_none(),
            )
        }),
        cell(ERROR_WITH_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            script.answer(404);
            holds(
                exists(&script).send_if_none_match("\"v1\"").await,
                "Some(false)",
                |answer| answer.into_value() == Some(false),
            )
        }),
    ]
}

fn a_4xx_or_5xx_without_an_arm() -> Vec<Cell> {
    vec![
        cell(ERROR_WITHOUT_ARM, NO_ARROW, async {
            let script = Script::new();
            script.answer(404).body("gone");
            refused(tasks(&script).delete(7).await, 404)
        }),
        cell(ERROR_WITHOUT_ARM, T, async {
            let script = Script::new();
            script.answer(404).header("x-a", "1").body("gone");
            let result = tasks(&script).get(7).await;
            if let Err(error) = &result {
                check(
                    error.headers().and_then(|h| h.get("x-a")) == Some("1")
                        && error.body() == Some(&b"gone"[..])
                        && error.kept() == Some(Kept::Complete)
                        && error.problem().is_none(),
                    || "the status, the headers and the body are kept".to_owned(),
                )?;
            }
            refused(result, 404)
        }),
        cell(ERROR_WITHOUT_ARM, OPTION, async {
            // A 5xx: no arm takes it; the problem is tried on the complete body.
            let script = Script::new();
            json(&script, 503, PROBLEM);
            let result = europe(&script).by_riot_id("Faker", "KR1").await;
            let has_problem = result
                .as_ref()
                .err()
                .is_some_and(|error| error.problem().is_some());
            refused(result, 503)?;
            check(has_problem, || "problem() is tried".to_owned())
        }),
        cell(ERROR_WITHOUT_ARM, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(500);
            refused(exists(&script).await, 500)
        }),
        cell(ERROR_WITHOUT_ARM, STREAM, async {
            let script = Script::new();
            script.answer(404).body("gone");
            let result = download(&script).await;
            let body = result
                .as_ref()
                .err()
                .and_then(|error| error.body().map(<[u8]>::to_vec));
            refused(result, 404)?;
            check(body.as_deref() == Some(&b"gone"[..]), || {
                "a refusal is read like any refusal".to_owned()
            })
        }),
        cell(ERROR_WITHOUT_ARM, RAW, async {
            let script = Script::new();
            script.answer(404).body("gone");
            let response = match tasks(&script).get(7).send_raw().await {
                Ok(response) => response,
                Err(error) => return Err(format!("{error}")),
            };
            check(response.status() == 404, || "not the status".to_owned())?;
            let body = response.bytes().await.map_err(|e| e.to_string())?;
            check(&body[..] == b"gone", || "nothing was read".to_owned())
        }),
        cell(ERROR_WITHOUT_ARM, READ, async {
            let call = tour::Tasks::new().get(7);
            let result = call.read_response(response(404, &[], "gone")).await;
            let tries = result.as_ref().err().map(concord::Error::tries);
            refused(result, 404)?;
            check(tries == Some(0), || format!("tries {tries:?}, not 0"))
        }),
        cell(ERROR_WITHOUT_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            script.answer(404);
            refused(
                tasks(&script).get(7).send_if_none_match("\"v1\"").await,
                404,
            )
        }),
    ]
}

#[tokio::test(start_paused = true)]
async fn c5_2_table_5_the_answer() {
    // D273: "`ByteStream::try_next()` gives a `concord::read::StreamError`, which becomes any `concord::Error<P>`".
    let mut cells = Vec::new();
    cells.extend(a_2xx_with_an_arm());
    cells.extend(a_2xx_without_an_arm());
    cells.extend(a_204());
    cells.extend(a_3xx());
    cells.extend(a_4xx_or_5xx_with_an_arm());
    cells.extend(a_4xx_or_5xx_without_an_arm());
    run_cells("table 5", cells).await;
}
