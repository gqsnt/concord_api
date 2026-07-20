# Phase 9 Integrated Conformance Report

## Verdict

`PHASE_9_COMPLETE_WITH_LIMITATIONS`

Every Phase 9-owned prepared-endpoint and final stable-extension criterion is implemented and
directly proved at the immutable source/test commit. The 15 partial rows retain only exact Phase 10
release-final clauses.

`PHASE_10_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable identities

- Phase 9 starting HEAD: `f376d8abef8db8ca12a756dfa8977412c4778c71`
- Branch: `master`
- Initial tree: clean
- Previous immutable source/test commit: `515b33ebd345a5eae06a6f2681cbfa4ff403ab9e`
- Previous evidence/report/state commit: `fe020a107770b0ea844a304efbcb73544c350949`
- Previous evidence anchor: `163677c7e8c51ff3f3925809087dbac18ae788fa`
- Prepared-endpoint runtime source/test commit:
  `fdb5fed4c47b29831325e403620923bd52769250`
- Extension-certification source/test commit and final immutable tested source:
  `a8732c3fdefb676bfbb46bee9d756a8282ec531f`
- Original Phase 9 evidence/report/state commit: `4f26b5d1336c6634068bbaac708bf05662880cf6`
- Original Phase 9 evidence anchor: `854467ba40ebf5f1b0d190a165ef50d09519575d`
- Compatible post-anchor metadata descendant: the final documentation-only commit with subject
  `docs: record phase 9 evidence anchor`; it records the real anchor identity and does not replace it
- Authoritative source validation recorded: `2026-07-19T17:13:21Z`
- Prepared runtime patch SHA-256:
  `15b3944f0212964783f33c648bd5764a084a75b68c4ccb7674ed3c4a5601e9e5`
- Extension-certification patch SHA-256:
  `1594f316782922274323b39de4aab3d96bcf99cd475c04bd454ab27f7dc5dbf0`
- Cumulative executable patch SHA-256:
  `5d2489e81d78759f64ceaccc8b162be3831cec0732a56350437f1a7ab03c6f1c`
- Evidence-correction starting HEAD: `4eac35d10b238532e574c550cffe9865303ea353`
- Superseded evidence/report/state commit: `4f26b5d1336c6634068bbaac708bf05662880cf6`
- Superseded evidence anchor: `854467ba40ebf5f1b0d190a165ef50d09519575d`
- Corrected Phase 9 evidence/report/state commit: `PENDING_PHASE_9_EVIDENCE`
- Corrected Phase 9 evidence anchor: `PENDING_PHASE_9_ANCHOR`
- Corrected anchor-metadata descendant: the final documentation-only commit with subject
  `docs: record phase 9 evidence correction anchor`

The ignored contract map is `git_diff/phase-9-contract-map.md` and contains exactly 279 unique
requirement IDs in 14 trace families. Its SHA-256 is
`c51f5cc055b7e83c055eff3e12888dcb2ceabffc99f71e976dd534e9d6437c46`. The authoritative ignored
command ledger is `git_diff/phase-9-validation.md`; the semantic evidence map, normative-owner
review, and claim atoms are under `git_diff/phase-9-*.tsv`.

The focused correction contract map is
`git_diff/phase-9-evidence-correction-contract-map.md`. It covers the five command-to-claim defects
under `API-PREP-001`, `API-RUNTIME-001`, `API-RUNTIME-002`, `FEAT-GOV-002`, and
`COMPAT-SURF-001`, plus the complete
281-row review, immutable-executable equivalence, preservation, and anchoring obligations. It
contains 10 unique requirement IDs: those five corrected owners plus the five exact catalog and
trace-audit owners. The
supplemental command ledger is `git_diff/phase-9-evidence-correction-validation.md`.

## Commit outcome

| Unit | Commit | Durable result |
|---|---|---|
| Prepared endpoint runtime and tests | `fdb5fed4c47b29831325e403620923bd52769250` | Adds the exact advanced prepared contracts, private body erasure, existing `EndpointSpec` integration, generated-client capability support, generated contract 10, deterministic runtime tests, and compile-fail privacy/capability proof. |
| Stable extension certification | `a8732c3fdefb676bfbb46bee9d756a8282ec531f` | Adds one public-only downstream fixture for all stable extension workflows, feature variants, rendered public/generated Rustdoc audits, and complete extension obligation documentation. |
| Original evidence/report/state | `4f26b5d1336c6634068bbaac708bf05662880cf6` | Superseded evidence history; records the initial Phase 9 evidence state without changing executable artifacts. |
| Original evidence anchor | `854467ba40ebf5f1b0d190a165ef50d09519575d` | Superseded evidence history; finalized the original evidence-state self-reference only. |
| Anchor metadata descendant | `docs: record phase 9 evidence anchor` | Records the real anchor hash in this report and controller state while retaining `anchor-must-be-an-ancestor`. |
| Corrected evidence/report/state | `PENDING_PHASE_9_EVIDENCE` | Supersedes the earlier Phase 9 evidence state by aligning commands, paths, assertions, coverage notes, and independent claim atoms; executable artifacts remain unchanged. |
| Corrected evidence anchor | `PENDING_PHASE_9_ANCHOR` | Finalizes the corrected evidence-state self-reference only. |
| Corrected anchor metadata descendant | `docs: record phase 9 evidence correction anchor` | Records the actual corrected anchor in this report and controller without creating another anchor. |

No executable artifact changed after `a8732c3fdefb676bfbb46bee9d756a8282ec531f`.

## Exact prepared public contracts

The advanced namespace now exports exactly:

```rust
pub struct PreparedBuffered<C>(PhantomData<C>);
pub struct PreparedBytes;
pub struct PreparedNoContent;
pub struct PreparedStream<M>(PhantomData<M>);

pub struct PreparedEndpointBuilder<R = PreparedNoContent> { /* private */ }
pub struct PreparedEndpoint<R = PreparedNoContent> { /* private */ }
```

Construction is exact:

```rust
impl PreparedEndpoint<PreparedNoContent> {
    pub fn builder(
        endpoint: &'static str,
        method: Method,
    ) -> PreparedEndpointBuilder<PreparedNoContent>;
}
```

`PreparedEndpointBuilder<R>` exposes only these stable methods:

```text
path_segment<T>(self, value: T) -> Self where T: Display
query<T>(self, name: impl Into<String>, value: T) -> Self where T: Display
query_optional<T>(self, name: impl Into<String>, value: Option<T>) -> Self where T: Display
query_values<T, I>(self, name: impl Into<String>, values: I) -> Self
    where T: Display, I: IntoIterator<Item = T>
header<T>(self, name: HeaderName, value: T) -> Self where T: Display
body(self, body: RequestBody) -> Self
body_codec<C>(self, value: C::Input) -> PreparedEndpointBuilder<R> where C: EncodeBody
auth(self, auth: Auth) -> Self
timeout(self, duration: Duration) -> Self
disable_timeout(self) -> Self
rate_limit(self, rule: RateLimitRule) -> Self
response<C>(self) -> PreparedEndpointBuilder<PreparedBuffered<C>> where C: DecodeBody
response_bytes(self) -> PreparedEndpointBuilder<PreparedBytes>
response_no_content(self) -> PreparedEndpointBuilder<PreparedNoContent>
response_stream<M>(self) -> PreparedEndpointBuilder<PreparedStream<M>> where M: MediaType
build(self) -> Result<PreparedEndpoint<R>, ConfigurationError>
```

All marker, builder, and endpoint fields are private. These types are exported through
`concord_core::advanced` only; the ordinary prelude remains unchanged. No raw-path, pagination,
raw-success, status-bypass, body-limit-bypass, recovery-bypass, or limiter-bypass method exists.

## Builder composition and route safety

Composition is deterministic:

- path segments, query pairs, authentication placements, and rate-limit rules append in call order;
- `query_optional(_, None)` appends nothing and `query_values` preserves iterator/repetition order;
- repeated public headers replace the earlier value through case-insensitive `HeaderName` equality;
- body selections replace the earlier body;
- response selections consume the previous builder and replace the marker type;
- timeout/disable selections replace the earlier Concord deadline selection;
- structural failures return typed `ConfigurationError` without retaining protected input text.

Every path value goes through `url` path-segment insertion. `/`, `?`, `#`, and `%` become encoded
data inside one segment. Literal `.` and `..` are first represented as percent text so `url` cannot
normalize them as path traversal; the percent signs are then segment-encoded, leaving inert ordered
segments and preserving the generated client's fixed base route. Empty segments follow `url`
semantics. Tests prove ordered ordinary, empty, slash, dot-dot, query, fragment, percent, and
repeated cases with no query/fragment injection and no raw-fragment API.

Header construction reuses the same reserved-header and `HeaderValue` validation as generated
public headers. Canonical preparation reuses the ordinary media, authentication, query/header
collision, `Authorization`, `Content-Type`, `Accept`, and stream-media validation.

Builder construction and `build` allocate no operation identity, execute no codec/factory/provider,
acquire no limiter, invoke no hook/debug output, construct no native request, and perform no HTTP.

## Body and response integration

One private `PreparedRequestBody` erases only the concrete prepared body selection and delegates
metadata, materialization, limits, replayability, and recovery reconstruction to the existing sealed
`BodySpec` implementations. It is not exported from a stable namespace or rendered Rustdoc.

Prepared execution supports:

- empty, reusable bytes, one-shot stream, and reconstructible `BodyFactory` request bodies;
- feature-owned multipart and multipart factory bodies;
- deferred `body_codec<C>` encoding through the existing bounded codec lifecycle;
- buffered decode, bytes, no-content, and bounded lazy stream response modes.

Codec and factory work begins only after `before_send` approves the Concord attempt. Encoded bytes
use the request-body limit and the post-synchronous-codec deadline check. One successful buffered
encoding is reused for the single permitted authentication recovery; reconstructible factories run
once per Concord attempt. One-shot streams and non-replayable multipart do not perform an unsafe
recovery resend. Reqwest-internal retry cannot recreate a Concord body.

Response markers map directly to the existing handlers:

```text
PreparedBuffered<C> -> C::Output
PreparedBytes       -> Bytes
PreparedNoContent   -> ()
PreparedStream<M>   -> StreamResponse<M>
```

Buffered, bytes, and no-content requests retain `.response()` metadata terminals. Prepared streams
have no metadata terminal, complete when the stream opens, retain caller-poll stream limits, and do
not emit stream-consumption completion callbacks.

## Generated client integration

Each generated client retains its existing API-specific public capability trait and private seal.
Generated contract version 10 adds implementations of that local seal and capability only for:

```rust
concord_core::advanced::PreparedEndpoint<R>
where
    R: concord_core::__private::PreparedResponseMarker
```

The core marker bridge is sealed and doc-hidden. It only maps the four supported stable markers to
existing private response handlers. Explicit generated endpoint values remain client-specific;
cross-client explicit endpoints, arbitrary application values, and downstream capability forgery
still fail at compile time. The same prepared definition intentionally works through distinct
generated clients and receives the selected client's base URL, managed core, HTTP identity,
credentials, hooks, limiter, and finite runtime configuration. No runtime client identifier,
dynamic ownership check, response downcast, public plan, or public decoder bridge was introduced.

Nested/repeated macro expansions, aliases, deterministic names, renamed `concord_core`, and private
contract mismatch diagnostics pass. No generated prepared helper name or public namespace collision
was added.

## Canonical lifecycle parity

`PreparedEndpoint<R>` implements the existing private `EndpointSpec` and returns the existing
`Request<PreparedEndpoint<R>>`. It uses the ordinary preparation and executor without a prepared
executor or peer request representation:

```text
PreparedEndpointBuilder<R>
-> PreparedEndpoint<R>
-> generated client.request(endpoint)
-> Request<PreparedEndpoint<R>>
-> existing EndpointSpec / HeadBuilder / PreparedHead / BodySpec
-> existing executor and managed application Reqwest client
```

The observed lifecycle remains:

```text
public head/body metadata preparation
-> before-send hook
-> body materialization
-> credential preflight
-> limiter acquisition
-> credential finalization
-> auth-free native request construction
-> safe request debug
-> native-only authentication
-> managed Reqwest execution
-> response debug and response-head hook
-> limiter observation
-> authentication challenge / status / body / decode
-> completion
```

Prepared tests cover method, logical/safe URL, path/query order, public/media headers, deadline,
body encoding, authentication, limiter plan/acquisition, debug, native send, response head, status
body, four response modes, and completion metadata. Build and first-poll failures cut off later work.
Hook, body, auth preflight, limiter, finalization, HTTP, status, response-body, decode, stream, and
completion phases retain ordinary categories and earliest-failure precedence.

Operation identity is allocated on first poll and retained across the one recovery. An operation
deadline is fixed at first poll; synchronous extension returns are followed by the established
deadline checks. Fixed-result completion runs outside that deadline and cannot change the result.
Cancellation drops future work without rollback promises or fabricated completion. With native
retry disabled, each Concord attempt performs one visible application execute call. The local HTTP/2
probe observes two hidden physical attempts but only one prepared hook/limiter/observation lifecycle
when Reqwest default retry is explicitly enabled.

## Authentication and limiter behavior

Prepared authentication contains only the ordered `Auth` values explicitly attached to the
prepared endpoint. Build/canonical preparation rejects public/header/query collisions, conflicting
`Authorization` ownership, incompatible duplicate placements, malformed structures, and more than
one challenge-enabled placement. Bearer, Basic, custom header, query, and compatible multiple
placements all use the existing native-only materialization boundary.

Credential preflight remains before limiter acquisition; exact-version finalization remains after
limiter waiting. Existing deterministic credential tests cover clear/replacement/expiry/rejection
during that wait. Prepared provider recovery proves one refresh, one operation identity/deadline,
two Concord attempts, two limiter acquisitions and observations, one codec encoding, one completion,
and no secret/public protected sentinel in automatic diagnostics. Static and endpoint-backed
challenge rules and provider refresh remain the existing typed credential-state behavior. No
credential version, lease, slot, or provider invalidation callback is public.

Prepared rate rules form the existing immutable `RateLimitPlan` in call order. The selected custom
limiter receives actual IDs, key parts, costs, windows, and response observation. Aggregate key-byte
limits fail before protected work. Authentication recovery reacquires once for the second Concord
attempt; hidden native retry does not reacquire.

With `rate-limit-governor`, the selected governor executes prepared plans and enforces its configured
key limit. Without the governor or an explicit limiter, the private fallback executes an empty
prepared plan but rejects a nonempty plan before native send. Explicit `NoopRateLimiter` or a custom
limiter may authorize a nonempty plan. There is no silent no-op fallback.

## Stable extension certification

`concord_macros/tests/fixtures/extension_certification` is one standalone downstream crate. Its only
core imports are `concord_core::prelude` and `concord_core::advanced`; it uses the macro crate only to
declare its test client. It implements or exercises:

- `MediaType`, `EncodeBody`, and `DecodeBody`;
- `BodyFactory` and feature-gated `MultipartFactory`;
- custom `CredentialMaterial`/`SecretCredential`, `CredentialProvider`, provider HTTP,
  `ProviderBodyFactory`, and typed generated credential handles;
- custom `PaginationController` and `HasNextCursor`;
- custom `RateLimiter` and `RateLimitObserver`;
- `RuntimeHooks` including page and aggregate completion;
- prepared buffered, bytes, no-content, and stream endpoints;
- prepared factory/custom-codec bodies plus feature-gated JSON and multipart factory workflows;
- `SafeProxy` and managed HTTP/runtime configuration.

The fixture compiles under default, no-default, JSON-only, multipart-only, and combined extension
features. Source audits reject `__private`, development integration, native Reqwest, `ClientCore`,
`EndpointSpec`, response handlers, prepared heads, execution plans, pagination adapters, downcasts,
transport/executor implementation, and unsafe code. Rendered core and generated Rustdoc exclude
private bridge/seal/body/response/native names.

Rustdoc for every stable extension contract now states its applicable concurrency, cancellation,
sensitive-input, application logging/storage, bounded CPU/memory/retry/map/queue/task/metric/I/O,
operation-deadline, and completion-outside-deadline obligations. It does not claim Concord can
preempt synchronous application code or automatically bound custom extension state.

## Security, limits, and privacy

Prepared high-entropy sentinels cover path, public query, sensitive public header, body, credential,
authentication header/query, rate key, response body, and authenticated native URL. Tests inspect
safe URLs, sanitized headers, `Display`, `Debug`, recursive sources, hooks, built-in debug output,
completion, limiter errors, request captures, wrapper privacy, public Rustdoc, and generated Rustdoc.
Only the approved selected recipients receive their intentional data: custom codecs, providers,
custom limiters, and managed native execution.

Existing positive finite request, response, status-body, stream, provider, rate-key, and pagination
limits remain. Prepared request-body forms and response modes cannot bypass their existing
first-excess checks. Checked arithmetic, HTTPS-only preflight, managed-client ownership, no system
proxy, native retry containment, and safe native-error reduction remain unchanged.

No `unsafe` block, production dependency, production feature, public state machine, public plan,
or validation recipe was added. The fixture lockfile expansion belongs only to the standalone
compile-topology workspace and introduces no production dependency.

## Exact executable change surface

The two source/test commits change exactly 43 files (3,673 insertions and 51 deletions):

```text
concord_core/src/__private.rs
concord_core/src/advanced.rs
concord_core/src/auth/material.rs
concord_core/src/auth/provider.rs
concord_core/src/body/mod.rs
concord_core/src/body/multipart.rs
concord_core/src/body/prepared.rs
concord_core/src/codec/mod.rs
concord_core/src/execute/auth_tests.rs
concord_core/src/execute/tests.rs
concord_core/src/generated_contract.rs
concord_core/src/hooks/mod.rs
concord_core/src/http/provider.rs
concord_core/src/pagination/controller.rs
concord_core/src/rate_limit/limiter.rs
concord_core/src/request/head.rs
concord_core/src/request/mod.rs
concord_core/src/request/prepared.rs
concord_core/tests/generated_contract.rs
concord_core/tests/public_rustdoc.rs
concord_macros/src/codegen.rs
concord_macros/tests/compile.rs
concord_macros/tests/fixtures/Cargo.lock
concord_macros/tests/fixtures/Cargo.toml
concord_macros/tests/fixtures/contract_mismatch/incompatible_core/lib.rs
concord_macros/tests/fixtures/extension_certification/Cargo.toml
concord_macros/tests/fixtures/extension_certification/src/lib.rs
concord_macros/tests/fixtures/renamed_core/src/lib.rs
concord_macros/tests/phase9_prepared.rs
concord_macros/tests/ui.rs
concord_macros/tests/ui/fail/arbitrary_endpoint.stderr
concord_macros/tests/ui/fail/cross_client_endpoint.stderr
concord_macros/tests/ui/fail/forged_endpoint.stderr
concord_macros/tests/ui/fail/phase9_prepared_builder_methods_absent.rs
concord_macros/tests/ui/fail/phase9_prepared_builder_methods_absent.stderr
concord_macros/tests/ui/fail/phase9_prepared_fields_private.rs
concord_macros/tests/ui/fail/phase9_prepared_fields_private.stderr
concord_macros/tests/ui/fail/phase9_prepared_no_prelude_export.rs
concord_macros/tests/ui/fail/phase9_prepared_no_prelude_export.stderr
concord_macros/tests/ui/fail/phase9_prepared_pagination_absent.rs
concord_macros/tests/ui/fail/phase9_prepared_pagination_absent.stderr
concord_macros/tests/ui/fail/phase9_prepared_stream_response_absent.rs
concord_macros/tests/ui/fail/phase9_prepared_stream_response_absent.stderr
```

Generated contract version 10 is the sole private macro/core compatibility revision. No test was
deleted, ignored, retried, weakened, or reclassified to conceal a failure.

## Evidence correction and executable equivalence

The evidence correction found five command-to-claim defects in the accepted Phase 9 catalog:

- `P9-API-001` cited public API, UI, namespace, and Rustdoc proof but omitted the prepared runtime
  tests required by `API-PREP-001`;
- `P9-API-006` inferred finite ordinary runtime ceilings from unrelated prepared-marker and Rustdoc
  checks instead of `RuntimeConfigBuilder`, its downstream type check, and method-absence audit;
- `P9-API-007` did not execute the status collection and disablement tests required by
  `API-RUNTIME-002`;
- `P9-FEAT-008` executed the exact no-governor test but omitted its runtime source/test path and did
  not independently bind empty-plan execution versus nonempty-plan fail-closed behavior for
  `FEAT-GOV-002`;
- `P9-FEAT-012` omitted
  `concord_macros/tests/fixtures/extension_certification/src/lib.rs`, the primary downstream
  implementation artifact for `COMPAT-SURF-001`, and used a broad feature matrix instead of the two
  exact fixture tests.

The complete 281-row review found no other inaccurate Phase 9 path, command, assertion, or
classification. All other source rows already name a focused Phase 9 command and a maintained
matrix command that passed at the immutable source identity. The strengthened verifier now parses
authority directly from the approved normative Markdown, represents the row assertion separately
from the complete normative clause, validates named Nextest or read-only-audit surfaces against the
command that actually selects them, and retains one exact unresolved atom for each partial row.
Human source-grounded owner review remains recorded for all 281 rows.

Before focused execution, an ordered `path + blob` audit compared 314 executable inputs at
`a8732c3fdefb676bfbb46bee9d756a8282ec531f` and the correction starting HEAD. Both sets had SHA-256
`4747a087576a62e604b450c6e0c521fccb4ac7a831ec3473e99d4bee91c1c382`; no executable path or blob
differed. Source, tests, fixtures, manifests, lockfile, generated contracts, Nextest configuration,
maintained recipes, toolchain, and dependency policy therefore remain exactly the validated Phase 9
inputs.

## Focused validation

Every command ran from `F:\projects\concord_api`, used no external service or environment override,
exited 0, and tested `a8732c3fdefb676bfbb46bee9d756a8282ec531f`:

| Exact command | Result |
|---|---|
| `cargo nextest run -p concord_core --all-features -E 'test(/prepared/)'` | 21/21 passed. |
| `cargo nextest run -p concord_macros --test phase9_prepared` | 9/9 exact API/generated-runtime/body/auth/limiter tests passed. |
| `cargo nextest run -p concord_macros --test ui` | 23/23 Trybuild groups passed, including Phase 9 capability/privacy/pagination absence. |
| `cargo nextest run -p concord_macros --test compile` | 10/10 fixture, feature, renamed-core, contract-mismatch, and generated Rustdoc tests passed. |
| `cargo nextest run -p concord_core --test public_rustdoc` | 1/1 rendered public Rustdoc/privacy test passed. |
| `cargo nextest run -p concord_core --no-default-features -E 'test(/prepared/)'` | 13/13 no-default prepared/body tests passed. |
| `cargo nextest run -p concord_core --no-default-features private_fallback_rejects_a_nonempty_runtime_plan_before_send` | 1/1 no-governor fail-closed test passed. |
| `cargo nextest run -p concord_core --all-features -E 'test(/hidden_reqwest_retry\|cancellation_drops_direct_and_factory_created_request_streams/)'` | 3/3 hidden-retry/cancellation tests passed. |
| `cargo nextest run -p concord_core --test runtime_config` | 1/1 downstream nonzero-limit signature test passed. |
| `cargo nextest run -p concord_core --test status_body_contract` | 12/12 finite complete-only status-body tests passed. |
| `cargo nextest run -p concord_core --lib -E 'test(=runtime::config::tests::status_capture_disablement_is_none_and_finite_limit_reenables)'` | 1/1 exact discard-only disablement test passed. |
| `cargo nextest run -p concord_macros --test compile -E 'test(complete_extension_fixture_compiles_through_public_namespaces_for_every_owned_feature_set) \| test(complete_extension_fixture_has_no_private_native_or_integration_escape)'` | 2/2 exact extension compilation/privacy tests passed across default, no-default, JSON, multipart, and combined features. |
| `powershell -NoProfile -File git_diff/validate-phase-9-runtime-api.ps1` | Immutable-source signature/absence/status/prepared-bypass audit passed. |

## Complete maintained validation

All 16 maintained commands exited 0 against the same immutable source commit:

| Exact command | Configuration and observed result |
|---|---|
| `just fmt-check` | Workspace formatting passed. |
| `just check` | Workspace/all targets/default features passed. |
| `just check-all` | Workspace/all targets/all features passed. |
| `just check-no-default` | Core/all targets/no default features passed. |
| `just clippy` | Workspace/default features with warnings denied passed. |
| `just clippy-all` | Workspace/all features with warnings denied passed. |
| `just test` | Nextest default workspace: 528/528 passed. |
| `just test-all` | Nextest CI all features: 532/532 passed. |
| `just test-no-default` | Maintained core no-default case: 1/1 passed. |
| `just doctest` | Maintained default Cargo doctests: 2/2 passed. |
| `just doctest-all` | Maintained all-feature Cargo doctests: 2/2 passed. |
| `just doc` | Three warning-denied default Rustdoc builds passed. |
| `just doc-all` | Three warning-denied all-feature Rustdoc builds passed. |
| `just deny` | Advisories, bans, licenses, and sources passed; only the two established allowed duplicate-version warnings were reported. |
| `just diff-check` | Tracked diff integrity passed. |
| `just release` | Complete maintained aggregate, including dangerous-feature isolation, exited 0. |

No applicable check was unavailable and no weaker substitute was used. The original exact
environment and command ledger has SHA-256
`607266de07508b39b9fcfde44c6f00ab15e05edd43092f734389b82e375ceff8`.
Because the executable input tree is byte-identical and this correction changes documentation and
catalogs only, the complete maintained Rust matrix was not rerun. Its immutable results remain
authoritative; only the focused existing tests and read-only audits needed for corrected evidence
traceability were rerun.

## Evidence and trace reconciliation

Phase 9 traces 279 exact requirements: 264 direct and 15 partial. Those traces consist of 276
immutable source/test evidence rows (262 direct, 14 partial) plus three requirement-trace audits
(two direct, one partial). Two additional direct catalog audits prove exact normative ownership and
historical preservation. The complete Phase 9 total is therefore 281 rows: 266 direct and 15
partial.

The complete evidence catalog contains 1,473 unique evidence IDs. All 1,192 Phase 0-8 rows remain
byte-identical and in original order; their 728,264-byte prefix has SHA-256
`51af8b44728f6cfbea3c0600d73933316aa34073a363e22b5d1bcf75cdb639d7`. Coverage still contains
exactly one row for each of 504 normative requirements; exactly 279 rows point to Phase 9 evidence,
with taxonomy 264 `P9-prepared-extension` and 15 `P9-partial-P10`. Planned `PLAN-*` identifiers
remain distinct from actual evidence.

The owner-aware claim-atom map contains 595 atoms total, 580 proved and 15 unresolved. Each of the
261 non-corrected direct rows has separate proved atoms for its Phase 9 assertion and its parsed
complete normative clause. The five corrected direct rows expand into 43 independent runtime,
finite-limit, status-capture, no-governor, and extension-workflow atoms. Each partial row has one
proved Phase 9 atom plus one exact unresolved Phase 10 owner atom. Every direct row has no unresolved
Phase 9 atom.

The corrected evidence anchor runs the ignored semantic audit against the real corrected
evidence-state identity. It derives 504 approved normative owners, checks all 281 Phase 9 rows and
595 claim atoms, validates
paths, commands, labels, results, identities, ancestry, counts, report/program/controller
reconciliation, coverage equality, and the unchanged Phase 0-8 byte prefix.

## Remaining limitations and Phase 10 owners

Phase 10 owns exactly these 15 partial requirements:

```text
API-PRIVATE-001 API-PRIVATE-002
GEN-DOC-002
SEC-TEST-001
FEAT-DIAG-001 FEAT-DOC-001
DSL-FEAT-001
CONF-PARSER-001 CONF-COMPILE-001 CONF-COMPILE-003
CONF-REQWEST-001 CONF-SEC-001 CONF-DOC-001 CONF-MSRV-001 CONF-TRACE-006
```

They retain only release-final whole-product namespace/private-signature, generated documentation,
sentinel/security, feature diagnostic/documentation/grammar, final raw-success generated category,
invalid-surface, Reqwest-assumption, canonical-example, MSRV, and evidence-completeness closure. No
failed Phase 9 criterion is classified as a limitation.

## Final architecture and engineering review

- One production `Request<E>`, public-head/prepared-head path, executor, application client,
  provider client, response handler set, authentication path, limiter path, operation identity,
  deadline, status path, and completion lifecycle remain.
- Private body erasure cannot execute independently and delegates to the existing sealed body
  lifecycle. The doc-hidden response marker bridge only maps exact stable markers.
- No lock spans an await or invokes application code; no detached work was added.
- Finite ceilings, checked arithmetic, first-excess behavior, earliest-error precedence,
  cancellation, recovery bounds, and redaction remain explicit and tested.
- No public native request/response/client/URL, transport, executor, endpoint plan, authentication
  plan, limiter state, pagination adapter, decoder plan, downcast, or client identifier exists.
- No prepared pagination, `.paginate`, page controller, raw-success marker, status bypass, raw path,
  response-limit bypass, or Phase 10 capability was implemented.
- No test, recipe, verifier, dependency policy, or historical evidence row was weakened.

## Next phase

Phase 10 — Product Completion and Release Candidate: `NOT_STARTED`.
