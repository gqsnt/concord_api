# Independent architecture and codec/HTTP challenge

## Summary

Prefer validated per-response contexts (B), erased once behind a reusable `Decoder<T>`. Keep codecs ordinary Rust types, apply Accept suggestions after authored composition, share name-only validation, and seal `NativeKey`. This is tested prototype evidence, not production acceptance.

This is the first independent report, written without another reviewer's proposal or synthesis. The scope is `ARCH_CHALLENGE.md`, with outputs under `proposal/`; the supplied coordinator annex is context, not authorization to implement its entire campaign in the snapshot. No repository source was edited, branch created, global setting changed, remote contacted, push or publication performed. All Cargo work was offline; socket fixtures bind `127.0.0.1`. No sub-agents or other providers were contacted.

## Identity and evidence boundary

* Verified snapshot HEAD: `78330b4f87065eb28649d68ccba875af0cce3a6f`. Its commit message says it snapshots `dfdeed8187793dff57ec30bc24d54d5f9a7e43e2`; the original commit object is unavailable here. The original-source SHA and implementation `81836ec` are supplied provenance, not independently reproduced commit identities.
* Requested and observed local harness settings: `gpt-6.1-sol`, `xhigh`, execute/default mode. I personally read the current session's `turn_context`, identified through `CODEX_THREAD_ID`. It records the requested model and effort, `approval_policy=never`, and workspace sandboxing. This verifies harness metadata, not an independently observable provider-side inference configuration. Selected parent-process flags exposed no additional model setting. See `evidence/environment.json`.
* Snapshot development pin: `1.97.1`; snapshot `rust-version`: `1.85`. Locally executed prototype compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1, `x86_64-unknown-linux-gnu`. Prototype packages explicitly require `1.99.1`. I did not change the snapshot pin or global default. The supplied claim that this is the latest official stable was not externally reverified under the no-nonloopback-network restriction.
* Locked and inspected sources: reqwest **0.13.5**, http **1.5.0**, mime **0.3.17**. These are the actual lock versions, not guessed from manifest ranges. Local registry source citations below are inspection evidence. `evidence/source-inspection.txt` records relevant paths, hashes and line ranges.
* Personally executed: standalone compilation; runtime/loopback tests; explicit `Send` checks; intended compile failures; isolated dependency consumers; warnings-denied Clippy and rustdoc; fmt; focused unchanged-source `header_plan` tests on **both 1.97.1 and 1.99.1**. This is a same-source focused control, not reproduction of the entire historical baseline or a compilation-performance benchmark.
* Inspected only: the snapshot macro compiler, error/runtime implementation, author guide, compatibility and Riot sources, previous decisions and registry code. The whole coordinator matrix, production crate extraction, packaging, redirect integration, fresh-user reviews and native Windows execution have **not** been completed by this challenge.

Reproduce from the snapshot root:

```sh
python3 proposal/verify.py
```

It sets `CARGO_TARGET_DIR` to the absolute equivalent of `proposal/target` and `CARGO_NET_OFFLINE=true`. `evidence/commands.json` records the exact commands, actual exit codes, expected outcomes and diagnostic assertions. Logs are separate per command. The prototype's Cargo.lock and each isolated probe's lock are retained. The target directory is disposable build output, excluded from the portable file inventory.

`FILES.txt` enumerates the portable files written, `SHA256SUMS` freezes their contents, and `FREEZE.json` identifies this first report and its verification ledger before any synthesis. Environment-block probes have no lock because dependency resolution could not complete offline.

## 1. Primary proposal B: validate and retain codec-defined state

The exact open contracts implemented in `prototype/core/src/lib.rs` are:

```rust
pub struct ResponseHead<'a> {
    pub status: http::StatusCode,
    pub version: http::Version,
    pub headers: &'a http::HeaderMap,
}

pub struct EncodedBody {
    pub body: reqwest::Body,
    pub content_type: Option<http::HeaderValue>,
}

pub trait Encode<T> {
    fn encode(value: T) -> Result<EncodedBody, BodyError>;
}

pub trait Decode<T> {
    type Context: Send + 'static;
    const ACCEPT: Option<&'static str> = None;
    fn check_content_type(head: ResponseHead<'_>) -> Result<(), BodyError>;
    fn context(head: ResponseHead<'_>) -> Result<Self::Context, BodyError>;
    fn decode_body(context: Self::Context, body: bytes::Bytes)
        -> Result<T, BodyError>;
    fn decode_body_unchecked(body: bytes::Bytes) -> Result<T, BodyError>;
}
```

`check_content_type` defaults to `Ok(())`. `decode_body_unchecked` defaults to a static “response metadata required” error; it does not manufacture a context. The latter is the explicitly named body-only route for an application decoding an error document, e.g. `<Json as Decode<Problem>>::decode_body_unchecked(bytes)`. It bypasses **all** head validation; its name and docs must say so. Version-dependent formats can decline it, as the downstream `Versioned` codec does. It is different from the adapter that skips only the media gate.

The content-type hook is separate from `context` for a concrete reason: an ordinary downstream generic `IgnoreContentType<F>` can skip exactly that hook and preserve version validation, charset checks and application metadata checks in `F::context`. A single indivisible `validate(head)` hook cannot support that adapter honestly without teaching core about an opt-out policy or making the adapter bypass unrelated checks. Codecs are trusted to honor the documented split; the type system cannot determine whether an extension put unrelated work in the wrong hook.

Exact primary storage, with private helper aliases expanded here:

```rust
type BodyFn<T> = Box<
    dyn FnOnce(bytes::Bytes) -> Result<T, BodyError> + Send + 'static
>;
type BeginFn<T> = for<'a> fn(ResponseHead<'a>)
    -> Result<BodyFn<T>, BodyError>;

pub struct Decoder<T> {
    begin: BeginFn<T>,
    accept: Option<&'static str>,
    drain: bool,
}
pub struct Prepared<T> {
    client: reqwest::Client,
    request: reqwest::Request,
    decoder: Decoder<T>,
}
```

`Decoder<T>` is manually `Copy + Clone` without `T: Copy/Clone`. Its public methods are:

```rust
impl<T> Decoder<T> {
    pub fn new<F: Decode<T>>() -> Self;
    pub fn suggested_accept(&self) -> Option<&'static str>;
    pub async fn decode(self, response: reqwest::Response) -> Result<T, Error>;
    pub async fn finish(self, result: Result<reqwest::Response, reqwest::Error>)
        -> Result<T, Error>;
}
impl Decoder<()> { pub fn drain() -> Self; }
impl<T> Prepared<T> {
    pub fn new(client: reqwest::Client, request: reqwest::Request,
               decoder: Decoder<T>) -> Self;
    pub fn request(&self) -> &reqwest::Request;
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<T>);
    pub async fn send(self) -> Result<T, Error>;
    pub async fn send_raw(self) -> Result<reqwest::Response, Error>;
}
```

The production extraction should keep the existing endpoint metadata/redacted `Prepared::Debug` alongside these mechanics. The reduced prototype deliberately omits that unrelated metadata field/formatter; the signatures above describe its actual storage, not a claim that production redaction was migrated.

The monomorphized `begin::<F,T>` does:

```rust
F::check_content_type(head)?;
let context = F::context(head)?;
Ok(Box::new(move |bytes| F::decode_body(context, bytes)))
```

The closure captures only `F::Context`, not a codec instance, response borrow or complete `HeaderMap`. Native ownership is exercised in the real implementation:

```rust
if !response.status().is_success() { /* return Status with response */ }
let body_decoder = match (self.begin)(ResponseHead::from(&response)) {
    Ok(decoder) => decoder,
    Err(error) => return Err(Error::metadata(error, response)),
};
let bytes = response.bytes().await /* safe transport conversion */ ?;
body_decoder(bytes) /* body-decode conversion */
```

There is no retained borrow when `bytes(self)` consumes the response. The separate `match` matters: a `map_err` closure capturing the response would move it even on the success path. `send`, native execution followed by `finish`, and direct `decode` all use this path. Status remains first. `send_raw` skips status/head/body processing. The no-output decoder has no format obligations or Accept suggestion and drains using repeated `chunk()` without collecting a complete body. Its drain has no independent time/size bound.

**Executed:** `Versioned` in the downstream consumer implements only `Decode<NonZeroU32>` (a foreign output type). It checks JSON media policy, parses `x-response-version` into an owned `u32`, and uses the retained version to decode `7` as `14`. It calls no hidden codec trait. An encoder-only `Upload: Encode<std::fs::File>` consumes a foreign, non-Clone file and streams it through public `reqwest::Body::wrap`; a local server receives the content. No decoder obligation is imposed on Upload. All exposed return types remain `Prepared<T>`/`T`.

**Send and allocation assessment:** Context must be `Send + 'static`; an `Rc` context fails with E0277, and a response-header borrow fails its lifetime requirement. The successful primary and alternative `finish` futures are passed through `assert_send<F: Future + Send>`. No future is boxed. B performs one function-pointer head call and one erased `FnOnce` body call. A nonzero context requires one closure box, plus whatever allocations the codec chooses; a zero-sized context creates a zero-sized closure, whose `Box::new` storage does not allocate according to the inspected standard-library implementation. Attached error sources and the outer public Error allocate on failure. Body buffering/reqwest and MIME parsing have their own costs.

Do not describe strict JSON as globally allocation-free: mime stores general parsed types in an owned String and may allocate parameter bookkeeping. The prototype Json has a unit context but parses Content-Type in the media gate and again for its separate charset policy. That is a consciously exposed implementation cost, not proof that retaining metadata is free. The version header is parsed once in B. A future optimization could reuse parsed media through a deliberate richer context; it must preserve the adapter's narrowly scoped bypass and be measured before adding complexity.

**Hardest semantic problem:** Keep media-policy opt-out separate from all other validation while erasing each codec's owned response state behind `Decoder<T>`. **Worst case:** enormous numbers of tiny responses with nonzero contexts, where one allocation and erased call per response may matter more than metadata work. A codec retaining a long charset/profile string can also allocate for that string. **Excludes:** async metadata callbacks, codec registries/instances, borrowed contexts, trailers, URL/remote-address access through ResponseHead, streaming output decoders and arbitrary non-Send retained state. Streaming *input* remains supported. None is required by this increment.

## 2. Meaningfully different alternative A: retain the native head

`prototype/core/src/borrowed.rs` compiles and runs a second interface, rather than spelling B differently:

```rust
pub trait Decode<T> {
    const ACCEPT: Option<&'static str> = None;
    fn check_content_type(head: ResponseHead<'_>) -> Result<(), BodyError>;
    fn check_head(head: ResponseHead<'_>) -> Result<(), BodyError>;
    fn decode_body(head: ResponseHead<'_>, body: bytes::Bytes)
        -> Result<T, BodyError>;
    fn decode_body_unchecked(body: bytes::Bytes) -> Result<T, BodyError>;
}
type Check = for<'a> fn(ResponseHead<'a>) -> Result<(), BodyError>;
type DecodeFn<T> = for<'a> fn(ResponseHead<'a>, bytes::Bytes)
    -> Result<T, BodyError>;
pub struct Decoder<T> {
    check_media: Check,
    check_head: Check,
    body: DecodeFn<T>,
    accept: Option<&'static str>,
    drain: bool,
}
```

The same default media/body-only methods apply. The A module implements the same public Decoder/Prepared method signatures shown for B, including `decode`, `finish`, `Decoder<()>::drain`, native parts, `send` and `send_raw`, with its own `borrowed::Decoder<T>` and `borrowed::Prepared<T>` types. A's `Prepared` has the same three private fields, with its own decoder substituted. Encode, errors, name-validator engine, crate graph and macro hygiene are shared. `borrowed::apply_accept` has the same signature as the B hook with A's decoder; both delegate to one suggestion insertion helper. The macro alternative emits `K::borrowed::prepare::<E,B,T>(..., K::borrowed::Decoder::<T>::new::<F>())` rather than the root B bridge. Both bridges share native body/request construction. Its compiled compatibility adapter delegates `check_head` and body decode while omitting only `check_content_type`; a native-parts/adapter and drain smoke test also runs. The macro probe itself selects B; it does not offer a new author-facing mode switch.

After both borrowed checks succeed, A copies status/version and **moves**, rather than clones, the map:

```rust
let status = response.status();
let version = response.version();
let headers = std::mem::take(response.headers_mut());
let bytes = response.bytes().await?;
decode_body(ResponseHead { status, version, headers: &headers }, bytes)
```

This uses the real reqwest API. Locked `Response::do_bytes` collects its already wrapped `self.res.into_body()`; it does not re-read Content-Type from the now-empty response map. Automatic transport decompression is arranged before this stage. Metadata failures occur **before** taking headers, so the preserved response is untouched. Successful buffering has no remaining native response to return anyway. I inspected reqwest's native `From<Response> for http::Response<Body>` conversion too; moving the map with `headers_mut` is the smaller operation and avoids reconstructing a response or dealing with its URL extension.

A introduces no new head-storage allocation and no body trait object. It keeps the existing HeaderMap allocation and every header value alive across buffering, including unrelated sensitive headers. The decoder makes ordinary function-pointer calls. The output callback can inspect headers after reading but must rederive validated version/charset state; the type system offers no consumed “validated version” token.

**Executed distinction:** the same downstream Versioned codec yields `14` through both interfaces. Instrumented version parsing is B=1, A=2. The exact printed raw result is in `evidence/tests.log`: decoder-handle sizes are B=32 bytes, A=48 bytes on the recorded x86_64 compiler. These are size/operation observations, not timing or allocation benchmarks. A compiler failure for the naive “borrow headers, consume response, then use borrow” implementation is retained under `counterexamples/borrow-across-bytes`.

**Hardest semantic problem:** safely preserve head context across a consuming reqwest body read without pretending a borrowed header can outlive the response. **Worst case:** a large header map, slow/large body and a costly version/profile parser; all headers remain live and validated state is reconstructed. It is also awkward if checking metadata creates a non-Clone ownership token/resource that body decoding must consume exactly once. B can carry such a Send token; A cannot carry it without becoming a context interface. **Excludes:** persistent typed validation state, ephemeral/non-reconstructible validation products, streaming output and the same out-of-scope policy subsystems as B.

I prefer B for explicit ownership and future downstream metadata needs. A is a viable simpler-performance alternative if retaining the whole native head and reparsing selected fields are accepted product trade-offs. No timing result here establishes either as universally faster.

## 3. Moving standard codecs out, with ordinary Rust resolution

The proposed production graph is:

```text
concord facade -> concord_core
               -> concord_macros (proc macro)
               -> concord_extras (optional)
concord_extras -> concord_core, serializer/MIME libraries
concord_core   -> reqwest, http, bytes, neutral composition support
concord_macros -> syn, quote, proc-macro2, http/mime for authored literals
```

“Extras depends only on core” means only core **among the Concord crates**; serde_json/serde_urlencoded/mime remain ordinary dependencies of the appropriate extras features. Core has no serializer-specific direct dependency. Macros needs no core host-runtime dependency and no extras dependency. The prototype's packages are prefixed `challenge-` to prevent confusion with production artifacts; their library names are `concord_core`, `concord_extras`, `concord_macros`, `concord`.

All four standard markers are actual implementations of the public traits in `prototype/extras/src/lib.rs`. Json/Form use their serializers; Text is UTF-8; Binary accepts arbitrary media, including image/png. There is no built-in word enum, codec MIME catalog, feature table or serde bound in the prototype proc macro. It parses a `syn::TypePath`, including generic arguments, and emits that path untouched. A lowercase alias called `json` follows exactly the same resolution path. Production must delete `syntax::Word`, `format_type`'s Word arm and built-in rustdoc/diagnostic tables, rather than moving them into the facade.

Exact hygienic facade wrapper implemented by the prototype:

```rust
#[doc(hidden)]
pub mod __private {
    pub use concord_core as core;
    pub use concord_macros::api as api_impl;
}
#[macro_export]
macro_rules! api {
    ($($tokens:tt)*) => {
        $crate::__private::api_impl! {
            $crate::__private::core; $($tokens)*
        }
    };
}
```

Let `K` denote the hygienic `$crate::__private::core` path passed in that first field. For an effective `format Json;`, a body `body item: NewItem` generates the obligation `Json: K::Encode<NewItem>` and an output `-> Item` generates `Json: K::Decode<Item>`. In the compiled prototype these obligations occur in:

```rust
K::prepare::<Json, NewItem, Item>(
    self.http, BASE, "POST", PATH, Some(self.body), None,
    K::Decoder::<Item>::new::<Json>(),
)
```

For `response my_crate::Csv;` and `-> Vec<Row>` the decoder expression is exactly:

```rust
K::Decoder::<Vec<Row>>::new::<my_crate::Csv>()
```

No encoder bound is required unless a body uses that codec; no decoder bound if there is no typed output. `request F "media/type";` replaces the request codec and explicit media together. It does not inherit a media literal from the previous codec. Generated prepare encodes only an actual body and installs that body's explicit/default media independently of the selected response. The prototype demonstrates form-in/Json-out and JSON bytes labelled `application/apply-patch+yaml`; there is no encoder/media allowlist.

There is no `K::Json` synthesis: Json must be in the author's Rust scope (or qualified), just as Csv must. A renamed facade works because its identity enters only through `$crate`; the user's format paths retain call-site hygiene. Core-only handwritten code calls `Decoder::new::<Local>()`/`Prepared::new` directly, requiring no facade. The separate macros+core consumer passes its renamed core path directly as the hidden first field. That proves the bridge edge; exposing raw proc-macro invocation as another supported authoring product is unnecessary. Pin facade/core/macros hidden-API versions exactly together; the prototype uses `=0.0.0` for those internal edges. A real incompatible-version diagnostic/packaged-consumer check remains integration work.

Executed isolated consumers:

* `consumers/facade-noextras`: renamed `sdk`, default features disabled, downstream `json` alias, cfg-disabled unavailable codec/payload. Its normal dependency tree contains no extras.
* `consumers/extras-only`: core + extras without facade/macros.
* `consumers/macros-core`: renamed core + macro without extras/facade.
* `prototype/core-only`: handwritten core codec and decoder, no extras/macros/facade dependency.

These are separate Cargo workspaces where appropriate; they are not an inference from a feature-unified workspace build. The full consumer imports extras independently and compiles Json, qualified Csv, lowercase alias, absolute path and generic `IgnoreContentType<Json>`.

Generated rustdoc can state “Typed output uses [`Json`] (`Json`), selected by the nearest response/format statement; the codec validates metadata and may suggest Accept. See its type docs for representation policy.” A generic adapter's intra-doc link targets its base type and the text retains the whole type expression. It can also report payload type, body presence, path, literal request media and the precise owner that selected the statement. It cannot claim an arbitrary codec's MIME catalog or evaluate a trait during expansion. Production should retain its existing inheritance-origin information for that precise owner; the bounded macro probe documents only “nearest statement.” Warnings-denied rustdoc resolves the prototype links.

**Scope of this executable macro:** one-level clients, body/output positions, the three statements, paths and cfg attributes. It is a lowering/hygiene experiment, not a replacement compiler: scopes, full production header DSL, optional bodies, all diagnostics, feature-forwarding granularity and source-packaging migration are not implemented here. The existing statement DSL remains fixed in the production proposal. Recommend facade JSON convenience by default plus opt-in Form and extras-free `default-features=false`; the prototype deliberately defaults to no extras and does not claim to validate the production feature policy.

## 4. Accept truth table and exact phase boundary

Both interfaces use `Decode::ACCEPT: Option<&'static str>` and the same core hook:

```rust
pub enum FallbackDisposition { Eligible, Suppressed }
pub fn apply_accept<T>(
    headers: &mut http::HeaderMap,
    disposition: FallbackDisposition,
    decoder: Decoder<T>,
) -> Result<(), BodyError>;
```

The hook inserts a checked `HeaderValue` only when disposition is Eligible, the completed authored HeaderMap has no Accept, and the decoder supplies a suggestion. Invalid extension suggestions produce a safe build error. No output means `Decoder::drain()` supplies none. For A, substitute its decoder in the hook; no HTTP rule changes.

Composition order is **declarations resolved/structurally validated → presence-based selection → checked conversion of surviving authored values → suggestion fallback → final generic sensitivity marking → native execution**. Encoder body media resolution is independent. The suggestion is never a synthetic `Set` in authored `HeaderPlan::compose`; it cannot become a removal target, create a duplicate introduction or alter append/override legality. Normal sensitivity marking must also cover an inserted suggestion when the resolved Accept name is classified sensitive; existing flags on authored values are never cleared. The reduced prototype does not migrate production's sensitivity engine.

The authored plan carries a generic per-name disposition/tombstone. A Remove makes the field Suppressed. A subsequent legal Set/Append/Override operation reopens authoring and restores Eligible, even if its optional contribution produces no actual value. A tombstone is about intent, not `HeaderMap::contains_key`.

| Authored composition / result | Prepared Accept with typed suggested decoder |
|---|---|
| No authored Accept, no removal | Suggestion |
| Surviving literal Accept (any casing) | Authored values only |
| Native ACCEPT, alias or reexport of same resolved name | Same as literal |
| Optional authored Accept absent, no suppression | Suggestion |
| Inherited declaration removed, whether its value was present or absent | Absent; retain suppression |
| Remove then required Set | Authored value |
| Remove then optional Set present | Authored value |
| Remove then optional Set absent | Suggestion; reintroduction reopens the field |
| Remove then Append present / absent | Authored append / suggestion, respectively |
| Optional OverrideElseInherit absent | Inherited actual values if any, otherwise eligible fallback |
| Optional OverrideElseInherit present | Replacement actual values, otherwise normal presence semantics |
| Empty authored HeaderValue | Empty remains authored presence |
| Repeated fields, lists, q=0 | Preserve authored bytes and order; no parsing/negotiation |
| No typed output | No decoder-derived Accept, regardless of inherited format |
| Remove without any earlier authored declaration | Structural error; a decoder suggestion creates no target |
| into_parts, caller deletes/replaces Accept | Do not reapply fallback; retain the same decoder |
| into_parts, caller inserts a broader/different Accept | Same decoder still validates response Content-Type |

For disabling **only** a suggestion without an authored declaration, select a small downstream codec wrapper with `ACCEPT=None`; do not accidentally legalize `remove ACCEPT` just because Json suggests a value. Ordinary overrides/removals keep their declaration-based target semantics even when an optional value is absent.

**Reqwest boundary, inspected and executed:** locked `ClientBuilder::new` installs default `Accept: */*`. At `Client::execute_request`, reqwest iterates client defaults and uses `Entry::Vacant` on request headers; present request fields win. Prepared request contents do not expose that Client map, and Concord cannot discover or globally suppress it. A generated `application/json` overrides a caller's `client/custom`. A suppressed/absent Concord field allows `client/custom`, or the native default `*/*`, onto the wire. An authored empty field is present and prevents substitution. `actual_wire_accept_defaults_suppression_empty_and_repetition` tests these exact wire outcomes, including repeated authored values. There is no empty-field hack or client rebuilding to simulate suppression.

Accept is a suggestion about the request, not a codec selector or response equality predicate. Json advertises exactly `application/json`, never `application/*+json`. An explicit Accept for HTML does not turn Json into an HTML decoder or cause prepare to reject that declaration. Native mutation does not rewrite Decoder. Applications own negotiation policy.

## 5. Name validation: a structural proof, not a fabricated request

Proposed public associated signature for **each generated client**, in both alternatives:

```rust
pub fn validate_header_names()
    -> Result<(), Vec<concord_core::headers::NameError>>;
```

The prototype manually spells a generated-looking `Contracts::validate_header_names` with this shape. It aggregates independent endpoint errors in declaration order without constructing a client or its unconstructible endpoint payload. It is not yet a generated production client method.

Exact reduced shared engine in `prototype/core/src/headers.rs`:

```rust
pub enum Op { Set, Append, Override, OverrideElseInherit, Remove }
pub struct Declaration<'a> {
    pub name: &'a http::HeaderName,
    pub op: Op,
    pub label: &'static str,
}
pub struct NameError {
    pub endpoint: &'static str,
    pub declaration: &'static str,
    pub earlier: Option<&'static str>,
    pub reason: &'static str,
}
pub struct CheckedNames<'a, const N: usize> { /* declarations private */ }
pub fn validate_structure<'a, const N: usize>(
    endpoint: &'static str,
    declarations: [Declaration<'a>; N],
) -> Result<CheckedNames<'a, N>, Vec<NameError>>;
impl<'a, const N: usize> CheckedNames<'a, N> {
    pub fn select(&self, present: [bool; N]) -> HeaderPlan<N>;
}
```

Map the current `HeaderPlan::compose` concretely:

1. Resolve every native occurrence through the sealed witness, preserving the normalized root-to-endpoint sequence and per-owner removals-before-contributions ordering already generated by `native_headers_code`.
2. Extract the existing `reserved_name`, `earlier`, `set_before` and operation-legality checks into `validate_structure`. These checks use no values or presence flags. An absent optional Set still introduces a declaration; it does not erase a target or legitimize a second Set.
3. Put `closes(j)` and `live[i]` behind `CheckedNames::select(present)`. Only OverrideElseInherit's actual presence affects closing; conversion has an additional per-source presence gate. Do not run any conversion merely to decide a native name.
4. Keep sticky name/sensitivity classification in a shared structural calculation, preserving all contributions, including removed/overridden ones. The reduced prototype does not implement that classifier. Install the Accept disposition derived from authored name/operation history independently of actual HeaderMap presence.
5. Keep runtime checks requiring actual body/header values in prepare. Codec-supplied/default media values, dynamic header validity, formatting and initializer behavior are not proved by this validator. Nor does it predict Client defaults, remote response types, payload schemas or actual presence.

There is an important baseline nuance: current `compose` checks `explicit_media && live[i]` for Content-Type **without** the final emitted-value presence gate. Do not quietly change that to “conflict only if a value happens to be present.” Under its declaration-based rule, a chain whose final Content-Type operation leaves an authored declaration active conflicts with an explicit request media declaration, even if the authored value is optional/absent. That universal name/declaration fact may be checked by the structural pass using the endpoint's explicit-media declaration bit; it does not need invented runtime presence. The prototype's reduced engine leaves this media integration unimplemented. If integration chooses a different presence-dependent rule, that is a separately documented semantic change with separate tests, not a stronger validator proof.

The validator may honestly prove that all currently active endpoints have coherent resolved name/operation chains, including duplicate introductions, target existence and reserved declarations. It cannot prove that any value initializer will succeed or that a future prepare will succeed. It must never call the current whole `compose` with all optional values arbitrarily true/false and advertise completeness.

Aggregate errors in stable endpoint/occurrence order; report the endpoint and static declaration labels (and earlier label where relevant). After a structurally invalid chain, diagnostics describe its written operation history; no invalid plan reaches select. No exponential branch enumeration is needed. cfg attributes must guard the generated validation blocks with the same effective cfg as endpoint/owner accessors. No active endpoints means Ok. Reserve/diagnose collisions with the new generated method name. Those full macro integration/sentinel/cfg tests remain required; the manually assembled prototype proves the engine split, not those unimplemented integrations.

**Executed cross-check:** every 3-entry chain over two names and five operations, across all eight presence assignments (8,000 cases), agrees with an independent sequential oracle for structural validity and emitted liveness. The snapshot's existing exhaustive `header_plan` oracle and sticky-sensitivity test also pass unchanged on both recorded compilers. The validator aggregation fixture takes no presence arrays and no payloads. Full defaults/Display/ToHeaderValue/codec panic-sentinel coverage across generated nested owners is still outstanding.

**Occurrence indexing proposal, inspection only:** `emit::key_users` scans every owner/header list once per key and from condition emission. Build one reverse index after model construction:

```rust
struct KeyOccurrence {
    path: syn::Path,
    text: String,
    declaring_owner: usize,
    contribution: ContribId,
    // path span and owner effective cfg retained through those objects
}
// Model::key_paths: Vec<KeyOccurrence>; one element per written native path.
```

A single pass over declaring owners/contributions can fill owner/provenance for existing occurrence IDs, avoiding parser-order surgery. Inherited consumers refer to that occurrence, rather than becoming new declarations. Accessors and condition cfg query the indexed declaring owner. Never key this index by printed path text: `$crate::KEY` from two crates can print identically and resolve differently. This proposal is not implemented or benchmarked here; 100/1,000/4,000-endpoint expansion/total-build measurements and runtime contribution scaling stay separate integration work.

## 6. Codec errors and native response preservation

Exact common API used by primary, alternative and all extras codecs:

```rust
pub struct Position { pub label: &'static str, pub value: u64 }
pub struct BodyError {
    label: &'static str,
    positions: [Option<Position>; 3],
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}
impl BodyError {
    pub fn new(label: &'static str) -> Self;
    pub fn at(self, label: &'static str, value: u64) -> Self;
    pub fn with_source(self, source: impl std::error::Error + Send + Sync + 'static)
        -> Self;
    pub fn description(&self) -> &'static str;
    pub fn positions(&self) -> impl Iterator<Item = Position> + '_;
    pub fn body_source(&self)
        -> Option<&(dyn std::error::Error + Send + Sync + 'static)>;
}
pub enum ErrorKind { Build, Transport, Status, Metadata, Decode }
```

The first three `at` calls fill bounded inline slots; further positions are ignored. That small documented limit avoids an unrestricted string map or diagnostic framework. Core has no JSON-specific category, record enum or serializer dependency. The static constructor rejects `BodyError::new(e.to_string())`/`format!(...)` at compile time. Numeric record/line/column context remains useful; labels must be trusted, content-free author text. This deliberately cannot prevent `String::leak`, sensitive literals, sensitive numbers, malicious extension logging or code putting secrets in a static label.

Display/Debug print the static label and numeric context, never the attached cause. `std::error::Error::source()` returns None for both BodyError and public Error. The explicit unredacted `body_source` accessor retrieves the parser source. The downstream NDJSON probe decodes a foreign `Vec<u32>`, failing on record 2 with line/column while a deliberately input-echoing parser cause remains retrievable only explicitly.

The public Error representation uses `Metadata { error: BodyError, response: reqwest::Response }`, separate from `Decode(BodyError)` and `Status(Response)`. Metadata failure means no Concord body polling; malformed/schema body failure means body reading already succeeded and there is no unread response. Transport failures are converted safely with URL removed and no source chain exposed. The prototype's Error is boxed to keep its outer size modest, as the existing Error already is.

Common response/cause accessors are exactly:

```rust
pub fn kind(&self) -> ErrorKind;
pub fn response(&self) -> Option<&reqwest::Response>;
pub fn into_response(self) -> Result<reqwest::Response, Self>;
pub fn codec_error(&self) -> Option<&BodyError>;
pub fn body_source(&self)
    -> Option<&(dyn std::error::Error + Send + Sync + 'static)>;
pub fn transport(&self) -> Option<&reqwest::Error>;
```

`response`/`into_response` cover both Status and Metadata. `into_response` on any other failure returns that error with its payload retained. All explicit native response/header/body/cause/transport access is unredacted; ordinary formatting does not print the native response's Debug (which includes URL and header values). Existing production endpoint/status accessors should be retained and extended to Metadata as appropriate.

Two independent unread-body witnesses execute:

* An instrumented custom `http_body::Body` would count each `poll_frame` and remain pending. Metadata and status failures return with the count exactly zero.
* A real local TCP server sends an HTML response head, then withholds its one-byte body behind a oneshot channel. Strict decode returns Metadata before that channel is released. Only after taking the preserved response does the test release the server and explicitly read `7`. A buffer-first implementation would time out. This proves the core does not wait for/read the response body; it does not claim reqwest or the operating system never performs network read-ahead.

The compatibility path is tested with HTML-labelled numeric JSON and accepts it only when selected. The same adapter rejects unsupported response versions, non-UTF-8 charset declarations, malformed/schema bodies and non-success status at their proper stages.

Strict extras Json requires exactly one Content-Type field: missing, malformed, duplicate equal, conflicting duplicates and comma-combined values all fail Metadata. It accepts application/json and concrete application structured +json types, case-insensitively with MIME parameters, while rejecting application/json+xml and wildcard/non-concrete suffix labels. UTF-8 is the body encoding; declared other charsets are rejected in the retained context hook, with no transcoding. Other parsed parameters do not select a new codec. Binary imposes no media gate or Accept suggestion. Text accepts arbitrary/missing/malformed media types, decodes UTF-8 bytes, and rejects non-UTF-8 charset declarations when a unique valid MIME type declares one. Form requires one application/x-www-form-urlencoded Content-Type with UTF-8 policy. `JsonOrEmpty` demonstrates an explicit bodyless-204 policy while 200 still requires JSON metadata and valid JSON.

Decompression is a transport concern. I inspected locked reqwest's gzip documentation and tower-http 0.6.11 `decompression/future.rs`: supported automatic decompression wraps the body and removes Content-Encoding and Content-Length before the response reaches codecs. Neither prototype decompresses anything or confuses Content-Encoding with media/charset. An actual gzip wire fixture was **not run**: an offline feature probe fails because `async-compression` is not cached. The separate reqwest `stream` feature probe fails because `wasm-streams` is missing, even for this native resolution. Both failures are retained in `blocked/` and `evidence/offline-block-*.log`, explicitly classified as environment blocks, not acceptance passes. The primary streaming upload works using public Body::wrap without enabling that feature. C19 therefore remains source-inspection evidence, not executable transport coverage.

## 7. NativeKey: seal the intended invariant witness

Use the existing UFCS call shape and add a private supertrait:

```rust
mod key_seal {
    pub trait Sealed {}
    impl Sealed for http::HeaderName {}
}
#[doc(hidden)]
pub trait NativeKey: key_seal::Sealed {
    fn native_key(&self) -> &http::HeaderName;
}
impl NativeKey for http::HeaderName {
    fn native_key(&self) -> &http::HeaderName { self }
}
// generated accessor:
// Clone::clone(K::NativeKey::native_key(&USER_PATH))
```

Keep the existing neutral `on_unimplemented` diagnostic in the production move. Implement no blanket Deref/AsRef/reference-wrapper support. Ordinary HeaderName constants, immutable statics, aliases and reexports retain their exact Self type. `$crate` path token hygiene is independent of sealing, so the current per-occurrence native accessor strategy can remain. Existing caching assumptions then concern real native names, not arbitrary extension callbacks. Encode/Decode remain open.

Executable baseline/closed contrast: the external `baseline-stateful` consumer implements the snapshot's public hidden NativeKey for an atomic-counter key and returns x-public on one call, authorization on the next. It compiles and runs. The same style of implementation against the sealed prototype fails with E0277 because it cannot implement the inaccessible supertrait.

The apparently simpler replacement `fn native_key(name: &HeaderName) -> &HeaderName` is **not an exact-type proof by itself**. Rust inserts Deref coercions. The runnable test `sealed_witness_supports_native_items_but_plain_reference_allows_deref_trick` defines a downstream stateful Deref wrapper and demonstrates that this function returns authorization and then x-public. The explicit UFCS sealed trait call does not grant that wrapper an implementation. A genuinely exact-type alternative would need a separately proved non-coercing check; that is unnecessary complexity compared with sealing the existing witness.

Constants/statics/aliases are tested against the sealed trait. Full cross-crate helper-macro/native-key occurrence hygiene and cached-sensitivity integration remain inspected/reasoned from the existing expansion, not a claim that the production compiler was patched and re-audited here.

## 8. Counterexamples and triage

| Evidence | Actual result | Disposition |
|---|---|---|
| Naive borrowed head used after bytes consumes Response | E0505 | A must move/copy selected head data; B must own Context |
| Borrowed header retained as Context | Lifetime failure | Do not smuggle a response borrow behind erased state |
| Rc retained Context | E0277 | Keep Send guarantee; copy safe state or reconstruct local non-Send machinery after the await |
| A version check plus decode | Two parses versus B's one | Real interface cost/awkwardness, not timing benchmark |
| Owned error description | E0308 | Static normal construction enforced |
| Snapshot stateful NativeKey implementation | Compiles and changes name | Confirmed invariant gap in baseline |
| Sealed stateful NativeKey implementation | E0277 | Closed in prototype |
| Naive &HeaderName function | Accepts stateful Deref | Reject this proposed “exact” witness as insufficient |
| Json subtype test initially omitted suffix distinction | Intended strict assertion failed; false Ok(7) | Corrected; before log retained, final strict test passes |
| Generic adapter skipping all validation | Version/charset would be lost | Interface instead separates media gate; retained checks tested |

The prototype is fit to inform contract selection. It is not a production candidate awaiting a rubber stamp. Integration blockers/limits: full header-validator generation and sensitivity composition, production macro extraction/migration, exact package/features/packaging consumers, full error-site audit, decompression runtime coverage, redirect credential boundary, workload measurements and fresh independent review remain the coordinator's work. There is no acceptance claim.

## 9. TEST_MATRIX mapping

“Executed” here means a discriminating prototype assertion/command, not completion of the production acceptance row. `evidence/commands.json` is the command index; named runtime tests are in `prototype/consumer/src/tests.rs`.

| Answer / decision | Affected IDs | Evidence and disposition |
|---|---|---|
| Q1 metadata/state ownership | C01–C08, C13, C15, C16, C20; E04, E06 | Executed primary/alternative version tests, strict media/schema/empty tests, pending socket and poll counter, safe error/take-response tests, Send and lifetime failures. Vendor media + wrong-schema combinations are not exhaustively crossed. |
| Q1 independent consuming Encode | A08; C12, C14; M07, M10 | Executed streaming foreign File, form-in/Json-out, no-body media absence, apply-patch media override. |
| Q1 no-output/raw/native escape | C09, C17, C18; M01 | Executed drain, raw 302 and retained-decoder mutation tests. Production inherited defaults and unbounded drain policy also inspected. |
| Q1 extras policies/custom empty | C04, C05, C10, C11 | Executed duplicate/malformed/case/charset policy, bodyless 204 custom codec, UTF-8 Text and image/png Binary. Invalid UTF-8 raw-byte and every Form metadata error combination are not individually exercised. |
| Q1 transport transformations | C19 | Locked reqwest/tower-http source inspected; gzip runtime not executed. |
| Q2 crate graph and path lowering | A01–A03, A05–A09; R08 | Executed core-only and isolated facade/extras/macros consumers, renamed facade, ordinary aliases/qualified/absolute/generic paths, encoder-only/decoder-only and cfg-disabled types, warnings-denied generated rustdoc. Production nested inheritance/helper macros remain integration work. |
| Q2 feature/package contract | A04, A10; R06, R09 | Proposed/documented boundaries only. No production JSON/Form feature-isolation matrix, unpacked package tests or documentation migration performed. |
| Q3 fallback/author intent | M01–M06; C18; H10 | Executed suppression/reintroduction engine cases, native/literal identity, empty/repeated/q-value wire behavior, caller defaults, decoder mutation and independent oracle. Full generated header DSL/sensitivity integration remains outstanding. |
| Q3 body/request media independence | M07–M10 | Executed no-body media absence, Form/Json split and apply-patch override. Empty/nullable/optional-body matrix M08 and full authored Content-Type conflict M09 are not implemented by the reduced macro. Baseline M09 declaration rule inspected. |
| Q4 no-instance shared structure | H01–H07, H12, H13 | Executed manual client-associated aggregation/unconstructible payload fixture, resolved aliases, all 3-entry/presence oracle; unchanged snapshot oracle on both compilers. Full generated nested/cfg/name-collision/sentinel/Riot/synthetic coverage not claimed. |
| Q4 occurrence index | H09, H14 | Concrete index design and complexity reasoning only; no index or performance change implemented/measured. |
| Q4/6 sticky classification | H07, H10, H11; M11, M12 | Snapshot sensitive-name/oracle source inspected; baseline sticky-sensitivity test executed. Full hygiene, removed-value guards and literal/dynamic validity regressions not re-audited after a production extraction. |
| Q5 errors | E01–E06; C06, C07 | Executed dynamic-label compile failure, secret-echo parser/source test, external NDJSON numeric record test, unread response and explicit take behavior. Core source has no serializer vocabulary/implementation. |
| Q6 witness closure | H08; H07, H10 | Executed baseline exploit, sealed external rejection, constants/statics/aliases and stateful-Deref counterexample. Cross-crate $crate occurrence behavior inspected/reasoned, not integrated. |
| Settings/compiler control | T01, T02 | Effective harness metadata and installed compilers verified locally; focused same-source HeaderPlan control on both compilers. Official latest-release verification and full baseline are not claimed. |
| Maintained verification/diagnostics | T03, T04 | Prototype fmt, Clippy, runtime/compile-fail/doc generation executed. No claim of local/CI suite unification or production diagnostics/nightly Clone review. |
| Other application/review work | R01–R05, R07 | Outside this bounded challenge; Riot source inspected, no live request, redirect implementation or fresh-review acceptance performed. Raw/status distinction is exercised by the prototype but not certified for Riot. |
| Optional path experiment | P01–P07 | Not undertaken; ordinary interpolation remains unchanged. No path experiment or feature adoption claim. |

Raw figures are limited to those actually observed: B=1/A=2 version parses and 32/48-byte decoder handles on the recorded host; 8,000 reduced structural oracle branches; poll counter zero on rejection. Cargo's own build/test durations in raw logs are not architecture-performance measurements. No benchmark numbers, allocation counts, universal speedup or end-to-end acceptance are inferred.
