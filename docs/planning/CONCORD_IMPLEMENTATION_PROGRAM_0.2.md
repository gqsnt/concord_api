# CONCORD_IMPLEMENTATION_PROGRAM_0.2

**Version:** 0.2
**Status:** Internal Implementation Candidate
**Authority:** Validated history through Phase 12 and approved delivery sequence through Phase 14
**Depends on:** All normative and architecture documents
**Supersedes:** `CONCORD_IMPLEMENTATION_PLAN_0.1.md` for future execution

## Purpose

This program preserves the implementation state validated through historical Phase 10 and records
the approved Phases 11-14 delivery sequence. Phase 10 remains valid for its exact source and 504
then-active requirements. The Phase 11 contract amendment returns the current normative package to
`Normative Implementation Candidate`; only Phase 14 may restore `Normative Release Final` after
whole-product conformance closure.

The 0.1 implementation plan remains available as historical evidence. Its unchecked tasks are not
an execution queue because several foundations were delivered in a different order.

## Completed phase ledger

| Phase | Verdict | Durable outcome | Accepted limitations |
|---|---|---|---|
| Phase 0 | `PHASE_0_COMPLETE_WITH_LIMITATIONS` | Workspace, macro/core contract spike, reduced auth/pagination models, Reqwest probes, and typed-executor proof. | Proof architecture was not a production endpoint runtime; governance and Rustdoc gates were later corrected. |
| Phase 1 | `PHASE_1_COMPLETE_WITH_LIMITATIONS` | Safe errors/redaction, finite configuration, private managed application/provider clients, deterministic final-call seam, and canonical Nextest/release architecture. | No generated client core, pending request, lifecycle execution, or successful-response pipeline existed. |
| Phase 2 | `PHASE_2_COMPLETE_WITH_LIMITATIONS` | First executable generated bodyless request: fallible generated construction, shared private core, stable pending wrappers, first-poll lifecycle, hooks/debug, managed Reqwest, bounded bytes/status handling, no-content, metadata terminal, and an API-specific sealed explicit-endpoint capability. | Only the explicitly authorized Phase 2 DSL and response subset is implemented; broad 0.1 evidence remains partial where later categories are absent, and the PR 2.3 uncommitted review patch omitted one subsequently reviewed committed file. |
| Phase 3 | `PHASE_3_COMPLETE_WITH_LIMITATIONS` | One buffered codec vertical slice: stable media/encode/decode contracts, Text, feature-owned JSON, downstream custom codecs, finite request bodies, bounded decoded successes, coherent media headers, POST body DSL, typed direct/metadata results, and preserved Bytes/NoContent/status behavior. The final corrected source type-gates `.response()` to metadata-capable modes, semantically proves that terminal in rendered Rustdoc, and completes source-only namespace proof across prelude, advanced, and dangerous. | Fifty broader requirement rows remain partial. Their unimplemented clauses include credential-storage privacy, public-header precedence, Brotli/Deflate runtime probes, full grammar/semantic coverage, Phase 4 streaming/factories/multipart, and provider/limiter/pagination/extension/release-wide variants owned by Phases 4-10. |
| Phase 4 | `PHASE_4_COMPLETE_WITH_LIMITATIONS` | One finite body architecture now covers buffered values, lazy request streams, reconstructible factories, inferred replayability, lazy typed response streams, and feature-gated request multipart. Generated `Stream<M>`/`Multipart` markers, Reqwest-owned multipart framing, stream-open completion, cancellation, no-default support, and maintained independent dangerous-feature isolation all use the existing request executor. | Forty-six source rows and one audit row remain partial where their text also requires Phase 5 shaping, Phase 6 recovery/providers, Phase 7 limiter behavior, Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 raw successful responses and release-wide grammar, security, documentation, platform, and evidence closure. |
| Phase 5 | `PHASE_5_COMPLETE_WITH_LIMITATIONS` | One normalized compiler and generated runtime now support multiple clients, root endpoints, nested scopes, required/optional/default client-scope-endpoint values, immutable snapshots, sealed request extension traits through generated preludes, explicit endpoint parity, host/path/ordered-query/public-header shaping, timeout/retry, all standard methods, custom methods, aliases, and deterministic names on the existing first-poll executor. | Seventy-three source requirements and one audit row remain partial for complete compound clauses owned by Phase 6 credentials/auth/providers/recovery, Phase 7 limiting, Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 positive raw success and release-wide closure. An independent controller correction reclassified 18 Phase 5 rows without changing product source. |
| Phase 6 | `PHASE_6_COMPLETE_WITH_LIMITATIONS` | The one generated request path now supports all six credential sources, typed public and generated authentication contracts, bounded single-flight credential state, source-aware preflight/finalization/native materialization, isolated finite provider HTTP, JSON-owned OAuth2 client credentials, and at most one complete recovery attempt with the same identity and deadline. | Forty-six source rows and one audit row remain partial only for compound clauses owned by Phase 7 limiting, Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 positive raw success and release-wide closure. The private no-op limiter seam is not Phase 7 evidence. |
| Phase 7 | `PHASE_7_COMPLETE_WITH_LIMITATIONS` | The existing generated request path now supports exact rate-limit DSL composition, stable advanced limiter contracts, immutable generated plans, a bounded process-local governor, custom and explicit no-op limiters, fail-closed no-governor behavior, deadline/cancellation-safe per-attempt acquisition, response-head cooldown observation, and real recovery/Retry-After integration. | Thirty-eight source/test rows and one audit row remain partial only for compound clauses owned by Phase 8 pagination, Phase 9 prepared endpoints/extension certification, or Phase 10 positive raw success and release-wide closure. |
| Phase 8 | `PHASE_8_COMPLETE_WITH_LIMITATIONS` | The same generated request path now supports built-in/custom pagination DSL, exact stable page-limit/controller/safe-patch contracts, conditionally generated directly awaitable terminals, one typed request-local sequential page loop, canonical cycle protection, finite page/item/identity ceilings, ordinary per-page deadlines/authentication/limiting/completion, and one aggregate completion. | Twenty-eight source/test rows and one audit row remain partial only for Phase 9 prepared endpoints/final extension certification or Phase 10 release-final feature, namespace, documentation, security, dependency, and evidence closure. |
| Phase 9 | `PHASE_9_COMPLETE_WITH_LIMITATIONS` | Stable advanced prepared endpoints now use the same `Request<E>`, private `EndpointSpec`, body/auth/limiter/deadline/hook/status/response/completion lifecycle, generated-client runtime, and managed HTTP clients as generated endpoints. One public-only downstream fixture certifies every stable extension workflow and feature-owned variant. | Fifteen rows remain partial only for Phase 10 release-final namespace, generated documentation, sentinel/security, feature/grammar/diagnostic, raw-success generated-category, dependency-assumption, canonical-example, MSRV, and evidence-completeness closure. |
| Phase 10 | `PHASE_10_COMPLETE` | Concord 0.1 release-candidate closure: bounded dangerous raw success, release-stable executable naming, canonical examples, local benchmarks, complete grammar/diagnostic, dependency, security, property, Rustdoc, feature, MSRV, and maintained-release validation, and direct evidence for all 504 normative requirements. | None. No waiver or unresolved release criterion remains. |
| Phase 11 | `PHASE_11_COMPLETE_WITH_LIMITATIONS` | One typed pagination state struct, consuming public controller trait, generated associated-controller integration, and controller-agnostic core loop; generated contract `11 -> 12`. | Only the then-unimplemented Phases 12-14. |
| Phase 12 | `PHASE_12_COMPLETE_WITH_LIMITATIONS` | Arbitrary request/response specification types, sealed buffered/streaming/discard acquisition, universal metadata, bounded byte streaming, type-neutral generated/prepared integration, pagination capability, and integrated optional records/CSV/NDJSON; generated contract `12 -> 13`. | Only unimplemented Phase 13 lexical rate limits and Phase 14 product/release-final closure. |

## Approved Phases 11-14 roadmap

| Phase | Title | Current status | Exact boundary |
|---|---|---|---|
| Phase 11 | Uniform Typed Pagination State | `COMPLETE_WITH_LIMITATIONS` | One struct DSL, one public consuming state-machine trait, one generated associated-controller integration, and one controller-agnostic core loop are validated; generated contract `11 -> 12`. |
| Phase 12 | Generic Body/Response Specifications and Integrated Streaming Records | `COMPLETE_WITH_LIMITATIONS` | Arbitrary Rust request/response specifications, three sealed core response modes, universal metadata, one bounded byte stream, type-neutral generated/prepared integration, pagination capability, and integrated optional records/CSV/NDJSON; generated contract `12 -> 13`. |
| Phase 13 | Lexically Scoped Rate Limits | `COMPLETE_WITH_LIMITATIONS` | Direct lexical ownership, ancestor composition, deterministic identities, global generated keys, advanced explicit-key preservation, and the preserved limiter lifecycle; generated contract `13 -> 14`. |
| Phase 14 | Product Completion, Conformance Closure, and Release Finalization | `NOT_STARTED` | Whole-product closure, deferred release-evidence hardening, and the only authorized restoration of `Normative Release Final`. |

Phase 13 is validated with limitations owned only by Phase 14. Phase 14 remains roadmap authority
until its approved mission. Phase 12
implements none of their product behavior.

Phase reports remain the detailed evidence owners:

- `docs/conformance/PHASE_0_REPORT.md`
- `docs/conformance/PHASE_1_REPORT.md`
- `docs/conformance/PHASE_2_REPORT.md`
- `docs/conformance/PHASE_3_REPORT.md`
- `docs/conformance/PHASE_4_REPORT.md`
- `docs/conformance/PHASE_5_REPORT.md`
- `docs/conformance/PHASE_6_REPORT.md`
- `docs/conformance/PHASE_7_REPORT.md`
- `docs/conformance/PHASE_8_REPORT.md`
- `docs/conformance/PHASE_9_REPORT.md`
- `docs/conformance/PHASE_10_REPORT.md`
- `docs/conformance/PHASE_11_REPORT.md`
- `docs/conformance/PHASE_12_REPORT.md`

## Phase 4 production baseline

The original Phase 2 source baseline is commit `055d74718c443412a605bd880cfad9415a824f99`.
The integrated-audit-corrected source baseline is
`ea1ce944fe1d2622db7a66600d734ac0dc04784f`.
The tested Phase 3 buffered source is
`c9521f649c91df0a0bae9786cba101f4d526692b`. The controller-reviewed corrected Phase 3 source is
`7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`. The final controller-reviewed Phase 3 source is
`54a64cb4ba062453dea26dd6af1dff97eaa3406e`; the complete maintained release matrix passed
against that immutable commit. The immutable tested Phase 4 product source is
`6472bc01e55bbd4282a8d5e06deeb9126650f9c4`. Controller correction source/test commit
`909021e7b82436bdfdff1e1ffea67f3255e63c77` adds only dangerous-feature isolation tests and
release wiring; its complete maintained and focused matrices pass. The complete evidence-state is
the documentation commit containing this program and every final Phase 4 evidence ID; the
subsequent documentation-only anchor reanchors the five audit rows without changing counts or
classifications. Exact hashes are recorded in the anchored Phase 4 report and Git history without
requiring impossible commit self-reference.

Its executable path is:

```text
DSL
→ normalized macro model
→ generated client/facade/endpoint
→ concord_core::Request<E>
→ first-poll identity/deadline/head preparation
→ safe public head and instance body metadata then before-send hook
→ one buffered, streamed, factory, or Reqwest-owned multipart materialization
→ one private managed native-body accounting and error boundary
→ private managed Reqwest execution
→ bounded status/Bytes; unpolled NoContent; media-validated buffered decode; or bounded lazy StreamResponse<M>
→ completion outside the result-determining deadline
→ direct semantic value, DecodedResponse<T>, or StreamResponse<M>
```

The Phase 0 executor proof and inert generated `RequestHead` are gone. The retained `cfg(test)` auth
and pagination models still prove independent future invariants and do not duplicate a Phase 2
production representation.

Each generated API now owns a locally sealed endpoint capability. Facade endpoint methods and
explicit `client.request(endpoint)` calls use the same capability-constrained path, so foreign
generated endpoints, arbitrary application values, and downstream capability implementations fail
at compile time without a runtime client identifier.

Text and downstream custom codecs work without optional JSON dependencies. With `json`, the same
generic buffered path owns `Json<T>` and activates only `mime`, `serde`, and `serde_json`. Generated
POST endpoints accept body inputs last and use the same API-specific sealed endpoint capability for
facade and explicit execution. A generated-private sealed `MetadataResponse` capability exposes
`.response()` only for buffered, Bytes, and NoContent response modes; direct await remains
independent.

The one evolved `BodySpec`, `BodyMetadata`, `PreparedBody`, and managed native-body adapter serve
empty/buffered bodies, one-shot streams, reconstructible factories, and multipart recipes. Public
`StreamBody` supports bytes, arbitrary streams, async readers, files, and exact length. Managed
polling enforces checked exact/aggregate limits, preserves safe body-error precedence, and stops on
drop. Replayability is private and derived; no application control exists. Public `RequestBody`
wires factories into the production executor through internal endpoints without adding a factory
DSL marker or prepared public endpoint.

`StreamResponse<M>` is one response handler in the existing architecture. Known-length and media
validation occur before return, unknown bodies remain lazy, cumulative post-decompression limits
withhold the first excess chunk, and caller polling is outside the endpoint deadline. Returning a
stream emits `StreamOpened` exactly once; later failure/drop emits no endpoint completion. The
handler deliberately lacks `MetadataResponse`, so streaming endpoints expose neither `.response()`
nor pagination.

Under `multipart`, public advanced recipes/factories support text, bytes, and streams. Reqwest owns
serialization and boundary generation; Concord accounts the entire emitted body including framing
through the same managed wrapper. The feature activates no JSON, compression, or dangerous
capability and adds no response mode. The parser/model/codegen pipeline maps `body: Stream<M>`,
`body: Multipart`, and `-> Stream<M>` without a second compiler path; generated contract version 5
is synchronized with core. Renamed dependencies, nested/repeated invocations, ownership sealing,
feature diagnostics, local generated execution, and rendered Rustdoc all pass on Rust 1.85.

All source-only core Cargo failures, including development-namespace escape, are now
ordinary Trybuild UI cases. The retained `config_features` and `development_features` projects
contain only genuine feature-graph checks; dependency-topology fixtures remain Cargo projects. A
feature-specific ordinary UI harness proves the development executor is also absent from the
existing dangerous namespace, while rendered core Rustdoc proves that the supported public
`.response()` terminal remains visible without rendering its private capability bound. Phase 4
now makes a complete default/custom-retry/development-tools/raw-flag isolation matrix a mandatory
dependency of `just release`; every dangerous configuration uses `--no-default-features` and
enables exactly one dangerous flag. The raw flag remains topology-only in this phase.

The corrected Phase 4 source-evidence audit records 72 direct and 46 partial rows across 118 exact
requirements, plus four direct and one partial catalog/report-integrity rows. The final 123 Phase 4
rows are 76 direct and 47 partial; the full catalog remains 568 unique IDs. `P4-FEAT-006` is partial
because positive raw-response APIs are absent even though all dangerous flags are isolated. Partial clauses name
their future owner and do not claim public-header shaping, recovery, limiter, pagination, prepared
endpoints, complete extension certification, full grammar/security evidence, or release-platform
closure. Historical Phase 0-3 evidence is unchanged.

## Phase 5 production baseline

The approved endpoint-setter decision is commit
`255e1fd94f4614b5a3c02cda0f820c4eb56146c5`. The immutable tested Phase 5 source is
`c513710c1e6ae2333a58ec481c2b74a4a0529a5f`; every maintained recipe, including `just release`,
passes against that exact commit. The original evidence state is
`a8353a151a35b3ba8a3d8ebd689ad269e7b1d948`, and its original anchor is
`10139ef98319d52a7fa6c874c72883e61427e813`. The corrected complete evidence state is
`afaa2fd091111e5fb9ca3a584ebb415735020907`; its later documentation-only anchor and full
classification details are recorded in `docs/conformance/PHASE_5_REPORT.md`.

The production compiler and execution path at the Phase 5 boundary was:

```text
DSL tokens
→ source AST
→ lexical symbol tables and resolved semantic references
→ normalized inherited client/scope/endpoint IR
→ deterministic generated names
→ immutable generated client/facade/endpoint snapshots
→ concord_core::Request<E>
→ optional endpoint-specific sealed extension methods from the generated client prelude
→ first-poll operation identity and deadline
→ one HeadBuilder and PreparedHead
→ validated host, path, ordered public query, public headers, and safe URL
→ body metadata and media ownership validation
→ before-send hook
→ existing body materialization and private managed Reqwest execution
→ existing bounded buffered, NoContent, or lazy stream response handling
→ completion and typed result
```

One `api!` invocation may generate multiple independently owned clients. Client-root endpoints,
nested scopes to depth 64, all required/optional/default public values, facade and explicit endpoint
parity, all standard methods, validated custom methods, aliases, and deterministic collision
diagnostics use one compiler pipeline. Client values resolve during build, scope values during
facade creation, and endpoint values during pending request creation; final public shaping remains
deferred to first poll. Arc-backed immutable snapshots avoid user-value `Clone` bounds.

Endpoint-specific request controls use public generated sealed extension traits. A single
`use client_support::prelude::*` imports deterministic client/full-scope/endpoint-qualified trait
aliases. The facade still returns `concord_core::Request<E>`, generic timeout/debug setters remain
inherent, and a single private consume-and-map bridge preserves the request core, options, body,
handler, and terminal capabilities without polling. Downstream trait forgery and cross-endpoint use
fail at compile time; no private core import appears in public Rustdoc.

The one evolved `HeadBuilder` preserves fixed scheme, port, and base path while validating dynamic
DNS labels and path segments, serializing ordered scalar/optional/vector public query pairs, and
building the logical and safe URLs. The existing `PreparedHead` owns the only public-header map.
Public `Accept` wins over codec defaults; codec/stream/multipart `Content-Type` ownership conflicts
fail before hooks. Inherited timeout declarations and request-local overrides remain distinct.
Application retry is disabled by omission/off, Reqwest default is explicit, dangerous custom retry
is feature-gated, and an overriding `.http(...)` selection prevents the DSL factory from running.

Generated contract version 6 is synchronized between macro and core. Normal/renamed dependencies,
nested/repeated invocations, contract mismatch, rendered Rustdoc, no-default, JSON, multipart,
streaming, ownership, and the maintained dangerous-feature matrix all pass. No authentication,
limiter, pagination execution, prepared endpoint, or positive raw-success API was added.

Phase 5 contributes 198 source-behavior rows (125 direct, 73 partial) plus five audit rows (four
direct, one partial). The complete Phase 5 total is 203 rows: 129 direct and 74 partial. The catalog
contains 771 unique IDs, all 504 normative IDs remain mapped exactly once in coverage, unknown IDs
are zero, and all 568 historical rows are byte-identical to the Phase 4 anchor.

### Phase 5 compound-evidence correction

An independent controller found that five rows classified only their Phase 5 portion instead of the
complete compound requirement. A clause-by-clause audit of all 143 direct Phase 5 source rows and
four direct audit rows found 13 additional overclaims. The following evidence rows are therefore
partial: `P5-PROD-003`, `P5-PROD-013`, `P5-PROD-014`, `P5-DSL-007`, `P5-DSL-044`,
`P5-DSL-050`, `P5-DSL-055`, `P5-DSL-056`, `P5-GEN-003`, `P5-GEN-018`, `P5-GEN-029`,
`P5-GEN-030`, `P5-GEN-032`, `P5-EXEC-009`, `P5-EXEC-012`, `P5-EXEC-013`, `P5-FEAT-008`,
and `P5-CONF-012`.

Their deferred clauses remain assigned to the established sequence: credentials, providers,
authentication, and recovery in Phase 6; limiter selection and execution in Phase 7; pagination in
Phase 8; prepared endpoints and full extension certification in Phase 9; and release-final Reqwest,
platform, documentation, security, evidence, and positive raw-success closure in Phase 10. The
correction changes evidence, coverage, report, and Program documentation only. Phase 5 product,
manifest, lockfile, recipe, and test source remain exactly the immutable
`c513710c1e6ae2333a58ec481c2b74a4a0529a5f` state. At that correction boundary, Phase 6 was next
and unstarted.

## Phase 6 production baseline

The immutable tested Phase 6 source/test commit is
`e4c1cca94962cf0ee6855bab3c252c1caba19202`. The complete focused and maintained matrix,
including `just release`, passed against that exact commit. The subsequent governance-only commit
`c7dfd11906d453de27702a6ff69f7f79a60e4dd9` changes only `.agents/`, `.codex/`, and `AGENTS.md`;
it changes no executable artifact and does not invalidate the source validation identity. The
Phase 6 evidence/report/state commit is `a4e4a004a6868f1d5dd462fef077cc948fd5109f`; the subsequent
documentation-only anchor records that evidence identity without attempting impossible commit
self-reference. Exact classification, commands, results, traces, and limitations are recorded in
`docs/conformance/PHASE_6_REPORT.md`.

The production path is now:

```text
normalized DSL credentials and authentication
→ generated declaration-ordered secrets/providers and shared typed credential handles
→ existing concord_core::Request<E>, HeadBuilder, PreparedHead, and PreparedBody
→ public preparation and body metadata
→ before-send hook and body materialization
→ complete credential preflight
→ private no-op Phase 7 acquisition seam
→ credential finalization
→ auth-free native construction and safe request debug
→ private native-only authentication materialization
→ managed application Reqwest execution
→ response-head handling and private no-op Phase 7 observation seam
→ ordinary status/decode or one source-aware complete recovery attempt
→ completion and typed result
```

Static API-key, static bearer, Basic, OAuth2 client credentials, endpoint-backed acquisition, and
custom providers use one private production slot architecture. Acquisition and refresh are
single-flight; waiter and leader cancellation retain progress; manual mutation and complete source
replacement supersede stale work; exact challenge mutation cannot overwrite newer material; finite
cooldown, version, provider-body, provider-response, and operation-deadline bounds fail closed; and
user/provider code and destructors run outside the slot lock.

Generated contract version 7 is synchronized between macro and core. Constructor inputs remain
required public values, then protected secrets, then custom providers in declaration order.
Generated client clones share credential slots while independent builds do not. Source-specific
handles expose only their approved replacement, set, clear, presence, and read-only-reference
capabilities. Endpoint-backed acquisition uses ordinary endpoint execution and preserves prior
material on failure.

Authentication remains absent from logical URLs, public headers, hooks, request debug, automatic
diagnostics, and generated documentation. It is materialized only on the private native request.
Provider HTTP has an independent client/channel, requires HTTPS before body-factory invocation,
uses finite request/response limits and the earlier deadline, and returns actual status/headers only
to the provider. OAuth2 is `json`-owned, uses exact Basic/form POST client-credentials behavior, and
reduces malformed, unsafe, non-success, and oversized responses to secret-safe bounded errors.

Phase 6 traces all 195 owned requirements through 192 source/test evidence rows (146 direct, 46
partial) and three requirement-trace state-audit rows (two direct, one partial). Two additional
direct state-audit rows prove catalog identity and historical-row preservation. The owned trace is
therefore 148 direct and 47 partial; the Phase 6 evidence total is 197 rows: 150 direct and 47
partial. The complete catalog contains 968 unique IDs; all 771 Phase
0-5 rows are byte-identical, all 504 normative requirements remain mapped once in coverage, and
every Phase 6 partial names its Phase 7-10 owner.

## Phase 7 production baseline

The corrected immutable final tested Phase 7 source/test commit is
`c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`. The accepted implementation chain remains
`fa85a77da74566e68db0a59954c1172daf75d574`,
`12691b5d5a5eec34507cb3e968fc35a8fe5c3a9d`, and
`52e4604fff4824f0061d78d38177d815e204ec78`. Correction commit
`8e3b1589ebf598c1850f273070cbc528b2dce66e` synchronizes the configured governor aggregate-key
bound at protected preparation and governor insertion. Correction commit
`c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9` serializes shared-directory Trybuild cases after the
maintained all-feature matrix exposed a concrete concurrent fixture replacement.

All 62 exact focused commands and all 16 maintained commands, including `just release`, passed
from the beginning against `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`. Evidence/report/state commit
`cb4640d4d4e21528c8e38f8104785e2c3ceac883` and anchor
`def42b74663df7b36755f61b8348d26449cce6ff` are superseded because their rows retained several
command-to-claim mismatches. The evidence-command-corrected evidence/report/state commit is
`19ed74ba4ba64f772733096ea5ad16c8551e4a9e`. Preliminary evidence state
`7b1373f82b33e36325047643d735b2e15bb5a4bc` was superseded after the strengthened semantic audit
found 24 incomplete plural-owner coverage milestones. The documentation-only corrected
anchor is `1c1a1da8739daf13dacc43ea259ab8163e8835b3`. A later documentation-only descendant records the
real anchor hash and does not replace that anchor. Exact classifications, commands, results,
traces, and remaining limitations are in `docs/conformance/PHASE_7_REPORT.md`.

The evidence-command correction changes no executable artifact. It atomizes all 100 Phase 7 rows
into 181 independent claims, aligns every proof assertion with an exact path and passing command,
and adds a structured source-identity/manifest/dependency audit plus five focused executions of
existing tests. The complete Rust matrix remains the authoritative immutable-source ledger and was
not rerun. Counts remain 61 direct and 39 partial overall: 57 direct and 38 partial source/test
rows, plus four direct and one partial evidence-state audit rows.

The production direct-attempt path is now:

```text
normalized DSL rate-limit templates and attachments
→ generated immutable effective plan and selected shared limiter
→ existing concord_core::Request<E>, HeadBuilder, PreparedHead, and PreparedBody
→ public preparation, canonical key resolution, and finite key-bound validation
→ before-send hook and body materialization
→ complete credential preflight
→ one deadline-bounded limiter acquisition per Concord attempt
→ credential finalization
→ auth-free native construction and safe request debug
→ private native-only authentication materialization
→ managed application Reqwest execution
→ response debug and response-head hook
→ body-free limiter observation affecting future calls only
→ ordinary status/decode or one source-aware recovery through a second acquisition
→ completion and typed result
```

The stable advanced surface includes exact opaque rate-limit values, safe errors, custom limiter and
observer contexts, cooldown scopes, the bounded governor, default observer, and explicit no-op
limiter. The ordinary prelude remains unchanged. Generated `.runtime(...)` remains the only custom
or no-op selection path; a DSL observer configures only the generated default governor. Generated
contract version 8 is synchronized between core and macro.

The default governor is finite at 4096 quota entries, 4096 cooldown entries, a 15-minute idle quota
lifetime, a 1024-byte aggregate resolved-key limit, and the existing 60-second cooldown cap. It
atomically accounts for multiple rules/windows and equivalent buckets, releases its lock before
waiting, consumes no quota while waiting, retains successful reservations, prunes/evicts
deterministically, and fails closed on poisoned or impossible state. Without governor support,
custom and explicit no-op limiters remain usable, empty plans use a private fallback, and required
nonempty plans fail before send.

The runtime builder now derives a private preparation-bound snapshot only for a selected built-in
governor through a safe blanket metadata bridge; custom and no-op limiters retain the 1024-byte
default. Lower, zero, default, increased, clone, independent-build, pending-request, side-effect
cutoff, recursive redaction, and governor-revalidation cases pass without changing any approved
public method signature or generated contract version.

Phase 7 traces its 98 coverage requirements through 95 source/test rows (57 direct, 38 partial) and
three requirement-trace audit rows (two direct, one partial). Two additional direct catalog audits
prove catalog identity and historical preservation. The corrected Phase 7 total is 100 rows: 61
direct and 39 partial. The complete catalog contains 1068 unique IDs; all 968 Phase 0-6 rows are
byte-identical, all 504 normative requirements remain mapped once, and all Phase 7 coverage rows
use clause-accurate source paths, tests, commands, taxonomy, and later owners.

## Phase 8 production baseline

This section records the immutable historical Phase 8 contract and source line. Its pagination
factory, safe-patch, and built-in-category statements are superseded for current implementation by
the Phase 11 authority and requirement-change record; they remain here only to describe the
historical evidence they originally validated.

The immutable final corrected Phase 8 source/test commit is
`515b33ebd345a5eae06a6f2681cbfa4ff403ab9e`. All correction-focused, retained Phase 8, and Phase 7
rate-limit regression commands plus all 16 maintained commands, including `just release`, passed
against that exact source. Evidence/report/state commit
`ccf569a3072344b345d79d2d551ccb0c0ebce57d` and anchor
`b86eb2e7516dcae4b26ac5d53f444351ebe15654` are superseded because `P8-CORR-007` described response
observation under `EXEC-RATE-002` instead of the normative atomic multi-rule reservation contract.
The atomic-reservation-corrected evidence/report/state commit is
`fe020a107770b0ea844a304efbcb73544c350949`; a later documentation-only anchor fixes that evidence-state
identity, and the final two-file metadata descendant records the real anchor in the Phase 8 report
and controller state without replacing it. The product and immutable source matrix are unchanged;
this evidence-only correction ran the exact four-test atomic governor proof and the strengthened
normative-owner audit rather than rerunning the full Rust matrix.

The one generated request path now adds an opaque directly awaitable pagination wrapper, one sealed
typed generated pagination specification, and one request-local sequential page state machine. Each
page clones the original bodyless endpoint snapshot, applies typed built-in bindings or a safe
query/header patch, constructs and retains a bounded canonical logical identity, and executes
through the ordinary preparation/executor/authentication/limiter/response/completion pipeline under
a fresh child operation identity and deadline. The proof-only pagination model was removed.

The exact stable public surface includes `PageLimit`, `PaginationError`, `PaginationRequest`,
`PaginationController`, `HasNextCursor`, and offset/page-number/cursor built-ins. Eligible generated
requests expose `.paginate(limit)` directly as a future returning `Vec<IntoIterator::Item>` with no
`.collect()` terminal or item trait. Soft/hard limits, empty and sealed built-in short termination,
custom/cursor authority, all-or-error collection, authentication recovery, hidden retry,
cancellation, and one aggregate completion are deterministic and bounded.

Defaults remain 1,000 requested pages, 100,000 collected items, and 4 MiB of retained canonical
identity bytes. Cycle protection and every ceiling are always enabled; checked arithmetic covers
page/item/identity counters and built-in progression.

The Phase 8 correction removes generated `"<pagination>"` rate-key substitution. Required
non-secret pagination-bound endpoint dimensions now retain their actual formatted per-page values,
and a private prepared-head accessor supplies the actual final logical host to the selected limiter.
Actual formatted key bytes are validated at preparation and revalidated by the built-in governor
before insertion. Page-varying keys form distinct buckets while equivalent keys aggregate. No
public signature, generated contract version, feature, dependency, or execution architecture
changed.

Continuation values affect controlled native requests and cycle identity and may intentionally
reach a selected custom limiter when they are declared resolved rate-key dimensions or the final
logical host. They remain absent from safe URLs, sanitized headers, hooks, automatic debug/events,
completion, Concord errors and sources, built-in governor errors, and rendered documentation.

Corrected Phase 8 traces 122 requirements through 119 source/test evidence rows (91 direct, 28
partial) and three requirement-trace audits (two direct, one partial). Two additional direct catalog
audits prove normative coverage and preservation. The corrected Phase 8 total is 124 rows: 95 direct
and 29 partial. The complete catalog contains 1,192 unique evidence IDs; all 1,068 Phase 0-7 rows
remain byte-identical, and all 504 normative requirements remain mapped once. The independent
claim-atom map contains 164 proved atoms and 29 unresolved later-phase owner atoms (193 total). The
owner audit parses all 504 approved requirement headings and paragraphs, binds all 124 Phase 8 rows
to those owners, and rejects response-observation claims under `EXEC-RATE-002`. Exact paths,
commands, classifications, and later owners are in `docs/conformance/PHASE_8_REPORT.md` and the
evidence catalog.

## Phase 9 production baseline

The immutable Phase 9 source/test commit is
`a8732c3fdefb676bfbb46bee9d756a8282ec531f`. The focused prepared/core, generated-client,
Trybuild/UI, downstream fixture, rendered Rustdoc, no-default, no-governor, hidden-retry, and
cancellation commands plus all 16 maintained commands, including `just release`, passed against
that exact source. The original evidence/report/state commit
`4f26b5d1336c6634068bbaac708bf05662880cf6` and anchor
`854467ba40ebf5f1b0d190a165ef50d09519575d` are superseded evidence history. The corrected
evidence/report/state commit is `7c01ec2d52d2612020b16928a848b970a27db4eb`; a subsequent documentation-only
corrected evidence anchor finalizes that evidence-state self-reference without changing executable
artifacts.

The advanced namespace now exposes only the exact four prepared response markers and opaque
`PreparedEndpointBuilder<R>` / `PreparedEndpoint<R>` contracts. Builder composition is
deterministic: one encoded segment per call; ordered repeated query pairs; case-insensitive public
header replacement; body/response/deadline replacement; ordered authentication and rate rules; and
typed body-safe `ConfigurationError`. There is no raw-path, pagination, raw-success, status-bypass,
or limit-bypass prepared method.

Prepared bodies use one private erasure wrapper over the existing sealed `BodySpec` lifecycle.
Empty, bytes, one-shot stream, reconstructible factory, deferred codec, multipart, and multipart
factory requests retain ordinary limit and replayability behavior. The four prepared response
markers map directly to existing buffered, bytes, no-content, and stream handlers. Streaming has no
metadata terminal and retains stream-open completion semantics.

Generated contract version 10 adds only a local API-specific sealed capability implementation for
supported `PreparedEndpoint<R>` markers. Generated explicit endpoints remain client-specific;
cross-client endpoints, arbitrary values, and capability forgery still fail at compile time. The
same prepared definition may intentionally use different generated clients, each supplying its own
base URL, managed core, HTTP identity, credentials, hooks, limiter, and finite runtime snapshot.
No runtime client ID, public response bridge, execution plan, downcast, or second executor exists.

Prepared authentication and limiting feed the existing immutable plans and canonical executor.
Credential preflight/finalization/native-only materialization, one recovery, limiter reacquisition,
response observation, hidden native retry containment, operation identity/deadline, completion,
cancellation, status/response errors, and safe diagnostics remain unchanged. Without governor or an
explicit limiter, empty prepared plans execute and nonempty plans fail before native send; explicit
no-op and custom limiters remain intentional opt-ins.

The standalone downstream certification fixture imports core only through `prelude` and `advanced`.
It compiles media/codecs, body/multipart factories, credential material/providers/provider HTTP,
pagination controllers/cursors, limiters/observers, runtime hooks, all prepared response modes,
custom/JSON/multipart bodies, safe proxy, and managed configuration across default, no-default,
JSON, multipart, and combined features. Source and rendered Rustdoc audits reject private,
generated-private, native Reqwest, plan/state, transport/executor, downcast, and pagination-adapter
access. Extension Rustdoc records concurrency, cancellation, sensitivity, application logging and
storage, boundedness, deadline, and completion obligations without promising automatic limits for
custom code or state.

Phase 9 traces 279 requirements through 276 immutable source/test rows (262 direct, 14 partial) and
three trace audits (two direct, one partial). Two additional direct catalog audits prove normative
coverage and historical preservation. The Phase 9 total is 281 rows: 266 direct and 15 partial. The
complete catalog contains 1,473 unique evidence IDs; all 1,192 Phase 0-8 rows remain byte-identical,
and all 504 normative requirements remain mapped once. The corrected owner-aware map contains 595
claim atoms: 580 proved and 15 exact unresolved Phase 10 atoms. It separates each direct row's
Phase 9 assertion from its parsed normative clause and expands the five corrected runtime,
no-governor, and extension rows into independent command-bound atoms. Exact identities, paths,
commands, and owners are
in `docs/conformance/PHASE_9_REPORT.md` and the evidence catalog.

## Phase 10 release-candidate baseline

The original Phase 10 evidence chain based on source
`52ceef99e7cca1fac256a55d85296480817b199a`, evidence
`5a6af781abf7551cc6da979c969f54f997572ffe`, anchor
`1c662d2e20b9e5c267743c108415de437bac6278`, and metadata
`8c8dc914194e99a4074bef378ae2ca481fb1b4b7` is retained as superseded correction history. Its raw
response product, behavior-oriented naming, examples, and release architecture remain accepted,
but its benchmark category labels and release evidence authority were not commit-exact.

The corrected immutable Phase 10 source/test commit is
`9317a94cf020d81dbd216ed2a7dc999550586898`. Four focused correction commits make the public-head
and credential-ready benchmarks exercise their named production operations, make the release
registry audit commit-scoped and owner-aware, select every registered security probe, preserve
feature-isolation proof under workspace feature unification, and give post-source paths exact
evidence-state precedence. Every focused role and all maintained commands, including `just
release`, passed against that exact source identity with Rust 1.85.0. The corrected
evidence/report/state identity is `5b157225b387387664a202d4cbfe5f85ba8c5289`; a documentation-only anchor
substitutes that identity without changing evidence classifications or executable artifacts.

Under `dangerous-raw-response`, eligible generated buffered, text, JSON, custom buffered, bytes,
and no-content requests expose `.raw_response()` and return the stable private-field
`concord_core::dangerous::RawResponse`. Raw execution shares the ordinary request pipeline through
successful status classification, applies the ordinary bounded complete collection, skips only the
endpoint decoder, returns the logical URL and actual response head, and emits ordinary successful
completion. The native Reqwest response remains private. Streaming, prepared, and paginated raw
terminals remain absent.

Ten canonical example targets compile all required product workflows without real credentials or
live external requests. One private harness-free benchmark target covers nine required local
overhead categories without a product threshold. Its public-head category calls the actual
preparation path without managed transport, and its ready-credential category runs static ready
preflight and finalization without acquisition or refresh. Release registries contain 68 grammar
entries, 18 selected Reqwest assumptions with deterministic probes, 15 high-entropy security
classes, and five reproducible property families. The maintained release command compiles examples
and benchmarks, executes the complete dangerous-feature and MSRV matrices, and retains warning
denial, zero Nextest retries, finite timeouts, bounded Cargo concurrency, and doctest-only
`cargo test` use.

Phase 10 traces 36 exact product and release-final requirements and adds 58 direct passing evidence
rows: 33 immutable-source rows and 25 evidence-state rows, including 24 cross-phase catalog or
governance audits. The complete evidence catalog contains 1,531 unique rows. Durable release
registries reconcile all 504 normative requirements to current direct passing evidence and derive
775 independent claim atoms from the authoritative paragraphs. All 775 pass with zero unresolved
atoms, zero waivers, and zero missing commit-scoped paths while preserving the 1,473-row Phase 0-9
prefix byte-for-byte. Source mode validates executable inputs without claiming final evidence
completeness; the separate final mode validates the evidence, anchor, and metadata identities.
After those gates passed, the nine normative owners retained `Normative Release Final` for the exact
historical Phase 10 contract. Concord 0.1 was a locally validated release candidate; no remote
publication, tag, registry upload, or deployment was performed. The approved Phase 11 requirement
change creates a new current implementation-candidate contract without changing that historical
result.

## Phase 11 corrected production baseline

The original Phase 11 closure based on source
`1e5d3c9e4567cde54a1b27acae5d48bd7b5d8607`, evidence
`22868a9de9bd55daa975dbcd484f085033a52217`, anchor
`bec42412043d24b8bab01bd7049b430abfb8a46a`, and metadata
`462faa033b5252c729e17f1c28fbd156b169376c` remains reachable correction history. Its product
contract and uniform typed-controller implementation remain accepted, but its downstream
extension evidence compiled the controller without executing generated construction, live binding,
and the core page loop. Several evidence commands also did not independently execute every claim
atom attributed to them.

The corrected immutable Phase 11 source/test commit is
`1ee2ce99800af671e173679efeb573cb06b0c713`. The correction adds a stable-public downstream
application controller and an ephemeral loopback two-page runtime, invokes that fixture through a
locked, offline, isolated-target Nextest harness, makes the source audit distinguish inherited
Phase 11 rows from new-source proof, and prevents an outer Nextest profile from changing fixture
selection. It changes no pagination product or macro implementation source. Every focused proof and
all 16 maintained commands, including `just release`, passed against that exact source with Rust
1.85.0.

The complete 22-row Phase 11 evidence set was re-audited claim by claim. All 22 current Phase 11
requirements have direct passing proof, with no partial row, waiver, unresolved atom, or missing
source path. The corrected catalog retains the exact 1,531-row, 1,081,762-byte Phase 0-10 prefix
with SHA-256 `aadd64241a2dc42bb19dd01643915724df1a55390debbe52f7d7becbdc71e091`.
The package remains `Normative Implementation Candidate`; Phase 12 remains active and
`NOT_STARTED`, no Phase 12-14 product behavior was added, and only Phase 14 may restore
`Normative Release Final`.

## Phase 12 approved implementation authority

The user-approved Phase 12 mission authorizes the current requirement amendment, ADR-045, both
architecture updates, prospective coverage, implementation, tests, features, exact optional CSV
dependency, generated contract `12 -> 13`, validation, evidence, and anchoring required for:

- one public `RequestBodySpec`, one managed outgoing `RequestBody`, metadata before hooks,
  preparation once, consistency validation, and inferred replayability;
- one public `ResponseBodySpec` selecting exactly sealed `Buffered`, `Streaming`, or `Discard`
  acquisition while core retains status, limits, deadline, completion, and native ownership;
- one `ApiResponse<T>` implementation, a `DecodedResponse<T>` alias, removal of
  `StreamResponse<M>`, and one bounded metadata-independent `ByteStream`;
- macro AST and normalized body `Option<syn::Type>` plus response `syn::Type`, with no semantic
  categories or built-in-name behavior;
- generated and prepared generic specification selection through the one existing executor;
- explicit `PageResponseSpec` capability rather than response-name pagination;
- integrated `records`, `csv`, and `ndjson` features inside `concord_core`, with one incremental
  record decoder/stream primitive, batching adapter, typed CSV and NDJSON, independent positive
  staging, and no separate crate or executor;
- complete downstream request, response-mode, page-response, and record-decoder proof plus exact
  feature, dependency, MSRV, security, lifecycle, regression, Rustdoc, and historical-prefix proof.

The authority commit changes no product source and makes no prospective row passing evidence.
Source and tests must be committed before validation; the final source identity then remains
immutable through the full maintained and focused matrix. Evidence/report/state, one self-reference
anchor, and one compatible metadata descendant follow only after successful validation.

Phase 12 explicitly excludes Phase 13 lexical rate-limit changes, Phase 14 release-final closure,
a records workspace crate, another request/response/records executor, unsealed or native response
access, prepared pagination, pagination over records, unbounded buffering, multipart response
parsing, SSE, WebSocket, reconnection, bidirectional streaming, and terminal stream-consumption
hooks. The package remains `Normative Implementation Candidate` throughout Phase 12.

## Phase 12 validated production baseline

The Phase 12 authority commit is `95a1b9075061184ff2fa5807bb28b16a4d172277`. The initial
source/test commit is `e71ed0671359f443f5aa165391ff56bd10debdf2`; the final non-amended
source correction for the original Phase 12 closure was
`d1aadcebcad0a65d92c4ed4a42d6b0a9c1f034b2`. Focused conformance review later found that the
downstream custom request specification lacked runtime proof through an explicit generated endpoint
value and that several Phase 12 rows used broad command-to-claim allocation. The executable
correction commit was
`8281c79d98406be7b88a20ea70d84af531e0b757`. It adds exact facade/explicit/prepared request-spec
parity, hook-veto, contradiction, and macro-neutrality proof plus validation-audit enforcement.

A later source-grounded audit identified remaining record opening/completion, post-return failure
and synchronous-drop completion, automatic diagnostic, rendered-Rustdoc feature isolation, and
public downstream credential/limiter/hook/controller runtime proof gaps. The non-amended executable
correction and final immutable tested identity is
`8307d0744363ea7ec8a98d5f5fe68c4429b71c31`. It adds deterministic records lifecycle and sentinel
tests, separate public/generated records Rustdoc checks, isolated records documentation roles,
public-only extension runtime, explicit security registry classes, and targeted release-registry
assertions. The tests exposed no product defect. No public contract, production dependency, feature,
generated contract, or approved runtime behavior changed. The complete focused and maintained
matrix, including `just release`, passed against that exact source identity on Rust 1.85.0.

The validated implementation has one public `RequestBodySpec`, one managed outgoing
`RequestBody`, one public `ResponseBodySpec`, exactly sealed `Buffered`, `Streaming`, and `Discard`
modes, one `ApiResponse<T>`, one metadata-free `ByteStream`, generic codec adapters, and a single
generated/prepared executor lifecycle. `DecodedResponse<T>` is only an alias; `StreamResponse<M>`
and the prepared response-marker taxonomy are absent. Macro body and response storage is type-
shaped and contains no semantic built-in-name classification. Pagination uses explicit
`PageResponseSpec` capability rather than response-name recognition. Core and macro generated
contract constants are 13.

Integrated feature-gated records remain inside `concord_core`. `records` has no Serde or CSV
dependency; `csv` enables records, Serde, and exact optional `csv = 1.3.1`; `ndjson` enables records,
Serde, and `serde_json` without enabling the general JSON endpoint feature. One `RecordDecoder`,
`Records<D>`, `RecordStream<D>`, and consumer-only `RecordBatches<D>` path provides bounded
chunk-independent framing. Built-in CSV and NDJSON use a 1 MiB staging maximum and withhold the
first excess append or framed record. There is no records crate, second runtime, prepared
pagination, or record-stream pagination.

Phase 12 adds exactly 127 direct and zero partial evidence rows after the byte-identical 1,553-row
Phase 0-11 catalog. Coverage contains 563 active and 11 superseded IDs. The historical Phase 10
release requirement, claim, and change-map registries remain historical; Phase 14 owns their whole-
product regeneration. The corrected claim-atom audit maps every complete active Phase 12 paragraph
to principal source/test/fixture paths and one or more exact passing commands, with each filtered
Nextest run backed by a nonzero inventory. The 179-row validation registry contains 71 preserved
historical and 108 current Phase 12 rows. Exact commits, commands, source paths, features, lineage,
record completion/drop behavior, diagnostic and Rustdoc results, downstream workflow mapping, and
the corrected anchor sequence are recorded in
`docs/conformance/PHASE_12_REPORT.md`.

## Release non-goals

Concord 0.1 intentionally does not provide:

- prepared pagination, prepared page controllers, or a prepared `.paginate(...)` method;
- prepared raw response, streaming raw response, or raw pagination;
- streaming terminal-consumption hooks, SSE, WebSocket, reconnecting or bidirectional streaming,
  pagination over record streams, or multipart response parsing.

No placeholder public API was added for these capabilities. Direct requests retain page index zero
and no aggregate parent identity.

## Current phase state

Phase 10 remains historically complete. Phase 11 remains validated for its exact source and
paragraph versions. Phase 12 — Generic Body/Response Specifications and Integrated Streaming
Records is `PHASE_12_COMPLETE_WITH_LIMITATIONS`; its limitations were owned by Phases 13-14.
Phase 13 — Lexically Scoped Rate Limits is `PHASE_13_COMPLETE_WITH_LIMITATIONS`; Phase 14 remains
`NOT_STARTED`. Final release promotion is deferred exclusively to Phase 14.

## Continuing constraints

- Keep one production request/head/execution representation.
- Keep Reqwest request/response values and managed clients private.
- Keep deterministic support private and out of generated signatures.
- Use Nextest for unit/integration tests and `cargo test` only for maintained doctest recipes.
- Preserve the immutable source/evidence anchor sequence for each phase.
- Keep the current package at `Normative Implementation Candidate` through Phases 11-13; only
  Phase 14 may restore `Normative Release Final` after complete direct release evidence is valid.

`PHASE_12_COMPLETE_WITH_LIMITATIONS`

`NORMATIVE_IMPLEMENTATION_CANDIDATE`

## Phase 13 approved implementation authority

The user-approved Phase 13 mission authorizes the requirement amendment
`docs/normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md`, ADR-046, both architecture
amendments, prospective coverage, implementation, tests, examples, generated contract `13 -> 14`,
validation, evidence, and anchoring required for lexically owned direct generated rate-limit
declarations.

Normal generated rules are declared directly at client, scope, or endpoint owners and compose in
client/outer-to-inner-scope/endpoint order. They use deterministic complete lexical declaration
identities and `RateLimitKey::global()`; request values never partition generated buckets and do not
create rate-limit-only `Display` bounds. Templates, attachments, `by`, replacement, off, and
generated dynamic dimensions are removed from the normal DSL. The optional observer is a separate
unique client-only `rate_limit_observer PATH;` declaration.

The proven governor, limiter, hook, credential, cancellation, recovery, observation, cooldown,
bounded-state, redaction, pagination, and advanced prepared explicit-key lifecycles remain owned by
their existing core paths. Phase 13 adds no new dependency, feature, public identity API, executor,
runtime state machine, or Phase 14 behavior. The package remains `Normative Implementation Candidate`;
Phase 13 is complete with only Phase 14 as a limitation; Phase 14 remains `NOT_STARTED` and alone
owns release-final promotion.

### Phase 13 correction closure

The Phase 13 correction mission stabilized generated lexical declaration identities at the generated
root, added collision-safe private symbols, descendant-module and real-governor bucket proofs,
expanded pagination validation to multiple pages with exact recovery/cancellation capture, and
rebuilt evidence from the exact Phase 0–12 catalog blob. The final executable source is the
immutable commit `3ff65c60dc4f360663f13d0e122fa102f6d21999`; Phase 13 evidence and validation state
records focused commands and the complete maintained matrix against that source, including the
locked/offline governor-disabled fixture, feature graph, outer harness, and prepared plan inspection.
The superseded
source/evidence/anchor chain remains historical. Phase 14 remains `NOT_STARTED` and continues to
own product-completion, conformance-closure, and release-final work.
