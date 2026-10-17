# Concord V3 — correction and machine qualification report

## Result and how to read this report

The implemented product is Concord V3, version **3.0.0-beta.1**. The correction
pass removes the unsupported older-version migration and compatibility
surfaces, fixes the demonstrated runtime/body/macro defects, strengthens
discriminating tests, and qualifies the selected architecture with native
Linux and Windows execution.

The measured product at 579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31 has a successful
complete Linux implementation gate, complete Windows implementation gate
and analyzer, paired runtime/scale measurements, and controlled attribution.
Later changes strengthen the inspection oracle, reject invalid support-CLI
options, add explicit body-arity UI oracles, correct a small public-doc wording
issue, and retain final evidence. Those exact changes are qualified separately.

**The generated FINAL_RETURN_RECEIPT.json owns the exact final commit, tree,
source-manifest digest, ZIP digest and final native CI result.** A source file
describing a workflow is not evidence that the workflow ran. A complete receipt
is written only after all required engineering outcomes, source checks,
frozen-manifest checks and archive verification succeed. Expected CLI nonzero
statuses and the separately scoped license-policy failure retain their actual
native outcomes.

This is engineering qualification of the selected beta product. Registry
publication, a stable release decision, a tag and a merge to main are separate
owner actions. No such publication or integration action is performed here.

## Source and integration boundaries

| Item | Identity or rule |
|---|---|
| Repository and active branch | gqsnt/concord_api, concord-v3 |
| Owner-pushed correction baseline | ae6006184f3dc903fdfe66c572098ac810e2c6c5 |
| Preserved main | 3765907aa2e424997bfd8f2fcdcc95275e6142f5 |
| Measured corrected product | 579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31 |
| Stronger inspection oracle | 2f647826dfefb61a6e5a6770f8af2c523198011e |
| Native UI capture driver | 2bba4e3797ece2f8254efd84ef31640605b65ff8 |
| Final commit, parent and tree | Generated from the real final checkout in the receipt/provenance |
| Integration method | Ordinary authorized source commits on concord-v3; no history rewrite |
| Version and package publication | 3.0.0-beta.1; publish remains disabled |
| Historical records | Two explicitly export-ignored files preserved byte-for-byte |
| Older Concord support | No converter, older-format import, compatibility alias or old-binary state import |

The final envelope contains every tracked source blob, the two preserved
history blobs, real Git patch-series output from the correction baseline,
native evidence archives and SHA-256 manifests. The maintained distributable
source-export remains a distinct artifact and excludes the historical files.

The private acceptance annex is delivered separately to its owner. Its private
authority hashes and detailed requirement mappings are absent from GitHub
source, logs and artifacts. The public package explicitly records this boundary;
a verified public ZIP does not claim to include that separate private annex.

## Corrected behavior

| Area | Defect or weak oracle | Resulting behavior and evidence |
|---|---|---|
| Provider deadlines | Local time allowance started after a synchronous body factory | The absolute local deadline is established before callbacks, intersected with parent authority and checked at subsequent handoffs. Slow callbacks cannot authorize later work after expiry. |
| Authentication publication | An expiry callback could finish after cancellation/deadline and still install material | Authority is rechecked across expiry and installation. A terminal flight result is published before retired material is destroyed outside the lock. Followers can reevaluate a retired leader. |
| Exact request bodies | Reaching the advertised byte count could hide trailing producer data | The final boundary chunk is retained until actual EOF. Zero-length producers, extra data, cancellation, retry and early-response behavior have explicit controls. |
| Multipart part names | Header formatting changed names after an independent parser read them | Quoted UTF-8 names use one shared counted/emitted escaped representation. |
| Empty multipart | Closing framing was absent for an empty body | The closing delimiter is counted and emitted; replay and real parser controls cover empty framing. |
| Generated scope/request names | Collision checks missed actual generated handles | Actual handles enter the collision map and diagnostics identify both originating spans. Reserved internal storage names and root credential rules are enforced. |
| Generated identifier hygiene | Caller constants could be captured by internal bindings | Reserved internal bindings and correct macro spans preserve caller expressions, constants, aliases and current default-reset semantics. |
| Contract CLI | Options belonging to another subcommand were silently accepted | Export/check reject --format; diff rejects --output/--snapshot before execution. Six binary negative controls keep source and sentinel output files unchanged. |
| Inspection mutation corpus | Three cases did not assert the complete affected endpoint set | Credential-source and default addition/change require exactly all five affected endpoint IDs. |
| Required body arguments | No explicit negative UI fixture for missing/extra required body arguments | Two native E0061 snapshots and two maintained-harness entries accompany the existing positive language fixture. |
| Public conversion wording | A conversion was described as preceding any request object | The contract is clarified to occur before execution, including setters on an already built inert request. Generated documentation is checked. |
| Portable qualification | Invalid just shell selection and implicit MSRV selection | Windows uses the supported PowerShell setting and native exit propagation; seven CI Cargo commands explicitly select Rust 1.86.0. |
| Isolated test fixtures | No-default fixture setup assumed TLS | Fixture transport and dev crypto setup match the tested feature configuration without weakening production transport policy. |
| Dependency security | The locked Rustls release had four newly identified advisories | Workspace minimum and affected lockfiles select Rustls 0.23.46; the later native advisory check passes. |
| Evidence tooling | Rustc phase names retained RSS text; ZIP checking could compare against a fresh filesystem instead of a frozen manifest | Native timing lines and values are preserved, phase names normalized with a real-format regression, and each archive member is verified against the frozen SHA-256 manifest. |

The original converter-specific origin finding is retired with the unsupported
component. It is not described as a repaired or supported converter.
[REVIEW_DISPOSITION.csv](REVIEW_DISPOSITION.csv) records each correction's
scope and whether its final integrated native receipt is required.

The retained tests are discriminating. For example, the EOF oracle now requires
exactly four polls for three chunks plus EOF, while retaining byte, media-type,
length and completion assertions. The multipart matrix used an independent
aiohttp 3.14.4 parser: 3/8 original round trips passed before correction and
8/8 afterward. Compiler failures are captured from the real pinned compiler,
not authored as guessed diagnostics.

## Strict V3 product scope

[V3_ONLY_REPORT.md](V3_ONLY_REPORT.md) and [ADR 0027](../../adrs/0027-v3-only-product.md)
define the current boundary. The migration command and implementation,
conversion-only tests/fixtures, old spelling alternatives, RequestBody alias,
ApiError::stage() compatibility accessor, catalog-v3 alias, contract formats
1/2 and old shared-store import protocol are removed or rejected.

Current V3 extension roles and semantics remain: EncodeBody, DecodeBody,
DisplayAtom, PreparedBody, explicit representation selection, request controls,
managed streaming, pagination, runtime endpoints, current shared-store
conformance and API-author deprecation. RetryFailure::stage() is a different
V3 API and remains. Provider paths such as /v2 belong to the remote service;
they do not create a Concord V2 compatibility mode.

The removal inventory's 234 converter-only tests are retired coverage for an
unsupported component, not an increase in test coverage. Historical worktrees
are used solely for measured reference comparisons. No compatibility work is
listed as a remaining requirement for the finished V3 product.

## Native qualification

[NATIVE_QUALIFICATION_REPORT.md](NATIVE_QUALIFICATION_REPORT.md) distinguishes
actual native exits, semantic outcomes and the source to which each applies.

| Evidence | Observed result |
|---|---|
| Dedicated Linux qualification, source 579e8eac | Full maintained implementation gate, analyzer and release-audit succeed |
| Windows CI #28, source 5b377223 | Full maintained implementation gate and analyzer succeed |
| Windows CI #29, measured source 579e8eac | Gate succeeds at 00:23:52Z, analyzer at 00:27:33Z, job ends successfully at 00:29:48Z on 17 October |
| Workspace suites on 2f647826 and 2bba4e37 | 1,624/1,624 default and 1,703/1,703 all-feature tests, zero skipped |
| Compiler contracts | 44 macro unit tests and the three maintained compiler-contract harnesses succeed on the observed candidate |
| Genuine MSRV | Native Rust 1.86.0 product checks and semantic UI succeed after explicit toolchain selection |
| Packaging/downstream | Package extraction, documentation and downstream use succeed for 3.0.0-beta.1 |
| Platform smokes | Linux and Windows clean-source exports plus four deterministic runtime controls succeed |
| Generated Rustdoc | Fourteen actual generated-page texts reviewed, page hashes and three clean Linux command receipts retained |
| Native UI capture | Missing/extra required body arguments emit E0061; second run without overwrite passes with stable inputs |
| Final integrated commit | Exact CI and targeted source/CLI/doc/envelope checks are adjudicated by the generated receipt |

The final return runs the actual contract binary for all four canonical targets,
including deterministic double export, self-checks, semantic differences,
format rejection and unsupported-alias rejection. The new CLI tests verify
that invalid options do not overwrite existing files. Compiler snapshots are
rerun by the maintained final harness without an overwrite environment.

The historical failure attempts remain evidence: the invalid just setting,
stale EOF oracle, two isolated TLS-fixture issues and the earlier falsely
labelled MSRV jobs are not relabelled as passing attempts. The disconnected
local Linux process has an UNKNOWN native exit. Separate GitHub Actions
results provide the replacement qualification.

The native CI dependency-policy command still reports three BSD-3-Clause
license rejections. Its advisories, bans and sources checks pass. License work
was excluded by the owner; no ignore or allowlist change was introduced to
alter that outcome. The combined CI can therefore remain red while its nine
engineering jobs succeed. The final collector verifies that this is the exact
remaining policy failure rather than accepting any arbitrary failed CI.

## Performance findings

[PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md) contains the full methods,
samples, residual costs and native attribution. CSV files retain 133 runtime
metrics, 19 scale points and 333 native compiler-phase rows.

The corrected V3 product has these measured changes against the owner-pushed
V3 baseline:

- Managed buffered execution saves exactly 528 allocated bytes per operation:
  28,402 to 27,874, with 63 allocations and the same 8,016-byte outer future.
- Each isolated first head plan saves 55 allocations and 9,466 allocated bytes.
  Peak live bytes increase by 288, so the result is lower allocation traffic,
  not a claim that all memory metrics decrease.
- At 1,000 endpoints, 2,000 unused generated helpers disappear. Median build
  time falls from 14,394 to 13,130 ms in the three paired before/final runs;
  local-edit median falls from 14,410 to 13,352 ms. Some other timing ranges
  overlap.
- Current serialized description size is unchanged before/after. The unstripped
  debug binary grows by 59,448 bytes; no binary-size improvement is claimed.

Compared with the isolated historical source, V3 retains a larger inline
execution future, more expensive first description construction and greater
type/borrow-checking work. Native layouts locate the outer 3,392-byte increase
in the retained V3 operation state, controls, journal and nested futures.
Controlled restoration of the removed response wrapper reproduces exactly
the separate 528-byte allocation cost. Nine real compiler profiles place the
remaining 1,000-endpoint compilation difference mainly in type checking and
MIR borrow checking, not macro expansion or LLVM.

The measurement scope is explicit: release-mode keep-alive loopback execution,
client-thread allocation counters, normal unstripped debug scale builds,
incremental disabled, same-runner comparisons within each mode, aligned
external dependencies and actual locked inputs. No remote provider latency,
global RSS, universal speedup or historical feature-parity claim follows.

## Remaining boundaries and owner actions

No supported V2 path, converter implementation or compatibility adapter remains
to be completed. The demonstrated defects and identified avoidable costs have
concrete corrections and maintained evidence.

The remaining boundaries are:

- The exact final receipt must exist and pass before claiming final integrated
  machine qualification. Prior successful source-qualified runs cannot prove
  a changed CLI or new test by themselves.
- Independent human workflow/usability exercises have no fabricated
  participants or outcomes. Their existing human-only claims remain Unknown.
  The generated-page review is a machine review with an explicit navigation
  and visual-inspection limit.
- Publishing a stable release, creating a tag, enabling registry publication
  or merging main requires the owner's separate release decision. The selected
  beta version and disabled publication state are retained.
- The private detailed acceptance annex is delivered separately. The public
  evidence ZIP is verified on its stated contents; a single archive containing
  that private annex is not claimed.
- Synchronous callbacks cannot be forcibly preempted; early responses do not
  establish producer drainage; native H2 coverage is prior knowledge, not a
  separate TLS/ALPN matrix; multipart interoperability is limited to actual
  tested parser versions and scenarios.

These limits define the evidence and release scope. They do not propose a
second product version or reopen removed compatibility requirements.
