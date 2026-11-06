# Concord SDK50 design and implementation mission — revision 2

Prepared 6 November 2026. This record reflects the user's latest instructions:
core/macros/ordinary usage are the principal product; SDK domains provide rich
evidence; client → scope → endpoint is fixed; challenge other abstractions to
their smallest sufficient form; integrate the current PR and control disk use.
It is an authority/architecture record, not completed implementation evidence.

## Baseline and current-PR integration

Reviewed source: gqsnt/concord_api PR #4, concord-dx-sdk20,
90b69011db92c8b900c23dd0c35176ad100d1204, tree
6a654331178f2124d96318637a75b2a854bc5a9e. Main was
a95fb2fb7548a719b0058da2032f59ecf4d069ea; PR #3 was already merged.

M0: verify current PR identity/head/main/drift and actual blockers. Fix in-scope
blockers, mark ready if needed, merge with preserved history and verify the
actual main integration locally and on Linux/native Windows/nightly. The current
SDK20 PR merge and necessary records-only receipt are explicitly authorized.
If already merged, verify its actual integration instead. Preserve all branches.
Create the new campaign branch from that verified integration or inspected
records-only successor; the earlier stacked-PR plan is superseded.

## Product scope and sequence

Keep SDK01–20 and implement SDK21–50 in six waves of five. Retain the five older
GitHub/Stripe/Shopify/Salesforce/Kubernetes controls: 55 primary cases. Review
relevant auxiliaries too. Preserve useful SDK20 results without replaying its
campaign; reopen design decisions when new evidence warrants it.

P0: reconcile current source and historical findings, complete the obligation/
residue inventory and remaining corrections. Separate semantic necessity from
current trait shape, generated code and package placement.

P1: perform compiled F1–F3 comparisons: request encoding/media/ownership;
response decoding/head/body/context/streaming; and declarations/generated Rust/
ordinary usage. For each use a simpler candidate, unlike witnesses, a simple
control, independent use/edit evidence and counterexamples. Implement selected
improvements and focused integrations. A reasoned retention is valid; a working
workaround alone does not close a design-cost finding.

P2: implement SDK21–50, with shared interfaces frozen for first attempts. Between
waves explicitly assess core/macros/usage, subtraction evidence and simple
controls, choose and implement improvements, migrate earlier consumers and
freeze again. Implement focused codec/pagination/auth/retry/ratelimit capabilities
with actual adoption. Keep the final five for first-pass validation after freeze.

P3: migrate all55 cases and affected examples to one candidate; verify required
behavior, compatibility, package/feature independence and platform gates; deliver
compiled integrations and a Concord retrospective organized around concepts,
responsibilities, improvements, costs and remaining limits.

## Design authority

The client → scope → endpoint semantic hierarchy is fixed. Present Encode,
Decode, Incremental, prepared/response types, policy hooks, generated names,
inheritance spelling and package placement can be challenged. Preserve essential
behavior through explicit contracts; do not protect all current shapes as givens.

Classify every proposal as core retention/change, macro, external capability,
SDK, application, omission or bounded experiment. Reuse does not automatically
justify a keyword. A single hard invariant can justify a narrow helper. Judge
the complete declaration/SDK/caller/extension/maintenance path: moving complexity
to each SDK is not removal. Supported changes remain compatible where feasible;
isolated breaking prototypes require concrete migration evidence and a separate
design decision, without silently breaking the supported candidate.

Keep concrete codecs and optional scheduling/retry/auth/admission engines in
focused capabilities unless a specific universal invariant requires a core
mechanism. Preserve one-shot ownership, operation identity, media/body agreement,
unread-head refusals, partial results and truthful completion. Separate byte
replayability, semantic permission and retry classification. Count operations,
pages, dispatches, token traffic and physical transmissions honestly.

## Evidence and resources

Use fresh author/consumer/maintainer/challenger roles with requested
Sol 6.1/Terra/Astra/Opus xhigh preferences and observed settings/fallbacks. Label
assistance; retain first outcomes. Daybreak capability availability is checked
in the execution environment; no blanket bypass or successful run is presumed.

Measure disk/RAM and projected growth. Default to at most two heavy jobs total,
including one full gate. Reuse bounded owned caches by active slots, not per SDK.
Automatic cleanup of positively identified inactive reproducible Concord target
output, including earlier campaign caches, is authorized. Preserve source,
worktrees, branches, history, fixtures and essential evidence. Checkpoint progress
and cache/job ownership so interruptions do not restart completed work.

## Completion and limits

Continue automatically through internal gates. Missing required code is
incomplete; unavailable external gates or insufficient safe capacity are explicit
blockers, never fabricated success. Historical gates and a green diagnostic job
do not prove the new exact candidate. Require actual nightly compatibility results.

Current SDK20 integration, local implementation/commits, candidate pushes and
draft PR updates are authorized. Do not automatically merge the later SDK50
candidate, tag, publish, force-push, rewrite published history, delete user/source
work, send external messages, or perform live provider writes/paid calls.

The extracted handoff's DESIGN_CHARTER, PROMPT, AUTHORITY and contracts contain
the full mission. Archive their inputs or hashes with the campaign records.
