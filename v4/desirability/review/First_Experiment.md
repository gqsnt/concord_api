# First experiment: truthful, discoverable public contracts

**Review proposal — 7 November 2026. No results have been measured.** This is a docs-only pilot specification, not execution authorization. Preserve existing authorization and read-only independent-review boundaries; it grants no push, merge, or publication. No human participants are required.

## Question

Can SDK callers identify and use the current public contract more reliably when generated documentation leads with purpose, required inputs, actual result/error, and decisive exceptions, with shared mechanics moved to linked detail?

This isolates **H04** on current signatures. It does not need to wait for H01–H03. Revalidate documentation after any later signature change; do not credit this experiment with fixing inference or result representation.

## Scope and comparator

Pin the actual source and existing docs. Baseline is the current public documentation plus its best documented navigation path. Candidate changes only documentation content, ordering, links and emitted doc attributes. Preserve public types, signatures, defaults, behavior, constructors and policy ownership. A generator edit must affect documentation only.

Use three witnesses and three controls:

| SDK | Contract to make discoverable or preserve |
|---|---|
| Cloudflare | Actual single/paged public results, provider-error ownership and retained metadata |
| Sentry | Base URL versus appended path, required scope values, and policy behavior |
| Kraken | Wire envelope versus public payload/result, exact inputs, and signing performed beyond ordinary preparation |
| Nager | Small ordinary call remains economical and accurate |
| Xero | Public tenant-bound construction and validation remain visible; do not advertise private generated controls as ordinary use |
| Strava | Auth/retry/rate responsibilities and endpoint-classification limits remain truthful; documentation does not create compiler enforcement |

Confirm each selected claim against pinned source before writing its answer key. If a historical complaint no longer exists, record that and select a current documented uncertainty within this scope; do not manufacture a deficit.

## Materials and tasks

Create four fixed tasks per SDK, 24 total:

1. Choose valid construction and predict destination/base/path for a stated operation.
2. Write one ordinary call with the stated inputs and use its result.
3. Identify actual public result and error ownership, including relevant metadata.
4. Explain the SDK-specific exception in the table and choose a permitted action.

Freeze questions, expected critical facts, fixtures, allowed aliases and scoring before candidate implementation. Use disjoint prompt examples. Both versions receive the same task, public types, permitted cheat sheet and equivalent documentation entry point. Readers see no internal source, tests, campaign records or other answers. Preserve first answers separately from any later recovery.

Use the same extraction method for both documentation versions. Text probes measure content and information hierarchy; they do not establish visual usability. Independently render both versions to check legibility, broken links, hidden content, and narrow-screen behavior.

## Finite sample and oracle

**Proposed design choices:** two available model providers, three fresh repetitions per SDK and version, four answers per session. This is **72 sessions and 288 answers**, equally divided between baseline and candidate. Interleave versions in randomized balanced order; pin settings and record observed metadata. With one provider, halve the sample and label the result single-provider and provisional.

Each answer passes only if all critical facts for that task are correct. Accept semantically equivalent type aliases; do not grade spelling. The runner compiles call answers against the pinned SDK and checks intended request/result behavior with independent fixtures. It must not use `prepare()` as the complete oracle for SDK signing or post-preparation policies.

Report each task, SDK and provider separately, plus paired corrections/regressions. No pooled percentage may conceal a wrong destination, false result/error promise, exposed tenant control or incorrect recovery rule. Count doc words and lookups only as secondary diagnostics.

## Proposed decision thresholds

These are pragmatic **pilot triage rules**, not established statistical significance or human-preference thresholds. Freeze them before reading candidate outcomes.

- **Gates:** no false public claim, behavior/API change, lost security boundary, invalid documented call, or weakened verification. Preserve Xero tenant checks and Kraken signing semantics. Independent review must validate this separation.
- **Win:** at least six net additional correct answers among the 72 candidate witness answers; neither provider has a negative net witness change; controls lose at most two answers among their 72 and no critical contract fact is newly misrepresented. Published documentation examples and invariant/control fixtures must pass. Reader-submitted call errors are failed task answers, not automatic fixture-gate failures.
- **Loss:** a gate failure, a repeated substantive regression attributable to the candidate, or no useful improvement coupled with increased caller obligations.
- **Inconclusive:** a smaller gain, provider disagreement, excessive variability, ceiling effects, or missing oracle/tooling. Record the uncertainty; retain the baseline unless a narrower deterministic correction independently justifies adoption.

A one-provider run cannot satisfy the two-provider win rule; it may identify concrete corrections and justify a later confirmation. A baseline ceiling is not permission to substitute word-count gains for task benefits.

A recurrent new critical-task failure in any SDK prevents a broad win. Investigate it as inconclusive; classify an attributable documentation defect as a loss. Aggregate gains cannot override this rule.

## Deliverable and next decision

Produce the pinned comparison manifest, old/new documentation captures, task/answer ledger, semantic call checks, independent review, and one verdict with costs and limitations. A promising candidate proceeds to broader affected-SDK documentation validation and existing required gates under applicable authorization. Structural input/result changes remain separate experiments.
