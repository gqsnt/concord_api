# Reflection (P3.5)

P4.2 update (2026-11-09): the owner chose runtime lookup. The implementation removes
`operations Name;`/`SendOperation`, uses one `SendWith` trait and automatic private inventories,
keeps all eight exhaustive classifications, defers default continuation Send bounds, and tests
Datadog's production posting match with a seeded future write. The snapshot evidence and limits
below describe the earlier direction; see [current migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

## What the campaigns taught

**SDK20 and SDK50** proved the core holds: 55 SDKs, from a one-endpoint holiday API to Kubernetes,
fit client → scope → endpoint, and the declaration describes the request fully. They also built
the evidence this mission used: 55 friction notes, canary tests in every SDK, and 77k lines of SDK
code that show where the declaration stops.

**The desirability loop** tested the generated surface with model readers. Its lasting results
are repairs and tests (canaries and seven leak fixes, T2–T5, H08). Its weakness was the
measure: readers at ceiling could not see errors, redaction or facades, so the largest costs were
never tried, while docs grew 63% and a negation statement (`controls`) passed every gate.

**This mission** measured glue instead and built every idea against three real SDKs before any
code. Three things stand out:

- **Most single ideas remove less than their lanes estimated.** Target files and spikes cut the
  lane estimates by 3–10×: A1 was estimated at ≈980 lines and measured −51 glue alone. The reason
  is the same each time. Much of what the classifier calls glue is provider semantics that only
  looks like plumbing (status routing, retry-after parsing, page validation). An idea removes the
  part that restates a fact; the decisions stay.
- **Combinations remove more than their parts when one carries the other's missing piece.**
  The errors direction (A1 + A2) removed −173 glue on 4 SDKs against −68 for the parts, because
  the derive absorbed the per-SDK conversion that cancelled A1's gain. SDK types (D1 + I1) and
  operations and paging (F3 + E1 + F1) added up without a bonus: −333 glue and −605 lines, and
  −397 glue over 18 SDKs after its review fixes (F3 + E1 −253, then F1 −147 before the fixes),
  the largest cut of the phase. Construction
  (C1 + C2 + G1) closed all four invariant breaks but removed almost no glue.
- **Some ideas are worth it without glue.** H1 cut generated docs from 786 to 84 words per
  endpoint; J1 made "Concord logs itself" true and canary-tested; construction closed FM1–FM4. The
  gallery shows these on their own merits, not on a glue number.

## Counterexamples that define the boundaries

- **Status-routed errors** (Datadog: 401/403/400/429 each to a different variant) stay hand-written:
  routing by status is behavior, so it cannot move into an attribute without breaking rule C2.
- **A proc macro cannot name a field's type.** `page items <field>` (F2, F3) cannot generate the
  item type without the author restating it; this killed F2 and limits F3.
- **Rust coherence** stops a blanket `SendWith → SendOperation` adapter, so `operations Name;`
  (a statement about Concord, rule C1) cannot simply disappear; the owner chooses between keeping
  it and a runtime lookup.
- **reqwest's own logging** prints the host, including a sensitive subdomain, at DEBUG/TRACE.
  Concord can only promise what its own `concord` target emits.
- **Sensitivity in generated docs** depends on the resolved header name, which a macro cannot see
  for native constants; H1 withholds every native literal and still has one mixed-chain case open.
- **A second runtime origin** (Mastodon's streaming host) does not fit "the base is one value".
- **A generic start moves validation.** Once `Pages::starting_at` replaced each SDK's `*_from`
  twin, checks that ran only in a constructor (Mailgun's saved-cursor binding, Xero's tenant and
  page size, Strava's page zero) could be skipped; each page factory must check every start.

## Ideas that failed, and why

- **F2 declared cursor**: two lines saved for seven new concepts (the item type cannot be named).
- **G2 `from_url`**: grew glue on its own (adapters cost more than the matching saved).
- **K2 route facts**: six lines for about eight concepts.
- **N3 always-generated operation enum**: incoherent on stable Rust; generating the enum for every
  client also grew the simple case (rule C3), so it was reverted.
- At screening, about 30 of 82 merged ideas died on a named charter rule: most often C1 (a
  statement about Concord), C2 (behavior in the declaration), C6 (needs a paragraph) or "only moves
  glue" (a new trait every SDK implements with the same lines).

## Process lessons

- Independent review found a real defect in almost every spike and direction (unsized fields,
  hygiene collisions, a missing default dispatch, five truth problems in generated docs, a sensitive
  base leaking through a getter, a start-validation bypass, untested redaction markers). Two rounds
  fixed most blocking findings. Three stay recorded as limits for P4: H1's mixed-chain leak, F3's
  unconditional `Send` requirement, and the errors derive, where the round-1 hygiene fix itself
  introduced two regressions (captured `display(with = …)` hooks, a panic on unused raw
  identifiers). A fix round can regress; the second review is what caught it.
- Worker lanes moved from Claude to Codex mid-phase on the owner's instruction. Reviews of
  Codex-written work by a separate Codex session are independent sessions of the same provider,
  and are labelled that way.
- Sharing a cargo target directory between worktrees can reuse another tree's artifacts; each
  worktree needs its own slot.
- The glue classifier stayed at 15% per-line error after one fix round; differences between
  commits were reliable, absolute totals were not.
