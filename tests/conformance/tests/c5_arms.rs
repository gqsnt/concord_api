//! C5: the answer, on the shapes that the four first clients of `tests/tour` do not have and
//! `Shapes` has: `-> Result<T, E>`, a `HEAD` with no arrow, a `HEAD` whose arrow has no arm for
//! every 2xx, an arm from above that binds `body`, arms that bind `headers` and `status`,
//! `require` and `mismatch` in an arm, `?` on the problem in a 2xx arm, a key of a map in the
//! place of a Mismatch, and a reader's message over 1 KiB (docs/reference/contracts.md C5; docs/reference/cases.md
//! table 5; docs/reference/messages.md section 2).
//!
//! What an answer gives is read with `read_response`, which runs the arms, the reader and the
//! problem of the endpoint as a send does (C3.10). The cells of table 5 that need `Shapes` are
//! at the end of this file; the others are in `c5_table5.rs`.

mod common;

use common::engine::*;
use concord::error::Kind;
use concord_testkit::Script;
use tour::{Conflict, Envelope, Receipt, ShapesProblem};

const CONFLICT: &str = r#"{"message":"held"}"#;
const PROBLEM_ENVELOPE: &str = r#"{"problem":{"code":"held","message":"the item is held"}}"#;

fn json_headers() -> [(&'static str, &'static str); 1] {
    [("content-type", "application/json")]
}

/// The error of a result, its kind checked.
#[track_caller]
fn error_of<T>(
    result: Result<T, concord::Error<ShapesProblem>>,
    kind: Kind,
) -> concord::Error<ShapesProblem> {
    let error = failure(result);
    assert_eq!(error.kind(), kind, "{error}");
    error
}

// -> Result<T, E> ----------------------------------------------------------------------

#[tokio::test]
async fn c5_2_a_result_arrow_gives_ok_of_the_body_and_what_an_arm_gives() {
    let call = tour::Shapes::new().outcome(1);
    // A 2xx without an arm: the body, read as `T`, given as `Ok`.
    let value = success(
        call.read_response(response(200, &json_headers(), ITEM))
            .await,
    );
    assert_eq!(value, Ok(item()));
    // `409 { body } => Err(body)`.
    let value = success(
        call.read_response(response(409, &json_headers(), CONFLICT))
            .await,
    );
    assert_eq!(
        value,
        Err(Conflict {
            message: "held".into()
        })
    );
    // A 204 without an arm: read as `T`; `Json` gives a Mismatch for no bytes.
    error_of(
        call.read_response(response(204, &[], "")).await,
        Kind::Mismatch,
    );
    // A 3xx, and a 4xx with no arm: a Refused error.
    let error = error_of(
        call.read_response(response(302, &[("location", "/x")], ""))
            .await,
        Kind::Refused,
    );
    assert!(error.is_status(302));
    let error = error_of(
        call.read_response(response(404, &[], "gone")).await,
        Kind::Refused,
    );
    assert!(error.is_status(404));
    assert_eq!(error.tries(), 0);
}

// HEAD ---------------------------------------------------------------------------------

#[tokio::test]
async fn c5_2_a_head_with_no_arrow_gives_the_unit_for_a_2xx_and_refuses_the_others() {
    let call = tour::Shapes::new().ping();
    for status in [200, 204] {
        let () = success(call.read_response(response(status, &[], "")).await);
    }
    for status in [302, 404, 500] {
        let error = error_of(
            call.read_response(response(status, &[], "")).await,
            Kind::Refused,
        );
        assert!(error.is_status(status));
    }
}

#[tokio::test]
async fn c5_2_a_head_whose_arrow_has_no_arm_for_a_2xx_gives_a_mismatch_for_it() {
    // `HEAD present(id) -> bool { status { 204 => true, 404 => false } }`
    let call = tour::Shapes::new().present(1);
    assert!(success(call.read_response(response(204, &[], "")).await));
    assert!(!success(call.read_response(response(404, &[], "")).await));
    let error = error_of(
        call.read_response(response(200, &[], "")).await,
        Kind::Mismatch,
    );
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, no arm takes this status, in shapes.present"
    );
    let error = error_of(
        call.read_response(response(500, &[], "")).await,
        Kind::Refused,
    );
    assert!(error.is_status(500));
}

// An arm from above that binds `body` --------------------------------------------------

#[tokio::test]
async fn c5_3_an_arm_from_above_that_binds_body_reads_it_with_the_reader_in_reach() {
    let wrapped = tour::Shapes::new().wrapped();
    // `2xx { body } => unpack(body)?` on the scope: the body is read as an `Envelope<Item>`.
    let value = success(
        wrapped
            .get(1)
            .read_response(response(
                200,
                &json_headers(),
                r#"{"data":{"id":1,"title":"one"}}"#,
            ))
            .await,
    );
    assert_eq!(value, item());
    // `remove 2xx`: the body, read into the type after the arrow.
    let value = success(
        wrapped
            .raw(1)
            .read_response(response(
                200,
                &json_headers(),
                r#"{"data":{"id":1,"title":"one"}}"#,
            ))
            .await,
    );
    assert_eq!(value, Envelope::Data(item()));
    // `override 2xx { body } => Some(unpack(body)?), 404 => None`: an arm of its own.
    let find = wrapped.find(1);
    let value = success(
        find.read_response(response(
            200,
            &json_headers(),
            r#"{"data":{"id":1,"title":"one"}}"#,
        ))
        .await,
    );
    assert_eq!(value, Some(item()));
    assert_eq!(
        success(find.read_response(response(404, &[], "")).await),
        None
    );
    // A body that does not read into the type of the arm: a Mismatch.
    error_of(
        wrapped
            .get(1)
            .read_response(response(200, &json_headers(), ITEM))
            .await,
        Kind::Mismatch,
    );
}

#[tokio::test]
async fn c5_8_the_question_mark_on_the_problem_in_a_2xx_arm_is_a_refused_error_with_it() {
    let call = tour::Shapes::new().wrapped().get(1);
    let error = error_of(
        call.read_response(response(200, &json_headers(), PROBLEM_ENVELOPE))
            .await,
        Kind::Refused,
    );
    assert!(error.is_status(200));
    assert_eq!(
        error.problem(),
        Some(&ShapesProblem {
            code: "held".into(),
            message: "the item is held".into(),
        })
    );
    assert_eq!(
        error.to_string(),
        "refused: status 200, in shapes.wrapped.get"
    );
}

// Arms that bind the head, and arms that refuse -----------------------------------------

#[tokio::test]
async fn c5_5_an_arm_may_bind_the_headers_and_the_status() {
    let call = tour::Shapes::new().create(item());
    // `201 { headers, status } => Receipt { … headers.require("location")? … }`
    let value = success(
        call.read_response(response(201, &[("location", "/items/1")], ""))
            .await,
    );
    assert_eq!(
        value,
        Receipt {
            status: 201,
            location: Some("/items/1".into()),
        }
    );
    // `202 { status: code } => Receipt { status: code.as_u16(), location: None }`
    let value = success(call.read_response(response(202, &[], "")).await);
    assert_eq!(
        value,
        Receipt {
            status: 202,
            location: None,
        }
    );
}

#[tokio::test]
async fn c5_8_require_or_mismatch_in_an_arm_ends_the_call_with_a_mismatch() {
    let call = tour::Shapes::new().create(item());
    // `headers.require("location")?` with no `location`.
    let error = error_of(
        call.read_response(response(201, &[], "")).await,
        Kind::Mismatch,
    );
    assert!(error.is_status(201));
    assert_eq!(
        error.to_string(),
        "mismatch: status 201, the status arm refused the answer, in shapes.create"
    );
    // `3xx => Err(concord::mismatch("a redirect is not a receipt"))?`
    let error = error_of(
        call.read_response(response(303, &[("location", "/elsewhere")], ""))
            .await,
        Kind::Mismatch,
    );
    assert_eq!(
        error.to_string(),
        "mismatch: status 303, the status arm refused the answer, in shapes.create"
    );
    // The words of the author are not printed in the line.
    assert!(!error.to_string().contains("redirect"));
}

// The place of a Mismatch, and the message of a reader -----------------------------------

#[tokio::test]
async fn c5_11_a_key_of_a_map_prints_as_key_in_the_place() {
    let call = tour::Shapes::new().counts();
    let body = r#"{"counts":{"secret-name":[{"kind":"a","n":1},{"kind":"b","n":"two"}]}}"#;
    let error = error_of(
        call.read_response(response(200, &json_headers(), body))
            .await,
        Kind::Mismatch,
    );
    let line = error.to_string();
    assert_eq!(
        line,
        "mismatch: status 200, counts[key][1].n is not what the type says, in shapes.counts"
    );
    assert!(!line.contains("secret-name"));
    // And a map that reads.
    let body = r#"{"counts":{"x":[{"kind":"a","n":1}]}}"#;
    let tally = success(
        call.read_response(response(200, &json_headers(), body))
            .await,
    );
    assert_eq!(tally.counts["x"][0].n, 1);
}

#[tokio::test]
async fn c5_6_a_reader_message_is_kept_up_to_1_kib_behind_details() {
    let call = tour::Shapes::new().long_message();
    let error = error_of(
        call.read_response(response(200, &json_headers(), ITEM))
            .await,
        Kind::Mismatch,
    );
    let message = tour::Chatty::message();
    assert!(message.len() > concord::KIB);
    // Not in the line: no text that the code gave.
    assert!(!error.to_string().contains("this reader reads"));
    // Behind `details()`: its first part, at most 1 KiB of it.
    let details = error.details().to_string();
    assert!(details.contains(&message[..1000]), "{details}");
    assert!(!details.contains(&message[..concord::KIB + 1]), "{details}");
}

// Table 5, through each way of sending ------------------------------------------------

const RESULT: &str = "-> Result<T, E>";
const HEAD_NO_ARROW: &str = "HEAD, no arrow";
const HEAD_ARROW: &str = "HEAD with an arrow";
const T: &str = "-> T";
const OPTION: &str = "-> Option<T>";
const RAW: &str = "send_raw()";
const IF_NONE_MATCH: &str = "send_if_none_match(etag)";

const WITH_ARM: &str = "A 2xx with an arm";
const READ: &str = "read_response";
const FROM_ABOVE: &str = "A 2xx with an arm from above that binds body";
const WITHOUT_ARM: &str = "A 2xx without an arm";
const NO_CONTENT: &str = "A 204";
const REDIRECT: &str = "A 3xx";
const ERROR_WITH_ARM: &str = "A 4xx or 5xx with an arm";
const ERROR_WITHOUT_ARM: &str = "A 4xx or 5xx without an arm";

/// A Refused error with this status.
fn refused<T>(result: Result<T, concord::Error<ShapesProblem>>, status: u16) -> Result<(), String> {
    let error = match result {
        Ok(_) => return Err(format!("a value, not a Refused error with status {status}")),
        Err(error) => error,
    };
    is(&error, Kind::Refused, None)?;
    check(error.is_status(status), || {
        format!("status {:?}, not {status}", error.status())
    })
}

/// A value that holds.
fn holds<T>(
    result: Result<T, concord::Error<ShapesProblem>>,
    what: &str,
    test: impl FnOnce(T) -> bool,
) -> Result<(), String> {
    match result {
        Ok(value) => check(test(value), || format!("the value is not {what}")),
        Err(error) => Err(format!("expected {what}, got {error}")),
    }
}

fn wrapped_data(script: &Script) {
    json(script, 200, r#"{"data":{"id":1,"title":"one"}}"#);
}

fn from_above() -> Vec<Cell> {
    vec![
        cell(FROM_ABOVE, T, async {
            let script = Script::new();
            wrapped_data(&script);
            holds(shapes(&script).wrapped().get(1).await, "the item", |v| {
                v == item()
            })
        }),
        cell(FROM_ABOVE, OPTION, async {
            // The endpoint writes the arm itself (`override 2xx { body } => Some(…)`).
            let script = Script::new();
            wrapped_data(&script);
            holds(shapes(&script).wrapped().find(1).await, "Some(item)", |v| {
                v == Some(item())
            })
        }),
        cell(FROM_ABOVE, RAW, async {
            let script = Script::new();
            script.answer(200).body("not an envelope");
            let response = match shapes(&script).wrapped().get(1).send_raw().await {
                Ok(response) => response,
                Err(error) => return Err(format!("{error}")),
            };
            let body = response.bytes().await.map_err(|e| e.to_string())?;
            check(&body[..] == b"not an envelope", || {
                "nothing was checked".to_owned()
            })
        }),
        cell(FROM_ABOVE, IF_NONE_MATCH, async {
            let script = Script::new();
            wrapped_data(&script);
            holds(
                shapes(&script)
                    .wrapped()
                    .get(1)
                    .send_if_none_match("\"v1\"")
                    .await,
                "Some(item)",
                |answer| answer.into_value() == Some(item()),
            )
        }),
    ]
}

fn with_arm() -> Vec<Cell> {
    vec![
        cell(WITH_ARM, T, async {
            // `shapes.create`: `201 { headers, status } => Receipt { … }`.
            let script = Script::new();
            script.answer(201).header("location", "/items/1");
            holds(
                shapes(&script).create(item()).await,
                "the receipt of the arm",
                |receipt| {
                    receipt
                        == Receipt {
                            status: 201,
                            location: Some("/items/1".to_owned()),
                        }
                },
            )
        }),
        cell(WITH_ARM, OPTION, async {
            // `shapes.wrapped.find`: `override 2xx { body } => Some(unpack(body)?)`.
            let script = Script::new();
            wrapped_data(&script);
            holds(shapes(&script).wrapped().find(1).await, "Some(item)", |v| {
                v == Some(item())
            })
        }),
    ]
}

fn from_above_read_response() -> Vec<Cell> {
    vec![cell(FROM_ABOVE, READ, async {
        // `shapes.wrapped.get` under `2xx { body } => unpack(body)?`: as a send gives (C3.10).
        let call = tour::Shapes::new().wrapped().get(1);
        holds(
            call.read_response(response(
                200,
                &json_headers(),
                r#"{"data":{"id":1,"title":"one"}}"#,
            ))
            .await,
            "the item",
            |v| v == item(),
        )
    })]
}

fn without_arm() -> Vec<Cell> {
    vec![
        cell(WITHOUT_ARM, RESULT, async {
            let script = Script::new();
            json(&script, 200, ITEM);
            holds(shapes(&script).outcome(1).await, "Ok(item)", |v| {
                v == Ok(item())
            })
        }),
        cell(WITHOUT_ARM, HEAD_NO_ARROW, async {
            let script = Script::new();
            script.answer(200);
            holds(shapes(&script).ping().await, "()", |()| true)
        }),
        cell(WITHOUT_ARM, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(200);
            let result = shapes(&script).present(1).await;
            let line = result.as_ref().err().map(ToString::to_string);
            match result {
                Ok(_) => Err("a value, not a Mismatch".to_owned()),
                Err(error) => is(&error, Kind::Mismatch, None),
            }?;
            check(
                line.as_deref()
                    == Some("mismatch: status 200, no arm takes this status, in shapes.present"),
                || format!("{line:?}"),
            )
        }),
        cell(WITHOUT_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            json(&script, 200, ITEM);
            holds(
                shapes(&script)
                    .outcome(1)
                    .send_if_none_match("\"v1\"")
                    .await,
                "Some(Ok(item))",
                |answer| answer.into_value() == Some(Ok(item())),
            )
        }),
    ]
}

fn no_content() -> Vec<Cell> {
    vec![
        cell(NO_CONTENT, RESULT, async {
            let script = Script::new();
            script.answer(204);
            match shapes(&script).outcome(1).await {
                Ok(_) => Err("a value, not a Mismatch".to_owned()),
                Err(error) => is(&error, Kind::Mismatch, None),
            }
        }),
        cell(NO_CONTENT, HEAD_NO_ARROW, async {
            let script = Script::new();
            script.answer(204);
            holds(shapes(&script).ping().await, "()", |()| true)
        }),
        cell(NO_CONTENT, HEAD_ARROW, async {
            let script = Script::new();
            script.answer(204);
            holds(shapes(&script).present(1).await, "true", |found| found)
        }),
    ]
}

fn redirect() -> Vec<Cell> {
    vec![
        cell(REDIRECT, RESULT, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(shapes(&script).outcome(1).await, 302)
        }),
        cell(REDIRECT, HEAD_NO_ARROW, async {
            let script = Script::new();
            script.answer(302).header("location", "/elsewhere");
            refused(shapes(&script).ping().await, 302)
        }),
        cell(REDIRECT, IF_NONE_MATCH, async {
            let script = Script::new();
            script.answer(304).header("etag", "\"v2\"");
            holds(
                shapes(&script).ping().send_if_none_match("\"v1\"").await,
                "None, with the headers",
                |answer| {
                    answer.status() == 304
                        && answer.headers().get("etag") == Some("\"v2\"")
                        && answer.into_value().is_none()
                },
            )
        }),
    ]
}

fn errors() -> Vec<Cell> {
    vec![
        cell(ERROR_WITH_ARM, RESULT, async {
            let script = Script::new();
            json(&script, 409, CONFLICT);
            holds(shapes(&script).outcome(1).await, "Err(conflict)", |v| {
                v == Err(Conflict {
                    message: "held".into(),
                })
            })
        }),
        cell(ERROR_WITH_ARM, IF_NONE_MATCH, async {
            let script = Script::new();
            json(&script, 409, CONFLICT);
            holds(
                shapes(&script)
                    .outcome(1)
                    .send_if_none_match("\"v1\"")
                    .await,
                "Some(Err(conflict))",
                |answer| matches!(answer.into_value(), Some(Err(_))),
            )
        }),
        cell(ERROR_WITHOUT_ARM, RESULT, async {
            let script = Script::new();
            script.answer(404).body("gone");
            refused(shapes(&script).outcome(1).await, 404)
        }),
        cell(ERROR_WITHOUT_ARM, HEAD_NO_ARROW, async {
            let script = Script::new();
            script.answer(500);
            refused(shapes(&script).ping().await, 500)
        }),
        cell(ERROR_WITHOUT_ARM, RAW, async {
            let script = Script::new();
            script.answer(500);
            holds(
                shapes(&script).ping().send_raw().await,
                "the response",
                |r| r.status() == 500,
            )
        }),
    ]
}

const NO_ARROW: &str = "No arrow";
const STREAM: &str = "read Stream";

/// A Mismatch whose line is this one.
fn mismatch_line<T>(
    result: Result<T, concord::Error<ShapesProblem>>,
    line: &str,
) -> Result<(), String> {
    let error = match result {
        Ok(_) => return Err("a value, not a Mismatch".to_owned()),
        Err(error) => error,
    };
    is(&error, Kind::Mismatch, None)?;
    check(error.to_string() == line, || format!("{error}, not {line}"))
}

/// The bytes of a stream, to its end.
async fn all_of(mut stream: concord::ByteStream) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.try_next().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// The ten cells that needed the endpoints added to `Shapes` for them (the engine):
/// `shapes.remove` (no arrow), `shapes.settled` (`-> Result<T, E>`), `shapes.part`
/// (`read Stream`), `shapes.wrapped.touch` and `shapes.wrapped.holds` (a `HEAD` under an arm
/// that binds `body`), `shapes.sealed.file` (`read Stream` under an arm that binds `body`).
fn the_cells_of_the_added_endpoints() -> Vec<Cell> {
    vec![
        cell(WITH_ARM, NO_ARROW, async {
            // `202 => Err(concord::mismatch(…))?`: what the arm gives, not the `()` of a 2xx.
            let script = Script::new();
            script.answer(202);
            mismatch_line(
                shapes(&script).remove(1).await,
                "mismatch: status 202, the status arm refused the answer, in shapes.remove",
            )?;
            let script = Script::new();
            script.answer(200).body("ignored");
            holds(shapes(&script).remove(1).await, "()", |()| true)
        }),
        cell(WITH_ARM, RESULT, async {
            // `202 => Err(Conflict { … })`: what the arm gives, a `Result<T, E>`; no body read.
            let script = Script::new();
            script.answer(202).body("not json");
            holds(shapes(&script).settled(1).await, "Err(pending)", |v| {
                v == Err(Conflict {
                    message: "pending".into(),
                })
            })
        }),
        cell(WITH_ARM, HEAD_NO_ARROW, async {
            // `override 202 => Err(concord::mismatch(…))?` on a `HEAD`.
            let script = Script::new();
            script.answer(202);
            mismatch_line(
                shapes(&script).wrapped().touch(1).await,
                "mismatch: status 202, the status arm refused the answer, in shapes.wrapped.touch",
            )
        }),
        cell(WITH_ARM, STREAM, async {
            // `204 => None`: what the arm gives; the status rule runs on the head.
            let script = Script::new();
            script.answer(204);
            holds(shapes(&script).part(1).await, "None", |v| v.is_none())?;
            // Without the arm, a 2xx hands the body over, unread.
            let script = Script::new();
            script.answer(200).body("ab");
            let stream = match shapes(&script).part(1).await {
                Ok(Some(stream)) => stream,
                Ok(None) => return Err("None for a 200".to_owned()),
                Err(error) => return Err(error.to_string()),
            };
            check(all_of(stream).await? == b"ab", || {
                "not the bytes".to_owned()
            })
        }),
        cell(FROM_ABOVE, HEAD_NO_ARROW, async {
            // Left out: the status is as if the arm were not written. A body that `unpack`
            // could not read is not read.
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .body("not an envelope");
            holds(shapes(&script).wrapped().touch(1).await, "()", |()| true)
        }),
        cell(FROM_ABOVE, HEAD_ARROW, async {
            // Left out: a 200 has no arm left (`override 204 => true` is the endpoint's own).
            let script = Script::new();
            wrapped_data(&script);
            mismatch_line(
                shapes(&script).wrapped().holds(1).await,
                "mismatch: status 200, no arm takes this status, in shapes.wrapped.holds",
            )?;
            let script = Script::new();
            script.answer(204);
            holds(shapes(&script).wrapped().holds(1).await, "true", |v| v)
        }),
        cell(FROM_ABOVE, STREAM, async {
            // `2xx { body, headers } => unseal(body, headers)?`: `body` is the stream that the
            // reader in reach gives, and the arm runs on the head.
            let script = Script::new();
            script.answer(200).header("x-seal", "1").body("ab");
            let stream = match shapes(&script).sealed().file(1).await {
                Ok(stream) => stream,
                Err(error) => return Err(error.to_string()),
            };
            check(all_of(stream).await? == b"ab", || {
                "not the bytes".to_owned()
            })?;
            // `?` on the problem in the arm: a Refused error with the problem (C5.8).
            let script = Script::new();
            script.answer(200).body("ab");
            let result = shapes(&script).sealed().file(1).await;
            let code = result
                .as_ref()
                .err()
                .and_then(|error| error.problem().map(|problem| problem.code.clone()));
            refused(result, 200)?;
            check(code.as_deref() == Some("unsealed"), || {
                format!("problem {code:?}")
            })
        }),
        cell(ERROR_WITH_ARM, NO_ARROW, async {
            // `404 => ()`: what the arm gives, not a Refused error.
            let script = Script::new();
            script.answer(404).body("gone");
            holds(shapes(&script).remove(1).await, "()", |()| true)?;
            let script = Script::new();
            script.answer(410);
            refused(shapes(&script).remove(1).await, 410)
        }),
        cell(ERROR_WITH_ARM, HEAD_NO_ARROW, async {
            let script = Script::new();
            script.answer(404);
            holds(shapes(&script).wrapped().touch(1).await, "()", |()| true)?;
            let script = Script::new();
            script.answer(410);
            refused(shapes(&script).wrapped().touch(1).await, 410)
        }),
        cell(ERROR_WITH_ARM, STREAM, async {
            // `404 => None`, on an endpoint whose 2xx is a stream.
            let script = Script::new();
            script.answer(404).body("gone");
            holds(shapes(&script).part(1).await, "None", |v| v.is_none())?;
            let script = Script::new();
            script.answer(410).body("gone");
            refused(shapes(&script).part(1).await, 410)
        }),
    ]
}

#[tokio::test(start_paused = true)]
async fn c5_2_table_5_the_cells_that_needed_an_endpoint() {
    let mut cells = Vec::new();
    cells.extend(with_arm());
    cells.extend(from_above());
    cells.extend(from_above_read_response());
    cells.extend(without_arm());
    cells.extend(no_content());
    cells.extend(redirect());
    cells.extend(errors());
    cells.extend(the_cells_of_the_added_endpoints());
    run_cells("table 5", cells).await;
}
