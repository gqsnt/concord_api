# Concord V3 direct corrections — 16 October 2026

## Scope and source

The owner requested direct repairs after applying the documentation diff from
[the source review](2026_10_16_review.md). The correction baseline is
`ae6006184f3dc903fdfe66c572098ac810e2c6c5`, whose production source matches the
reviewed `ff1be9cb588bcc664c366a55abe555590e7de01e`.
The owner subsequently selected **V3 as the only supported product version**.
[ADR 0027](../adrs/0027-v3-only-product.md) records the consequences.

The authorized private Edition 1 specification was read and its SHA-256
verified as `16fe616c5d9281660a9ed0809e9f957257236032f7ec27a4c8a96a0ad3c89d60`.
The authorized private closure prompt was also read, SHA-256
`b62bf4f85cb3817dc2eda398bc0a974939bde8f1deb94a1339a48fe5985ceadd`.
Their complete contents are not copied into the repository. The later owner
instruction supersedes conversion and older-binary continuity obligations.

## Findings and implemented corrections

| Finding | Disposition | Implementation and maintained evidence |
|---|---|---|
| V3R-01: foreign macro origin in conversion | **Closed by component retirement** | `migrate-v3` is removed; current CLI rejects the command, old target alias and old contract formats. No converter correctness claim remains. |
| V3R-02: provider-local timeout started after body construction | **Corrected; native regression passed** | `core/http/provider.rs` establishes the absolute local deadline before callbacks, intersects provider/parent authority, checks before/after callbacks and before native handoff; `concord/tests/provider_deadlines.rs`. |
| V3R-03: slow expiry callback could install late credentials | **Corrected; native regression passed** | `core/auth/slot.rs` checks acquisition authority across `expires_at()` and commit, publishes flight outcomes before releasing refused material outside the lock; slot and native provider regressions. |
| V3R-04: exact Content-Length could hide producer tail/EOF | **Corrected; native H1/H2 regressions passed** | `body/prepared.rs` retains the chunk reaching the exact boundary until observed producer EOF. Exact-zero streams remain observable. `http/application.rs` preserves managed framing; native request-length fixtures exercise both protocols. |
| V3R-05: multipart field names changed when parsed | **Corrected; native and independent parser evidence passed** | `body/multipart.rs` emits quoted `name` parameters with backslash/quote escaping and UTF-8, using the same header writer for length and emission. |
| V3R-06: empty multipart emitted no framing | **Corrected; direct/factory/replay regressions passed** | Zero-part recipes emit and count their closing delimiter; absent body and a present empty part remain distinct. |
| V3R-07: scope handle collided with a generated request type | **Corrected; semantic and native UI regression passed** | `macros/semantic/names.rs` registers the actual scope handle and reports both declaration locations as `C3-GENERATED`. A nested `credentials` scope remains legal. |
| V3R-08: private bindings captured caller constants | **Corrected; native runtime regression passed** | Synthesized storage and local bindings use the reserved `__concord_*` namespace; real aliases/constants, bound/unbound plans, defaults/reset, bodies/providers and pagination are exercised. |

The two reviewed documentation corrections are also applied. Retry classifier
input describes the latest attempt's dispatch facts; the executor separately
retains aggregate knowledge and applies replay safety. Empty-chunk bounds are
1,024 total for production/buffered/status/provider acquisition, and 1,024
consecutive for an acquired response stream, with a 32-per-poll cooperative
limit in both paths.

### Adjacent fixes established by the same reproductions

Cancellation during provider body/expiry callbacks now remains inside the
same authority checks. A cancelled provider leader relinquishes its flight so
an independent follower can acquire; neither late installation nor a later
native send is authorized. The executor preserves the `Cancelled` category
while linking provider evidence.

The constant-capture defect also affected direct internal names such as
`value`, `builder`, `sink` and `cursor`. The correction covers these bindings,
not only the initially reproduced value-field prefix. Mixed-site spans alone
were tested and found insufficient. Authored public methods, types, constants,
argument order and value transitions retain their meanings. Internal parameter
names are visible in some Rustdoc signatures; the documentation audit reflects
the emitted signatures.

The structured Rustdoc review additionally found ambiguous conversion prose
on non-String setters. It now says that borrowed strings are accepted only
when the target is `String`; numeric inputs still use exact identity.

## V3-only scope

The removal manifest contains 96 tracked paths: conversion implementation,
exclusive fixtures/tests, two obsolete guides, the former Catalog conversion
example and the old performance declaration. The original removal accounts
for 42,569 lines before V3 fixture relocation and other edits.

The public `RequestBody` and `ApiError::stage()` compatibility aliases and the
shared-store old-binary markers/import protocol are removed. The canonical
error accessor is `failure_stage()`; the distinct `RetryFailure::stage()`
calculation remains a native V3 operation. Native V3 extension roles such as
`PreparedBody`, `EncodeBody`, `DecodeBody`, `DisplayAtom` and API-author
`#[deprecated]` remain part of the selected V3 contract. Native compile-fail
fixtures explicitly reject `concord::body::RequestBody` and the retired
`ApiError::stage()` accessor.

The positive V3 member snapshot moves into the macro crate. The contract CLI
uses one `catalog` target for the canonical V3 Catalog. Current performance
tooling generates V3 only. Rejection diagnostics may explain a valid V3
spelling; they do not execute retired grammar.

Shared recordings now live in `fixtures/contracts/`. The Catalog rate-key
oracle is byte-identical to its preceding V3 recording. The Riot recording
changes only the example's `User-Agent` to `ConcordRiotExample/3`; all other
fields, keys, schemas and identities were compared structurally and preserved.
Its 46 endpoints are still evaluated under two origins (92 contexts), with
real limiter-sharing assertions and eight contract-edit controls. Provider
wire routes such as `/v2` are authored API data, independent of Concord's major
version.

234 tests previously executed solely for conversion disappear with that
component. This is a supported-scope reduction, not improved coverage. Current
V3 tests, positive member inventories and unsupported-surface tests remain.
Historical review and benchmark records keep their original source scope.

## Bounded performance corrections

Three avoidable costs were isolated and corrected without changing public
semantics:

1. The response acquisition wrapper duplicated an otherwise identical async
   future. Its body is now the direct `acquire_buffered` implementation.
2. Static contract construction composed each owner's query/header program
   twice. It now composes once, retains the sensitivity sets and moves the
   already-redacted operations into the effective owner/endpoint views.
3. Required-only endpoints emitted two unused generic `__with` helpers.
   These helpers are now emitted only when a value/body control needs them.

An independent code review found no lost redaction, provenance, ordering,
error classification or helper dependency. The earlier preservation of
composition lists remains a memory-measurement consideration. A controlled
100-endpoint hypothesis removes 200 methods and 200 declarations while
preserving description bytes. Exploratory timings overlapped other work and
are not a latency improvement claim. The final source-bound campaign records
actual runtime, allocation, layout and scale results separately.

## Focused evidence already obtained

These suites overlap and their counts must not be added as unique tests.
All results below are scoped to the recorded correction inputs, before the
final integrated gate.

| Area | Observed result |
|---|---|
| Provider regressions before correction | 8 demonstrated behavioral failures among 10 controls |
| Provider regressions after correction | 11 native tests passed; two additional core regressions maintained |
| Auth/provider retained suites | 262 auth-core, 23 provider-core, R8 OAuth 8, R7 OAuth 1 and compression 6 passed |
| V3 shared store | 8 store tests and 1 generated native adapter test passed |
| Request body units | 52 passed |
| Native exact-length paths | 7 maintained tests, including 18 core H1/H2 scenarios plus deadline/cancellation/retry/early-response controls |
| Native multipart paths | 3 passed |
| Independent multipart parser | aiohttp 3.14.4: original matrix 3/8 round trips before, 8/8 after |
| Current schema/oracle/example/downstream selection | 70 passed: core 36, examples 30, facade 1, downstream TLS 3 |
| Macro units/member snapshot | 44 passed before the final combined-tree changes |
| Native identifier hygiene | 3 passed; final gate also covers the added nested-scope positive assertion |
| Scoped native UI comparison | 5 fixtures passed without snapshot overwrite |
| Removed public alias | Native E0432 captured for `concord::body::RequestBody`; final UI matrix verifies the pinned snapshot |
| First integrated Clippy attempt | Detected one obsolete import and two needless test borrows; corrected, final matrix pending |

## Discoveries during integrated qualification

The first complete launch exposed a pre-existing invalid `justfile` setting:
`[windows]` cannot qualify a global `set shell` in the pinned just 1.46.0.
The file now uses `set windows-shell` with PowerShell `-NoProfile`. Native
controls verify parsing, shell selection and propagation of a failed command.

The first GitHub default test pass on `6085722` ran 1,624 tests, with 1,623
successes and one stale exact-length poll assertion. The local aggregate
reproduced that same failure. Three data chunks now require a fourth producer
poll to establish EOF before releasing the final exact-length chunk. The test
expects exactly four polls and retains its assertions for captured bytes,
Content-Type, Content-Length and completion. This updates the test to the
deliberately corrected body contract; it does not relax a failure into a range.

Fresh CI also found four RustSec advisories issued on 14 October 2026:
[RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html),
[0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html),
[0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html) and
[0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html).
All identify Rustls 0.23.46 as patched. Both tracked lockfiles now select that
version, and the workspace dependency minimum is 0.23.46 so downstream
consumers also receive the corrected floor. No other locked version changes
in this update. The selected Rustls features remain ring/std/tls12; exposure
conditions vary between the advisories, so a dependency finding is not evidence
that every described exploit is reachable through Concord's managed API.
The advisories check and the separately failing license check must not be
conflated. Final native/CI requalification follows these corrections.

The completed Linux attempt on `91c6cb9` passed all three workspace test
configurations and their doctests, then exposed a test-fixture assumption in
the isolated `concord --no-default-features` run. Uncredentialed identifier
and pagination fixtures now use HTTP. The credential-placement fixture keeps
HTTPS and executes only with `default-tls`, as required by Concord's existing
authentication policy; its generated declarations still compile without TLS.
No authentication transport rule was relaxed. Native isolated checks pass
without default features and with all features after this fixture correction.
The isolated core suite subsequently exposed the same HTTPS assumption in a
new expiry/Drop regression. Its fixture now builds a native HTTP capability
and initializes ring explicitly for the dev-only Reqwest feature unification;
Concord's TLS capability remains unchanged. The absolute parent deadline,
terminal publication, unlocked destruction and `(1, 0)` counters are retained.
All 700 isolated core unit tests and their doctests pass after this setup fix;
the extras package also passes its independent no-default test invocation.

Inspection of the CI MSRV log also established that the repository's pinned
1.97.1 toolchain overrode the job's installed default 1.86.0. The seven Cargo
commands in that job now explicitly select `+${{ env.RUST_MSRV }}` and record
the selected compiler. Earlier green jobs with the MSRV label are not counted
as genuine 1.86 evidence. The maintained `just msrv` recipe already selected
1.86.0 explicitly and remains part of both complete platform gates.

## Qualification and limits

The current [gate record](../release_gate_status.md) owns final qualification.
A complete Windows job now runs `just verify-implementation` and the analyzer;
its existence is not a passing Windows receipt. Linux uses the same maintained
aggregate. Dependencies remain locked, including the Rustls security update
above; already-locked `h2`/`http` dev-dependency entries and retired xtask
dependency entries also changed in the initial correction.

Synchronous user callbacks cannot be forcibly preempted. Deadline/cancellation
checks prevent subsequent unauthorized work once those callbacks return.
Exact request-body EOF is established before normal body completion; an early
server response does not imply the producer was drained. Early 413 is tested
in H1/H2. H2 coverage uses native prior knowledge, not a separate TLS/ALPN
matrix. Multipart interoperability is established for the actual parser
version tested, without a universal server claim.

The matrix retains 81 obligation IDs. O04 is retired by owner direction, and
80 remain applicable with the current V3 interpretation of R01/R02/R06/R13.
The V3 human task packets are prepared, with zero invented participant or
independent-human results. The structured implementer review is separate.
License work, registry publication and main integration are outside this
correction's implementation scope.
