//! C12.1 to C12.5, the writers of request bodies: `concord::bracket_form` with
//! `concord::bracket_pairs`, `concord::gzip`, `concord::multipart_mixed` with
//! `concord::multipart_related`, and `concord::json_lines` (features `form`, `gzip-body`,
//! `multipart`, `json`).
//!
//! Written from the public surface and its rustdoc alone. Where a promise is about what is
//! sent, the body goes through a description of `concord::api!` and a generated client over a
//! `concord_testkit::Script`, on Tokio's paused clock; the bytes that reach the script are the
//! evidence. Where a promise is about the value a writer returns, the writer is called through
//! the facade.

mod common;

use common::engine::{failure, success};
use concord::error::Kind;
use concord::http::{HeaderMap, HeaderValue};
use concord::{Body, Retry};
use concord_testkit::Script;
use futures_core::Stream;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::error::Error as _;
use std::io::Read as _;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

concord::api! {
    client Writers {
        base "https://writers.example.test";

        POST nested(value: Value) { path "nested"; body concord::bracket_form(value); }
        POST nested_typed(value: Value) {
            path "nested";
            headers { "content-type" = "text/x-mine"; }
            body concord::bracket_form(value);
        }
        PUT nested_again(value: Value) { path "nested"; body concord::bracket_form(value); }
        POST entries(metadata: BTreeMap<String, String>, expand?: Vec<String>) {
            path "entries";
            body concord::bracket_form { metadata; expand; }
        }
        GET search() { path "search"; query { "page" = 1; } }

        POST gz(data: Body) {
            path "gz";
            headers { "content-encoding" = "gzip"; }
            body concord::gzip(data);
        }
        POST gz_json(events: Vec<String>) {
            path "gz";
            headers { "content-encoding" = "gzip"; }
            body concord::gzip(concord::json(events)?);
        }
        POST gz_typed(data: Body) {
            path "gz";
            headers { "content-encoding" = "gzip"; "content-type" = "application/x-mine"; }
            body concord::gzip(data);
        }
        PUT gz_again(data: Body) {
            path "gz";
            headers { "content-encoding" = "gzip"; }
            body concord::gzip(data);
        }

        POST mixed(parts: Vec<(HeaderMap, Body)>) {
            path "mixed";
            body concord::multipart_mixed(parts.iter().cloned());
        }
        POST related(parts: Vec<(HeaderMap, Body)>) {
            path "related";
            body concord::multipart_related(parts.iter().cloned());
        }
        PUT mixed_again(parts: Vec<(HeaderMap, Body)>) {
            path "mixed";
            body concord::multipart_mixed(parts.iter().cloned());
        }
        PUT related_again(parts: Vec<(HeaderMap, Body)>) {
            path "related";
            body concord::multipart_related(parts.iter().cloned());
        }

        POST lines(records: Vec<Value>, max: usize) {
            path "lines";
            body concord::json_lines(records, *max);
        }
        POST lines_typed(records: Vec<Value>, max: usize) {
            path "lines";
            headers { "content-type" = "application/json-seq"; }
            body concord::json_lines(records, *max);
        }
        PUT lines_again(records: Vec<Value>, max: usize) {
            path "lines";
            body concord::json_lines(records, *max);
        }
        PUT lines_counted(records: Vec<Counted>, max: usize) {
            path "lines";
            body concord::json_lines(records, *max);
        }
    }
}

// What the tests share ----------------------------------------------------------------

/// Text that keys, values and headers hold, so that a printed line can be searched for it.
const MARK: &str = "pr1vate";

fn writers(script: &Script) -> Writers {
    Writers::new().with_transport(script.clone())
}

/// The one request that reached the script.
#[track_caller]
fn only_request(script: &Script) -> concord_testkit::script::Request {
    let requests = script.requests();
    assert_eq!(requests.len(), 1, "one request reached the transport");
    requests.into_iter().next().unwrap()
}

/// The body bytes of a recorded request.
#[track_caller]
fn sent_body(request: &concord_testkit::script::Request) -> Vec<u8> {
    request
        .body()
        .expect("the body was sent as bytes in memory")
        .to_vec()
}

/// A Build error with no attempt: the call failed before the transport saw anything.
#[track_caller]
fn refused_before_sending(script: &Script, error: &concord::Error) {
    assert_eq!(error.kind(), Kind::Build, "the kind of the error");
    assert!(
        script.requests().is_empty(),
        "nothing reached the transport"
    );
}

/// Every printed form of an error and of each error in its `source()` chain.
fn printed_chain(error: &(dyn std::error::Error + 'static)) -> Vec<String> {
    let mut out = Vec::new();
    let mut next = Some(error);
    while let Some(error) = next {
        out.push(error.to_string());
        out.push(format!("{error:?}"));
        next = error.source();
    }
    out
}

#[track_caller]
fn assert_no_mark(lines: &[String]) {
    for line in lines {
        assert!(!line.contains(MARK), "a printed line holds a value: {line}");
    }
}

/// The pairs, or the test fails with the kind of the failure.
#[track_caller]
fn pairs<T: Serialize>(value: T) -> Vec<(String, String)> {
    match concord::bracket_pairs(value) {
        Ok(pairs) => pairs,
        Err(error) => panic!("expected pairs, got an error: {:?}", error.source()),
    }
}

fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// The refusal of a shape: a `WriteError` whose source is `BracketFormError::Shape`.
#[track_caller]
fn refused_shape<T: Serialize>(value: T) {
    match concord::bracket_pairs(&value) {
        Ok(pairs) => panic!("expected a refusal, got {pairs:?}"),
        Err(error) => assert!(
            matches!(
                concord::BracketFormError::find(&error),
                Some(concord::BracketFormError::Shape)
            ),
            "the source of the refusal is Shape"
        ),
    }
    assert!(
        concord::bracket_form(&value).is_err(),
        "bracket_form refuses what bracket_pairs refuses"
    );
}

/// A one-shot stream that records whether it was polled; it gives `kept` once.
struct Watched {
    polled: Arc<AtomicBool>,
    done: bool,
}

impl Stream for Watched {
    type Item = Result<bytes::Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polled.store(true, Ordering::SeqCst);
        if self.done {
            Poll::Ready(None)
        } else {
            self.done = true;
            Poll::Ready(Some(Ok(bytes::Bytes::from_static(b"kept"))))
        }
    }
}

/// A stream body and the flag that says whether it was ever polled.
fn watched() -> (Body, Arc<AtomicBool>) {
    let polled = Arc::new(AtomicBool::new(false));
    let body = Body::stream(Watched {
        polled: polled.clone(),
        done: false,
    });
    (body, polled)
}

/// The kept clone of a stream was not claimed: it still gives its bytes.
async fn still_unclaimed(clone: Body) {
    let stream = clone
        .into_stream()
        .expect("the source of the stream was not claimed");
    assert_eq!(common::read(stream).await, b"kept");
}

/// Bytes that do not look like anything, from a fixed seed.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// The single gzip member of `bytes`, decoded; the test fails if anything follows it.
#[track_caller]
fn gunzip_one_member(bytes: &[u8]) -> Vec<u8> {
    let mut decoder = flate2::bufread::GzDecoder::new(bytes);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .expect("the body is valid gzip");
    let rest: &[u8] = decoder.into_inner();
    assert!(
        rest.is_empty(),
        "{} bytes follow the first member",
        rest.len()
    );
    out
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            concord::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

fn part(content_type: &str, body: impl Into<Body>) -> (HeaderMap, Body) {
    (headers(&[("content-type", content_type)]), body.into())
}

/// The candidate boundary number `n`.
fn candidate(n: u32) -> String {
    format!("concord-{n:032}")
}

/// The media type and the parameters of a content type (quotes removed).
fn media(content_type: &str) -> (String, Vec<(String, String)>) {
    let mut pieces = content_type.split(';');
    let media = pieces.next().unwrap().trim().to_ascii_lowercase();
    let params = pieces
        .map(|param| {
            let (k, v) = param.split_once('=').expect("a parameter has `=`");
            (
                k.trim().to_ascii_lowercase(),
                v.trim().trim_matches('"').to_owned(),
            )
        })
        .collect();
    (media, params)
}

fn param(params: &[(String, String)], name: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// A part as read back: its header lines (name, value bytes) in order, and its bytes.
#[derive(Debug, PartialEq)]
struct Read {
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The parts of a multipart body framed by `boundary` (RFC 2046): each delimiter is
/// `CRLF--boundary`, the last one is followed by `--`. Header lines end in CRLF and are
/// `name:value`, with one optional space after the colon. A preamble before the first
/// delimiter and an epilogue after the last are tolerated (the rustdoc does not mention them).
#[track_caller]
fn read_parts(body: &[u8], boundary: &str) -> Vec<Read> {
    let delimiter = format!("\r\n--{boundary}").into_bytes();
    let mut rest = [b"\r\n".as_slice(), body].concat();
    let first = find(&rest, &delimiter).expect("a first delimiter");
    rest.drain(..first + delimiter.len());
    let mut parts = Vec::new();
    loop {
        if rest.starts_with(b"--") {
            return parts;
        }
        assert!(rest.starts_with(b"\r\n"), "a delimiter line ends in CRLF");
        let end = find(&rest, &delimiter).expect("a part is closed by a delimiter");
        let segment = rest[2..end].to_vec();
        rest.drain(..end + delimiter.len());
        let split = find(&segment, b"\r\n\r\n").expect("the headers end with an empty line");
        let headers = segment[..split]
            .split(|b| *b == b'\n')
            .map(|line| {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                let colon = line.iter().position(|b| *b == b':').expect("name:value");
                let value = &line[colon + 1..];
                let value = value.strip_prefix(b" ").unwrap_or(value);
                (
                    String::from_utf8(line[..colon].to_vec()).unwrap(),
                    value.to_vec(),
                )
            })
            .collect();
        parts.push(Read {
            headers,
            body: segment[split + 4..].to_vec(),
        });
    }
}

fn read(headers: &[(&str, &[u8])], body: &[u8]) -> Read {
    Read {
        headers: headers
            .iter()
            .map(|(n, v)| ((*n).to_owned(), v.to_vec()))
            .collect(),
        body: body.to_vec(),
    }
}

/// The boundary of a sent multipart request, after checking its media type.
#[track_caller]
fn sent_boundary(request: &concord_testkit::script::Request, expected_media: &str) -> String {
    let (media, params) = media(request.header("content-type").expect("a content type"));
    assert_eq!(media, expected_media);
    param(&params, "boundary").expect("a boundary parameter")
}

/// The error of a multipart writer, or the test fails.
#[track_caller]
fn mime_error(result: Result<Body, concord::MimeWriteError>) -> concord::MimeWriteError {
    match result {
        Ok(body) => panic!("expected a refusal, got {body:?}"),
        Err(error) => error,
    }
}

/// The error of `json_lines`, or the test fails.
#[track_caller]
fn lines_error(result: Result<Body, concord::JsonLinesError>) -> concord::JsonLinesError {
    match result {
        Ok(body) => panic!("expected a refusal, got {body:?}"),
        Err(error) => error,
    }
}

/// A value nested `depth` levels deep in maps under the key `k`.
fn deep(depth: usize) -> Value {
    let mut value = json!("x");
    for _ in 0..depth {
        value = json!({ "k": value });
    }
    value
}

/// A record that counts how many times it is serialized.
#[derive(Clone)]
struct Counted(Arc<AtomicUsize>);

impl Serialize for Counted {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        serializer.serialize_u8(7)
    }
}

/// A record whose serialization fails.
struct Refuses;

impl Serialize for Refuses {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("refused by the record"))
    }
}

// C12.1 ------------------------------------------------------------------------------

#[test]
fn c12_1_a_map_is_written_as_a_bracketed_key() {
    assert_eq!(pairs(json!({ "a": { "b": "x" } })), owned(&[("a[b]", "x")]));
    assert_eq!(
        pairs(json!({ "a": { "b": { "c": "x" } } })),
        owned(&[("a[b][c]", "x")])
    );
}

#[test]
fn c12_1_every_list_is_indexed_from_zero_scalars_included() {
    assert_eq!(
        pairs(json!({ "tags": ["x", "y", "z"] })),
        owned(&[("tags[0]", "x"), ("tags[1]", "y"), ("tags[2]", "z")])
    );
    // One element is still indexed: no `tags=x`, no `tags[]=x`.
    assert_eq!(pairs(json!({ "tags": ["x"] })), owned(&[("tags[0]", "x")]));
    assert_eq!(
        pairs(json!({ "grid": [["a", "b"], ["c"]] })),
        owned(&[
            ("grid[0][0]", "a"),
            ("grid[0][1]", "b"),
            ("grid[1][0]", "c")
        ])
    );
}

#[test]
fn c12_1_maps_in_lists_and_lists_in_maps_nest_in_one_convention() {
    assert_eq!(
        pairs(json!({
            "items": [{ "price": "p1", "quantity": 2 }, { "price": "p2", "quantity": 1 }],
            "meta": { "k": ["v0", "v1"] },
        })),
        owned(&[
            ("items[0][price]", "p1"),
            ("items[0][quantity]", "2"),
            ("items[1][price]", "p2"),
            ("items[1][quantity]", "1"),
            ("meta[k][0]", "v0"),
            ("meta[k][1]", "v1"),
        ])
    );
}

#[test]
fn c12_1_pairs_follow_the_order_of_the_value_not_the_alphabet() {
    #[derive(Serialize)]
    struct Inner {
        zeta: u8,
        alpha: u8,
    }
    #[derive(Serialize)]
    struct Outer {
        zulu: &'static str,
        inner: Inner,
        alpha: Vec<u8>,
    }
    let value = Outer {
        zulu: "z",
        inner: Inner { zeta: 1, alpha: 2 },
        alpha: vec![9, 8],
    };
    let expected = owned(&[
        ("zulu", "z"),
        ("inner[zeta]", "1"),
        ("inner[alpha]", "2"),
        ("alpha[0]", "9"),
        ("alpha[1]", "8"),
    ]);
    assert_eq!(pairs(&value), expected);
    assert_eq!(
        pairs(&value),
        expected,
        "the same value twice, the same pairs"
    );
}

#[test]
fn c12_1_an_omitted_element_keeps_the_indices_of_the_others() {
    #[derive(Serialize)]
    struct Form {
        a: Vec<Option<u8>>,
    }
    assert_eq!(
        pairs(Form {
            a: vec![Some(1), None, Some(3)]
        }),
        owned(&[("a[0]", "1"), ("a[2]", "3")])
    );
    assert_eq!(
        pairs(Form {
            a: vec![None, None, Some(5)]
        }),
        owned(&[("a[2]", "5")])
    );
}

#[test]
fn c12_1_a_unit_variant_is_written_as_its_serde_name() {
    #[derive(Serialize)]
    #[serde(rename_all = "snake_case")]
    enum Status {
        InProgress,
        #[serde(rename = "DONE-now")]
        Done,
    }
    #[derive(Serialize)]
    struct Form {
        status: Status,
        history: Vec<Status>,
    }
    assert_eq!(
        pairs(Form {
            status: Status::InProgress,
            history: vec![Status::Done, Status::InProgress],
        }),
        owned(&[
            ("status", "in_progress"),
            ("history[0]", "DONE-now"),
            ("history[1]", "in_progress"),
        ])
    );
}

#[test]
fn c12_1_a_variant_that_carries_data_is_refused() {
    #[derive(Serialize)]
    enum Payload {
        Newtype(u8),
        Tuple(u8, u8),
        Struct { x: u8 },
    }
    #[derive(Serialize)]
    struct Form {
        p: Payload,
    }
    refused_shape(Form {
        p: Payload::Newtype(1),
    });
    refused_shape(Form {
        p: Payload::Tuple(1, 2),
    });
    refused_shape(Form {
        p: Payload::Struct { x: 1 },
    });
    // Inside a list, after an element that was written.
    #[derive(Serialize)]
    struct Listed {
        ok: u8,
        ps: Vec<Payload>,
    }
    refused_shape(Listed {
        ok: 1,
        ps: vec![Payload::Newtype(1)],
    });
}

#[test]
fn c12_1_what_is_absent_writes_nothing() {
    #[derive(Serialize)]
    struct Form {
        none: Option<u8>,
        unit: (),
        list: Vec<u8>,
        map: BTreeMap<String, u8>,
        nested: BTreeMap<String, Vec<u8>>,
        kept: u8,
    }
    let value = Form {
        none: None,
        unit: (),
        list: vec![],
        map: BTreeMap::new(),
        nested: BTreeMap::from([("e".to_owned(), vec![])]),
        kept: 1,
    };
    assert_eq!(pairs(&value), owned(&[("kept", "1")]));

    #[derive(Serialize)]
    struct Empty {
        none: Option<u8>,
        list: Vec<u8>,
    }
    assert_eq!(
        pairs(Empty {
            none: None,
            list: vec![]
        }),
        vec![]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_1_an_absent_root_writes_an_empty_body() {
    assert_eq!(pairs(None::<BTreeMap<String, u8>>), vec![]);
    assert_eq!(pairs(json!({})), vec![]);

    let script = Script::new();
    script.answer(200);
    success(writers(&script).nested(json!({ "a": [] })).await);
    let request = only_request(&script);
    assert_eq!(request.body().unwrap_or_default(), b"");
}

#[test]
fn c12_1_a_root_in_an_option_or_a_newtype_is_written() {
    #[derive(Serialize)]
    struct Inner {
        a: u8,
    }
    #[derive(Serialize)]
    struct Wrapper(Inner);
    assert_eq!(pairs(Some(Inner { a: 1 })), owned(&[("a", "1")]));
    assert_eq!(pairs(Wrapper(Inner { a: 2 })), owned(&[("a", "2")]));
    assert_eq!(pairs(Some(Wrapper(Inner { a: 3 }))), owned(&[("a", "3")]));
}

#[test]
fn c12_1_a_root_that_is_not_a_map_or_struct_is_refused() {
    refused_shape(5u8);
    refused_shape("text");
    refused_shape(vec![1u8, 2]);
    refused_shape(Some(vec![("a", "b")]));
    refused_shape(json!(["a"]));
}

#[test]
fn c12_1_a_newtype_is_transparent() {
    #[derive(Serialize)]
    struct Id(u32);
    #[derive(Serialize)]
    struct Tags(Vec<&'static str>);
    #[derive(Serialize)]
    struct Form {
        id: Id,
        tags: Tags,
    }
    assert_eq!(
        pairs(Form {
            id: Id(7),
            tags: Tags(vec!["t"])
        }),
        owned(&[("id", "7"), ("tags[0]", "t")])
    );
}

#[test]
fn c12_1_a_key_that_is_empty_or_holds_a_bracket_is_refused() {
    for key in ["", "a[", "a]", "[", "]", "a[b]"] {
        refused_shape(BTreeMap::from([(key, "x")]));
        // The same key one level down.
        refused_shape(BTreeMap::from([("ok", BTreeMap::from([(key, "x")]))]));
    }
}

#[test]
fn c12_1_a_key_that_does_not_serialize_as_a_string_is_refused() {
    // "Keys must serialize as nonempty strings without brackets."
    refused_shape(BTreeMap::from([("ok", BTreeMap::from([(1u32, "x")]))]));
    refused_shape(BTreeMap::from([((1u8, 2u8), "x")]));
}

#[test]
fn c12_1_nesting_deeper_than_the_guard_is_refused_and_shallow_nesting_is_written() {
    // Ten levels: well inside the guard of 128.
    let mut key = "a".to_owned();
    for _ in 0..10 {
        key.push_str("[k]");
    }
    assert_eq!(pairs(json!({ "a": deep(10) })), vec![(key, "x".to_owned())]);
    // Two hundred levels: beyond it.
    assert!(concord::bracket_pairs(json!({ "a": deep(200) })).is_err());
    assert!(concord::bracket_form(json!({ "a": deep(200) })).is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_1_the_body_sent_is_form_encoded_after_nesting() {
    let script = Script::new();
    script.answer(200);
    // Keys in alphabetical order, so that the order of a JSON map does not matter here.
    success(
        writers(&script)
            .nested(json!({
                "a": { "b c": "x y" },
                "l": ["&=+%", "é"],
            }))
            .await,
    );
    let request = only_request(&script);
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        String::from_utf8(sent_body(&request)).unwrap(),
        "a%5Bb+c%5D=x+y&l%5B0%5D=%26%3D%2B%25&l%5B1%5D=%C3%A9"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_1_a_content_type_of_the_description_wins() {
    let script = Script::new();
    script.answer(200);
    success(writers(&script).nested_typed(json!({ "a": ["x"] })).await);
    let request = only_request(&script);
    assert_eq!(request.header("content-type"), Some("text/x-mine"));
    assert_eq!(sent_body(&request), b"a%5B0%5D=x");
}

#[tokio::test(start_paused = true)]
async fn c12_1_the_entry_form_works_like_form() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let metadata = BTreeMap::from([
        ("order".to_owned(), "7781".to_owned()),
        ("ref".to_owned(), "a b".to_owned()),
    ]);
    success(writers(&script).entries(metadata.clone()).await);
    success(
        writers(&script)
            .entries(metadata)
            .expand(["customer", "invoice"])
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        String::from_utf8(sent_body(&requests[0])).unwrap(),
        "metadata%5Border%5D=7781&metadata%5Bref%5D=a+b",
        "an absent optional entry writes nothing"
    );
    assert_eq!(
        String::from_utf8(sent_body(&requests[1])).unwrap(),
        "metadata%5Border%5D=7781&metadata%5Bref%5D=a+b&expand%5B0%5D=customer&expand%5B1%5D=invoice"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_1_a_shape_that_cannot_be_written_is_refused_before_anything_is_sent() {
    let script = Script::new();
    script.answer(200);
    let error = failure(writers(&script).nested(json!({ "a": { "b[": "x" } })).await);
    refused_before_sending(&script, &error);
    assert!(matches!(
        concord::BracketFormError::find(&error),
        Some(concord::BracketFormError::Shape)
    ));
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_1_bracket_pairs_are_not_encoded_and_the_url_encodes_them_once() {
    let filter = json!({ "filter": { "status": "50% off", "tags": ["a&b"] } });
    let pairs = pairs(&filter);
    assert_eq!(
        pairs,
        owned(&[("filter[status]", "50% off"), ("filter[tags][0]", "a&b")])
    );

    let script = Script::new();
    script.answer(200);
    success(
        writers(&script)
            .search()
            .edit_request(move |request| {
                request.url_mut().query_pairs_mut().extend_pairs(&pairs);
            })
            .await,
    );
    let request = only_request(&script);
    assert_eq!(
        request.url().as_str(),
        "https://writers.example.test/search?page=1&filter%5Bstatus%5D=50%25+off&filter%5Btags%5D%5B0%5D=a%26b"
    );
}

// C12.2 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_2_the_server_decodes_the_input_byte_for_byte() {
    let every_byte: Vec<u8> = (0..=255u8).collect();
    let inputs: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        every_byte.clone(),
        every_byte.iter().rev().copied().collect(),
        b"\x1f\x8b\x08\x00 already looks like gzip".to_vec(),
        noise(1 << 20, 7),
        vec![b'a'; 3 << 20],
    ];
    for input in inputs {
        let script = Script::new();
        script.answer(200);
        success(
            writers(&script)
                .gz(Body::new(input.clone(), "application/octet-stream"))
                .await,
        );
        let request = only_request(&script);
        assert_eq!(request.header("content-encoding"), Some("gzip"));
        let sent = sent_body(&request);
        assert_eq!(
            gunzip_one_member(&sent),
            input,
            "an input of {} bytes",
            input.len()
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c12_2_the_content_type_of_the_input_is_kept() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    success(
        writers(&script)
            .gz(Body::new("id,name\n1,a\n", "text/csv; charset=utf-8"))
            .await,
    );
    success(
        writers(&script)
            .gz_json(vec!["ready".to_owned(), "set".to_owned()])
            .await,
    );
    let requests = script.requests();
    assert_eq!(
        requests[0].header("content-type"),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        gunzip_one_member(&sent_body(&requests[0])),
        b"id,name\n1,a\n"
    );
    assert_eq!(requests[1].header("content-type"), Some("application/json"));
    assert_eq!(
        gunzip_one_member(&sent_body(&requests[1])),
        br#"["ready","set"]"#
    );
}

#[tokio::test(start_paused = true)]
async fn c12_2_a_content_type_of_the_description_wins() {
    let script = Script::new();
    script.answer(200);
    success(
        writers(&script)
            .gz_typed(Body::new("x", "text/plain"))
            .await,
    );
    let request = only_request(&script);
    assert_eq!(request.header("content-type"), Some("application/x-mine"));
    assert_eq!(gunzip_one_member(&sent_body(&request)), b"x");
}

#[tokio::test(start_paused = true)]
async fn c12_2_the_same_input_gives_the_same_bytes() {
    let input = noise(70_000, 3);
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let client = writers(&script);
    success(client.gz(Body::new(input.clone(), "a/b")).await);
    // Time passes between the two calls (on Tokio's clock and on the system's).
    tokio::time::advance(std::time::Duration::from_secs(3600)).await;
    std::thread::sleep(std::time::Duration::from_millis(1100));
    success(client.gz(Body::new(input.clone(), "a/b")).await);
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(sent_body(&requests[0]), sent_body(&requests[1]));

    // Directly: the same bytes, from every form of input.
    let a = concord::gzip(Body::new(input.clone(), "a/b")).unwrap();
    let b = concord::gzip(input.clone()).unwrap();
    let c = concord::gzip(Body::from(input)).unwrap();
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert_eq!(a.as_bytes(), c.as_bytes());
}

#[test]
fn c12_2_the_output_is_one_member_with_no_time_name_or_comment() {
    let body = concord::gzip(Body::new("hello", "text/plain")).unwrap();
    let bytes = body.as_bytes().expect("the output is bytes");
    assert_eq!(&bytes[..3], &[0x1f, 0x8b, 0x08], "gzip magic and deflate");
    assert_eq!(bytes[3] & 0x18, 0, "no file name and no comment (FLG)");
    assert_eq!(&bytes[4..8], &[0, 0, 0, 0], "no modification time (MTIME)");
    assert_eq!(gunzip_one_member(bytes), b"hello");
}

#[tokio::test(start_paused = true)]
async fn c12_2_a_stream_is_refused_without_being_polled_or_claimed() {
    let (stream, polled) = watched();
    let kept = stream.clone();
    let script = Script::new();
    script.answer(200);
    let error = failure(writers(&script).gz(stream).await);
    refused_before_sending(&script, &error);
    assert!(matches!(
        concord::GzipError::find(&error),
        Some(concord::GzipError::Stream)
    ));
    assert!(!polled.load(Ordering::SeqCst), "the stream was not polled");
    still_unclaimed(kept).await;
}

#[tokio::test(start_paused = true)]
async fn c12_2_a_body_joined_with_a_stream_is_refused_without_being_polled() {
    let (stream, polled) = watched();
    let kept = stream.clone();
    let joined = Body::concat([Body::from("head "), stream]);
    match concord::gzip(joined) {
        Ok(body) => panic!("expected a refusal, got {body:?}"),
        Err(error) => assert!(matches!(error, concord::GzipError::Stream)),
    }
    assert!(!polled.load(Ordering::SeqCst), "the stream was not polled");
    still_unclaimed(kept).await;
}

// C12.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_3_mixed_writes_each_part_with_its_headers_in_order() {
    let binary: Vec<u8> = vec![0x00, 0xff, b'\r', b'\n', b'-', b'-', 0x80, b'\r', b'\n'];
    let mut second = HeaderMap::new();
    second.insert("z-last", HeaderValue::from_static("z"));
    second.insert(
        "content-type",
        HeaderValue::from_static("application/octet-stream"),
    );
    second.append("a-first", HeaderValue::from_static("one"));
    second.append("a-first", HeaderValue::from_static("two"));
    let parts = vec![
        part("application/json", r#"{"operation":"check"}"#),
        (second, Body::from(binary.clone())),
        part("text/plain; charset=utf-8", "third\r\n"),
    ];
    let script = Script::new();
    script.answer(200);
    success(writers(&script).mixed(parts).await);

    let request = only_request(&script);
    let boundary = sent_boundary(&request, "multipart/mixed");
    assert_eq!(
        read_parts(&sent_body(&request), &boundary),
        vec![
            read(
                &[("content-type", b"application/json")],
                br#"{"operation":"check"}"#
            ),
            read(
                &[
                    ("a-first", b"one"),
                    ("a-first", b"two"),
                    ("content-type", b"application/octet-stream"),
                    ("z-last", b"z"),
                ],
                &binary
            ),
            read(
                &[("content-type", b"text/plain; charset=utf-8")],
                b"third\r\n"
            ),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_3_one_part_and_an_empty_part_are_written() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    success(writers(&script).mixed(vec![part("text/plain", "")]).await);
    success(
        writers(&script)
            .related(vec![part("application/json", "{}")])
            .await,
    );
    let requests = script.requests();
    let boundary = sent_boundary(&requests[0], "multipart/mixed");
    assert_eq!(
        read_parts(&sent_body(&requests[0]), &boundary),
        vec![read(&[("content-type", b"text/plain")], b"")]
    );
    let boundary = sent_boundary(&requests[1], "multipart/related");
    assert_eq!(
        read_parts(&sent_body(&requests[1]), &boundary),
        vec![read(&[("content-type", b"application/json")], b"{}")]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_3_the_boundary_is_the_first_candidate_absent_from_every_part_and_header() {
    let zero = candidate(0);
    let one = candidate(1);
    let cases: Vec<(Vec<(HeaderMap, Body)>, String)> = vec![
        (vec![part("text/plain", "plain")], candidate(0)),
        // A prefix of the first candidate is not the candidate.
        (
            vec![part("text/plain", zero[..zero.len() - 1].to_owned())],
            candidate(0),
        ),
        (vec![part("text/plain", format!("x{zero}x"))], candidate(1)),
        // The candidate in the last part only.
        (
            vec![part("text/plain", "a"), part("text/plain", zero.clone())],
            candidate(1),
        ),
        // The first in a body, the second in a header of another part.
        (
            vec![
                part("text/plain", zero.clone()),
                (
                    headers(&[("content-type", "text/plain"), ("x-note", &one)]),
                    Body::from("b"),
                ),
            ],
            candidate(2),
        ),
        // In a header value only.
        (
            vec![(
                headers(&[("content-type", "text/plain"), ("x-note", &zero)]),
                Body::from("b"),
            )],
            candidate(1),
        ),
    ];
    for (n, (parts, expected)) in cases.into_iter().enumerate() {
        let script = Script::new();
        script.answer(200);
        success(writers(&script).mixed(parts.clone()).await);
        let request = only_request(&script);
        let boundary = sent_boundary(&request, "multipart/mixed");
        assert_eq!(boundary, expected, "case {n}");
        let body = sent_body(&request);
        assert_eq!(
            read_parts(&body, &boundary).len(),
            parts.len(),
            "case {n}: every part is framed by the boundary"
        );
        for (_, part) in &parts {
            assert!(find(part.as_bytes().unwrap(), boundary.as_bytes()).is_none());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_3_related_names_the_first_part_media_type_without_its_parameters() {
    let parts = vec![
        part(
            "application/json; charset=utf-8",
            r#"{"image":"cid:image-1"}"#,
        ),
        (
            headers(&[("content-type", "image/png"), ("content-id", "<image-1>")]),
            Body::from(vec![137u8, 80, 78, 71]),
        ),
    ];
    let script = Script::new();
    script.answer(200);
    success(writers(&script).related(parts).await);
    let request = only_request(&script);
    let content_type = request.header("content-type").unwrap().to_owned();
    assert!(
        content_type.contains(r#"type="application/json""#),
        "a quoted type parameter: {content_type}"
    );
    let (media_type, params) = media(&content_type);
    assert_eq!(media_type, "multipart/related");
    assert_eq!(param(&params, "type").as_deref(), Some("application/json"));
    assert_eq!(param(&params, "start"), None, "no start parameter");
    let boundary = param(&params, "boundary").unwrap();
    assert_eq!(
        read_parts(&sent_body(&request), &boundary),
        vec![
            read(
                &[("content-type", b"application/json; charset=utf-8")],
                br#"{"image":"cid:image-1"}"#
            ),
            read(
                &[("content-id", b"<image-1>"), ("content-type", b"image/png")],
                &[137, 80, 78, 71]
            ),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_3_the_same_parts_give_the_same_bytes() {
    let parts = vec![
        part("application/json", "{}"),
        part("application/octet-stream", noise(5000, 11)),
    ];
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200);
    }
    let client = writers(&script);
    success(client.mixed(parts.clone()).await);
    success(client.mixed(parts.clone()).await);
    success(client.related(parts.clone()).await);
    success(client.related(parts).await);
    let requests = script.requests();
    assert_eq!(sent_body(&requests[0]), sent_body(&requests[1]));
    assert_eq!(
        requests[0].header("content-type"),
        requests[1].header("content-type")
    );
    assert_eq!(sent_body(&requests[2]), sent_body(&requests[3]));
    assert_eq!(
        requests[2].header("content-type"),
        requests[3].header("content-type")
    );
}

#[tokio::test(start_paused = true)]
async fn c12_3_header_values_are_written_as_their_bytes() {
    let mut map = headers(&[("content-type", "text/plain")]);
    map.insert(
        "x-latin",
        HeaderValue::from_bytes(&[b'c', b'a', b'f', 0xe9]).unwrap(),
    );
    let script = Script::new();
    script.answer(200);
    success(writers(&script).mixed(vec![(map, Body::from("b"))]).await);
    let request = only_request(&script);
    let boundary = sent_boundary(&request, "multipart/mixed");
    assert_eq!(
        read_parts(&sent_body(&request), &boundary),
        vec![read(
            &[
                ("content-type", b"text/plain"),
                ("x-latin", &[b'c', b'a', b'f', 0xe9])
            ],
            b"b"
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_3_the_content_type_of_a_part_body_is_ignored() {
    let script = Script::new();
    script.answer(200);
    success(
        writers(&script)
            .mixed(vec![part("application/json", Body::new("{}", "text/csv"))])
            .await,
    );
    let request = only_request(&script);
    let boundary = sent_boundary(&request, "multipart/mixed");
    assert_eq!(
        read_parts(&sent_body(&request), &boundary),
        vec![read(&[("content-type", b"application/json")], b"{}")]
    );
    // And a part whose only content type is the body's own has none.
    let error = mime_error(concord::multipart_mixed([(
        HeaderMap::new(),
        Body::new("{}", "application/json"),
    )]));
    assert!(matches!(
        error,
        concord::MimeWriteError::ContentType { part: 1, .. }
    ));
}

#[test]
fn c12_3_no_part_is_refused() {
    let none: Vec<(HeaderMap, Body)> = vec![];
    assert!(matches!(
        mime_error(concord::multipart_mixed(none.clone())),
        concord::MimeWriteError::Empty
    ));
    assert!(matches!(
        mime_error(concord::multipart_related(none)),
        concord::MimeWriteError::Empty
    ));
}

#[tokio::test(start_paused = true)]
async fn c12_3_a_stream_part_is_refused_with_its_position_without_being_polled() {
    for position in 1..=3usize {
        for related in [false, true] {
            let (stream, polled) = watched();
            let kept = stream.clone();
            let mut parts = vec![part("text/plain", "a"), part("text/plain", "b")];
            parts.insert(position - 1, part("text/plain", stream));
            let script = Script::new();
            script.answer(200);
            let error = if related {
                failure(writers(&script).related(parts).await)
            } else {
                failure(writers(&script).mixed(parts).await)
            };
            refused_before_sending(&script, &error);
            match concord::MimeWriteError::find(&error) {
                Some(concord::MimeWriteError::Stream { part, .. }) => {
                    assert_eq!(*part, position, "related: {related}")
                }
                other => panic!("expected Stream at {position}, got {other:?}"),
            }
            assert!(!polled.load(Ordering::SeqCst), "the stream was not polled");
            still_unclaimed(kept).await;
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_3_a_stream_before_a_part_that_fails_validation_is_not_claimed() {
    // Which of the two failures is reported is not said; that no source is claimed is.
    let (stream, polled) = watched();
    let kept = stream.clone();
    let parts = vec![
        part("text/plain", stream),
        (HeaderMap::new(), Body::from("no content type")),
    ];
    let error = mime_error(concord::multipart_related(parts));
    assert!(matches!(
        error,
        concord::MimeWriteError::Stream { part: 1, .. }
            | concord::MimeWriteError::ContentType { part: 2, .. }
    ));
    assert!(!polled.load(Ordering::SeqCst), "the stream was not polled");
    still_unclaimed(kept).await;
}

#[test]
fn c12_3_a_part_without_one_valid_content_type_is_refused_with_its_position() {
    let invalid: Vec<HeaderMap> = vec![
        HeaderMap::new(),
        headers(&[("x-note", "no content type")]),
        headers(&[
            ("content-type", "text/plain"),
            ("content-type", "text/plain"),
        ]),
        headers(&[
            ("content-type", "text/plain"),
            ("content-type", "text/html"),
        ]),
        headers(&[("content-type", "text")]),
        headers(&[("content-type", "text/")]),
        headers(&[("content-type", "/plain")]),
        headers(&[("content-type", "text/pla in")]),
        headers(&[("content-type", "text/plain; charset")]),
        headers(&[("content-type", "")]),
    ];
    for (n, map) in invalid.into_iter().enumerate() {
        for related in [false, true] {
            let parts = vec![
                part("text/plain", "a"),
                (map.clone(), Body::from("b")),
                part("text/plain", "c"),
            ];
            let result = if related {
                concord::multipart_related(parts)
            } else {
                concord::multipart_mixed(parts)
            };
            match result {
                Err(concord::MimeWriteError::ContentType { part, .. }) => {
                    assert_eq!(part, 2, "case {n}, related: {related}")
                }
                Err(other) => panic!("case {n}, related {related}: got {other:?}"),
                Ok(_) => panic!("case {n}, related {related}: accepted {map:?}"),
            }
        }
    }
    // Valid media types with parameters are accepted.
    for valid in [
        "text/plain; charset=utf-8",
        "application/vnd.api+json",
        r#"text/plain; name="a b; c""#,
    ] {
        assert!(
            concord::multipart_mixed([part(valid, "x")]).is_ok(),
            "{valid}"
        );
    }
}

// C12.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_4_one_compact_value_per_line_and_a_line_feed_after_each() {
    let script = Script::new();
    script.answer(200);
    success(
        writers(&script)
            .lines(
                vec![
                    json!({ "a": 1, "b": [true, null] }),
                    json!("two\nlines"),
                    json!(3),
                    json!({}),
                ],
                1024,
            )
            .await,
    );
    let request = only_request(&script);
    assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
    assert_eq!(
        String::from_utf8(sent_body(&request)).unwrap(),
        "{\"a\":1,\"b\":[true,null]}\n\"two\\nlines\"\n3\n{}\n"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_4_one_record_ends_in_a_line_feed_and_no_record_is_an_empty_body() {
    let one = concord::json_lines([json!(1)], 16).unwrap();
    assert_eq!(one.as_bytes(), Some(&b"1\n"[..]));
    let none = concord::json_lines(Vec::<Value>::new(), 16).unwrap();
    assert_eq!(none.as_bytes(), Some(&b""[..]));

    let script = Script::new();
    script.answer(200);
    success(writers(&script).lines(vec![], 16).await);
    let request = only_request(&script);
    assert_eq!(request.body().unwrap_or_default(), b"");
}

#[test]
fn c12_4_the_bound_counts_every_line_feed() {
    // "1\n": two bytes.
    assert_eq!(
        concord::json_lines([1u8], 2).unwrap().as_bytes(),
        Some(&b"1\n"[..])
    );
    assert!(matches!(
        lines_error(concord::json_lines([1u8], 1)),
        concord::JsonLinesError::TooLarge
    ));
    // "1\n2\n3\n": six bytes; the bound exactly, and one byte less.
    assert_eq!(
        concord::json_lines([1u8, 2, 3], 6).unwrap().as_bytes(),
        Some(&b"1\n2\n3\n"[..])
    );
    assert!(matches!(
        lines_error(concord::json_lines([1u8, 2, 3], 5)),
        concord::JsonLinesError::TooLarge
    ));
    // The last record alone passes the bound.
    let text = "x".repeat(100);
    assert!(concord::json_lines([&text], 103).is_ok());
    assert!(matches!(
        lines_error(concord::json_lines([&text], 102)),
        concord::JsonLinesError::TooLarge
    ));
}

#[test]
fn c12_4_zero_accepts_only_an_empty_iterator() {
    assert_eq!(
        concord::json_lines(Vec::<u8>::new(), 0).unwrap().as_bytes(),
        Some(&b""[..])
    );
    assert!(matches!(
        lines_error(concord::json_lines([0u8], 0)),
        concord::JsonLinesError::TooLarge
    ));
    assert!(matches!(
        lines_error(concord::json_lines([""], 0)),
        concord::JsonLinesError::TooLarge
    ));
}

#[test]
fn c12_4_the_largest_bound_is_accepted() {
    assert_eq!(
        concord::json_lines([1u8, 2], usize::MAX)
            .unwrap()
            .as_bytes(),
        Some(&b"1\n2\n"[..])
    );
}

#[test]
fn c12_4_an_endless_iterator_is_refused_at_its_bound() {
    let pulled = AtomicUsize::new(0);
    let endless = std::iter::repeat_with(|| {
        pulled.fetch_add(1, Ordering::SeqCst);
        7u8
    });
    assert!(matches!(
        lines_error(concord::json_lines(endless, 1000)),
        concord::JsonLinesError::TooLarge
    ));
    // "7\n" is two bytes: the bound is passed at the 501st record.
    let pulled = pulled.load(Ordering::SeqCst);
    assert!(
        (501..=502).contains(&pulled),
        "{pulled} records pulled for a bound of 500 records"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_4_one_byte_more_than_the_bound_is_refused_before_anything_is_sent() {
    // `{"n":1}\n` is eight bytes; two records are sixteen.
    let records = vec![json!({ "n": 1 }), json!({ "n": 2 })];
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    success(writers(&script).lines(records.clone(), 16).await);
    assert_eq!(script.requests().len(), 1);
    let error = failure(writers(&script).lines(records, 15).await);
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(script.requests().len(), 1, "the second call sent nothing");
    assert_eq!(script.unused(), 1);
    assert!(matches!(
        concord::JsonLinesError::find(&error),
        Some(concord::JsonLinesError::TooLarge)
    ));
}

#[test]
fn c12_4_a_record_that_does_not_serialize_is_refused_with_its_position() {
    #[derive(Serialize)]
    #[serde(untagged)]
    enum Record {
        Fine(u8),
        Broken(Refuses),
    }
    for (records, position) in [
        (vec![Record::Broken(Refuses)], 1usize),
        (
            vec![Record::Fine(1), Record::Fine(2), Record::Broken(Refuses)],
            3,
        ),
        (
            vec![Record::Fine(1), Record::Broken(Refuses), Record::Fine(3)],
            2,
        ),
    ] {
        match lines_error(concord::json_lines(records, 1024)) {
            concord::JsonLinesError::Serialize { record, source, .. } => {
                assert_eq!(record, position);
                assert!(source.source().is_some(), "the format's cause is kept");
            }
            other => panic!("expected Serialize at {position}, got {other:?}"),
        }
    }
    // A map key that JSON cannot write.
    let bad_key = BTreeMap::from([((1u8, 2u8), 3u8)]);
    let ok = BTreeMap::new();
    match lines_error(concord::json_lines([&ok, &bad_key], 1024)) {
        concord::JsonLinesError::Serialize { record, .. } => assert_eq!(record, 2),
        other => panic!("expected Serialize at 2, got {other:?}"),
    }
}

#[test]
fn c12_4_a_record_nested_beyond_the_guard_is_refused_with_its_position() {
    let records = vec![deep(3), deep(3), deep(200)];
    match lines_error(concord::json_lines(&records, usize::MAX)) {
        concord::JsonLinesError::Serialize { record, .. } => assert_eq!(record, 3),
        other => panic!("expected Serialize at 3, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn c12_4_a_content_type_of_the_description_wins() {
    let script = Script::new();
    script.answer(200);
    success(writers(&script).lines_typed(vec![json!(1)], 16).await);
    let request = only_request(&script);
    assert_eq!(request.header("content-type"), Some("application/json-seq"));
    assert_eq!(sent_body(&request), b"1\n");
}

#[test]
fn c12_4_debug_shows_the_variant_and_the_position_only() {
    let too_large = lines_error(concord::json_lines([MARK], 1));
    assert_eq!(format!("{too_large:?}"), "TooLarge");
    let broken = lines_error(concord::json_lines([deep(200)], usize::MAX));
    assert_eq!(format!("{broken:?}"), "Serialize { record: 1 }");
}

// C12.5 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_5_a_writer_failure_is_a_build_error_with_no_attempt() {
    let script = Script::new();
    script.answer(200);
    let client = writers(&script).with_retry(Retry::standard());

    let errors = vec![
        failure(client.nested(json!(["not a map"])).await),
        failure(client.nested_again(json!({ "k": { "": 1 } })).await),
        failure(client.gz(watched().0).await),
        failure(client.gz_again(watched().0).await),
        failure(client.mixed(vec![]).await),
        failure(
            client
                .mixed_again(vec![(HeaderMap::new(), Body::from("x"))])
                .await,
        ),
        failure(
            client
                .related_again(vec![part("text/plain", watched().0)])
                .await,
        ),
        failure(client.lines(vec![json!(1)], 1).await),
        failure(client.lines_again(vec![deep(200)], 1024).await),
    ];
    for (n, error) in errors.iter().enumerate() {
        assert_eq!(error.kind(), Kind::Build, "case {n}");
    }
    assert!(script.requests().is_empty(), "no attempt was made");
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_5_an_error_prints_names_and_never_values() {
    let script = Script::new();
    let client = writers(&script);

    let mut errors: Vec<concord::Error> = Vec::new();
    // A key that cannot be written, holding the mark.
    errors.push(failure(
        client
            .nested(json!({ "a": { format!("{MARK}["): MARK } }))
            .await,
    ));
    // A variant with a payload, a root that is a list holding the mark.
    errors.push(failure(client.nested(json!([MARK])).await));
    // A stream whose content type holds the mark.
    let (stream, _) = watched();
    errors.push(failure(
        client.gz(stream.content_type(format!("text/{MARK}"))).await,
    ));
    // A part with a header that holds the mark, then a stream part.
    let marked = headers(&[("content-type", "text/plain"), ("x-secret", MARK)]);
    errors.push(failure(
        client
            .mixed(vec![
                (marked.clone(), Body::from(MARK)),
                part("text/plain", watched().0),
            ])
            .await,
    ));
    // A content type that is the mark itself.
    errors.push(failure(
        client
            .related(vec![(headers(&[("content-type", MARK)]), Body::from(MARK))])
            .await,
    ));
    // Records that hold the mark, over the bound; and one of them too deep.
    errors.push(failure(client.lines(vec![json!(MARK); 4], 10).await));
    let mut too_deep = deep(200);
    too_deep["k"] = json!({ MARK: deep(199) });
    errors.push(failure(
        client.lines(vec![json!(MARK), too_deep], 1 << 20).await,
    ));

    assert!(script.requests().is_empty());
    for (n, error) in errors.iter().enumerate() {
        assert_eq!(error.kind(), Kind::Build, "case {n}");
        let lines = printed_chain(error);
        assert_no_mark(&lines);
    }
    // The writer's own error is found, and prints names only.
    let lines = printed_chain(concord::MimeWriteError::find(&errors[3]).unwrap());
    assert_eq!(
        lines[0],
        "MIME part 2 is a stream that can be sent only once"
    );
    assert_no_mark(&lines);
}

#[tokio::test(start_paused = true)]
async fn c12_5_every_writer_body_is_sent_again_unchanged_on_a_retry() {
    let parts = vec![
        part("application/json", "{}"),
        part("application/octet-stream", noise(300, 5)),
    ];
    let script = Script::new();
    for _ in 0..5 {
        script.answer(503);
        script.answer(200);
    }
    let client = writers(&script).with_retry(Retry::standard());
    success(
        client
            .nested_again(json!({ "a": { "b": ["x", "y"] } }))
            .await,
    );
    success(
        client
            .gz_again(Body::new(noise(4000, 9), "application/octet-stream"))
            .await,
    );
    success(client.mixed_again(parts.clone()).await);
    success(client.related_again(parts).await);
    success(
        client
            .lines_again(vec![json!({ "n": 1 }), json!("two")], 64)
            .await,
    );

    let requests = script.requests();
    assert_eq!(requests.len(), 10, "each call was tried twice");
    for pair in requests.chunks(2) {
        assert_eq!(pair[0].url(), pair[1].url());
        assert_eq!(
            sent_body(&pair[0]),
            sent_body(&pair[1]),
            "{}",
            pair[0].url()
        );
        assert!(!sent_body(&pair[0]).is_empty());
        assert_eq!(
            pair[0].header("content-type"),
            pair[1].header("content-type")
        );
        assert_eq!(
            pair[0].header("content-encoding"),
            pair[1].header("content-encoding")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c12_5_the_body_is_built_once_for_every_attempt_of_a_call() {
    let count = Arc::new(AtomicUsize::new(0));
    let records = vec![Counted(count.clone()), Counted(count.clone())];
    let script = Script::new();
    script.answer(503);
    script.answer(503);
    script.answer(200);
    success(
        writers(&script)
            .with_retry(Retry::standard())
            .lines_counted(records, 64)
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(sent_body(request), b"7\n7\n");
    }
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "each record was serialized once, before the first attempt"
    );
}
