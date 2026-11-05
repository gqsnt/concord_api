# OPUS review findings — pagination and caller workflows (strand 4) and cross-check

Reviewer: Claude subagent; this session's system prompt names `claude-opus-5-5`; the requested model/effort is not visible to me. I authored none of the reviewed code. Checkout: `/home/canarit/projects/concord-ext44/rv44-opus`, detached `0dd65501`; `rustc 1.99.1`; `CARGO_BUILD_JOBS=6`; loopback only; nothing outside `review/opus/` modified.

Order followed: caller task first (`TASK_NOTES.md`, written before reading any `FRICTION.md`, `MAINTENANCE.md`, `NOTES.md`, `DECISIONS.md` or `v4/extensions-real-apis/*`), then the review.

(Saved verbatim by the coordinator from the subagent's returned text: the subagent's harness refused file writes.)

## What was run (observed)

| Command | Result |
|---|---|
| `cargo test --offline -p concord-pagination` (repo root) | 68 pass (61 integration tests in 6 files + 7 doctests) |
| `cargo test --offline -p concord-example-{github,stripe,shopify,salesforce,kubernetes}-app` | all pass (GH-A 9, GH-B 10, KU-A 9, KU-B 13+2, SF-A 9, SF-B 14+3, SH-A 6, SH-B 12, ST-A 4, ST-B 3); `review/opus/apps-test-output.txt` |
| `cd review/opus/task && CARGO_TARGET_DIR=../target cargo test --offline -- --nocapture --test-threads 1` | 13 pass: 9 caller-task tests + 4 probes; `review/opus/task-test-output.txt` |

Probes (`review/opus/task/tests/`; each asserts the *observed* behaviour, so it fails when the behaviour changes):

| Probe | File |
|---|---|
| persisted GitHub child link restored into another parent | `keep_going.rs::persisted_child_link_restored_into_another_parent_is_not_refused` |
| `manifest_jobs` overlaps discovery and jobs at window 1 | `keep_going.rs::manifest_jobs_variant_overlaps_discovery_with_jobs` (baseline `hand_written_traversal_is_serial`) |
| GitHub `Link: next` equal to the requested URL | `probe_stuck_cursor.rs` |
| SH-A nested variant failure mid-page | `probe_shopify_nested.rs` |
| JSON-record full job through `independent` | `probe_jsonl_jobs.rs` |

## Summary

| ID | Title | Classification | Severity | Evidence | Disposition |
|---|---|---|---|---|---|
| OPUS-1 | GitHub `LinkPages::restore` re-binds any API URL: persisted child continuations lose their binding | correctness defect | medium | reproduced | fix now |
| OPUS-2 | Non-advancing continuations accepted by GitHub, Kubernetes, Salesforce adapters; only the budget ends the loop | correctness defect (provider robustness) | low | reproduced (GitHub), source-inspected (K8s, SF) | fix now |
| OPUS-3 | `Items` hides the page: metadata dropped, skipped items unverifiable; crate docs suggest a check `Items` cannot support | developer friction | medium | reproduced (task) + source | propose for next release (doc part: fix now) |
| OPUS-4 | Restarting uses two traversal types; two consumers merge reports twice and disagree on budget semantics | developer friction | medium | source-inspected in two consumers (one independent) | propose for next release |
| OPUS-5 | `manifest_jobs` cannot express a strictly serial provider; serial parent→child workflows hand-roll the traversal | speculative opportunity | low | reproduced | blocked by missing evidence |
| OPUS-6 | SH-A nested failure: parent summary says `Stopped`, resume = next products page, no failure; unprocessed siblings lost | correctness defect (report; latent) | low | reproduced | fix now (in SH-A) |
| OPUS-7 | Same drain / error-stash / outcome-mapping loop written ~13 times across five apps | developer friction | low | source-inspected | propose for next release |
| OPUS-8 | Much of the public surface has no real consumer | compatibility risk | medium (if published) | source-inspected (grep) | propose for next release |
| OPUS-9 | PAG-08's JSON-record full-job consumer is absent (CSV only); feasible in ~40 lines | requirement-evidence gap | low | reproduced (grep + feasibility probe) | fix now (add test) |
| OPUS-10 | `concord-pagination` is not discoverable from README / RECIPES / CHANGELOG | documentation gap | low | source-inspected | fix now |
| OPUS-11 | Reports name the *start* key of a failed multi-page job; the real restart travels in the error value | developer friction / documentation gap | low | reproduced (task) | fix now (docs) |

Counts:
- By classification: correctness defect 3 (1 medium, 2 low); developer friction 3 (2 medium, 1 low); developer friction / documentation gap 1 (low); compatibility risk 1 (medium); documentation gap 1 (low); requirement-evidence gap 1 (low); speculative opportunity 1 (low); performance-resource limit 0.
- By severity: high 0, medium 4, low 7.
- No defect found in `concord-pagination`'s own scheduling, ordering, budget or cancellation logic. Every defect is in SDK adapters, workflows, docs or evidence.

---

## OPUS-1 — GitHub `LinkPages::restore` re-binds any API URL

- **Affected task.** An application that saves a GitHub continuation and restores it later.
  - The SDK documents this use: `NextLink::raw_url()` is "for an application that persists a continuation; restore it with `LinkPages::restore`".
  - Examples: a GH-A child restart, a GH-B run/job scan.
- **Location.**
  - `examples/github-sdk/src/paging.rs:245-253`: `restore` checks only `is_api_url`, then attaches `self.binding`.
  - `paging.rs:282-288`: the later binding check compares the factory's binding with itself, so it always passes.
- **Reproduction.** `keep_going.rs::persisted_child_link_restored_into_another_parent_is_not_refused`.
  - Setup: a stored `.../repositories/101/issues/13/comments?page=2` is restored through `repo.issue_comment_pages(IssueNumber(11), ..)` and fetched.
  - Observed: one request to `/repositories/101/issues/13/comments?page=2`; the result is recorded as `Issue { number: 11, comments: 1 }`.
  - Expected: `GitHubError::ContinuationMismatch`, as for an in-memory link (`tests/paging.rs::continuation_binding_is_checked`).
  - The SDK's own test (`tests/paging.rs:279-301`) covers only the matching case and a foreign origin.
- **Cross-SDK.** Each SDK binds continuations differently; none share code:
  - Stripe persists a fingerprint of the prepared request and refuses a mismatch (fails closed).
  - SF-A's `QueryResume` has private fields and cannot be persisted (fails closed).
  - Kubernetes `Continuation::resume(token, snapshot)` checks the snapshot only, not the query.
  - Shopify `Cursor::new` has no binding.
  - Only GitHub *claims* a binding that restore then discards.
- **Smallest fix.** In `restore`, require the URL path to end with the operation path for the factory's parent (`issues/{n}/comments`, `pulls/{n}/reviews`, `actions/runs/{id}/jobs`, …). The binding already holds the operation and parent. Otherwise return `ContinuationMismatch`.
- **Different representation.** Stripe's approach: persist `(url, binding fingerprint)` via `NextLink::into_parts` / `from_parts` and compare on restore.
- **Impact.** No signature change (`restore` already returns `Result`). A few string comparisons per restore. No dependency. GH-A and GH-B are unaffected (they keep links in memory).
- **Disposition.** Fix now.
- **Falsifier.** The probe gets `ContinuationMismatch`, or the request is never sent.

## OPUS-2 — Non-advancing continuations accepted by three adapters

- **Affected task.** Any sequential traversal against a provider (or proxy/cache) that returns the continuation it was just given. Worst for callers with a huge budget.
- **Location.**
  - No `next == requested` check in:
    - `github-sdk/src/paging.rs:301-320`;
    - `kubernetes-sdk/src/list.rs:303-309` (the requested token is dropped; only the snapshot is compared);
    - `salesforce-sdk/src/query.rs:214-223` and `bulk_query.rs` `ResultSets::fetch` (`Sforce-Locator`).
  - Stripe (`stripe-sdk/src/pages.rs:370-380`) and Shopify (`shopify-sdk/src/connection.rs:59-67`) refuse it with `CursorDidNotAdvance`.
  - `checks/dx03-fanout-sink/src/stage1.rs:139` uses `Budget::new(NonZeroUsize::MAX)` and collects every row; its NOTES say it "adds no cycle detection".
- **Reproduction.** `probe_stuck_cursor.rs`: every response carries `Link: <.../repositories/101/issues?page=2>; rel="next"`; budget 50.
  - Observed: 50 requests, the same 2 items 50 times (100 items), no error, `Outcome::Bounded` with a resume point that loops again.
  - Expected: a protocol error on the first repeat, as Stripe and Shopify do.
- **Cross-SDK.** Missing in 3 of 5 adapters, present in 2. Same cause each time: the adapter compares nothing against the requested key.
- **Smallest fix.**
  - In each of the three adapters (3–6 lines): keep the requested key in the fetch future and refuse `next == requested` with the SDK's protocol error.
  - In the crate's `Budget` docs, one sentence: the budget is the only guard against cycles longer than one page.
- **Different representation.** An opt-in `Sequential::refuse_repeated_key()` for `K: PartialEq` that ends the chain with a report flag (the crate cannot build the adapter's `E`). This is weaker: an A→B→A cycle still needs the budget.
- **Impact.** Additive error variants in three SDKs; no crate change; negligible runtime cost.
- **Disposition.** Fix now.
- **Falsifier.** A provider contract that legitimately repeats a continuation. None of the five documents one.

## OPUS-3 — `Items` hides the page from its consumer

- **Affected task.** Item-level processing with resumable checkpoints (GH-A and my task); any item consumer that needs request ids or rate limits, or wants to verify a replayed page.
- **Location.**
  - `crates/concord-pagination/src/sequential.rs:410-423`: `page.meta` is dropped, and the first `skip` items are drained without being shown to the caller.
  - The crate's own advice at `sequential.rs:303-305` ("an adapter can [check], for example with a snapshot token kept in the page metadata") cannot work through `Items`.
- **Reproduction.**
  - (a) Metadata: GH-A writes a shared `Arc<Mutex<Vec<RequestRecord>>>` from inside every page future (`examples/github-app/src/requests.rs:35-37, 81-92`; its comment says the item consumer "does not return page metadata"). ST-A avoids `Items` for the same reason (`stripe-sdk/FRICTION.md` F6).
  - (b) Verification: my task had to arm a check in the factory for its first call only (`review/opus/task/src/lib.rs`, the `verify.take()` closure). Observed in `changed_listing_is_refused_on_resume`: `Stale { expected: 11, found: Some(13) }`.
- **Cross-SDK.** (a) was reported independently by GitHub (`github-sdk/FRICTION.md` F9) and Stripe (F6). (b) is new. `Items` has one production consumer (GH-A); the metadata loss is the stated reason the other candidate (ST-A) did not adopt it.
- **Smallest fix.**
  - Keep the current page's `M` next to the item buffer and add `Items::page_meta(&self) -> Option<&M>`.
  - Fix the doc sentence now: "check inside the factory, e.g. compare the replayed page's item at `skip - 1`".
- **Different representation.** `Items::next_with_page() -> Option<Result<(T, &M), E>>`, or `resume_items_verified(.., verify: FnOnce(&[T]) -> Result<(), E>)`, which sees the skipped prefix.
- **Impact.** Additive; `M` is already in memory; no new bound. GH-A could drop its ~40-line side channel.
- **Disposition.** Accessor: propose for next release. Doc sentence: fix now.
- **Falsifier.** A consumer needing per-item rather than per-page metadata (then the tuple form is better), or evidence that no item consumer needs page metadata (two SDKs say otherwise).

## OPUS-4 — Restarting uses two traversal types; two merges, two budget semantics

- **Affected task.** Continuing a manifest-discovered export after a failure: SF-B `continue_export` and DX-03 stage 2 `ExportReport::resume`.
- **Location.**
  - SF-B runs two phases:
    - Phase A, `independent` over the re-runs: `examples/salesforce-app/src/sf_b.rs:542-580`;
    - Phase B, `manifest_jobs` from `discovery.resume`: `sf_b.rs:582-628`.
  - Each phase is followed by the same ~25-line mapping of `retained` / `unemitted_errors` / `unfinished` / `rest`-or-`queued`.
  - `checks/dx03-fanout-sink/src/stage2.rs:193-260` repeats the same structure.
  - Budget disagreement:
    - SF-B gives each phase the full `job_budget` (`sf_b.rs:87`: "re-runs and discovery each have this budget"), so a restart call can download up to twice the budget.
    - DX-03 shares one budget across both phases (`stage2.rs:225-226`); its NOTES say: "otherwise a nominal per-pass limit would silently double".
  - PAG-03 chose `independent` re-runs "rather than a separate injection API".
- **Reproduction.** Source-inspected in both consumers. Both pass their tests; the difference is semantic. The two report types differ (`IndependentReport::rest: Option<Peekable<..>>` vs `JobsReport::queued: Vec<D>`), which forces the mapping to be written twice per consumer.
- **Cross-SDK.** Same cause in an independently written caller (DX-03: another model, public APIs only) and in SF-B. The Salesforce author also reported it (`salesforce-sdk/FRICTION.md` F9).
- **Smallest fix.** A lazy seed for `manifest_jobs`:
  - seeded descriptors are admitted before or ahead of discovery;
  - they share the job budget and window, and appear in the same `JobsReport` (unadmitted seeds land in `queued`);
  - possible API: `JobOptions::seed(..)`, or `manifest_jobs_from(JobsRestart { rerun, discovery }, ..)` with `JobsReport::into_restart()` producing the restart.
- **Different representation.** Keep two traversals, but give both report types one shared settled part with one merge method.
- **Impact.**
  - Additive API, no new dependency.
  - One window instead of two phases (today the window sits idle at the phase boundary until the last re-run finishes).
  - SF-B and DX-03 each lose ~70-80 lines, and the budget ambiguity disappears.
- **Disposition.** Propose for next release. This reverses PAG-03, based on consumer evidence.
- **Falsifier.** A consumer needing different options for re-runs and for discovery. Neither consumer does: both pass identical options, apart from DX-03's remaining-budget arithmetic.

## OPUS-5 — `manifest_jobs` cannot express a strictly serial provider

- **Affected task.** A parent chain that discovers per-item child work under a "serial requests" rule:
  - GH-A: issues → comments, details, reviews;
  - ST-A: invoices → line items;
  - SH-A: products → variants.
  - All three hand-roll the nesting. My task needed continue-after-child-failure with failed keys, which is exactly what `manifest_jobs` + `OnError::Continue` offers.
- **Location.** `crates/concord-pagination/src/jobs.rs:284-292`: `may_discover` ignores the window. The `window + 1` bound is documented (`jobs.rs:61-62`, PAG-08).
- **Reproduction.** `keep_going.rs::manifest_jobs_variant_overlaps_discovery_with_jobs`, window 1, 40 ms reply delay.
  - Functionally correct: records 11, 12, 13; failed `[(13, false)]`; `ExhaustedWithFailures`.
  - But the issue-page request overlapped PR #12's details and reviews requests: 2 overlaps in 10 requests, arrival gaps 21–170 µs over two runs.
  - The hand-written traversal never overlaps (`hand_written_traversal_is_serial`).
  - The checkpoint after the first record is `admitted=1, queued=[(12, true)], discovery=At(page 2)`. Resuming needs the persisted descriptors plus a manifest key, not a single item position.
- **Cross-SDK.** Three serial providers (GitHub, Stripe, Shopify) each wrote their own loops. Contract §7.4 correctly forbids parallelizing a serial provider just to use a feature.
- **Smallest change, if ever wanted.** `JobOptions::discovery_in_window()`: admit a manifest call only when `active + retained + 1 <= window` (contract §3.5 already allows a shared window). About 10 lines plus a test.
- **Different representation.** Keep the nesting in workflow code, as today.
- **Disposition.** Blocked by missing evidence. No workflow has asked for this, and SH-A's author found explicit loops clearer (`shopify-sdk/FRICTION.md` F5).
- **Falsifier.** A serial workflow that would adopt `manifest_jobs` if discovery shared the window, and would accept `JobsCheckpoint` instead of item checkpoints.

## OPUS-6 — SH-A nested failure: misleading parent summary, unprocessed siblings lost

- **Affected task.** SH-A reconciliation when a product's nested variant chain fails.
- **Location.**
  - `examples/shopify-app/src/catalog_inventory.rs:666-676`: `state.stop` causes a `break` out of the page loop, then `TraversalSummary::new(pages.finish(), failure)` runs with `failure = None`.
  - The field docs at `catalog_inventory.rs:211-221` say `resume` is "the failed page after a failure, the next page when bounded" and that `failure` is present "when outcome is Failed".
- **Reproduction.** `probe_shopify_nested.rs`, using the SH-A fixtures; product 101's variant page `dmFyLXAxLTI=` answers THROTTLED.
  - Observed for the products stage: `outcome=Stopped, pages=1, resume=Some("cHJvZC0y")` (the *next* products page), `failure=None`.
  - `counts.products=1`: product 102, after 101 on the same page, is never processed, and appears nowhere in the report.
  - Requests stop after the failed variant page. The failure itself is recorded only in `variant_pages.incomplete`.
  - Expected: the parent summary names the page being consumed, or lists the unprocessed siblings.
  - Severity is low because SH-A takes no resume input today: the effect is a misleading report, not data loss in a resumed run.
- **Cross-SDK.** Same root as GH-A's "checkpoint before `next()`" (`github-sdk/FRICTION.md` F10): after a page is emitted, the chain position has already moved past it. GH-A handles this; SH-A does not. The Shopify author noted the nested variants (F6) but not the skipped siblings.
- **Smallest fix.** In SH-A, take `pages.checkpoint().cloned()` before each `next()` and report it as `resume` when stopping mid-page; document the `Stopped` case. Alternatively, record the unprocessed product ids.
- **Different representation.** Walk products with `Items`. That would also need OPUS-3's accessor, because SH-A records cost per page.
- **Impact.** Local to SH-A; no API change.
- **Disposition.** Fix now.
- **Falsifier.** The probe reports the current page as the resume point, or a list containing 102.

## OPUS-7 — The same drain / error-stash / outcome-mapping loop, ~13 times

- **Affected task.** Every sequential consumer that needs both the page error and the final report.
- **Pattern.** `let mut failure = None; while let Some(p) = chain.next().await { match p { Ok(..) => .., Err(e) => failure = Some(e) } } let report = chain.finish(); match (report.outcome, failure) ..`
- **Location.**
  - GH-A `audit.rs:777-819`;
  - ST-A `reconciliation.rs:421-455`;
  - SH-A `catalog_inventory.rs`, six times (e.g. 576-602, 613-676);
  - KU-A `collect.rs:149-213`;
  - SF-A `sf_a.rs:504-536`;
  - GH-B `artifacts.rs`, three early-return variants;
  - also in the maintenance checks.
- **Why it repeats.** `next()` moves the error out, and `SequentialReport` keeps only keys, so every caller has to stash the error itself.
- **Cross-SDK.** Reported independently by Shopify F5 (which suggests a `try_for_each_page` helper), Kubernetes F6, Salesforce F10 and the Kubernetes maintenance exercise (F6). ST-A already wrote its own `drain` helper with an `AsyncFnMut` callback.
- **Smallest fix.** An additive `Sequential::drain(self, each: impl AsyncFnMut(Page<T, K, M>) -> ControlFlow<()>) -> Drained<K, E>` that returns the report together with the error. ST-A's `drain` shows this shape compiles on this toolchain with borrowed `&mut` state.
- **Different representation.** Keep the loops and document the pattern in the crate README.
- **Impact.** Additive, no dependency; about 10 lines saved per call site.
- **Disposition.** Propose for next release, low priority.
- **Falsifier.** A call site whose per-page work cannot run inside a callback. None found.

## OPUS-8 — Public surface without real consumers

- **Evidence** (grep over `examples/*-app/src`, `examples/*-sdk/src`, `checks/*/src`):
  - 0 uses: `first_page_then`, `Options::failure_limit`, `Independent::checkpoint` / `Checkpoint`, `Jobs::checkpoint` / `JobsCheckpoint`, `restart()`, report `failed()`, `progress()`, `Items::finish` remainder / `after_remainder`, `is_discovery_incomplete`.
  - 1 use (DX-03 only): `Order::Completion`, `into_rerun`.
  - `independent` and `manifest_jobs`: one provider consumer (SF-B, where `independent` serves only as the OPUS-4 restart mechanism), plus DX-03.
  - No workflow runs a known independent page plan (P02) or a first-page plan (P03) against a provider; those exist only in crate tests.
  - window + jobs + independent are ~1,300 of the crate's ~2,300 source lines.
- **Reproduction.** `grep -rn --include=*.rs "<name>" examples/*-app/src examples/*-sdk/src checks/*/src`.
- **Classification.** Compatibility risk. The surface is contract-mandated (PAG-02, PAG-06) and well tested, but no real provider adapter has shaped it yet. OPUS-4 already shows `manifest_jobs` wanting a change after its first consumer.
- **Smallest action.** Keep it, but mark `first_page_then`, the sparse snapshots and `failure_limit` as provisional in the README until a provider consumer exists. Do not extend the scheduler further without one.
- **Different representation.** Put these members behind an `unstable` feature before any crates.io publication (the crate is `publish = false` today).
- **Impact.** Docs only, or a feature flag.
- **Disposition.** Propose for next release; tie it to the separate publication decision.
- **Falsifier.** A provider workflow that consumes these members without changing their shape.

## OPUS-9 — PAG-08 JSON-record full-job consumer absent

- **Requirement.** PAG-08: "CSV and JSON-record full-job consumers hold permits through body/sink completion and account for partial and un-emitted completed shards". `contracts/PAGINATION.md` §6 adds: "Use the JSON-record reader in an additional streamed job".
- **Location.**
  - The JSON-record readers are used in SH-B, KU-B and the stress harness, but none of these go through `independent` or `manifest_jobs`.
  - The crate's job tests use a synthetic shard job (`tests/jobs.rs:562-628`).
  - SF-B and DX-03 read CSV.
- **Reproduction.**
  - Absence: the files using `JsonLinesReader` / `JsonValuesReader` and the files calling `manifest_jobs` / `independent` do not intersect.
  - Feasibility: `probe_jsonl_jobs.rs` runs four JSONL jobs at window 2; the first body stalls after one record. Observed: `in_flight=1, retained=1`; only `/a` and `/b` were requested; `finish` gives `Stopped`, `retained=[b: 3 records]`, `unfinished=[a Cancelled, b Retained]`, `open_after: 0`.
- **Smallest fix.** Add a maintained test like the probe (~60 lines) and map it to PAG-08. The scheduler is already codec-agnostic, so this is missing evidence, not missing functionality.
- **Disposition.** Fix now.
- **Falsifier.** A JSON-record job consumer I missed. I searched every `*.rs` outside `crates/concord-extras`.

## OPUS-10 — `concord-pagination` is not discoverable

- **Location.**
  - `README.md:23-25`: "Concord deliberately does not manage … pagination … Those stay in your code".
  - `README.md:195`: the pagination row points to RECIPES.
  - `docs/RECIPES.md:287-293`: shows only `collect_items`, a hand-written traversal.
  - `CHANGELOG.md:3-6`: "No public API, behaviour or version change: the four crates are still 4.2.0".
  - None of these mentions the fifth crate.
- **Reproduction.** `grep -rn "concord-pagination\|concord_pagination" README.md CHANGELOG.md docs/` returns nothing.
- **Smallest fix.** Add a README row, a RECIPES paragraph pointing to the crate and its README, and a CHANGELOG entry. The statement that core Concord does no pagination stays true.
- **Disposition.** Fix now.

## OPUS-11 — Failed key versus restart key for multi-page jobs

- **Location.** `IndependentReport::failed` / `into_rerun` (`independent.rs:386-404`) and `JobsReport::failed` / `into_rerun` (`jobs.rs:510-530`) return the key each job *started* with.
- **Reproduction.** My `repair` pass runs child chains as independent jobs.
  - `repair_that_fails_again_reports_the_new_restart_point` shows that the useful restart point (`reviews?page=2`) exists only in the error value.
  - `into_rerun` would refetch pages that were already processed.
- **Fix.** One doc sentence: the keys are start keys, so a job with internal progress must return its restart point in its error.
- **Disposition.** Fix now (docs).

---

## Brief topics — verdicts

| Topic | Verdict |
|---|---|
| Factory ergonomics | Good: a one-line `\|key\| pages.fetch(key)` everywhere; no `Send`/`'static`; closures can borrow. Limit: a traversal cannot be returned from a function without naming closure/future types (Stripe F6, Shopify F5). |
| Nested extraction | Good: adapters move items out of envelopes before projection. Embedded first pages start chains with `Resume::At` without charging the budget (Stripe lines, Shopify variants). |
| Opaque dependent cursors | Good; but loop protection is per adapter (OPUS-2). |
| Discovered parallel work | Correct and bounded; one provider consumer; serial providers cannot use it (OPUS-5). |
| Ordered buffering | Correct: retained results occupy window slots; a stalled early key holds at most `window-1` later results; verified also with JSON jobs (`window.rs:125-127`). |
| Stop/cancel | Correct: `finish` and drop release owned futures; job guards remove staged shards; KU races the cancel-safe `next()` against its stop signal. |
| Continuation binding | Five SDKs, five representations; GitHub's persisted path fails open (OPUS-1). |
| Mutable collections | Honest "live best effort" docs; item replay assumes stable pages; verification is only possible inside the factory (OPUS-3). |
| Retry/replay | No retry anywhere; budgets count logical calls; replays are explicit. No consumer composes retry in a factory yet, so that rule is untested in practice. |
| Page metadata through `Items` | Lost (OPUS-3). |
| Checkpoints | Item checkpoints work as documented; persistence is inconsistent across SDKs (OPUS-1); sparse snapshots unused (OPUS-8). |
| Restart composition | Works, but duplicated and ambiguous on budgets (OPUS-4). |
| Generalization vs consumers | `sequential` justified (6 workflows, 5 SDKs, 4 maintenance checks, DX-03); `Items` 1 consumer; `independent` / `manifest_jobs` 1 provider; `first_page_then` 0 (OPUS-8). |

## Counter-evidence (what works well)

- **Scheduler correctness.** I traced these paths across `window.rs`, `independent.rs`, `jobs.rs` and `sequential.rs` and found no inconsistency:
  - halt/emit ordering;
  - retained-result accounting;
  - budget charged at admission;
  - bounded vs exhausted told apart by one-key look-ahead;
  - discovery failure in both modes;
  - `finish` while futures are still running.
- The crate tests cover the contract's named regressions: stalled page 2 / retained 3-4 / failing 5, long alternating failures, empty manifests.
- Dependencies are light: only `futures-util` with `alloc`. No runtime, no spawned tasks, `unsafe_code = "forbid"`.
- Typed errors stay typed, and `Debug` never prints keys.
- My caller task compiled on the first build. Page-boundary checkpoints, empty pages with `next`, the failed page as resume point and `finish` after an early stop all behaved as documented.
- DX-03 (another model) called the job facade "the correct concurrency unit" and the report docs "unusually useful".
- The workflows are honest about consistency: GH-A `LiveBestEffort`, KU snapshot checks and restart boundaries, SF-A `RestartRequired`, ST-A keep-going child chains.

## Overall judgment (strand 4)

- `concord-pagination` is correct, bounded and well documented, and its `sequential` core is clearly validated by real consumers.
- Its scheduler half is equally careful, but rests on one provider workflow and one independent caller. Several members have no consumer, and the one consumer pattern that exists already argues for an API change (seeded jobs).
- The reproduced defects are all outside the crate: a GitHub restore-binding hole, missing non-advancing checks in three adapters, a misleading SH-A report, and a missing PAG-08 JSON test.
- Recommendations:
  - fix OPUS-1/2/6/9/10/11 now (all small);
  - add `Items::page_meta` and a seeded `manifest_jobs` in the next release;
  - keep the rest of the scheduler provisional until a second provider uses it.

## Cross-check: other strands and next-release synthesis

I did not read the other strands' outputs; these are predictions.

| Expected recommendation (strand) | Position | Evidence that would change my mind |
|---|---|---|
| Public "typed value + response metadata" send path; every SDK re-implements ~70 lines (ASTRA; GitHub F1, Stripe F4) | **Accept**, as an extras/native helper rather than a core response wrapper; every pagination adapter depends on this plumbing for `Page::meta` | Two SDKs needing metadata the helper cannot capture |
| Terminal-vs-isolable flag on stream `ReadError` (ASTRA; Shopify F7 caused a defect) | **Accept** | Terminal errors guaranteed to be followed by `None`, documented and tested |
| Box `CodecError`'s payload (Salesforce F7: 112 bytes, `result_large_err`) | **Accept** | A benchmark showing the extra allocation matters on a hot path |
| Generated read accessors / common trait for list operations (TERRA; Stripe F3 macro, K8s `ListOperation` macro, Salesforce `QueryRecord` ×3) | **Partly accept**: getters are cheap; a pagination-specific generated trait is unnecessary (GitHub's `fn(Op) -> Result<Prepared<R>>` pointer avoids one) | A fourth SDK forced into macro bridges despite the fn-pointer pattern |
| Pagination statement in the DSL / core | **Reject**: the five adapters differ in key, binding, snapshot and loop checks; the crate needed zero core changes (PAG-15); the contract forbids mandatory grammar | Two adapters becoming identical after the OPUS-1/2 fixes |
| Crate-level resume-binding hook (Stripe F6) | **Reject**: the factory already sees the key, and every existing binding check lives there; a hook adds a second place to forget | A binding that needs traversal-only information |
| `Stream` impls (PAG-14 declined) | **Defer**: cheap to add, but no consumer has asked | An SDK that needs to return or box a traversal |
| Serde/persistence for checkpoints in the crate | **Reject** (contract: no persistence framework) | Three SDKs writing identical persistence shims |
| Removing `first_page_then` | **Reject removal**; mark it provisional instead | Still no consumer by the next review |

**Next-release priorities**

1. **Fix now.** OPUS-1, OPUS-2 (plus the `Budget` doc sentence), OPUS-6, OPUS-9, OPUS-10, OPUS-11, and the OPUS-3 doc sentence.
2. **Propose for next release.** `Items::page_meta` (OPUS-3); seeded `manifest_jobs` / `JobsRestart` with one shared budget (OPUS-4); optional `Sequential::drain` (OPUS-7).
3. **Hold.** No new scheduler features (OPUS-5). Keep unconsumed members provisional (OPUS-8) until a second provider uses them; revisit at the publication decision.

## Limits of this review

- No live provider calls; loopback only.
- The requested model/effort is not visible to me.
- CSV/JSONL codecs, the DSL and core seams are out of scope beyond what pagination consumers touch.
- The OPUS-5 overlap is a loopback timing observation: it shows the overlap happens, not how often in production.
- "Independent" is claimed only for DX-03 and the maintenance exercises, as their notes state; whether the five SDKs had different authors cannot be verified from the repository.
