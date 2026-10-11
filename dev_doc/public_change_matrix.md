# Concord public change matrix — v2.1 → v3

**Status:** Active inventory of public changes from `2.1.0-beta.1` (branch
`baseline-v2.1`) to `3.0.0-beta.1`. Each row was checked against this tree's
source on 2026-09-26. The user-facing migration procedure is
[`docs/migration_v3.md`](../docs/migration_v3.md); the reasons are in the
[v3 ADR index](adrs/README.md). The historical v2.0 → v2.1 matrix is kept at the
end of this file.

## Façade root and modules (`crates/concord/src/lib.rs`)

| Area | v2.1 | v3 | Consequence |
|---|---|---|---|
| Root re-exports (§7.2) | `api`, `ApiError`, `ApiResponse`, atom roles, `Binary`, `ByteStream`, `Bytes`, `ClientBuildError`, `Discard`, `Empty`, `RequestStream`, `SecretString`, `Text`, `PageLimit`, `Json`/`ItemStream` when enabled | Adds `CallOptions`, `CancellationToken`, `ControlError`, `ControlSummary`, `DispatchKnowledge`, `ExecutionMeta`, `IntoValue`, `OperationId`, `RequestControls`, `StoreOutcome`; removes `PageLimit` | Every §7.2 name is importable from `concord::` |
| `concord::prelude` | includes `PageLimit` | `PageLimit` removed | — |
| `concord::inspection` | — (descriptions lived in `concord::endpoint`) | New module re-exporting core's `ContractDescription` model | Static contract via `Client::contract()`; JSON tooling in `concord-inspect` |
| `concord::endpoint` | v2.1 description types (`ContractDescription`, `SemanticChangeClass`, `contract_tool_snapshot`, `diff_contract_tool_snapshots`, …), `RuntimeValue`, `RuntimeEndpointBinding` | Those removed; adds `AbsentStatus`, `BoundRuntimeRequest`, `StoreRequest`, `RuntimeAuthDisposition`, `RuntimeBindingFacts`, `RuntimeFamily`, `RuntimeFamilyMode`, `RuntimeOperation`, `RuntimeOperationFact`, `RuntimeRouteMode`, `MAX_RUNTIME_KEY_BYTES` | Descriptions and diffs move to `inspection`/`concord-inspect` (`concord.contract/3`) |
| `concord::body` | `RequestBody`, `RequestBodyMetadata`, `RequestContentType` | `PreparedBody`, `BodyDescription`; `RequestBodyMetadata`/`RequestContentType` removed; `RequestBody` kept only as a deprecated alias of `PreparedBody` | One media authority (ADR 0018) |
| `concord::response` | `PageResponseRepresentation` | Removed; adds `BodyDisposition`, `OptionalOutcome`, `StreamState`, `StreamSummary`, `StreamErrorKind`, `MAX_ACCEPT_BYTES`, `NotFound` | Driver-based page eligibility; head-only absence (ADRs 0016, 0019) |
| `concord::codec` | Json, Text, Binary, codec roles | Adds `NotFound` | Buffered 404 adapter |
| `concord::pagination` | `PageLimit`, `PaginatedRequest` | Removed; adds `Pages`, `Page`, `CollectPrefix`, `CollectAll`, `Prefix`, `TraversalSummary`, `StopReason`, `ContinuationKnowledge`, `OffsetInit`, `PageInit`, `CursorInit`, `NEXT_CURSOR_HEADER` | Distinct terminals (ADR 0017) |
| `concord::rate_limit` | Policy, key and limiter contracts | Adds `RateKeyEncoder`, `Enforcement`, `LimiterCapability`, `RatePolicySchema`, `RateDimensionSchema`, `RateWindowSchema`, `RatePlanSummary`, `RatePolicySummary`, `encode_canonical`, `UTF8_ENCODING_ID`, `RATE_KEY_FRAMING_VERSION`, `ORIGIN_LABEL`, `ENDPOINT_LABEL`, … | Encoding IDs and schema continuity (ADR 0023) |
| `concord::auth` | Materials, providers, placements | Adds `StoreOutcome`, `StoreRequest`, `CredentialVersion`, `ProviderOrigin` (+ error types), `ChallengeRecovery`, `AuthTarget`, provider/flight limits | Intent/version protocol and provider allowlist (ADR 0021) |
| `concord::error` | Error taxonomy | Adds `AttemptFacts`, `OperationCounts`, `StatusBodyDecodeErrorKind` | Journal-derived facts on every escaping error |

## Generated surface

| Area | v2.1 | v3 | Consequence |
|---|---|---|---|
| Optional value controls | `set_x(Option<T>)`, `omit_x()`, `reset_x()` | `set_x(impl IntoValue<T>)` payload, `replace_x(Option<T>)`, `set_x_if_some(Option<T>)`, `omit_x()`, `reset_x()` per the §4.3 matrix | Old `set_x(opt)` must become `replace_x(opt)` first (ADR 0014) |
| Conversions | exact types | Closed `IntoValue<T>`: identity, `&str -> String`, `&String -> String` | No numeric widening or parsing |
| Pagination terminals | `.paginate(PageLimit::Prefix(n) / CompleteWithin(n))` returning `Vec` | `.pages(n)`, `.collect_prefix(n)` → `Prefix<Item>`, `.collect_all(NonZeroU32)` → `Vec<Item>` | Output types change intentionally |
| Credential storage | `store_as_x()` installs unconditionally | `store_as_x()` → `StoreRequest` resolving to `StoreOutcome::{Installed, Superseded}`; `client.credentials()` gains local clear/replace methods per credential | Callers handle `Superseded` |
| Request controls | per-request `debug(...)`; no shared control trait | `RequestControls` (`with_options`, `control_summary`, `cancel_with`), inherent `timeout(d) -> Result<Self, ControlError>`, `reset_timeout()` on named requests and terminal wrappers; generated named requests no longer expose `debug(...)` (client-wide `configure_runtime(|r| r.debug(level))` remains) | Generic helpers over all requests (ADR 0026) |
| Accepted-response terminals | `ResponseRequest<E>`, `AcceptedBytesRequest<E>` generic over the endpoint spec | Erased `ResponseRequest<O>` over the declared output; non-generic `AcceptedBytesRequest` | Simpler caller signatures |
| Expected absence | custom buffered representation | `absent on <status>;` with `-> Option<T>`, or `NotFound<R>` | Head-only absence (ADR 0019) |
| Body-bearing request `Clone` | conditional | Body-free requests and intents are `Clone`; requests with a body slot are not | Use a request factory (VALUE-05) |

## Runtime endpoints and configuration

| Area | v2.1 | v3 | Consequence |
|---|---|---|---|
| Binding | `endpoint.try_bind(&owner)`, `endpoint.plan(&owner)` | `owner.bind(endpoint) -> Result<BoundRuntimeRequest<R, O>, ApiError>`, `owner.plan_runtime(&endpoint)` on client and scope handles | Same owner projection at root and scope (ADR 0024) |
| Builder type defaults | `RuntimeEndpoint<R = Empty, Output = ()>` | `RuntimeEndpoint<R = (), O = ()>`; `.response::<R, O>()` selects the pair | — |
| Query/header operations | `query`, `query_value`, `query_optional`, `query_when_present`, `query_each_when_present`, `append_query…`, `remove_query`, `query_off`; headers take `HeaderName`; `headers_off` | Spec §16.2 family: `query`, `query_optional`, `query_each`, `query_each_optional`, `query_replace…`, `query_when_present`, `query_when_present_each`, `query_append…`, `query_remove`, `query_replace_all`, `query_off`, the `header_…` equivalents (names as `&str`, `header_off`), and `_sensitive`/`_via` modifiers | Same symbolic algebra as static code |
| Path operations | `path`, `path_value`, `replace_path(values)`, `path_off` | `path`, `path_via`, `path_sensitive`, `path_sensitive_via`, `path_replace()`, `path_off()` | — |
| Body/response selection | `body(RequestBody)`, `body_spec`, `body_codec`, `response_empty()`, `response_discard()`, builder `accepted_bytes()` | `body_repr::<B, T>(input)`, `response::<R, O>()`, `expose_accepted_bytes()`; `BoundRuntimeRequest::accepted_bytes() -> Result<AcceptedBytesRequest, ApiError>` | — |
| Authentication | `auth(Auth)`, `replace_auth(Auth)`, `auth_off()` | `auth_off()`, `auth_remove(..)` by safe target identity; no source selection or new private target | Declare an explicit scope instead (RUNTIME-02) |
| Runtime config setters | `request_body_limit(NonZeroUsize)`, `pagination_max_pages(NonZeroU32)`, … | Primitive `usize`/`u32` arguments; invalid values reported at build | Migrate with `.get()` (ADR 0025); new `aggregate_max_sends(u32)` (§20.4) |

## Extension traits

| Area | v2.1 | v3 | Consequence |
|---|---|---|---|
| `ResponseRepresentation` negotiation | `fn accept() -> Option<&'static str>` | `const ACCEPT: Option<&'static str>` | Static inspection (ADR 0016) |
| `RequestRepresentation` | metadata callback plus `prepare` | Single `prepare` returning `PreparedBody` | One media authority (ADR 0018) |
| Pagination controllers | public-field initialization, `PageResponseRepresentation` gate | `PaginationController<P>` with `Init`, `Cursor`, `start`, `cursor`, `advance`; `Buffered` driver required | Private controller state (ADR 0017) |
| `RateKeyPart` | used through `&dyn RateKeyPart` | Associated `ENCODING_ID` constant; `RateKeyEncoder<T>` for adapters; typed encoding | Encoding contract named (ADR 0023) |
| `CredentialProvider` | provider HTTP helper without declared origins | `http_origins()` declares exact allowed origins (empty by default) | Undeclared helper origins are rejected before transport |
| `RateLimiter` | partial reservation allowed for custom limiters | Atomic all-or-none admission; capability descriptor | Custom limiter migration (ADR 0023) |
| Provider flights | provider traffic had no identity of its own | `ProviderAcquisitionFacts` / `ProviderAcquisitionOutcome` (`error`), `ApiError::provider_acquisition()`, `ExecutionMeta::provider_acquisition()`, `OperationEvent::ProviderAcquisitionFinished` | Distinct linked provider operation (spec §13.1, EXEC-01); additive |
| Role-trait diagnostics | raw trait errors spanning the whole `api!` block | `#[diagnostic::on_unimplemented]` with `[C3-*]` codes on the role traits; generated obligations spanned at the owning placement | Compiler messages change (DIAG-01); no signature change |

## Historical: v2.0 → v2.1 matrix

Kept unchanged as history; it describes the previous major transition, not
v3.

| Area | Supplied v2.0.0 / earlier v1-style surface | v2.1 target | Consequence |
|---|---|---|---|
| source tree | client then sibling scopes/endpoints | all children inside client | module/navigation/source symmetry |
| client values | `vars` | `values` | clearer retained-fact vocabulary |
| optionality | exact `Option<T>` token recognized | postfix `?`; type opaque | alias transparency; breaking declarations |
| collection repetition | inferred/trait/wrapper behavior | explicit `each` at placement | cardinality visible |
| defaults | `= default` | `#[default]` | explicit trait-default policy |
| optional control | inner `.set_x(T)`, `.clear_x()` | exact `.set_x(Option<T>)`, `.omit_x()`, plus initialized `.reset_x()` | nested optional truth, explicit omission/reset, released `with_x` namespace |
| owner refs | `vars`, `scope`, `ep` | `client`, exact scope name, `endpoint` | no search rebinding |
| scalar composition | `fmt[...]` | `concat [...]` | one bounded atom |
| scalar wire text | implicit `Display` | `RequestAtom` or `via Adapter` | stable protocol role |
| host | `host [...]` replaces labels | `subdomain` prepend/replace/off | composable domain hierarchy |
| query/header | implicit optional/repetition; symbols in candidate | assignment, `when_present`, `append`, `each`, `remove` | word-visible inheritance/cardinality |
| request body | `#[body] body: Json<T>` | `body input: T` + inherited/local representation | exact caller input |
| response | `-> Json<T>` / wrapper towers | `-> T`, representation default or inline `via` | exact awaited output |
| representation | leaf wrapper/spec | client/scope directional default + inline exception/fence | lower repetition, effective provenance required |
| status | hidden in response spec | endpoint `status` finite set/range | reflectable success boundary |
| effect | method/repeatability binary | `safe`, `idempotent`, `non_idempotent` | pagination and retry truth |
| response bytes | universal `.response_bytes()` on modes | endpoint `expose accepted_bytes` + `.accepted_bytes()` | explicit authority/capability |
| streams | decoder/acquisition wrapper types | `ByteStream`, `ItemStream<T>`, `RequestStream<T>` | exact lifecycle without mechanism leakage |
| timeout | optional/off/per-page recreation | one finite absolute operation deadline | bounded truthful operation |
| retry | policy + hidden method/body assumptions | app classifier/backoff/budget under effect/body/dispatch gates | no unsafe amplification |
| server delay | extras parses independently | one core `ServerDelayHint` | no duplicate waiting |
| auth | private source/slot model with leaf clauses | retained model, grouped auth, exact challenge attribution | stronger security and readability |
| rate policies | `by`, windows, implicit local feature fallback | stable IDs/key/cost/`allow`; explicit limiter | truthful enforcement scope |
| rate feedback | observer can fail current result | future-admission non-fallible feedback | response cannot be hidden |
| pagination | response Vec/trait extraction, per-page concerns | safe complete page; controller owns items+continuation | envelope/header support; bounded authority |
| hooks | broad before/response/completion authority | read-only `OperationObserver` + narrow policy owners | no middleware import |
| plan | `SafeRequestPlan` | `RequestPlan` credential-free/redaction-aware | honest disclosure guarantee |
| error | implementation/stage variants and hook errors | stable category/stage/dispatch/IDs + safe diagnostics | caller decision model |
| unbound | flat/explicit endpoint forms | isomorphic `Client::unbound()` tree | same domain language |
| runtime endpoint | dynamic builder under old semantics | full v2.1 patch/representation/status/effect, fallible `try_bind` | same executor, no authority escape |
| public version | local `main` at `f7464c5` is 2.0.0 | 2.1.0 target | intentional source/API break remains an owner/product-history decision |
