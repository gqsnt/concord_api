//! Request writers through the facade, generated descriptions and scripted HTTP.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use concord::body::WriteError;
use concord::bytes::Bytes;
use concord::error::Kind;
use concord::http::{HeaderMap, HeaderValue};
use concord::{Body, Retry};
use concord_testkit::Script;
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::error::Error as _;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone, Serialize)]
struct Nested {
    z: String,
    metadata: BTreeMap<String, String>,
    items: Vec<Option<BTreeMap<String, String>>>,
    mode: Mode,
    absent: Option<u8>,
    unit: (),
    empty: Vec<u8>,
}

#[derive(Clone, Serialize)]
enum Mode {
    #[serde(rename = "send_invoice")]
    Invoice,
}

fn nested() -> Nested {
    Nested {
        z: "a +&/é".into(),
        metadata: BTreeMap::from([("order".into(), "7781".into())]),
        items: vec![
            None,
            Some(BTreeMap::from([("price".into(), "price_1".into())])),
        ],
        mode: Mode::Invoice,
        absent: None,
        unit: (),
        empty: vec![],
    }
}

#[derive(Clone, Serialize)]
enum Bad {
    Newtype(u8),
    Tuple(u8, u8),
    Fields { value: u8 },
}

#[derive(Clone)]
struct Invalid;
impl Serialize for Invalid {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("PRIVATE_SERIALIZER_MESSAGE"))
    }
}

#[derive(Clone)]
struct Deep(usize);
impl Serialize for Deep {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0 == 0 {
            serializer.serialize_str("PRIVATE_DEEP_VALUE")
        } else {
            serializer.serialize_some(&Deep(self.0 - 1))
        }
    }
}

concord::api! {
    client Writers {
        base "https://writers.test";
        POST form(value: Nested) { body concord::bracket_form(value); idempotent; }
        POST entries(metadata: BTreeMap<String, String>, mode: Mode, omitted?: String) {
            body concord::bracket_form { metadata; mode; omitted; }
            idempotent;
        }
        POST secret_entry(token: concord::Secret) { body concord::bracket_form { token; } }
        POST shape(bad: Bad) { body concord::bracket_form { bad; } }
        POST depth(deep: Deep) { body concord::bracket_form { deep; } }
        POST invalid(value: Invalid) { body concord::bracket_form { value; } }
        GET query() { query { "page" = 1; } }
        POST gzip(value: Body) { body concord::gzip(value); idempotent; headers { "content-encoding" = "gzip"; } }
        POST gzip_override(value: Body) { body concord::gzip(value); headers { "content-type" = "custom/type"; } }
        POST mixed(parts: Vec<(HeaderMap, Body)>) { body concord::multipart_mixed(parts.clone()); idempotent; }
        POST related(parts: Vec<(HeaderMap, Body)>) { body concord::multipart_related(parts.clone()); idempotent; }
        POST lines(records: Vec<serde_json::Value>, max: usize) { body concord::json_lines(records, *max); idempotent; }
        POST lines_override(records: Vec<serde_json::Value>) { body concord::json_lines(records, 100); headers { "content-type" = "custom/type"; } }
        POST lines_invalid(records: Vec<Invalid>) { body concord::json_lines(records, 1000); }
    }
}

fn client(script: &Script) -> Writers {
    Writers::new().with_transport(script.clone())
}

fn attempts(script: &Script, expected: &[u32]) {
    let requests = script.requests();
    assert_eq!(requests.len(), expected.len());
    assert_eq!(
        requests
            .iter()
            .map(|request| request.info().number())
            .collect::<Vec<_>>(),
        expected
    );
}

fn build_failure(error: &concord::Error, script: &Script) {
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    attempts(script, &[]);
}

#[test]
fn c12_1_pairs_preserve_nested_order_indices_and_renamed_variants() {
    assert_eq!(
        concord::bracket_pairs(nested()).unwrap(),
        vec![
            ("z".into(), "a +&/é".into()),
            ("metadata[order]".into(), "7781".into()),
            ("items[1][price]".into(), "price_1".into()),
            ("mode".into(), "send_invoice".into()),
        ]
    );
    assert_eq!(
        concord::bracket_form(nested()).unwrap().as_bytes().unwrap(),
        b"z=a+%2B%26%2F%C3%A9&metadata%5Border%5D=7781&items%5B1%5D%5Bprice%5D=price_1&mode=send_invoice"
    );
    assert_eq!(concord::bracket_pairs(None::<Nested>).unwrap(), vec![]);
    assert_eq!(concord::bracket_pairs(()).unwrap(), vec![]);
    assert_eq!(
        concord::bracket_form(BTreeMap::<String, u8>::new())
            .unwrap()
            .as_bytes(),
        Some(b"".as_slice())
    );
}

#[tokio::test(start_paused = true)]
async fn c12_1_entries_and_secrets_use_the_shipped_writer_directly() {
    let script = Script::new();
    script.answer(204);
    script.answer(204);
    client(&script)
        .entries(nested().metadata, Mode::Invoice)
        .await
        .unwrap();
    client(&script).secret_entry("PRIVATE_TOKEN").await.unwrap();
    attempts(&script, &[1, 1]);
    let requests = script.requests();
    assert_eq!(
        requests[0].body(),
        Some(b"metadata%5Border%5D=7781&mode=send_invoice".as_slice())
    );
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(requests[1].body(), Some(b"token=PRIVATE_TOKEN".as_slice()));
}

#[tokio::test(start_paused = true)]
async fn c12_1_runtime_query_pairs_append_once_without_reencoding() {
    let script = Script::new();
    script.answer(204);
    let pairs = concord::bracket_pairs(nested()).unwrap();
    client(&script)
        .query()
        .edit_request(move |request| {
            request.url_mut().query_pairs_mut().extend_pairs(&pairs);
        })
        .await
        .unwrap();
    attempts(&script, &[1]);
    assert_eq!(script.requests()[0].url(), "https://writers.test/?page=1&z=a+%2B%26%2F%C3%A9&metadata%5Border%5D=7781&items%5B1%5D%5Bprice%5D=price_1&mode=send_invoice");
}

#[test]
fn c12_1_unsupported_roots_keys_and_payload_variants_keep_a_typed_shape() {
    for result in [
        concord::bracket_pairs(7),
        concord::bracket_pairs(vec![1]),
        concord::bracket_pairs(BTreeMap::from([(1, 2)])),
        concord::bracket_pairs(BTreeMap::from([("", 2)])),
        concord::bracket_pairs(BTreeMap::from([("a[b]", 2)])),
        concord::bracket_pairs(BTreeMap::from([("a]", 2)])),
    ] {
        let error = result.unwrap_err();
        assert!(matches!(
            concord::BracketFormError::find(&error),
            Some(concord::BracketFormError::Shape)
        ));
        assert_eq!(error.to_string(), "the body could not be serialized");
        assert_eq!(
            error.source().unwrap().to_string(),
            "the bracketed form has an unsupported shape"
        );
    }
    for bad in [Bad::Newtype(1), Bad::Tuple(1, 2), Bad::Fields { value: 1 }] {
        assert!(concord::bracket_pairs(BTreeMap::from([("bad", bad)])).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_1_depth_failures_name_only_the_description_entry() {
    let script = Script::new();
    let error = client(&script).depth(Deep(129)).await.unwrap_err();
    build_failure(&error, &script);
    assert!(error.to_string().contains("input deep"), "{error}");
    assert!(!format!("{error} {error:?}").contains("PRIVATE_DEEP_VALUE"));
    assert!(concord::bracket_form(BTreeMap::from([("x", Deep(120))])).is_ok());
}

#[test]
fn c12_1_bracket_writers_accept_128_levels_and_refuse_129() {
    // The map entry adds one level to Deep's nested options. Callers can use
    // the whole documented nesting budget with either bracket writer.
    let at_bound = BTreeMap::from([("x", Deep(127))]);
    assert_eq!(
        concord::bracket_pairs(&at_bound).unwrap(),
        [("x".to_owned(), "PRIVATE_DEEP_VALUE".to_owned())]
    );
    assert_eq!(
        concord::bracket_form(&at_bound)
            .unwrap()
            .as_bytes()
            .unwrap(),
        b"x=PRIVATE_DEEP_VALUE"
    );
    let over_bound = BTreeMap::from([("x", Deep(128))]);
    for error in [
        concord::bracket_pairs(&over_bound).unwrap_err(),
        concord::bracket_form(&over_bound).unwrap_err(),
    ] {
        assert!(error.source().unwrap().to_string().contains("128"));
        assert!(!format!("{error} {error:?}").contains("PRIVATE_DEEP_VALUE"));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_1_shape_failures_are_refused_before_send_without_printing_values() {
    let script = Script::new();
    let error = client(&script).shape(Bad::Newtype(173)).await.unwrap_err();
    build_failure(&error, &script);
    assert!(matches!(
        concord::BracketFormError::find(&error),
        Some(concord::BracketFormError::Shape)
    ));
    assert!(!format!("{error} {error:?}").contains("173"));
    let cause = error.source().unwrap();
    assert!(!format!("{cause} {cause:?}").contains("173"));

    // Shape failures name the endpoint; only depth failures name an input.
    // Callers can identify the failing operation without exposing its values.
    assert_eq!(
        error.to_string(),
        "build error: the body could not be constructed, in writers.shape"
    );
    assert_eq!(
        cause.to_string(),
        "the bracketed form has an unsupported shape"
    );
}

#[test]
fn c12_1_scalar_tuple_byte_and_transparent_shapes_follow_serde() {
    #[derive(Serialize)]
    struct Wrapped<T>(T);
    #[derive(Serialize)]
    struct Scalars {
        yes: bool,
        signed: i128,
        unsigned: u128,
        decimal: f64,
        letter: char,
        pair: (u8, bool),
        wrapped: Wrapped<&'static str>,
    }
    let value = Scalars {
        yes: true,
        signed: -2,
        unsigned: 3,
        decimal: 1.5,
        letter: 'é',
        pair: (7, false),
        wrapped: Wrapped("hi"),
    };
    assert_eq!(concord::bracket_form(value).unwrap().as_bytes().unwrap(), b"yes=true&signed=-2&unsigned=3&decimal=1.5&letter=%C3%A9&pair%5B0%5D=7&pair%5B1%5D=false&wrapped=hi");
    struct Raw;
    impl Serialize for Raw {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&[0, 255])
        }
    }
    assert_eq!(
        concord::bracket_form(BTreeMap::from([("bytes", Raw)]))
            .unwrap()
            .as_bytes()
            .unwrap(),
        b"bytes%5B0%5D=0&bytes%5B1%5D=255"
    );
}

#[test]
fn c12_1_booleans_and_floats_use_display_and_percent_escapes_are_uppercase() {
    #[derive(Serialize)]
    struct Scalars {
        yes: bool,
        no: bool,
        decimal: f64,
        whole: f32,
        negative_zero: f64,
        nan: f64,
        positive_infinity: f64,
        negative_infinity: f64,
        #[serde(rename = "café")]
        text: &'static str,
    }
    let value = Scalars {
        yes: true,
        no: false,
        decimal: 1.5,
        whole: 2.0,
        negative_zero: -0.0,
        nan: f64::NAN,
        positive_infinity: f64::INFINITY,
        negative_infinity: f64::NEG_INFINITY,
        text: "+/é",
    };
    assert_eq!(
        concord::bracket_pairs(&value).unwrap(),
        [
            ("yes", "true"),
            ("no", "false"),
            ("decimal", "1.5"),
            ("whole", "2"),
            ("negative_zero", "-0"),
            ("nan", "NaN"),
            ("positive_infinity", "inf"),
            ("negative_infinity", "-inf"),
            ("café", "+/é"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
    );
    assert_eq!(
        concord::bracket_form(value).unwrap().as_bytes().unwrap(),
        concat!(
            "yes=true&no=false&decimal=1.5&whole=2&negative_zero=-0&nan=NaN",
            "&positive_infinity=inf&negative_infinity=-inf&caf%C3%A9=%2B%2F%C3%A9",
        )
        .as_bytes()
    );
}

fn mime_headers(media_type: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_str(media_type).unwrap());
    headers
}

fn mime_parts() -> Vec<(HeaderMap, Body)> {
    let mut first = mime_headers("application/json; charset=utf-8");
    first.append("x-note", HeaderValue::from_static("one"));
    first.append("x-note", HeaderValue::from_static("two"));
    let mut second = mime_headers("application/octet-stream");
    second.insert(
        "content-disposition",
        HeaderValue::from_bytes(b"attachment; filename=\"\xc3\xa9\"").unwrap(),
    );
    vec![
        (first, Body::new("{}", "ignored/type")),
        (second, Body::from(vec![0, 255, 13, 10])),
    ]
}

fn mime_bytes(boundary: &str) -> Vec<u8> {
    let mut bytes = format!("--{boundary}\r\ncontent-type: application/json; charset=utf-8\r\nx-note: one\r\nx-note: two\r\n\r\n{{}}\r\n--{boundary}\r\ncontent-disposition: attachment; filename=\"é\"\r\ncontent-type: application/octet-stream\r\n\r\n").into_bytes();
    bytes.extend_from_slice(&[0, 255, 13, 10]);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    bytes
}

#[tokio::test(start_paused = true)]
async fn c12_3_mime_writes_exact_framing_raw_headers_and_root_type() {
    let script = Script::new();
    script.answer(204);
    script.answer(204);
    client(&script).mixed(mime_parts()).await.unwrap();
    client(&script).related(mime_parts()).await.unwrap();
    attempts(&script, &[1, 1]);
    let boundary = "concord-00000000000000000000000000000000";
    let expected = mime_bytes(boundary);
    let requests = script.requests();
    assert_eq!(requests[0].body().unwrap(), expected);
    assert_eq!(requests[1].body().unwrap(), expected);
    assert_eq!(
        requests[0].header("content-type").unwrap(),
        format!("multipart/mixed; boundary={boundary}")
    );
    assert_eq!(
        requests[1].header("content-type").unwrap(),
        format!("multipart/related; type=\"application/json\"; boundary={boundary}")
    );
    assert_eq!(
        concord::multipart_mixed(mime_parts()).unwrap().as_bytes(),
        concord::multipart_mixed(mime_parts()).unwrap().as_bytes()
    );
    assert_eq!(
        concord::multipart_related(mime_parts()).unwrap().as_bytes(),
        concord::multipart_related(mime_parts()).unwrap().as_bytes()
    );
}

#[test]
fn c12_3_boundary_search_passes_128_collisions_in_payload_and_headers() {
    let payload: String = (0..140).map(|n| format!("concord-{n:032x}| ")).collect();
    let mut headers = mime_headers("text/plain");
    headers.insert(
        "x-collision",
        HeaderValue::from_static("concord-0000000000000000000000000000008c"),
    );
    let body = concord::multipart_mixed([(headers, Body::from(payload.clone()))]).unwrap();
    let boundary = "concord-0000000000000000000000000000008d";
    let expected = format!("--{boundary}\r\ncontent-type: text/plain\r\nx-collision: concord-0000000000000000000000000000008c\r\n\r\n{payload}\r\n--{boundary}--\r\n");
    assert_eq!(body.as_bytes().unwrap(), expected.as_bytes());
}

#[tokio::test(start_paused = true)]
async fn c12_3_missing_duplicate_and_malformed_content_types_name_the_part() {
    let mut duplicate = mime_headers("text/plain");
    duplicate.append("content-type", HeaderValue::from_static("application/json"));
    for headers in [
        HeaderMap::new(),
        duplicate,
        mime_headers("nonsense"),
        mime_headers("text/plain; bad"),
        mime_headers("text/plain; a=\"unterminated"),
    ] {
        let script = Script::new();
        let error = client(&script)
            .mixed(vec![
                (mime_headers("text/plain"), Body::from("ok")),
                (headers, Body::from("PRIVATE_PART")),
            ])
            .await
            .unwrap_err();
        build_failure(&error, &script);
        assert!(matches!(
            concord::MimeWriteError::find(&error),
            Some(concord::MimeWriteError::ContentType { part: 2, .. })
        ));
        assert!(!format!("{error} {error:?}").contains("PRIVATE_PART"));
    }
    let script = Script::new();
    let error = client(&script).related(vec![]).await.unwrap_err();
    build_failure(&error, &script);
    assert!(matches!(
        concord::MimeWriteError::find(&error),
        Some(concord::MimeWriteError::Empty)
    ));
}

fn source_stream(polls: Arc<AtomicUsize>, drops: Arc<AtomicUsize>) -> Body {
    struct Source {
        polls: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }
    impl futures_core::Stream for Source {
        type Item = Result<Bytes, std::io::Error>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::Ready(None)
        }
    }
    impl Drop for Source {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    Body::stream(Source { polls, drops })
}

#[tokio::test(start_paused = true)]
async fn c12_3_first_invalid_part_wins_over_later_validation_errors() {
    for related in [false, true] {
        for stream_first in [false, true] {
            let polls = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let source = source_stream(polls.clone(), drops.clone());
            let stream = (mime_headers("text/plain"), source.clone());
            let bad_type = (mime_headers("invalid"), Body::from("PRIVATE_PART"));
            let mut parts = vec![(mime_headers("text/plain"), Body::from("ok"))];
            if stream_first {
                parts.extend([stream, bad_type]);
            } else {
                parts.extend([bad_type, stream]);
            }
            let script = Script::new();
            let error = if related {
                client(&script).related(parts).await.unwrap_err()
            } else {
                client(&script).mixed(parts).await.unwrap_err()
            };
            build_failure(&error, &script);
            let cause = concord::MimeWriteError::find(&error).unwrap();
            if stream_first {
                assert!(matches!(
                    cause,
                    concord::MimeWriteError::Stream { part: 2, .. }
                ));
            } else {
                assert!(matches!(
                    cause,
                    concord::MimeWriteError::ContentType { part: 2, .. }
                ));
            }
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            drop(source.into_stream().unwrap());
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_3_stream_refusal_names_the_part_without_claiming_or_polling() {
    for related in [false, true] {
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let source = source_stream(polls.clone(), drops.clone());
        let parts = vec![
            (mime_headers("text/plain"), Body::from("ok")),
            (mime_headers("text/plain"), source.clone()),
        ];
        let script = Script::new();
        let error = if related {
            client(&script).related(parts).await.unwrap_err()
        } else {
            client(&script).mixed(parts).await.unwrap_err()
        };
        build_failure(&error, &script);
        assert!(matches!(
            concord::MimeWriteError::find(&error),
            Some(concord::MimeWriteError::Stream { part: 2, .. })
        ));
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(source.into_stream().unwrap());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_2_gzip_decodes_exactly_is_deterministic_and_keeps_the_media_type() {
    use std::io::Read;
    for input in [
        vec![],
        b"PRIVATE_BYTES".repeat(100),
        (0..=255).collect::<Vec<u8>>(),
        vec![7; 100_000],
    ] {
        let original = Body::new(input.clone(), "application/custom");
        let first = concord::gzip(&original).unwrap();
        let second = concord::gzip(original.clone()).unwrap();
        assert_eq!(first.as_bytes(), second.as_bytes());
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(first.as_bytes().unwrap())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, input);
        assert_eq!(&first.as_bytes().unwrap()[4..8], &[0, 0, 0, 0]);
        let script = Script::new();
        script.answer(204);
        client(&script).gzip(original).await.unwrap();
        attempts(&script, &[1]);
        let requests = script.requests();
        assert_eq!(
            requests[0].header("content-type"),
            Some("application/custom")
        );
        assert_eq!(requests[0].header("content-encoding"), Some("gzip"));
        assert_eq!(requests[0].body(), first.as_bytes());
    }
    let script = Script::new();
    script.answer(204);
    client(&script)
        .gzip_override(Body::new("payload", "original/type"))
        .await
        .unwrap();
    attempts(&script, &[1]);
    assert_eq!(
        script.requests()[0].header("content-type"),
        Some("custom/type")
    );
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_2_gzip_preserves_an_absent_content_type() {
    let input = Body::from("payload");
    let encoded = concord::gzip(&input).unwrap();
    assert_eq!(encoded.__content_type(), None);
    let script = Script::new();
    script.answer(204);
    client(&script).gzip(input).await.unwrap();
    attempts(&script, &[1]);
    let requests = script.requests();
    assert_eq!(requests[0].header("content-type"), None);
    assert_eq!(requests[0].header("content-encoding"), Some("gzip"));
    assert_eq!(requests[0].body(), encoded.as_bytes());
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_2_gzip_alone_sets_no_content_encoding() {
    let script = Script::new();
    script.answer(204);
    client(&script)
        .gzip_override(Body::new("payload", "text/plain"))
        .await
        .unwrap();
    attempts(&script, &[1]);
    assert_eq!(script.requests()[0].header("content-encoding"), None);
    assert_eq!(
        script.requests()[0].body(),
        concord::gzip(Body::new("payload", "text/plain"))
            .unwrap()
            .as_bytes()
    );
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_5_writers_start_no_task_on_success_or_failure() {
    let metrics = tokio::runtime::Handle::current().metrics();
    let before = metrics.num_alive_tasks();
    assert!(concord::bracket_pairs(nested()).is_ok());
    assert_eq!(metrics.num_alive_tasks(), before);
    assert!(concord::bracket_pairs(Invalid).is_err());
    assert_eq!(metrics.num_alive_tasks(), before);

    for result in [
        concord::bracket_form(nested()).map_err(|_| ()),
        concord::bracket_form(Invalid).map_err(|_| ()),
        concord::gzip(Body::from("payload")).map_err(|_| ()),
        concord::gzip(source_stream(Arc::default(), Arc::default())).map_err(|_| ()),
        concord::multipart_mixed(mime_parts()).map_err(|_| ()),
        concord::multipart_mixed(vec![]).map_err(|_| ()),
        concord::multipart_related(mime_parts()).map_err(|_| ()),
        concord::multipart_related(vec![]).map_err(|_| ()),
        concord::json_lines([1], 2).map_err(|_| ()),
        concord::json_lines([1], 1).map_err(|_| ()),
    ] {
        drop(result);
        assert_eq!(metrics.num_alive_tasks(), before);
    }

    // Observe the runtime while a writer's call is pending, as well as after
    // synchronous serialization: the caller drives all of its work.
    let script = Script::new();
    script.answer(204).after(std::time::Duration::from_secs(2));
    let api = client(&script);
    let (result, during) = tokio::join!(api.form(nested()).send(), async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert_eq!(script.requests().len(), 1);
        metrics.num_alive_tasks()
    });
    result.unwrap();
    assert_eq!(during, before);
    assert_eq!(metrics.num_alive_tasks(), before);
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_2_gzip_refuses_streams_without_claiming_polling_or_leaving_work() {
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = source_stream(polls.clone(), drops.clone());
    let script = Script::new();
    let error = client(&script).gzip(source.clone()).await.unwrap_err();
    build_failure(&error, &script);
    assert!(matches!(
        concord::GzipError::find(&error),
        Some(concord::GzipError::Stream)
    ));
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(source.into_stream().unwrap());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_4_json_lines_has_a_final_lf_and_description_media_type_wins() {
    let values = vec![
        serde_json::json!({"index": {"_id":"42"}}),
        serde_json::json!({"title":"a\nb"}),
        serde_json::Value::Null,
    ];
    let expected = b"{\"index\":{\"_id\":\"42\"}}\n{\"title\":\"a\\nb\"}\nnull\n";
    let body = concord::json_lines(&values, expected.len()).unwrap();
    assert_eq!(body.as_bytes().unwrap(), expected);
    assert_eq!(
        concord::json_lines(Vec::<u8>::new(), 0)
            .unwrap()
            .as_bytes()
            .unwrap(),
        b""
    );
    let script = Script::new();
    script.answer(204);
    script.answer(204);
    client(&script)
        .lines(values.clone(), expected.len())
        .await
        .unwrap();
    client(&script).lines_override(values).await.unwrap();
    attempts(&script, &[1, 1]);
    assert_eq!(script.requests()[0].body().unwrap(), expected);
    assert_eq!(
        script.requests()[0].header("content-type"),
        Some("application/x-ndjson")
    );
    assert_eq!(
        script.requests()[1].header("content-type"),
        Some("custom/type")
    );
}

#[tokio::test(start_paused = true)]
async fn c12_4_the_total_bound_includes_lf_and_rejects_one_more_byte_before_send() {
    for (records, max) in [
        (vec![serde_json::json!(1)], 0),
        (vec![serde_json::json!(1)], 1),
        (vec![serde_json::json!("large")], 7),
        (vec![serde_json::json!(1), serde_json::json!(2)], 3),
    ] {
        let script = Script::new();
        let error = client(&script).lines(records, max).await.unwrap_err();
        build_failure(&error, &script);
        assert!(matches!(
            concord::JsonLinesError::find(&error),
            Some(concord::JsonLinesError::TooLarge)
        ));
    }
    let seen = Arc::new(AtomicUsize::new(0));
    let seen_in_iterator = seen.clone();
    let lazy = std::iter::repeat_with(move || {
        seen_in_iterator.fetch_add(1, Ordering::SeqCst);
        1
    });
    assert!(matches!(
        concord::json_lines(lazy, 4),
        Err(concord::JsonLinesError::TooLarge)
    ));
    assert_eq!(seen.load(Ordering::SeqCst), 3);
    assert!(matches!(
        concord::json_lines(["x".repeat(100_000)], 32),
        Err(concord::JsonLinesError::TooLarge)
    ));
}

#[test]
fn c12_4_serialization_errors_keep_their_position_and_original_format_cause() {
    #[derive(Serialize)]
    #[serde(untagged)]
    enum Record {
        Good(u8),
        Bad(Invalid),
    }
    let error = concord::json_lines([Record::Good(1), Record::Bad(Invalid)], 100).unwrap_err();
    assert!(matches!(
        error,
        concord::JsonLinesError::Serialize { record: 2, .. }
    ));
    let writer = error
        .source()
        .unwrap()
        .downcast_ref::<WriteError>()
        .unwrap();
    let format = writer
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .unwrap();
    assert_eq!(format.to_string(), "PRIVATE_SERIALIZER_MESSAGE");
    assert!(!format!("{error} {error:?}").contains("PRIVATE_SERIALIZER_MESSAGE"));
    let depth = concord::json_lines([Deep(129)], 1000).unwrap_err();
    assert!(matches!(
        depth,
        concord::JsonLinesError::Serialize { record: 1, .. }
    ));
}

#[test]
fn c12_4_first_failing_record_wins_and_the_iterator_stops_there() {
    struct Record<'a> {
        value: Option<u8>,
        serialized: &'a AtomicUsize,
    }
    impl Serialize for Record<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.serialized.fetch_add(1, Ordering::SeqCst);
            match self.value {
                Some(value) => serializer.serialize_u8(value),
                None => Invalid.serialize(serializer),
            }
        }
    }
    for (values, max, expected_pulls, failed_record) in [
        (vec![Some(123), None], 2, 1, None),
        (vec![Some(1), Some(22), None], 3, 2, None),
        (vec![Some(1), Some(22), None], 4, 2, None),
        (vec![Some(1), None, Some(22)], 2, 2, Some(2)),
        (vec![None, Some(123)], 0, 1, Some(1)),
    ] {
        let pulls = AtomicUsize::new(0);
        let serialized = AtomicUsize::new(0);
        let records = values.into_iter().map(|value| {
            pulls.fetch_add(1, Ordering::SeqCst);
            Record {
                value,
                serialized: &serialized,
            }
        });
        let error = concord::json_lines(records, max).unwrap_err();
        match failed_record {
            Some(expected) => assert!(matches!(
                error,
                concord::JsonLinesError::Serialize { record, .. } if record == expected
            )),
            None => assert!(matches!(error, concord::JsonLinesError::TooLarge)),
        }
        assert_eq!(pulls.load(Ordering::SeqCst), expected_pulls);
        assert_eq!(serialized.load(Ordering::SeqCst), expected_pulls);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_5_script_records_use_the_documented_request_type() {
    let script = Script::new();
    script.answer(204);
    client(&script)
        .lines(vec![serde_json::json!({"ready": true})], 64)
        .await
        .unwrap();
    let requests: Vec<concord_testkit::script::Request> = script.requests();
    attempts(&script, &[1]);
    assert_eq!(requests[0].body(), Some(b"{\"ready\":true}\n".as_slice()));
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-ndjson")
    );
}

#[tokio::test(start_paused = true)]
async fn c12_5_serialization_errors_are_build_errors_with_zero_attempts_and_safe_text() {
    let script = Script::new();
    let form = client(&script).invalid(Invalid).await.unwrap_err();
    build_failure(&form, &script);
    assert!(form
        .source()
        .unwrap()
        .downcast_ref::<concord::body::WriteError>()
        .is_none());
    assert!(form
        .source()
        .unwrap()
        .downcast_ref::<serde_urlencoded::ser::Error>()
        .is_some());
    assert_eq!(
        form.source().unwrap().to_string(),
        "PRIVATE_SERIALIZER_MESSAGE"
    );
    let lines = client(&script)
        .lines_invalid(vec![Invalid])
        .await
        .unwrap_err();
    build_failure(&lines, &script);
    assert!(matches!(
        concord::JsonLinesError::find(&lines),
        Some(concord::JsonLinesError::Serialize { record: 1, .. })
    ));
    for error in [&form, &lines] {
        assert!(!format!("{error} {error:?}").contains("PRIVATE_SERIALIZER_MESSAGE"));
    }
}

#[cfg(feature = "gzip-body")]
#[tokio::test(start_paused = true)]
async fn c12_5_only_the_engine_retries_identical_buffered_writer_bodies() {
    let script = Script::new();
    for _ in 0..5 {
        script.answer(503);
        script.answer(204);
    }
    let api = client(&script).with_retry(Retry::standard());
    api.form(nested()).await.unwrap();
    api.gzip(Body::new("private", "text/plain")).await.unwrap();
    api.mixed(mime_parts()).await.unwrap();
    api.related(mime_parts()).await.unwrap();
    api.lines(vec![serde_json::json!({"a":1})], 100)
        .await
        .unwrap();
    attempts(&script, &[1, 2, 1, 2, 1, 2, 1, 2, 1, 2]);
    for pair in script.requests().as_chunks::<2>().0 {
        assert_eq!(pair[0].body(), pair[1].body());
        assert_eq!(pair[0].headers(), pair[1].headers());
    }
    assert_eq!(script.unused(), 0);
    let once = Script::new();
    once.answer(503);
    client(&once).form(nested()).await.unwrap_err();
    attempts(&once, &[1]);
}

#[test]
fn c12_5_errors_find_wrapped_causes_without_printing_or_losing_them() {
    let shape = concord::bracket_form(7).unwrap_err();
    assert!(concord::BracketFormError::find(&shape).is_some());
    #[derive(Debug)]
    struct Wrapped(Box<dyn std::error::Error + Send + Sync>);
    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("wrapped failure")
        }
    }
    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.0.as_ref())
        }
    }
    let lines = concord::json_lines([Invalid], 100).unwrap_err();
    let wrapper = Wrapped(Box::new(lines));
    assert!(concord::JsonLinesError::find(&wrapper).is_some());
    let mime = concord::multipart_mixed(Vec::<(HeaderMap, Body)>::new()).unwrap_err();
    assert_eq!(mime.to_string(), "the MIME body has no parts");
    assert_eq!(format!("{mime:?}"), "Empty");
    assert!(mime.source().is_none());
    assert!(concord::MimeWriteError::find(&Wrapped(Box::new(mime))).is_some());
}

#[tokio::test(start_paused = true)]
async fn c12_5_dropping_an_unsent_call_leaves_no_source_or_attempt() {
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let script = Script::new();
    let call = client(&script).gzip(source_stream(polls.clone(), drops.clone()));
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    drop(call);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    attempts(&script, &[]);
}

#[test]
fn c12_4_a_size_failure_drops_its_lazy_iterator_without_consuming_the_rest() {
    struct Records {
        pulls: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }
    impl Iterator for Records {
        type Item = u8;
        fn next(&mut self) -> Option<u8> {
            self.pulls.fetch_add(1, Ordering::SeqCst);
            Some(1)
        }
    }
    impl Drop for Records {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    let pulls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let error = concord::json_lines(
        Records {
            pulls: pulls.clone(),
            drops: drops.clone(),
        },
        2,
    )
    .unwrap_err();
    assert!(matches!(error, concord::JsonLinesError::TooLarge));
    assert_eq!(pulls.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
