# REPORT.md

## Evidence and settings

I read the allowed user-facing material, starting with `docs/CHEATSHEET.md`, `docs/AUTHOR_GUIDE.md`, and `docs/RECIPES.md`, and inspected `examples/native/src/repr.rs` and relevant caller and diagnostic documentation. I did not read `crates/` or `v4/`.

I personally ran `git rev-parse HEAD` and `git status --short`. HEAD was `3be0b069c10a84ffddd8b9c3a0386e6fd15b209e`, matching the supplied independent snapshot HEAD; status produced no entries. I did not verify the original source SHA, execute tests, compile the sketches below, or observe HTTP traffic. Documentation statements about existing behavior are **inspected-only evidence**, not my test results.

The request specifies `gpt-6.1-terra`, effort `xhigh`, and static/read-only mode. I cannot independently verify the effective model or reasoning effort from the information available to me. The environment specifies a read-only filesystem; I did not attempt a write. Per the run note, this final message is the report.

All proposed APIs and future behavior below are **guesses and preferences**, not claims about an implementation.

## 1. Custom decoders

I want head validation to produce typed, per-response state that is passed directly into body decoding. That makes the order visible and avoids a global, thread-local, or shared mutable version value. I also want the error to retain the unread response when head validation fails.

This is the trait shape I would like; the names and signatures are invented:

```rust
// GUESS: illustrative future API, not compiled.
use concord::bytes::Bytes;
use concord::reqwest::header::HeaderMap;

pub trait Decode<T> {
    /// State produced once for this response and consumed by decode.
    type Head;

    /// Optional fallback, used only when no higher-priority Accept is present.
    const ACCEPT: Option<&'static str> = None;

    /// Called after success-status handling and before reading the body.
    fn check_head(headers: &HeaderMap) -> Result<Self::Head, HeadError>;

    /// Called only after check_head succeeds and the body is read.
    fn decode(head: Self::Head, body: Bytes) -> Result<T, BodyError>;
}

// Invented errors: labels are static; optional numeric context is structured.
pub struct HeadError;
impl HeadError {
    pub fn new(label: &'static str) -> Self { todo!() }
}

pub struct BodyError;
impl BodyError {
    pub fn new(label: &'static str) -> Self { todo!() }
    pub fn with_record(self, record: usize) -> Self { todo!() }
    pub fn with_line(self, line: usize) -> Self { todo!() }
    pub fn with_column(self, column: usize) -> Self { todo!() }
    pub fn with_source<E: std::error::Error + Send + Sync + 'static>(
        self,
        source: E,
    ) -> Self { todo!() }
}
```

For the vendor API, I would use a real media-type parser rather than prefix or suffix string matching. The example assumes a `mime` dependency and intentionally requires exactly one value for each relevant header:

```rust
// GUESS: uses the illustrative traits above; not compiled.
use concord::bytes::Bytes;
use concord::reqwest::header::{HeaderMap, CONTENT_TYPE};
use serde::de::DeserializeOwned;

pub struct AcmeJson;

pub struct AcmeHead {
    version: u32,
}

fn exactly_one<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<&'a str, HeadError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next().ok_or_else(|| HeadError::new("missing header"))?;
    if values.next().is_some() {
        return Err(HeadError::new("multiple header values"));
    }
    first.to_str().map_err(|_| HeadError::new("invalid header value"))
}

impl<T: DeserializeOwned> Decode<T> for AcmeJson {
    type Head = AcmeHead;
    const ACCEPT: Option<&'static str> = Some("application/vnd.acme.v2+json");

    fn check_head(headers: &HeaderMap) -> Result<AcmeHead, HeadError> {
        let raw_type = exactly_one(headers, CONTENT_TYPE.as_str())?;
        let media: mime::Mime = raw_type
            .parse()
            .map_err(|_| HeadError::new("invalid content type"))?;

        // Parameters such as charset do not change the type/subtype.
        if media.type_() != mime::APPLICATION
            || media.subtype().as_str() != "vnd.acme.v2"
            || media.suffix().map(|s| s.as_str()) != Some("json")
        {
            return Err(HeadError::new("unexpected content type"));
        }

        let raw_version = exactly_one(headers, "acme-version")?;
        let version: u32 = raw_version
            .parse()
            .map_err(|_| HeadError::new("invalid acme version"))?;
        if version != 2 {
            return Err(HeadError::new("unsupported acme version"));
        }

        Ok(AcmeHead { version })
    }

    fn decode(head: AcmeHead, body: Bytes) -> Result<T, BodyError> {
        // In a real SDK I can select a version-specific schema here.
        debug_assert_eq!(head.version, 2);
        serde_json::from_slice(&body)
            .map_err(|e| BodyError::new("invalid acme json").with_source(e))
    }
}
```

For NDJSON, I want **record**, **physical line**, and **column** to be separate. Here record numbers count nonblank records from one; physical lines also start at one, including blank lines. The column comes from `serde_json` and is relative to the current record line.

```rust
// GUESS: uses the illustrative traits above; not compiled.
use concord::bytes::Bytes;
use concord::reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;

pub struct Ndjson;

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    type Head = ();
    const ACCEPT: Option<&'static str> = Some("application/x-ndjson");

    fn check_head(_headers: &HeaderMap) -> Result<(), HeadError> {
        // This example does not require a particular response media type.
        Ok(())
    }

    fn decode(_: (), body: Bytes) -> Result<Vec<T>, BodyError> {
        let mut records = Vec::new();

        for (line_index, line) in body.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            let record_number = records.len() + 1;
            let value = serde_json::from_slice(line).map_err(|e| {
                let column = e.column();
                BodyError::new("invalid ndjson record")
                    .with_record(record_number)
                    .with_line(line_index + 1)
                    .with_column(column)
                    .with_source(e)
            })?;
            records.push(value);
        }
        Ok(records)
    }
}
```

I would expect this to buffer the body, like the current documented `Decode` contract; it is not a streaming decoder. I would not put a record’s content in the static label. The attached source may contain body content, so I would inspect or log it deliberately.

What would confuse me:

- Does the head hook receive only headers, or status and other metadata too? Status is already handled, but passing a read-only response head would allow richer codec policies.
- Is head state owned per response and passed to `decode`, including when I use `Prepared::into_parts()` and `Decoder::decode()` myself? Can it borrow headers, or must it be owned?
- Are missing, malformed, and duplicate `Content-Type` values rejected? How are case, parameters, and `+json` handled?
- Are numeric positions one-based, and is a column a byte or character offset? Can record and line both be supplied? Are absent fields distinguishable from zero?
- Does `with_source` remain available, and are source errors excluded from ordinary `Display` as in the current documentation?
- Can generic wrappers such as the compatibility adapter be written directly in the macro, or must I introduce a type alias?

## 2. Cheat-sheet declaration with types

**Guess:** the declaration below changes only the format spellings and imports from the current cheat sheet. In particular, the existing bytes format maps to the type `Binary`.

```rust
use concord::{Binary, Json, Text};
use concord::reqwest::{
    self,
    header::{ACCEPT, AUTHORIZATION, HeaderName},
};
use serde::{Deserialize, Serialize};

pub const X_TENANT: HeaderName = HeaderName::from_static("x-tenant");

#[derive(Deserialize)]
pub struct Doc { pub id: u64 }
#[derive(Serialize)]
pub struct NewDoc { pub title: String }
#[derive(Serialize)]
pub struct Rename { pub title: String }

concord::api! {
    /// A documents API.
    pub client Docs(
        #[sensitive] header(AUTHORIZATION) token: String,
        header(X_TENANT) tenant?: String,
    ) {
        base "https://api.docs.example/v1";
        headers { ACCEPT = "application/json"; "x-client" = "concord"; }
        format Json;

        scope folder(id: u32) {
            path "folders/{id}";

            GET list(query page?: u32, query("per-page") per_page: u32 = 50)
                -> Vec<Doc>
            {
                path "docs";
            }
            POST create(body doc: NewDoc) -> Doc { path "docs"; }
            PATCH rename(doc: u64, body change: Rename) -> Doc {
                path "docs/{doc}";
                request Json "application/merge-patch+json";
            }
            PUT write(doc: u64, body markdown: String) {
                path "docs/{doc}/content";
                request Text "text/markdown";
            }
            GET raw(doc: u64) -> Vec<u8> {
                path "docs/{doc}/raw";
                response Binary;
            }
            DELETE remove(doc: u64) { path "docs/{doc}"; }
            GET public(doc: u64) -> Doc {
                path "public/{doc}";
                headers {
                    remove AUTHORIZATION;
                    override "x-client" = "concord-public";
                }
            }
        }
    }
}
```

`use concord::Json;` followed by `format Json;` is natural Rust. I would usually prefer it to `use concord::Json as json;`: lowercase aliases look like the old special format words and may invite a naming-style warning. An alias should nevertheless work if format positions truly accept ordinary Rust type paths. A fully qualified `format concord::Json;` is also clear and avoids an import.

If I forget the import, I expect a compile error at `Json` saying the type cannot be found, ideally with a suggestion to import `concord::Json`. If the `json` feature is disabled, I expect an unresolved or unavailable `concord::Json`, with a diagnostic pointing to the feature where possible. I would not expect the macro to silently select another codec. Whether unused inherited paths are resolved remains a question: the current author guide says unused formats resolve nothing.

## 3. Predicted behavior

These are my desired outcomes; the brief does not settle every precedence or empty-body rule.

| Case | Prediction and what would surprise me |
|---|---|
| Typed JSON endpoint, no authored `Accept` | **Guess:** the decoder suggests `Accept: application/json`. I would be surprised by no JSON fallback, or by a fallback on an endpoint that does not decode. |
| Client authors `ACCEPT = "application/vnd.acme.v2+json"` | **Guess:** the authored value wins over the JSON fallback, without an additional `application/json` value. A response with that `+json` type passes standard JSON media validation. |
| Endpoint has `remove ACCEPT;` under that client | **Preference/guess:** removal expresses intent to suppress the inherited header *and* the codec fallback. The effective wire header could still be affected by transport defaults. It would surprise me if the removed header immediately reappeared as the JSON fallback; the brief does not explicitly define removal’s interaction with fallback. |
| Endpoint without `->` | **Guess:** no decoder, no decoder-suggested `Accept`, and no response media check. Existing documented behavior accepts any 2xx and drains the body. An independently authored or transport `Accept` can still be present. |
| `reqwest::Client` has a default `Accept` | **Preference/guess:** an explicitly configured transport default should be respected before a codec fallback. I would want the documentation to specify the actual precedence and whether Concord can observe that default while preparing a request. If it cannot, inserting a fallback on the request may mask the transport default; that outcome would surprise me unless clearly documented. |
| `200`, `Content-Type: text/html`, JSON-looking body | **Guess:** strict `Json` rejects the head before body reading, retaining the unread response. It should not try JSON merely because the bytes look valid. The named compatibility adapter should allow body decoding while still enforcing other checks. |
| `204`, no body, `-> Option<Item>` with JSON | **Guess:** this is an error, not `None`. With no `Content-Type`, strict validation likely fails first and preserves the unread response; if an acceptable JSON type is present, the empty body fails JSON decoding. `None` represents JSON `null`; a dedicated JSON-or-empty decoder is needed for a different policy. I would be surprised by a silent 204-to-`None` conversion. |
| PNG download, `response Binary;` | **Guess:** return the bytes unchanged without requiring `application/octet-stream`; `image/png` is valid for a binary download. I would not expect `Binary` to validate PNG structure. If it suggests an `Accept`, I would prefer no restrictive fallback over one that excludes PNG. |

For strict JSON, I would also expect missing or malformed `Content-Type` to fail, and valid JSON types with parameters or a structured `+json` suffix to pass. Those details need to be stated explicitly.

## 4. Names and promises

- **Compatibility adapter: `IgnoreContentType<Json>`.** This tells me exactly which check is skipped. It promises JSON syntax and deserialization still run, and any unrelated head checks remain in force. I find `UncheckedContentType` less direct: “unchecked” could mean the entire response or decoder is unchecked. I would want an example for a missing header as well as a wrong one.
- **Validator: `Client::validate_header_names()`.** This sounds like a side-effect-free associated function that needs neither a client instance nor a request. I expect a `Result`, actionable endpoint and header context, and coverage of all compiled-in endpoints, including native names whose resolved values can conflict. I would want the docs to spell out `cfg` coverage and whether validation reports the first failure or all failures. It should not promise to validate dynamic header *values* or server behavior.
- **Mismatch accessors: `error.head_error()`, `error.response()`, and `error.into_response()`.** I would use `head_error()` to distinguish a response-head validation failure and inspect its static label or structured reason; `response()` borrows the unread response; `into_response()` consumes the error and returns the response for explicit recovery. This extends the current documented response accessors beyond status errors. I would expect `status()` and `endpoint()` to remain available before consuming the error. A media mismatch should be distinguishable from an invalid JSON body, even if both share a broader error kind.

## 5. Documentation I need first

1. **Migration guide:** old format words to imports and type paths, feature flags, strict JSON behavior, the compatibility adapter, and changes to error handling.
2. **Custom decoder recipe:** head validation, owned per-response state, structured error positions, unread-response recovery, and the relationship between `send`, `finish`, and `decode`.
3. **Header and media behavior reference:** authored headers, `remove`, codec fallback, reqwest defaults, no-output endpoints, JSON suffixes and parameters, missing headers, and 204 responses; include the new header-name validator.

The mistake I expect most often is assuming that a body which parses as JSON is sufficient, then encountering a server or proxy that omits `Content-Type` or returns `text/plain` or `text/html`. I would first verify the server contract and actual header; I would use `IgnoreContentType<Json>` deliberately where compatibility requires it.