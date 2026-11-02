//! Body and response formats: one or more tests per row of `v4/native-repr/CONTRACT.md` (rows
//! 1–12e and the lexical precedence table), on the wire through the loopback server. Compile-fail
//! rows are trybuild cases (`tests/ui/format_*.rs`, `tests/ui/representation_*.rs`,
//! `tests/ui_json/`, `tests/ui_features/`); the generated rustdoc (12e) is checked by the
//! `concord-macros` unit tests; `form` has its own suite (`tests/forms.rs`, feature `form`).
#![deny(warnings)]

mod support;

use concord::bytes::Bytes;
use concord::reqwest::header::HeaderValue;
use concord::{BodyError, Decode, Encode, ErrorKind, reqwest};
use serde::{Deserialize, Serialize};
use support::{Recorded, Reply, Server};

/// An item.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Item {
    /// Identifier.
    pub id: u64,
}

/// A body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewItem {
    /// Name.
    pub name: String,
}

/// A discount; `Option<Discount>` as a required body is JSON `null` when `None`.
#[derive(Debug, Clone, Serialize)]
pub struct Discount {
    /// Percentage.
    pub percent: u8,
}

/// A merge patch.
#[derive(Debug, Clone, Serialize)]
pub struct Patch {
    /// New name, or `null` to remove it.
    pub name: Option<String>,
}

/// A payload whose name contains `Json`: nothing is read from it.
#[derive(Debug, PartialEq, Deserialize)]
pub struct JsonDoc {
    /// Content.
    pub doc: String,
}

/// Row 5: a payload alias of `String`.
pub type Raw = String;
/// Row 5: a payload alias of `()`.
pub type Unit = ();

/// Row 5: a format through a type alias (`type Wire = concord::Text`), named by path.
pub mod formats {
    /// Text under another name: behaves as `text`.
    pub type Wire = concord::Text;
}

/// Row 9: a downstream format, named by its path on an endpoint and in a default.
pub mod codecs {
    use concord::bytes::Bytes;
    use concord::{BodyError, Decode, Encode, reqwest};

    /// `a,b` lines: one row per line.
    pub struct Csv;

    impl Encode<Vec<(String, u32)>> for Csv {
        const CONTENT_TYPE: Option<&'static str> = Some("text/csv");

        fn encode(rows: Vec<(String, u32)>) -> Result<reqwest::Body, BodyError> {
            let text: Vec<String> = rows.iter().map(|(k, v)| format!("{k},{v}")).collect();
            Ok(reqwest::Body::from(text.join("\n")))
        }
    }

    impl Decode<Vec<(String, u32)>> for Csv {
        fn decode(body: Bytes) -> Result<Vec<(String, u32)>, BodyError> {
            let text = String::from_utf8(body.into()).map_err(|_| BodyError::new("not UTF-8"))?;
            text.lines()
                .map(|line| {
                    let (k, v) = line.split_once(',').ok_or(BodyError::new("not a row"))?;
                    let v = v.parse().map_err(|_| BodyError::new("not a number"))?;
                    Ok((k.to_owned(), v))
                })
                .collect()
        }
    }

    /// A typed unit through a custom format: the body must be `ok`.
    impl Decode<()> for Csv {
        fn decode(body: Bytes) -> Result<(), BodyError> {
            if body.as_ref() == b"ok" {
                Ok(())
            } else {
                Err(BodyError::new("expected `ok`"))
            }
        }
    }
}

/// Rows 8f/8g: a format with its own constant media type.
pub struct Private;

impl Encode<String> for Private {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-private");

    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(value.into())
    }
}

/// Row 8g: a format without a media type.
pub struct Untyped;

impl Encode<String> for Untyped {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(value.into())
    }
}

/// Raw bytes of every `content-type` line on the wire.
fn content_types(r: &Recorded) -> Vec<Vec<u8>> {
    r.header_bytes_all("content-type")
        .into_iter()
        .map(<[u8]>::to_vec)
        .collect()
}

fn ct(value: &str) -> Vec<Vec<u8>> {
    vec![value.as_bytes().to_vec()]
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

concord::api! {
    /// A mostly-JSON client: defaults once, exceptions written as endpoint statements.
    pub client Shop {
        base "http://shop.invalid";
        request json;
        response json;

        GET item(id: u64) -> Item { path "items/{id}"; }
        PUT discount(body d: Option<Discount>) { path "discount"; }
        POST search(body q?: NewItem) -> Vec<Item> { path "search"; }
        PUT note(body note: String) {
            request text;
            path "note";
        }
        PUT blob(body b: Bytes) {
            request bytes;
            path "blob";
        }
        DELETE remove(id: u64) { path "items/{id}"; }
        GET ping() -> () { path "ping"; }
        GET ping_json() -> () {
            response json;
            path "ping";
        }
        GET version() -> String { path "version"; }
        GET readme() -> String {
            response text;
            path "readme";
        }
        PATCH patch(id: u64, body p: Patch) -> Item {
            request json "application/merge-patch+json";
            path "items/{id}";
        }
    }
}

async fn shop(reply: Reply) -> (Server, Shop) {
    let server = Server::reply(reply).await;
    let api = Shop::new(http()).with_base_url(server.url()).unwrap();
    (server, api)
}

/// Row 1: an endpoint without a body under a `request` default has no body, and the format adds
/// no `Content-Type`.
#[tokio::test]
async fn row1_no_body_under_a_request_default() {
    let (server, api) = shop(Reply::json(200, r#"{"id":7}"#)).await;
    assert_eq!(api.item(7).send().await.unwrap(), Item { id: 7 });
    let r = server.single();
    assert!(r.body.is_empty());
    assert!(content_types(&r).is_empty(), "{r:?}");
}

/// Row 2a: a required `Option` body is JSON `null`, with the format's `Content-Type`.
#[tokio::test]
async fn row2a_required_option_body_is_json_null() {
    let (server, api) = shop(Reply::new(204)).await;
    api.discount(None).send().await.unwrap();
    let r = server.single();
    assert_eq!(r.body, b"null");
    assert_eq!(content_types(&r), ct("application/json"));
}

/// Row 2b: an absent optional body (never set, or `maybe_q(None)`) sends no body and no
/// `Content-Type`; a set one sends both.
#[tokio::test]
async fn row2b_absent_optional_body_sends_nothing() {
    let (server, api) = shop(Reply::json(200, "[]")).await;
    api.search().send().await.unwrap();
    api.search().maybe_q(None).send().await.unwrap();
    api.search()
        .set_q(NewItem { name: "x".into() })
        .send()
        .await
        .unwrap();
    let r = server.requests();
    for absent in &r[..2] {
        assert!(absent.body.is_empty());
        assert!(content_types(absent).is_empty(), "{absent:?}");
    }
    assert_eq!(r[2].body, br#"{"name":"x"}"#);
    assert_eq!(content_types(&r[2]), ct("application/json"));
}

/// Row 2c: an empty `text` or `bytes` body is a present body: the format's `Content-Type` is sent
/// (hyper sends no `content-length: 0` for an empty body, so the media type is the difference).
#[tokio::test]
async fn row2c_empty_text_and_bytes_bodies_are_present() {
    let (server, api) = shop(Reply::new(204)).await;
    api.note("").send().await.unwrap();
    api.blob(Bytes::new()).send().await.unwrap();
    let r = server.requests();
    assert!(r[0].body.is_empty() && r[1].body.is_empty());
    assert_eq!(content_types(&r[0]), ct("text/plain; charset=utf-8"));
    assert_eq!(content_types(&r[1]), ct("application/octet-stream"));
    let prepared = api.note("").prepare().unwrap();
    assert_eq!(
        prepared.request().body().and_then(|b| b.as_bytes()),
        Some(&b""[..])
    );
}

/// Row 3a: no `->` under a `response` default: any 2xx succeeds (an empty 204, a non-JSON 200),
/// the body is drained, the output is `()`; the default does not apply. A non-2xx is still a
/// status error.
#[tokio::test]
async fn row3a_no_output_drains_under_a_response_default() {
    for reply in [
        Reply::new(204),
        Reply::text(200, "not json"),
        Reply::json(200, r#"{"deleted":true}"#),
    ] {
        let (_server, api) = shop(reply).await;
        let () = api.remove(1).send().await.unwrap();
    }
    let (_server, api) = shop(Reply::text(404, "gone")).await;
    let error = api.remove(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
}

/// Rows 3b/3c: `-> ()` with the inherited format and `-> ()` with `response json;` on the endpoint
/// decode a typed unit: JSON
/// `null` is accepted, an empty 204 is a decode error — unlike no `->` (row 3a).
#[tokio::test]
async fn row3b_3c_typed_unit_is_decoded() {
    let (_server, api) = shop(Reply::json(200, "null")).await;
    api.ping().send().await.unwrap();
    api.ping_json().send().await.unwrap();
    let (_server, api) = shop(Reply::new(204)).await;
    for error in [
        api.ping().send().await.unwrap_err(),
        api.ping_json().send().await.unwrap_err(),
    ] {
        assert_eq!(error.kind(), ErrorKind::Decode);
        assert!(
            error.to_string().contains("unexpected end of input"),
            "{error}"
        );
    }
    let () = api.remove(1).send().await.unwrap();
}

/// Rows 4a/4b: `-> String` under `response json;` is a JSON string; with `response text;` on the
/// endpoint it is raw text.
#[tokio::test]
async fn row4_json_string_versus_raw_text() {
    let (_server, api) = shop(Reply::json(200, r#""2.4.1""#)).await;
    assert_eq!(api.version().send().await.unwrap(), "2.4.1");
    assert_eq!(api.readme().send().await.unwrap(), r#""2.4.1""#);
    let (_server, api) = shop(Reply::text(200, "2.4.1")).await;
    assert_eq!(api.readme().send().await.unwrap(), "2.4.1");
    let error = api.version().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

/// Row 6b / 8a: an endpoint `request` statement with a media type replaces the inherited format
/// and media type for that body; the media type is sent exactly once, with the body.
#[tokio::test]
async fn row6b_in_place_exception_with_media_type() {
    let (server, api) = shop(Reply::json(200, r#"{"id":3}"#)).await;
    let item = api.patch(3, Patch { name: None }).send().await.unwrap();
    assert_eq!(item, Item { id: 3 });
    let r = server.single();
    assert_eq!(r.body, br#"{"name":null}"#);
    assert_eq!(content_types(&r), ct("application/merge-patch+json"));
}

concord::api! {
    /// Row 5: aliases and spellings. Behaviour follows the written format and the defaults only.
    pub client Aliases {
        base "http://aliases.invalid";
        request json;
        response json;

        GET raw() -> Raw { path "raw"; }
        GET raw_text() -> Raw {
            response formats::Wire;
            path "raw";
        }
        GET unit() -> Unit { path "unit"; }
        GET doc() -> JsonDoc { path "doc"; }
        PUT json_body(body b: Raw) { path "b"; }
        PUT text_body(body b: Raw) {
            request formats::Wire;
            path "b";
        }
    }
}

/// Row 5: `type Raw = String` is a JSON string under the default and raw text through
/// `formats::Wire` (an alias of `concord::Text`); `type Unit = ()` decodes like `-> ()`; a
/// payload named `JsonDoc` is an ordinary JSON payload.
#[tokio::test]
async fn row5_aliases_follow_the_written_format() {
    let server = Server::start(|r| match r.path() {
        "/raw" => Reply::json(200, r#""v""#),
        "/unit" => Reply::json(200, "null"),
        "/doc" => Reply::json(200, r#"{"doc":"d"}"#),
        _ => Reply::new(204),
    })
    .await;
    let api = Aliases::new(http()).with_base_url(server.url()).unwrap();
    assert_eq!(api.raw().send().await.unwrap(), "v");
    assert_eq!(api.raw_text().send().await.unwrap(), r#""v""#);
    api.unit().send().await.unwrap();
    assert_eq!(api.doc().send().await.unwrap().doc, "d");
    api.json_body("x").send().await.unwrap();
    api.text_body("x").send().await.unwrap();
    let r = server.requests();
    assert_eq!(
        (r[4].body.as_slice(), content_types(&r[4])),
        (&b"\"x\""[..], ct("application/json"))
    );
    assert_eq!(
        (r[5].body.as_slice(), content_types(&r[5])),
        (&b"x"[..], ct("text/plain; charset=utf-8"))
    );

    let empty = Server::reply(Reply::new(204)).await;
    let api = Aliases::new(http()).with_base_url(empty.url()).unwrap();
    assert_eq!(
        api.unit().send().await.unwrap_err().kind(),
        ErrorKind::Decode
    );
}

/// The marker types named by path in a statement (`format Json;`) behave exactly like the words
/// `json` / `text` (review F3a); an absolute path works in either direction (review F3d).
mod migrated {
    use super::{Item, NewItem};
    use concord::{Json, Text};

    concord::api! {
        pub client Migrated {
            base "http://migrated.invalid";
            POST by_path(body item: NewItem) -> Item {
                format Json;
                path "p";
            }
            POST by_word(body item: NewItem) -> Item {
                format json;
                path "w";
            }
            PUT note(body n: String) -> String {
                format Text;
                path "n";
            }
            // An absolute path in either direction (review F3d: the body form was rejected).
            PUT absolute(body n: String) -> String {
                request ::concord::Text;
                response ::concord::Text;
                path "n";
            }
        }
    }
}

#[tokio::test]
async fn migrated_marker_paths_behave_like_the_words() {
    let server = Server::start(|r| match r.path() {
        "/n" => Reply::new(200).body("raw"),
        _ => Reply::json(200, r#"{"id":3}"#),
    })
    .await;
    let api = migrated::Migrated::new(http())
        .with_base_url(server.url())
        .unwrap();
    let new = || NewItem { name: "n".into() };
    assert_eq!(
        api.by_path(new()).send().await.unwrap(),
        api.by_word(new()).send().await.unwrap()
    );
    assert_eq!(api.note("x").send().await.unwrap(), "raw");
    assert_eq!(api.absolute("y").send().await.unwrap(), "raw");
    let r = server.requests();
    assert_eq!(
        (r[0].body.as_slice(), content_types(&r[0])),
        (r[1].body.as_slice(), content_types(&r[1]))
    );
    assert_eq!(content_types(&r[2]), ct("text/plain; charset=utf-8"));
    assert_eq!(
        (r[3].body.as_slice(), content_types(&r[3])),
        (&b"y"[..], ct("text/plain; charset=utf-8"))
    );
}

concord::api! {
    /// Row 6a: request and response defaults are independent; the nearest owner wins; a scope
    /// without its own default inherits (there is no clearing, row 6d).
    pub client Nested {
        base "http://nested.invalid";
        request json;
        response json;

        POST create(body b: NewItem) -> Item { path "create"; }
        scope legacy {
            response text;
            POST echo(body b: NewItem) -> String { path "echo"; }
            scope deeper {
                request text;
                POST note(body n: String) -> String { path "note"; }
            }
        }
        scope plain {
            GET version() -> String { path "version"; }
        }
    }
}

/// Row 6a / 6d.
#[tokio::test]
async fn row6a_nearest_owner_per_direction() {
    let server = Server::start(|r| match r.path() {
        "/create" => Reply::json(200, r#"{"id":1}"#),
        "/version" => Reply::json(200, r#""1.0""#),
        _ => Reply::text(200, "raw"),
    })
    .await;
    let api = Nested::new(http()).with_base_url(server.url()).unwrap();
    let new = || NewItem { name: "n".into() };
    assert_eq!(api.create(new()).send().await.unwrap(), Item { id: 1 });
    assert_eq!(api.legacy().echo(new()).send().await.unwrap(), "raw");
    assert_eq!(
        api.legacy().deeper().note("hi").send().await.unwrap(),
        "raw"
    );
    assert_eq!(api.plain().version().send().await.unwrap(), "1.0");
    let r = server.requests();
    assert_eq!(content_types(&r[0]), ct("application/json"));
    assert_eq!(content_types(&r[1]), ct("application/json"));
    assert_eq!(r[2].body, b"hi");
    assert_eq!(content_types(&r[2]), ct("text/plain; charset=utf-8"));
}

concord::api! {
    /// Row 6c: an endpoint's own statements, the nearest per direction. `format F;` sets both
    /// directions and needs only one of them to apply; `request` and `response` are independent.
    pub client OnTheEndpoint {
        base "http://endpoint.invalid";
        request json;
        response json;

        POST both(body b: String) -> String {
            format text;
            path "both";
        }
        POST only_body(body b: String) {
            format text;
            path "only-body";
        }
        GET only_output() -> String {
            format text;
            path "only-output";
        }
        POST request_only(body b: NewItem) -> Item {
            request json "application/vnd.x+json";
            path "request-only";
        }
        POST response_only(body b: NewItem) -> String {
            response text;
            path "response-only";
        }
        scope inner {
            request bytes;
            response bytes;
            POST swapped(body b: String) -> String {
                request text;
                response text;
                path "swapped";
            }
            POST inherited(body b: Vec<u8>) -> Vec<u8> { path "inherited"; }
        }
    }
}

/// Row 6c: `format text;` on an endpoint replaces the client's `json` in both directions (body and
/// output), and applies to whichever of the two the endpoint has.
#[tokio::test]
async fn row6c_endpoint_format_sets_both_directions() {
    let server = Server::start(|r| match r.path() {
        "/both" | "/only-output" => Reply::text(200, "raw"),
        _ => Reply::new(204),
    })
    .await;
    let api = OnTheEndpoint::new(http())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(api.both("x").send().await.unwrap(), "raw");
    api.only_body("y").send().await.unwrap();
    assert_eq!(api.only_output().send().await.unwrap(), "raw");
    let r = server.requests();
    assert_eq!(
        (r[0].body.as_slice(), content_types(&r[0])),
        (&b"x"[..], ct("text/plain; charset=utf-8"))
    );
    assert_eq!(
        (r[1].body.as_slice(), content_types(&r[1])),
        (&b"y"[..], ct("text/plain; charset=utf-8"))
    );
    assert!(r[2].body.is_empty() && content_types(&r[2]).is_empty());
}

/// Row 6c: an endpoint's `request` and `response` statements are independent of each other and of
/// the owners' statements: the direction without a statement of its own keeps the client's, and an
/// endpoint statement wins over the scope's in its direction only.
#[tokio::test]
async fn row6c_endpoint_statements_override_per_direction() {
    let server = Server::start(|r| match r.path() {
        "/request-only" => Reply::json(200, r#"{"id":5}"#),
        "/response-only" => Reply::text(200, "raw"),
        "/swapped" => Reply::text(200, "raw"),
        _ => Reply::new(200).body("bin"),
    })
    .await;
    let api = OnTheEndpoint::new(http())
        .with_base_url(server.url())
        .unwrap();
    let new = || NewItem { name: "n".into() };
    // The response keeps the client's `json`, the request has its own format and media type.
    assert_eq!(
        api.request_only(new()).send().await.unwrap(),
        Item { id: 5 }
    );
    // The request keeps the client's `json`, the response has its own format.
    assert_eq!(api.response_only(new()).send().await.unwrap(), "raw");
    // The endpoint's statements win over the scope's `bytes` in both directions.
    assert_eq!(api.inner().swapped("s").send().await.unwrap(), "raw");
    // Without statements of its own the endpoint takes the scope's.
    assert_eq!(
        api.inner().inherited(vec![1, 2]).send().await.unwrap(),
        b"bin"
    );
    let r = server.requests();
    assert_eq!(r[0].body, br#"{"name":"n"}"#);
    assert_eq!(content_types(&r[0]), ct("application/vnd.x+json"));
    assert_eq!(r[1].body, br#"{"name":"n"}"#);
    assert_eq!(content_types(&r[1]), ct("application/json"));
    assert_eq!(
        (r[2].body.as_slice(), content_types(&r[2])),
        (&b"s"[..], ct("text/plain; charset=utf-8"))
    );
    assert_eq!(
        (r[3].body.as_slice(), content_types(&r[3])),
        (&[1u8, 2][..], ct("application/octet-stream"))
    );
}

concord::api! {
    /// Row 8a: the body's media type: a default's media type reached through `body`, an endpoint
    /// `request json "…";` with a media type of its own, and an endpoint statement without a
    /// media type that replaces the default's format and media type.
    pub client Media {
        base "http://media.invalid";
        request json "application/vnd.api+json";

        POST create(body b: NewItem) { path "a"; }
        POST problem(body b: NewItem) {
            request json "application/problem+json";
            path "b";
        }
        POST plain(body b: NewItem) {
            request json;
            path "c";
        }
        POST note(body n: String) {
            request text;
            path "d";
        }
        POST maybe(body b?: NewItem) { path "e"; }
        POST untyped(body b: String) {
            request Untyped;
            path "f";
        }
    }
}

/// Rows 8a and 8g (a format without a media type sends none).
#[tokio::test]
async fn row8a_body_media_types() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Media::new(http()).with_base_url(server.url()).unwrap();
    let new = || NewItem { name: "n".into() };
    api.create(new()).send().await.unwrap();
    api.problem(new()).send().await.unwrap();
    api.plain(new()).send().await.unwrap();
    api.note("x").send().await.unwrap();
    api.maybe().send().await.unwrap();
    api.untyped("u").send().await.unwrap();
    let r = server.requests();
    assert_eq!(content_types(&r[0]), ct("application/vnd.api+json"));
    assert_eq!(content_types(&r[1]), ct("application/problem+json"));
    assert_eq!(content_types(&r[2]), ct("application/json"));
    assert_eq!(content_types(&r[3]), ct("text/plain; charset=utf-8"));
    assert!(content_types(&r[4]).is_empty() && r[4].body.is_empty());
    assert!(content_types(&r[5]).is_empty());
    assert_eq!(r[5].body, b"u");
    for r in &r[..3] {
        assert_eq!(r.body, br#"{"name":"n"}"#);
    }
}

concord::api! {
    /// Row 8b: `content-type` header contributions and an implicit body media type.
    pub client Header {
        base "http://header.invalid";
        POST up(header("content-type") ct?: HeaderValue, body b?: Vec<u8>) {
            request bytes;
            path "up";
        }
    }
}

concord::api! {
    /// Row 8b: an `each` contribution and a text contribution that cannot be converted.
    pub client HeaderEach(header("content-type") each cts?: Vec<HeaderValue>) {
        base "http://header.invalid";
        request text;

        POST up(body b: String) { path "up"; }
        scope typed(header("content-type") ct: String) {
            headers { remove "content-type"; }
            POST up(body b: String) { path "up"; }
        }
    }
}

/// Row 8b, the four runtime cases: no body + no header → none; no body + header → the header;
/// body + no header → the format's media type; body + header → the header (sent once).
#[tokio::test]
async fn row8b_four_runtime_cases() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Header::new(http()).with_base_url(server.url()).unwrap();
    let csv = HeaderValue::from_static("text/csv");
    api.up().send().await.unwrap();
    api.up().set_ct(&csv).send().await.unwrap();
    api.up().set_b(vec![1, 2]).send().await.unwrap();
    api.up()
        .set_ct(&csv)
        .set_b(vec![1, 2])
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert!(content_types(&r[0]).is_empty() && r[0].body.is_empty());
    assert_eq!(content_types(&r[1]), ct("text/csv"));
    assert!(r[1].body.is_empty(), "the header is sent without a body");
    assert_eq!(content_types(&r[2]), ct("application/octet-stream"));
    assert_eq!(content_types(&r[3]), ct("text/csv"));
    assert_eq!(r[3].body, [1, 2]);
}

/// Row 8b, edges: an empty native value counts as present (it is sent, the format's media type is
/// not); an empty `each` contributes nothing (the format's media type is sent); a contribution
/// that cannot be converted stays a build error, with or without a body.
#[tokio::test]
async fn row8b_empty_value_empty_each_and_conversion_failure() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Header::new(http()).with_base_url(server.url()).unwrap();
    api.up()
        .set_ct(HeaderValue::from_static(""))
        .set_b(vec![1])
        .send()
        .await
        .unwrap();
    let each = HeaderEach::new(http()).with_base_url(server.url()).unwrap();
    each.clone()
        .set_cts(Vec::new())
        .up("x")
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(content_types(&r[0]), vec![Vec::<u8>::new()]);
    assert_eq!(content_types(&r[1]), ct("text/plain; charset=utf-8"));

    let error = each
        .typed("text/plain\r\nx: 1")
        .up("x")
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.to_string().contains("content-type"), "{error}");
}

concord::api! {
    /// Row 8c, compile-pass side (the rejections are `tests/ui/format_header_media_conflict.rs`):
    /// an endpoint `request` statement without a media type under a header contribution is the 8b
    /// rule, and a `remove "content-type"` that drops every inherited contribution allows the
    /// default's media type.
    pub client Contract(header("content-type") ct?: HeaderValue) {
        base "http://contract.invalid";
        request json "application/vnd.api+json";

        POST plain(body p: String) {
            request text;
            path "plain";
        }
        scope clean {
            headers { remove "content-type"; }
            POST save(body p: NewItem) { path "save"; }
        }
    }
}

/// Row 8c: `remove` resolves the conflict; the endpoint statement's media type yields to the header.
#[tokio::test]
async fn row8c_remove_resolves_the_conflict() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Contract::new(http())
        .with_base_url(server.url())
        .unwrap()
        .set_ct(HeaderValue::from_static("text/markdown"));
    api.clean()
        .save(NewItem { name: "n".into() })
        .send()
        .await
        .unwrap();
    api.plain("p").send().await.unwrap();
    api.clone().maybe_ct(None).plain("p").send().await.unwrap();
    let r = server.requests();
    assert_eq!(content_types(&r[0]), ct("application/vnd.api+json"));
    assert_eq!(content_types(&r[1]), ct("text/markdown"));
    assert_eq!(content_types(&r[2]), ct("text/plain; charset=utf-8"));
}

concord::api! {
    /// Row 8f: the `content-type` key is marked sensitive on the client, so the media type Concord
    /// inserts is flagged too — also after a `remove` (sticky classification).
    pub client Secret(#[sensitive] header("content-type") ct?: HeaderValue) {
        base "http://secret.invalid";
        request Private;

        POST save(body p: String) { path "save"; }
        scope removed {
            headers { remove "content-type"; }
            POST save(body p: NewItem) {
                request json "application/x-declared";
                path "removed";
            }
        }
    }
}

concord::api! {
    /// Row 8f control: the same format without a sensitive `content-type` key.
    pub client Open {
        base "http://open.invalid";
        POST save(body p: String) {
            request Private;
            path "save";
        }
    }
}

/// Row 8f: the inserted `Content-Type` carries the key's static classification: the flag is set,
/// `Prepared`'s `Debug` redacts it, and it is still sent (positive control). Without the marker
/// it is neither flagged nor redacted.
#[tokio::test]
async fn row8f_inserted_content_type_is_classified() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Secret::new(http()).with_base_url(server.url()).unwrap();
    let prepared = api.save("s").prepare().unwrap();
    let value = &prepared.request().headers()["content-type"];
    assert_eq!(value, "application/x-private");
    assert!(value.is_sensitive());
    let shown = format!("{prepared:?}");
    assert!(shown.contains("content-type: <redacted>"), "{shown}");
    assert!(!shown.contains("x-private"), "{shown}");
    prepared.send().await.unwrap();

    let removed = api
        .removed()
        .save(NewItem { name: "n".into() })
        .prepare()
        .unwrap();
    let value = &removed.request().headers()["content-type"];
    assert_eq!(value, "application/x-declared");
    assert!(value.is_sensitive());
    removed.send().await.unwrap();

    let r = server.requests();
    assert_eq!(content_types(&r[0]), ct("application/x-private"));
    assert_eq!(content_types(&r[1]), ct("application/x-declared"));

    let open = Open::new(http()).save("s").prepare().unwrap();
    assert!(!open.request().headers()["content-type"].is_sensitive());
    assert!(format!("{open:?}").contains("content-type: application/x-private"));
}

/// Row 8d: Concord adds no `Accept` (the server sees reqwest's own `*/*`) and does not check the
/// response `Content-Type`.
#[tokio::test]
async fn row8d_no_accept_and_no_response_type_check() {
    let (server, api) =
        shop(Reply::text(200, r#"{"id":2}"#).header("content-type", "text/html")).await;
    assert_eq!(api.item(2).send().await.unwrap(), Item { id: 2 });
    assert_eq!(server.single().header_all("accept"), ["*/*"]);
}

concord::api! {
    /// Row 9: a downstream format named by path, in a default and on the endpoint.
    pub client Rows {
        base "http://rows.invalid";
        response codecs::Csv;

        GET export() -> Vec<(String, u32)> { path "export"; }
        GET done() -> () { path "done"; }
        POST import(body rows: Vec<(String, u32)>) -> Item {
            request codecs::Csv;
            response json;
            path "import";
        }
    }
}

/// Row 9.
#[tokio::test]
async fn row9_custom_codec_in_place_and_as_default() {
    let server = Server::start(|r| match r.path() {
        "/export" => Reply::text(200, "a,1\nb,2"),
        "/done" => Reply::text(200, "ok"),
        _ => Reply::json(200, r#"{"id":9}"#),
    })
    .await;
    let api = Rows::new(http()).with_base_url(server.url()).unwrap();
    let rows = vec![("a".to_owned(), 1), ("b".to_owned(), 2)];
    assert_eq!(api.export().send().await.unwrap(), rows);
    api.done().send().await.unwrap();
    assert_eq!(api.import(rows).send().await.unwrap(), Item { id: 9 });
    let r = server.requests();
    assert_eq!(r[2].body, b"a,1\nb,2");
    assert_eq!(content_types(&r[2]), ct("text/csv"));
}

/// Row 11: status handling is unchanged under defaults: a non-2xx keeps the unread response; the
/// decoder is retained after native execution.
#[tokio::test]
async fn row11_status_and_native_decoder_unchanged() {
    let (_server, api) = shop(Reply::json(422, r#"{"problem":"x"}"#)).await;
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    let body = error.into_response().unwrap().text().await.unwrap();
    assert_eq!(body, r#"{"problem":"x"}"#);

    let (_server, api) = shop(Reply::json(200, r#"{"id":4}"#)).await;
    let (client, request, decoder) = api.item(4).prepare().unwrap().into_parts();
    let item = decoder.finish(client.execute(request).await).await.unwrap();
    assert_eq!(item, Item { id: 4 });
}

concord::api! {
    /// Row 12b: a default is referenced only through an alias gated by its consumers' `cfg`s, so
    /// a default naming types that do not exist compiles when its only users are disabled or when
    /// nothing uses it; disabled endpoints may name missing formats and payloads in their own statements.
    pub client Gated {
        base "http://gated.invalid";

        scope off {
            request AlsoMissing;
            response DoesNotExist;
            #[cfg(any())]
            GET gone() -> Item { path "gone"; }
            #[cfg(any())]
            POST gone_body(body b: Item) { path "gone"; }
            GET ping() { path "ping"; }
        }
        #[cfg(any())]
        scope hidden {
            response Missing;
            GET x() -> Item { path "x"; }
        }
        #[cfg(any())]
        GET in_place() -> MissingToo {
            response self::missing::Format;
            path "y";
        }
    }
}

/// Row 12b (compile-pass): the declaration above compiles; enabled endpoints work.
#[test]
fn row12b_unused_and_cfg_disabled_defaults_are_never_referenced() {
    let p = Gated::new(http()).off().ping().prepare().unwrap();
    assert_eq!(p.request().url().path(), "/ping");
}

/// Lexical precedence: values named like format words, format paths that start with a word, and a
/// payload named `json`.
pub mod lexical {
    use concord::bytes::Bytes;
    use concord::{BodyError, Decode, Encode, reqwest};

    /// `json::Codec`: a path that starts with a word is a path, not the word.
    pub mod json {
        use super::*;

        /// Upper-cases text.
        pub struct Codec;

        impl Encode<String> for Codec {
            const CONTENT_TYPE: Option<&'static str> = Some("text/x-upper");
            fn encode(value: String) -> Result<reqwest::Body, BodyError> {
                Ok(value.to_uppercase().into())
            }
        }
    }

    /// `self::csv`: a lowercase format type, written as a path.
    #[allow(non_camel_case_types)]
    pub struct csv;

    impl Encode<Vec<u8>> for csv {
        const CONTENT_TYPE: Option<&'static str> = Some("text/csv");
        fn encode(value: Vec<u8>) -> Result<reqwest::Body, BodyError> {
            Ok(value.into())
        }
    }

    impl Decode<Vec<u8>> for csv {
        fn decode(body: Bytes) -> Result<Vec<u8>, BodyError> {
            Ok(body.into())
        }
    }

    concord::api! {
        /// Every row of the lexical table that compiles.
        pub client Lexical {
            base "http://lexical.invalid";
            response bytes;

            /// `json: T` and `json?: T` are values named `json`.
            GET values(json: String, text?: String) {
                path "v/{json}";
                query { "t" = text; }
            }
            /// `header("x") json: T` is a header value named `json`.
            GET header(header("x-json") json: String) { path "h"; }
            /// `body query: T` is a JSON body named `query`.
            POST body_named_query(body query: super::NewItem) {
                request json;
                path "q";
            }
            /// `request self::csv;` and `request json::Codec;` are format paths.
            POST lower(body rows: Vec<u8>) {
                request self::csv;
                path "l";
            }
            POST codec(body rows: String) {
                request json::Codec;
                path "c";
            }
            /// `-> Vec<u8> {` and `-> Vec<u8>;` are the inherited format.
            GET inherited() -> Vec<u8> { path "i"; }
            GET inherited_semi() -> Vec<u8>;
            /// `response self::csv;` is an explicit custom format.
            GET explicit() -> Vec<u8> {
                response self::csv;
                path "e";
            }
        }
    }
}

/// The lexical precedence table, on the wire.
#[tokio::test]
async fn lexical_precedence() {
    use lexical::Lexical;
    let server = Server::reply(Reply::new(200).body("raw")).await;
    let api = Lexical::new(http()).with_base_url(server.url()).unwrap();
    api.values("j").set_text("t").send().await.unwrap();
    api.header("h").send().await.unwrap();
    api.body_named_query(NewItem { name: "q".into() })
        .send()
        .await
        .unwrap();
    api.lower(b"a,b".to_vec()).send().await.unwrap();
    api.codec("up").send().await.unwrap();
    assert_eq!(api.inherited().send().await.unwrap(), b"raw");
    assert_eq!(api.inherited_semi().send().await.unwrap(), b"raw");
    assert_eq!(api.explicit().send().await.unwrap(), b"raw");
    let r = server.requests();
    assert_eq!(r[0].target, "/v/j?t=t");
    assert_eq!(r[1].header("x-json"), Some("h"));
    assert_eq!(r[2].body, br#"{"name":"q"}"#);
    assert_eq!(content_types(&r[2]), ct("application/json"));
    assert_eq!(content_types(&r[3]), ct("text/csv"));
    assert_eq!(
        (r[4].body.as_slice(), content_types(&r[4])),
        (&b"UP"[..], ct("text/x-upper"))
    );
}

/// `-> r#json`: a raw identifier is never a word; here it is the payload type, decoded with the
/// inherited format.
pub mod raw_word {
    /// A payload type named `json`.
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq, serde::Deserialize)]
    pub struct json {
        /// Content.
        pub v: u8,
    }

    concord::api! {
        /// A payload named like a format word.
        pub client RawWord {
            base "http://raw.invalid";
            response json;
            GET get() -> r#json { path "g"; }
        }
    }
}

/// Lexical table: `-> r#json`.
#[tokio::test]
async fn lexical_raw_identifier_is_a_payload() {
    let server = Server::reply(Reply::json(200, r#"{"v":5}"#)).await;
    let api = raw_word::RawWord::new(http())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(api.get().send().await.unwrap(), raw_word::json { v: 5 });
}

concord::api! {
    /// CHARTER §7 cross-feature fixture: native headers under format defaults.
    pub client Cross(
        header("x-static") fixed: HeaderValue = HeaderValue::from_static("s"),
        header("x-opt") opt?: HeaderValue,
        header("content-type") ct?: HeaderValue,
    ) {
        base "http://cross.invalid";
        headers { "x-literal" = "l"; }
        request json;
        response json;

        POST create(body b?: NewItem, header("x-unlisted") secret?: HeaderValue) -> Item {
            path "c";
            headers { override "x-static" = "o"; }
        }
        GET get() -> Item { path "g"; }
    }
}

concord::api! {
    /// The same headers without format defaults (formats written on the endpoint).
    pub client CrossLocal(
        header("x-static") fixed: HeaderValue = HeaderValue::from_static("s"),
        header("x-opt") opt?: HeaderValue,
        header("content-type") ct?: HeaderValue,
    ) {
        base "http://cross.invalid";
        headers { "x-literal" = "l"; }

        POST create(body b?: NewItem, header("x-unlisted") secret?: HeaderValue) -> Item {
            format json;
            path "c";
            headers { override "x-static" = "o"; }
        }
        GET get() -> Item {
            response json;
            path "g";
        }
    }
}

/// Every header line of a prepared request, with its flag, in map order.
fn header_lines<T>(p: &concord::Prepared<T>) -> Vec<(String, Vec<u8>, bool)> {
    p.request()
        .headers()
        .iter()
        .map(|(n, v)| (n.to_string(), v.as_bytes().to_vec(), v.is_sensitive()))
        .collect()
}

/// H11: an optional native `content-type` or other header does not decide whether a body exists:
/// the body follows its own presence only.
#[tokio::test]
async fn h11_optional_native_headers_do_not_affect_body_presence() {
    let server = Server::reply(Reply::json(200, r#"{"id":1}"#)).await;
    let api = Cross::new(http()).with_base_url(server.url()).unwrap();
    let ct_value = HeaderValue::from_static("application/vnd.x+json");
    api.clone()
        .set_ct(&ct_value)
        .set_opt(HeaderValue::from_static("o"))
        .create()
        .send()
        .await
        .unwrap();
    api.create()
        .set_b(NewItem { name: "n".into() })
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert!(r[0].body.is_empty(), "a header does not create a body");
    assert_eq!(content_types(&r[0]), ct("application/vnd.x+json"));
    assert_eq!(
        r[1].body, br#"{"name":"n"}"#,
        "no header does not remove the body"
    );
    assert_eq!(content_types(&r[1]), ct("application/json"));
}

/// H12: a format default changes neither which ordinary headers are sent nor their inheritance:
/// with and without defaults, the same values give the same header lines (bodies aside, whose
/// `Content-Type` is the format's).
#[test]
fn h12_format_defaults_do_not_affect_header_presence_or_inheritance() {
    let with = Cross::new(http()).set_opt(HeaderValue::from_static("o"));
    let local = CrossLocal::new(http()).set_opt(HeaderValue::from_static("o"));
    assert_eq!(
        header_lines(&with.get().prepare().unwrap()),
        header_lines(&local.get().prepare().unwrap())
    );
    assert_eq!(
        header_lines(&with.create().prepare().unwrap()),
        header_lines(&local.create().prepare().unwrap())
    );
    let lines = header_lines(&with.create().prepare().unwrap());
    let names: Vec<&str> = lines.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(
        names.contains(&"x-literal") && names.contains(&"x-opt"),
        "{names:?}"
    );
    let fixed = lines.iter().find(|(n, _, _)| n == "x-static").unwrap();
    assert_eq!(fixed.1, b"o", "the endpoint's override still applies");
    assert!(
        !names.contains(&"content-type"),
        "no body, no media type: {names:?}"
    );
}

/// CHARTER §7: a natively sensitive header value stays protected under format defaults, without a
/// marker: flagged on the wire request, redacted in `Prepared`'s `Debug`, and sent (positive
/// control).
#[tokio::test]
async fn native_sensitive_header_stays_protected_under_defaults() {
    let server = Server::reply(Reply::json(200, r#"{"id":1}"#)).await;
    let api = Cross::new(http()).with_base_url(server.url()).unwrap();
    let mut secret = HeaderValue::from_static("s3cr3t");
    secret.set_sensitive(true);
    let prepared = api
        .create()
        .set_b(NewItem { name: "n".into() })
        .set_secret(secret)
        .prepare()
        .unwrap();
    assert!(prepared.request().headers()["x-unlisted"].is_sensitive());
    assert!(!prepared.request().headers()["content-type"].is_sensitive());
    let shown = format!("{prepared:?}");
    assert!(shown.contains("x-unlisted: <redacted>"), "{shown}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
    prepared.send().await.unwrap();
    assert_eq!(server.single().header("x-unlisted"), Some("s3cr3t"));
}

/// Rows 2d, 6c, 7, 8c, 8e, 8g, 9, 12a and 12d are compile-time: this lists their trybuild cases
/// so that removing one fails here.
#[test]
fn compile_time_rows_have_ui_cases() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for case in [
        "ui/body_errors.rs", // 2d, at most one body, bodies on endpoints only
        "ui/format_endpoint_statement.rs", // 6c (nothing to apply to, duplicates, order)
        "ui/format_media_type_needs_format.rs", // 8a (`request "m";` without a format)
        "ui/format_removed_spellings.rs", // 6c/7 (removed in-place forms name their replacement)
        "ui/representation_missing_via.rs", // 7
        "ui/representation_alpha_spelling.rs", // 7 + alpha hints
        "ui/format_header_media_conflict.rs", // 8c
        "ui/format_output_media_type.rs", // 8e (`response`, `format`)
        "ui/format_media_type_invalid.rs", // 8g (explicit)
        "ui/format_invalid_content_type.rs", // 8g (format constant)
        "ui/representation_unsupported_payload.rs", // 9 (endpoint, scope; scalar payloads)
        "ui/format_inherited_unsupported.rs", // 9 (inherited)
        "ui_json/format_serde_bound.rs", // 9 (blanket bound)
        "ui_features/feature_disabled_formats.rs", // 12a/12b
        "ui/format_via_removed.rs", // 12d
        "ui/format_reserved_words.rs", // lexical: words reserved in format position
        "ui/format_swapped_order.rs", // swapped order
    ] {
        assert!(root.join(case).is_file(), "{case}");
        assert!(root.join(case).with_extension("stderr").is_file(), "{case}");
    }
}

/// Decoding through `Decode` directly is unchanged (error documents).
#[test]
fn decode_is_usable_directly() {
    let item = <concord::Json as Decode<Item>>::decode(Bytes::from_static(br#"{"id":1}"#));
    assert_eq!(item.unwrap(), Item { id: 1 });
}
