# Phase 6 Integrated Conformance Report

## Verdict

`PHASE_6_COMPLETE_WITH_LIMITATIONS`

All Phase 6-owned authentication exit criteria are satisfied at the immutable source/test commit.
The remaining limitations are clauses of compound Concord 0.1 requirements whose unimplemented
behavior is explicitly owned by Phases 7-10. They do not weaken or defer any Phase 6 credential,
authentication, provider HTTP, OAuth2, or recovery behavior.

`PHASE_7_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting HEAD: `c7dfd11906d453de27702a6ff69f7f79a60e4dd9`
- Branch: `master`
- Initial working tree: clean
- Immutable Phase 5 source: `c513710c1e6ae2333a58ec481c2b74a4a0529a5f`
- Phase 5 evidence state: `afaa2fd091111e5fb9ca3a584ebb415735020907`
- Phase 5 anchor: `856ee0bfb6cbeae433c164e37056d0ffd32341e0`
- Immutable tested Phase 6 source/test commit:
  `e4c1cca94962cf0ee6855bab3c252c1caba19202`
- Post-source governance commit:
  `c7dfd11906d453de27702a6ff69f7f79a60e4dd9`
- Phase 6 evidence/report/state commit: `a4e4a004a6868f1d5dd462fef077cc948fd5109f`
- Phase 6 evidence anchor: the subsequent documentation-only
  `docs: anchor phase 6 evidence` commit; its object identity is intentionally external to its own
  content because a commit cannot contain its own hash
- Phase 6 source patch SHA-256:
  `52089f9ed3dda85e73aa1e665dab00d1866f78ba950aad1c5459bf20a5ba8c6e`
- Authoritative validation date: `2026-07-18`
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Nextest exercised: 0.9.140; repository minimum 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The post-source governance commit changes exactly `.agents/skills/concord-phase/SKILL.md`,
`.codex/config.toml`, `.codex/rules/concord.rules`, and `AGENTS.md`. It changes no product source,
test, manifest, lockfile, generated contract, fixture, validation recipe, or executable validation
configuration. The complete maintained and focused validation matrix was executed before that
governance-only commit against the immutable Phase 6 source/test commit. The tested tree identity
therefore remains authoritative and the matrix was not rerun at the governance HEAD.

The committed source patch was recreated with `git show --no-ext-diff --binary --format=fuller`
and is byte-identical to `git_diff/phase-6-unit-authentication.patch`. Both have the required
SHA-256 above. The ignored contract map contains 195 unique exact requirement IDs, and the
validation log identifies only the immutable tested source commit.

## Phase 6 commit and implementation boundary

| Unit | Commit | Subject | Durable result |
|---|---|---|---|
| Phase 6 source/test | `e4c1cca94962cf0ee6855bab3c252c1caba19202` | `feat: implement phase 6 authentication` | Added production credentials, typed public and generated authentication contracts, provider HTTP, OAuth2 client credentials, executor preflight/finalization/native materialization, and one bounded source-aware recovery on the existing request path. |
| Post-source governance | `c7dfd11906d453de27702a6ff69f7f79a60e4dd9` | `chore: harden agent commit policy` | Changed only repository-agent governance files after validation; no executable artifact changed. |
| Evidence/report/state | `a4e4a004a6868f1d5dd462fef077cc948fd5109f` | `docs: record phase 6 conformance evidence` | Adds the report and actual evidence and reconciles coverage, planning, controller state, and current-state documentation. |
| Evidence anchor | external identity | `docs: anchor phase 6 evidence` | Replaces the pending evidence-state/time markers and corrects tokenization in the recorded `P6-AUDIT-003` command; changes no executable artifact, classification, evidence ID, or count. |

Phase 6 extends the one production compiler, generated client, request, prepared head, body,
executor, application HTTP client, and provider HTTP client already in the repository. It does not
add a parallel execution plan, request type, decoder path, native URL, public header map, transport
abstraction, or generated API tree.

The delivered Phase 6 boundary is:

- six credential-source categories: static API key, static bearer, Basic, built-in OAuth2 client
  credentials, endpoint-backed acquisition, and custom provider;
- four authentication placements: bearer, Basic, named header, and named query;
- typed protected materials, read-only references, mutable endpoint handles, providers, recovery
  modes, challenge policies, bounded provider HTTP, and secret-safe authentication errors;
- generated constructor/builder inputs in required-value, secret, then custom-provider declaration
  order, plus source-specific credential handles and endpoint acquisition terminals;
- production credential coordination with single-flight acquisition/refresh, exact-version
  challenge mutation, stale-completion exclusion, cancellation progress, cooldown caps, checked
  version arithmetic, and destruction outside locks;
- authentication preflight and finalization around the private no-op Phase 7 seam, with native-only
  materialization after safe request debug;
- at most one source-aware recovery, preserving operation identity and deadline and re-entering the
  complete attempt lifecycle;
- HTTPS-only finite provider HTTP and JSON-owned OAuth2 client-credentials behavior.

No real rate limiter, rate-limit DSL or generated configuration, pagination execution, prepared
public endpoint, positive dangerous raw-success API, or Phase 10 release surface is implemented.

## Authority and contract map

The normative owner chain was reviewed from
`docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md`, then the Product, DSL, Public API,
Generated API, Security and Limits, Execution, Feature and Compatibility, and Conformance
documents. The current implementation program, Phase 5 report and chain, accepted decisions,
authentication/core/macro architecture, repository guide, source commit, maintained recipes, and
validation artifacts were checked against those owners.

No material authority conflict was found. The ignored contract map is
`git_diff/phase-6-contract-map.md`. Its 195 unique requirement IDs are divided into 16 trace groups
and remain within the approved Phase 6 boundary. The only accepted lifecycle qualification is
`EXEC-RECOVER-006`: Phase 6 re-enters a private no-op limiter seam, while Phase 7 owns real limiter
authority and the observable second acquisition.

## Public and generated API surface

### Public authentication contracts

`concord_core::prelude` exposes only the ordinary protected materials needed by generated clients:
`SecretString`, `AccessToken`, `ApiKey`, and `BasicCredential`. `concord_core::advanced` exposes the
approved stable extension contracts: `CredentialMaterial`, `SecretCredential`,
`CredentialProvider`, `CredentialRequest`, `CredentialReason`, `RecoveryMode`, `AuthError`,
`CredentialRef`, `CredentialHandle`, `Auth`, `ChallengePolicy`, and the bounded provider HTTP
types.

All native Reqwest clients, requests, responses, native URLs, prepared authentication material,
credential slot state, versions, leases, waiters, and execution helpers remain private. Built-in
materials and `AuthError` have explicit redacted formatting and do not retain arbitrary provider
error source chains.

### Generated contracts

The macro/core generated-private contract is synchronized at version 7. Generated construction
accepts required public values first, protected secrets second, and custom providers third, each in
normative declaration order. Missing required secrets or providers returns the existing safe
missing-input error without retaining DSL names or values.

Generated client clones share credential slots; separately built clients do not. A generated
`credentials()` facade returns typed source-specific handles:

- static API-key and bearer handles expose complete-value replacement;
- Basic replacement accepts and replaces the username/password pair atomically;
- OAuth2 replacement accepts and replaces the client-id/client-secret pair atomically while
  retaining the declared token URL and optional scope;
- endpoint-backed handles expose `set`, `clear`, and synchronous `is_set`;
- custom-provider handles expose `clear` and synchronous `is_set`, but not direct material `set`;
- all handles expose only typed read-only `CredentialRef` values to advanced authentication plans.

Endpoint-backed sources use generated `acquire_as_*` consuming terminals. The ordinary endpoint is
executed through the existing request path and its decoded success is stored atomically; a failed
endpoint execution preserves the prior credential.

## Credential state and concurrency guarantees

One private production credential slot owns source selection, material identity, monotonic version,
ready/failure/rejected state, and single-flight coordination. Deterministic concurrency and bounded
systematic tests establish:

- one provider operation for a concurrent missing/expired/expiring cohort;
- cohort-consistent success or failure, with a later no-cooldown call able to retry;
- waiter cancellation does not cancel the shared leader;
- leader cancellation publishes progress so a waiter or later call can continue;
- manual set, clear, complete source replacement, and exact rejection supersede blocked older work;
- stale acquisition, refresh, finalization, or challenge completion cannot restore an older value;
- exact recovery joins an in-flight refresh and cannot restore the challenged version;
- newer valid ready material wins over an older recovery decision;
- refresh skew, retry-after cooldown, and the configured cooldown cap remain finite;
- version overflow fails closed and discarded provider material is destroyed outside the slot lock;
- provider code, provider HTTP, hooks, body factories, and user destructors are never invoked while
  the credential-state lock is held.

`CredentialRef` is read-only. `CredentialHandle` mutation is limited to endpoint-owned storage.
Every source replacement is complete and atomic; no mutable partial Basic or OAuth2 input is
observable.

## Request lifecycle and recovery

The one executor now follows this Phase 6 ordering for each Concord attempt:

```text
public head and body metadata
→ before-send hook
→ body materialization
→ complete credential preflight
→ private limiter-acquisition seam
→ credential finalization
→ auth-free native request construction
→ safe request debug
→ private native authentication materialization
→ managed Reqwest execution
→ response-head debug and hook
→ private limiter-observation seam
→ challenge classification or ordinary status/body/decode
→ completion
```

Preparation failures call no body factory, provider, hook, limiter seam, or native execution as
specified. A body failure occurs before provider work. Complete credential preflight preserves
earlier successful acquisition if a later placement fails. Repeated placements of one credential
reuse the attempt-local result. Finalization rechecks selection/version/expiry after the limiter
seam, so cleared, replaced, rejected, or expired material cannot be sent stale.

The logical URL and public headers never contain authentication. Query and header material is
inserted only into the private native request after request debug. Invalid native authentication
encoding fails after safe debug and before execution. Authenticated cleartext application URLs fail
at public preparation before hooks, bodies, providers, or send.

At most one challenge-enabled placement is permitted. A recoverable first challenge is left unread,
attributed to exactly one source/version, and may produce one second Concord attempt. Recovery
preserves the same operation ID and original deadline, reconstructs a body before provider refresh,
and repeats hook, body, credential preflight, limiter seam, finalization, native construction,
debug, authentication, and execution. Reusable bytes are reused; reconstructible streams and
multipart bodies invoke their factory once per Concord attempt; one-shot bodies suppress refresh
and resend. A second challenge is terminal. Hook veto or reconstruction failure remains primary and
still performs the exact required challenge mutation.

Static, endpoint-backed, custom-provider, and OAuth2 recovery modes remain source-aware. Default
static and endpoint-backed placements use `Never`; built-in OAuth2 bearer defaults to
`Unauthorized`; custom providers require an explicit challenge suffix.

## Provider HTTP and OAuth2

Provider HTTP uses the existing independently constructed provider Reqwest client and deterministic
provider channel. It does not use application hooks, debug, identity, limiter seam, endpoint codecs,
or application custom retry. Application TLS identity is never installed on the provider client;
configured roots apply to both clients.

`ProviderRequest` is finite and uses a bounded empty/bytes/factory body. The URL is checked for
HTTPS and TLS availability before a provider body factory can run. A factory runs exactly once per
provider HTTP operation, and request-size validation completes before native execution. Provider
request timeout is bounded by the earlier of its own finite timeout and the remaining parent
operation deadline. Provider responses expose actual status and headers to the provider only and
collect no more than the finite effective response limit. Non-success status is not reclassified by
Concord; the provider owns interpretation.

With `json`, the built-in OAuth2 client-credentials provider performs an HTTPS POST using
OAuth-compatible Basic client authentication and form-encoded `grant_type=client_credentials` plus
optional scope. Basic components are form-encoded before Base64. Successful JSON requires a
non-empty `access_token`, accepts absent or case-insensitive bearer `token_type`, and accepts only a
checked non-negative integer `expires_in`. Refresh reacquires through the client-credentials grant;
no refresh-token grant is invented. Non-success, malformed, missing, unsafe, expired, and oversized
responses reduce to finite secret-safe authentication errors. OAuth2 runtime is absent without JSON;
protected use without TLS fails closed.

## Security, redaction, limits, cancellation, features, and dependencies

- High-entropy sentinel tests recursively inspect `Display`, `Debug`, error sources, hooks, logical
  metadata, generated Rustdoc, provider/application channel isolation, and request debug.
- Secret values enter only controlled native-capture comparisons or documented extension calls.
- Logical/safe URLs exclude query authentication; native URLs remain private.
- Public headers and native authentication headers remain separate maps with preparation-time
  collision checks.
- Request, response, status-body, stream, multipart, provider request/response, cooldown, deadline,
  and version arithmetic retain finite bounds and checked arithmetic.
- Cancellation drops futures and prevents later request phases; no detached credential cleanup task
  is spawned.
- No new runtime dependency was added. Reqwest remains private and mandatory with default features
  disabled.
- OAuth2 is owned by `json`; TLS remains independently selectable; the no-default and JSON-only
  boundaries compile; dangerous features remain isolated.
- `unsafe` remains forbidden in both production crates.
- Default, all-feature, no-default, JSON-only, minimal JSON+TLS, downstream/renamed dependency,
  Trybuild UI, Rustdoc, doctest, dependency-policy, and dangerous-feature configurations all passed
  their recorded checks.

## Exact focused validation

Every command in this section was recorded against
`e4c1cca94962cf0ee6855bab3c252c1caba19202` in
`git_diff/phase-6-validation-log.md`.

| ID | Exact command and configuration | Exit/result |
|---|---|---|
| F1 | `cargo nextest run -p concord_core --all-features --lib -E 'test(/^auth::/) | test(/^execute::auth_tests::/) | test(/^http::provider::tests::/)'` — core library, all features, default Nextest profile, retries disabled | 0; 103/103 passed, 188 skipped |
| F2 | `cargo nextest run -p concord_core --no-default-features --lib -E 'test(/^auth::/) | test(/^http::provider::tests::/)'` — core library, no defaults | 0; 76/76 passed, 165 skipped |
| F3 | `cargo nextest run -p concord_macros --lib -E 'test(/phase6_/)'` — macro parser/resolver/codegen semantic units | 0; 8/8 passed, 14 skipped |
| F4 | `cargo nextest run -p concord_macros --test phase6_generated` — generated downstream runtime | 0; 3/3 passed |
| F5 | `cargo nextest run -p concord_core --test authentication_api --test ui` — exact public API plus privacy UI | 0; 3/3 passed |
| F6 | `cargo nextest run -p concord_macros --test ui --test compile` — Trybuild, feature topology, downstream, and Rustdoc fixtures | 0; 27/27 passed |
| F7 | `cargo check -p concord_core --all-targets --no-default-features --features json` — JSON only | 0; passed |
| F8 | `cargo nextest run -p concord_core --no-default-features --features json --lib -E 'test(/^auth::oauth2::tests::/) | test(/^http::provider::tests::/)'` — JSON only | 0; 8 provider tests passed, 236 skipped; OAuth2 runtime correctly absent without TLS |
| F9 | `cargo nextest run -p concord_core --no-default-features --features json,default-tls --lib -E 'test(/^auth::oauth2::tests::/)'` — minimal OAuth2 runtime features | Initial sandbox spawn failed before execution with Windows error 1312; escalated retry exited 0 with 5/5 passed, 268 skipped |

## Complete maintained validation

| Exact command | Configuration | Exit/result |
|---|---|---|
| `just fmt-check` | `cargo fmt --all -- --check` | 0; passed |
| `just check` | workspace, all targets, defaults | 0; passed |
| `just check-all` | workspace, all targets, all features | 0; passed |
| `just check-no-default` | `concord_core`, all targets, no defaults | 0; passed |
| `just clippy` | workspace, all targets, defaults, `-D warnings` | 0; passed |
| `just clippy-all` | workspace, all targets, all features, `-D warnings` | 0; passed |
| `just test` | workspace defaults, default Nextest profile | 0; 435/435 passed |
| `just test-all` | workspace all features, CI Nextest profile | 0; 439/439 passed |
| `just test-no-default` | core no defaults, maintained typed HTTP smoke | 0; 1/1 passed, 240 skipped |
| `just doctest` | workspace default-feature doctests | 0; two compile-fail doctests passed; other crates had zero |
| `just doctest-all` | workspace all-feature doctests | 0; two compile-fail doctests passed; other crates had zero |
| `just doc` | each workspace library, defaults, Rustdoc `-D warnings` | 0; passed |
| `just doc-all` | each workspace library, all features, Rustdoc `-D warnings` | 0; passed |
| `just deny` | advisories, bans, licenses, sources | 0; passed with known duplicate-version warnings only |
| `just diff-check` | maintained whitespace/diff recipe | 0; passed |
| `just release` | complete repository-owned local release recipe | 0; passed in 73.1 seconds |

The release recipe repeated the 435/435 default and 439/439 all-feature suites, the 1/1
no-default smoke, and independent dangerous-feature suites: default 2/2,
`dangerous-custom-retry` 3/3, `dangerous-dev-tools` 2/2, and `dangerous-raw-response` 2/2. The only
dependency-policy output was the known duplicate-version warnings for `core-foundation` and
`windows-sys`.

## Requirement-to-source-to-test-to-command-to-evidence trace

Each trace group names the complete requirement inventory from the contract map, production or
generated owner, exact observable test owner, executed command IDs above, and actual evidence IDs.
Individual catalog rows narrow each claim to the requirement summary and direct/partial boundary;
the shared command does not broaden a row beyond its exact clause.

| Trace | Requirements | Source/generated owner | Test owner and command | Evidence |
|---|---|---|---|---|
| T1 DSL credentials/auth | `DSL-CRED-001`-`005`; `DSL-AUTH-001`-`006` | `concord_macros/src/{ast,parse,model,resolve}.rs` | macro Phase 6 units and UI/compile; F3, F6 | `P6-DSL-001`-`011` |
| T2 public auth/provider API | `API-CREDMAT-001`; `API-HANDLE-001`-`003`; `API-AUTH-001`-`003`; `API-PHTTP-001`-`004` | `concord_core/src/auth/*`, `http/provider.rs`, `advanced.rs`, `prelude.rs` | public API/UI and provider runtime; F1, F2, F5 | `P6-API-001`-`011` |
| T3 generated credential/auth | `GEN-CRED-001`-`004`; `GEN-AUTHFLOW-001`-`002` | `concord_macros/src/codegen.rs`, core generated-private bridge | generated runtime, UI/compile/Rustdoc; F4, F6 | `P6-GEN-001`-`006` |
| T4 credential bounds | `BOUND-AUTH-001`-`005` | `concord_core/src/auth/slot.rs` and reduced auth model | deterministic slot/model suites; F1, F2 | `P6-BOUND-001`-`005` |
| T5 auth lifecycle | `EXEC-AUTH-001`-`006`, `008`-`013` | auth placement, prepared head, executor, application HTTP | executor auth suite; F1 | `P6-EXEC-001`-`012` |
| T6 recovery | `EXEC-RECOVER-001`-`009` | slot/placement, body reconstruction, executor attempt loop | executor recovery/body suite; F1 | `P6-EXEC-013`-`021` |
| T7 provider execution | `EXEC-PHTTP-001`-`005` | provider HTTP and OAuth2 | provider/OAuth2 suites; F1, F2, F8, F9 | `P6-EXEC-022`-`026` |
| T8 auth conformance | `CONF-AUTH-001`-`002` | production slot plus reduced systematic model | deterministic and model checks; F1, F2 | `P6-CONF-001`-`002` |
| T9 product guarantees | 12 exact T9 IDs in the contract map | one client/request/executor, auth state, logical/native split | executor, generated state, provider tests; F1, F4 | `P6-PROD-001`-`012` |
| T10 DSL cross-cuts | 13 exact T10 IDs in the contract map | normalized parser/resolver/name pipeline | semantic units and compiler diagnostics; F3, F6 | `P6-DSL-012`-`024` |
| T11 public cross-cuts | 9 exact T11 IDs in the contract map | stable namespaces and private native boundaries | public API/UI, docs, release; F5, F6, `just doc-all`, `just release` | `P6-API-012`-`020` |
| T12 generated cross-cuts | 13 exact T12 IDs in the contract map | one generator and generated-private contract v7 | generated runtime, UI/compile/Rustdoc; F4, F6 | `P6-GEN-007`-`019` |
| T13 security/body | 20 `SEC-*` and 5 `BOUND-BODY-*` exact T13 IDs | redacted auth/material/provider/executor/body boundaries | sentinel, recovery-body, provider/public UI; F1, F2, F5 | `P6-SEC-001`-`020`; `P6-BOUND-006`-`010` |
| T14 execution cross-cuts | 25 exact T14 IDs | canonical executor and managed HTTP/body path | lifecycle/recovery tests; F1 plus maintained matrix | `P6-EXEC-027`-`051` |
| T15 compatibility/features | 11 `COMPAT-*` and 7 `FEAT-*` exact T15 IDs | existing manifests/features/private Reqwest topology | F2, F6-F9 and maintained release | `P6-COMPAT-001`-`011`; `P6-FEAT-001`-`007` |
| T16 conformance cross-cuts | 19 exact T16 IDs | maintained parser/compiler/runtime/security/docs/evidence architecture | F1-F9, complete maintained matrix, and state audits | `P6-CONF-003`-`018`; `P6-AUDIT-003`-`005` |

## Evidence, coverage, and controller state

Phase 6 traces all 195 owned requirements through 192 source/test-based evidence rows (146 direct,
46 partial) and three requirement-trace state-audit rows (two direct, one partial). Two additional
direct state-audit rows prove catalog identity and byte preservation of historical evidence. The
owned trace is therefore 148 direct and 47 partial; the Phase 6 evidence total is 197 rows: 150
direct and 47 partial. The complete catalog has 968 unique evidence IDs: all 771 Phase 0-5 rows
followed byte-for-byte by the 197 Phase 6 rows.

The coverage plan retains exactly one row for each of 504 normative requirement IDs. All 195
Phase 6-owned rows name current production/test paths, approved evidence labels, actual Phase 6
evidence IDs, the immutable tested source, the exact maintained or focused command, and direct or
later-phase partial ownership. Planned rows remain structurally distinct from passing actual
evidence.

The controller index records Phase 6 as the latest validated phase with this verdict, the immutable
source identity, the Phase 6 report, and the anchored evidence-state identity. Phase 7 — Rate
Limiting is the next active planning entry and remains `NOT_STARTED`.

The first anchor audit attempt passed `P6-AUDIT-001` and `P6-AUDIT-002`, then exposed a missing
PowerShell token separator in the newly recorded `P6-AUDIT-003` one-line command. The anchor
corrects that documentation-only command syntax. The corrected `P6-AUDIT-003` and all five final
state-audit commands passed against the immutable evidence/state commit; the catalog records their
observed anchor-audit time.

## Remaining limitations and later-phase owners

The 46 partial source/test rows and one partial evidence-state audit row are the complete 47 known
later-phase limitations within the Phase 6 inventory.

| Later owner | Exact partial requirements | Remaining clause |
|---|---|---|
| Phase 7 | `EXEC-RECOVER-006`; `DSL-ORDER-001`; `DSL-SCOPE-001`; `GEN-BUILD-001`; `GEN-OWN-002`; `EXEC-BUILD-002` | Real limiter authority, configuration ordering/selection/ownership, and observable acquisition on each recovery attempt. The Phase 6 no-op seam is not limiter evidence. |
| Phases 7-8 | `PROD-GUAR-002`; `PROD-GUAR-008`; `PROD-GUAR-010`; `DSL-END-004`; `DSL-NORM-001`; `GEN-DOC-002`; `SEC-FAIL-001`; `EXEC-ORDER-001`; `EXEC-DEADLINE-001`; `CONF-SEMA-001`; `CONF-ORDER-001` | Limiter and pagination diagnostics, preparation, fail-closed behavior, normalization, documentation exclusions, precedence, ordering, and deadline behavior. |
| Phases 7-9 | `DSL-NAME-002`; `GEN-NAME-001`; `GEN-NAME-002`; `EXEC-CONSTRUCT-001`; `EXEC-CREATE-001`; `EXEC-CREATE-002`; `EXEC-NATIVE-001`; `EXEC-NATIVE-002`; `EXEC-PREC-001`; `EXEC-CANCEL-001`; `EXEC-COUNT-001`; `EXEC-COUNT-002`; `CONF-DOWNSTREAM-001`; `CONF-DOWNSTREAM-002` | Limiter, pagination, and prepared-endpoint names, construction/laziness, native attempts, precedence, cancellation, counts, and downstream/extension certification. |
| Phases 7-10 | `DSL-FEAT-001`; `API-PRIVATE-001`; `API-PRIVATE-002`; `FEAT-DIAG-001`; `FEAT-DOC-001`; `CONF-PARSER-001`; `CONF-COMPILE-001`; `CONF-COMPILE-002`; `CONF-COMPILE-003`; `CONF-REQWEST-001`; `CONF-SEC-001`; `CONF-DOC-001` | Later feature syntax and diagnostics, final public/private audit, later parser/pass/fail/extension fixtures, Reqwest assumptions, security matrices, and compile-checked examples. |
| Phases 7, 8, and 10 | `SEC-TEST-001` | Limiter and pagination security cases plus the release-final security matrix. |
| Phase 10 | `COMPAT-BUILD-001`; `CONF-MSRV-001`; `CONF-TRACE-006` | Completed-product supported-build/MSRV proof and actual passing or waived evidence for every release-applicable normative requirement. |

The table's `CONF-TRACE-006` limitation is evidenced by `P6-AUDIT-005`: it proves exact Phase 6
artifact ownership and counts, while Phase 10 still owns release-final evidence completeness.

Phase 8 continues to own pagination execution and generated terminals. Phase 9 continues to own
prepared public endpoints and final stable-extension certification. Phase 10 continues to own the
positive dangerous raw successful-response surface and release-wide grammar, platform,
documentation, security, performance, and evidence closure.

## Engineering and architecture review

- No unexplained pre-existing work was overwritten; the starting tree was clean.
- No source correction was required.
- No source, test, manifest, lockfile, generated contract, fixture, recipe, or validation
  configuration changed after the tested source commit.
- The complete source patch and all 54 changed paths were reviewed, not only the implementation
  summary.
- The existing request/body/response/compiler/HTTP architecture was evolved in place.
- No lock is held across `await` or user/provider code in the production credential slot.
- No detached cleanup work, unsafe block, new dependency, public transport, speculative extension
  point, or silent error-to-success fallback was added.
- Error precedence, deadline ownership, cancellation, checked arithmetic, finite limits, and
  redaction remain explicit.
- All six credential sources, generated mutations, lifecycle phases, recovery branches, provider
  isolation, feature graphs, and later-phase absences have observable tests.

## Post-phase governance follow-up

Two policy refinements remain later governance work and did not block Phase 6 closure:

- permit read-only `git worktree list` while continuing to forbid every mutating worktree action;
- strengthen commit-rule matching so unsafe commit options remain forbidden regardless of argument
  order.

This mission does not modify `.agents/`, `.codex/`, or `AGENTS.md`.

`PHASE_6_COMPLETE_WITH_LIMITATIONS`

`PHASE_7_NOT_STARTED`
