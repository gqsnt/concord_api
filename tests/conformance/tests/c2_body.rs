//! C2.9, C2.11, C2.12, C2.14, C2.15, C2.16: the body (docs/reference/contracts.md, group C2;
//! docs/internals/engine.md, "How a value is written"; docs/reference/rules.md rule 9).

#[macro_use]
mod common;

use common::*;
use concord::__private::{AdaptValue as _, Core, Draft, Options, ValueRef};
use concord::body::Entries;
use concord::{form, json, multipart, Body, Multipart, MultipartError, Secret};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};

const ONE_SHOT_REQUEST: &str =
    "build error: the body is a stream; into_request takes it, in files.upload";
const ALREADY_SENT: &str =
    "build error: the body is a stream that was already sent, in files.upload";
const NOT_WRITTEN: &str = "build error: the body could not be written, in tasks.create";
const LENGTH: &str =
    "build error: the content-length is not the length of the body, in files.upload";

#[derive(Serialize)]
struct Task {
    title: &'static str,
    done: bool,
}

#[derive(Serialize)]
struct Nested {
    a: u32,
}

fn post() -> Draft {
    draft(http::Method::POST, "https://api.example.com/v1")
}

fn put() -> Draft {
    draft(http::Method::PUT, "https://files.example.com/files")
}

fn json_text(entries: &Entries<'_>) -> String {
    let body = json(entries).expect("the entries write as JSON");
    String::from_utf8(body.as_bytes().expect("in memory").to_vec()).expect("UTF-8")
}

fn form_text(entries: &Entries<'_>) -> String {
    let body = form(entries).expect("the entries write as a form");
    String::from_utf8(body.as_bytes().expect("in memory").to_vec()).expect("UTF-8")
}

/// The boundary of a `multipart/form-data` label, with or without quotes.
fn boundary_of(body: &Body) -> String {
    let label = body.__content_type().expect("a multipart body has a label");
    assert!(label.starts_with("multipart/form-data"), "{label}");
    let (_, boundary) = label.split_once("boundary=").expect("a boundary parameter");
    let boundary = boundary.split(';').next().unwrap_or("").trim();
    boundary.trim_matches('"').to_owned()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// C2.9 -------------------------------------------------------------------------------

#[test]
fn c2_9_json_writes_with_serde_json_and_says_application_json() {
    let task = Task {
        title: "a \"b\" é",
        done: false,
    };
    let body = json(&task).expect("a struct writes");
    assert_eq!(
        body.as_bytes(),
        Some(serde_json::to_vec(&task).unwrap().as_slice())
    );
    assert_eq!(body.__content_type(), Some("application/json"));
}

#[test]
fn c2_9_form_writes_a_form_and_says_application_x_www_form_urlencoded() {
    let body = form([("title", "abc"), ("done", "false")]).expect("pairs write");
    assert_eq!(body.as_bytes(), Some(b"title=abc&done=false".as_slice()));
    assert_eq!(
        body.__content_type(),
        Some("application/x-www-form-urlencoded")
    );
}

#[test]
fn c2_9_a_body_made_by_an_expression_follows_its_type() {
    // `json!({ "cursor": cursor })` writes `null` for a `cursor?` that was not set,
    // where `body json { cursor; }` leaves it out (rule 9, silent).
    let cursor: Option<String> = None;
    let body = json(serde_json::json!({ "cursor": cursor })).expect("writes");
    assert_eq!(body.as_bytes(), Some(br#"{"cursor":null}"#.as_slice()));
    let value = ValueRef(&cursor);
    let mut entries = Entries::new();
    entries.__push("cursor", (&value).adapt());
    assert_eq!(json_text(&entries), "{}");
}

#[test]
fn c2_9_the_body_and_its_content_type_go_to_the_request() {
    let task = Task {
        title: "t",
        done: true,
    };
    let mut draft = post();
    draft.body(json(&task));
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(request.method(), http::Method::POST);
    assert_eq!(request.url().as_str(), "https://api.example.com/v1");
    assert_eq!(
        body_bytes(&request),
        Some(serde_json::to_vec(&task).unwrap())
    );
    assert_eq!(header(&request, "content-type"), ["application/json"]);

    let mut draft = post();
    draft.body(form([("a", "1")]));
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(body_bytes(&request), Some(b"a=1".to_vec()));
    assert_eq!(
        header(&request, "content-type"),
        ["application/x-www-form-urlencoded"]
    );

    // A body with no label writes no `content-type`.
    let mut draft = post();
    draft.body(Body::from("raw"));
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(body_bytes(&request), Some(b"raw".to_vec()));
    assert!(header(&request, "content-type").is_empty());
}

#[test]
fn c2_9_a_content_type_entry_with_a_value_wins_over_the_label_of_the_body() {
    let mut draft = post();
    draft.header("content-type", &[lit!("application/vnd.api+json")]);
    draft.body(json(serde_json::json!({ "a": 1 })));
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(
        header(&request, "content-type"),
        ["application/vnd.api+json"]
    );
}

#[test]
fn c2_9_a_reference_to_a_body_is_cloned() {
    let body = Body::new("shared", "text/plain");
    let mut draft = post();
    draft.body(&body);
    let request = ok(finish(draft, "tasks.create"));
    assert_eq!(body_bytes(&request), Some(b"shared".to_vec()));
    assert_eq!(header(&request, "content-type"), ["text/plain"]);
    assert_eq!(body.as_bytes(), Some(b"shared".as_slice()));
}

#[test]
fn c2_9_an_err_is_a_build_error_with_that_error_as_its_source() {
    let mut draft = post();
    draft.body(Err::<Body, _>(std::io::Error::other("the writer refused")));
    let error = finish(draft, "tasks.create").expect_err("a Build error");
    assert_eq!(error.kind(), concord::error::Kind::Build);
    // the existing text; the author's error is `source()`, not printed.
    assert_eq!(error.to_string(), NOT_WRITTEN);
    let source = std::error::Error::source(&error).expect("the writer's error is the source");
    let io = source
        .downcast_ref::<std::io::Error>()
        .expect("the same error");
    assert_eq!(io.to_string(), "the writer refused");

    // A `Result` of a `&Body` too.
    let body = Body::from("x");
    let mut draft = post();
    draft.body(Ok::<&Body, std::io::Error>(&body));
    assert_eq!(
        body_bytes(&ok(finish(draft, "tasks.create"))),
        Some(b"x".to_vec())
    );
}

#[test]
fn c2_9_request_refuses_a_one_shot_stream_without_spending_the_call() {
    let file = stream("photo");
    let mut draft = put();
    draft.body(&file);
    assert_eq!(build_error(finish(draft, "files.upload")), ONE_SHOT_REQUEST);
    // Nothing was taken: the stream is still whole.
    assert!(file.into_stream().is_ok());
}

#[tokio::test]
async fn c2_9_into_request_gives_the_request_and_takes_the_stream_with_it() {
    let file = stream("photo");
    let mut draft = put();
    draft.body(&file);
    let mut request = ok(finish_for_send(draft, "files.upload"));
    assert_eq!(
        body_bytes(&request),
        None,
        "a stream has no bytes in memory"
    );
    assert_eq!(read_request_body(&mut request).await, b"photo");
    assert!(
        file.into_stream().is_err(),
        "the stream went with the request"
    );
}

#[test]
fn c2_9_multipart_builds_a_multipart_body() {
    let photo = vec![0xffu8, 0xd8, 0x00];
    let mut draft = post();
    draft.body(
        multipart()
            .text("chat_id", 42)
            .file("photo", "a.jpg", "image/jpeg", photo.clone())
            .finish(),
    );
    let request = ok(finish(draft, "tasks.create"));
    let label = header(&request, "content-type");
    assert_eq!(label.len(), 1);
    assert!(label[0].starts_with("multipart/form-data"), "{label:?}");
    let bytes = body_bytes(&request).expect("every part is in memory");
    assert!(contains(&bytes, &photo));
}

// C2.11 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c2_11_a_build_error_means_that_nothing_was_sent() {
    let script = concord_testkit::Script::new();
    script.answer(200).body("ok");
    let core = Core::<()>::default().with_transport(script.clone());
    let options = Options::<()>::default();

    let failing = [
        {
            let mut draft = get();
            draft.segment(&[val!("id", "..")]);
            draft
        },
        {
            let mut draft = get();
            draft.header("x-note", &[val!("x-note", "a\nb")]);
            draft
        },
        {
            let mut draft = post();
            draft.body(Err::<Body, _>(std::io::Error::other("no")));
            draft
        },
        {
            let spent = stream("spent");
            drop(spent.clone().into_stream());
            let mut draft = put();
            draft.body(spent);
            draft
        },
    ];
    for draft in failing {
        // Exercise the completed build half; Run/prepare belongs to the send package.
        let error = draft
            .finish_for_send(&core, &options, "tasks.get")
            .expect_err("a Build error");
        assert_eq!(error.kind(), concord::error::Kind::Build);
    }
    assert!(
        script.requests().is_empty(),
        "nothing reached the transport"
    );
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c2_11_a_send_whose_build_fails_reaches_no_middleware_and_no_transport() {
    use common::engine::{failure, success, Log, Logs};
    let item = |title: &str| tour::Item {
        id: 1,
        title: title.to_owned(),
    };
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"id":1,"title":"t"}"#);
    let log = Log::default();
    let shapes = tour::Shapes::new()
        .with_transport(script.clone())
        .with_middleware(Logs {
            log: log.clone(),
            name: "m",
        });
    let spent = stream("spent");
    drop(spent.clone().into_stream());
    let failing = [
        failure(shapes.hosted("a.b").await),
        failure(shapes.search(f64::NAN).await),
        failure(
            shapes
                .search(1.5)
                .filter(tour::Filter {
                    owner: "ann".into(),
                    open: true,
                })
                .await,
        ),
        failure(shapes.measured(item(""), 20).await),
        failure(shapes.measured(item("t"), 3).await),
        failure(shapes.parsed("twelve").await),
        failure(shapes.upload(spent).await),
        failure(shapes.clone().with_base("not a url").again(1).await),
    ];
    for error in failing {
        assert_eq!(error.kind(), concord::error::Kind::Build, "{error}");
        assert_eq!(error.tries(), 0, "{error}");
    }
    assert!(log.events().is_empty(), "no middleware saw a request");
    assert!(
        script.requests().is_empty(),
        "nothing reached the transport"
    );
    // A request that builds goes through, once.
    let sent = success(shapes.measured(item("t"), 20).await);
    assert_eq!(sent, item("t"));
    assert_eq!(script.requests().len(), 1);
    assert!(!log.events().is_empty());
}

// C2.12 ------------------------------------------------------------------------------

#[test]
fn c2_12_a_written_content_length_must_be_the_length_of_a_body_in_memory() {
    let with_length = |length: u64| {
        let mut draft = put();
        draft.header("content-length", &[val!("content-length", length)]);
        draft.body(Body::new("abc", "text/plain"));
        finish(draft, "files.upload")
    };
    let request = ok(with_length(3));
    assert_eq!(header(&request, "content-length"), ["3"]);
    assert_eq!(body_bytes(&request), Some(b"abc".to_vec()));
    assert_eq!(build_error(with_length(4)), LENGTH);
    assert_eq!(build_error(with_length(2)), LENGTH);
    assert_eq!(build_error(with_length(0)), LENGTH);
}

#[test]
fn c2_12_the_length_of_a_body_in_memory_is_its_length_in_bytes() {
    // `é€`: two characters, five bytes of UTF-8.
    let with_length = |length: u64| {
        let mut draft = put();
        draft.header("content-length", &[val!("content-length", length)]);
        draft.body(Body::new("é€", "text/plain"));
        finish(draft, "files.upload")
    };
    let request = ok(with_length(5));
    assert_eq!(header(&request, "content-length"), ["5"]);
    assert_eq!(body_bytes(&request), Some("é€".as_bytes().to_vec()));
    assert_eq!(build_error(with_length(2)), LENGTH);
    // A body made of bodies, in memory: the length of the whole.
    let mut draft = put();
    draft.header("content-length", &[val!("content-length", 6u64)]);
    draft.body(Body::concat([Body::from("abc"), Body::from("def")]));
    assert_eq!(
        body_bytes(&ok(finish(draft, "files.upload"))),
        Some(b"abcdef".to_vec())
    );
}

#[tokio::test]
async fn c2_12_a_written_content_length_of_a_stream_is_sent_as_written() {
    for length in [5u64, 1, 1000] {
        let mut draft = put();
        draft.header("content-length", &[val!("content-length", length)]);
        draft.body(stream("photo"));
        let mut request = ok(finish_for_send(draft, "files.upload"));
        assert_eq!(header(&request, "content-length"), [length.to_string()]);
        // The stream is given whole: neither cut nor padded to the length.
        assert_eq!(read_request_body(&mut request).await, b"photo", "{length}");
    }
}

// C2.14 ------------------------------------------------------------------------------

#[test]
fn c2_14_entries_are_written_as_a_map_in_the_order_written() {
    let (z, a, m) = (ValueRef("last"), ValueRef(1u32), ValueRef(true));
    let mut entries = Entries::new();
    entries.__push("z", (&z).adapt());
    entries.__push("a", (&a).adapt());
    entries.__push("m", (&m).adapt());
    assert_eq!(json_text(&entries), r#"{"z":"last","a":1,"m":true}"#);
    assert_eq!(form_text(&entries), "z=last&a=1&m=true");
}

#[test]
fn c2_14_a_none_writes_nothing() {
    // `x;` with `x?: T` or `x: Option<T>`: no key, or the value.
    for (x, json_expected, form_expected) in [
        (None, r#"{"first":0,"last":9}"#, "first=0&last=9"),
        (
            Some(5u32),
            r#"{"first":0,"x":5,"last":9}"#,
            "first=0&x=5&last=9",
        ),
    ] {
        let (first, value, last) = (ValueRef(0u8), ValueRef(&x), ValueRef(9u8));
        let mut entries = Entries::new();
        entries.__push("first", (&first).adapt());
        entries.__push("x", (&value).adapt());
        entries.__push("last", (&last).adapt());
        assert_eq!(json_text(&entries), json_expected);
        assert_eq!(form_text(&entries), form_expected);
    }
}

#[test]
fn c2_14_an_optional_option_has_three_states_in_json_and_two_in_a_form() {
    // `milestone;` with `milestone?: Option<u64>`.
    for (milestone, json_expected, form_expected) in [
        (None, "{}", ""),
        (Some(None), r#"{"milestone":null}"#, ""),
        (Some(Some(5u64)), r#"{"milestone":5}"#, "milestone=5"),
    ] {
        let value = ValueRef(&milestone);
        let mut entries = Entries::new();
        entries.__push("milestone", (&value).adapt());
        assert_eq!(json_text(&entries), json_expected, "{milestone:?}");
        assert_eq!(form_text(&entries), form_expected, "{milestone:?}");
    }
}

#[test]
fn c2_14_a_keyed_entry_whose_value_is_a_none_writes_no_key() {
    // `"x" = client.x;` and `"ids" = ids.as_ref().map(join);`.
    let client_x: Option<String> = None;
    let ids: Option<Vec<u32>> = None;
    let join = |ids: &Vec<u32>| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let x = ValueRef(&client_x);
    let joined = ValueRef(ids.as_ref().map(join));
    let mut entries = Entries::new();
    entries.__push("x", (&x).adapt());
    entries.__push("ids", (&joined).adapt());
    assert_eq!(json_text(&entries), "{}");
    assert_eq!(form_text(&entries), "");

    let ids = Some(vec![1u32, 2]);
    let joined = ValueRef(ids.as_ref().map(join));
    let mut entries = Entries::new();
    entries.__push("ids", (&joined).adapt());
    assert_eq!(json_text(&entries), r#"{"ids":"1,2"}"#);
    assert_eq!(form_text(&entries), "ids=1%2C2");
}

#[test]
fn c2_14_some_of_an_option_writes_null_or_the_value() {
    // `"k" = Some(x);` with `x: Option<T>`: the way to write a `None` as `null`.
    for (x, json_expected, form_expected) in [
        (None, r#"{"k":null}"#, ""),
        (Some(3u64), r#"{"k":3}"#, "k=3"),
    ] {
        let value = ValueRef(Some(&x));
        let mut entries = Entries::new();
        entries.__push("k", (&value).adapt());
        assert_eq!(json_text(&entries), json_expected);
        // A form has no `null`: an inner `None` writes no pair either.
        assert_eq!(form_text(&entries), form_expected);
    }
}

#[test]
fn c2_14_a_list_or_a_struct_as_a_value_cannot_be_written_in_a_form() {
    let list = ValueRef(vec![1u32, 2]);
    let mut entries = Entries::new();
    entries.__push("list", (&list).adapt());
    assert!(form(&entries).is_err());
    assert_eq!(json_text(&entries), r#"{"list":[1,2]}"#);

    let nested = ValueRef(Nested { a: 1 });
    let mut entries = Entries::new();
    entries.__push("nested", (&nested).adapt());
    assert!(form(&entries).is_err());
    assert_eq!(json_text(&entries), r#"{"nested":{"a":1}}"#);
}

#[test]
fn c2_14_a_secret_is_written_as_its_text_short_or_as_a_value() {
    // `body form { grant_type; client_id; client_secret; }`
    let grant_type = String::from("client_credentials");
    let client_id = String::from("id-1");
    let client_secret = Secret::new("s3cr3t");
    let (a, b, c) = (
        ValueRef(&grant_type),
        ValueRef(&client_id),
        ValueRef(&client_secret),
    );
    let mut entries = Entries::new();
    entries.__push("grant_type", (&a).adapt());
    entries.__push("client_id", (&b).adapt());
    entries.__push("client_secret", (&c).adapt());
    assert_eq!(
        form_text(&entries),
        "grant_type=client_credentials&client_id=id-1&client_secret=s3cr3t"
    );
    assert_eq!(
        json_text(&entries),
        r#"{"grant_type":"client_credentials","client_id":"id-1","client_secret":"s3cr3t"}"#
    );
    // `"key" = client.key;` with a `Secret`, and `key?: Option<Secret>` set and not set.
    let set = Some(Secret::new("k-1"));
    let unset = None::<Secret>;
    let (value, set, unset) = (ValueRef(&&client_secret), ValueRef(&set), ValueRef(&unset));
    let mut entries = Entries::new();
    entries.__push("key", (&value).adapt());
    entries.__push("set", (&set).adapt());
    entries.__push("unset", (&unset).adapt());
    assert_eq!(json_text(&entries), r#"{"key":"s3cr3t","set":"k-1"}"#);
    assert_eq!(form_text(&entries), "key=s3cr3t&set=k-1");
}

#[test]
fn c2_14_the_unit_is_not_a_none() {
    let unit = ValueRef(());
    let mut entries = Entries::new();
    entries.__push("unit", (&unit).adapt());
    assert_eq!(json_text(&entries), r#"{"unit":null}"#);
    assert!(form(&entries).is_err(), "a form cannot write the unit");
}

#[test]
fn c2_14_a_form_that_cannot_be_written_is_a_build_error() {
    for value in [
        serde_json::json!([1, 2]),
        serde_json::json!([]),
        serde_json::json!({ "a": 1 }),
        serde_json::Value::Null,
    ] {
        let entry = ValueRef(value);
        let mut entries = Entries::new();
        entries.__push("value", (&entry).adapt());
        let mut draft = post();
        draft.body(form(&entries));
        assert_eq!(build_error(finish(draft, "tasks.create")), NOT_WRITTEN);
    }
}

/// A value whose `Serialize` counts how many times it is asked.
#[derive(Default)]
struct Counted(AtomicUsize);

impl Counted {
    fn asked(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl Serialize for Counted {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        serializer.serialize_u32(5)
    }
}

/// A value of the author's whose `Serialize` writes a `None`.
struct WritesNone;

impl Serialize for WritesNone {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_none()
    }
}

#[test]
fn c2_14_a_value_is_asked_whether_it_is_a_none_when_the_body_is_written_not_before() {
    let counted = Counted::default();
    let value = ValueRef(&counted);
    let mut entries = Entries::new();
    entries.__push("n", (&value).adapt());
    assert_eq!(counted.asked(), 0, "adding an entry writes nothing");
    assert_eq!(json_text(&entries), r#"{"n":5}"#);
    // Asked twice: whether it is a `None`, then to be written (C2.14).
    assert_eq!(counted.asked(), 2);
    assert_eq!(form_text(&entries), "n=5");
    assert_eq!(counted.asked(), 4);
}

#[test]
fn c2_14_a_value_whose_serialize_writes_a_none_writes_no_key() {
    // The macro knows no type: `Entries` asks `serde`, so a type of the author's that writes
    // a `None` is a `None`.
    let (first, none, last) = (ValueRef(1u8), ValueRef(WritesNone), ValueRef(2u8));
    let mut entries = Entries::new();
    entries.__push("first", (&first).adapt());
    entries.__push("none", (&none).adapt());
    entries.__push("last", (&last).adapt());
    assert_eq!(json_text(&entries), r#"{"first":1,"last":2}"#);
    assert_eq!(form_text(&entries), "first=1&last=2");
}

#[test]
fn c2_14_an_optional_input_of_an_optional_secret_has_three_states_in_json_and_two_in_a_form() {
    // `key;` with `key?: Option<Secret>`: one level of `Option` is presence (C1.7; C2.14).
    let secret = Secret::new("k-1");
    for (key, json_expected, form_expected) in [
        (None, "{}", ""),
        (Some(None), r#"{"key":null}"#, ""),
        (Some(Some(&secret)), r#"{"key":"k-1"}"#, "key=k-1"),
    ] {
        let value = ValueRef(&key);
        let mut entries = Entries::new();
        entries.__push("key", (&value).adapt());
        assert_eq!(json_text(&entries), json_expected);
        assert_eq!(form_text(&entries), form_expected);
    }
    // And owned.
    let owned: Option<Option<Secret>> = Some(Some(Secret::new("k-2")));
    let value = ValueRef(&owned);
    let mut entries = Entries::new();
    entries.__push("key", (&value).adapt());
    assert_eq!(json_text(&entries), r#"{"key":"k-2"}"#);
}

// C2.15 ------------------------------------------------------------------------------

#[test]
fn c2_15_concat_joins_bodies_in_memory_in_order_as_bytes() {
    let body = Body::concat([
        Body::from("one,"),
        Body::new("two,", "text/plain"),
        Body::concat([Body::from("three,"), Body::from(b"four".to_vec())]),
    ]);
    assert_eq!(body.as_bytes(), Some(b"one,two,three,four".as_slice()));
    assert_eq!(Body::concat([]).as_bytes(), Some(b"".as_slice()));
}

#[tokio::test]
async fn c2_15_concat_with_a_stream_is_a_stream_in_order() {
    let body = Body::concat([Body::from("head,"), stream("middle,"), Body::from("tail")]);
    assert_eq!(body.as_bytes(), None, "a stream has no bytes in memory");
    let taken = body.into_stream().expect("taken once");
    assert_eq!(read(taken).await, b"head,middle,tail");
}

#[test]
fn c2_15_a_one_shot_stream_inside_is_taken_with_the_whole() {
    let inner = stream("inner");
    let other = stream("other");
    let body = Body::concat([Body::from("a"), inner.clone(), other.clone()]);
    // Looking does not take anything.
    assert_eq!(body.as_bytes(), None);
    assert_eq!(body.clone().as_bytes(), None);
    let _taken = body.into_stream().expect("taken");
    assert!(
        inner.into_stream().is_err(),
        "the inner stream went with the whole"
    );
    assert!(
        other.into_stream().is_err(),
        "every inner stream went with the whole"
    );
}

#[test]
fn c2_15_a_clone_shares_the_stream_and_the_second_take_is_refused() {
    let first = stream("once");
    let second = first.clone();
    let _taken = first.into_stream().expect("the first take");
    assert!(second.clone().into_stream().is_err());
    // Also inside a body made of bodies, and none of its other parts is taken.
    let fresh = stream("fresh");
    let whole = Body::concat([fresh.clone(), second]);
    assert!(whole.into_stream().is_err());
    assert!(fresh.into_stream().is_ok(), "a failed take touches no part");
}

#[tokio::test]
async fn c2_15_an_expression_that_makes_a_new_stream_each_time_gives_each_take_its_own() {
    let fresh = || Body::concat([Body::from("<"), stream("x"), Body::from(">")]);
    for _ in 0..2 {
        let taken = fresh().into_stream().expect("a new stream");
        assert_eq!(read(taken).await, b"<x>");
    }
}

#[test]
fn c2_15_as_bytes_and_into_stream_are_public() {
    let as_bytes: fn(&Body) -> Option<&[u8]> = Body::as_bytes;
    let into_stream: fn(Body) -> Result<concord::body::BodyStream, std::io::Error> =
        Body::into_stream;
    assert_eq!(as_bytes(&Body::from("x")), Some(b"x".as_slice()));
    assert!(into_stream(Body::from("x")).is_ok());
}

#[test]
fn c2_15_request_refuses_a_body_that_holds_a_one_shot_stream_without_touching_a_part() {
    let part = stream("part");
    let mut draft = put();
    draft.body(Body::concat([Body::from("head"), part.clone()]));
    assert_eq!(build_error(finish(draft, "files.upload")), ONE_SHOT_REQUEST);
    assert!(part.into_stream().is_ok(), "no part was touched");

    // Inside `multipart` too.
    let file = stream("file");
    let mut draft = put();
    draft.body(
        multipart()
            .file("f", "a.bin", "application/octet-stream", file.clone())
            .finish(),
    );
    assert_eq!(build_error(finish(draft, "files.upload")), ONE_SHOT_REQUEST);
    assert!(file.into_stream().is_ok(), "no part was touched");
}

#[test]
fn c2_15_a_stream_that_was_already_sent_is_a_build_error_before_anything_goes_out() {
    // One `Body`, shared by two clones of a call: the first send takes it.
    let file = stream("photo");
    let mut first = put();
    first.body(&file);
    assert!(finish_for_send(first, "files.upload").is_ok());
    let mut second = put();
    second.body(&file);
    assert_eq!(
        build_error(finish_for_send(second, "files.upload")),
        ALREADY_SENT
    );

    // Inside a body made of bodies.
    let part = stream("part");
    let mut first = put();
    first.body(Body::concat([Body::from("head"), part.clone()]));
    assert!(finish_for_send(first, "files.upload").is_ok());
    let mut second = put();
    second.body(Body::concat([Body::from("head"), part]));
    assert_eq!(
        build_error(finish_for_send(second, "files.upload")),
        ALREADY_SENT
    );

    // Inside `multipart`.
    let file = stream("file");
    let mut first = put();
    first.body(
        multipart()
            .file("f", "a.bin", "application/octet-stream", file.clone())
            .finish(),
    );
    assert!(finish_for_send(first, "files.upload").is_ok());
    let mut second = put();
    second.body(
        multipart()
            .file("f", "a.bin", "application/octet-stream", file)
            .finish(),
    );
    assert_eq!(
        build_error(finish_for_send(second, "files.upload")),
        ALREADY_SENT
    );
}

#[test]
fn c2_15_a_body_that_is_new_for_each_send_gives_each_send_its_own() {
    let fresh = || Body::concat([Body::from("head"), stream("tail")]);
    for _ in 0..2 {
        let mut draft = put();
        draft.body(fresh());
        assert!(finish_for_send(draft, "files.upload").is_ok());
    }
}

// C2.16 ------------------------------------------------------------------------------

#[test]
fn c2_16_text_and_file_give_the_builder_back_and_finish_gives_a_result_of_a_body() {
    let builder: Multipart = multipart().text("chat_id", 42);
    let builder: Multipart = builder.file("photo", "a.jpg", "image/jpeg", vec![1u8, 2, 3]);
    let finished: Result<Body, MultipartError> = builder.finish();
    let body = finished.expect("a legal multipart body");
    assert!(body.as_bytes().is_some(), "every part is in memory");
}

#[test]
fn c2_16_the_body_is_multipart_form_data_with_its_boundary() {
    let photo = [0xffu8, 0xd8, 0x00, 0x0d, 0x0a];
    let body = multipart()
        .text("chat_id", "42")
        .file("photo", "a.jpg", "image/jpeg", photo.to_vec())
        .finish()
        .expect("finishes");
    let boundary = boundary_of(&body);
    assert!(!boundary.is_empty());
    let bytes = body.as_bytes().expect("in memory");
    let delimiter = format!("--{boundary}");
    let close = format!("--{boundary}--");
    assert!(contains(bytes, delimiter.as_bytes()));
    assert!(contains(bytes, close.as_bytes()));
    assert!(contains(bytes, b"name=\"chat_id\""));
    assert!(contains(bytes, b"name=\"photo\""));
    assert!(contains(bytes, b"filename=\"a.jpg\""));
    assert!(contains(bytes, b"image/jpeg"));
    assert!(contains(bytes, &photo));
    // In the order written.
    let at = |needle: &[u8]| bytes.windows(needle.len()).position(|w| w == needle);
    assert!(at(b"chat_id") < at(b"a.jpg"));
    // Every byte, framed as RFC 7578 writes a `multipart/form-data` body.
    let mut expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"chat_id\"\r\n\r\n42\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"a.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\n"
    )
    .into_bytes();
    expected.extend_from_slice(&photo);
    expected.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
    assert_framed(bytes, &expected);
}

/// The body is `expected`, then at most a line end after the close delimiter.
#[track_caller]
fn assert_framed(bytes: &[u8], expected: &[u8]) {
    let rest = bytes
        .strip_prefix(expected)
        .unwrap_or_else(|| panic!("the body is {:?}", String::from_utf8_lossy(bytes)));
    assert!(
        rest.is_empty() || rest == b"\r\n",
        "after the close: {rest:?}"
    );
}

#[test]
fn c2_16_a_content_type_that_is_not_a_legal_header_value_is_the_error_of_finish() {
    for content_type in ["image/jpeg\r\nx-evil: 1", "a\nb", "a\0b", "a\u{7f}b"] {
        let finished = multipart()
            .text("a", "b")
            .file("f", "a.bin", content_type, vec![0u8])
            .finish();
        assert!(finished.is_err(), "{content_type:?}");
    }
}

#[test]
fn c2_16_in_memory_the_same_inputs_give_the_same_bytes() {
    let make = || {
        multipart()
            .text("chat_id", 7)
            .file("doc", "a.txt", "text/plain", "hello")
            .finish()
            .expect("finishes")
    };
    let (first, second) = (make(), make());
    assert_eq!(first.as_bytes(), second.as_bytes());
    assert_eq!(first.__content_type(), second.__content_type());
}

#[test]
fn c2_16_in_memory_the_boundary_is_one_that_no_part_holds() {
    let plain = multipart().text("a", "x").finish().expect("finishes");
    let first = boundary_of(&plain);
    // Each part now holds the boundary that the plain body chose.
    let make = || {
        multipart()
            .text("a", &first)
            .file(
                "f",
                "a.bin",
                "application/octet-stream",
                first.clone().into_bytes(),
            )
            .finish()
            .expect("finishes")
    };
    let body = make();
    let second = boundary_of(&body);
    assert_ne!(second, first);
    assert!(!first.contains(&second), "no part holds the boundary");
    // Still not random.
    assert_eq!(make().as_bytes(), body.as_bytes());
    assert_eq!(boundary_of(&make()), second);
}

#[test]
fn c2_16_in_memory_the_boundary_is_the_first_of_the_list_that_no_part_holds() {
    let text = |value: &str| multipart().text("a", value).finish().expect("finishes");
    // One part, or two copies of it: no part holds more, so the boundary is the same.
    let one = boundary_of(&text("x"));
    let two = boundary_of(
        &multipart()
            .text("a", "x")
            .text("a", "x")
            .finish()
            .expect("finishes"),
    );
    assert_eq!(one, two);
    let file = |count: usize, bytes: &[u8]| {
        let mut builder = multipart();
        for _ in 0..count {
            builder = builder.file("f", "a.bin", "application/octet-stream", bytes.to_vec());
        }
        boundary_of(&builder.finish().expect("finishes"))
    };
    assert_eq!(file(1, b"x"), file(3, b"x"));
    assert_eq!(file(1, b"x"), one);
    // The first is held by a text, or by a file, or by both: the next of the list, the
    // same in the three cases.
    let in_text = boundary_of(&text(&one));
    let in_file = file(1, one.as_bytes());
    let in_both = boundary_of(
        &multipart()
            .text("a", &one)
            .file(
                "f",
                "a.bin",
                "application/octet-stream",
                one.clone().into_bytes(),
            )
            .finish()
            .expect("finishes"),
    );
    assert_ne!(in_text, one);
    assert_eq!(in_file, in_text);
    assert_eq!(in_both, in_text);
    // Held twice, it is still the next.
    assert_eq!(file(2, one.as_bytes()), in_text);
    // The first and the next held: the one after, and the first is not taken back.
    let both = format!("{one} {in_text}");
    let third = boundary_of(&text(&both));
    assert!(third != one && third != in_text);
    assert!(!both.contains(&third));
}

#[test]
fn c2_16_with_a_part_that_is_a_stream_the_boundary_carries_128_random_bits() {
    // No sample proves randomness; this refuses a counter, a clock or a short draw. Over 64
    // draws, the places where the draws differ, times the bits of the characters seen there,
    // must reach 128.
    let draws: Vec<String> = (0..64)
        .map(|_| {
            let body = multipart()
                .text("a", "x")
                .file("f", "a.bin", "application/octet-stream", stream("data"))
                .finish()
                .expect("finishes");
            boundary_of(&body)
        })
        .collect();
    let distinct: std::collections::BTreeSet<&String> = draws.iter().collect();
    assert_eq!(distinct.len(), draws.len(), "every draw is new");
    let shortest = draws.iter().map(String::len).min().unwrap_or(0);
    let mut varying = 0u32;
    let mut alphabet = std::collections::BTreeSet::new();
    for place in 0..shortest {
        let seen: std::collections::BTreeSet<u8> =
            draws.iter().map(|draw| draw.as_bytes()[place]).collect();
        if seen.len() > 1 {
            varying += 1;
            alphabet.extend(seen);
        }
    }
    let bits = f64::from(varying) * (alphabet.len() as f64).log2();
    assert!(
        bits >= 128.0,
        "{varying} places vary, over {} characters: {bits} bits",
        alphabet.len()
    );
}

#[test]
fn c2_16_with_a_part_that_is_a_stream_the_boundary_is_random() {
    let make = || {
        multipart()
            .text("a", "x")
            .file("f", "a.bin", "application/octet-stream", stream("data"))
            .finish()
            .expect("finishes")
    };
    let (first, second) = (make(), make());
    assert_eq!(
        first.as_bytes(),
        None,
        "a body with a stream part is a stream"
    );
    // Two draws of 128 bits.
    assert_ne!(boundary_of(&first), boundary_of(&second));
}

#[tokio::test]
async fn c2_16_a_stream_part_is_taken_with_the_whole() {
    let data = stream("data");
    let body = multipart()
        .text("a", "x")
        .file("f", "a.bin", "application/octet-stream", data.clone())
        .finish()
        .expect("finishes");
    let boundary = boundary_of(&body);
    let bytes = read(body.into_stream().expect("taken")).await;
    assert!(contains(&bytes, b"data"));
    assert!(contains(&bytes, format!("--{boundary}--").as_bytes()));
    assert!(data.into_stream().is_err(), "the part went with the whole");
    // Every byte, framed as in memory.
    let expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nx\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"f\"; filename=\"a.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\ndata\r\n--{boundary}--"
    );
    assert_framed(&bytes, expected.as_bytes());
}

#[test]
fn c2_16_a_control_character_in_a_name_is_the_error_of_finish_and_a_quote_is_percent_22() {
    // "In `multipart`, a field name or a file name that holds a control
    // character (a NUL, CR, LF, a tab) is the error of `finish()`. A `"` is written `%22`;
    // nothing else is changed."
    for name in ["a\rb", "a\nb", "a\0b", "a\tb", "\r", "%0D\r"] {
        assert!(
            multipart().text(name, "v").finish().is_err(),
            "field name {name:?}"
        );
        assert!(
            multipart()
                .file("f", name, "text/plain", "v")
                .finish()
                .is_err(),
            "file name {name:?}"
        );
    }
    let body = multipart()
        .file("a\"b", "c\"d.txt", "text/plain", "v")
        .finish()
        .expect("a quote is legal");
    let bytes = body.as_bytes().expect("in memory");
    assert!(contains(bytes, b"name=\"a%22b\""));
    assert!(contains(bytes, b"filename=\"c%22d.txt\""));
    // Nothing else is changed: `%0D` stays `%0D`, not a carriage return.
    let one = multipart().text("%0D", "v").finish().expect("finishes");
    let other = multipart().text("%22", "v").finish().expect("finishes");
    assert_ne!(one.as_bytes(), other.as_bytes());
}

#[test]
fn c2_16_the_names_quote_and_percent_22_are_written_the_same() {
    // "`%` is written as it is, as browsers do [...] So the names `"` and
    // `%22` are written the same. [...] Concord does not check that the names of one body
    // differ."
    let quote = multipart().text("\"", "v").finish().expect("finishes");
    let percent = multipart().text("%22", "v").finish().expect("finishes");
    assert_eq!(quote.as_bytes(), percent.as_bytes(), "field names");
    assert!(contains(
        quote.as_bytes().expect("in memory"),
        b"name=\"%22\""
    ));

    let quote = multipart()
        .file("f", "\".txt", "text/plain", "v")
        .finish()
        .expect("finishes");
    let percent = multipart()
        .file("f", "%22.txt", "text/plain", "v")
        .finish()
        .expect("finishes");
    assert_eq!(quote.as_bytes(), percent.as_bytes(), "file names");

    // Two parts of one body with the same name: no error.
    let twice = multipart()
        .text("a", "1")
        .text("a", "2")
        .finish()
        .expect("names need not differ");
    let twice = twice.as_bytes().expect("in memory");
    assert_eq!(
        twice
            .windows(b"name=\"a\"".len())
            .filter(|window| *window == b"name=\"a\"")
            .count(),
        2
    );
}

// -------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c2_7_c2_9_the_label_of_a_body_is_a_header_value() {
    // "`Body::new(bytes, label)` and `Body::stream(…, label)` with a label
    // that is not a legal header value give the Build error of a header: `the value of header
    // content-type is not a legal header value`."
    const LABEL: &str = "build error: the value of header content-type is not a legal header value, in shapes.upload";
    for label in [
        "text/plain\r\nx-label-7d21: 1",
        "text/plain; x=label-7d21\n",
        "text/label-7d21\u{e9}",
        "text/label-7d21\u{0}",
    ] {
        for streamed in [false, true] {
            let body = if streamed {
                stream("abc").content_type(label)
            } else {
                Body::new("abc", label)
            };
            let script = concord_testkit::Script::new();
            script.answer(200);
            let shapes = engine::shapes(&script);
            let call = shapes.upload(body);
            if !streamed {
                assert_eq!(
                    engine::failure(call.request()).to_string(),
                    LABEL,
                    "{label:?}"
                );
            }
            let error = engine::failure(call.await);
            assert_eq!(error.kind(), concord::error::Kind::Build, "{label:?}");
            assert_eq!(error.to_string(), LABEL, "{label:?}");
            assert_eq!(error.tries(), 0);
            assert!(std::error::Error::source(&error).is_none());
            assert!(!format!("{error:?}").contains("label-7d21"));
            assert!(script.requests().is_empty(), "{label:?}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c2_9_a_legal_label_is_sent_as_the_content_type() {
    // The other side of a label that is a legal header value is the content type.
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(engine::ITEM);
    engine::success(
        engine::shapes(&script)
            .upload(Body::new("abc", "text/plain; charset=utf-8"))
            .await,
    );
    assert_eq!(
        script.requests()[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
}
