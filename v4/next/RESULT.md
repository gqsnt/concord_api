# P4 result: the owner's five picks on `concord-next`

Anchor: `4222656b` (`main` after PR #6). Head at writing: `d2c3ee37` plus this record. All
numbers below come from `tools/glue/measure.py` (the glue classifier, about 15% per-line error:
read differences, not absolute totals) over all 55 example SDKs, or from the commands named.

The owner picked, in this order: (1) SDK types, (2) operations and paging, (3) construction,
(4) errors, (5) Concord logs, with eight decisions recorded in `OWNER_QUEUE.md` (Q1). H1
(fact-line docs) was not picked; decision 8 is recorded for when it is.

## What changed, anchor → head (all 55 SDKs)

| Measure | Anchor | Head | Δ |
|---|---:|---:|---:|
| M1 glue | 21,155 | 18,886 | **−2,269** |
| glue: error | 11,714 | 10,602 | −1,112 |
| glue: redaction | 1,516 | 441 | −1,075 |
| glue: paging | 3,348 | 3,132 | −216 |
| glue: forwarding | 774 | 747 | −27 |
| glue: construction | 1,242 | 1,373 | +131 |
| glue: result | 2,397 | 2,426 | +29 |
| glue: operation / docs | 44 / 120 | 46 / 119 | +2 / −1 |
| model / behavior / other | 18,113 / 23,711 / 8,392 | 18,173 / 24,285 / 8,204 | +60 / +574 / −188 |
| SDK `src` nonblank lines (hand count) | 77,018 | 75,875 | −1,143 |
| M4 DSL lines (endpoints) | 2,079 (316) | 2,187 (316) | +108 |
| M4 DSL lines per endpoint | 6.58 | 6.92 | +0.34 |
| M5 negation statements | 0 | 0 | 0 |
| M6 simple controls (Nager / PokeAPI / Frankfurter DSL lines) | 8 / 7 / 11 | 8 / 7 / 11 | byte-identical sources |

Behavior grows (+574) because hand-written code that stays (status routing, signing, retry and
paging decisions) is re-classified once the restated facts around it are gone, and checked origin
types add real validation. Generated endpoint documentation shrank by 2,225 words in pick 3; no
pick made an endpoint's generated words grow.

## Per pick

| Pick | Commits (merge → final) | Glue Δ (this pick) | Review (MERGE_IDEA, Codex, same provider) | Local gate | CI on the pick's own head |
|---|---|---:|---|---|---|
| 1 SDK types (D1; decision 7: `serde_plain`, `WireDisplay` removed) | `332db3f9` → `3e47d00c` | −1,238 | r1 ACCEPT_WITH_FIXES (blocking test fixed) | PASS 66/66 | green: Linux, Windows, nightly |
| 2 Operations and paging (F3 + E1 + F1; decision 1: runtime lookup) | `6bbae0f8` → `ca43d791`, gate fix `4ebd52c4` | −369 | r1 ACCEPT_WITH_FIXES (count corrected; read classifiers; name reservation) | PASS 66/66 on `4ebd52c4` (failed on `9dad4b9c`/`ca43d791`: feature-dependent UI text, fixed) | Windows green; Linux blocked by runner disk (fixed in `f820ef8d`) |
| 3 Construction (C1 + C2 + G1; decisions 4, 5) | `a72caa4d` → `890334af` | +121 | r1 ACCEPT_WITH_FIXES (all yes on C1–C9; doc fixes) | PASS 66/66 on `5448c90f` | Windows green; Linux blocked by runner disk |
| 4 Errors (A1 + A2 + `#[from]`; decision 6) | `c0a3489f` → `8fda4ae1` | −784 | r1 REJECT (coordinator's control restore left a doc excerpt stale) → r2 ACCEPT, no findings | `f820ef8d`: 65/66, USGS isolated build lacked `extras` (fixed in pick 5's commit) | Linux: every gate step passed except the same USGS step (disk fix worked); Windows pending |
| 5 Concord logs (J1 + J3; decision 3) | `fe79ef53` → `d2c3ee37` | 0 | r1 ACCEPT_WITH_FIXES (summary status test added) | see "Final verification" | see "Final verification" |

Every pick's lane, fix lanes and the conflict-resolution lane for pick 4 ran on Codex
`gpt-6.1-sol` xhigh (requested and observed in each lane's `meta.json`); every review was an
independent Codex session of the same provider. Lane records: `work/lanes/p4-*`; reviews:
`work/reviews/merge-idea-p4-*` (local mission workspace).

### What each pick did

1. **SDK types.** `#[derive(concord::Debug)]` with `#[sensitive]` (fields shown unless marked,
   decision 2) replaces hand-written redaction in 45 SDKs (218 more definitions). Wire spelling
   uses `serde_plain::derive_display_from_serialize!` (63 carriers); `concord::WireDisplay` is
   removed. serde_plain panics for a non-scalar shape, and Concord does not catch it (decision 7;
   a lane-added `catch_unwind` was removed by the coordinator).
2. **Operations and paging.** `operations Name;` and `SendOperation` are gone; every policy
   implements `SendWith<T, D>` and reads the endpoint's `Operation` (name, method, `is_read()`) at
   run time. Each client gets a doc-hidden, crate-private operation inventory, so the eight former
   H08 adopters keep an exhaustive match (an unclassified endpoint is still E0004); the simple
   controls' public items and docs are byte-identical. 26 declared continuations in eight SDKs;
   23 declared POST reads in ten SDKs; `Pages::starting_at` / `refuse_repeated_keys` replace 17
   twins; page factories check every start.
3. **Construction.** Required client/scope values have no setter unless `mut` (121 prior setters:
   108 removed, 13 kept where the provider lets the value rotate); `base x;` takes the origin from
   a checked value (Salesforce, Datadog, Zendesk, DigitalOcean, Sentry, USGS, ClickHouse, Registry,
   QuickBooks connection); Mastodon's REST and streaming origins are two handles; getters replace
   copied wrapper state. FM1–FM4 fail to compile. QuickBooks `prepare()` shows the connection's
   host (decision 5).
4. **Errors.** 48 SDKs declare the provider error document they used to read by hand
   (`error D in F;`); 87 SDK enums and 12 example enums derive `concord::derive::Error`; typed
   provider splits derive `From<concord::Error<D>>`. The two derive regressions that rejected the
   direction are fixed. Status-routed, in-2xx, retry and paging behavior stay hand-written.
   (Commit `498a9890`'s message says 88 enums; the count after the C3 restore is 87.)
5. **Concord logs.** Optional `tracing` feature: one `concord.call` event per request with six
   fields, none of them a value; `Summary` and four `summary()` accessors; `Prepared`'s Debug
   always withholds a `query(..)` query (decision 3). A committed footprint check (run by
   `scripts/verify.sh`) proves the feature-off build carries no route literals or event code.

### Coordinator corrections (recorded, each reviewed afterwards)

- Pick 1: removed a lane-added `catch_unwind` in `concord-core` (it would have caught every user
  `Display` panic and does nothing under `panic = "abort"`).
- Pick 4: restored Frankfurter and PokeAPI after the lane migrated them (charter C3). The
  coordinator first re-tested only those crates, which left a doc excerpt stale; the review caught
  it and the fix lane restored the excerpt.
- Merges: the coordinator resolved the conflicts of picks 2, 3 and 5 and adapted J1's route
  template to value bases (a value base contributes no path to the event route).

## Breaking changes (summary; complete lists in `docs/MIGRATION.md` and `CHANGELOG.md`)

Debug text of migrated SDK types; 24 integer-backed `Display` impls ignore formatter flags;
`WireDisplay` removed; `operations Name;` / `SendOperation` removed and the hidden
`__<Client>Operation` name reserved; 17 `*_from` paging methods and two SDK error variants removed,
`PagesError::Repeated` added; 108 required setters removed, value bases have no `with_base_url`,
checked origin and key types in several configs; `D` in SDK signatures, Registry/Anthropic/S3 error
field and variant changes, consumed native error bodies; `Prepared` Debug withholds `query(..)`
queries; S3 and Kraken request Debug via `summary()`.

## Final verification

To be completed on the final head (this record's commit): full local gate
(`scripts/verify.sh --package --bench`) and CI on Linux and Windows. Coordinator workspace test on
`995609ae`: 4,196 passed, 0 failed, 31 existing ignores.

## Not done, and limits

- **Merge into `main`: on hold** (owner, 2026-11-09 21:55). Nothing was merged, tagged or
  published.
- No pick ran on a CI head of its own once Linux CI ran out of disk (picks 2–4); their local Linux
  gates and Windows CI are recorded above, and the final head carries all of them.
- M8 (compile time, allocations) was not measured; no live provider was called.
- Reviews are same-provider (Codex reviewing Codex lanes); no cross-provider review of P4.
- The glue classifier's 15% per-line error applies to every number above.
