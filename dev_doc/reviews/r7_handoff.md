# Concord V3 R7 handoff

Repository: [gqsnt/concord_api](https://github.com/gqsnt/concord_api).
Source branch: [concord-v3](https://github.com/gqsnt/concord_api/tree/concord-v3).
Implementation candidate: `e19c7bf0a096d77713c0c8b413f0a7f678080245`.
The library/tool/test trees and root lock retain that qualification. Post-push
CI follow-ups add fixture dependency fetches, pinned Rust source availability and
patched fixture Rustls; scoped native repeats qualify those changes.
Its exact pushed SHA, remote verification and CI snapshot are recorded in the
separate post-commit `push_receipt.json` delivered with the final source ZIP and
outer handoff. No later self-referential source commit is needed.
CI's all-features license check remains rejected for three BSD-3-Clause Brotli
dependencies. The existing allowlist is preserved for the owner's policy decision;
default-feature dependency audits pass. Exact scope is in validation.

The supplied R6 commit and parent were absent. Import commit
`16f61c0753b313b8c559a4a57dd3dfbe1d2e7345` was made on preserved real main
`3765907aa2e424997bfd8f2fcdcc95275e6142f5`. It exactly reproduces R6 tree
`0b1688a9f1e22898328d7d28eeedf87d7d44e5e1` and all 1,019 manifest entries.
Source digest: `b9a081afdb6276265d13aad7479f642cb1b9e4746efe873e60498b9bb13bba77`.
ZIP digest: `87aa33be9cca6744601e82733c18cecfd2f82af9c0635d237946b63e9bf8962b`.
[Import provenance](r7_import.json) records the 31 obsolete tracked V2 paths
removed in the isolated task worktree. No archive wrapper or Git directory
was imported. Primary checkout, local main and unrelated work were preserved.

| Finding | Disposition | Result |
|---|---|---|
| R7-01 | CONFIRMED_FIXED | Valid short credentials execute; actual expiry/rejection and admission races remain checked |
| R7-02 | CONFIRMED_FIXED | Qualified native path reproduced wrong binding; syntax relocation preserves authored types/values; simple direct-parent example REFUTED |
| R7-03 | CONFIRMED_FIXED | Every decoder completion/progress boundary applies lease/cancellation precheck |
| R7-04 | CONFIRMED_FIXED | Empty-plan feedback has a bounded opaque origin identity |
| R7-05 | CONFIRMED_FIXED | Capacity pressure retains one finite conservative barrier |
| R7-06 | CONFIRMED_FIXED | 1 MiB JSON payload and LF/CRLF framing agree across encoder/buffer/stream |
| R7-07 | CONFIRMED_FIXED | Duplicate effective snapshot index keys are refused before diffing |
| R7-08 | CONFIRMED_FIXED | Bounded, anchored rejection delays survive OAuth2 provider errors |
| R7-S01 | CONFIRMED_FIXED (narrow allocation scope) | One allocation/24 bytes removed per measured buffered execution; no latency/general-performance acceptance |

[Finding adjudication](r7_findings.md) and [validation](r7_validation.md) contain
exact probes, BEFORE/AFTER outcomes, controls, source identities, gate commands
and limitations. Adjacent repairs address Windows loopback socket mode,
large-contract debug stack use and the locked Rustls advisory. The production
diff was reviewed for ownership, lock/Drop placement, privacy, cancellation,
feature isolation, finite bounds and intentional test-oracle changes.

Intentional public beta changes: `RateLimitRequest` gains optional opaque
`cooldown_origin` (maintained context literals migrated); `CooldownOrigin`
exposes equality/ordering/hash but no host text; `ProviderResponse::retry_after()`
provides a bounded remaining hint. Default full record-frame cap becomes
1,048,578 bytes while NDJSON payload remains 1,048,576 and CSV's decoder ceiling
is unchanged. Inspection gains structural invalid/duplicate-key errors; these
checks do not imply complete schema validation.

Lifecycle remains `3.0.0-beta.1`, every package `publish = false`. The owner
authorized this ordinary source-branch push. Merge, license, package namespace,
tags, releases and publication remain owner decisions. The canonical lifecycle
record is [release-gate status](../release_gate_status.md).

Native Windows and Linux/WSL use pin 1.97.1 and MSRV 1.86.0; exact outcomes
are source-bound in validation and the evidence index. External Edition 1,
human studies and independent human review were not supplied or re-established.
The returned ZIP excludes Git/build outputs and includes final committed source,
per-file manifest, focused repair diff/commit list, native receipts and the
post-commit push/CI receipt. Historical failed/setup/interrupted runs stay distinct.
