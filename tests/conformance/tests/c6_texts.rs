//! C6.2: the texts of docs/reference/messages.md section 2 that needed an endpoint that the four first
//! clients of `tests/tour` do not have, on `Shapes`: texts 2, 4, 5, 7, 13 and 25 of the table,
//! failed entry expressions and transport failures. Each is compared
//! as a whole line.
//!
//! A text of the request is seen through `request()`, and a text of the answer through
//! `read_response`, both of which run now; the same texts through a send wait for it.

mod common;

use common::engine::*;
use concord::error::{Class, Kind};
use concord_testkit::Script;
use tour::{Filter, Item, ShapesProblem};

fn titled(title: &str) -> Item {
    Item {
        id: 1,
        title: title.to_owned(),
    }
}

fn filter() -> Filter {
    Filter {
        owner: "ann".into(),
        open: true,
    }
}

/// The line of an error, its kind checked.
#[track_caller]
fn line<T>(result: Result<T, concord::Error<ShapesProblem>>, kind: Kind) -> String {
    let error = failure(result);
    assert_eq!(error.kind(), kind, "{error}");
    error.to_string()
}

#[test]
fn c6_2_the_texts_of_a_request_that_needed_an_endpoint() {
    let shapes = tour::Shapes::new();
    let build = Kind::Build;
    // 2. An invalid host hole: a `String` input.
    for tenant in ["", "a.b", "-a", "é"] {
        assert_eq!(
            line(shapes.hosted(tenant).request(), build),
            "build error: input tenant is not one label of a host, in shapes.hosted",
            "{tenant:?}"
        );
    }
    // 4. A shape that cannot be written at that place: a struct in a query.
    assert_eq!(
        line(shapes.search(1.5).filter(filter()).request(), build),
        "build error: input filter cannot be written in a query, in shapes.search"
    );
    // 5. A number that is not finite.
    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            line(shapes.search(ratio).request(), build),
            "build error: input ratio is not a finite number, in shapes.search"
        );
    }
    // 7. A body that could not be written: the expression gave an `Err`.
    let error = failure(shapes.measured(titled(""), 20).request());
    assert_eq!(
        error.to_string(),
        "build error: the body could not be written, in shapes.measured"
    );
    let source = std::error::Error::source(&error).expect("the author's error");
    assert!(source.is::<tour::EmptyTitle>());
    assert!(!error.to_string().contains("title"));
    // 25. A written content-length that is not the length of the body in memory.
    for length in [0, 19, 21] {
        assert_eq!(
            line(shapes.measured(titled("t"), length).request(), build),
            "build error: the content-length is not the length of the body, in shapes.measured"
        );
    }
    // An entry whose expression fails: the key, and the author's error as the source.
    let error = failure(shapes.parsed("twelve").request());
    assert_eq!(
        error.to_string(),
        "build error: input n could not be written, in shapes.parsed"
    );
    let source = std::error::Error::source(&error).expect("the author's error");
    assert!(source.is::<std::num::ParseIntError>());
    assert!(!error.to_string().contains("twelve"));
}

#[tokio::test]
async fn c6_2_the_texts_of_an_answer_that_needed_an_endpoint() {
    let mismatch = Kind::Mismatch;
    // 13. A reader that holds the server to its label: `Json.strict()`.
    let strict = tour::Shapes::new().strict();
    for label in [&[("content-type", "text/plain")][..], &[]] {
        assert_eq!(
            line(strict.read_response(response(200, label, ITEM)).await, mismatch),
            "mismatch: status 200, the content type is not the one the reader expects, in shapes.strict"
        );
    }
    let json_label = [("content-type", "application/json")];
    assert_eq!(
        success(strict.read_response(response(200, &json_label, ITEM)).await),
        item()
    );
    // A Mismatch at the root, with the reader's label.
    let counts = tour::Shapes::new().counts();
    assert_eq!(
        line(
            counts
                .read_response(response(200, &json_label, "not json"))
                .await,
            mismatch
        ),
        "mismatch: status 200, the body is not what the type says, in shapes.counts"
    );
    // `require(…)` and `mismatch(…)` in an arm.
    let create = tour::Shapes::new().create(item());
    assert_eq!(
        line(create.read_response(response(201, &[], "")).await, mismatch),
        "mismatch: status 201, the status arm refused the answer, in shapes.create"
    );
    assert_eq!(
        line(create.read_response(response(301, &[], "")).await, mismatch),
        "mismatch: status 301, the status arm refused the answer, in shapes.create"
    );
}

#[tokio::test(start_paused = true)]
async fn c6_2_the_texts_through_a_send_that_needed_an_endpoint() {
    let script = Script::new();
    let shapes = shapes(&script);
    let build = Kind::Build;
    assert_eq!(
        line(shapes.hosted("a.b").await, build),
        "build error: input tenant is not one label of a host, in shapes.hosted"
    );
    assert_eq!(
        line(shapes.search(1.5).filter(filter()).await, build),
        "build error: input filter cannot be written in a query, in shapes.search"
    );
    assert_eq!(
        line(shapes.search(f64::NAN).await, build),
        "build error: input ratio is not a finite number, in shapes.search"
    );
    assert_eq!(
        line(shapes.measured(titled(""), 20).await, build),
        "build error: the body could not be written, in shapes.measured"
    );
    assert_eq!(
        line(shapes.measured(titled("t"), 3).await, build),
        "build error: the content-length is not the length of the body, in shapes.measured"
    );
    assert_eq!(
        line(shapes.parsed("twelve").await, build),
        "build error: input n could not be written, in shapes.parsed"
    );
    assert!(script.requests().is_empty(), "a Build error sends nothing");

    // 13, through a send.
    script
        .answer(200)
        .header("content-type", "text/plain")
        .body(ITEM);
    assert_eq!(
        line(shapes.strict().await, Kind::Mismatch),
        "mismatch: status 200, the content type is not the one the reader expects, in shapes.strict"
    );
}

#[tokio::test(start_paused = true)]
async fn c6_2_the_texts_of_a_transport_that_added() {
    // class Other; a timeout that the transport reported; a body that broke with no
    // head known.
    for (class, text) in [
        (
            Class::Other,
            "transport error: the transport failed, in shapes.again",
        ),
        (
            Class::Timeout,
            "transport error: a timeout was reported by the transport, in shapes.again",
        ),
        (
            Class::Body,
            "transport error: the body broke, in shapes.again",
        ),
    ] {
        let shapes = tour::Shapes::new().with_transport(FailsWith(class));
        let error = failure(shapes.again(1).await);
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(error.class(), Some(class));
        assert_eq!(error.to_string(), text);
        // The caller's own text of the source is not printed.
        assert!(!error.to_string().contains(CALLER_TEXT));
    }
}
