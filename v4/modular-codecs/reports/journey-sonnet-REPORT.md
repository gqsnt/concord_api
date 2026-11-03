## Delivery note (read first)

REPORT.md was NOT written. My Write call to /home/canarit/projects/concord_v4_modular_work/proposals/sonnet-journey/REPORT.md was refused by the harness ("Subagents should return findings as text, not write report files"). I did not route around that with Bash. The full report is inline below, so please save it to that path yourself (about 370 lines).

What does exist on disk is the scratch crate I was allowed to build: /home/canarit/projects/concord_v4_modular_work/proposals/sonnet-journey/scratch/. It is a mock of the trait shapes I expect, not Concord. `cargo +1.99.1 --offline test` passed 3 tests (the Acme decoder, the NDJSON record number, the 204 `JsonOrEmpty`). It also confirmed that `Decode<T> for AcmeV2` and `Decode<Versioned<T>> for AcmeV2` do not overlap. I removed its `target/` directory afterwards.

## Short summary of the five answers

1. **Decoders.** I expect `Decode<T>` to have `type Checked`, `const DEFAULT_ACCEPT: Option<&'static str>`, `check_head(&ResponseHead) -> Result<Checked, HeadMismatch>` and `decode(Checked, Bytes) -> Result<T, BodyError>`.
   - The Acme decoder checks the media type and `Acme-Version: 2` in `check_head` and returns a `SchemaVersion` enum. `Versioned<T>` hands the version to the caller.
   - The NDJSON decoder reports `with_record`, `with_line` and `with_column` on a `BodyError` with a static label.
   - What I find confusing:
     - `type Checked = ();` has to be written in every body-only codec, because associated type defaults are unstable.
     - `check_head` sits inside `Decode<T>`, so it needs a `T` it never uses.
     - "Head" is overloaded: `ResponseHead` is the input, and I first named the carried value `Head` too.
     - A builder `record(n)` and a getter `record()` cannot share a name.
     - A codec author needs a public way to build a `ResponseHead` for unit tests.
2. **Cheatsheet.** `format Json;`, `request Json "…";`, `request Text "…";`, `response Binary;`, with `use concord::{Binary, Json, Text};`. `use concord::Json as json;` is not something I would write.
   - Forgetting the import should give rustc's E0412 at the word, with a `use concord::Json;` suggestion.
   - A disabled `json` feature should give E0432 at my `use` line, naming `concord`'s `json` feature rather than `concord_extras`.
   - Traps: `response Bytes;` instead of `Binary`, and name clashes with `axum::Json` and `reqwest::multipart::Form`.
3. **Predictions.**
   - Typed JSON with no authored `Accept` sends `accept: application/json`, visible at `prepare()`.
   - An authored client `ACCEPT` wins and nothing is merged.
   - `remove ACCEPT;` brings the fallback back by the stated rule. That surprises me, so the docs should give `ACCEPT = "*/*"` as the "no preference" idiom.
   - An endpoint without `->` gets no check and no fallback.
   - A reqwest default `Accept` is silently beaten by Concord's request-level fallback (my guess about reqwest). I call this the sharpest edge.
   - A 200 `text/html` reply fails at the head check, before the body is read, with a new error kind, a static message and the response still unread.
   - A 204 to `-> Option<Item>` stays an error, but the message should say "empty" rather than blame the media type.
   - `Binary` does no media check and suggests no `Accept`.
4. **Names.**
   - Adapter: `IgnoreContentType<Json>`. It names exactly what is skipped, and `Unchecked*` evokes std's undefined-behaviour meaning.
   - Validator: keep `Client::validate_header_names()`. A short name like `validate` or `check` would collide with real endpoint names.
   - Errors: `ErrorKind::HeadMismatch`, the existing `response()` and `into_response()`, and `head_mismatch()` returning a static `label()` and `header_name()`, never a header value.
5. **Docs and the likely mistake.**
   - Three pages, in order: a cheatsheet plus format-types migration note; a "what goes on the wire: Accept and Content-Type" truth table; a compiled vendor-codec example crate.
   - The likely mistake is silencing the strict check for the whole client with the adapter, because a mock server or fixture sends no `Content-Type`.

---

# Author-journey report: custom codecs, strict media checks, header-name validation

Participant: Claude Sonnet. Role: a Rust developer who maintains an SDK on top of Concord.
**(guess)** marks something I fill in myself; unmarked statements about the next version restate the
brief, and statements about today's Concord come from the docs.

## 0. Basis

- Read: README, CHEATSHEET, AUTHOR_GUIDE, RECIPES, CALLER_GUIDE, QUICKSTART, the format parts of
  DIAGNOSTICS and MIGRATION, `examples/*/src`. Not read: `crates/`, `v4/`.
- Today, from the docs: `Decode<T>::decode(body: Bytes) -> Result<T, BodyError>`;
  `Encode<T>` has `const CONTENT_TYPE` and `encode`; `BodyError::new(text).with_source(e)`;
  "Concord sends no `Accept` and never checks the response `Content-Type`"; a status error keeps
  the unread response (`response()`, `into_response()`); the words `json text bytes form`.
- The Rust in section 1 type-checks (modulo imports and one placeholder body) against a **mock of
  the trait shapes I expect** in `scratch/` (`cargo +1.99.1 --offline test`, three tests pass). The
  mock is mine, not Concord's. It shows my code is coherent Rust and that two `impl Decode<..> for
  AcmeV2` do not overlap; it says nothing about Concord.

## 1. Task 1: a vendor decoder and an NDJSON decoder

### 1.1 Trait shapes I expect (all **(guess)** beyond the brief's behaviour)

```rust
pub struct ResponseHead<'a> { /* status, headers; I must be able to build one in my own tests */ }
impl ResponseHead<'_> {
    pub fn status(&self) -> StatusCode;
    pub fn headers(&self) -> &HeaderMap;
    pub fn media_type(&self) -> Option<MediaType<'_>>; // wish: parsed Content-Type (case, params, +suffix)
}
pub struct HeadMismatch; // HeadMismatch::new("static label").header(NAME): never a header *value*
pub struct BodyError;    // BodyError::new("static label").with_record(n).with_line(n).with_column(n).with_source(e)

pub trait Decode<T>: Sized {
    type Checked: Send + 'static;                       // what check_head learned; handed to decode
    const DEFAULT_ACCEPT: Option<&'static str> = None;  // the fallback; an authored Accept wins
    fn check_head(head: &ResponseHead<'_>) -> Result<Self::Checked, HeadMismatch>;
    fn decode(checked: Self::Checked, body: Bytes) -> Result<T, BodyError>;
}
// Encode<T> as today: const CONTENT_TYPE: Option<&'static str>; fn encode(T) -> Result<reqwest::Body, BodyError>
```

Why this shape: one trait, because `response F;` names one type; the check is a separate function so
the runtime can call it before reading the body; the value it returns is handed to `decode`, so I
parse the version once; `DEFAULT_ACCEPT` mirrors `Encode::CONTENT_TYPE` (a compile-time constant,
`Option<&'static str>`); both error types hold only static text and numbers, like today's rule
"descriptions never contain body content".

### 1.2 The vendor decoder (`application/vnd.acme.v2+json`, `Acme-Version: 2`)

```rust
use concord::reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use concord::{BodyError, Bytes, Decode, Encode, HeadMismatch, ResponseHead};
use serde::{Serialize, de::DeserializeOwned};

pub const ACME_VERSION: HeaderName = HeaderName::from_static("acme-version");
const MEDIA: &str = "application/vnd.acme.v2+json";

/// Holding a `SchemaVersion` is proof that the head check passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchemaVersion { V2 }

pub struct AcmeV2;

impl<T: DeserializeOwned> Decode<T> for AcmeV2 {
    type Checked = SchemaVersion;
    const DEFAULT_ACCEPT: Option<&'static str> = Some(MEDIA);

    fn check_head(head: &ResponseHead<'_>) -> Result<SchemaVersion, HeadMismatch> {
        match head.media_type() {
            Some(m) if m.is(MEDIA) => {}
            Some(_) => return Err(HeadMismatch::new("Content-Type is not the Acme v2 type").header(CONTENT_TYPE)),
            None => return Err(HeadMismatch::new("response has no Content-Type").header(CONTENT_TYPE)),
        }
        match head.headers().get(&ACME_VERSION).map(HeaderValue::as_bytes) {
            Some(b"2") => Ok(SchemaVersion::V2),
            Some(_) => Err(HeadMismatch::new("unsupported Acme-Version").header(ACME_VERSION)),
            None => Err(HeadMismatch::new("missing Acme-Version").header(ACME_VERSION)),
        }
    }

    fn decode(version: SchemaVersion, body: Bytes) -> Result<T, BodyError> {
        match version {
            SchemaVersion::V2 => serde_json::from_slice(&body).map_err(|e| {
                BodyError::new("invalid Acme v2 document")
                    .with_line(e.line() as u64).with_column(e.column() as u64).with_source(e)
            }),
        }
    }
}

impl<T: Serialize> Encode<T> for AcmeV2 { // requests carry the same vendor type
    const CONTENT_TYPE: Option<&'static str> = Some(MEDIA);
    fn encode(value: T) -> Result<reqwest::Body, BodyError> { /* serde_json::to_vec */ }
}

/// An output type that hands the version to the caller.
pub struct Versioned<T> { pub version: SchemaVersion, pub value: T }

impl<T: DeserializeOwned> Decode<Versioned<T>> for AcmeV2 {
    type Checked = SchemaVersion;
    const DEFAULT_ACCEPT: Option<&'static str> = Some(MEDIA);
    fn check_head(head: &ResponseHead<'_>) -> Result<SchemaVersion, HeadMismatch> {
        <Self as Decode<T>>::check_head(head) // delegation; note I must name some T
    }
    fn decode(version: SchemaVersion, body: Bytes) -> Result<Versioned<T>, BodyError> {
        let value = <Self as Decode<T>>::decode(version, body)?;
        Ok(Versioned { version, value })
    }
}
```

Because `SchemaVersion` has one variant, adding `V3` later turns every `match` in `decode` into a
compile error: that is the reason for the enum instead of a bare `u8`.

### 1.3 Using it

```rust
concord::api! {
    pub client Acme(#[sensitive] header(AUTHORIZATION) token: String) {
        base "https://api.acme.example";
        format AcmeV2;                                       // bodies and outputs, both directions

        GET widget(id: u64) -> Widget { path "widgets/{id}"; }
        GET widget_v(id: u64) -> Versioned<Widget> { path "widgets/{id}"; } // caller sees the version
        POST create(body w: NewWidget) -> Widget { path "widgets"; }       // Content-Type: ...v2+json
        GET export(since: String) -> Vec<Row> { path "export"; response codec::Ndjson; }
        DELETE remove(id: u64) { path "widgets/{id}"; }      // no `->`: no head check, no Accept
    }
}

match acme.widget(7).send().await {
    Ok(w) => use_it(w),
    Err(e) if e.kind() == ErrorKind::HeadMismatch => {        // (guess) names, see section 4
        let m = e.head_mismatch().expect("kind says so");
        log::warn!("acme: {} (header {:?})", m.label(), m.header_name()); // static text only
        let status = e.response().map(|r| r.status());        // borrowed, as for Status errors
        let page = e.into_response()?.text().await?;          // the body is still unread
    }
    Err(e) => return Err(e.into()),
}
```

### 1.4 NDJSON that says which record failed

```rust
pub struct Ndjson;

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    type Checked = ();                                        // nothing to carry
    fn check_head(head: &ResponseHead<'_>) -> Result<(), HeadMismatch> {
        match head.media_type() {
            Some(m) if m.is("application/x-ndjson") || m.is("application/ndjson") => Ok(()),
            _ => Err(HeadMismatch::new("response is not NDJSON").header(CONTENT_TYPE)),
        }
    }
    fn decode((): (), body: Bytes) -> Result<Vec<T>, BodyError> {
        let (mut out, mut record) = (Vec::new(), 0u64);
        for (index, line) in body.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) { continue; } // blank: not a record
            record += 1;
            match serde_json::from_slice(line) {
                Ok(value) => out.push(value),
                Err(e) => return Err(BodyError::new("invalid NDJSON record")
                    .with_record(record).with_line(index as u64 + 1)
                    .with_column(e.column() as u64).with_source(e)),
            }
        }
        Ok(out)
    }
}
```

The caller reads the numbers, not a sentence **(guess: accessor name)**:

```rust
Err(e) if e.kind() == ErrorKind::Decode => {
    if let Some(b) = e.body_error() { // the content-free part: label + numbers
        eprintln!("{}: record {:?}, line {:?}, column {:?}", b.label(), b.record(), b.line(), b.column());
    }
}
```

With the mock, `{"id":1}`, blank, `{"id":2}`, `{"id":x}` reports record 3, line 4. Record and
line differ as soon as a blank line is skipped, which is why I want both fields.

### 1.5 What I would find confusing

1. **`type Checked = ();` in every body-only codec.** Associated type defaults are unstable, so I
   expect to write that line in `Text`-like codecs and in NDJSON. One line is fine; a second line of
   ceremony is not. A split into a T-free supertrait (`Codec { Checked, DEFAULT_ACCEPT, check_head }`)
   plus `Decode<T>: Codec` would remove the repeated `check_head` (point 2); whether body-only codecs
   could then get a blanket impl I cannot tell from outside, given the macro's one-trait bound. I would
   prefer the split but accept the single trait.
2. **`check_head` sits inside `Decode<T>`**, so every call needs a `T` it does not use
   (`<Json as Decode<()>>::check_head`), and every extra `impl Decode<X> for Mine` repeats it
   (`Versioned<T>` above just delegates). Checking the head has nothing to do with the payload type.
3. **"Head" is overloaded.** The input is `ResponseHead`; I first named the carried value `Head` too
   and could not read my own code. `Checked` is my fix. Also `HEAD` is an HTTP method Concord supports.
4. **Static labels.** Porting `BodyError::new(format!("line {}", n))` becomes a type error
   (`String` is not `&'static str`). The message must teach: "put numbers in `with_line`".
5. **Builder versus getter names.** `record(n)` and `record()` cannot both exist, so I assumed
   `with_*` builders (next to today's `with_source`) and plain getters. I would mix up `body_source()`
   (underlying error, may quote the body) and whatever returns the safe label+numbers; the docs must
   put the two side by side.
6. **Units.** Is `record` 1-based? `column` 1-based, bytes or characters? serde_json's are 1-based
   (I believe, **guess**); I want the docs to say what Concord's fields mean, not what serde's do.
7. **When is the head check called?** I assume: only endpoints with `->`, only after a 2xx status,
   also on the `Decoder::finish` / `Decoder::decode(response)` native path, never on `send_raw()`.
8. **Testing without a server.** If only the runtime can build a `ResponseHead`, I cannot unit-test
   a head check. I need `ResponseHead::new(status, &headers)` or `From<&http::response::Parts>`.
9. **Partial results.** For NDJSON I lose the first two good records when the third fails. Fine for
   now; I would only note it.

## 2. Task 2: the CHEATSHEET with format types

```rust
use concord::reqwest::{self, header::{AUTHORIZATION, HeaderName}};
use concord::{Binary, Json, Text};
use serde::{Deserialize, Serialize};

pub const X_TENANT: HeaderName = HeaderName::from_static("x-tenant");
// Doc, NewDoc, Rename as before

concord::api! {
    /// A documents API.
    pub client Docs(
        #[sensitive] header(AUTHORIZATION) token: String, // required: a field of `DocsConfig`
        header(X_TENANT) tenant?: String,                 // optional: absent until set
    ) {
        base "https://api.docs.example/v1";
        headers { "x-client" = "concord"; }               // no ACCEPT: `Json` supplies one
        format Json; // requests and responses below are JSON unless a nearer statement says otherwise

        scope folder(id: u32) {
            path "folders/{id}";

            GET list(query page?: u32, query("per-page") per_page: u32 = 50) -> Vec<Doc> { path "docs"; }
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
                response Binary;                              // any Content-Type, no Accept preference
            }
            DELETE remove(doc: u64) { path "docs/{doc}"; }    // no `->`: any 2xx, nothing checked
            GET public(doc: u64) -> Doc {
                path "public/{doc}";
                headers { remove AUTHORIZATION; override "x-client" = "concord-public"; }
            }
        }
    }
}
```

Two things I would say on the page. First, the old sample put `ACCEPT = "application/json"` on the
client, which also went out on `raw` (a binary download): a per-codec fallback fixes that latent
mistake, so the line can go. Second, the `main` doctest must still pass: it asserts no `accept`
header, so one assertion (`list.request().headers()["accept"] == "application/json"`) must be added,
which doubles as the documentation of the new behaviour.

Rows to change:

| Row | New text |
|---|---|
| `format F;`, `request F;`, `request F "type/subtype";`, `response F;` | unchanged, but `F` is a type path in scope |
| `json`, `text`, `bytes`, `form`, or a path (`self::Csv`) | `Json`, `Text`, `Binary`, `Form` (`use concord::{..}`), or any path of yours; the built-ins have no privilege |
| last "Remember" bullet | "A typed output checks the response head before reading the body (`Json`: `application/json` or `+json`; relax one endpoint with `IgnoreContentType<Json>`) and suggests an `Accept` unless you author one. No `->`: nothing is checked or suggested." |

**Is `format Json;` with `use concord::Json;` natural?** Yes. It is the normal Rust spelling of
"this is a type", it greps, rust-analyzer can jump to it, and it reads like a sentence. Three
frictions I expect:

- **Name clashes.** SDK crates often sit next to servers: `axum::Json`, `actix_web::web::Json`,
  `axum::Form`, `reqwest::multipart::Form`. I would write `format concord::Json;` or
  `use concord::Json as WireJson;`, so path form must work everywhere a name does.
- **`bytes` becomes `Binary`, not `Bytes`.** Anyone with `use concord::bytes::Bytes;` in scope (the
  payload type!) will try `response Bytes;`. The native rustc error ("`Bytes: Decode<Bytes>` is not
  satisfied") is unhelpful; I would want `#[diagnostic::on_unimplemented]` on `Decode`/`Encode`
  saying "`Bytes` is a payload type, not a format; formats are `Json`, `Text`, `Binary`, `Form` or
  your own".
- **Muscle memory.** Existing declarations say `format json;`. I want a macro-level hint for the four
  old words ("formats are types now: `Json`, `use concord::Json;`") instead of rustc's bare
  "cannot find type `json`" **(guess: a lowercase word may even resolve to something unrelated)**.

**Would I write `use concord::Json as json;`?** No. It makes `format json;` keep compiling, but it
hides that the thing is a type, breaks the one convention (types are CamelCase) a reader relies on,
and is indistinguishable from the old reserved word. I expect it to work anyway **(guess)** if the
macro really drops today's rule "a bare lowercase identifier in format position must be one of the
words"; I would be unhappy if a leftover bespoke error rejected it, because then the built-ins would
still have a privilege.

**Forgetting the import.** I expect rustc's ordinary error, at the word I wrote:

```text
error[E0412]: cannot find type `Json` in this scope
  |         format Json;
  |                ^^^^ not found in this scope
help: consider importing this struct: use concord::Json;
```

**(guess)** that the span points at my token and not at `concord::api!`, and that rustc offers the
`use`. **Feature `json` disabled:** I expect the error at my `use` line, not inside the macro:

```text
error[E0432]: unresolved import `concord::Json`
  = note: found an item that was configured out
  = note: the item is gated behind the `json` feature
```

It must name the feature of the crate I depend on (`concord`'s `json`), not of `concord_extras`, which I
do not know exists. Today's message ("cannot find type `Json` in crate `$crate`", one per statement)
is exactly what I would not want to see.

## 3. Task 3: predictions

Common assumption **(guess)**: the fallback is added when the request is prepared, so
`prepare().request().headers()` already shows it, and `into_parts()` users see it too.

| Case | What I predict should happen | What would surprise me |
|---|---|---|
| 1. typed JSON endpoint, no authored `Accept` | Exactly one header, `accept: application/json` **(guess: exact string)**, from `Json`'s fallback; visible in `prepare()`. | A list such as `application/json, */*;q=0.1`; the header appearing only at `send()`; the fallback also appearing on endpoints without `->`. Also: it is a behaviour change from today's "Concord sends no Accept", and a server that 406s on `application/json` while serving only a vendor type will start failing. That needs a changelog line. |
| 2. client sets `ACCEPT = "application/vnd.acme.v2+json"` | Sent exactly as authored, one value; no `application/json` appended; Concord does not compare it with the codec. The strict check still passes for a `+json` reply. | Merging into a list; a compile error because Accept and codec "disagree". |
| 3. endpoint `remove ACCEPT;` under a client that sets it | By the stated rule (no authored Accept, so the decoder's fallback applies) the fallback comes back: `application/json`. | This outcome, even though I predicted it. `remove` reads as "send no Accept". I would accept it only if the docs say so in the table and give the idiom for "no preference": `ACCEPT = "*/*"`, which is what reqwest sends anyway. The alternative (`remove` also suppresses the fallback) would need `remove` to be legal with nothing inherited, which breaks today's "remove requires a key set further up". |
| 4. endpoint without `->` | No decoder: no head check, no fallback, any 2xx, body drained. Authored `Accept` is still sent. A bodied `POST` still sends its `Content-Type`. | An `Accept: application/json` because the client says `format Json;` (a format applies only where there is a body or output); a media-type check on a drained body; an error on `text/html`. |
| 5. `reqwest::Client` built with a default `Accept` | Concord cannot see the client's defaults. Its fallback is set on the request, and reqwest, I believe, only adds default headers a request lacks **(guess, reqwest behaviour)**, so the fallback silently beats the transport default. An authored `Accept` beats it too, as today. | This is the sharpest edge. README's promise is "transport settings belong to your reqwest::Client", and today the client default wins because Concord sends nothing. I would expect a loud docs warning: put `Accept` in the declaration (or `*/*`) if you rely on the transport default. I would be surprised if nothing said so. |
| 6. `200`, `Content-Type: text/html`, JSON-looking body | Strict `Json` fails in the head check, before the body is read. A new kind of error (not `Status`, not `Decode`), static `Display` text with no header value, the response still unread in the error. `IgnoreContentType<Json>` would decode it. | The body being read and parsed first; a serde message; kind `Status` for a 200; the error text quoting the received type. Edge cases I want documented: `application/json; charset=utf-8` and `APPLICATION/JSON` pass; `application/problem+json` and `+json` vendor types pass; `text/json`, `text/plain`, `application/json-seq` fail; a missing `Content-Type` fails **(guess)**. |
| 7. `204`, no body, `-> Option<Item>` with JSON | Still an error: JSON has no empty document (today's rule), and `Option<T>` only maps `null`. The error should say "empty" (a no-content status carries no media type to check). | A *media-type* error for a 204 (the real cause is "no content", and the message would send me to the wrong place); `Option<T>` silently becoming `None` (I would like it, but it contradicts today's guide). The fix is the recipe `JsonOrEmpty`, rewritten with a head check that looks at the status (`scratch/src/adapters.rs`, tested). |
| 8. PNG declared `response Binary;` | `-> Bytes`/`Vec<u8>`. No media-type check (`image/png`, `application/octet-stream`, or none all pass) and no fallback of its own, so reqwest sends `*/*`. An `Accept` authored on the client still goes out, so keep client-wide `Accept` off clients that mix codecs. | `Binary` requiring `application/octet-stream`, or suggesting it (a CDN could 406). Not a surprise but a gap: the actual `Content-Type` of the download is lost; I would use the native exit. |

## 4. Task 4: names

**Compatibility adapter: `IgnoreContentType<Json>`.**

- It names exactly what is skipped, the thing I will search for after seeing a failing `Content-Type`,
  and today's guide already says the response `Content-Type` "is ignored". The verb also says that a
  decision was made: I know it is wrong and I am choosing not to care.
- `UncheckedContentType<Json>` borrows `*_unchecked` from std, where the word means "undefined
  behaviour if you are wrong". A mislabelled reply here is a plain decode error, never UB. It also reads
  as "a kind of content type", and a bare `Unchecked` suggests everything is off.
- Promises: skips the `Content-Type` check only. Status is still required to be 2xx, the JSON is still
  strict, other head checks still run, and the `Accept` fallback is unchanged **(guess; the docs must
  say it)**. It is for one endpoint (`response IgnoreContentType<Json>;`), not a client default.
- Warning: `IgnoreContentType<C>` reads as a general adapter. A user cannot write it for their own
  codec (my mock needed `Checked = ()` luck), so either it is provided for `Json` only, or codecs opt
  in through a documented trait. `IgnoreContentType<Csv>` should fail with a message naming that trait.
- I also expect `response IgnoreContentType<Json>;` to parse as a path with generic arguments **(guess)**.

**Validator: `Client::validate_header_names()`, kept.**

- A short verb such as `validate()` or `check()` is a bad name on a generated client, because real
  vendor APIs have endpoints called `validate` and `check`, and section 9 makes a collision a compile
  error. The long name steals nothing.
- Promises: an associated function (no client, no config, no I/O, deterministic); covers every
  endpoint compiled in; returns one report listing every problem, each naming the endpoint
  (`Docs.folder.rename`) and the native constant as I wrote it (`X_TENANT`); `Ok` means no
  `prepare()` will fail with a header-**name** build error. It says nothing about header values, so
  the docs must say "names only; values are checked by `prepare()`".
- "Names" could be read as "syntax"; the doc line fixes that: it checks the composition of the names
  a declaration uses (a header set twice, `remove` of nothing, a reserved name via a constant).
- Usage: `#[test] fn headers() { Docs::validate_header_names().unwrap(); }`, replacing the advice
  "a test that prepares every endpoint". It should exist on literal-only declarations too (trivially
  `Ok`), so the test survives a later switch to constants.

**Error accessors for "the response metadata did not match".**

| Name | What it promises |
|---|---|
| `ErrorKind::HeadMismatch` | The exchange worked and the status was 2xx, but the codec refused the response before its body was read. The body is unread. Not transient (retrying the same request will not help, **guess**). |
| `error.response()` / `error.into_response()` | The same two names as for `Status`, same meaning: borrowed (read headers), taken (read body). Both documented as "`Status` and `HeadMismatch`"; `None` / `Err(self)` otherwise. |
| `error.head_mismatch() -> Option<&HeadMismatch>` | Which check failed: `label()` (static) and `header_name()` (which header, if any). Never a header value; the value is in `response().headers()` if I want it. |

Why one word: the codec returns `HeadMismatch`, the kind is `HeadMismatch`, the accessor is
`head_mismatch()`. A kind named `Head` would collide with the HTTP method, and `Mismatch` alone reads
like "mismatched types". I would keep `error.status()` as `None` for this kind and tell me to use
`response().status()`: `status().is_some()` is already how people mean "the server said no".

## 5. Task 5: what I need first, and the likely mistake

Three pages or examples, in order:

1. **Cheatsheet plus a "format types" migration note.** Every existing declaration changes, so this
   comes first: old word to new type (`json` to `Json`, `bytes` to `Binary`), imports and features,
   the name-clash advice, and the exact errors for a missing import and a disabled feature.
2. **A "What goes on the wire: `Accept` and `Content-Type`" page.** The eight cases above as a truth
   table, authored versus fallback versus transport default, how to see the result in `prepare()`,
   how to relax one endpoint with `IgnoreContentType<Json>`, and how to handle a `HeadMismatch` error
   (`response()`, `into_response()`, a code sample).
3. **A compiled example crate for a vendor codec.** Acme (media type, version header, `Accept`),
   NDJSON with record numbers, `JsonOrEmpty` rewritten with a head check, a unit test that builds a
   `ResponseHead` by hand, and `validate_header_names()` in a test. It replaces `repr::Ndjson` in
   `examples/native` and `JsonOrEmpty` in `examples/compat`.

**The mistake I expect most:** hitting the strict check on a mislabelled reply (or on a test
double with no `Content-Type`) and silencing it for the whole client with `IgnoreContentType<Json>`,
instead of fixing the fixture or applying the adapter to the one misbehaving endpoint. The docs'
own stand-in server always sends a correct `Content-Type` (`Response { content_type, .. }`), so
nothing in the examples prepares anyone for it. Mock servers I know of often omit it or default to
`text/plain` **(guess about specific mock crates)**; after the upgrade, the whole suite fails with
the same message and the reflex is the global adapter. A close second: reaching for the adapter on a
`204`, where it does not help.

## Appendix: pages that will go stale (found while reading)

- CHEATSHEET last "Remember" bullet and the `format json;` row; README feature table and example
  (`<Json as Decode<Problem>>::decode(response.bytes().await?)`).
- QUICKSTART "Nothing checks that the server agrees" and "Concord adds no `Accept` header".
- AUTHOR_GUIDE 5.1 table ("the response `Content-Type` is ignored"), the sentence at the end of 5.3,
  the words paragraph, and 5.7.
- DIAGNOSTICS `format_output_media_type` ("Concord sends no `Accept` and does not check ...") and the
  "format word whose feature is off" paragraph.
- RECIPES "Error documents" and CALLER_GUIDE section 6: decoding a non-2xx body with
  `<Json as Decode<Problem>>::decode(bytes)` has no obvious spelling once decoding has a head phase.
  I would expect either body-only decoding to stay available for error documents, or
  `error.decode_body::<Json, Problem>().await` that applies the head check to the stored response
  **(guess)**; a proxy's HTML 502 would then report a mismatch instead of a JSON parse error.
- `examples/compat` `JsonOrEmpty` and `examples/native` `repr::Ndjson` (both written against the
  body-only `decode`).
