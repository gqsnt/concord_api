# Direction: ops-paging — operation facts, continuation policy and traversal setup (F3 + E1 + F1)

P4.2 update (2026-11-09): the owner chose runtime lookup. The implementation removes
`operations Name;`/`SendOperation`, uses one `SendWith` trait and automatic private inventories,
keeps all eight exhaustive classifications, defers default continuation Send bounds, and tests
Datadog's production posting match with a seeded future write. The snapshot evidence and limits
below describe the earlier direction; see [current migration](../../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Condensed from the [direction source](/home/canarit/projects/Concord_Next/work/design/directions/ops-paging.md), [fix result](/home/canarit/projects/Concord_Next/work/lanes/ops-paging-fix-r1/result.md) and reviews below. Direction and fix lanes: Codex `gpt-6.1-sol` xhigh, confirmed by [direction metadata](/home/canarit/projects/Concord_Next/work/lanes/dir-ops-paging/meta.json) and [fix metadata](/home/canarit/projects/Concord_Next/work/lanes/ops-paging-fix-r1/meta.json). Reviews: independent session, same provider.
Branch `direction/ops-paging` at `46f66793`, pushed as `spike/dir-ops-paging`; base `ba10189b` at `/home/canarit/projects/Concord_Next/work/wt/rv-p1`; F3+E1 merge `6d943873`; F1 snapshot `60374269`. Worktree: `/home/canarit/projects/Concord_Next/work/wt/dir-ops-paging`. The reviewed commit supersedes the lanes' uncommitted-tree notes.

**Status: works, with recorded limits.** Round 2's final figures supersede the earlier totals: **9,160 → 8,763 glue (−397), −434 SDK source lines over 18 SDKs**. Including newly touched Xero, glue **329 → 329**, gives **−397 glue / −422 source over 19 SDKs**. Snapshot figures below are explicitly at `60374269`.

## Shape

GitHub's `GET issue_comments(number: IssueNumber, query since?: Timestamp, query per_page?: PerPage) -> Vec<IssueComment>` retains `path "issues/{number}/comments";` and adds `page next link;`. GET supplies the read fact. Reading POSTs use `#[read]`: Datadog search and Kraken's account scope. Ordinary Rust policy reads `Operation::is_read()`.

Caller: `comments.pages_from(resume).max_pages(cap)` becomes `comments.pages().starting_at(resume).refuse_repeated_keys().max_pages(cap)`. Discord uses `channel.history(limit).starting_at(resume)`. GitHub selects the guard at the call; Datadog/Notion enable it inside their traversal methods.

F1 lives in concord-pagination. Both builders require an unpolled `Pages`; the last start wins. Both panic at the caller after the first poll, including Pending, a page or exhaustion. Construction/builders invoke the factory **0** times. Dropping an unpolled `next_page()` future leaves setup available; `#[track_caller]` locates misuse.

The guard admits each `At(key)` once, with `Eq + Hash + Clone`, immediately before the factory. A resumed key is fresh; Start has no invented key. Exhaustion precedes caps; caps precede admission. Accepted items remain. Refusal yields `PagesError::Repeated { resume, progress }`, then Ended; reports use `Outcome::Repeated`, retain the refused position and set `failed = None`.

One optional sequential-engine guard serves pages/items/streams/events/reports. The erased facade projects its outcome through the existing error path. An exclusive-access mutex preserves `Sequential: Sync` without `K: Sync`; polling uses `get_mut`, with **0** lock operations. Pending futures are retained without readmission.

`Followed<E>::operation()` delegates to E; preparation retains its metadata. Static/prepared/error assertions and operation-aware policy checks preserve the read fact on both sends, including Datadog's paged POST. F1 adds **0** read flags, parsers or send paths. SDK wrappers retain listing identity/provider validation; **3** Datadog/Notion seen-key sets become one guard.

## What disappears

At `60374269`, the cohort includes **18** touched SDKs plus **3** controls. M1 **9,160 → 8,760 (−400)**: error **4,964 → 4,844 (−120)**; paging **2,166 → 1,904 (−262)**; forwarding **170 → 154 (−16)**; operation **41 → 40 (−1)**; redaction **561 → 560 (−1)**; result/construction/docs **881/302/75 → 881/302/75 (0)**.

M2 model **7,465 → 7,436 (−29)**; behavior **8,548 → 8,523 (−25)**. M4 DSL **949 → 977 (+28)**; endpoints **128 → 128 (0)**; lines/endpoint **7.41 → 7.63 (+0.22)**. F3 continuation facts, expanded declarations and Kraken's read attribute account for the increase; F1 adds **0** DSL lines. M5 all SDK negations **0 → 0**. Tokens/endpoint unmeasured.

Hand counts include all nonblank SDK src Rust, comments/docs included; tests/examples/Markdown excluded. The table preserves the pre-fix snapshot. File deltas are base → snapshot; zero-net changed files are omitted. M+B means model + behavior.

| SDK | Glue | M+B | DSL | Source base → snapshot; F1 delta | Files shrinking / growing |
|---|---|---|---|---|---|
| cloudflare-zones | 453 → 445 (−8) | 976 → 976 | 43 → 43 | 2019 → 2011 (−8); −8 | pagination.rs −8 |
| datadog-events | 555 → 544 (−11) | 747 → 738 | 25 → 25 | 1459 → 1438 (−21); −21 | client.rs −15; error.rs −6 |
| digitalocean | 296 → 254 (−42) | 339 → 339 | 19 → 27 | 697 → 663 (−34); −42 | pagination.rs −42 / lib.rs +8 |
| discord-rest | 368 → 375 (+7) | 732 → 721 | 24 → 24 | 1539 → 1535 (−4); −4 | api.rs −4 |
| distribution-registry | 1180 → 1176 (−4) | 1849 → 1845 | 56 → 56 | 3769 → 3761 (−8); −8 | pages.rs −8 |
| github | 1255 → 1131 (−124) | 1585 → 1566 | 150 → 167 | 4094 → 3956 (−138); −7 | link.rs −140; pages.rs −29 / api.rs +18; paging.rs +3; meta.rs +10 |
| gitlab | 719 → 576 (−143) | 727 → 727 | 26 → 27 | 1761 → 1615 (−146); −7 | link.rs −125; pages.rs −22 / api.rs +1 |
| kraken-spot-rest | 306 → 306 (+0) | 607 → 607 | 26 → 27 | 1036 → 1037 (+1); +0 | lib.rs +1 |
| mailgun | 271 → 272 (+1) | 625 → 625 | 25 → 26 | 1028 → 1030 (+2); +0 | lib.rs +1; events.rs +1 |
| microsoft-graph | 563 → 558 (−5) | 355 → 355 | 18 → 18 | 1042 → 1037 (−5); −5 | paging.rs −5 |
| notion | 271 → 220 (−51) | 576 → 594 | 16 → 16 | 937 → 903 (−34); −34 | client.rs −29; error.rs −5 |
| paypal | 361 → 361 (+0) | 1140 → 1135 | 78 → 78 | 1846 → 1841 (−5); +0 | policy.rs −5 |
| sentry | 265 → 265 (+0) | 565 → 557 | 30 → 30 | 973 → 965 (−8); +0 | policy.rs −8 |
| spotify-web-api | 472 → 463 (−9) | 1019 → 1019 | 49 → 49 | 1767 → 1758 (−9); −9 | pages.rs −9 |
| square-payments | 173 → 173 (+0) | 199 → 199 | 26 → 26 | 453 → 453 (+0); +0 | — |
| strava | 248 → 247 (−1) | 862 → 854 | 40 → 40 | 1319 → 1309 (−10); +0 | auth.rs −10 |
| stripe | 743 → 733 (−10) | 2191 → 2191 | 236 → 236 | 3838 → 3828 (−10); −10 | pages.rs −10 |
| zendesk-support | 326 → 326 (+0) | 672 → 664 | 36 → 36 | 1212 → 1204 (−8); +0 | policy.rs −8 |

Snapshot hand total **30,789 → 30,344 (−445)**; merge **30,499**, so F1 removes **155** source lines. Stripe app changes **1** call and **1** doc reference, **0** net source. Discord reclassifies retained behavior as forwarding: glue **+7**, source **−4**. Notion reclassifies retained closures: glue deletion exceeds **34** removed source lines. These measures differ.
Snapshot F1 glue **8,907 → 8,760 (−147)**: paging **−120**, forwarding **−16**, error **−10**, redaction **−1**. F3/E1 remove **253**; **253 + 147 = 400** is additive. Extra quantitative interaction gain is unproven; a separate compiled F1-only spike was outside scope. Audit found **18** twins in **12** SDKs; **17** public twins in **11** SDKs disappear. Mailgun retains its eager twin.

## Simple case

Every pre-existing Nager Holidays/PokeAPI/Frankfurter file is byte-identical to `ba10189b`, including manifests, declarations and caller examples. M6 DSL **8 → 8**, **7 → 7**, **11 → 11**; concepts/public surface stay fixed.
Inherited E1 adds a PokeAPI verification test and operation_effect example. Private read metadata changes for Nager **1**, PokeAPI **2**, Frankfurter **2** endpoints. Existing PokeAPI workflows/canaries pass.

## Costs

M3 F1: starting_at, refuse_repeated_keys, PagesError::Repeated, Outcome::Repeated: **4** library feature units, **0** DSL additions/public traits/types; **147 / 4 = 36.75** incremental glue lines/unit. F3: **1** statement/**2** forms, **3** public traits (Follow/LinkContinuation/UrlContinuation), **3** core types (NextUrl/NextUrlError/Followed). E1: `#[read]`, `Operation::is_read()`.

Combined source convention: **13** units, **400 / 13 = 30.77** snapshot glue lines/unit. Strict core/macros DSL/trait/type inventory: **8**. Moved Link projection adds **2** types (PageLinks/PageLinkError), **2** parse_page_links* functions; Sends gains **1** hidden required method. Including those **5** API costs gives **18** units, **22.22** glue lines/unit. These ratios predate the fix.

Enabled guard: boxed closure, growing HashSet, key clone/hash per admission; storage O(distinct admitted keys), bounded by max_pages when supplied. Each traversal has a fresh set. Unguarded traversal allocates **0** guard/set storage. Sequential grows by an optional field; Pages gains a first-poll marker. One admission decision per key; demand-driven execution remains.

Library nonblank source, tests excluded, base → snapshot (F1 delta): concord-core **4457 → 4645 (+188; +0)**; concord-macros **9487 → 9827 (+340; +0)**; concord-pagination **3898 → 4148 (+250; +110)**; concord **273 → 276 (+3; +0)**.

M7: **0** generated per-endpoint doc words added by F1; inherited endpoint doc shape stays fixed. Builder docs contain **45**/**57** whitespace words; pagination README grows **95** net words. SDK helper docs disappear. M8 compile time, allocation and expanded-line benchmarks are unmeasured; Sync has a regression assertion.
Complete removed-name list follows. Existing first-page signatures stay fixed.

| SDK | Removed public name | Replacement |
|---|---|---|
| GitHub | LinkPages::pages_from(from) | .pages().starting_at(from) |
| Discord | DiscordChannel::history_from(from, limit) | .history(limit).starting_at(from) |
| Cloudflare | CloudflareZonesList::pages_from(from) | .pages().starting_at(from) |
| Cloudflare | CloudflareZoneDnsRecordsList::pages_from(from) | .pages().starting_at(from) |
| DigitalOcean | DigitalOcean::droplet_pages_from(size, cap, from) | .droplet_pages(size, cap).starting_at(from) |
| DigitalOcean | DigitalOcean::tagged_droplet_pages_from(tag, size, cap, from) | .tagged_droplet_pages(tag, size, cap).starting_at(from) |
| DigitalOcean | DigitalOcean::action_pages_from(id, size, cap, from) | .action_pages(id, size, cap).starting_at(from) |
| DigitalOcean | DigitalOcean::snapshot_pages_from(id, size, cap, from) | .snapshot_pages(id, size, cap).starting_at(from) |
| GitLab | GitLabProjectIssues::pages_from(from) | .pages().starting_at(from) |
| Datadog | DatadogEvents::search_pages_from(request, from) | .search_pages(request).starting_at(from) |
| Registry | Registry::catalog_from(options, from) | .catalog(options).starting_at(from) |
| Registry | Repository::tags_from(options, from) | .tags(options).starting_at(from) |
| Graph | Listing::pages_from(from) | .pages().starting_at(from) |
| Notion | NotionClient::query_pages_from(id, query, from) | .query_pages(id, query).starting_at(from) |
| Notion | NotionClient::children_from(id, size, from) | .children(id, size).starting_at(from) |
| Stripe | CursorList::pages_from(from) | .pages().starting_at(from) |
| Spotify | free pages_from(first, from), including crate-root re-export | pages(first).starting_at(from) |

Further F1 breaks: exhaustive PagesError matches need `Repeated { .. }`; non_exhaustive Outcome presentation/mapping must handle Repeated. Datadog removes ProtocolError::RepeatedCursor; Notion removes Error::CursorCycle. Imports/matches migrate to the shared error. Setup after polling panics at the caller.
Datadog/Notion cycles change Failed → Repeated; refusal consumes **0** admissions/provider failures, report `failed = None`. Datadog's **2** successes change `(admitted, succeeded, failed)` **(3, 2, 1) → (2, 2, 0)**; Notion's **3** successes change **(4, 3, 1) → (3, 3, 0)**. Accepted prefixes, HTTP counts and refused positions stay fixed.
Inherited F3 breaks/additions: public `Sends<O>` gains required hidden `__send_followed`; handwritten implementations must adapt; repository implementations are generated. Bodyless GET page-next endpoints gain Follow, the continuation trait and Sends; policy-less dispatch returns declared `Result<T, concord::Error>`. NextUrl preserves original text, withholds it in Debug/Display/errors, and exposes it through explicit/native accessors; bound key formats stay tested.
GitHub LinkPages::new drops prepare; bounds become `Op: LinkContinuation<R> + Clone + Send` (+ `'static` where needed), `R: Listing + Send`. **12** constructor callers change in src/pages.rs, final lines **50, 59, 72, 77, 85, 94, 108, 121, 129, 139, 154, 167**. **13** listings gain continuation support; KeepMeta supplies policy.
GitHub/GitLab Links/LinkError become shared PageLinks/PageLinkError type identities; downstream local-trait impls can overlap. Debug `Links` → `PageLinks`; GitLab NotText suffix `not visible ASCII` → `not visible ASCII text`; its alias gains is_empty. GitLab decodes before Link validation, changing diagnostic priority if both fail; items still wait for validation. decode/api_error become private.
DigitalOcean/Mailgun store private NextUrl fields; generated follow dispatch routes page-two errors through SDK policy. Stored key serialization and eager/context validation remain covered. E1 accepts endpoint/scope read attributes and adds Operation::is_read(); hidden EndpointMeta gains read and declared gains its **third** argument. Hidden-API consumers adapt; `operations Name;` stays opt-in, with corrected SDK public surfaces preserved.
M9: **53 existing Rust calls = 52 SDK test calls + 1 Stripe app call**; **12 README expressions**, including prose/table/comments/examples. Exact before/after ledger: [direction source](/home/canarit/projects/Concord_Next/work/design/directions/ops-paging.md) and [call-migrations.json](/home/canarit/projects/Concord_Next/work/lanes/dir-ops-paging/call-migrations.json). **2** compiled examples added: github-sdk/examples/resume_github.rs and discord-rest-sdk/examples/resume_history.rs. Historical FIRST_USE/FRICTION/MAINTENANCE stay historical. **15** start-delegator calls disappear; Cloudflare's **2** twins called its shared walker in parallel. Imports/Spotify re-export change too.

## What it rules out

Setup after polling needs a fresh Pages. Generic starts carry a key; SDKs retain listing/credential identity. Guarded equality makes admitted keys single-use; tail polling/revisits need an unguarded or fresh traversal. Start stays distinct; detecting the original request URL requires provider context.
Mailgun keeps pages_from: pages() returns `Result<Pages, Error>`; pages_from eagerly validates origin/domain/query/credentials. starting_at cannot replace that synchronous contract. Mailgun's own cycle check covers the original Start URL; replacing it changes timing/prefixes. The fix repeats context checks inside the factory too.
GitHub first-page/self-link, Notion immediate response-cursor and Discord order/size checks remain Rust. They reject invalid responses before items escape; admission-time equality would alter that contract. Only traversal-level Datadog/Notion sets and their dedicated errors disappear. Raw cursor identity obligations remain with callers where already documented; the audit adds no new guarantees. Drive changes already rejects Start without a token.
Inherited [F3-next-url limits](/home/canarit/projects/Concord_Next/work/wt/next/v4/next/spikes/F3-next-url.md), named, not re-reviewed: blocking unconditional Send restriction in policy-less default dispatch, including non-Send inputs without calling send; policy-less streaming untested; page items needs explicit item type/Rust projection; Graph keeps hand dispatch/delta links; wrong-final-type diagnostics can list unrelated From impls; M8 unmeasured. P4 must preserve non-Send declarations, bound dispatch where needed or omit that impl, and add a compile-pass regression. Current page next covers bodyless GET; Datadog POST-body cursors use Rust factories. Caller transport/policy still owns redirects/rewrites; prepare/parts/raw-send escape hatches remain.
Inherited [E1-operation-effect limits](/home/canarit/projects/Concord_Next/work/wt/next/v4/next/spikes/E1-operation-effect.md), named, not re-reviewed: operations Name still selects dispatch (C1); blanket adaptation overlaps downstream generic impls without proving every stable alternative impossible; Datadog's exhaustive HTTP **202** posting classification lacks a targeted future-write fixture (C9); client-level read diagnostic remains generic; final per-client enum-doc comparison/M8 unmeasured. The new paged-search assertion catches a lost read fact, not that separate write-classification limit. Future dispatch simplification must retain read identity; traversal/retry runtime stays outside the DSL.

## Tests

Lane Cargo commands used --locked --offline, target `/home/canarit/projects/Concord_Next/work/targets/slot-ops-paging`, CARGO_BUILD_JOBS=**6**, CARGO_INCREMENTAL=**0**. Fixtures use loopback. Only selected libraries/SDKs/apps/maintenance/canaries ran; no full-workspace gate. Groups overlap and must not be summed.

| Selection | Recorded result |
|---|---|
| Clean F3+E1 merge | **1,256 passed, 0 failed, 4 ignored, 148 suites**; archived `6d943873`, --manifest-path .lane-merged/Cargo.toml; scratch removed |
| Eleven F1 SDKs | **574 passed, 0 failed, 0 ignored, 81 suites** |
| Final concord/pagination/GitHub/Discord/Datadog/DigitalOcean | **830 passed, 0 failed, 0 ignored, 82 suites** |
| GitHub/Stripe apps + maintenance | **50 passed, 0 failed, 0 ignored, 22 suites** |
| Clippy/rustdoc | Library/SDK/app/maintenance selections pass, **0** failures, warnings denied; clippy --all-features --all-targets; docs --no-deps --all-features |
| Fix selection, restored | Pagination/Mailgun/Xero/Strava: **275 passed, 0 failed, 0 ignored, exit 0**; **256** unit/integration + **19** doctests; **4** canaries included; **0** measured/filtered |
| Fix breakdown: unit/integration + doctests; canaries included | Pagination **102 + 11**; Mailgun **43 + 2**, **2** canaries; Strava **63 + 4**, **1** canary; Xero **48 + 2**, **1** canary |

Coordinator's own recheck at `60374269`: `cargo test --all-features -p concord-pagination -p concord-sdk-datadog-events -p concord-sdk-notion`: **211 passed, 0 failed, 27 suites**, exit **0**; [coordinator-check.log](/home/canarit/projects/Concord_Next/work/lanes/dir-ops-paging/coordinator-check.log).
Canary coverage: merge **12 suites / 14 tests**; F1 **11 / 13**; focused SDKs **4 / 5**. **12** new pagination tests cover Start/At equivalence, success/failure/caps/metadata/checkpoints/reports, order/last choice, laziness, poll boundary, cancellation, immediate/long cycles, opt-in, Clone-only keys, prefixes, all consumers, fused streams, redaction and Sync. **2** GitHub tests cover a **3**-page cycle/fragments, binding/headers, withheld cursor and restored/mismatched starts. Discord adds valid descent plus **3** invalid-order cases. Original wire, malformed, empty, oversized, policy/rate/canary and prefix checks remain.
M11: **5** Rust **2024** scratch probes, all expected failures, **0** surprises; primary spans use authored caller code/public names. Wrong String key for Pages<_, u32>: **E0308, wrong_key.rs:5:42**. Missing Eq: **E0277, missing_eq.rs:7:19**. Missing Hash: **E0277, missing_hash.rs:7:19**. starting_at after items: **E0599, after_items.rs:5:27**, points to Pages. Exhaustive old **3** variants: **E0004, exhaustive_error.rs:4:11**, names Repeated. Runtime late-setup assertions cover Pending/page/exhaustion.
Exactly **3** existing F1 expected values change; fix changes **0**:

- Datadog tests/protocol.rs::a_repeated_cursor_ends_the_traversal_before_a_loop: `Some(Error::Protocol(ProtocolError::RepeatedCursor))` → PagesError::Repeated; provider accessor None; `(admitted, failed) = (2, 0)`; original **2** successful pages/**2** requests retained.
- Datadog tests/first_use.rs::caller_searches_pages_and_handles_incomplete_results, repeated branch: old provider repeat error → PagesError::Repeated, provider None, **(2, 0)** counters. Empty-cursor/decode typed errors/status/metadata, accepted items/refused position and **2** requests retained.
- Notion tests/workflows.rs::repeated_and_cyclic_cursors_stop_without_duplicate_requests: `PagesError::Provider { error: Error::CursorCycle, .. }` → PagesError::Repeated; `(admitted, failed) = (3, 0)`; accepted **3**-page prefix/**3** requests retained.

Other F1 test edits migrate calls/formatting or add assertions; existing fixtures/bytes/counts/metadata/serialized keys remain. F3 changes **0** existing expectations/stderr. Inherited E1: endpoint_attribute_unsupported.stderr adds read; operation_docs_spell_out_the_declared_name_and_method adds **false** as declared's **third** argument for Api.repo.remove/DELETE; earlier enum-existence changes were reverted. **4** hidden GET EndpointMeta literals gain `read: true`: header_plan.rs::META, native_headers.rs::full_header_map_with_native_values, native_limits.rs::header_map_capacity, benches/criterion/benches/runtime.rs::plan_scaling.

## Lane's assessment

Deletion is additive; wire evidence supports operation/read/continuation composition. Final reviewed deletion is **397 glue / 434 source** for **18** SDKs, or **397 / 422** for **19** including Xero. Declaration facts stay small; Rust owns starts/admission/traversal. Costs remain **17** removed names, **2** removed errors, an exhaustive arm, guard storage, runtime setup assertions and the Mailgun/F3/E1 limits. Snapshot **400/445** is superseded for the final assessment.

## Review

### Round 1

[ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/direction-ops-paging-review-1.md). Independent session, same provider. CHARTER: C1=n C2=y C3=y C4=y C5=y C6=y C7=y C8=y C9=n.

- Blocking: starting_at bypasses Mailgun query/credential binding, Xero tenant/page-size/modified-since checks and Strava page-zero refusal. Preserve eager checks, repeat them in factories before send, add zero-additional-request regressions and audit remaining factories.
- Non-blocking: Budget incorrectly says keys are never compared/only budgets prevent cycles; SequentialReport omits Repeated. Narrow the unguarded claim and update outcomes.
- Review reproduced the narrower **7**-SDK M1 **4,070 → 3,707 (−363)**, M5 **0 → 0**, including controls. This differs in scope from the **18**-SDK snapshot. Confirmed compatibility/migrations, honest expected changes and no leak through Repeated/Outcome/Debug; Resume redacts, Repeated::source() is None. No builds/tests/mutations/benchmarks or complete factory audit were rerun.

### Fixes after round 1

`46f66793` repeats Mailgun's complete original-URL/query/domain/origin/auth check (LinkError::Context); Xero checks tenant, size, modified-since in that order (TenantMismatch; Input("continuation page size mismatch"); Input("continuation modified-since mismatch")); Strava rejects page **0** (Invalid("page size or starting page")), with immutable per-page validation retained. Eager checks remain. Budget/SequentialReport docs are corrected. No new API/concept/engine behavior, signature/fixture/manifest/lockfile change, weakened assertion or changed expectation.
Audit: **40** engine factories across **36** SDKs, **12** LinkPages wrappers, Stripe's **7** wrappers and Sentry's **2** macro expansions; no further constructor-only bypass. Full audit stays in the [fix result](/home/canarit/projects/Concord_Next/work/lanes/ops-paging-fix-r1/result.md). Existing bounds include Arxiv's **30,000** window, typed nonzero pages, USGS Start offset **1**, and provider request ceilings; raw cursor identity duties stay unchanged.
**3** new regressions pass (**exit 0, 3 passed, 0 failed/ignored**). Mailgun: **1** initial request; recipient/credential/domain refusals keep **1**; valid resume yields **2** items/count **2**. Xero: **2** initial requests; tenant/size/filtered↔plain/sync-time refusals keep **2**; valid resume yields **1** item/count **3**. Strava constructor/selected At(0) keep **0** requests; valid At(**2**) sends **1**, page=**2**.
Each factory-only mutation compiled then failed the new assertion: **exit 101, 0 passed, 1 failed, 0 ignored**; Mailgun line **192**, returned items **2**, **9** filtered; Xero line **262**, items **1**, **7** filtered; Strava line **168**, items **0**, **10** filtered. Eager checks stayed. Sequential mutations restored sources with matching SHA-256; final tests/clippy/rustdoc followed restoration. **8** changed Rust files pass rustfmt edition **2024** checks; lane reports diff checks clean. Four-crate clippy/rustdoc pass with warnings denied, **exit 0**; no workspace-wide run.

### Round 2 (last)

[ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/direction-ops-paging-review-2.md). Independent session, same provider; reviewed `60374269..46f66793`. CHARTER: C1=n C2=y C3=y C4=y C5=y C6=y C7=y C8=y C9=n. Both round-1 findings fixed; audit/regressions/mutation restoration/**275** passes/clippy/rustdoc corroborated; no new code problem. Inherited dispatch-switch/future-write-fixture reservations remain.

- Non-blocking: stale final measurements. This record uses later reviewed **−397/−434** for **18** SDKs and **−397/−422** for **19**, superseding **−400/−445** at `60374269`; the direction source appends the same correction.
- Reviewers did not rerun builds/tests/mutations/benchmarks or freshly review unchanged F3/E1. Their inherited limits remain linked above.
