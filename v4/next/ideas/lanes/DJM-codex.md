# D / J / M — codex idea lane

Evidence baseline: read-only `work/wt/rv-p1`, commit `ba10189b`, 2026-11-09. All source references below are relative to that repository. This is a design document; the proposed APIs are not implemented or claimed to compile today. Declaration excerpts omit documentation only, except where an omission is explicitly marked. Estimates count physical source lines removed, then describe replacement cost; they are not classifier scores and are not additive across alternatives.

Read the charter, D/J/M briefs, construction problem, current language guides, macro composition/emission code, glue inventory, and relevant witness/control source and tests. `work/inventory/FRICTION.md` was absent; SDK-local FRICTION/MAINTENANCE notes supplied the fallback. No other idea-lane or design output was consulted.

The inventory reports 1,478 nonblank redaction-glue lines, versus the charter's older 2,954-line Debug count. Its witness redaction counts are Salesforce 121, Elasticsearch 61, GitHub 56, ClickHouse 51, Registry 49, Xero 65, Shopify 37, Mailgun 32, S3 31 and Twilio 31. These are heuristic buckets, not an assertion that every formatter can disappear. In particular, a wire `Display` is often necessary serialization, not redaction boilerplate.

Three evidence details constrain all proposals:

* Twilio deliberately prints Message SID, status and error code (`twilio-sdk/src/models.rs:129-165`); hiding the entire Message is a regression. Europe PMC's publication source/ID are deliberately public in generated paths (`europe-pmc-sdk/tests/canary.rs:1-13`). No design infers secrecy from names such as `id`, `sid`, `user` or `username`.
* The brief's public ClickHouse username example differs from this checkout: `clickhouse-sdk/src/wire.rs:26-30` and `tests/canary.rs:7-10` explicitly classify its **authentication** username as sensitive. Preserve that fact here; a separately declared public username stays printable. Do not silently rewrite the SDK's current promise.
* `clickhouse-sdk/src/wire.rs:83-91` positively tests that a dynamic `param_x` value is visible in Prepared Debug, although endpoint Debug hides it. AUTHOR_GUIDE §4.1a currently forbids `#[sensitive] query(..)`. Reusing today's Prepared Debug verbatim for automatic logs would therefore fail the intended guarantee.

The simple controls remain byte-for-byte declarations under every design: Nager `src/lib.rs:15-39`, PokeAPI `src/lib.rs:70-88`, Frankfurter `src/lib.rs:19-54`. Features and test dependencies belong in Cargo/Rust, never `logging`, `testing` or `controls` statements in `api!`. None of these designs claims to solve the construction/credential-host invariants in CONSTRUCTION_PROBLEM.md.

## D1. Selective Debug derive — smaller than you think

Fact it expresses: Twilio's account, addresses, message text and provider explanation are private; its Message SID, status and error code are public diagnostic data.

Declaration (author writes): the Rust model is the declaration of these field facts. The endpoint `GET get_message(sid: String) -> Message { path "Messages/{sid}.json"; }` (`twilio-sdk/src/lib.rs:42-44`) is unchanged.

Before, `examples/twilio-sdk/src/models.rs:133-151`:

```rust
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub sid: String,
    pub account_sid: String,
    pub status: MessageStatus,
    pub body: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
}
```

After:

```rust
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct Message {
    pub sid: String,
    #[sensitive]
    pub account_sid: String,
    pub status: MessageStatus,
    #[sensitive]
    pub body: Option<String>,
    #[sensitive]
    pub from: Option<String>,
    #[sensitive]
    pub to: Option<String>,
    pub error_code: Option<i64>,
    #[sensitive]
    pub error_message: Option<String>,
}
```

Call site (caller writes), before → after:

```rust
let response = api.get_message("SM_fixture_message").await?;
let shown = format!("{:?}", response.value);
// → exactly the same calls; sid/status/error_code remain visible.
```

Mechanism: generate an ordinary `std::fmt::Debug` impl. A sensitive field contributes a constant withheld marker, without reading or formatting its value; public fields use Debug. Do not derive Display, Serialize, validation, or logging. Support structs, tuple fields and enums; generate bounds only for fields actually formatted. A marked native `reqwest::Request`, response, stream or non-Debug generic field requires no Debug bound. Do not attempt to mutate a separately defined model because an `api!` invocation uses it as a body: procedural macros cannot safely apply global, use-site-dependent secrecy to arbitrary types.

SDK code that disappears:

* `examples/twilio-sdk/src/models.rs:157-165`: 9 lines, replaced by the derive and five field markers above; net about 4 lines. Applying the same derive with all CreateMessage fields marked removes `:64-68`, another 5 lines, although that record's total line count need not shrink.
* `examples/distribution-registry-sdk/src/model.rs:36-47`: 12 lines, with token/access_token marked and expires_in public; about 9 net lines saved.
* `examples/elasticsearch-sdk/src/error.rs:422-437`: 16 lines including the private Withheld helper, after marking reason, root_cause and caused_by; about 12 net lines saved. Keep the provider's kind field visible.
* These are actual formatter deletions, not deletion of response models or validation. Salesforce's custom count summary (`src/model.rs:190-199`) is intentionally **not** counted: this derive does not invent a `Vec::len` projection.

Hardest counterexample: **Twilio Message** is the primary example and retains the required public fields. Europe PMC Source/PublicationId and ClickHouse Identifier can keep ordinary public Debug. Existing credential-user markings remain sensitive. For native values, withholding is unconditional, irrespective of the native formatter or HeaderValue flag. A secret copied into a field declared public is a classification error, not something this derive can infer from bytes.

Simple control (Nager): unchanged, including its existing standard Debug models. PokeAPI and Frankfurter unchanged too.

Charter check:

* C1: sensitivity is a fact on the provider field, not a generated-surface switch.
* C2: the model declares visibility; generated Rust implements formatting; request/response behavior stays in Rust.
* C3: adoption is local to models needing it; controls gain no words.
* C4: marked fields are withheld, unmarked fields use Debug; no name heuristic or body-use inference.
* C5: duplicate/malformed markers and unsupported item shapes point at the authored field; a missing public-field Debug bound points there too.
* C6: “Marked fields never format their contents” is the complete rule.
* C7: all model fields, native access and request exits remain available.
* C8: no runtime registry, traversal of secret payloads or logging actor.
* C9: require normal/alternate Debug canaries, nested collections, enums, non-Debug native payloads, and a public SID/status positive assertion; removing a marker must make a test fail.

New concepts: `#[derive(concord::Debug)]`; existing `#[sensitive]` extended to Rust fields.

Breaking changes: opt-in Debug text changes, particularly omitted fields becoming constant markers; snapshots may change. No layout, wire, constructor, endpoint or result-type change. Existing manual Debug impl must be removed before adding the derive.

Why it might be wrong: A newly added, unmarked private field would print; this is explicit classification, not automatic taint tracking.
It removes formatter maintenance, but not the author's responsibility to classify new provider fields and exercise them in canary fixtures.

Cross-family effect: removes formatting glue from A error documents and B response records as well as D, and supplies safe ordinary model Debug for J. It does not justify logging arbitrary success values automatically.

## D2. Secret storage as a value type

Fact it expresses: an S3 signing secret/session token remains private wherever that value is stored; the access-key ID remains intentionally visible in this SDK.

Declaration (author writes): `examples/amazon-s3-sdk/src/sigv4.rs:39-44`, before → after. `S3Api(signer: Signer) { ... send signer; ... }` (`src/api.rs:33-38`) remains unchanged.

```rust
// Before
#[derive(Clone)]
pub struct Credentials {
    access_key_id: Arc<str>,
    secret_access_key: Arc<str>,
    session_token: Option<Arc<str>>,
}

// After
#[derive(Clone, Debug)]
pub struct Credentials {
    access_key_id: Arc<str>,
    secret_access_key: concord::Secret<Arc<str>>,
    session_token: Option<concord::Secret<Arc<str>>>,
}
```

Call site (caller writes): before → after is unchanged:

```rust
let credentials = Credentials::new("fixture-access-id", "fixture-signing-secret")?
    .with_session_token("fixture-session-token")?;
let shown = format!("{credentials:?}");
```

The SDK's signer deliberately changes its raw borrow (`src/sigv4.rs:108-115`):

```rust
// Before, arguments to aws_credential_types::Credentials::new:
&*self.secret_access_key,
self.session_token.as_deref().map(str::to_owned),

// After:
self.secret_access_key.expose().as_ref(),
self.session_token.as_ref().map(|s| s.expose().as_ref().to_owned()),
```

Mechanism: a transparent owning wrapper whose Debug is constant without `T: Debug`. It has `new`, `expose`, `into_inner` and conditional Clone/Eq; no Deref, raw Display or implicit string conversion. No allocation, erasure, global state or zeroization claim. Construction wraps already validated values in `Secret::new`; validation remains SDK Rust. A core `ToHeaderValue` adapter for `Secret<HeaderValue>` preserves the native bytes and sets sensitivity. For other native values, the wrapper provides formatting safety only: expose the request explicitly to execute/edit it. Do not blanket-serialize Secret into every codec; an SDK may deliberately expose it at its existing encoding boundary.

SDK code that disappears:

* `examples/amazon-s3-sdk/src/sigv4.rs:119-130`: 12-line Credentials Debug impl. About 7–9 net lines after changing constructor wraps and signer borrows; retain ASCII validation at `:60-73` and `:86-95`.
* `examples/twilio-sdk/src/auth.rs:36-40` and `examples/elasticsearch-sdk/src/model.rs:116-120`: 5 lines each, if each private HeaderValue field becomes Secret<HeaderValue> and its parent uses ordinary derived Debug. `into_header` explicitly unwraps it; the existing concord-auth validation remains.
* `examples/shopify-sdk/src/scalars.rs:184-188` and `:208-212`: 5 lines each for AccessToken and Cursor if their private storage adopts the wrapper. Keep Cursor's transparent serde representation through explicit transparent codec implementations or an explicit serde adapter; do not count that adapter as free.

Hardest counterexample: **native reqwest values** need no Debug bound and cannot accidentally invoke native Debug through the wrapper. Twilio should wrap only private fields, never the whole Message; D1 is less disruptive for its public String/Option fields. Europe PMC IDs and public ClickHouse identifiers need no wrapper. A public username is not converted into a secret by spelling; this checkout's auth username remains classified as the SDK says.

Simple control (Nager): unchanged; no Secret values. PokeAPI and Frankfurter unchanged.

Charter check:

* C1: the field's type says its content is private in every owner.
* C2: storage expresses the fact; acquisition, signing, rotation and serialization decisions remain ordinary Rust.
* C3: no change to declarations or dependencies for controls.
* C4: only explicitly wrapped values are affected; no whole-type inference from an endpoint use.
* C5: an accidental Display/Deref assumption fails at the SDK's actual use; expose is the explicit repair.
* C6: “Secret never formats its contents; expose deliberately” is sufficient.
* C7: expose/into_inner preserve native access, including the caller's reqwest client and requests.
* C8: same representation as T, no background work or clock; wrapping is independent of logging features.
* C9: nested Option/Arc/Result/collection canaries; poison a wrapped native Debug to prove it is never called; wire/signature controls prove secrets still reach signing/auth correctly.

New concepts: `Secret<T>` and explicit exposure; a closed native-header adapter.

Breaking changes: private-storage migrations preserve public constructors; public field migrations change their types. Debug snapshots change. There is intentionally no implicit Display/Deref/serde behavior; affected SDK-internal borrows and explicit encoders must migrate.

Why it might be wrong: Existing SDK secret newtypes already provide validation and explicit exposure, so nesting Secret may add ceremony for only a five-line formatter saving.
Ownership does not track copied plaintext after expose; it cannot guarantee arbitrary caller/native logging is safe.

Cross-family effect: one storage fact protects credentials in C construction records, A errors retaining tokens, B result records and J diagnostics. It complements D1 but overlapping formatter deletions must not be counted twice.

## D3. Closed diagnostic vocabulary for payload-bearing errors

Fact it expresses: Xero failures have named stages; credentials, filesystem paths and provider documents are payload data, not the diagnostic label of that stage.

Declaration (author writes): a **Rust error formatter derive**, not an `api!` error/retry language. Before, `examples/xero-accounting-sdk/src/error.rs:86-118`:

```rust
#[derive(Debug)]
pub enum ErrorKind {
    Input(&'static str),
    TenantMismatch,
    RecoveryRequired,
    StoreBusy,
    Storage,
    Authentication(Arc<Error>),
    Concord(Box<concord::Error>),
    Provider(Box<ProviderFailure>),
    Contract(&'static str),
    InvoiceValidation,
    Admission(concord_ratelimit::Error),
    Deadline,
}
pub struct Error {
    kind: ErrorKind,
    uncertain: bool,
}
```

After:

```rust
#[derive(concord::Diagnostic)]
pub enum ErrorKind {
    #[diagnostic("invalid invoice input")]
    Input(&'static str),
    #[diagnostic("tenant binding mismatch")]
    TenantMismatch,
    #[diagnostic("token journal requires recovery or reauthorization")]
    RecoveryRequired,
    #[diagnostic("token journal is locked")]
    StoreBusy,
    #[diagnostic("token journal persistence failed")]
    Storage,
    #[diagnostic("credential acquisition failed")]
    Authentication(Arc<Error>),
    #[diagnostic("invoice request or response failed")]
    Concord(Box<concord::Error>),
    #[diagnostic("provider refused the request")]
    Provider(Box<ProviderFailure>),
    #[diagnostic("provider success contract failed")]
    Contract(&'static str),
    #[diagnostic("invoice validation failed")]
    InvoiceValidation,
    #[diagnostic("local rate admission refused")]
    Admission(concord_ratelimit::Error),
    #[diagnostic("read retry budget expired")]
    Deadline,
}
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    uncertain: bool,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.kind, f)
    }
}
```

The Accounting declaration stays `send policy -> Result<_,Error>;` (`src/lib.rs:42`). Diagnostic strings here are Rust formatting behavior; they are deliberately not provider statements in the HTTP DSL.

Call site (caller writes), before → after:

```rust
let summary = format!("{error} {error:?}"); // same call
let uncertain = error.write_uncertain(); // same typed fact
match error.kind() { /* same variants and payload access */ }
```

Mechanism: every variant must have one literal label; the derive implements Display with that label and Debug with the variant identity, and never visits payloads. No interpolation expressions, automatic source traversal, or assumed-safe `&'static str` payloads. It does not implement provider decoding, recovery or an error-source chain. The existing outer `std::error::Error` impl remains source-free. Payloads stay available through Rust matches/accessors. This differs from D1's per-field Debug and D2's storage changes: it provides a fixed diagnostic vocabulary without imposing formatting requirements on any payload.

SDK code that disappears: `examples/xero-accounting-sdk/src/error.rs:166-182` (17-line Display match) and `:183-190` (8-line Debug impl), replaced by the five-line delegate above and derived Debug. Twelve label attributes add about 12 lines: conservative total saving about 7 lines, with a larger maintenance gain because stage labels no longer require two formatter matches. Keep `Error::kind`, uncertainty/class/concord accessors (`:119-155`), From conversions (`:156-165`) and every provider model. This does **not** delete ErrorKind's provider/behavior cases or 158 inventory error-glue lines.

Hardest counterexample: **errors carrying native reqwest/io values** remain safe because neither Display nor Debug traverses payloads and no payload Debug bound is generated. This does not solve Twilio Message's selective data rendering; use D1 there. It also must not replace TwilioError's useful operation/status/code rendering with a blanket label without a separate decision; no Twilio formatter deletion is credited.

Simple control (Nager): unchanged; this derive is optional Rust support. PokeAPI and Frankfurter unchanged.

Charter check:

* C1: no formatting configuration is added to `api!`; HTTP/provider facts are untouched.
* C2: labels are ordinary Rust diagnostic behavior; the DSL neither decodes nor recovers through this derive.
* C3: controls retain all existing declarations and error types.
* C4: exactly the authored literal prints; payloads never print, including static strings.
* C5: missing/duplicate labels, interpolation and non-enum use fail at the offending variant/attribute.
* C6: “A variant prints its literal label, never its payload” is the whole rule.
* C7: matching variants and deliberate document/native access remain possible.
* C8: no source walk, allocation or actors in the generated formatters.
* C9: replace each payload with canary-bearing native/provider values; check Display, both Debug modes and the outer source chain; missing-label and interpolation compile-fail tests.

New concepts: `derive(concord::Diagnostic)` and literal-only `#[diagnostic("...")]` on Rust enum variants.

Breaking changes: opt-in error text/Debug shape. Xero Input/Contract lose their specific static payload label in ordinary formatting; callers can still inspect it via kind(). No variant, payload, uncertainty or send-result type changes.

Why it might be wrong: It adds a Rust formatter convenience near established error-derive territory, rather than an HTTP fact the API compiler uniquely knows.
The deliberately small vocabulary loses useful detail; keep it optional and prefer existing safe specialized formatters when that detail matters.

Cross-family effect: addresses A's formatter boilerplate, D's payload secrecy and J's safe error labels together, without building an error/retry framework.

## J1. One safe completion event per execution — smaller than you think

Fact it expresses: the declaration already identifies the operation, HTTP method and route; execution adds a response status or transport-failure kind.

Declaration (author writes): **no new DSL syntax**. Before → after is the same Twilio declaration (`examples/twilio-sdk/src/lib.rs:23-30,42-44`):

```rust
pub client TwilioApi(
    #[sensitive] authorization: HeaderValue,
    #[sensitive] account_sid: String,
) {
    base "https://api.twilio.com";
    path "2010-04-01/Accounts/{account_sid}";
    headers { AUTHORIZATION = authorization; }
    response Json;
    // Other existing endpoint omitted from this excerpt.
    GET get_message(sid: String) -> Message {
        path "Messages/{sid}.json";
    }
}
```

Only dependency opt-in changes, from the real `examples/twilio-sdk/Cargo.toml:12`:

```toml
# Before
concord = { path = "../../crates/concord", features = ["form", "json"] }
# After
concord = { path = "../../crates/concord", features = ["form", "json", "tracing"] }
```

Call site (caller writes), before → after:

```rust
let response = api.get_message("SM_fixture_message").await?;
// → same call. The application installs its tracing subscriber in ordinary Rust.
```

Mechanism: one optional structured event at `Decoder::execute` completion. Start with only `operation`, `declared_method`, `declared_route`, `status` or a fixed transport class, and `elapsed_to_head`. The route is the **authored template**, e.g. `2010-04-01/Accounts/{account_sid}/Messages/{sid}.json`, never a rendering of request.url(). Do not log runtime host labels, path/query inputs, request/response bodies, header values, provider documents, native errors, or arbitrary Debug. For decoders without generated metadata, emit an anonymous execution outcome and omit the identity/route. User-created Operation labels are not automatically trusted declaration metadata.

This deliberately starts below the proposed span-per-send design: no attempt number, nested span tree, response rendering, or automatic decoded-value logging. A successful `execute` event means **a head arrived**, including non-2xx; it does not mean the body decoded, the stream completed, or a write committed. No event generated by `prepare()` implies a network operation. Native code that executes reqwest directly is outside this event boundary; existing `Decoder::execute` paths such as S3 signing are covered. Reqwest's internal retries/redirects are not separately observable attempts here.

SDK code that disappears: **0 current SDK logging lines**. Search of the ten witness `src/` trees found no operational tracing/log implementation to delete; documentation println examples do not qualify. `examples/twilio-sdk/Cargo.toml:12` gains one feature, and `examples/amazon-s3-sdk/src/client.rs:351-358` needs no replacement wrapper because it already calls Decoder::execute. This design fills J's missing capability, rather than claiming that logging removes Twilio's provider-error adapter or S3's signer. In core, the insertion is `crates/concord-core/src/prepared.rs:600-613`; no SDK endpoint instrumentation is generated.

Hardest counterexample: **zero cost when logging is off**. With `tracing` absent, cfg removes the dependency, clock reads, event construction and any additional fields/branches/future state. With the feature present but no enabled subscriber, check enabled before reading the clock or constructing fields. Sensitive host labels and path holes cannot leak because no runtime URL enters the event at all. ClickHouse's currently exposed dynamic query pairs also never enter it. Twilio Message Debug still shows SID/status under D1; the execution logger does not need to print the Message to be useful.

Simple control (Nager): its declaration and calls remain unchanged. Enabling tracing in an application can observe it without adding any provider words; PokeAPI and Frankfurter likewise.

Charter check:

* C1: observability is a Cargo feature, not an HTTP declaration statement.
* C2: provider identity comes from the existing declaration; logging/subscriber choices are Rust behavior.
* C3: every control keeps its exact declaration.
* C4: fixed event schema, template identity and explicitly named time-to-head; no inferred attempt or success claim.
* C5: no new DSL errors; normal Cargo/type diagnostics cover opt-in.
* C6: “One safe event when the execution returns a head or transport failure.”
* C7: caller's client, prepare, raw send and native exits remain available.
* C8: one observation around the existing execution; no retries, middleware registry or spawned tasks.
* C9: capture events for the seven existing canary scenarios; inject host/path/query/header/body/source canaries, check no occurrence and unchanged wire; inspect feature-off optimized code/layout/dependency graph for added work.

New concepts: optional `tracing` feature and a fixed execution event; optional read-only static route metadata on Operation.

Breaking changes: additive feature only. A feature-on application receives new events; event schema needs a stability policy. No SDK signatures, request bytes or execution counts change.

Why it might be wrong: A template/status event may be too sparse to diagnose the exact request, especially native target rewrites.
Adding more fields casually is the main danger: timing/attempt/full-URL ambitions would turn the minimum safe mechanism into a different design.

Cross-family effect: uses E's operation identity and gives M3 a small, exhaustively testable log surface. It needs neither D's model formatting nor A/B result decoding to be safe.

## J2. A public safe diagnostic view, including a conservative native view

Fact it expresses: declared placements have known sensitivity; dynamically supplied pairs and natively rewritten URLs may lack a trustworthy placement map.

Declaration (author writes): extend the existing sensitivity fact to an entire QueryPairs contribution. Before, `examples/clickhouse-sdk/src/wire.rs:40-49`:

```rust
GET select(
    query("query") sql: String,
    format: OutputFormat,
    query("query_id") query_id?: QueryId,
    query("wait_end_of_query") wait_end_of_query?: u8,
    query("send_progress_in_http_headers") progress?: u8,
    query(..) params?: Vec<(String, String)>,
) -> stream {
    headers { "x-clickhouse-format" = "{format}"; }
}
```

After:

```rust
GET select(
    #[sensitive] query("query") sql: String,
    format: OutputFormat,
    query("query_id") query_id?: QueryId,
    query("wait_end_of_query") wait_end_of_query?: u8,
    query("send_progress_in_http_headers") progress?: u8,
    #[sensitive] query(..) params?: Vec<(String, String)>,
) -> stream {
    headers { "x-clickhouse-format" = "{format}"; }
}
```

The params marker preserves the private-bound-value fact already expressed by Param Debug (`src/query.rs:255-260`). The SQL marker is an additional conservative classification: current Query Debug **does print SQL source and bound names** (`src/query.rs:461-469`), so it is not evidence that SQL is already promised private. An SDK that deliberately treats SQL source as public can omit that marker; it still needs the params change for private bindings. Apply the same pair fact to command's params (`src/wire.rs:65`), and explicitly decide whether insert SQL can contain private literals. A dynamic public query contribution, e.g. Elasticsearch's concurrency number pair, need not gain a marker. No guessing from `param_` key names.

Call site (caller writes), before → after for an existing Prepared value:

```rust
// Before: only a textual Debug representation is available.
let shown = format!("{prepared:?}");

// After: an immutable, structured, safely formattable view.
let diagnostic = prepared.diagnostics();
tracing::debug!(request = %diagnostic);
```

For the real S3 native holder, `examples/amazon-s3-sdk/src/client.rs:362-378` changes from its handwritten field-by-field Debug to:

```rust
impl<T> std::fmt::Debug for SignedRequest<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(
            &concord::diagnostics::native_request(
                "SignedRequest", &self.request, self.decoder.operation(),
            ),
            f,
        )
    }
}
```

Mechanism: publish typed safe facets of the existing composition metadata, rather than exposing raw fields and expecting every logger to reimplement masking. `RequestDiagnostic` offers safe method/operation/URL/header summaries, with Display/Debug and an optional structured serializer whose fields are equally safe. Prepared Debug delegates to this view so the guarantee has one implementation. Response diagnostics offer status/operation only, or already-safe declared facts; never raw response URL, Location, request-id/header values or decoded models. Error diagnostics expose kind/delivery/status, never a native source's text.

For `#[sensitive] query(..)`, withhold the **entire query string** for that endpoint, including dynamic keys; do not allocate a runtime taint map or risk missing duplicate-key aliases. An endpoint-level metadata bit suffices, even if the optional pair contribution is absent. Ordinary known sensitive named keys/segments/labels retain existing sticky composition. Unmarked dynamic pairs remain an explicit public contribution; the SDK facts must be correct.

`native_request` is intentionally more conservative: withhold the entire actual URL and every header value, and omit arbitrary native header names; show only a recognized HTTP verb and generated operation identity. A custom native method is `CUSTOM`, not arbitrary bytes. Generated operation metadata does not certify that an edited URL still has the original shape. A followed Prepared URL remains wholly withheld. The native adapter requires no reconstruction/reimport of a raw request and imposes no new restrictions on into_parts. It cannot emit raw request/response/source references through the structured view.

SDK code that disappears:

* `examples/amazon-s3-sdk/src/client.rs:362-378`: 17-line native-request Debug impl, replaced by about 11 lines above (about 6 net; a shorter one-expression formatting style saves more). The URL/source masking policy is no longer SDK glue. Signing, byte rewriting, range and decoding remain.
* `examples/clickhouse-sdk/src/wire.rs:83-91`: rewrite the existing test that asserts a bound value **is** printable into a negative canary assertion; this is a corrected expectation, not a claimed test deletion. Two field markers are new provider facts, not a net DSL-line reduction.
* No response-policy wrappers are counted as removed. The current S3 friction note's older SignedRequest line numbers were checked against the source above.

Hardest counterexample: **ClickHouse QueryPairs and native reqwest edits**. Whole-query/native-URL withholding makes key aliases, opaque links, changed hosts, signed URLs and percent-encoding irrelevant to masking. Public Twilio Message SID and Europe PMC publication IDs can still be shown by their ordinary declared diagnostics, while sensitive account segments are withheld. The current auth-user marker stays effective; nothing classifies all usernames globally.

Simple control (Nager): unchanged. Its existing public country/year path rendering remains available; PokeAPI and Frankfurter add no concepts.

Charter check:

* C1: only the existing private-data fact grows to cover dynamic query contributions.
* C2: typed views provide facts to Rust logging; no subscriber, dispatch loop or retry engine in the declaration.
* C3: controls' declarations stay identical.
* C4: known placements follow explicit markers; sensitive dynamic queries and unproven native URLs are withheld whole.
* C5: accept the new marker at the actual query(..) source span; malformed markers and invalid placements still diagnose authored code.
* C6: “Log the diagnostic view; native URLs and sensitive dynamic queries are opaque.”
* C7: raw request/response and native execution APIs remain deliberately reachable.
* C8: the view borrows; creating it performs no I/O, body polling or eager formatting; static pair masking adds no logging actor.
* C9: normal/alternate/structured output canaries for duplicate pairs, secret dynamic keys, Unicode, path/host holes, native rewrites, followed links, reflected response headers and errors; wire bytes must be identical before/after.

New concepts: `RequestDiagnostic`, `Prepared::diagnostics`, conservative native/response/error adapters; existing `#[sensitive]` allowed on query(..).

Breaking changes: stronger Prepared Debug for opted-in dynamic pairs; ClickHouse's positive-printing regression must change. Native summary text loses raw header-name detail. Proposed methods may require generated-name collision checks if later added directly to endpoint handles; keeping them on core Prepared avoids that expansion.

Why it might be wrong: Whole-query withholding sacrifices useful nonsecret fields, and public values marked incorrectly remain a classification problem.
A structured serializer is a second output path unless it consumes exactly the same safe facets; it must never serialize the hidden native backing object.

Cross-family effect: one composition-derived view serves D's native-holder diagnostics, J logging and M3 leak checks; it also fixes a concrete gap in I's dynamic-value support. It removes hand masking in A/B metadata presentations only where the existing diagnostic shape actually matches.

## J3. Explicit typed observation at the Rust execution boundary

Fact it expresses: an operation has a declared identity; a particular execution has an outcome, and an SDK retry loop may know its own attempt number.

Declaration (author writes): before → after remains `examples/xero-accounting-sdk/src/lib.rs:37-43,49`:

```rust
pub client Accounting(policy:Policy) {
    base "https://api.xero.com";
    path "api.xro/2.0";
    format concord::Json;
    limit BODY_LIMIT;
    send policy -> Result<_,Error>;
    scope tenant(#[sensitive] header("xero-tenant-id") tenant:String) {
        // Other existing endpoints omitted from this excerpt.
        GET get(#[sensitive] id:String) -> InvoicesDocument {path "Invoices/{id}";}
    }
}
```

Call site (caller/SDK behavior writes): the exact native execution in Xero's retry closure (`examples/xero-accounting-sdk/src/policy.rs:73-76`) before → after:

```rust
// Before
request.headers_mut().insert(AUTHORIZATION, used.header().clone());
let received = decoder.execute(http, request).await?;

// After; observer is supplied explicitly by ordinary Rust policy configuration.
request.headers_mut().insert(AUTHORIZATION, used.header().clone());
let received = decoder
    .execute_observed(http, request, observer, Some(attempt))
    .await?;
```

An application using only Prepared can choose observation for one call without an SDK policy:

```rust
// Before
let received = api.prepare_get_message("SM_fixture_message")?.receive().await?;
// After
let received = api.prepare_get_message("SM_fixture_message")?
    .receive_observed(&my_observer).await?;
```

The latter uses real Twilio public preparation (`src/lib.rs:139-149`). An SDK that wants this on every generated send can add a Rust `with_observer` method and observer field to its existing policy, behind its own optional feature; the field is not a new API/provider input. Ordinary helpers keep calling the existing unobserved methods by default. This proposal does not pretend a generic wrapper around SendWith can see the attempts inside an arbitrary policy.

Mechanism: an `Observer` with a synchronous `on_event(&self, ExecutionEvent)` receives only safe typed facts: generated identity, numeric attempt if supplied, status or fixed failure kind, elapsed-to-head, optionally cancellation. No endpoint, raw Request, response, Error source, formatter callback or decoded T is passed to it. Events borrow only generated metadata or contain scalar outcomes. The Rust observer decides whether to trace, count metrics, or retain an event. It cannot edit the in-flight request through the event. No observer registry, async observer tasks, automatic replay or hidden per-client counters.

Keep the existing execute/receive implementations uninstrumented; the observed overloads own all additional event state. Unobserved defaults call the original methods directly, rather than an async observing wrapper with a nominal Noop: a generic constant branch alone is not proof that an async future has unchanged layout. Thus default futures have no observer fields, clocks or cancellation Drop guard. Explicit observers opt into their own cost; a stored trait object is paid only by an SDK/app that explicitly chooses it. Default SDK builds must not gain an Option/Arc observer field merely because this API exists. Cancellation is a distinct event, never “not sent”; an observed call may use a Drop guard for it. A head event is still not stream completion. A future body-observation API is a separate design.

SDK code that disappears: **0 existing logging lines**, as with J1. `examples/xero-accounting-sdk/src/policy.rs:76` is replaced by an observed execution call, not deleted retry glue. Preserve auth invalidation (`:77-79`), cooldown (`:80-84`), lease release (`:86-101`), Retry decisions (`:105-117`) and provider finish (`:137-151`). Claiming these as savings would move behavior into Concord and violate C2. This option buys explicit typed access and isolation rather than immediate line savings.

Hardest counterexample: **Xero auth refresh/retry plus S3 native signing**. The Rust loop passes its own `attempt`; Concord never guesses. Native signing can use the same observed Decoder call after signing, without exposing native URL values. Any internal reqwest redirect/retry remains below this boundary, so the event describes one executor invocation, not a count of physical HTTP exchanges. The unchanged unobserved overload is the zero-cost off path; canaries in host labels/path/query/body/source cannot enter the fixed event type.

Simple control (Nager): unchanged, including its current send policy; no observer field or generic parameter is added to it. PokeAPI and Frankfurter unchanged.

Charter check:

* C1: no logging policy words in `api!`; attempt is a Rust execution fact.
* C2: typed facts go out to Rust callbacks; auth/retry/limiter decisions stay exactly where they are.
* C3: no observer obligation in generated client construction or controls.
* C4: attempt is absent unless the caller supplies it; outcomes mean head/transport/cancel, not business success.
* C5: wrong observer signatures are ordinary Rust errors at the call; no generated trait-name diagnostics.
* C6: “An explicit observer sees safe facts about this one execution.”
* C7: observed and unobserved execution preserve native request/decoder exits.
* C8: explicit synchronous observation, no middleware discovery, counters or background tasks; observer panics follow ordinary Rust semantics.
* C9: event recording under concurrent clients proves isolation; cancellation and retry counts are exact; poison native Debug/Display proves they are never called; compare the default unobserved code/future layout and dependencies with the pre-feature baseline.

New concepts: `Observer`, `ExecutionEvent`, explicit receive_observed/execute_observed methods; optional attempt number supplied in Rust.

Breaking changes: additive execution methods. Opt-in SDK configuration is additional Rust API; the existing client signature need not change. Any public event enum should be non_exhaustive or versioned before publication.

Why it might be wrong: A callback API is more surface than a tracing feature, and downstream observers can add latency or panic.
Adding events for every body/codec phase would require persistent state across several ownership boundaries; the small head-only version intentionally avoids that framework.

Cross-family effect: M3 can use the observer as a deterministic log capture sink; E operation identity and D3 fixed diagnostic kinds remain reusable. J1 could be an adapter over this event schema, but the designs have different opt-in models and should not require both.

## M1. Publish the existing fixture support and bind before building scripts — smaller than you think

Fact it expresses: a registry returns same-origin absolute Location/Link URLs, so a fixture must know its bound origin before constructing those replies.

Declaration (author writes): before → after is unchanged, using `examples/distribution-registry-sdk/src/api.rs:34-41`:

```rust
pub(crate) client Registry(
    #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
) {
    base "https://registry.invalid";
    GET ping() { path "v2/"; }
    // Existing catalog endpoint omitted from this excerpt.
}
```

Test dependencies change from private workspace support packages to a public `concord-test` dev-dependency. Initially move/re-export the actual server and canary implementation, with compatibility shims for the old package names. Do not build another server. `examples/testserver/Cargo.toml:1-8` and `checks/canary/Cargo.toml:1-8` currently set `publish = false`.

Call site (test caller writes): the real Registry helper currently requires `self_aware(build)` (`tests/protocol.rs:35-46`):

```rust
// Before: helper implementation, required for every self-referencing script.
async fn self_aware(build: impl Fn(&str) -> Vec<Reply> + Send + Sync + 'static) -> TestServer {
    let url: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let script: Arc<OnceLock<Script>> = Arc::new(OnceLock::new());
    let (u, s) = (Arc::clone(&url), Arc::clone(&script));
    let server = TestServer::start(move |_| {
        s.get_or_init(|| Script::new(build(u.get().unwrap())))
            .next()
    })
    .await;
    url.set(server.url()).unwrap();
    server
}

// After: the existing build closure is passed directly to shared support.
let server = concord_test::TestServer::scripted_with_origin(build).await;
```

Bind the listener first, synchronously evaluate the script factory with that listener's origin, and only then start accepting. The closure is test behavior in Rust. Keep the existing TestServer API for stateful provider fakes. A plain `scripted(replies)` constructor can return `(server, script)` so queue-exhaustion assertions remain possible.

One optional reusable Rust builder also captures the repeated test transport choice:

```rust
// Before: twilio-sdk/tests/support/mod.rs:32-38
reqwest::Client::builder()
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .retry(reqwest::retry::never())
    .timeout(Duration::from_secs(2))
    .build()
    .unwrap()

// After: explicitly chosen fixture transport, not a production default.
concord_test::http_client_builder()
    .timeout(Duration::from_secs(2))
    .build()
    .unwrap()
```

SDK code that disappears:

* `examples/distribution-registry-sdk/tests/protocol.rs:35-46`: 12-line self_aware helper; replace call sites directly and drop unused OnceLock imports where applicable.
* `examples/twilio-sdk/tests/support/mod.rs:56-58`: 3-line Script/clone/start plumbing becomes one shared `scripted` call; retain construction of the Twilio client and return tuple at `:59-60`.
* `examples/elasticsearch-sdk/tests/common/mod.rs:28-29`: 2-line Script/server setup becomes one shared call; keep its ApiKey/client construction at `:30`.
* Twilio's transport expression `tests/support/mod.rs:32-38` loses about 3 net lines; Elasticsearch's `tests/common/mod.rs:18-23` loses about 3 net lines. Host/context constructors stay in SDK tests.
* Do not claim the historical Salesforce F14 “~70-line Router” as a current deletion: that named helper was not present in this SDK's current tests. Likewise S3 F8's HEAD limitation is already fixed by `examples/testserver/src/lib.rs:161-163` (`Body::HeadOnly`); preserve it instead of proposing it anew.

Hardest counterexample: **S3 virtual bucket hosts and real HEAD semantics**. Keep ClientBuilder::resolve support and explicit port-bearing origins; `amazon-s3-sdk/tests/common/mod.rs:67-81` still owns region/credentials/clock and enumerates its bucket hostnames. Keep Body::HeadOnly, streamed/failing bodies, gates, refused ports, cleanup counters and paused-clock APIs. Do not replace the S3 independent signature verifier or Registry's stateful upload/token fake with a reply queue.

Simple control (Nager): its declaration stays unchanged; adopting the dev-dependency is optional and can shorten test setup. No required fixture statement. PokeAPI and Frankfurter likewise.

Charter check:

* C1: test-server setup is Rust, never an HTTP/provider declaration keyword.
* C2: scripts/routing/state are test behavior; literal expected URLs/replies provide independent evidence of facts.
* C3: controls' production declarations and dependency surface do not grow.
* C4: scripted queues are FIFO; origin-dependent scripts are built once after binding and before accepting.
* C5: an unexpected request or exhausted script reports the test case/route, not generated internals; borrowed factories need no artificial static lifetime when evaluated before spawning.
* C6: “Build replies with the server's own bound origin” explains the only new server method.
* C7: expose the real socket address and ordinary reqwest builder; no replacement HTTP client abstraction.
* C8: owned loopback tasks and explicit shutdown; no global server, DNS overrides or sleep-based readiness.
* C9: reuse existing server lifecycle/HEAD/paused/failure tests; add self-referencing Link/Location and competing-server isolation regressions; assert no live tasks after shutdown.

New concepts: public dev-only `concord-test`; scripted_with_origin and plain scripted constructors; explicit fixture HTTP builder. Existing Recorded/Reply/Body/Script/Gate/Cleanup concepts remain the same.

Breaking changes: none with old workspace packages re-exporting the public support temporarily. SDK dev-dependency/import migrations are mechanical. No new production constructor or bypass of future construction invariants is implied.

Why it might be wrong: Publishing test support commits Concord to a real compatibility surface, including server scheduling and cleanup semantics.
The minimum removes modest setup glue; a universal provider router would be a much larger, less justified framework and is deliberately not part of it.

Cross-family effect: gives D/J canary tests a reusable transport foundation; host/origin fixtures also exercise C credential binding and F continuations without production DSL additions.

## M2. Independent request expectations usable on Prepared and Recorded

Fact it expresses: Twilio create is a POST to the account's Messages.json route with exact form bytes, one Basic header and specified media headers.

Declaration (author writes): before → after is unchanged, `examples/twilio-sdk/src/lib.rs:33-36`:

```rust
POST create_message(body message: CreateMessage) -> Message {
    path "Messages.json";
    request Form;
}
```

The author adds **Rust test expectations**, not a `tests {}` section to the declaration. Provider documentation/fixtures supply the expected bytes independently; production declaration facts are not duplicated into a second runtime request builder.

Call site (test caller writes), before from `examples/twilio-sdk/tests/protocol.rs:20-34`:

```rust
let request = server.single();
assert_eq!(request.method, "POST");
assert_eq!(request.target, CREATE_PATH);
assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
assert_eq!(
    request.header_all("content-type"),
    ["application/x-www-form-urlencoded"]
);
assert_eq!(request.header_all("accept"), ["application/json"]);
assert_eq!(
    request.header("content-length"),
    Some(FORM.len().to_string().as_str())
);
assert_eq!(request.body, FORM);
assert_eq!(request.body_len, FORM.len() as u64);
```

After:

```rust
let expected = concord_test::RequestExpectation::post(CREATE_PATH)
    .header_all("authorization", [AUTHORIZATION])
    .header_all("content-type", ["application/x-www-form-urlencoded"])
    .header_all("accept", ["application/json"])
    .header_all("content-length", [FORM.len().to_string()])
    .body_exact(FORM);
expected.assert_recorded(&server.single());
```

For a separate build-only contract, the caller supplies an expectation that does not require headers reqwest adds at execution:

```rust
let prepared = client.prepare_create_message(message())?;
concord_test::RequestExpectation::post(CREATE_PATH)
    .header_all("authorization", [AUTHORIZATION])
    .header_all("content-type", ["application/x-www-form-urlencoded"])
    .header_all("accept", ["application/json"])
    .body_exact(FORM)
    .assert_prepared(&prepared);
```

Mechanism: one strict assertion vocabulary adapts a borrowed Prepared request or a Recorded exchange. Compare raw path/query targets without sorting or decoding away differences; compare every occurrence of a requested header name in order, not global header-name order. Body equality also checks Recorded.body_len to detect captured-prefix truncation. Absence is explicit (`no_header`, `empty_body`); unspecified fields are not asserted. No serialization via the same API decoder to manufacture the expected body. No implicit snapshot blessing, no network in assert_prepared, and no draining a streaming body to make it comparable. For a body whose bytes are unavailable before sending, body_exact fails clearly and the author uses assert_recorded after explicit execution.

Failure messages identify method/target/header occurrence/body position and the test assertion site, without automatically dumping raw credentials, URLs or bodies. Diagnostic safety is tested for the assertion helper too. Authors can retain their deliberately synthetic fixture bytes in checked-in expectations.

A Rust case table can share common expectations and generate ordinary tests from explicitly supplied sample inputs. It must never synthesize required credentials/domain types or pretend every endpoint can be called automatically. Where an operation inventory already exists, as `GitHubOperation::ALL` (`github-sdk/src/api.rs:68`), optionally check case coverage against it with normal Rust. Do not require an operations declaration or new generated inventory from Nager, PokeAPI or Frankfurter merely to use assertions.

SDK code that disappears:

* `examples/twilio-sdk/tests/protocol.rs:20-34`: 15-line recorded-request assertion block becomes 7 lines above, about 8 net lines. Keep response semantics, status/operation checks and script exhaustion at `:35-50`.
* Other parallel blocks, e.g. `examples/twilio-sdk/tests/protocol.rs:58-63`, can adopt the same helper, but may save only one or two lines; no inflated estimate for them.
* `examples/salesforce-sdk/tests/wire.rs:74-82`: the 9-line common method/path/auth/host assertion helper can become a shared test expectation factory, but its Salesforce prefix and TOKEN expectations remain authored data. Estimate 3–4 net lines, not deletion of the provider facts.
* No server integration test is removed just because its prepare assertion passes. In particular S3 signing rewrites and reqwest-added framing/default headers still require their wire tests.

Hardest counterexample: **S3/native edits and streaming bodies**. assert_prepared describes that Prepared stage, not the final signed request; assert_recorded tests final bytes, with the independent SigV4 verifier retained (`amazon-s3-sdk/tests/common/mod.rs:83` onward). ClickHouse streamed insert and Mailgun one-shot sources must not be polled by a preparation assertion. Request target ordering, repeated headers, empty-versus-absent fields and content lengths stay visible rather than being normalized into equivalence.

Simple control (Nager): declaration unchanged. It can optionally use the same explicit country/year target assertion; no snapshots, metadata traits or coverage list are mandatory. PokeAPI and Frankfurter unchanged.

Charter check:

* C1: HTTP facts stay in api!; expectations are independent test evidence, not extra provider/compiler configuration.
* C2: checks are Rust behavior and never construct a production request from a second fact table.
* C3: no change to controls' declarations or required test shape.
* C4: comparisons are exact and field selection is explicit; no hidden normalization or automatic approval.
* C5: mismatches report the author's assertion and concrete field/occurrence, not macro internals.
* C6: “Assert these exact fields on this prepared or recorded request.”
* C7: accepts native request/recorded data without replacing reqwest or its body semantics.
* C8: assertions inspect; they neither send nor poll/consume lazy bodies.
* C9: deliberately mutate method/path/order/header repetition/body/framing and ensure each check fails; expected data must come from independent fixtures, not the api! compiler's own plan.

New concepts: RequestExpectation with Prepared/Recorded adapters; optional Rust case tables using existing operation inventories.

Breaking changes: additive dev tooling only. Stricter exact header repetition/byte-count checks can reveal tests that previously accepted extra values; this is intentional evidence improvement.

Why it might be wrong: Fluent expectations may be more surface than direct assert_eq, especially for one-field tests.
Generating expected bytes from the same declaration would make these tests tautological; the utility must keep expected data author-controlled and retain real wire checks.

Cross-family effect: tests C construction boundaries, I input encoding, K composition order and L syntax sugar with one assertion vocabulary; it complements M1 rather than replacing transport tests.

## M3. Public canary probes with positive wire and public-field controls

Fact it expresses: Twilio's account/auth/message content is withheld, while the message SID/status is intentional public output; a secret must still affect the actual request.

Declaration (author writes): before → after retains the real sensitive account and public message identifier from `examples/twilio-sdk/src/lib.rs:24-28,42-44`:

```rust
#[sensitive] authorization: HeaderValue,
#[sensitive] account_sid: String,
// Within the existing client:
path "2010-04-01/Accounts/{account_sid}";
GET get_message(sid: String) -> Message {
    path "Messages/{sid}.json";
}
```

This is a pair of unchanged excerpts, not a standalone api! invocation. D1's optional model-field declaration can be tested by the same probe, but is not required to use it.

Call site (test caller writes): before, existing shared support plus an SDK-local helper (`examples/twilio-sdk/tests/canary.rs:10,57-64`):

```rust
use concord_check_canary::Canaries;
fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}
```

After, public support packages the common boundary probes:

```rust
use concord_test::CanaryProbe;
let mut probe = CanaryProbe::new("twilio");
let account = probe.secret("path account");
let password = probe.secret("Basic password");
// Existing fixture/client construction and provider scenarios use these Strings.
probe.concord_error(&error);
probe.assert_on_wire(&server.requests());
probe.assert_public("message SID", &format!("{message:?}"), "SM_fixture_message");
probe.assert_public("message status", &format!("{message:?}"), "Queued");
```

The above calls illustrate separate fixture outcomes, not a claim that one execution simultaneously produced both error and success. The real public decoded Message's normal/alternate Debug is checked for each private canary as today, and public assertions run on the successful fixture.

Mechanism: promote and extend `checks/canary/src/lib.rs`, which **already** issues distinct canaries, checks normal/alternate Debug, Display, error chains and positive wire presence, including decoding Basic auth. Do not invent another scanner. Add named secrets, a shared probe of Concord Error/Received/ResponseMeta boundaries, a cycle/depth guard for hostile source chains, and public-output positive assertions. Keep deliberate registrations of SDK-specific fields, variants and returned surfaces: Rust offers no automatic reflection over every public method.

Retain the seven existing provider-driven scenarios (success, 4xx, 5xx, malformed body, wrong media, truncated body, refused connection). A helper may enumerate them, but provider document/status/format/stream interpretations stay callbacks/data in SDK tests. Do not force ClickHouse's drained inserts to decode JSON or S3 HEAD failures to acquire nonexistent error bodies.

Add a recording observer for J3 or a tracing subscriber attached to the J1 test future, and test every enabled output path with the same probes. Extend wire receipts to include host (existing wire_text at `checks/canary/src/lib.rs:132-153` omits Recorded.host), raw target, decoded Basic, and explicit encoding-aware controls where used. Tests of sensitive host labels cannot be satisfied by a target-only scan. Use separate public sample values for Twilio SID/status, Europe PMC identifiers and intentionally public ClickHouse identifiers/usernames; never register these as secrets and then maintain a blanket “identifier exemption”.

S3's signing secret is not sent verbatim. It needs a named **derived-use proof** invoking the existing independent signature verifier; its absence from wire is not a secrecy test failure. Response-only canaries need an injection receipt proving the fake actually emitted them. A mere broad `never_sent` exemption is not a positive control. Test code deliberately calling expose/raw native Debug remains explicitly outside the ordinary safe-surface guarantee; it is not silently scanned and excused.

SDK code that disappears:

* `examples/twilio-sdk/tests/canary.rs:57-64`: 8-line Concord-error helper replaced by one probe method at callers. `:44-55`: 12-line common receive/head probe can move into the public helper with its existing decode semantics; keep provider Result/error matching (`:68-89`).
* `examples/salesforce-sdk/tests/canary.rs:108-132`: 25 lines of common Concord-error and Prepared/Received/decode probing can use that helper; keep Salesforce error-variant and streaming/CSV-specific coverage.
* `examples/clickhouse-sdk/tests/canary.rs:48-54`: 7-line Concord-error/head helper disappears; keep the SDK Error/Rows/late-exception checks.
* `examples/github-sdk/tests/support/mod.rs:113-121`: 9-line assert_hidden loop, and `examples/distribution-registry-sdk/tests/support/mod.rs:123-129`: 7-line Debug/Display loop, can use the public probe; net savings depend on their call-site migration. Existing `checks/canary` code is shared infrastructure, not 176 newly deleted SDK lines.
* No claim that hundreds of provider-specific canary scenarios can be deleted. Public visibility assertions and log capture add tests intentionally; the net whole-suite line count could rise while duplicated probing falls.

Hardest counterexample: **Twilio public fields, Europe PMC identifiers and ClickHouse's two username contexts**. Pair absence assertions for private fields with presence assertions for public fields; fail both leakage and indiscriminate hiding. Preserve this checkout's sensitive auth username rather than changing its promise to fit a generic rule. Poison native errors and reflected response headers to exercise D/J boundaries; ordinary raw reqwest formatting is a deliberate exit. S3 validates derived secret use rather than exempting it without proof.

Simple control (Nager): declaration unchanged and no credentials invented. It can use failure/head probes without issuing a secret. PokeAPI/Frankfurter public values remain public; none needs new DSL markers for this testing utility.

Charter check:

* C1: no test/canary declarations in api!; field sensitivity remains the existing provider fact.
* C2: scenario construction and surface registration are explicit Rust tests using typed facts.
* C3: controls gain no provider concepts or declaration lines.
* C4: named private values must be absent; independently named public values must be present on specified surfaces; no name-based exemptions.
* C5: failures identify the secret/public label, surface, scenario and encoding/proof, rather than a generated implementation detail.
* C6: “Prove it was used; prove it stayed hidden; prove public diagnostics survived.”
* C7: tests use public SDK construction and native escape routes deliberately; no private macro reach-through.
* C8: per-test probe and observer state, no global subscriber/server or production canary code.
* C9: the utility itself must fail under seeded leaks, missing wire/response/derived-use proofs, over-redaction, cyclic sources and captured-log leaks; feature-off checks also prove no observer/logging work remains.

New concepts: public CanaryProbe, named secret/public checks, shared boundary probes and explicit use/injection proofs; compatibility aliases for existing Canaries are possible.

Breaking changes: additive dev API with an old-name shim. Stronger proof requirements reveal previously unexercised surfaces; source-chain cycles now fail with a bounded diagnostic instead of hanging. Production signatures/wire behavior are unchanged.

Why it might be wrong: No test harness can discover unregistered SDK-owned fields or prove secrecy against every provider reflection; representative fixture coverage remains authored work.
A higher assertion count is not coverage: missing registrations, overbroad exemptions and tests that never send a canary must fail review/proofs rather than inflate a metric.

Cross-family effect: this is D/J's shared C9 enforcement, also checking C's credential/context bindings and A/B's error/result diagnostic surfaces. It reuses M1 transport and can inspect M2 expectation failures for accidental raw-value output.

## Choice and boundaries

Start with D1, J1 and M1: each is independently useful, requires no new HTTP-language word, and preserves simple declarations. M3 should gate D/J adoption, not trail it. J2 addresses the concrete QueryPairs/native-boundary deficit and can supply the common safe view later; it needs more composition tests than a tracing feature alone. D2 is valuable for secrets passed through many owning types, with limited benefit when an SDK already has a good secret newtype. D3 is deliberately a lower-priority Rust convenience, not a reason to grow the HTTP DSL. J3 is the explicit-observer alternative for policy authors who need per-attempt facts or per-client isolation. M2 is optional assertion reuse, not permission to replace independent wire evidence with compiler-generated expectations.

| Family | Different mechanism | Immediate evidenced deletion |
|---|---|---|
| D1 | Generate a selective standard Debug formatter from field facts | Twilio/Registry/Elasticsearch formatter bodies |
| D2 | Give private storage intrinsic formatting behavior | S3 credential and native-credential wrapper formatters |
| D3 | Print a closed error-stage vocabulary without traversing payloads | Xero outer Display/Debug match bodies |
| J1 | Push fixed automatic tracing events at executor completion | None: observability is currently missing |
| J2 | Pull a typed safe rendering from composition/native context | S3 handwritten native request summary |
| J3 | Push safe typed events to an explicitly supplied Rust observer | None: policy execution/retry behavior stays |
| M1 | Share the existing server and post-bind script construction | Registry/Twilio/Elasticsearch setup plumbing |
| M2 | Share exact comparisons over independently supplied expectations | Twilio/Salesforce assertion plumbing |
| M3 | Share canary boundary probes and positive controls | Twilio/Salesforce/ClickHouse/GitHub/Registry probe plumbing |

All C9 entries above are proposed acceptance gates, not newly executed passing tests. This lane changes only this output document. Gross deletion ranges overlap across alternatives; no aggregate “glue goes to zero” claim is made from these estimates.
