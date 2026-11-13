//! The fifth client, `Shapes`: a call to each endpoint compiles with the type it promises,
//! the author's helpers of `data.rs` run, and each request is asserted as the paper builds it.
//!
//! The tests that build a request with `request()` run on the engine of M1-E1.
#![allow(dead_code, unused_variables)]

use concord::error::Kind;
use concord::{reqwest, Body};
use std::future::IntoFuture;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tour::*;

fn owned_send<T: Send + 'static>(value: T) {}
fn cloneable<T: Clone>(value: &T) {}

// Every endpoint, with the type that `.await` gives.
// The references check that arguments and setters take borrowed inputs.
#[allow(clippy::needless_borrows_for_generic_args)]
async fn every_endpoint(
    shapes: &Shapes,
    body: Body,
    trace: Trace,
    item: Item,
    filter: Filter,
) -> shapes::Result<()> {
    let again: Item = shapes.again(1).await?;
    let replaced: Item = shapes.replace(1, &item).await?;
    let ordered: Item = shapes.order(&trace).await?;
    let uploaded: Item = shapes.upload(&body).await?;
    let outcome: Result<Item, Conflict> = shapes.outcome(1).await?;
    let pinged: () = shapes.ping().await?;
    let present: bool = shapes.present(1).await?;
    let receipt: Receipt = shapes.create(&item).await?;
    let hosted: Item = shapes.hosted("acme").await?;
    let found: Vec<Item> = shapes.search(1.5).filter(&filter).filter(None).await?;
    let measured: Item = shapes.measured(&item, 20).await?;
    let parsed: Item = shapes.parsed("12").await?;
    let tally: Tally = shapes.counts().await?;
    let strict: Item = shapes.strict().await?;
    let long: Item = shapes.long_message().await?;
    let got: Item = shapes.wrapped().get(1).await?;
    let raw: Envelope<Item> = shapes.wrapped().raw(1).await?;
    let maybe: Option<Item> = shapes.wrapped().find(1).await?;
    let noted: Item = shapes.noted(&trace).await?;
    let titled: Item = shapes.titled(1, "one").await?;
    let held: Result<Item, Conflict> = shapes.held(1).await?;
    let touched: () = shapes.wrapped().touch(1).await?;
    let holds: bool = shapes.wrapped().holds(&1).await?;
    let removed: () = shapes.remove(1).await?;
    let settled: Result<Item, Conflict> = shapes.settled(1).await?;
    let part: Option<concord::ByteStream> = shapes.part(1).await?;
    let file: concord::ByteStream = shapes.sealed().file(1).await?;
    Ok(())
}

// What every call has, on the shapes that the other clients do not have.
async fn every_surface(shapes: &Shapes, trace: Trace) -> shapes::Result<()> {
    let order = shapes
        .order(trace)
        .with_idle_timeout(Duration::from_secs(1));
    let request: reqwest::Request = order.request()?;
    let taken: reqwest::Request = order.clone().into_request()?;
    let full: concord::Answer<Item> = order.clone().send_full().await?;
    let raw: reqwest::Response = order.clone().send_raw().await?;
    let future: concord::Sending<Item, ShapesProblem> = order.into_future();
    owned_send(future);

    // A GET and a HEAD have a conditional send; their value is then an Option.
    let ping: concord::Answer<Option<()>> = shapes.ping().send_if_none_match("e1").await?;
    let present: concord::Answer<Option<bool>> = shapes.present(1).send_if_none_match("e1").await?;
    let got: concord::Answer<Option<Item>> =
        shapes.wrapped().get(1).send_if_none_match("e1").await?;
    let outcome: concord::Answer<Option<Result<Item, Conflict>>> =
        shapes.outcome(1).send_if_none_match("e1").await?;

    // An answer that the caller got is read with the arms of the endpoint.
    let response = concord::reply(201, "");
    let receipt: Receipt = shapes
        .create(Item {
            id: 1,
            title: "one".into(),
        })
        .read_response(response)
        .await?;
    Ok(())
}

fn calls_and_handles_are_send_and_clone(shapes: Shapes, error: shapes::Error) {
    let wrapped: shapes::Wrapped = shapes.wrapped();
    cloneable(&wrapped);
    owned_send(wrapped.clone());
    let find: shapes::wrapped::Find = wrapped.find(1);
    cloneable(&find);
    owned_send(find.clone().send());
    let upload: shapes::Upload = shapes.upload(Body::new("abc", "text/plain"));
    owned_send(upload.send());
    let problem: Option<&ShapesProblem> = error.problem();
}

// The author's helpers.

#[test]
fn unpack_gives_the_value_or_the_problem() {
    let item = Item {
        id: 1,
        title: "one".into(),
    };
    assert_eq!(unpack(Envelope::Data(item.clone())), Ok(item));
    let problem = ShapesProblem {
        code: "held".into(),
        message: "the item is held".into(),
    };
    assert_eq!(
        unpack::<Item>(Envelope::Problem(problem.clone())),
        Err(problem)
    );
}

#[test]
fn an_envelope_is_read_from_one_key() {
    let data: Envelope<Item> = serde_json::from_str(r#"{"data":{"id":1,"title":"one"}}"#).unwrap();
    assert_eq!(
        data,
        Envelope::Data(Item {
            id: 1,
            title: "one".into()
        })
    );
    let problem: Envelope<Item> =
        serde_json::from_str(r#"{"problem":{"code":"held","message":"m"}}"#).unwrap();
    assert!(matches!(problem, Envelope::Problem(_)));
}

#[test]
fn checked_writes_json_and_refuses_an_empty_title() {
    let body = checked(&Item {
        id: 1,
        title: "t".into(),
    })
    .unwrap();
    assert_eq!(body.as_bytes().unwrap(), br#"{"id":1,"title":"t"}"#);
    let error = checked(&Item {
        id: 1,
        title: String::new(),
    })
    .unwrap_err();
    assert!(error.is::<EmptyTitle>());
}

#[test]
fn a_trace_notes_in_order_and_gives_what_was_chosen() {
    let trace = Trace::new(true, Duration::from_millis(1500));
    let clone = trace.clone();
    assert_eq!(clone.idle_timeout(), Duration::from_millis(1500));
    assert!(clone.idempotent());
    assert_eq!(clone.note("query"), "query");
    assert_eq!(trace.seen(), ["idle_timeout", "idempotent", "query"]);
}

#[test]
fn the_message_of_chatty_is_longer_than_1_kib() {
    assert!(Chatty::message().len() > concord::KIB);
}

#[test]
fn debug_prints_no_input() {
    let shapes = Shapes::new();
    let hosted = format!("{:?}", shapes.hosted("private-tenant"));
    assert!(!hosted.contains("private-tenant"));
    assert!(hosted.contains("shapes.hosted"));
    let parsed = format!("{:?}", shapes.parsed("private-number"));
    assert!(!parsed.contains("private-number"));
    let found = format!("{:?}", shapes.wrapped().find(424242));
    assert!(!found.contains("424242"));
    assert!(found.contains("shapes.wrapped.find"));
}

// The requests, as the paper builds them.

fn item(title: &str) -> Item {
    Item {
        id: 1,
        title: title.into(),
    }
}

// A helper of the tests below: a missing body is a failed test.
#[allow(clippy::unwrap_used)]
fn body_of(request: &reqwest::Request) -> &[u8] {
    request.body().and_then(|body| body.as_bytes()).unwrap()
}

// A helper of the tests below: a request that was built is a failed test.
#[allow(clippy::unwrap_used)]
fn build_error(result: shapes::Result<reqwest::Request>) -> String {
    let error = result.unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    error.to_string()
}

/// A stream that ends at once: a body that can be sent once.
struct Empty;

impl futures_core::Stream for Empty {
    type Item = Result<bytes::Bytes, std::io::Error>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

#[test]
fn request_of_each_method_and_path() {
    let shapes = Shapes::new();
    let cases = [
        (
            shapes.again(7).request(),
            "POST",
            "https://shapes.example.com/again/7",
        ),
        (
            shapes.ping().request(),
            "HEAD",
            "https://shapes.example.com/ping",
        ),
        (
            shapes.present(7).request(),
            "HEAD",
            "https://shapes.example.com/items/7",
        ),
        (
            shapes.outcome(7).request(),
            "GET",
            "https://shapes.example.com/outcome/7",
        ),
        (
            shapes.counts().request(),
            "GET",
            "https://shapes.example.com/counts",
        ),
        (
            shapes.strict().request(),
            "GET",
            "https://shapes.example.com/strict",
        ),
        (
            shapes.long_message().request(),
            "GET",
            "https://shapes.example.com/long-message",
        ),
        (
            shapes.wrapped().get(7).request(),
            "GET",
            "https://shapes.example.com/wrapped/7",
        ),
        (
            shapes.wrapped().raw(7).request(),
            "GET",
            "https://shapes.example.com/wrapped/raw/7",
        ),
        (
            shapes.wrapped().find(7).request(),
            "GET",
            "https://shapes.example.com/wrapped/find/7",
        ),
    ];
    for (request, method, url) in cases {
        let request = request.unwrap();
        assert_eq!(request.method().as_str(), method);
        assert_eq!(request.url().as_str(), url);
        assert!(request.body().is_none(), "{url}");
    }
}

#[test]
fn request_with_a_json_body() {
    let shapes = Shapes::new();
    for (request, method, url) in [
        (
            shapes.replace(7, item("t")).request(),
            "PUT",
            "https://shapes.example.com/items/7",
        ),
        (
            shapes.create(item("t")).request(),
            "POST",
            "https://shapes.example.com/items",
        ),
    ] {
        let request = request.unwrap();
        assert_eq!(request.method().as_str(), method);
        assert_eq!(request.url().as_str(), url);
        assert_eq!(request.headers()["content-type"], "application/json");
        assert_eq!(body_of(&request), br#"{"id":1,"title":"t"}"#);
    }
}

#[test]
fn request_runs_the_entries_and_neither_idle_timeout_nor_idempotent() {
    let trace = Trace::new(true, Duration::from_secs(5));
    let request = Shapes::new().order(&trace).request().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://shapes.example.com/order?step=query"
    );
    assert_eq!(trace.seen(), ["query"]);
}

#[test]
fn request_of_a_body_that_the_caller_gives() {
    let shapes = Shapes::new();
    let request = shapes
        .upload(Body::new("abc", "text/plain"))
        .request()
        .unwrap();
    assert_eq!(request.headers()["content-type"], "text/plain");
    assert_eq!(body_of(&request), b"abc");

    let stream = shapes.upload(Body::stream(Empty));
    assert_eq!(
        build_error(stream.request()),
        "build error: the body is a stream; into_request takes it, in shapes.upload"
    );
    let taken = stream.into_request().unwrap();
    assert!(taken.body().unwrap().as_bytes().is_none());
}

#[test]
fn request_with_a_string_hole_in_the_host() {
    let shapes = Shapes::new();
    let request = shapes.hosted("acme").request().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://acme.shapes.example.com/tenant"
    );
    // A17: an empty host value is the Build error of a host hole.
    for tenant in ["", "a.b", "a b"] {
        assert_eq!(
            build_error(shapes.hosted(tenant).request()),
            "build error: input tenant is not one label of a host, in shapes.hosted",
            "{tenant:?}"
        );
    }
}

#[test]
fn request_with_a_float_and_a_struct_in_the_query() {
    let shapes = Shapes::new();
    for (ratio, query) in [(1.5, "ratio=1.5"), (1.0, "ratio=1.0")] {
        let request = shapes.search(ratio).request().unwrap();
        assert_eq!(request.url().query(), Some(query));
    }
    assert_eq!(
        build_error(shapes.search(f64::NAN).request()),
        "build error: input ratio is not a finite number, in shapes.search"
    );
    let filter = Filter {
        owner: "ann".into(),
        open: true,
    };
    assert_eq!(
        build_error(shapes.search(1.5).filter(filter).request()),
        "build error: input filter cannot be written in a query, in shapes.search"
    );
}

#[test]
fn request_with_a_written_content_length_and_a_body_that_fails() {
    let shapes = Shapes::new();
    let request = shapes.measured(item("t"), 20).request().unwrap();
    assert_eq!(request.headers()["content-length"], "20");
    assert_eq!(body_of(&request), br#"{"id":1,"title":"t"}"#);
    assert_eq!(
        build_error(shapes.measured(item("t"), 3).request()),
        "build error: the content-length is not the length of the body, in shapes.measured"
    );
    // A2: the author's error is the source.
    let error = shapes.measured(item(""), 20).request().unwrap_err();
    assert_eq!(
        error.to_string(),
        "build error: the body could not be written, in shapes.measured"
    );
    let source = std::error::Error::source(&error).unwrap();
    assert!(source.is::<EmptyTitle>());
}

#[test]
fn request_with_an_entry_that_uses_the_question_mark() {
    let shapes = Shapes::new();
    let request = shapes.parsed("12").request().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://shapes.example.com/parsed?n=12"
    );
    // A15: the key names the entry, and the author's error is the source.
    let error = shapes.parsed("twelve").request().unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(
        error.to_string(),
        "build error: input n could not be written, in shapes.parsed"
    );
    let source = std::error::Error::source(&error).unwrap();
    assert!(source.is::<std::num::ParseIntError>());
}

// The names of the author, in the expressions of an answer (docs/macro.md section 4).

/// A client whose every send gets this answer.
fn answering(status: u16, body: &'static str) -> Shapes {
    Shapes::new().with_transport(concord::fake(move |_| Ok(concord::reply(status, body))))
}

#[tokio::test]
async fn an_input_is_bound_before_the_read_of_its_endpoint() {
    let trace = Trace::new(false, Duration::from_secs(1));
    let item: Item = answering(200, r#"{"id":1,"title":"one"}"#)
        .noted(&trace)
        .await
        .unwrap();
    assert_eq!(item.id, 1);
    // The endpoint's `read` ran once, for the one answer that it read.
    assert_eq!(trace.seen(), ["read"]);

    // A refusal is read by the client's reader of its problem, not by the endpoint's.
    let trace = Trace::new(false, Duration::from_secs(1));
    let error = answering(500, r#"{"code":"down","message":"m"}"#)
        .noted(&trace)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.problem().map(|p| p.code.as_str()), Some("down"));
    assert!(trace.seen().is_empty());
}

#[tokio::test]
async fn an_input_is_bound_before_an_arm_of_its_endpoint() {
    let item: Item = answering(404, "").titled(7, "seven").await.unwrap();
    assert_eq!(
        item,
        Item {
            id: 7,
            title: "seven".into()
        }
    );
    // Any other 2xx is read as the type after the arrow.
    let item: Item = answering(200, r#"{"id":1,"title":"one"}"#)
        .titled(7, "seven")
        .await
        .unwrap();
    assert_eq!(item.id, 1);
}

#[tokio::test]
async fn an_arm_that_renames_its_body_keeps_an_input_named_body() {
    let shapes = answering(409, r#"{"message":"held by another change"}"#);
    assert_eq!(
        shapes.held(7).request().unwrap().url().as_str(),
        "https://shapes.example.com/held/7"
    );
    let held: Result<Item, Conflict> = shapes.held(7).await.unwrap();
    assert_eq!(
        held,
        Err(Conflict {
            message: "item 7: held by another change".into()
        })
    );
}
