# Concord V3 R8 handoff

R8-01 through R8-04 are **CONFIRMED_FIXED** by native BEFORE/AFTER execution:
retain challenge-recovered preparation, retain rejected-head cooldown through
returned body errors, consume OAuth TTL from std monotonic send start and require
declared Bearer type. [Dispositions](r8_findings.md), [validation](r8_validation.md)
and [evidence index](r8_evidence_index.json) describe scenarios, controls and exits.

Qualified code candidate: `cef236aa2502df5653d77de2975051b5a6d49565`. The final documentation commit retains
exact compiled inputs. The return's `push_receipt.json` records final SHA/tree,
ordinary `concord-v3` push and matching remote ref after fetch/recheck; its CI
snapshot/job outputs identify the exact completed run. That receipt is outside
the source commit to avoid self-reference. Source manifest verifies every
exported byte against committed blobs, including Windows LF normalization.

Both native Windows and Linux/WSL pass the complete maintained implementation
gate with pin 1.97.1 and MSRV 1.86.0. Windows package/workflow/default deny and
both analyzers pass. All-feature deny exits 4 on both: only POL-01 license
policy remains owner-reserved. No functional local gate remains pending;
standalone Linux pwsh/actionlint absence is explicit in validation. Historical
R7 evidence is retained but does not qualify R8 inputs.

Public behavior is intentionally stricter for malformed OAuth responses:
missing/null/numeric/empty/unsupported token_type is refused; bearer/BeArEr and
unknown fields remain accepted, and expires_in remains optional. Product public
function signatures, versions, publication flags and dependencies are unchanged.
Only test support adds a bounded TLS wire fixture helper. Full production diff
was manually checked for prepared ownership, stale selection, lock/callback/Drop,
cancellation, clocks, error/journal semantics and public changes. See companion
`manual_review.md`; tests/Clippy are not asserted to be independent review.

`Concord_V3_R8_Return.zip` includes the exact maintained pushed-source export,
blob manifest, correction diff/commit list, handoff, all R8 BEFORE/AFTER/setup
logs, per-run receipts/overlays, final gates, source binding and external push/CI
evidence. Every indispensable R8 referenced log is included. Git, target,
caches, private environment values and credentials are excluded; fixed loopback
certificate/key fixtures remain ordinary committed non-secret test source.
Historical R7 raw logs are not re-exported in this ZIP; their source records
and original external R7 artifact directory are preserved. They are not evidence
for these new source gates.

Lifecycle remains `3.0.0-beta.1`, every package `publish = false`; the owner
authorized only source-branch correction push. Main began at
`3765907aa2e424997bfd8f2fcdcc95275e6142f5` and final receipt checks its state.
Other branches, primary/user work and existing R7 artifacts/checkouts are
preserved. No merge, tag/release, stable version, namespace/legal decision or
publication. Broader performance, external Edition 1 and human/stable gates
remain outside this correction; canonical status is [release-gate status](../release_gate_status.md).
