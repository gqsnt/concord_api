# A/B designs — codex

Repository inspected: `work/wt/rv-p1`, HEAD `ba10189b`. This is a design comparison, not an implementation or a claim that the proposed syntax compiles. All source references below are relative to that repository. Before blocks quote current source; comments marking omissions are editorial. After blocks are proposals. Alternatives are not cumulative, and their savings must not be added together.

The friction catalogue was absent. I used the witness SDKs' own `FRICTION.md` and available `MAINTENANCE.md`, including their integration corrections, alongside `GLUE.draft.md`, the charter, construction problem, author guide, cheat sheet, and macro source. I did not inspect other idea lanes or the design directory.

The inventory counts **nonblank, heuristically classified** lines. My removal estimates count **physical source lines, including local comments** in the specified ranges; a partial range is identified as such. They are candidate deletions after migration, not measured savings. Models, validation, signing, refresh, retries, polling, and traversal do not become savings merely because the classifier labels their container an error.

## Evidence that constrains both families

| SDK | Current code and implication |
|---|---|
| distribution-registry | `examples/distribution-registry-sdk/src/error.rs:748-784` combines a bounded, media-label-agnostic JSON document with a Bearer challenge. Document plumbing can move; challenge interpretation and acquisition remain Rust. `src/api.rs:67-83` also mixes binary, drained, and streaming success bodies. |
| amazon-s3 | `examples/amazon-s3-sdk/src/error.rs:263-304` bounds and interprets XML errors independently of success formats. `src/client.rs:387-407` treats 304 separately. `src/api.rs:78-107` documents HEAD's absence of an error body. |
| salesforce | `examples/salesforce-sdk/src/error.rs:20-43` is a public nominal error enum worth preserving. `src/error.rs:207-236` reads an array-or-object error model with `IgnoreContentType<Json>`; that model remains `src/model.rs:301-316`. |
| backblaze-b2-native | `examples/backblaze-b2-native-sdk/src/wire.rs:94-101` uses **JSON**, including for errors of a binary download. `src/error.rs:255-275` distinguishes a document whose status contradicts HTTP. The supplied B2/XML counterexample must not become an assumption that this Native SDK uses XML. |
| anthropic-messages | `examples/anthropic-messages-sdk/src/lib.rs:152-167` is nearly pure response adaptation. Its stream's HTTP-opening error document is JSON even though success is `MessageEvents` (`src/lib.rs:111-113`). Midstream failures are different errors (`src/error.rs:325` onward). |
| quickbooks-online | `examples/quickbooks-online-sdk/src/format.rs:60-68` smuggles a typed Fault through `CodecError`; `src/policy.rs:175-182` downcasts and clones it. Faults can occur at the root or under `QueryResponse`. Refresh and uncertain-write decisions are separate behavior. |
| slack | `examples/slack-sdk/src/lib.rs:235-279` distinguishes HTTP refusal, `ok:false`, inconsistent envelopes, and successful payloads. `src/wire.rs:8-29` makes payload fields optional to allow failure documents through decoding. |
| kraken-spot-rest | `examples/kraken-spot-rest-sdk/src/policy.rs:148-167` checks ordered error strings and requires `result`. The endpoint declares `Envelope<T>`; the public policy returns `Response<T>`. Signing in `src/policy.rs:73-118` must survive. |
| cloudflare-zones | `examples/cloudflare-zones-sdk/src/wire.rs:123-176` selects by `success` before decoding `result`. Errors beside `success:true` are diagnostics. The result template cannot currently express the projection (`FRICTION.md`, F1). |
| stripe | `examples/stripe-sdk/src/send.rs:79-142` contains a provider-specific metadata projection plus a generic holder. The former is not removed by simply adding `concord::Response<T>`. `FRICTION.md`, F4 records the repeated response recipe. |
| notion | `examples/notion-sdk/src/client.rs:224-246` retains raw inconsistent errors and compares body status with HTTP. Query incompleteness (`src/client.rs:105-114`) and cursor validation remain distinct from HTTP document decoding. |

Two further limits: Snowflake's `examples/snowflake-sdk/src/api.rs:7-32` correctly models **202 Pending versus 200 Complete**, without polling in the format; Linear's `examples/linear-sdk/src/lib.rs:140-151` correctly retains **data and errors together**. An error-list heuristic must not replace either contract.

The controls already have their chosen shapes: Nager `examples/nager-holidays-sdk/src/lib.rs:22-38`, PokeAPI `examples/pokeapi-sdk/src/lib.rs:79-87`, Frankfurter `examples/frankfurter-sdk/src/lib.rs:32-53`. Every alternative below leaves all three declarations unchanged, including their existing send policies. No migration is required of them.

# Family A — errors

## A1. One typed HTTP error document — smaller than you think

**Fact it expresses:** A non-success HTTP response has this document type and this independently named representation.

**Declaration (author writes), before → after:** Anthropic, `examples/anthropic-messages-sdk/src/lib.rs:83-86`:

```rust
// Before
base "https://api.anthropic.com";
path "v1";
format Json;
send SendPolicy -> Result<Response<_>, Error>;

// After: one new provider fact; ordinary Rust KeepHead replaces the local adapter.
base "https://api.anthropic.com";
path "v1";
format Json;
error Json => ApiErrorDocument;
send concord::KeepHead -> Result<concord::Response<_>, concord::CallError<ApiErrorDocument>>;
```

The endpoints, including `POST stream(body request: StreamRequest) -> stream StreamEvent { response MessageEvents; }`, do not change. `KeepHead` is an ordinary stateless Rust `SendWith` implementation, not an implicit actor or a new DSL action. Its carrier is B1; an SDK that wants bare values can omit the policy and obtain `Result<T, CallError<D>>`.

`CallError<D>` has two categories: `Concord(concord::Error)` and `Http(HttpFailure<D>)`. `HttpFailure<D>` owns the original `ResponseMeta` and `Result<D, concord::Error>`. Thus a 429 with HTML, an oversized body, or a broken read stays an HTTP 429, with an independent document failure. `provider()` returns an optional typed document; `document_error()` explains its absence. No `D: Debug` bound and no provider text in default formatting or `source()`.

This is intentionally smaller than status tables or a general response language: one inherited document fact, the existing `Received::document` mechanism, one reusable failure holder. Most of the uniform document/metadata adaptation goes away; provider models and special failures do not.

**Call site (caller writes), before → after:**

```rust
// Before
let response = api.messages().create(request).send().await?;
let message = response.value;
// HTTP refusal inspection used Error::Api(api_error), api_error.provider_error().

// After: the successful value/head convention stays the same.
let response = api.messages().create(request).send().await?;
let message = response.value;
// A failed send now has a typed document without an SDK failure wrapper:
if let concord::CallError::Http(http) = failure {
    let status = http.meta().status();
    let document: Option<&ApiErrorDocument> = http.provider();
    let document_failure: Option<&concord::Error> = http.document_error();
}
```

The root document becomes public and is re-exported with `pub use error::ErrorDocument as ApiErrorDocument;`: Anthropic's present `ErrorDocument` is crate-private. Provider-message and request-ID accessors can move from the existing failure wrapper to that model, or stay behind a compatibility wrapper. This is not a promise of source compatibility with `Error::Api`.

**SDK code that disappears:**

- `examples/anthropic-messages-sdk/src/lib.rs:152-167`: 16 lines of local `SendWith` implementation; its declaration/docs at `142-150` can also be shortened, but are not counted.
- `examples/anthropic-messages-sdk/src/error.rs:227-245`, `256-259`, `271-284`, `299-323`: 62 lines of generic holder/construction/accessor/formatting machinery, replaced by `HttpFailure<D>`. Provider-specific accessors at `246-269` need relocation; their bodies are not counted as deleted behavior.
- Gross target: **78 lines**; allow roughly 5–15 replacement/export/accessor lines, so **about 60–70 net** in this witness. B1's carrier deletion is excluded here to avoid double counting.
- B2 Native's `examples/backblaze-b2-native-sdk/src/wire.rs:82-101` has another 20-line instance of the same adapter. Its status-consistency rule at `src/error.rs:255-275` remains Rust under this minimal design.

**Hardest counterexample:** **Slack.** This design alone does not turn `ok:false` on HTTP 200 into an error. Keep its current typed envelope and Rust check, or use B3. It must never claim that HTTP success is provider success. Cloudflare diagnostics remain successful data; Snowflake's 202 remains an ordinary typed pending result. S3 names an `ErrorXml` codec regardless of its success format; HEAD needs A3 or the existing Rust special case. B2 Native names `Json` explicitly.

For Anthropic streaming, only failure to open the HTTP stream uses this document. Once opened, `MessageStream` continues to report its typed incomplete/error-event outcomes; no stream buffering or reconnection is introduced.

**Simple control (Nager):** Unchanged. No `error` statement is compulsory; its unread-error behavior remains available. PokeAPI and Frankfurter likewise keep their existing declarations and limits.

**Charter check:**

- C1: The new statement says a document type and representation, not how Concord should generate methods.
- C2: Decoding facts are inherited; recovery, refresh, and interpretation of inconsistent documents remain Rust with typed document/head access.
- C3: Entirely opt-in; none of the three controls grows.
- C4: Non-2xx selection and the explicit error codec are visible; no inference from success format or type names.
- C5: Missing `Decode<D>` support and inaccessible public document types must be reported at the `error` statement.
- C6: Caller rule: “HTTP errors retain their head and either this document or why it could not be read.”
- C7: Custom codecs use the same `Decode` contract; native request/response escapes and caller transport remain.
- C8: One execution per default send, no retry or hidden acquisition.
- C9: Require fixtures for 429 plus valid/HTML/empty/malformed/oversized/interrupted documents, canary formatting and source chains, and JSON errors on an SSE-opening failure. These tests are proposed, not run here.

**New concepts:** `error F => D;`, `CallError<D>`, `HttpFailure<D>`, ordinary Rust `KeepHead`; B1's `Response<T>` is a shared dependency, not a second error mechanism. Error-read bounds live in an ordinary Rust read policy. `KeepHead` has a documented fixed 64 KiB default, matching this witness; another budget uses an explicit Rust policy or prepared-read override. No invented provider size limit enters the declaration.

**Breaking changes:** Opting in changes the generated send error type. Direct `Error::Api` matches and explicit result signatures migrate. Making the document public is an SDK surface decision. Client policies must accept the authored response contract as described in the implementation boundary below. SDK-local InvalidRequest and midstream Incomplete errors remain their own types or retain an outer SDK enum.

**Why it might be wrong:**

A uniform document holder solves considerably less than all 374 inventory-labelled error lines in Anthropic; code enums and stream errors are not boilerplate to erase.
The generic failure type may be a worse public API than an existing SDK enum, which is the reason to evaluate A2 separately.

## A2. Keep the SDK's enum; derive its adaptation

**Fact it expresses:** Salesforce's declared HTTP failure document belongs to the SDK's existing nominal failure contract, while local failures remain ordinary Rust variants.

**Declaration (author writes), before → after:** Salesforce, `examples/salesforce-sdk/src/api.rs:39-43`:

```rust
// Before
pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
    base "https://instance.salesforce.invalid";
    path "services/data/v{version}";
    headers { AUTHORIZATION = "Bearer {token}"; }
    format Json;
    // Existing children omitted.
}

// After
pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
    base "https://instance.salesforce.invalid";
    path "services/data/v{version}";
    headers { AUTHORIZATION = "Bearer {token}"; }
    format Json;
    error IgnoreContentType<Json> => ApiErrorBody as SfError;
    // The same children.
}
```

The representation choice preserves current `ApiFailure::read`, which accepts bodies labelled `text/plain`. It is explicitly an SDK compatibility choice already present in the code, not a new claim that Salesforce promises a particular label.

The real enum's leading variants, `examples/salesforce-sdk/src/error.rs:20-31`, become an annotated Rust declaration:

```rust
// Before
#[non_exhaustive]
pub enum SfError {
    Concord(concord::Error),
    Api(Box<ApiFailure>),
    Protocol(Box<ProtocolError>),
    Link(LinkError),
    // Existing Open, Read, Incomplete, Job, Csv variants omitted.
}

// After: same names and payload layouts.
#[derive(concord::Error)]
#[non_exhaustive]
pub enum SfError {
    #[from]
    Concord(concord::Error),
    #[provider]
    Api(Box<ApiFailure>),
    #[from]
    Protocol(Box<ProtocolError>),
    #[from]
    Link(LinkError),
    // The other existing variants receive their corresponding #[from] where appropriate.
}
```

The derive emits `From` delegation, value-free default `Display`/`Debug`, a deliberately empty default `source()` chain, and conversion from the declared typed HTTP failure. It does **not** infer that a variant called `Api` is special. `#[provider]` identifies the nominal wrapper and requires an ordinary Rust `From<HttpFailure<ApiErrorBody>> for ApiFailure`. That conversion retains Salesforce's `into_vec`, `ApiUsage` parsing and `ErrorBodyState` interpretation; approximately 15–25 lines replace the current asynchronous reader. No callback, formatter, or recovery expression is embedded in `api!`.

Unlike A1, the compiler returns **`SfError`**, not a new core enum at every public SDK call. The derive only handles mechanical wrapping; provider-specific methods such as `has_error_code`, `is_expired_locator`, and usage interpretation remain handwritten.

**Call site (caller writes), before → after:**

```rust
// Before: the public generated operation needs explicit async adaptation.
let result = match ctx.api().accounts().describe().send().await {
    Ok(value) => Ok(value),
    Err(error) => Err(SfError::from_concord(error).await),
};
match result {
    Err(SfError::Api(failure)) => inspect(failure.status(), failure.errors()),
    Err(SfError::Concord(error)) => inspect_transport(error),
    other => consume(other),
}

// After: generated send implements the same nominal failure contract.
let result = ctx.api().accounts().describe().send().await;
match result {
    Err(SfError::Api(failure)) => inspect(failure.status(), failure.errors()),
    Err(SfError::Concord(error)) => inspect_transport(error),
    other => consume(other),
}
```

The public `OrgContext::api` accessor is `examples/salesforce-sdk/src/context.rs:115-117`; the asynchronous conversion is `src/error.rs:49-58`. This preserves pattern matching, not all textual diagnostics or helper functions. A public `from_concord` compatibility method can stay for errors obtained through deliberately raw/native paths. CSV open/read failures continue to enter their existing variants in Rust.

**SDK code that disappears:**

- `examples/salesforce-sdk/src/error.rs:102-144`: 43 lines of seven conversion implementations, reproduced by the annotated enum.
- `examples/salesforce-sdk/src/error.rs:146-180`: 35 lines of outer formatting/debug/error-trait delegation. The derive defaults withhold payloads; authors requiring the precise current diagnostic text keep their handwritten formatting and do not count those 35 lines.
- `examples/salesforce-sdk/src/error.rs:207-236`: the 30-line asynchronous read/assembly method is replaced by the synchronous 15–25-line conversion above; the document model and body-state model remain.
- Gross target **108 lines**, net roughly **65–80** after attributes and conversion code; about **30–45 net** if exact current formatting is retained. Transport wrappers at `examples/salesforce-sdk/src/transport.rs:59-72` can shrink when their callers migrate, but are not counted until that migration is checked.

**Hardest counterexample:** **Callers matching SDK variants.** `SfError::Api(Box<ApiFailure>)` and the other payload layouts remain unchanged. This is the main advantage over A1/A3. The derive must reject two `#[provider]` destinations or duplicate `#[from]` source types at their attributes, rather than guess.

Slack's `Api` versus `RateLimited` distinction is not inferable from enum names; keep its Rust classification or explicitly combine B3/A3. Cloudflare's attempts counter also remains policy-owned. S3 can keep its rich `S3Error` variants with a custom conversion, but this derive does not automatically know its 304 rule. A successful GraphQL response containing errors is not routed into the provider variant by the derive.

**Simple control (Nager):** Unchanged; no derive or nominal error mapping is required. Its current `Error` and policy remain valid.

**Charter check:**

- C1: `error … as SfError` names the SDK's failure contract, analogous to a nominal success output; derive-generation choices stay in ordinary Rust attributes, outside the HTTP DSL.
- C2: The document/format association is declared once. Body-state interpretation and usage parsing remain typed Rust. No automatic recovery is derived.
- C3: Both the DSL association and derive are optional; the controls retain their source shape.
- C4: Explicit `#[provider]`/`#[from]` determine routing; names, order and payload types do not cause hidden inference.
- C5: Ambiguous conversions and incompatible provider wrappers fail at the enum attribute or `as SfError`, naming the authored variant/type.
- C6: “Keep your enum; mark the wrapped failures you want converted.” Complex custom mappings stay explicit Rust.
- C7: The async document reader still uses any declared codec; ordinary manual `From`, formatting and native access remain possible.
- C8: The derive is synchronous adaptation after one execution; no I/O or recovery in error conversions.
- C9: Require downstream match/struct-pattern compilation, conversion compile-fail cases, text/plain Salesforce error fixtures, and a canary in every variant plus its source chain.

**New concepts:** `as SdkError` on a document fact; `#[derive(concord::Error)]`, `#[provider]`, `#[from]`; a typed synchronous conversion from `HttpFailure<D>`. No generated provider-code taxonomy.

**Breaking changes:** None to the chosen enum's variant names/layouts if the existing payloads are retained. Generated endpoint send errors change from `concord::Error` to `SfError` on adoption. Default formatted messages change unless handwritten formatting is kept. Removing legacy transport helpers or `from_concord` is a separate, avoidable public break. No blanket conversion is generated for arbitrary SDK protocol failures.

**Why it might be wrong:**

This may merely replace a few dozen easy lines with another derive and a complicated public conversion contract; existing Rust error crates already solve part of that problem.
The `as` clause must remain a nominal result contract, not grow into a DSL for constructing arbitrary Rust error variants; otherwise reject the design under C1/C6.

## A3. A nominal table of HTTP failure cases

**Fact it expresses:** Specific HTTP failure statuses have distinct documented outcomes and body representations, including outcomes with no document.

**Declaration (author writes), before → after:** S3 client, `examples/amazon-s3-sdk/src/api.rs:37-38`:

```rust
// Before
base "https://s3.us-east-1.amazonaws.com";
send signer;

// After
base "https://s3.us-east-1.amazonaws.com";
errors S3Status {
    304 => NotModified;
    otherwise => Api(crate::xml::ErrorDocument) via ErrorXml;
}
send signer;
```

For the real HEAD endpoint at `examples/amazon-s3-sdk/src/api.rs:97-107`, add the provider's exception without changing its inputs:

```rust
// Before: the preceding HEAD parameter list is unchanged.
) -> ObjectHead {
    path "{key}";
    headers {
        "range" = "{range}";
        "if-match" = "{if_match}";
        "if-none-match" = "{if_none_match}";
        "if-modified-since" = "{if_modified_since}";
        "if-unmodified-since" = "{if_unmodified_since}";
    }
    response HeadFormat;
}

// After: the same endpoint body, with this one additional fact.
error body absent;
```

The table is consulted only for non-2xx HTTP responses. Exact statuses must be disjoint; an optional `otherwise` names the remaining non-2xx set and is not order-dependent. Without a matching case, retain a generic unexpected-status failure, never reinterpret it as success. An author can use disjoint status classes as cases; unresolved overlaps are errors, not “first wins.” Scope tables replace inherited tables as a unit; there is no implicit merge.

`S3Status` is a generated nominal sum: `NotModified` contains the head and no document, `Api` contains `HttpFailure<ErrorDocument>`. Its outer error is `CaseError<S3Status>`, with `Concord(concord::Error)` and `Http(S3Status)` variants; A1's homogeneous `CallError<D>` remains a separate alternative. The HEAD statement keeps that same sum but marks its document outcome `NotExpected` instead of attempting XML parsing. It does not map HEAD 404 to `Option::None`. Conditional validators and S3 header accessors can remain models over the retained head.

`ErrorXml` is a normal custom `Decode<ErrorDocument>` adapter around the existing `parse_error_document`; it does not reuse `ListXml`, `HeadFormat`, or `Download`. This separates “the representation of an object” from “the representation of a failure.” SDK read bounds remain Rust settings, preserving the present 64 KiB bound.

**Call site (caller writes), before → after:**

```rust
// Before
match api.bucket(bucket).head_object(key).send().await {
    Err(S3Error::NotModified(previous)) => use_cached(previous),
    Err(S3Error::Api(error)) => inspect(error.status(), error.code()),
    other => consume(other),
}

// After: this alternative deliberately exposes the generated HTTP sum.
match api.bucket(bucket).head_object(key).send().await {
    Err(concord::CaseError::Http(S3Status::NotModified(head))) => use_cached_head(head),
    Err(concord::CaseError::Http(S3Status::Api(error))) => {
        inspect_document(error.meta().status(), error.document());
    }
    other => consume(other),
}
```

Signing/input/body/protocol errors still need the SDK's Rust outer error if the caller wants one combined result. A2 can adapt the nominal cases back to `S3Error`; that compatibility conversion is additional code and is not claimed as a deletion here.

**SDK code that disappears:**

- `examples/amazon-s3-sdk/src/client.rs:385-407`: 23 lines of shared response routing replaced by the status contract. S3's selected metadata projection still needs its model/accessors; signing and range checks stay.
- `examples/amazon-s3-sdk/src/error.rs:263-304`: 42 lines of bounded-read/assembly machinery, replaced by roughly 12–20 lines implementing the ordinary `ErrorXml` adapter plus shared `HttpFailure` state. The XML model/parser at `examples/amazon-s3-sdk/src/xml.rs:289-314` remains.
- Gross **65**, net about **40–50 lines** before optional compatibility work. The existing `NotModified` header model at `src/error.rs:318` onward is not counted as removed.
- Registry's `examples/distribution-registry-sdk/src/error.rs:758-783` has similar document wrapping, but its challenge extraction at `751-755` stays Rust. Status-checked upload/session semantics are not automatically error-table savings.

**Hardest counterexample:** **S3 HEAD and a binary/streaming GET with an XML refusal.** Status selection happens before opening/decoding success, and HEAD never reads a nonexistent error document. `NotExpected`, `Empty`, `Malformed`, `Limit`, and `ReadFailed` are distinct observations; a failed XML parse cannot replace the HTTP status.

**Snowflake:** 202 is outside this failure table because it is HTTP success. Keep `StatementBody::Pending` and the existing Rust poller. A separate success-status union can replace its format's small match, but that is not necessary for this proposal and no polling engine follows from the table. Slack/Kraken's 200 failures require B3 or their existing Rust checks. Linear partial data does not become an HTTP error.

**Simple control (Nager):** Unchanged. It need not list 2xx or declare a default failure table. Its 400 classification can stay in its present Rust policy.

**Charter check:**

- C1: Each case is an HTTP status/document fact; `error body absent` states HEAD's provider contract.
- C2: Cases are typed for policies; cache use, retry, range reconciliation and polling stay Rust.
- C3: Tables are absent unless an SDK opts in; the controls gain no cases or lines.
- C4: Non-2xx-only matching, disjoint cases and an explicit remainder make precedence visible.
- C5: Duplicate statuses, overlapping classes and unsupported codecs point to the conflicting cases.
- C6: “An HTTP status selects one documented failure case; bodyless cases do not decode.” No per-case actions.
- C7: Every case accepts an ordinary custom codec; status-neutral receive and native response ownership remain.
- C8: A case selects data interpretation after one execution; it never repeats or waits.
- C9: Require 304/412/404 HEAD fixtures with a body-read trap, GET XML versus proxy HTML, overlap compile-fail cases, head-retention assertions and secret canaries.

**New concepts:** `errors Name { status => Variant; … }`, explicit remainder, bodyless failure fact, named generated HTTP sum, `CaseError<H>` for that sum, shared document-state enum. This is a nominal status algebra, unlike A1's one homogeneous document or A2's reuse of an existing Rust enum.

**Breaking changes:** The chosen public result acquires a named status sum; existing `S3Error` patterns need A2-style compatibility or migration. HEAD adds a declaration line because it has a real exceptional fact. Unknown-status handling must remain explicit. Export names and nominal-sum evolution require the same semver care as handwritten enums.

**Why it might be wrong:**

Most witnesses use one document for all failure statuses; a status algebra would add ceremony without replacing substantially more code than A1.
S3's 304 is useful as a cached-result outcome, so forcing it into a generated error sum may preserve today's API accident rather than the best caller contract.

# Family B — results

## B1. A shared safe value/head holder — smaller than you think

**Fact it expresses:** An HTTP answer has a decoded value and the response head that produced it; this is common structure, not a provider-specific DSL switch.

**Declaration (author writes), before → after:** Notion's real declaration, `examples/notion-sdk/src/api.rs:17-20` and `27`, is intentionally identical:

```rust
// Before
base "https://api.notion.com/v1";
headers { AUTHORIZATION = "Bearer {token}"; }
format Json;
limit crate::RESPONSE_LIMIT;
// Other existing endpoints omitted.
GET page(page_id: String) -> Page { path "pages/{page_id}"; }

// After: exactly the same declaration.
base "https://api.notion.com/v1";
headers { AUTHORIZATION = "Bearer {token}"; }
format Json;
limit crate::RESPONSE_LIMIT;
// The same other endpoints.
GET page(page_id: String) -> Page { path "pages/{page_id}"; }
```

Change the SDK's ordinary Rust export, not the HTTP language:

```rust
// Before: examples/notion-sdk/src/client.rs:9-22 defines the local Response<T>.
// After
pub use concord::Response;
```

Core provides `Response<T> { pub value: T, pub meta: ResponseMeta }`, tuple conversion, `map`, `into_value`, and `into_parts`. Its `Debug` prints the core head safely and **always withholds the value**, without `T: Debug`. An additive `Received::decode_response()` is the named-holder equivalent of today's `decode_with_meta()`. It preserves finite limits, no-output drain semantics and streaming open semantics.

There is no `response_meta;`, `with_response;`, or “turn on wrappers” statement. Ordinary Rust policy decides to return the carrier. A generic `KeepHead` policy is useful for cases with no other behavior, but is not injected into every client.

**Call site (caller writes), before → after:**

```rust
// Before
let response = client.retrieve_page(page_id).await?;
use_page(response.value);
inspect(response.meta.headers());

// After: unchanged, including the SDK's re-exported Response name.
let response = client.retrieve_page(page_id).await?;
use_page(response.value);
inspect(response.meta.headers());
```

The corresponding success assembly in `receive` can change from `let (value, meta) = received.decode_with_meta().await?; Ok(Response { value, meta })` to `Ok(received.decode_response().await?)`. Its non-2xx validation remains unchanged. No new terminal method is required of callers.

**SDK code that disappears:**

- `examples/notion-sdk/src/client.rs:9-22`: **14 lines**, replaced by a re-export. The two-line success assembly at `227-228` contracts to one line; the 18-line error-body branch is not deleted.
- `examples/kraken-spot-rest-sdk/src/policy.rs:14-28`: **15 lines** of the same safe holder, replaced by a re-export; signer and error checks remain.
- Combined witness target about **27 net lines** after two re-exports, plus a small assembly simplification. These are most of the **holder-only** lines in those witnesses, not most of all result glue.
- Anthropic `examples/anthropic-messages-sdk/src/lib.rs:118-127` is another 10-line holder, but its inherent `request_id()` at `129-134` needs an SDK wrapper or an imported extension trait. Do not count that migration as free.
- Stripe's metadata `examples/stripe-sdk/src/send.rs:79-133` stays. Its holder at `135-142` uses that custom metadata and is **not** replaceable by this deliberately small `Response<T>` alone. A generic metadata parameter is deferred until it proves necessary.

**Hardest counterexample:** **Slack/Cloudflare diagnostics on success.** `Response<T>` only retains the HTTP head. It does not automatically preserve body warnings, error lists, page information, or extensions. Use the complete envelope as `T`, retain the existing SDK-specific holder, or adopt B2/B3. Replacing Slack's five-field `Response` with `Response<Conversation>` would lose information and is explicitly disallowed.

For no-output endpoints, `Response<()>` comes from draining the successful body; it does not mean JSON null. For streams, the carrier holds the opened reader/Incoming and head; it does not collect the stream. Generic code can continue to ask for `Sends<Result<Response<T>, E>>` where the SDK has chosen that policy.

**Simple control (Nager):** Unchanged, including its named `HolidayResponse { holidays, meta }`. A universal forced migration would break C3 and gains little. PokeAPI's custom final-URL metadata and Frankfurter's `RateResponse { rate, meta }` also remain untouched.

**Charter check:**

- C1: No language word is added for a library packaging preference.
- C2: Provider data models stay provider models; the library supplies generic mechanical structure and safe metadata ownership.
- C3: Zero declaration changes in all three controls, including their present return shapes.
- C4: Public fields match the existing repeated holder; no envelope or success inference.
- C5: Ordinary Rust type errors at the alias/use site; no new macro inference.
- C6: “Value plus the head it came with.”
- C7: `into_parts` and existing raw/native ownership paths remain available.
- C8: Converting a tuple to a holder adds no execution, retry or state.
- C9: Require a non-Debug payload compilation test, a secret-canary payload whose Debug would leak if invoked, header-value canaries, finite-limit checks, and a stream-open test proving no body polling.

**New concepts:** Core `Response<T>` and an additive tuple-to-holder receive method; optional ordinary Rust `KeepHead`. No keyword, new provider fact grammar or universal result-shape change.

**Breaking changes:** Opt-in replacement changes the concrete type's defining crate and may change trait implementations or debug text. Preserve only justified common traits: do not add a `Debug` payload bound. SDK-specific inherent accessors cannot be attached to an external type, so keep those wrappers unless their caller migration is accepted. Existing `ResponseMeta` customizations do not disappear.

**Why it might be wrong:**

A dozen deleted lines per SDK is not a reason to break established provider-specific names or accessors.
If most remaining holders have extra body fields or custom metadata, the small generic type may have fewer useful adopters than the inventory's “20 wrappers” suggests.

## B2. Keep the complete wire model; expose a typed value view

**Fact it expresses:** The endpoint's logical data lives inside a shared wire envelope, which also contains independently useful diagnostics.

**Declaration (author writes), before → after:** Linear, `examples/linear-sdk/src/lib.rs:54-60`:

```rust
// Before
request Graphql;
response Json;
limit BODY_LIMIT;
POST issues(body request: GraphqlRequest<ListVariables>) -> Envelope<IssuesData>;
POST issue(body request: GraphqlRequest<IssueVariables>) -> Envelope<IssueData>;

// After
request Graphql;
response Json;
envelope Envelope<_>;
limit BODY_LIMIT;
send KeepGraphqlErrors -> Result<concord::EnvelopeResponse<_, Envelope<_>>, Error>;
POST issues(body request: GraphqlRequest<ListVariables>) -> IssuesData;
POST issue(body request: GraphqlRequest<IssueVariables>) -> IssueData;
```

`KeepGraphqlErrors` is the existing response adapter moved into an ordinary Rust policy; its HTTP raw-body/error semantics remain. It returns the shared envelope holder rather than defining a local one. This alternative does not require a new GraphQL engine or a different JSON decoder.

The real envelope model at `examples/linear-sdk/src/lib.rs:141-151` remains the authoritative wire schema. Add only the projection annotation:

```rust
// Before
#[derive(Clone, Debug, Deserialize)]
pub struct Envelope<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GraphqlError>,
    #[serde(default)]
    pub extensions: Option<Value>,
}

// After
#[derive(Clone, Debug, Deserialize, concord::Envelope)]
pub struct Envelope<T> {
    #[concord(value)]
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GraphqlError>,
    #[serde(default)]
    pub extensions: Option<Value>,
}
```

The derive implements a small view trait, not decoding or success classification. Its generated view of this field is `Option<&T>`, not `&T`, and it emits no `unwrap`. The single owned envelope remains reachable through `.envelope()` and `.into_envelope()`; `.value()` borrows its selected field. No second payload copy or self-referential reference is stored. A consuming caller can take `(Envelope<T>, ResponseMeta)` and move fields normally.

Here `_` in the **new envelope statement and its send template** is the endpoint's written logical type (`IssueData`). `Envelope<_>` is the explicit wire type. Existing declarations without `envelope` keep today's placeholder meaning. The generated signature is the concrete `Result<EnvelopeResponse<IssueData, Envelope<IssueData>>, Error>`, and `Sends<O>` uses that exact result. No “look inside the outer generic type name” heuristic is involved.

**Call site (caller writes), before → after:**

```rust
// Before
let response = linear.issue("ENG-1").await?;
let selected: Option<&IssueData> = response.envelope.data.as_ref();
inspect(&response.envelope.errors, &response.envelope.extensions);

// After: the existing validated facade may stay; the wire operation also has a named result.
let response = linear.issue("ENG-1").await?;
let selected: Option<&IssueData> = response.value();
inspect(&response.envelope().errors, &response.envelope().extensions);
```

The facade still constructs fixed GraphQL documents and validates IDs. Existing `is_success()` is ordinary Rust over presence plus diagnostics; it can remain on a thin SDK wrapper or become an extension method. It is not generated from a field called `errors`.

**SDK code that disappears:**

- `examples/linear-sdk/src/lib.rs:155-162`: **8 lines** defining the local response holder become an alias/re-export of the shared holder.
- `examples/linear-sdk/src/lib.rs:242-243`: **2 lines** assembling envelope plus head become a shared `decode_envelope_response` operation. The HTTP branch at `225-240` remains in the Rust policy; errors and raw bytes are still retained there.
- Gross **10 lines**, about **5–8 net** after export/derive annotations. Moving a function into a policy is not counted as deleting its body. If an inherent-method compatibility wrapper is retained, the holder saving falls correspondingly.
- Repeated `Envelope<…>` shells disappear from the two endpoint outputs at `58-60` and the result signature becomes concrete. This is mainly a semantic/documentation improvement, not a large line-count win. It addresses B and H; it does not by itself eliminate A or F glue.

**Hardest counterexample:** **Linear data beside errors, including a null connection.** The value view remains optional; errors, extensions, error paths, locations and unknown error members remain in the full typed envelope. A present `IssuesData { issues: None }` is still distinguishable from absent/null top-level data. The current model already preserves that inner nullable connection; page validation remains Rust. A selected field missing from the body still follows its existing Serde presence rule.

This mechanism is deliberately neutral about Kraken and Slack success. If used there, their Rust policies must still check the envelope before exposing a successful domain value; it does not repair Cloudflare's one-pass discriminator problem. A full custom envelope model can keep the two-phase deserializer already written. An endpoint with no output has no finite value to project; a stream continues to use its incremental format and explicit event schema. Neither is wrapped in a fictitious JSON envelope.

**Simple control (Nager):** Unchanged. No implicit envelopes, derives, or changes to `_` in existing send templates. PokeAPI and Frankfurter likewise retain the current meaning of their written output types.

**Charter check:**

- C1: `envelope Envelope<_>` states a wire relationship; the annotated field states where that model's data is.
- C2: The schema owns that structural fact once. Completeness/error policy stays Rust and can inspect the entire typed envelope.
- C3: New logical/wire separation applies only with an explicit envelope statement; controls do not grow or change meaning.
- C4: Written logical type, written envelope constructor, one marked field; no convention based on `data` or `result` names.
- C5: Zero/multiple value fields, invalid envelope arity, and an incompatible policy output point at the field/envelope/template respectively.
- C6: “Name the shared envelope once; view its data while keeping the whole answer.” Optional data stays visibly optional.
- C7: Existing Deserialize/custom codecs remain authoritative; raw documents and native request ownership are still available.
- C8: Projection borrows one decoded value; it performs no new request or independent state mutation.
- C9: Require errors-only, partial-data, null-connection and missing-selected-field fixtures; non-Clone envelope compilation; a downstream `Sends<O>` check; and a compile test showing old templates keep their meaning.

**New concepts:** `envelope E<_>;`, `#[derive(concord::Envelope)]`, one value-field annotation, a view trait and `EnvelopeResponse<T, E>`. No predicates, success extraction, error classification or new serializer.

**Breaking changes:** Adopting the statement moves wire genericity out of endpoint outputs. Native prepared output remains the explicit wire type; policy bounds must use it. The public holder changes from a field `envelope` to an accessor, unless a compatibility wrapper is retained. `value()` borrows and can be optional, so this does not preserve a by-value `.value` API. Inherent helper methods may require a wrapper or trait import.

**Why it might be wrong:**

This adds several public types to save very little source; a normal `Response<Envelope<T>>` plus ordinary field access may be clearer.
The logical/wire split could make `_` harder to teach than today's single substitution, even when both meanings are explicit. Prefer B1 alone if that happens.

## B3. Declare a discriminated body contract; generate the envelope adapter

**Fact it expresses:** A body discriminant selects either a typed value with diagnostics or a provider refusal, before the value field is decoded.

**Declaration (author writes), before → after:** Cloudflare, `examples/cloudflare-zones-sdk/src/lib.rs:70-72`, and its real single-object endpoint at `105`:

```rust
// Before
format Json;
limit SUCCESS_BODY_LIMIT;
send policy;
// Under scope zone(zone_id: ZoneId):
GET get() -> Single<Zone>;

// After
request Json;
response Json {
    discriminant "success": bool;
    true => value "result": _, notes CloudflareNotes;
    false => error ErrorEnvelope;
}
limit SUCCESS_BODY_LIMIT;
send policy -> Result<concord::Reply<_, CloudflareNotes>, Error>;
// Under the same scope with the same path:
GET get() -> Zone;
```

The real listing endpoint's `-> Paged<Zone>` at `examples/cloudflare-zones-sdk/src/lib.rs:97` becomes `-> Vec<Zone>`; both use the same inherited response contract. `CloudflareNotes` is an ordinary Serde model containing defaulted `errors: Vec<ResponseInfo>`, `messages: Vec<ResponseInfo>`, and `result_info: Option<PageInfo>`. These fields are not converted into a traversal decision. All request inputs, paths and rate/replay policies stay the same.

Semantics are a finite tagged data schema, analogous to a Rust enum, not executable DSL control flow: read the declared scalar tag; select its declared shape; decode only that shape. On `true`, `result` must exist, be non-null and decode as the endpoint's type. Decode notes from the same document. On `false`, decode `ErrorEnvelope` from the whole body and do not decode `result`. A missing/non-boolean tag, absent/null required result, or unrecognized closed-tag value is a contract/decode failure with head and bounded raw document retained.

`Reply<T, N>` owns `value`, `notes` and the HTTP head. Explicit raw-document access retains otherwise unmodeled information; ordinary Debug withholds the body, value and notes. Bodies remain bounded by the existing finite-output limit. The policy receives a typed refusal by value rather than a `CodecError` source to downcast. It still decides how to classify uncertain creates and when to retry reads; the response contract only describes the answer to one attempt.

Unlike B2, this alternative does **not** require a handwritten generic `Single<R>`/`Paged<R>` deserializer or retain those types as the wire authority. The declaration owns the discriminator/projection relationship; Rust supplies only the payload and notes models. Unlike B1, it interprets a provider-specific body contract, not merely a tuple.

**Call site (caller writes), before → after:**

```rust
// Before
let response = cf.zone(zone_id).get().send().await?;
use_zone(response.result);
inspect(response.errors, response.messages);

// After: diagnostics on a successful answer stay available.
let response = cf.zone(zone_id).get().send().await?;
use_zone(response.value);
inspect(response.notes.errors, response.notes.messages);
```

The generated send signature literally names `Result<Reply<Zone, CloudflareNotes>, Error>`; `_` is the written endpoint value. `Sends<O>` exposes the same type, without an associated-policy-output puzzle. A non-2xx body can still use A1/A3's independent error document. HTTP error decoding is selected before this 2xx body contract, so a JSON note error cannot hide HTTP 429.

**SDK code that disappears:**

- `examples/cloudflare-zones-sdk/src/wire.rs:123-176`: **54 lines** of RawValue staging, parsing and two manual Deserialize implementations.
- `examples/cloudflare-zones-sdk/src/policy.rs:16-45`: **30 lines** of two handwritten successful response holders, replaced by the common reply and the notes model. The notes model costs about 10–16 lines and remains provider data, not a claimed disappearance.
- Envelope mapping at `examples/cloudflare-zones-sdk/src/policy.rs:173-197` and `215-238`: **49 lines** collapse into typed contract decoding plus short Rust error/uncertainty conversion. Reserve about 10–18 replacement lines. The attempt count, create-failure rule at `201-207`, target validation and replay loop at `98-121` remain.
- Gross **133**, estimated **about 90–105 net physical lines** after the new schema, notes model, exports and residual policy conversions. The old `Single`/`Paged` enum definitions at `src/wire.rs:89-121` also become unnecessary internally, but are public exports; deletion is a separately accepted break and not in this estimate.
- Cross-family candidate: QuickBooks can declare root versus nested Fault presence before its value branch. Then `examples/quickbooks-online-sdk/src/format.rs:13-70` (**58 lines**) and `src/policy.rs:174-182` (**9 lines**) disappear as the erased-source workaround; typed policy matching replaces the present `fault_in` calls. This requires an explicit schema choice for the root/nested fault cases, not a global “any Fault-like name is an error” convention.
- Slack's `examples/slack-sdk/src/lib.rs:253-278` is a further 26-line candidate for replacing application-envelope routing, once its full body contract below is declared. Its HTTP 429 classification and traversal remain Rust. Do not count the whole Client wrapper as removed.

**Hardest counterexample:** **Cloudflare errors on success and a malformed result on failure.** Only `success` selects the branch. A true response with nonempty `errors` is successful and retains those entries in notes. A false response with `result: { unexpected: ... }` decodes the refusal without trying `Zone` or `Vec<Zone>`; this is the exact failure that naive derived envelopes cannot handle.

The same mechanism must admit these explicit, independently tested schemas:

| Counterexample | Required contract, without inferred behavior |
|---|---|
| Slack | Required boolean `ok`; false requires a nonempty error code, with warnings/scope/metadata retained. True forbids an `error` field and requires the endpoint's `members` or `channel` payload. Preserve the distinction between an omitted field and a present contradictory one. Use a validated nonempty-code model; no magic truthiness. |
| Kraken | The declared error-list shape has an empty branch selecting required `result`, and a nonempty branch retaining the ordered strings. The conservative choice that even warning strings fail is an explicit SDK contract, not a universal meaning of a field called `error`. Classification remains Rust. |
| Linear | **Do not use a refusal branch merely because errors exist.** Its response schema has optional `data` plus retained diagnostics together. Keep the full envelope (B2 or the existing model), or declare a product of value/notes. A caller chooses strict-versus-partial acceptance in Rust. |
| Snowflake | A status discriminant can expose `Pending(ServerMessage)` at 202 and `Complete(ResultSet)` at 200 as an ordinary success union. The existing `StatementJson` remains a valid custom-format alternative; neither representation polls. |
| S3 / B2 Native | Their independent error codecs are respectively XML and JSON here. They do not use a JSON success-body discriminator merely to gain typed HTTP errors. |

Boolean, field-presence and empty/nonempty-list partitions must be a **closed, small repertoire of data-shape facts**, not arbitrary predicates or Rust expressions. Do not implement every row by adding a separate provider keyword. The first prototype should prove Cloudflare plus Slack; if those already require a rules language, reject this alternative. The table describes required coverage, not a claim that the displayed boolean-only grammar already implements all of it.

For streaming responses, the incremental format remains responsible for event envelopes and incomplete reads. A `response MessageEvents;` replaces the entire inherited finite response contract, just as response formats currently replace each other. For an endpoint with no `->`, there is no typed envelope: success is still drained. An API whose acknowledgement body has an `ok` contract must declare an actual output model, not rely on omission of `->` to validate a body.

**Simple control (Nager):** Unchanged. `response Json;` and `format Json;` keep exactly today's meaning. No output wrapper, classifier, new policy trait bound or declaration line is imposed on the three controls.

**Charter check:**

- C1: A discriminant, field location, required value and diagnostic model are response facts; there are no retry instructions, callbacks or generated-surface controls.
- C2: Typed branch outcomes replace SDK restatement/downcasts; retry, refresh, uncertainty and traversal stay Rust with typed head/notes/refusal access.
- C3: Existing response statements are unchanged; only providers with an authored body contract opt in.
- C4: No generic errors-field heuristic. Branch partitions are explicit, disjoint and checked; inherited response contracts are replaced as a unit.
- C5: Overlapping/missing closed-tag branches, an invalid field selector and absent codec capabilities fail at the authored response schema. Runtime provider contradictions report the operation and named field, not macro internals.
- C6: The teachable rule is “Select the documented body shape, then decode its value and keep its diagnostics.” Reject rather than expand into a general expression language.
- C7: Projection requires a small opt-in document-view capability implemented by the selected codec; JSON is an implementation, not a privileged core AST. Custom XML/other codecs may implement the same capability, and codecs without it can keep their Rust decoder. Native escapes remain.
- C8: Schema interpretation is deterministic for one received body. No implicit request, poll, refresh, retry, sleep or global observer.
- C9: Require Cloudflare's failure-with-wrong-result fixture and success diagnostics, Slack missing/false/conflicting fields, Kraken simultaneous errors/result, QuickBooks both Fault locations without downcasting, Linear partial-data retention, Snowflake pending without a second request, downstream `Sends<O>` compilation, and redaction canaries in notes/raw documents/source chains.

**New concepts:** A finite discriminated response schema, value selector, ordinary notes model, `Reply<T, N>`, typed body refusal, opt-in codec document-view capability. This is the largest design in the set; its bounded shape vocabulary is a kill condition, not an invitation to build a framework.

**Breaking changes:** Adopting SDKs change wire output types and public successful response fields. Cloudflare's public wire enum exports need compatibility types or a major release. Existing send policies must adopt typed contract decoding and preserve their own attempt/uncertainty classifications. `_` means the explicitly declared value for this new syntax; existing templates keep their meaning. Keeping bounded raw documents has a memory cost that must be measured and avoided for streams.

**Why it might be wrong:**

Even a finite shape vocabulary may exceed C6 once Slack's contradictions, Kraken's lists and nested QuickBooks faults are included; the correct answer may be a Rust model derive plus a much smaller core transport change.
It can appear to delete semantics by moving them into the macro. Acceptance requires wire/canary/compile-fail tests for the exact lost branches, not just a smaller glue count.

# Implementation boundary and choice

The current macro has **one** output token stream: `crates/concord-macros/src/emit.rs:2088-2158` uses it for `SendWith`/`SendOperation`, the send template and `Sends<O>`. `with_output` at `2265-2289` replaces every type `_` with that whole output and deliberately does not inspect macro bodies or expressions. Merely changing that substitution to the first generic argument would break legitimate outputs and is not one of these designs.

A1–A3 need an authored response contract carried to the response boundary. A coherent implementation can add a defaulted contract parameter to `Prepared<T, C = Plain>`, `Received<T, C = Plain>`, `Decoder<T, C = Plain>`, and the endpoint/policy traits. Existing undeclared contracts use `Plain`. Authored-contract policies explicitly accept `C` and receive typed facts; they finish using the same decoder after native signing or request edits. This has real trait-migration costs for adopting SDKs; it must not be hidden behind generated associated types that authors cannot diagnose.

For these opt-ins, `send()`, the prepared send, and native `decoder.finish/decode` must agree on the declared **response** contract. Preparing remains free of signing/refresh behavior, as today. `receive()` still returns any status with the body unread. `send_raw`, `into_response`, and deliberately selected `document::<F, D>` remain explicit escapes. Error-read limits and raw-body retention are ordinary Rust read settings, with each witness's present bound preserved; declarations must not pretend SDK safety caps are provider maximums. A success limit must not accidentally apply to a stream or replace a smaller HTTP-error bound.

B2 instead needs distinct logical and wire types and a view trait; B3 needs a typed refusal channel in addition to ordinary codec errors. A provider refusal is data, not a parser failure, and should never travel through an erased `std::error::Error` source. On every path retain the original head/operation and distinguish HTTP refusal, valid body refusal, and failure to decode the selected body shape.

I would prototype **A1 + B1** first: the smallest common machinery, with explicit model/behavior leftovers. Run **A2** as a separate migration experiment against Salesforce's existing matches. Treat **A3** as a justified exception for providers with actual status-dependent contracts, not a new default. Compare **B2** against simply returning `Response<Envelope<T>>`; its tiny measured savings may not pay for its concepts. **B3** is the high-payoff/high-risk candidate across A, B, G and H: it can remove envelope adapters, the QuickBooks typed-error workaround, result wrappers and explanatory result docs together, but only if its fact vocabulary stays small.

No A/B proposal solves construction invariants. In particular, removing an error adapter does not justify publishing a private client with unsafe setters/base changes, or counting Salesforce/Slack/Notion construction wrappers as eliminated. The open construction problem remains binding on any facade-removal claim.
