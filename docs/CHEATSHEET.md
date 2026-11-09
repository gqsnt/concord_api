# Concord cheat sheet

The whole language on one page. Each row links to its precise rule in the
[author guide](AUTHOR_GUIDE.md); the caller side is in the [caller guide](CALLER_GUIDE.md). The
example below is compiled and run as a doctest.

```rust
use concord::reqwest::{self, header::{ACCEPT, AUTHORIZATION, HeaderName}};
use serde::{Deserialize, Serialize};
use concord::{Binary, Json, Text};

/// A vendor header, as a native constant.
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
        #[sensitive] header(AUTHORIZATION) token: String, // required: a field of `DocsConfig`
        header(X_TENANT) tenant?: String,                 // optional: absent until set
    ) {
        base "https://api.docs.example/v1";
        headers { ACCEPT = "application/json"; "x-client" = "concord"; }
        format Json; // requests and responses below are JSON unless a nearer statement says otherwise

        scope folder(id: u32) {
            path "folders/{id}";

            GET list(query page?: u32, query("per-page") per_page: u32 = 50) -> Vec<Doc> {
                path "docs";
            }
            POST create(body doc: NewDoc) -> Doc { path "docs"; }
            PATCH rename(doc: u64, body change: Rename) -> Doc {
                path "docs/{doc}";
                request Json "application/merge-patch+json"; // format and media type together
            }
            PUT write(doc: u64, body markdown: String) {
                path "docs/{doc}/content";
                request Text "text/markdown";
            }
            GET raw(doc: u64) -> Vec<u8> {
                path "docs/{doc}/raw";
                response Binary;
            }
            DELETE remove(doc: u64) { path "docs/{doc}"; } // no `->`: any 2xx, body not decoded
            GET public(doc: u64) -> Doc {
                path "public/{doc}";
                headers { remove AUTHORIZATION; override "x-client" = "concord-public"; }
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let config = DocsConfig { token: "Bearer t".into() };
    let docs = Docs::new(reqwest::Client::new(), config).set_tenant("acme");
    let folder = docs.folder(7);

    let list = folder.list().set_page(2).prepare()?;
    assert_eq!(
        list.request().url().as_str(),
        "https://api.docs.example/v1/folders/7/docs?page=2&per-page=50"
    );
    assert_eq!(list.request().headers()["x-tenant"], "acme");

    let rename = folder.rename(3, Rename { title: "Notes".into() }).prepare()?;
    assert_eq!(rename.request().headers()["content-type"], "application/merge-patch+json");

    let public = folder.public(3).prepare()?;
    assert!(public.request().headers().get("authorization").is_none());
    assert!(!format!("{docs:?}").contains("Bearer t")); // never printed by Concord
    Ok(())
}
```

## Declaring

| Write | Means | Rule |
|---|---|---|
| `pub client Name(..) { base ".."; .. }` | one API; its required values form `NameConfig` | [§1](AUTHOR_GUIDE.md#1-model-the-api-as-a-tree) |
| `scope name(..) { .. }` | a level the caller navigates: shared path, subdomain, keys, values | [§1](AUTHOR_GUIDE.md#1-model-the-api-as-a-tree) |
| `GET name(..) -> T { .. }` | an endpoint (`GET POST PUT PATCH DELETE HEAD OPTIONS`); no `->`: no typed output | [§5.5](AUTHOR_GUIDE.md#55-no-output-versus---) |
| `x: T`, `mut x: T`, `x: T = e`, `x?: T` | required and fixed on handles, required and replaceable on handles, initialized, optional | [§2](AUTHOR_GUIDE.md#2-values-required-optional-initialized) |
| `params Name(x?: T, ..);` (client), `GET op(..Name)` | a parameter group: optional/initialized values written once, spliced where `..Name` is written; trait `Name` holds their setters for every including endpoint | [§2.1](AUTHOR_GUIDE.md#21-parameter-groups-one-fact-many-endpoints) |
| `path "a/{x}"`, `subdomain "{x}"` | templates; `{client.x}` or `{scope.x}` reads an enclosing owner's value | [§3](AUTHOR_GUIDE.md#3-base-paths-and-subdomains) |
| `query x`, `query("k") x`, `header("k") x`, `header(KEY) x`, `body x` | placements in the parameter list; `each` sends one occurrence per item | [§4.1](AUTHOR_GUIDE.md#41-placing-a-value) |
| `query(..) x?: T` (endpoints) | `T: QueryPairs` writes its own key(s): exclusive cursors, run-time names; never `#[sensitive]` | [§4.1a](AUTHOR_GUIDE.md#41a-values-that-choose-their-own-keys-query) |
| `query { "k" = v; }`, `headers { "k" = "t {x}"; KEY = v; }` | blocks. **Left of `=` is a key**: a literal, or for headers a Rust path to a `HeaderName`. **Right of `=` is a value**: a template, or a value's name | [§4.2](AUTHOR_GUIDE.md#42-blocks-templates-and-references), [§4.8](AUTHOR_GUIDE.md#48-header-names-and-values) |
| `append`, `override`, `override .. else inherit`, `remove` | change a key set further up; anything else on an inherited key is an error | [§4.3](AUTHOR_GUIDE.md#43-what-an-owner-may-do-with-an-inherited-key) |
| `format F;`, `request F;`, `request F "type/subtype";`, `response F;` | formats on a client, scope or endpoint; the nearest wins per direction; a media type comes with its format | [§5.2](AUTHOR_GUIDE.md#52-where-formats-come-from) |
| `Json`, `Text`, `Binary`, `Form`, `IgnoreContentType<Json>`, `multipart::Multipart`, or any path (`self::Csv`) | a format type: one implementing `Encode<T>`/`Decode<T>` (Concord's or yours), named like any Rust type | [§5.1](AUTHOR_GUIDE.md#51-the-formats), [§5.7](AUTHOR_GUIDE.md#57-custom-formats), [formats](CUSTOM_FORMATS.md) |
| `body form: MultipartBody` + `request Multipart;` (feature `multipart`) | `MultipartBody::form_data().text(..).file(..)` or `::related().part(Part::new(b).header(..))`; boundary chosen per body, sent with `Content-Length`; a contradicting declared media type or `content-type` is a `Build` error | [§5.10](AUTHOR_GUIDE.md#510-bodies-whose-media-type-is-known-at-run-time) |
| `ACCEPT = ".."`, `remove ACCEPT` | an authored `Accept` always wins over the format's suggestion; a chain ending in `remove` sends none from Concord | [§5.8](AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type) |
| `#[sensitive]` | never printed by Concord; also every key, segment and label it feeds | [§7](AUTHOR_GUIDE.md#7-sensitive-values) |
| `limit N;` | bounds every finite typed output's body at and below this owner (nearest wins): `ErrorKind::Limit` beyond, on `send()`, `decode()` and `prepare()?.send()` | [§5.10a](AUTHOR_GUIDE.md#510a-bounding-finite-bodies-limit) |
| `#[read]` (scope or endpoint) | a provider read whose method does not already imply one; GET, HEAD and OPTIONS read by default | [§5.14](AUTHOR_GUIDE.md#514-classifying-every-operation-at-runtime) |
| `page next link;`, `page next url links?.next;` | next-page URL source on a bodyless GET; followed requests keep its policy and identity | [§5.15](AUTHOR_GUIDE.md#515-declared-continuation-and-traversal-starts) |
| Runtime operation classification | `E::operation()` gives name, method and `is_read()`; map through the crate-private `__ClientOperation::from_operation` and match exhaustively for E0004 on an omitted endpoint | [§5.14](AUTHOR_GUIDE.md#514-classifying-every-operation-at-runtime) |
| `#[cfg(..)]`, `#[deprecated]`, `///` on scopes and endpoints | applied to the subtree or the navigation method; names are checked as if every `cfg` were on, so cfg-exclusive siblings need distinct names | [§8](AUTHOR_GUIDE.md#8-attributes) |

Usual order inside an owner: `base` (client), `path`, `subdomain`, `query { }`, `headers { }`,
format statements, `send` (client), parameter groups `params Name(..);` (client),
then children.

## Calling

| Generated | Use |
|---|---|
| `Docs::new(http, DocsConfig { .. })` | builds the client; initializers run once here |
| `Docs::validate_header_names()` | in a unit test: every endpoint's header names checked without building anything ([§4.9](AUTHOR_GUIDE.md#49-checking-header-names-in-a-test)) |
| `.set_x(v)`, `.maybe_x(Some(v) / None)` | endpoints, optional/initialized values and required `mut` values; return snapshot handles |
| `.x()`, `.http()`, `.base_url()`, `scope.client()` | borrowed facts; sensitive values and whole headers have crate-only getters |
| `base origin;` | a checked client value implementing `AsRef<concord::BaseUrl>`; select fixtures in that value at construction |
| `docs.folder(7)`, `folder.rename(3, change)` | navigation: required values are positional, in declaration order |
| `.send().await` (no client send policy) | `prepare`, one `execute`, require 2xx, then declared handling: finite decode, drain or open an unread stream; no Concord retry or extra request (also true of `Prepared::send`) |
| `.prepare()?` → `Prepared<T, D>` | `receive()` (below), `send()`, the native `reqwest::Request` (`request()`), `send_raw()`, `into_parts()` |
| `error Problem in Json;` | non-2xx document at the client, scope or endpoint; nearest wins; `send()` uses `concord::Error<Problem>`; no request header changes ([§5.11a](AUTHOR_GUIDE.md#511a-declared-error-documents)) |
| `#[derive(concord::derive::Error)]` | SDK enum Display, source fields and tuple conversions; `#[from(concord::Error<Problem>)]` splits a provider `Failure<Problem>` from other errors |
| `.prepare()?.receive().await?` → `Received<T, D>` | one execution, any status, body unread: `head()`, `meta()` (owned copy), `status()`, `url()`, then one of `decode()`, `decode_limited(n)`, `decode_with_meta()` (value and head), `document::<F, D>(n)` (any status, bounded, the format's head checks first: provider error documents), `into_incoming()`, `bytes_limited(n)`, `into_response()` ([§5.11](AUTHOR_GUIDE.md#511-reading-response-metadata-in-an-sdk)) |
| `-> stream T` / `-> stream` | `send()` returns the reader / the unread `Incoming`; readers' `open(..)` takes `Incoming`, `Received<T, D>` or a native response; `completion()`, `into_stream()` |
| `-> stream sse::Event` + `response Sse;` (feature `sse`) | server-sent events: `next()` → `Event` (`event()`, `data()` as text, `id()`), `last_event_id()`, `retry()`; a body ending inside an event is an error; no reconnect |
| `prepared.follow(url)?` | the same request at a server-returned URL (same origin only; `Debug` withholds it) |
| `send policy;` (client value or stateless type) | generated `send()` calls `SendWith<T, D>::send` (`D = ()` without an error document) and returns the SDK's `Output`; the policy may sign, make sub-requests or repeat requests |
| `send policy -> Result<_, Error>;` | the same, and every generated `send()` declares that result (`_` = the endpoint's output) instead of `<P as SendWith<T, D>>::Output`; a mismatch is a compile error at the template ([§5.13](AUTHOR_GUIDE.md#513-the-sdks-own-send-send-policy)) |
| `concord::Endpoint<T, D>` | implemented by every generated endpoint for its output `T` and error document `D` (`D = ()` without a declaration); provides `prepare` and `operation()`: write one SDK adapter for all operations |
| `GeneratedEndpoint::operation()` | the endpoint's declared identity without naming its output: compare with `error.operation()` |
| `into_parts()` → `(Client, Request, Decoder<T, D>)` | execute it yourself, then `decoder.finish(result)` or `decoder.decode(response)` |
| `error.kind()`, `status()`, `endpoint()` | `Build`, `Transport`, `Status`, `Metadata`, `Decode`, `Limit`; `status()` is `Some` for `Status` only |
| `error.head()`, `error.head_meta()` | the head of the response any failure after it arrived concerns (`Status`, `Metadata`, `Decode`, `Limit`, a mid-body `Transport`): borrowed, or an owned copy; `None` before a head |
| `error.delivery()` → `Delivery::{NotSent, Unknown, Answered}` | what the failure proves: Concord refused it before sending / the client failed with no head (a refused connection included: redirects) / a head arrived. The SDK decides which writes are uncertain ([§5.11b](AUTHOR_GUIDE.md#511b-a-write-that-may-have-happened-errordelivery)) |
| `concord::field::unsigned(headers, name)`, `field::single(..)`; `head.unsigned(name)` | one header field, strictly: absent `Ok(None)`; repeated, invalid (`+5`), overflow → `FieldError` (no value) ([§5.11](AUTHOR_GUIDE.md#511-reading-response-metadata-in-an-sdk)) |
| `error.response()`, `error.into_response()`, `error.meta()` | the unread response of a `Status` or a `Metadata` error: borrowed (headers), taken (body), or its head owned |
| `error.codec_error()` | the format's `CodecError` (a `Metadata`, `Decode` or encoding `Build` error): static `label()`, `get("line")`, and `source_unredacted()` |
| `error.transport()` | the reqwest error, which may contain URLs |
| `<Json as Decode<T>>::decode_body((), bytes)` | decode bytes you already hold: no head check and no context (for a received error document, `document` runs both) |

## Remember

- The format, not the Rust type, decides the wire: `-> String` under `Json` decodes a JSON string;
  `request Text;` sends raw text. An empty response is not JSON: an endpoint that returns nothing
  omits `->`; one that sometimes answers nothing needs a format that decides on the status
  ([formats](CUSTOM_FORMATS.md#7-strict-json-and-the-explicit-adapter)).
- `Json` refuses a 2xx response that lacks exactly one `Content-Type` of `application/json` or
  `application/*+json`, before reading the body (`ErrorKind::Metadata`, the response kept
  unread). For a server you cannot fix, `response IgnoreContentType<Json>;` on that endpoint
  skips only that check.
- An optional body: `set_b(v)` always sends `v` encoded (`None` in an `Option<T>` body is `null`);
  `maybe_b(None)` sends no body.
- Initializers run each time their owner is created: put a per-request value (an idempotency key)
  on the endpoint, not the client.
- Handles are snapshots: after `with_base_url` or a setter on the client, create scopes again from
  the new handle.
- A typed output sends its format's `Accept` suggestion (`Json`: `application/json`) unless the
  chain declares an `accept` or ends by removing one; an endpoint without `->` suggests nothing.
  A request `Accept` replaces a `reqwest::Client` default; when Concord sends none, reqwest sends
  its own (`*/*`). The response `Content-Type` is checked by the format and never compared with
  `Accept`. reqwest also adds `content-length` and `host` when it executes the request.
- The list of well-known secret names is a safety net; mark every secret `#[sensitive]`.

SDK-owned values: derive `concord::Debug`, marking secret fields `#[sensitive]` (unmarked
fields are shown). For Display that matches scalar serde spelling, use
`serde_plain::derive_display_from_serialize!(Type);` with `serde_plain = "1.0.2"`;
keep computed formatting in Rust. See [wire formatting](AUTHOR_GUIDE.md#6-wire-formatting-display-and-newtypes).

`page next link;` and `page next url links?.next;` declare provider continuation URLs on bodyless
GET endpoints. `endpoint.follow(next).send()` preserves the policy, decoding and operation; the
URL must share the original origin. Non-Send inputs remain constructible and preparable;
thread-crossing sends require Send. `Pages::starting_at(Resume::At(key))` replaces a traversal's
start before polling; `.refuse_repeated_keys()` refuses repeated admitted keys before dispatch.
