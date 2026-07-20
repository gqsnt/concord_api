# Phase 10 Conformance Report

## Verdict

`PHASE_10_COMPLETE`

`Concord 0.1 Release Candidate`

Phase 10 — Product Completion and Release Candidate is complete. The immutable source/test identity
is `52ceef99e7cca1fac256a55d85296480817b199a`. All 504 normative requirements have current direct
passing evidence, zero claim atoms remain unresolved, and no waiver was used. The nine normative
owners transitioned to `Normative Release Final` only after the complete local release matrix
passed. This is a locally validated release candidate, not a published release.

## Authority and baseline

- Mission starting HEAD: `e05008c1ebc88e04f4df1001ecce00ae888282a3`
- Branch: `master`
- Initial tree: clean
- Previous immutable source/test commit: `a8732c3fdefb676bfbb46bee9d756a8282ec531f`
- Previous evidence/report/state commit: `7c01ec2d52d2612020b16928a848b970a27db4eb`
- Previous evidence anchor: `6c267878c6f62d2a9f45090f1172194e16b3b747`
- Phase 10 immutable source/test commit: `52ceef99e7cca1fac256a55d85296480817b199a`
- Phase 10 evidence/report/state commit: `PENDING_PHASE_10_EVIDENCE`
- Phase 10 evidence anchor: `PENDING_PHASE_10_ANCHOR`
- Anchor-identity metadata descendant: the final documentation-only commit with subject
  `docs: record phase 10 evidence anchor`
- Head policy: `anchor-must-be-an-ancestor`

The baseline HEAD, branch, cleanliness, Phase 9 object existence, and ancestry were observed before
editing. No material authority or implementation-boundary difference was found.

## Contract map and release registry

The ignored implementation map is `git_diff/phase-10-contract-map.md`. It contains 36 exact Phase
10 product or release-final requirement traces and a 504-entry release registry. Each trace records
the normative owner, verified pre-edit state, intended change, exact acceptance proof, maintained
command, and direct evidence owner.

The 36 Phase 10 requirements are:

- raw-success ownership: `PROD-SURF-001`, `API-RESP-001`, `API-DANGER-001`, `GEN-TERM-003`,
  `SEC-RESP-003`, `SEC-RESP-006`, `SEC-DANGER-001`, `EXEC-RAW-001`, `FEAT-DANGER-001`,
  `FEAT-DANGER-002`, `FEAT-DIAG-001`, `FEAT-DOC-001`, `COMPAT-FEATURE-001`;
- release-final closure: `API-PRIVATE-001`, `API-PRIVATE-002`, `GEN-DOC-002`, `SEC-TEST-001`,
  `DSL-FEAT-001`, `CONF-PARSER-001`, `CONF-COMPILE-001`, `CONF-COMPILE-003`,
  `CONF-REQWEST-001`, `CONF-SEC-001`, `CONF-DOC-001`, `CONF-MSRV-001`, `CONF-TRACE-006`,
  `COMPAT-BUILD-001`, `CONF-CORE-001`, `CONF-CORE-002`, `CONF-CI-001`, `CONF-CI-002`,
  `CONF-CI-003`, `CONF-CI-004`, `CONF-PROP-001`, `CONF-TEST-001`, and `CONF-RELEASE-001`.

The release registry mechanically reconciles exactly 504 normative IDs, exactly 504 coverage rows,
1,531 unique evidence rows, all 35 approved evidence labels, and zero waivers. Every requirement has
at least one direct passing evidence owner, and the complete registry was reconfirmed against the
release source identity. No requirement remains represented only by planned coverage.

## Source commits

| Commit | Purpose |
|---|---|
| `148251e6f08a577fa9cad0ad54be273af60ed02f` | Normalize executable behavior naming. |
| `c9ed133f7c48e1b8104d85013e90290ff6cc10e8` | Add gated raw successful responses. |
| `5db2936d50f7a0729bf8f39a289a80c00908271e` | Add canonical examples and local benchmarks. |
| `f0f4111b9438508eff4963bb29ac2ad68e209601` | Complete release conformance registries. |
| `d14afe6528f4a9e368e7d013d7954cdfcd263aa8` | Extend the release validation graph. |
| `22fff7668523ed434680a4dbb41bc58eeb439c2b` | Correct the OAuth expectation affected by behavior-sentinel normalization. |
| `52ceef99e7cca1fac256a55d85296480817b199a` | Correct the historical-evidence append audit without weakening it. |

The two corrections were concrete validation findings. The first regenerated the exact Basic-auth
header expected from newly normalized test material. The second stopped the release registry audit
from treating a unified-diff `---` header as a historical evidence deletion. Both were committed as
focused source corrections; the complete matrix was rerun against the latter immutable commit.

## Dangerous raw successful response

Under `dangerous-raw-response`, `concord_core::dangerous` exports the stable private-field value:

```rust
pub struct RawResponse { /* private */ }

impl RawResponse {
    pub fn url(&self) -> &Url;
    pub fn status(&self) -> StatusCode;
    pub fn version(&self) -> Version;
    pub fn headers(&self) -> &HeaderMap;
    pub fn metadata(&self) -> &ExecutionMeta;
    pub fn body(&self) -> &Bytes;
    pub fn into_body(self) -> Bytes;
}
```

`url()` returns the logical URL; native authenticated URLs remain private. `headers()` returns the
actual successful response headers. The body accessors return only complete bounded success bytes.
Structural `Debug` includes status, metadata, and body length but never body bytes. Public Rustdoc
states that callers receive sensitive response material and therefore assume a reduced secrecy
guarantee. No native Reqwest response, client, request, URL, plan, decoder, transport, or executor is
exposed.

Eligible generated buffered, text, JSON, custom buffered, bytes, and no-content requests expose:

```rust
pub fn raw_response(self) -> RawResponseRequest<E>;
```

Awaiting that wrapper produces `Result<RawResponse, ApiError>`. Streaming endpoints, prepared
endpoints, feature-disabled clients, and pagination do not expose a raw terminal. Direct await,
`.response()`, `.paginate()`, prepared endpoints, and the ordinary decoded/bytes/no-content/stream
surfaces remain unchanged.

Raw execution reuses the one production request and private `EndpointSpec` execution bridge. It
preserves before-send hooks, deferred body materialization, credential preflight and finalization,
limiter acquisition and observation, native-only authentication, managed Reqwest execution,
response debug and response-head hooks, at most one authentication recovery, hidden native retry
containment, operation deadlines, cancellation, request counts, and completion outside the fixed
result deadline. After ordinary successful-status classification it applies the ordinary buffered
response limit, rejects known oversize before polling, collects complete bounded bytes, skips only
the endpoint decoder and decoder-owned media validation, and emits `CompletionOutcome::Success`.
Non-success responses remain ordinary typed `HttpStatus` errors with the existing finite status-body
contract.

## Dangerous feature boundary

| Core feature graph | RawResponse / generated raw terminal | ReqwestRetryBuilder | Development support |
|---|---:|---:|---:|
| no dangerous feature | absent | absent | absent |
| `dangerous-raw-response` only | present | absent | absent |
| `dangerous-custom-retry` only | absent | present | absent |
| `development` only | absent | absent | present |
| both stable dangerous features | present | present | absent |
| all features | present | present | present |

No dangerous feature is enabled by default. Feature availability is a binary-wide compiled-crate
capability subject to normal Cargo feature unification. Compile-pass and compile-fail fixtures prove
genuine method/type presence and absence; there is no always-visible runtime fallback.

## Release-stable naming

The pre-edit inventory is `git_diff/release-naming-inventory.md`; the final map is
`git_diff/phase-10-release-naming-map.md`. Five integration suites were renamed:

- `phase5_generated.rs` → `generated_request_shaping.rs`
- `phase6_generated.rs` → `generated_authentication.rs`
- `phase7_generated.rs` → `generated_rate_limiting.rs`
- `phase8_generated.rs` → `generated_pagination.rs`
- `phase9_prepared.rs` → `generated_prepared_endpoints.rs`

Nineteen Trybuild fixture pairs (`.rs` and `.stderr`) were renamed:

- `phase6_credential_ref_mutation_absent` → `authentication_credential_ref_mutation_absent`
- `phase6_custom_set_absent` → `authentication_custom_set_absent`
- `phase7_rate_clause_order` → `rate_limit_clause_order`
- `phase7_rate_display_required` → `rate_limit_display_required`
- `phase7_rate_invalid_cost` → `rate_limit_invalid_cost`
- `phase7_rate_off_exclusive` → `rate_limit_off_exclusive`
- `phase7_rate_optional_dimension` → `rate_limit_optional_dimension`
- `phase7_rate_secret_dimension` → `rate_limit_secret_dimension`
- `phase7_rate_undefined_attachment` → `rate_limit_undefined_attachment`
- `phase8_pagination_bounds` → `pagination_bounds`
- `phase8_pagination_method_absence` → `pagination_method_absence`
- `phase8_pagination_modes` → `pagination_modes`
- `phase8_pagination_syntax` → `pagination_syntax`
- `phase8_pagination_types` → `pagination_types`
- `phase9_prepared_builder_methods_absent` → `prepared_endpoint_builder_methods_absent`
- `phase9_prepared_fields_private` → `prepared_endpoint_fields_private`
- `phase9_prepared_no_prelude_export` → `prepared_endpoint_no_prelude_export`
- `phase9_prepared_pagination_absent` → `prepared_endpoint_pagination_absent`
- `phase9_prepared_stream_response_absent` → `prepared_endpoint_stream_response_absent`

Parser, resolver, UI, and generated-runtime test identifiers plus executable sentinels, URLs,
hosts, media types, temporary filenames, hook/codec errors, allow reasons, and implementation
comments were normalized to behavior terminology. Cargo and Nextest discovered all renamed suites;
36 Trybuild pairs remain coherent. The naming audit scanned 340 tracked executable files and found
no prohibited Phase 5-10 chronology marker. Historical reports, evidence IDs and rows, completed
ledger entries, decision history, and commit history were not renamed.

## Canonical examples

The private `concord_examples` package contains ten compile-checked targets plus a deterministic
construction test:

| Target | Required workflows |
|---|---|
| `src/main.rs` | Minimal generated client and explicit generated endpoint. |
| `examples/full_dsl_reference.rs` | Every completed DSL grammar category. |
| `examples/body_workflows.rs` | JSON, text, custom codec, request stream, response stream, multipart upload. |
| `examples/authentication.rs` | Static, OAuth2, endpoint-backed, custom provider, and provider HTTP authentication. |
| `examples/rate_limiting.rs` | Built-in/custom limiting and `Retry-After` observation. |
| `examples/pagination.rs` | Offset, page-number, cursor, and custom pagination. |
| `examples/prepared_endpoint.rs` | Stable prepared endpoints without prepared pagination or raw response. |
| `examples/dangerous_raw_response.rs` | Feature-gated bounded raw successful response. |
| `examples/dangerous_custom_retry.rs` | Feature-gated custom Reqwest retry configuration. |
| `examples/regional_api.rs` | Large Riot-style regional API fixture. |

All examples compile under their exact feature graphs, use no real credentials or private
integration, and perform no live external request in deterministic tests.

## Performance package

The private unpublished `concord_perf` package uses one stable harness-free target,
`perf/benches/concord_overhead.rs`, with `std::hint::black_box`. It covers exactly nine local
categories: core client construction; generated client/facade/request creation; public head
preparation; codec encode/decode; body-limit polling; credential ready-path preparation; limiter
ready-path acquisition; pagination loop overhead; and stream wrapping. Compilation is mandatory;
the maintained smoke execution is bounded to one local iteration per category. No threshold is
claimed and no production runtime dependency was added.

## Release registries and audits

- Grammar: 68 completed productions in `concord_macros/tests/release/grammar_registry.csv`, each
  bound to positive parser, semantic/model, generated compile, and invalid diagnostic owners where
  applicable; 64 grammar/UI test roles passed.
- Reqwest: 18 selected Reqwest 0.13.4 assumptions in
  `concord_core/tests/release/reqwest_assumptions.csv`, covering counts, retry defaults and hidden
  containment, replay/factory invocation, timeout, logical/native URLs, query authentication,
  TLS, HTTP/2, proxy behavior, streaming, head access, and body ownership; 25 registry/contract/raw
  probes plus five focused production-boundary probes passed.
- Security: 15 sentinel classes in `concord_core/tests/release/security_sentinels.csv`, covering
  credentials, authentication values, request/multipart and success/status/raw bodies, native URLs,
  cursors/identities, rate keys, provider values, public sensitive headers, continuation values,
  controller state, and collected items across automatic formatting, sources, hooks, debug,
  completion, wrapper, and Rustdoc surfaces. Fourteen registry/integration tests and six focused
  source/property tests passed.
- Properties: five reproducible families in `concord_core/tests/release/properties.csv`: path
  segmentation, logical-URL auth exclusion, byte-limit first excess, credential-version
  monotonicity, and pagination identity cycles.
- Public/private and Rustdoc: rendered core and generated Rustdoc plus downstream public-only
  compilation prove the stable `prelude`, `advanced`, `dangerous`, generated modules/preludes, and
  extension traits while excluding all named private/native surfaces. Both focused Rustdoc tests
  passed.
- Artifacts: release audit found 10 example targets, nine benchmark categories, 68 grammar entries,
  18 Reqwest assumptions, 15 security classes, and five property families.

## Compatibility and MSRV

Reqwest remains exactly 0.13.4 with its default features disabled and private mandatory ownership.
Macro/runtime dependency separation, prohibited direct transport dependencies, explicit versions,
Cargo Deny license/source policy, and `publish = false` are retained. The feature matrix passed for
core default, all features, no default, no-default JSON, no-default multipart, each compression
feature independently, each dangerous feature independently, generated raw presence/absence,
macros, downstream generated fixture, downstream public extension fixture, examples, and benchmark
compilation.

The exact MSRV compiler was `rustc 1.85.0 (4d91de4e4 2025-02-17)`. It compiled:

- `concord_core --no-default-features`;
- `concord_core --all-features`;
- `concord_macros`;
- the downstream generated fixture;
- the downstream public extension fixture;
- canonical examples;
- the dangerous raw-response generated workflow.

## Exact changed executable paths

Production source changed only in the existing architecture:

- `concord_core/src/__private.rs`, `dangerous.rs`, `lib.rs`, `generated_contract.rs`;
- `concord_core/src/request/{endpoint.rs,head.rs,mod.rs,pending.rs,tests.rs}`;
- `concord_core/src/response/{metadata.rs,mod.rs}` and
  `concord_core/src/execute/{mod.rs,auth_tests.rs,tests.rs}`;
- behavior-naming-only edits in `concord_core/src/auth/{material.rs,oauth2.rs,slot.rs}`,
  `body/prepared.rs`, `error/{context.rs,status.rs}`, `hooks/{events.rs,mod.rs}`,
  `http/{config.rs,proxy.rs}`, and `rate_limit/limiter.rs`;
- `concord_macros/src/{codegen.rs,lib.rs,parse.rs,resolve.rs}`.

Tests and fixtures changed in:

- `concord_core/tests/{authentication_api.rs,body_extensions.rs,dangerous_feature_ui.rs,dangerous_features.rs,generated_contract.rs,pagination_rate_limit.rs,public_rustdoc.rs,raw_response.rs,release_registries.rs}`;
- `concord_core/tests/release/{properties.csv,reqwest_assumptions.csv,security_sentinels.csv}`;
- the dangerous raw/retry UI fixtures under `concord_core/tests/ui/fail/`;
- `concord_macros/tests/{compile.rs,grammar_registry.rs,ui.rs}` and
  `concord_macros/tests/release/grammar_registry.csv`;
- the five renamed generated integration suites and 19 renamed Trybuild pairs listed above;
- `concord_macros/tests/fixtures/contract_mismatch/incompatible_core/lib.rs` and
  `concord_macros/tests/fixtures/renamed_core/{Cargo.toml,src/lib.rs}`.

Examples, benchmarks, manifests, and release tooling changed in:

- `concord_examples/{Cargo.toml,src/lib.rs,src/main.rs,examples/*.rs,tests/canonical_examples.rs}`;
- `perf/{Cargo.toml,README.md,benches/concord_overhead.rs}`;
- `.gitignore`, `Cargo.toml`, `Cargo.lock`, `justfile`;
- `scripts/{audit-release-artifacts.ps1,audit-release-naming.ps1,audit-release-registry.ps1}`.

No `.agents`, `.codex`, controller-policy, remote, publication, deployment, or Phase 11 surface was
changed.

## Final validation

All commands below ran from `F:\projects\concord_api` against immutable source commit
`52ceef99e7cca1fac256a55d85296480817b199a`. Only documentation/evidence files were dirty during
the final rerun; `git diff 52ceef99... --` showed no executable difference.

Focused roles:

| Command | Result |
|---|---|
| `just _test-dangerous-features` | Pass; exact none/raw/retry/development/combined/all presence and absence. |
| `just _feature-matrix` | Pass; completed independent feature and downstream topology matrix. |
| `just _examples` | Pass; all examples compiled and deterministic example test 1/1 passed. |
| `just _benchmarks` | Pass; benchmark compiled and all nine one-iteration local categories ran. |
| `just _msrv` | Pass with exact Rust 1.85.0 across seven required configurations. |
| `just _grammar-and-diagnostics` | Pass; 64/64 selected grammar/UI tests. |
| `just _reqwest-assumptions` | Pass; 25/25 registry/contract/raw tests and 5/5 production-boundary probes. |
| `just _security-and-properties` | Pass; 14/14 registry/integration tests and 6/6 focused property/source tests. |
| `just _rustdoc-audits` | Pass; core 1/1 and generated 1/1 rendered-document audits. |
| `just _release-audits` | Pass; naming 340 files/36 Trybuild pairs; artifacts 10/9/68/18/15/5; source registry 504/1,531/35/0. |

Maintained matrix:

| Command | Result |
|---|---|
| `just fmt-check` | Pass. |
| `just check` | Pass. |
| `just check-all` | Pass. |
| `just check-no-default` | Pass. |
| `just clippy` | Pass with warnings denied. |
| `just clippy-all` | Pass with warnings denied. |
| `just test` | Pass, 535/535, zero skipped. |
| `just test-all` | Pass, 549/549, zero skipped. |
| `just test-no-default` | Pass, 1/1. |
| `just doctest` | Pass, two doctests. |
| `just doctest-all` | Pass, four doctests. |
| `just doc` | Pass with warnings denied. |
| `just doc-all` | Pass with warnings denied. |
| `just deny` | Pass advisories, bans, licenses, and sources; only the established allowed duplicate-version warnings were reported. |
| `just diff-check` | Pass. |
| `just release` | Pass, exit 0, 92.2 seconds in the final observed rerun. |

Tool versions were Rust/Cargo 1.85.0, cargo-nextest 0.9.140, just 1.46.0, and cargo-deny 0.20.2.
No applicable check was unavailable.

## Evidence and release state

Phase 10 adds 58 rows: 58 direct, zero partial, zero waived. Thirteen `P10-RAW-*` rows own the raw
contract, 23 `P10-REL-*` rows own exact Phase 10 release-final requirements, and 22 `P10-GOV-*`
rows close the remaining whole-registry governance owners. The catalog total is 1,531. A mechanical
append audit proves all 1,473 Phase 0-9 evidence rows remain byte-identical and in their original
order. Historical partial rows remain historical; they were not reclassified or rewritten.
The 15 owner-aware claim atoms left unresolved by Phase 9 now have direct passing Phase 10 owners;
all 595 atoms are proved and none remains unresolved.

The current coverage plan uses release-stable executable paths. The implementation program records
Phase 10 as terminal and complete. README and documentation indexes describe the local release
candidate and dangerous feature guarantees without claiming publication. `PROJECT_CONTROL.toml`
records the exact source identity, 36 direct requirement traces, 58 direct Phase 10 rows, all 504
requirements proved, all 595 owner-aware claim atoms proved, and the pending evidence/anchor
identities that the documentation-only anchor sequence finalizes.

## Engineering and architecture review

The public `RawResponse` and generated `RawResponseRequest<E>` are the exact mission-authorized new
abstractions. The already approved `dangerous-raw-response` feature now owns their positive surface;
no feature was added. The private harness-free benchmark package is dev-only and unpublished. No
new production dependency, unsafe block, state machine, transport, client, request/execution path,
native response escape, speculative extension point, test retry, weakened assertion, waiver, or
hard performance threshold was introduced.

One production `Request<E>`, one private `EndpointSpec` bridge, one prepared-head/body lifecycle,
one response classification/collection path, one managed application Reqwest client, and one
isolated provider client remain. Locks are not held across await; user code is not called under core
locks; no detached cleanup path was added. Checked finite limits, primary-error precedence,
cancellation, redaction, and completion semantics remain intact.

Explicit negatives confirmed by source, compile-fail, and release audits include: no prepared raw
response; no streaming raw response; no raw pagination; no raw non-success exposure; no status or
buffer-limit bypass; no decoder fallback; no native Reqwest value exposure; no prepared pagination;
no SSE, WebSocket, record/CSV streaming, bidirectional streaming, or multipart response parsing; no
publication metadata change; and no Phase 11 placeholder.

## Final state

- Normative status: `Normative Release Final` in all nine normative documents.
- Release result: `PHASE_10_COMPLETE`.
- Product result: `Concord 0.1 Release Candidate`.
- Normative requirements: 504 passing, zero waived, zero unresolved.
- Claim atoms: 595 proved, zero unresolved.
- Remote publication/tag/deployment: none.
- Next phase: none defined or started.
