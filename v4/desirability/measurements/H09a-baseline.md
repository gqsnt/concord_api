# H09a baseline — resuming a traversal from a saved position (task baseline before any card)

Purpose (queue triage 2026-11-09, owner "Measure H11/H09a first"): measure whether callers can complete
ordinary resume use with today's `_from` twins before H09a (`Pages::starting_at` removes `_from` twins) may be
carded. No card, threshold or candidate exists; this record states measurements and a proposed disposition.

Instrument: pilot harness `reader` tasks (H04 key format, R3 sandbox compile/run, exact inner request vector).
Sealed bank `work/sealed/h09a-bank`, set `h09a-bank` (`d81d3df1…`, ledger line `4f5b0478` on `dz/h09a-bank`),
authored by a Codex lane (`h09a-baseline-codex`, observed `gpt-6.1-sol` xhigh); material commit `475a7867`.
31 tasks / 60 facts in 13 SDK groups: 18 traversal `_from` twins in 12 SDKs — resume tasks (15), fresh
traversals of the same listings (11), a Mailgun contrary item (edge), PokeAPI simple resume/fresh controls,
Stripe as a separately labelled supplement. Coordinator validation in the R3 sandbox: 122/122 (31 references,
60 seeded wrong answers, 31 equivalent spellings) at the ledger hash. Readers: 13 groups × 2 providers × 3
repetitions = 78 sessions per round (186 answers); requested Codex `gpt-6.1-sol` xhigh — observed the same;
requested Claude `claude-opus-5-5` effort medium — observed model `claude-opus-5-5`, effort not visible.

## Round 1 (materials v1) — instrument finding, not a task deficit
Pass counts (answers): resume 20/90, fresh 63/66, edge 1/6, supplement resume 0/6. Every failed resume answer
was a compile error. Cause: each resume answer constructs the position as `concord_pagination::Resume::At(..)`,
and the v1 materials (H04's renderer: the SDK crate's own public items) showed `Resume<..>` only inside
signatures — never its definition (variants `Start`, `At(K)`) — in all 12 twin SDKs; readers guessed variant
names (`Resume::Cursor`, `Resume::cursor`) and module paths. On docs.rs the type name is a link to its page, so
v1 withheld what a reader of the published docs reaches in one click. The one SDK whose signatures spell the
crate path (Spotify) scored 6/6 even in round 1. This failure would change the disposition (A3), so it was
corrected before any decision.

## Round 2 (materials v2: + linked definitions)
`prepare_linked.py` (bank-local, coordinator): v1 materials kept byte-for-byte as the prefix, plus one appended
section per SDK with the public `concord_pagination` items whose names occur in the SDK's own signatures
(`Resume`, `Pages`; `Page`/`PageItems` where named), rendered from the same commit's rustdoc JSON (provenance
`h04-tree-475a7867…`) by the unchanged H04 `render_item` and example filter. CHEATSHEET unchanged; PokeAPI
unchanged (no linked names). Re-packed as `baseline-2` with the same seed, template and schedule; only the
TYPES/DOCS inputs of the 12 twin SDKs changed. All 78 sessions re-run.

| class | Codex (r1 → r2) | Claude (r1 → r2) |
|---|---|---|
| resume (15 tasks; 14 scored, see below) | 15/42 → **37/42** | 5/42 → **42/42** |
| fresh (11) | 31/33 → 33/33 | 32/33 → 33/33 |
| edge: Mailgun contrary (1) | 1/3 → 3/3 | 0/3 → 3/3 |
| supplement resume / fresh (Stripe) | 0/3 → 2/3 / 3/3 → 3/3 | 0/3 → 3/3 / 3/3 → 3/3 |
| simple resume / fresh (PokeAPI) | 3/3 → 3/3 / 0/3 → 0/3 | 3/3 → 3/3 / 3/3 → 3/3 |

Remaining misses in round 2:
- **Codex invents a module path** in 5 of 42 resume answers (GitLab 3, Distribution Registry 2) and 1 of 3 Stripe
  supplement answers: `concord::pagination::Resume`, which exists nowhere in the materials or the source (the
  materials spell `concord_pagination::Resume`; the prompt forbids invented prefixes). Claude: none.
- **PokeAPI simple-fresh, Codex 0/3 in both rounds:** an explicit `offset=0` on the first request; the key
  requires the exact first request. A strict control, unrelated to resuming.
- **Excluded task (instrument defect found in scoring):** `h09a/notion/02` requires a value its task text does
  not state (Codex 2/3, Claude 0/3 in round 2); it is excluded from the counts above and recorded as a known
  limit. A corrected bank would need a new name and full revalidation (runbook).

## Disposition (h09a-baseline-review-sol: REJECT of "not carded" → narrow card)
My proposal was "not carded". The independent review (Codex `gpt-6.1-sol` xhigh, read-only) **rejected** it
(blocking): Codex resume 37/42 against fresh 33/33, with failures in all three GitLab repetitions, are measured
ordinary-use failures; provider disagreement limits generalization, and the re-export census is context, not an
independent defect. The evidence **does not support** H09a's candidate (`Pages::starting_at`, removing the twins);
it supports a **bounded import-discovery card**: does explicit, qualified `Resume` guidance at the resume
methods reduce the observed resume compile failures? Smallest comparator: the current `_from` API with today's
docs versus the same API with that guidance (docs first; SDK re-exports only if needed). → **H09c** (card
`cards/H09c.md`). H09a as formulated (`Pages::starting_at`) is not carded.

The review also confirmed: every round-1 resume failure is a compile failure consistent with the omitted
`Resume` definition (verdict-changing, A3); round 2 is a legitimate correction within the two-round cap
(appended content reproduces exactly from same-commit public rustdoc; all 39 files keep their v1 prefix;
CHEATSHEET and PokeAPI identical; packs, template, seed, schedule, set hash and the 122/122 validation
verified); the recounts above; the exclusion of `h09a/notion/02`; the five Codex misses all being the invented
path. Non-blocking finding applied: `prepare_linked.py` parses the task files (SDK identities, commit) and the
unchanged H04 navigation filter reads SDK source as in v1; no task, key or source content supplies the additions.

Observation kept for the record: 9 of the 12 twin SDKs neither re-export `Resume` nor the `concord_pagination`
crate (Cloudflare, DigitalOcean and Spotify re-export the crate), so their resume callers name a second crate;
one provider guessed a `concord::pagination` module instead.
