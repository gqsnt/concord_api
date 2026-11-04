# Writing a format

A **format** says how a request body is encoded and how a successful response is decoded. It is a
Rust type that implements `Encode<T>` and/or `Decode<T>`, and a declaration names it by its path:
`format Csv;`, `request codecs::Ndjson;`, `response Versioned;`. `Json`, `Text`, `Binary`, `Form`
and `IgnoreContentType<F>` are written the same way, against the same public traits, in another
crate (`concord-extras`); they have no privilege over yours.

This page is for the person who writes a format. How a declaration chooses formats is in the
[author guide, section 5](AUTHOR_GUIDE.md#5-bodies-and-responses-formats); what an `Accept` or
`Content-Type` header does on the wire is in its
[reference table](AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type).

Every Rust block below is an excerpt of a file of this repository that is compiled and tested;
`cargo test -p concord-example-quickstart` fails when an excerpt stops matching its source. Each
label names the file; its tests run the code.

## 1. What you implement

The two traits, as `crates/concord-core/src/codec.rs` declares them (documentation left out):

```text
trait Encode<T> {
    const CONTENT_TYPE: Option<&'static str>;
    fn encode(value: T) -> Result<reqwest::Body, CodecError>;
}

trait Decode<T> {
    type Context: Send + 'static;
    const ACCEPT: Option<&'static str> = None;
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
    fn context(head: &ResponseHead<'_>) -> Result<Self::Context, CodecError>;
    fn decode_body(context: Self::Context, body: Bytes) -> Result<T, CodecError>;
}
```

- A format is a marker type of yours (`pub struct Csv;`). It satisfies the orphan rules whatever
  the payload type is. Implement the traits for one payload (`impl Encode<Vec<Row>> for Csv`) or
  for a bound (`impl<T: Serialize> Encode<T> for Csv`).
- Concord picks a format only from the nearest `format`, `request` or `response` statement. It
  never looks at a Rust type name, and no format name is reserved.
- In an application, import the traits and helpers from `concord` (`Encode`, `Decode`,
  `CodecError`, `ResponseHead`, `ContentTypeField`, `concord::bytes`, `concord::reqwest`). A crate
  that only offers formats and does not use `api!` can depend on `concord-core`, which has the
  same items and implements no format.
- A request stream with a fixed media type can use `Encode<T>`: the by-value payload may become
  a streaming `reqwest::Body` (`Body::wrap_stream`, which needs reqwest's `stream` feature,
  enabled in your own `Cargo.toml`). Preparing the request need not read the payload; executing
  it reads it once. [`examples/native/src/upload.rs`](../examples/native/src/upload.rs) moves a
  non-`Clone` one-shot source this way.
- `Encode::CONTENT_TYPE` is static, and `Encode::encode` does not return per-body media headers.
  For a media type that changes with the body (a multipart boundary), use the native
  request-construction route (`into_parts`, then reqwest's multipart support, as in
  [`examples/multipart`](../examples/multipart)). An explicit matching `content-type` header is
  also possible when the endpoint or SDK controls that body/header pair: it replaces the format's
  static value ([author guide](AUTHOR_GUIDE.md#53-media-types-and-content-type)).
- For a response read incrementally, use `send_raw` and native reads: typed `Decode` receives
  the collected body bytes. See [recipes](RECIPES.md) for the native routes.

## 2. An encoder only

A format that only sends bodies implements `Encode<T>`: a constant media type and the encoding.
The payload arrives by value, so a one-shot payload can move into a streaming `reqwest::Body`.
Excerpt of [`examples/native/src/repr.rs`](../examples/native/src/repr.rs), a form encoder for
ordered pairs:

```rust
pub struct Form(());

impl Encode<Vec<(String, String)>> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: Vec<(String, String)>) -> Result<reqwest::Body, CodecError> {
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(value)
            .finish();
        Ok(reqwest::Body::from(encoded))
    }
}
```

A statement names it, and the payload stays a plain Rust type. Excerpt of
[`examples/native/src/api.rs`](../examples/native/src/api.rs):

```rust
POST feedback(body fields: Vec<(String, String)>) {
    request repr::Form;
    path "feedback";
}
```

- `CONTENT_TYPE` is sent as `Content-Type` with the body, unless the `request` statement declares
  a media type or the endpoint sends a `content-type` header ([wire table](AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type)).
  `None` adds none. A value that is not a valid header value (visible ASCII) is a compile error
  wherever the format is used.
- A failing `encode` stops the call before anything is sent: `ErrorKind::Build`, displayed after
  the endpoint name as `request not built: body: <label>`. `error.codec_error()` returns the
  `CodecError`.

## 3. A decoder only

A format that only reads responses implements `Decode<T>`. The smallest one is stateless: its
`Context` is `()`, `context` accepts, and `decode_body` turns bytes into `T`. Excerpt of
[`examples/compat/src/lib.rs`](../examples/compat/src/lib.rs), a decoder that applies an
application rule after JSON decoding:

```rust
/// Checks the synthetic envelope after JSON decoding.
pub struct Checked;
impl Decode<Envelope> for Checked {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: concord::bytes::Bytes) -> Result<Envelope, CodecError> {
        let envelope = <concord::Json as Decode<Envelope>>::decode_body((), body)?;
        if !envelope.ok {
            return Err(CodecError::new("application rejected request")
                .with_source(std::io::Error::other(envelope.error.unwrap_or_default())));
        }
        Ok(envelope)
    }
}
```

- `<Json as Decode<Envelope>>::decode_body((), body)` decodes bytes with no head check: it is the
  body-only route (section 4).
- `Checked` does not override `check_content_type`, so it accepts any `Content-Type`, and it
  sets no `ACCEPT`. A proxy's `text/html` page reaches the JSON parser and fails there, as an
  `ErrorKind::Decode` error. To keep `Json`'s media check, delegate: `check_content_type(head)` calls
  `<Json as Decode<Envelope>>::check_content_type(head)` (section 5 does this).

## 4. The order of events on a 2xx response

| Step | Concord calls | It sees | If it fails | The response |
|---|---|---|---|---|
| 0 | nothing: a non-2xx status | status | `ErrorKind::Status`; the format is not consulted | unread |
| 1 | `check_content_type(&head)` | the head | `ErrorKind::Metadata` | unread |
| 2 | `context(&head)` | the head | `ErrorKind::Metadata` | unread |
| 3 | reads the body | — | `ErrorKind::Transport` | consumed |
| 4 | `decode_body(context, bytes)` | the context and the body, never the head | `ErrorKind::Decode` | consumed |

- Step 1 holds the format's checks of the `Content-Type` field (media type, charset), and only
  those. `IgnoreContentType<F>` skips exactly this step. Step 2 holds every other check of the
  head (a version header, a status the format treats specially) and produces the owned
  `Context` that `decode_body` needs.
- `Context` lives across the body read, so it is `Send + 'static`: it cannot borrow the response.
  It is `()` for a stateless format; it can be a parsed version, a charset, a flag.
- `decode_body` never receives the head, only the `Context`. A refusal made in step 1 or 2 keeps
  the response unread (`Error::response()` borrows it, `Error::into_response()` takes it). Nothing
  stops a format from carrying head data in its `Context` and refusing on it in `decode_body`, but
  that refusal comes after the body was read: it is a `Decode` error and the response is gone. So
  put every refusal that depends only on the head in `check_content_type` or `context`.
- `decode_body` is also the body-only route. Calling `<F as Decode<T>>::decode_body(context,
  bytes)` yourself decodes bytes with no head check, by name. That is how to decode an API's error
  document (`<Json as Decode<Problem>>::decode_body((), bytes)`).
- `ResponseHead` borrows the native response before its body is read: `status()` (a 2xx here),
  `version()`, `headers()` (unredacted) and `content_type()`. The last returns a
  `ContentTypeField`: `Missing`, `One(&HeaderValue)` or `Repeated`. It counts field lines and
  interprets nothing; parsing a media type is the format's job (`concord-core` has no parser).
  The head shows the fields reqwest delivers: a body reqwest decompressed itself has already lost
  `Content-Encoding` and `Content-Length`. `Debug` prints header names, never values.

## 5. A vendor format that checks a version header

A service sends `x-api-version` with every response, and the body is only readable for version 2.
The format checks the header in `context`, before the body is read, and hands the parsed version
to `decode_body` as its typed `Context`. Excerpt of
[`crates/concord/tests/metadata.rs`](../crates/concord/tests/metadata.rs), the header and the
format type:

```rust
/// The response header carrying the schema version of [`Versioned`].
pub const API_VERSION: HeaderName = HeaderName::from_static("x-api-version");

/// A downstream format that needs response metadata (C13): `application/json` checked by
/// `Json`'s own rule, a required `x-api-version: 2` checked in `context`, and the version handed to
/// the body decoder. Written against the public traits only.
pub struct Versioned;
```

Excerpt of [`crates/concord/tests/metadata.rs`](../crates/concord/tests/metadata.rs), the
implementation (the `DECODED` counter in `decode_body` is test scaffolding):

```rust
impl Decode<Item> for Versioned {
    type Context = u32;
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<Item>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<u32, CodecError> {
        let mut values = head.headers().get_all(&API_VERSION).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(CodecError::new("the response has no single x-api-version"));
        };
        match value.to_str().ok().and_then(|v| v.parse().ok()) {
            Some(2) => Ok(2),
            Some(v) => Err(CodecError::new("unsupported x-api-version").at("version", v)),
            None => Err(CodecError::new("x-api-version is not a number")),
        }
    }

    fn decode_body(version: u32, body: Bytes) -> Result<Item, CodecError> {
        DECODED.fetch_add(1, Ordering::SeqCst);
        let item: Item = <Json as Decode<Item>>::decode_body((), body)?;
        // The version known from the head shapes decoding: v2 ids are offset.
        Ok(Item {
            id: item.id + u64::from(version) * 1000,
        })
    }
}
```

- `check_content_type` delegates to `Json`: the format keeps the strict media check.
- `context` is the only place that reads `x-api-version`. A missing, repeated, non-numeric or
  unsupported value is a `CodecError` with a static label (`.at("version", v)` adds the number),
  so the call fails with `ErrorKind::Metadata`, and the body is never read or decoded.
- `decode_body` receives `u32` instead of the head. It cannot look at the headers again.
- `ACCEPT` makes the endpoint suggest `application/json` ([section 8](#8-the-accept-suggestion)).

A statement selects it like any other format. Excerpt of
[`crates/concord/tests/metadata.rs`](../crates/concord/tests/metadata.rs):

```rust
concord::api! {
    /// Metadata fixture.
    pub client Meta {
        base "http://meta.invalid";
        response Json;
        GET item() -> Item { path "item"; }
        GET versioned() -> Item { path "item"; response Versioned; }
        GET versioned_any_type() -> Item {
            path "item";
            response IgnoreContentType<Versioned>;
        }
        GET nothing() { path "item"; }
    }
}
```

The same test sends a missing, unsupported and non-numeric version through `versioned()` and
through `versioned_any_type()`, and every one is `ErrorKind::Metadata`: the adapter skips the
`Content-Type` check only (section 7).

`Decode<T>` returns only `T`. When the caller needs what the head said (the version itself),
decode into an output type of yours that carries it, such as `WithVersion<T> { version, value }`,
and declare that as the endpoint's output (`-> WithVersion<Item>`, with
`impl Decode<WithVersion<Item>> for Versioned`); the context carries the version from `context`
to `decode_body`.

### Narrowing to one exact vendor media type

`Json`'s check accepts the whole `application/*+json` family. A format for one vendor type runs
`Json`'s checks first, then compares the essence (type and subtype, case-insensitive, parameters
ignored) with its own. `Json` has already refused a missing, repeated, non-ASCII or malformed
field, so the text before the first `;` is the essence; no media-type parser is needed. Excerpt of
[`crates/concord-extras/tests/media.rs`](../crates/concord-extras/tests/media.rs):

```rust
impl<T: serde::de::DeserializeOwned> Decode<T> for AcmeV2 {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some(ACME_V2);

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<T>>::check_content_type(head)?;
        // `Json` accepted exactly one visible-ASCII field: compare its type and subtype, the
        // text before any parameter, case-insensitively.
        let essence = match head.content_type() {
            ContentTypeField::One(value) => value.to_str().ok(),
            _ => None,
        }
        .and_then(|text| text.split(';').next())
        .map(|essence| essence.trim_end_matches([' ', '\t']));
        if !essence.is_some_and(|essence| essence.eq_ignore_ascii_case(ACME_V2)) {
            return Err(CodecError::new(
                "the response Content-Type is not application/vnd.acme.v2+json",
            ));
        }
        Ok(())
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<T, CodecError> {
        <Json as Decode<T>>::decode_body((), body)
    }
}
```

The same test accepts `Application/Vnd.Acme.V2+JSON; profile=full` and refuses
`application/json`, `application/vnd.acme.v3+json` and `application/problem+json` before any
body is read. A version header, as in `Versioned` above, would go in `context`, so that
`IgnoreContentType<AcmeV2>` still checks it.

## 6. NDJSON with positions

A line-oriented format reports where a record failed, with numbers and without content.
Excerpt of [`examples/native/src/repr.rs`](../examples/native/src/repr.rs):

> The maintained implementation is `concord_extras::jsonl::{JsonLines, Ndjson}` (feature `jsonl`):
> a documented framing profile (UTF-8, LF/CRLF, blank lines, byte order mark, final line, media
> types), the same framing for the finite `Decode` and an incremental `JsonLinesReader`, lazy
> uploads and a consecutive-JSON-values reader. Use it rather than copying this example, which
> stays here as a small illustration of positions in a format of your own.

```rust
/// Newline-delimited JSON (`application/x-ndjson`): a `Vec<T>` is one JSON object per line.
///
/// Use it for line-oriented bulk endpoints. Encoding writes a trailing `\n` after every record;
/// decoding ignores blank lines (including a final newline). The whole body is buffered, as
/// for every `Decode`; for unbounded feeds use the streaming recipe instead.
pub struct Ndjson(());

impl<T: Serialize> Encode<Vec<T>> for Ndjson {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-ndjson");

    fn encode(value: Vec<T>) -> Result<reqwest::Body, CodecError> {
        let mut out = Vec::new();
        for (index, record) in value.iter().enumerate() {
            serde_json::to_writer(&mut out, record).map_err(|e| {
                CodecError::new("an NDJSON record could not be encoded")
                    .at("record", index as u64 + 1)
                    .with_source(e)
            })?;
            out.push(b'\n');
        }
        Ok(reqwest::Body::from(out))
    }
}

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Vec<T>, CodecError> {
        let mut records = Vec::new();
        for (index, line) in body.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let record = serde_json::from_slice(line).map_err(|e| {
                // Positions only: never the line's content (it is in the unredacted source).
                CodecError::new("an NDJSON line is not a valid record")
                    .at("record", records.len() as u64 + 1)
                    .at("line", index as u64 + 1)
                    .at("column", e.column() as u64)
                    .with_source(e)
            })?;
            records.push(record);
        }
        Ok(records)
    }
}
```

- `CodecError::new` takes a `&'static str`. The usual accident, putting the parser's message
  (which quotes the input) into the error with `CodecError::new(e.to_string())` or
  `CodecError::new(format!(..))`, does not compile.
- `.at("record", n)` adds a numeric position under a static name. Up to three are kept, in order;
  later ones are ignored. The conventional names are `record`, `line`, `column` and `byte` (`Json`
  reports `line` and `column`, `Text` a `byte` offset). `error.codec_error().and_then(|e|
  e.get("record"))` reads one back.
- `.with_source(e)` attaches the underlying error. `Display` and `Debug` never format it, and it
  is not `std::error::Error::source`. It is reachable only through
  `CodecError::source_unredacted()`, and it may quote the body.
- An encoding failure keeps the position too (`record`), and the call is an `ErrorKind::Build`
  error.
- This `Ndjson` does not override `check_content_type`, so it accepts any response `Content-Type`.
  A format that wants `application/x-ndjson` checks it there, in a few lines of its own (the
  standard formats parse the field with the `mime` crate).

The test shows the message, the positions and the unredacted source. Excerpt of
[`examples/native/tests/bodies.rs`](../examples/native/tests/bodies.rs):

```rust
#[tokio::test]
async fn ndjson_decode_error_is_content_free() {
    let server =
        Server::reply(Reply::new(200).body("{\"sku\":\"A\",\"qty\":1}\n{\"sku\":\"TOPSECRET\"}\n"))
            .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = import_stock(&api, vec![row("A", 1)]).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let shown = error.to_string();
    assert!(
        shown.contains("an NDJSON line is not a valid record (record 2, line 2, column"),
        "{shown}"
    );
    assert!(!shown.contains("TOPSECRET"), "{shown}");
    let codec = error.codec_error().unwrap();
    assert_eq!((codec.get("record"), codec.get("line")), (Some(2), Some(2)));
    // The detailed serde error is available on request, unredacted.
    assert!(codec.source_unredacted().is_some());
}
```

Static labels and numbers make the ordinary route safe. They are not a proof: a format is trusted
code, and a deliberate `String::leak`, a secret in a string literal or data smuggled through the
numbers still reaches `Display`. Review a format as you would review any code that handles the
body.

## 7. Strict `Json`, and the explicit adapter

`Json` checks the response head before it reads the body. The rules are tested in
[`crates/concord-extras/tests/media.rs`](../crates/concord-extras/tests/media.rs).

| `Content-Type` of the 2xx response | `Json` |
|---|---|
| exactly one field: `application/json`, or `application/*+json` such as `application/problem+json` and `application/vnd.github+json`; type, subtype and parameter names in any case; parameters as RFC 9110 writes them (quoted values, whitespace before `;`, empty parameters); no `charset`, or one `utf-8` or `us-ascii` | accepted |
| missing | refused |
| repeated (even with equal values), comma-joined (`application/json, text/html`), not an RFC 9110 media type (`charset = utf-8`, an unterminated quote), not visible ASCII | refused |
| a media range (`application/*`, `*/*`) | refused |
| another type: `text/html`, `text/json`, `application/json-seq`, `application/x-ndjson`, `application/json+xml` | refused |
| another `charset` (`iso-8859-1`), or `charset` twice (even equal) | refused |

A refusal is `ErrorKind::Metadata` and keeps the response unread. A JSON media type does not make
the body valid JSON or a `T`: that is checked when the body is decoded (`ErrorKind::Decode`). An
empty or whitespace-only body is not JSON, for `()` and `Option<T>` too; `null` is.

The other standard formats, for comparison:

| Format | Response head check | `Accept` suggestion |
|---|---|---|
| `Json` | the table above | `application/json` |
| `Text` | refuses only a single well-formed `Content-Type` with a `charset` that is not UTF-8 (`us-ascii` is fine), repeated or not; any media type, a missing field, several fields or a malformed one (bytes outside visible ASCII included) is accepted | none |
| `Binary` | none (`image/png` is fine) | none |
| `Form` | exactly one `application/x-www-form-urlencoded`; no `charset`, or one `utf-8` or `us-ascii` | `application/x-www-form-urlencoded` |
| `IgnoreContentType<F>` | `F`'s, without its `check_content_type` | `F`'s |

**`IgnoreContentType<F>`** skips only `F`'s `check_content_type`. The status must still be 2xx,
`F`'s other head checks (`context`) still run, `F` still decodes the body (malformed JSON is still
a decode error), and `F`'s `Accept` suggestion and encoding are unchanged. Select it on the
endpoint, or the scope, whose server mislabels its responses, not on the whole client. Excerpt of
[`crates/concord/tests/bodies.rs`](../crates/concord/tests/bodies.rs):

```rust
/// A server that mislabels its JSON: the check is skipped deliberately, here only.
GET legacy(id: u64) -> Item {
    response IgnoreContentType<Json>;
    path "legacy/{id}";
}
```

It works with any format that keeps its `Content-Type` checks in `check_content_type`, yours
included: in section 5, `response IgnoreContentType<Versioned>;` still refuses a missing version
header. The adapter's own test on `Json` shows each guarantee. Excerpt of
[`crates/concord-extras/tests/media.rs`](../crates/concord-extras/tests/media.rs):

```rust
/// The adapter skips exactly the `Content-Type` check; the body is still `Json`'s.
#[test]
fn ignore_content_type_skips_only_the_media_check() {
    assert_eq!(
        verdict::<IgnoreContentType<Json>, V>(&[b"text/html"]),
        Ok(())
    );
    assert_eq!(verdict::<IgnoreContentType<Json>, V>(&[]), Ok(()));
    assert_eq!(
        <IgnoreContentType<Json> as Decode<V>>::ACCEPT,
        Some("application/json")
    );
    assert!(
        <IgnoreContentType<Json> as Decode<V>>::decode_body((), Bytes::from_static(b"<html>"))
            .is_err()
    );
    assert!(
        <IgnoreContentType<Json> as Decode<V>>::decode_body((), Bytes::from_static(b"[1]"))
            .is_ok()
    );
}
```

**A format that decides on the head.** An endpoint that answers `200` with a JSON document or
`204` without a body needs a format of its own: `Json` has no empty document and requires a JSON
`Content-Type`. This one lets a `204` pass without a `Content-Type`, and holds every other 2xx to
`Json`'s check. The head decides, before the body is read, and `Json` stays strict. Excerpt of
[`examples/compat/src/lib.rs`](../examples/compat/src/lib.rs):

```rust
/// A response format for an endpoint that answers either JSON or nothing (`200` with a document,
/// `204 No Content` without). JSON itself has no empty document and `Json` requires a JSON
/// `Content-Type`, so this policy is the application's own: a `204` needs no `Content-Type` and
/// decodes to `None`; any other 2xx must pass `Json`'s head check and decode as JSON (`null` is
/// `None` too). The head decides, before the body is read; `Json` itself stays strict.
pub struct JsonOrEmpty;

impl<T: serde::de::DeserializeOwned> Decode<Option<T>> for JsonOrEmpty {
    /// Whether the response was a `204 No Content`.
    type Context = bool;
    const ACCEPT: Option<&'static str> = <concord::Json as Decode<Option<T>>>::ACCEPT;

    fn check_content_type(head: &concord::ResponseHead<'_>) -> Result<(), CodecError> {
        if head.status() == concord::reqwest::StatusCode::NO_CONTENT {
            return Ok(());
        }
        <concord::Json as Decode<Option<T>>>::check_content_type(head)
    }

    fn context(head: &concord::ResponseHead<'_>) -> Result<bool, CodecError> {
        Ok(head.status() == concord::reqwest::StatusCode::NO_CONTENT)
    }

    fn decode_body(no_content: bool, body: concord::bytes::Bytes) -> Result<Option<T>, CodecError> {
        if no_content {
            return Ok(None);
        }
        <concord::Json as Decode<Option<T>>>::decode_body((), body)
    }
}
```

## 8. The `Accept` suggestion

`const ACCEPT: Option<&'static str>` is a request preference a format may suggest. Concord sends
it only as a fallback: for an endpoint with a typed output, when the header chain leaves no
`accept` value and does not end with `remove accept`. A value you author (literal, native, empty,
repeated, with q-values) always wins and is sent as written. The suggestion is checked when the
declaration is compiled, like `CONTENT_TYPE`. It is never compared with the response
`Content-Type`, and it never selects another format. The whole table is in the
[author guide](AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type).

To stop only the suggestion for an endpoint, give it a format without one. A small wrapper does
it, and `remove ACCEPT` does not: a `remove` of a header nobody set is an error. The wrapper
forwards all three decoding steps of the format it wraps and leaves `ACCEPT` at its default,
`None`, so `response NoSuggestion<Json>;` still refuses a response that is not JSON, before the
body is read, and decodes exactly as `Json` does. Excerpt of
[`crates/concord/tests/accept.rs`](../crates/concord/tests/accept.rs), whose
`no_suggestion_keeps_the_metadata_checks` test checks exactly that:

```rust
/// `F` without its `Accept` suggestion: how an author disables only the suggestion (a wrapper of
/// theirs, not a `remove` of a header nobody set). All three decoding steps are `F`'s, so
/// `NoSuggestion<Json>` checks the `Content-Type` exactly as `Json` does; only `ACCEPT` is left at
/// its default, `None`.
pub struct NoSuggestion<F>(PhantomData<fn() -> F>);

impl<T, F: Decode<T>> Decode<T> for NoSuggestion<F> {
    type Context = F::Context;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        F::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<F::Context, CodecError> {
        F::context(head)
    }

    fn decode_body(context: F::Context, body: Bytes) -> Result<T, CodecError> {
        F::decode_body(context, body)
    }
}
```

A wrapper that leaves out `check_content_type` would also drop `Json`'s `Content-Type` check;
skipping that check is a separate, deliberate choice ([section 7](#7-strict-json-and-the-explicit-adapter)).

Suppressing Concord's suggestion does not mean that no `Accept` reaches the wire: when Concord
sends none, reqwest sends its own (`*/*`, or the default of your `reqwest::Client`).

## 9. Testing a format

**On a head built by hand**, with no server: `ResponseHead::new(status, version, &headers)`. A
format generic over its payload (`impl<T: DeserializeOwned> Decode<T> for AcmeV2`) is called
through the trait with the payload named, `<AcmeV2 as Decode<Item>>::check_content_type(&head)`:
`AcmeV2::check_content_type(&head)` alone cannot choose `T` (rustc's E0283). The standard formats'
tests use one helper for every case. In an application crate, import
`ResponseHead` from `concord`, `StatusCode` and `Version` from `concord::reqwest`. Excerpt of
[`crates/concord-extras/tests/media.rs`](../crates/concord-extras/tests/media.rs):

```rust
fn headers(content_types: &[&[u8]]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for ct in content_types {
        map.append(CONTENT_TYPE, HeaderValue::from_bytes(ct).unwrap());
    }
    map
}

/// The format's verdict on a 200 with these `Content-Type` field lines: `Ok` or the static label.
fn verdict<F: Decode<T>, T>(content_types: &[&[u8]]) -> Result<(), &'static str> {
    let map = headers(content_types);
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    F::check_content_type(&head)
        .and_then(|()| F::context(&head).map(drop))
        .map_err(|e| e.label())
}
```

**On a body alone**: `<F as Decode<T>>::decode_body(context, Bytes::from_static(..))`. The
adapter test above does it for `Json`.

**On a response you obtained yourself**: `Decoder::<T>::new::<F>()` builds the handling a
declared endpoint has (status, head, body) for a format and output type of your choice, outside
any declaration. Its errors name no endpoint. Excerpt of
[`crates/concord/tests/metadata.rs`](../crates/concord/tests/metadata.rs):

```rust
let server = Server::reply(html()).await;
let response = reqwest::Client::new().get(server.url()).send().await;
let error = Decoder::<Item>::new::<Json>()
    .finish(response)
    .await
    .unwrap_err();
assert_eq!(error.kind(), ErrorKind::Metadata);
assert_eq!(error.endpoint(), None);
let server = Server::reply(Reply::json(200, r#"{"id":3}"#)).await;
let response = reqwest::Client::new().get(server.url()).send().await;
let item = Decoder::<Item>::new::<Json>()
    .finish(response)
    .await
    .unwrap();
assert_eq!(item, Item { id: 3 });
```

**Through a declaration, on loopback**: the same file starts a server that sends a response head
and withholds the body, to show that a refusal needs only the head.

## 10. Checklist

- One `Encode<T>`/`Decode<T>` impl per payload type, or a bound; the statement names the format.
- Every `Content-Type` check goes in `check_content_type`, every other head check in `context`;
  `decode_body` reads bytes and the context only.
- `CodecError` labels are static text; positions are numbers; the underlying error goes to
  `with_source`.
- Set `ACCEPT` only if the format has a preferred media type. Wrap the format to drop it.
- Test the head with `ResponseHead::new`, the body with `decode_body`, and one request on
  loopback.
