# Concord V3 follow-up review — 17 October 2026

This is a new review of `concord-v3`, starting from the fully qualified commit
`570eec460dee1d83828ae48e35396ae07652aabc`. The owner requested another direct
correction pass, an open-source license, agent execution of the prepared user
and independent Rustdoc exercises, and Rust highlighting for Markdown examples.
V3 remains the only supported product version.

## Corrections and improvements

| Area | Finding and resulting behavior |
|---|---|
| Source license | Select `MIT OR Apache-2.0`; add both canonical texts at the root and in the four public crates, license all 17 package manifests, and inherit repository metadata in the workspace. Package verification checks both extracted texts and the SPDX expression. Dependency licenses keep their separate policy. |
| CSV decoding | A fresh per-record parser could strip U+FEFF from a later field, including turning an invalid integer into valid data. Consume one optional UTF-8 BOM only at the actual body start; preserve later characters, including across chunk boundaries. See the [runtime review](runtime_review.md). |
| Contract diff | A dependency-site change could name unchanged consumers elsewhere in the client. Resolve only added/removed site memberships for that field; retain full consumer coverage for other dependency changes. The actual Catalog exercise now names seven regional endpoints and excludes the unchanged root endpoint. |
| Markdown examples | All 49 existing `concord`/`concord-pseudocode` fences in 17 Markdown files use `rust`. Non-rendered classification comments preserve compiled DSL checks, source anchors, illustrative status and retired-syntax checks. |
| Generated Rustdoc | Endpoint method and request cards expose the request-body representation and its origin independently of the response representation and its origin. They identify inherited client/scope choices and endpoint-local `via` selections directly. |
| Observation documentation | Automatic events carry value-free structural target counts and typed response facts, not header maps. The event list includes `ProviderAcquisitionFinished`. `SanitizedHeaderMap` no longer promises that every unmarked public value is safe to log. |
| Exercise reproducibility | Concrete maintained declarations, diagnostics and isolated source edits accompany the task sheets. The results retain initial wrong predictions, corrections and actual native checks. |
| Analyzer integration | The new exercise target now declares its `json,ndjson` condition in source as well as its Cargo target. Rust Analyzer's default-feature scan no longer tries to import an unavailable `NdJson`; all eight exercises still execute with their required features. The Linux analyzer job explicitly installs `rust-src`, consistent with the toolchain file and Windows setup. |

## Requested exercises

- [Twelve user exercises](dx_exercises.md): an agent answered the task packet
  using public documentation and examples, then checked its predictions.
  Eight new public-path integration tests verify selected value, patch, codec,
  effect, traversal and stream behavior.
- [Ten independent Rustdoc/diff exercises](rustdoc_review.md): a fresh agent
  used public Markdown and generated pages before any implementation tour.
  The record distinguishes the initial missing diff handoff from the later
  real isolated source edit and native contract export/diff.
- [Runtime review](runtime_review.md): the confirmed CSV and inspector issues
  have old-code failure controls and corrected native tests; inspected areas
  without a new confirmed finding are named with their limits.

These are agent executions requested by the owner. They complete the requested
technical exercises; they are not observations of unfamiliar human developers.
Human participant counts remain zero, and empirical claims C63/C64 remain
unmeasured. The optional human-only protocols are retained without blocking
completion of the owner-directed agent evaluation.

## Native evidence and source scope

The local follow-up uses the pinned Rust 1.97.1 toolchain and locked
dependencies. Its receipts retain the actual command, native exit, log digest,
checked-out commit, tracked-diff digest and untracked source digests. Local
development checks do not describe a dirty checkout as a clean committed tree.
The [native validation index](evidence/validation/README.md) retains the actual
local receipts and logs. The final CI run identifies the pushed source commit
qualified across the maintained Windows/Linux matrix; a local success does
not pre-qualify that later run.

Observed focused results before the final integration check:

| Check | Native result |
|---|---|
| CSV old-code discriminant | All three new unit regressions and both new managed HTTP regressions failed with the old production decoder; exit 101. An earlier sandbox listener denial is retained separately and is not product evidence. |
| CSV correction | Six selected unit tests and eight CSV integration contracts pass; exit 0. |
| Inspector old-code discriminant | Both generated-contract regressions fail with the extra endpoint attribution; exit 101. |
| Inspector correction | All 19 unit and 18 integration tests pass. Two real CLI replays of the original Catalog snapshots name exactly seven regional endpoints, ten `Provenance` changes and no unresolved entries. The original incorrect output remains in the evidence. |
| Public user exercises | All eight tests pass with default features disabled and only `json,ndjson` enabled; exit 0. |
| Macro unit tests | 45 tests pass, including the new independent request/response provenance cases; exit 0. |
| Tool tests | 63 unit tests and six CLI integration tests pass, including the new Markdown classification controls; exit 0. |
| Generated documentation | Warning-denied workspace Rustdoc generates successfully; endpoint-card audits pass. The changed core and facade pages were separately refreshed; all five targeted independent documentation reread checks pass. |
| Endpoint effect exercise | A compiled Catalog edit changes `catalog.item.update` from idempotent to non-idempotent. The native diff identifies exactly that endpoint, `effect.value` and `RuntimePolicy`, with no unresolved change. |
| Integrated local quality | All-target, all-feature workspace Clippy with warnings denied, repository policy, formatting and the compiled-documentation check pass. |

The first pushed follow-up, `ed5ac62`, passed native CI suites of 1,629 default
and 1,721 all-feature tests, the explicit Rust 1.86 checks, documentation,
packaged downstream consumers, and all four cargo-deny categories. Its Linux
Rust Analyzer diagnostics job failed in [CI #35](https://github.com/gqsnt/concord_api/actions/runs/39030889270).
Local reproduction with the complete toolchain isolated one unresolved
`NdJson` import in the new feature-dependent exercise target. Mirroring the
target's required features in its source removes that diagnostic; the full
native analyzer scan then succeeds and all eight exercises execute again.
The [before/after record](evidence/validation/analyzer_feature_gate.json)
retains original receipts, log hashes and the exact diagnostic. CI #35 is not
relabeled as a fully successful run; the corrected commit has its own CI.

This review does not rerun or transfer the earlier performance campaign to
modified code. The older measurement reports keep their original source scope;
no new general throughput, latency or memory improvement is claimed.

## Release state

The project source license is implemented. The requested agent exercises and
independent documentation review are recorded here. The beta remains
`3.0.0-beta.1` with publication disabled. A main merge, registry namespace
confirmation, stable version/tag and registry publication are separate release
actions, as recorded in the [owner decisions](../../owner_release_decisions.md).
The [release-gate status](../../release_gate_status.md) remains the lifecycle
record; earlier native failures and their source identities remain historical.
