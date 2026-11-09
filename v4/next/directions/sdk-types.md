# Direction: SDK types — secrecy and wire spelling declared on SDK-owned values (D1 + I1)

Condensed from the [direction source](/home/canarit/projects/Concord_Next/work/design/directions/sdk-types.md), [fix result](/home/canarit/projects/Concord_Next/work/lanes/sdk-types-fix-r1/result.md) and reviews below. Lane: Codex `gpt-6.1-sol` xhigh, confirmed by [observed metadata](/home/canarit/projects/Concord_Next/work/lanes/dir-sdk-types/meta.json); this supersedes the source's earlier claim that model/effort metadata was unavailable. Reviews: independent session, same provider.
Branch `direction/sdk-types` at `6eb75e68`, pushed as `spike/dir-sdk-types`; base `ba10189b` at `/home/canarit/projects/Concord_Next/work/wt/rv-p1`; starting D1/I1 merge `6b5a60ce`; round-1 tree `b40523e5`. Worktree: `/home/canarit/projects/Concord_Next/work/wt/dir-sdk-types`. The later reviewed commit supersedes the lanes' uncommitted-tree notes.

**Status: works.** Two separate derives describe logging and scalar wire spelling on SDK Rust values. The final inventory is **47 Serialize additions and 106 frozen enum spellings**. The missing-Serialize diagnostic still fails C5.

## Shape

Stripe's IdempotencyKey replaces manual Display/Debug with:

```rust
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize,
         concord::Debug, concord::WireDisplay)]
#[serde(transparent)]
pub struct IdempotencyKey(#[sensitive] String);
```
Construction, validation and `expose()` stay Rust. Serialize supplies raw wire text; sensitive controls Debug only. Compact output stays `IdempotencyKey(<redacted>)`; pretty tuples expand. Existing `#[sensitive] idempotency_key?: IdempotencyKey` placement and caller `.set_idempotency_key(key).prepare()?` stay fixed. Renamed facades use `#[wire_display(crate = concord4)]`; `crate::facade` also works.
Carried implementation: `debug.rs` **329** source lines, `wire_display.rs` **40**, `wire_text.rs` **180**, plus registrations/exports/docs/tests. The cost is roughly **550** implementation lines; concord-core is unchanged. The serde bridge needs json/form/csv/jsonl. No new dependency or lockfile change is reported. GitHub adds redacted WithMeta/NextLink; Stripe removes public_debug and its **10** calls; GitLab marks PrivateToken/NextPage. The extension replaces **18** expanded Debug impls and **1** Display: GitHub **2**, Stripe **14** plus IdempotencyKey Display, GitLab **2**. **10** Stripe impls were expressible by standard Debug already.

## What disappears

Hand counts cover nonblank Rust under SDK src, including attributes/declarations and macro bodies once. Tests/examples are excluded. Classifier glue is separate. The table preserves the source's pre-fix figures; the fix changes tests only.

| SDK | Source base → final (removed; extra after merge) | Glue | Model + behavior | DSL | Net removed by file | Merge → final tests |
|---|---|---|---|---|---|---|
| salesforce | 5,464 → 5,364 (100; 0) | 941 → 881 | 3,162 → 3,151 | 81 → 81 | bulk_query.rs 15; context.rs 7; csv.rs 26; error.rs 11; ids.rs 10; model.rs 16; soql.rs 15 | 45 → 45 |
| twilio | 529 → 510 (19; 0) | 281 → 250 | 190 → 203 | 16 → 16 | auth.rs 5; error.rs 6; models.rs 8 | 52 → 52 |
| elasticsearch | 2,206 → 2,128 (78; 0) | 551 → 497 | 1,491 → 1,469 | 23 → 23 | bulk.rs 16; document.rs 7; error.rs 13; model.rs 23; pit.rs 19 | 42 → 43 |
| backblaze-b2-native | 2,473 → 2,438 (35; 0) | 986 → 947 | 1,270 → 1,274 | 44 → 44 | error.rs 8; model.rs 27 | 52 → 52 |
| quickbooks-online | 2,518 → 2,484 (34; 0) | 500 → 460 | 1,672 → 1,678 | 51 → 51 | auth.rs 26; error.rs 8 | 48 → 48 |
| xero-accounting | 1,656 → 1,600 (56; 0) | 329 → 267 | 1,199 → 1,208 | 26 → 26 | auth.rs 5; error.rs 4; lib.rs 13; models.rs 25; store.rs 9 | 49 → 49 |
| github | 4,094 → 4,043 (51; 13) | 1,255 → 1,242 | 1,585 → 1,563 | 150 → 150 | ids.rs 15; meta.rs 6; models.rs 4; paging.rs 7; params.rs 19 | 62 → 64 |
| stripe | 3,838 → 3,720 (118; 45) | 743 → 718 | 2,191 → 2,124 | 236 → 236 | ids.rs 32; lib.rs 41; send.rs 7; types.rs 42; models.rs grows 4 | 31 → 33 |
| gitlab | 1,761 → 1,694 (67; 10) | 719 → 710 | 727 → 670 | 26 → 26 | models.rs 8; pages.rs 5; params.rs 54 | 63 → 65 |
| kubernetes | 2,674 → 2,649 (25; 0) | 483 → 483 | 1,464 → 1,440 | 105 → 105 | api.rs 9; ids.rs 16 | 32 → 33 |
| cloudflare-zones | 2,019 → 1,997 (22; 0) | 453 → 453 | 976 → 968 | 43 → 43 | models.rs 22 | 65 → 67 |

All SDK negations remain **0 → 0**. Hand total **29,232 → 28,627 (−605)**; published parts **537** (D1 **294** + I1 **243**), extension **68** source / **47** glue. Elasticsearch parts add **50 + 28 = 78** source lines. GitHub **51 vs 38**, Stripe **118 vs 73**, GitLab **67 vs 57** reflect broader adoption. There is no demonstrated super-additive mechanism benefit; an extended D1 could remove the same redaction blocks.

| Aggregate, including controls | Base → final |
|---|---|
| M1 glue | 7,576 → 7,243 (−333); merged parts removed 286 |
| Glue categories | Redaction 618 → 276 (−342); error 4,202 → 4,204 (+2); result 654 → 656 (+2); construction 267 → 269 (+2); paging 1,524 → 1,527 (+3); operation/forwarding/docs 5/270/36 unchanged |
| M2 | Model 7,456 → 7,309 (−147); behavior 8,718 → 8,686 (−32); combined 16,174 → 15,995 (−179); other 3,776 → 3,683 (−93) |
| M4 / M5 | DSL/endpoints 827/129 → 827/129; lines/endpoint 6.41 → 6.41; negations 0 → 0 |

## Simple case

Nager, PokeAPI and Frankfurter directories are byte-identical, including manifests/tests/docs. DSL stays **8/7/11**, with the same source and concepts. Tests/canaries pass: **31 → 31**, **26 → 26**, **50 → 50**.

## Costs

M3: **2** public derives, **166.5** classifier glue lines removed per derive under that narrow convention. Reused sensitive marks fields/types/variants; inherited public selects visible named fields inside sensitive containers; wire_display(crate = path) is optional. No DSL/core trait/core type additions. The hidden serde bridge is exported support. M7: endpoint docs unchanged; extension adds **0** product-doc words. Inherited D1 counted about **121** item-doc and **32** crate-doc words; no combined census. M8 compile time, expansion and allocations are unmeasured; direct formatter streaming is inspection evidence.
Complete Debug compatibility inventory follows. Nested values inherit these changes. Pretty builders propagate pretty formatting; snapshots/parsers may need migration.

| SDK | Every recorded Debug change |
|---|---|
| salesforce | AccessToken compact stays fixed, pretty tuple expands. Attributes, ApiErrorDoc, QueryLocator, ResultLocator, CsvError::Source, FieldText, JobFailure, ResultLink, NextResultSet::At use `<redacted>` instead of old placeholders/quotes; ApiErrorDoc follows field order; Last/Unknown keep unit names. Account/Contact show all declared fields with sensitive contents hidden. ContactExportRow exposes optional presence. ManifestCursor adds redacted locator before next_ordinal and loses ellipsis. Literal::{Text,Pattern,Id,Integer,Bool,Date,DateTime} become Variant(<redacted>), dropping Literal::; Literal::Null becomes Null. |
| twilio | BasicAuth compact stays fixed, pretty tuple expands. CreateMessage remains opaque in both modes. ProviderError changes message marker and exposes optional more_info presence. Message lists every field, with body/from/to/error_message presence and a redacted account_sid. |
| elasticsearch | ApiKey pretty tuple expands; compact stays fixed. Sort marker changes. SearchBody adds track_total_hits/query presence. Hit/Document expose sensitive optional presence while withholding contents. BulkReport adds limits/refresh. ErrorCause label type becomes kind; nested kinds appear, reasons stay hidden. |
| backblaze-b2-native | FileVersion/ErrorDocument marker text/quotes become `<redacted>`. ApplicationKey tuple becomes `{ credentials: <redacted> }`. UploadTarget replaces synthetic upload_url/token with real optional redacted current. |
| quickbooks-online | RefreshToken/AuthorizationCode compact stays fixed, pretty expands. OAuthApp client_secret becomes credentials and token_base appears. Private Grant follows declaration and shows public redirect_uri. Private TokenResponse becomes `TokenResponse { .. }`. FaultError follows declaration order and redacted markers. |
| xero-accounting | Grant compact stays `Grant(<redacted>)`; pretty changes from flat text to `Grant(\n    <redacted>,\n)`. TenantId changes marker and pretty layout. Auth (private), FileStore, Xero, Problem, Tenant, PageCursor, InvoiceRef, ContactRef, LineItem, DraftInvoice, Invoice change opaque tuple placeholders to `Type { .. }`. |
| github | `WithMeta<T>` value marker changes withheld → redacted, without a `T: Debug` bound. NextLink changes URL marker and puts url before binding; URL remains hidden. |
| stripe | CustomerId, SubscriptionId, SubscriptionItemId, InvoiceId, InvoiceLineItemId, ProductId, PriceId, BalanceTransactionId, TaxRateId, PaymentMethodId stay equivalent in both modes. AccountId/SecretKey/IdempotencyKey compact stays fixed; pretty becomes `Name(\n    <redacted>,\n)`. ApiError message/param quoted withheld markers become redacted inside Some; doc_url newly exposes presence only; kind/code rendering/order stays fixed. |
| gitlab | PrivateToken compact stays fixed, pretty tuple expands. NextPage changes `NextPage(<withheld>)` → `NextPage { .. }` in both modes. |

Complete Display flag inventory: **24 types/branches** below stop forwarding outer integer formatting flags. Default spelling/request bytes stay fixed. For **7**, `{:>6}` changes `"     7"` → `"7"`, `{:+}` `"+7"` → `"7"`, `{:06}` `"000007"` → `"7"`; alignment/fill/width also change.

| SDK / file | Affected types or branches |
|---|---|
| github ids.rs / params.rs | RepositoryId, IssueId, IssueNumber, PullRequestId, PullNumber, CommentId, ReviewId, UserId, LabelId, MilestoneNumber, WorkflowId, RunId, JobId, ArtifactId, CheckSuiteId, WorkflowRef::Id; PerPage |
| gitlab params.rs | ProjectId, IssueIid, IssueId, PerPage, ProjectRef::Id |
| cloudflare-zones models.rs | ZonesPerPage, RecordsPerPage |

WorkflowRef::File/ProjectRef::Path already wrote raw strings. Stripe Timestamp/Limit already ignored outer flags; IdempotencyKey retains that behavior. Kubernetes/Elasticsearch and other string/enum formatters add **0** flag changes. Error-message Display stays manual.
Complete Serialize additions (**47**): an API expansion that permits explicit disclosure of sensitive values. Debug sensitivity does not govern serialization/Display. Existing Serialize contracts stay fixed.

| SDK (count) | Every newly serializable type |
|---|---|
| github (19) | WorkflowRef; State, StateReason, AuthorAssociation, ReviewState, RunState, Conclusion, FileStatus; StateFilter, IssueSort, PullSort, CommentSort, Direction, RunStatus, JobFilter, ArchiveFormat; PerPage, ApiVersion, ETag |
| stripe (5) | SubscriptionStatusFilter, PriceTypeFilter, Limit, ApiVersion, IdempotencyKey |
| gitlab (9) | ProjectId, IssueIid, IssueId, ProjectRef, IssueState, IssueOrder, SortOrder, PerPage, IssueStatus |
| kubernetes (1) | ResourceVersionMatch |
| cloudflare-zones (11) | ZoneId, AccountId, RecordId, ZoneStatus, ZoneType, RecordType, ZoneOrder, RecordOrder, Direction, ZonesPerPage, RecordsPerPage |
| elasticsearch (2) | Action, Refresh |

M9: **0** changed application calls. Constructors, validation, visibility, layouts, navigation and placements stay fixed. D1's default prints newly added unmarked fields. Sensitive containers with public fields supply the alternative default. Syntactic Option preserves presence; aliases are opaque. Marked fields skip formatting and need no Debug bounds. Unmarked native types retain their own behavior. AccountId/IdempotencyKey/ContinueToken intentionally remain displayable as raw secrets.
M11 has **5 seeds per derive**. D1: standard Debug plus sensitive gives an unknown-helper error/misleading sanitize suggestion; retained manual Debug gives E0119; marker before derive rejects; public outside a sensitive container rejects; shown field without Debug gives E0277 at its type. No Concord internals appear, though standard format_args may appear. I1: missing Serialize gives E0277/help naming `concord::__macro::Serialize` (C5); runtime option, duplicate crate and string-instead-of-path reject at authored tokens; retained manual Display gives E0119 (temporary consumer line 3, manual impl line 13). That last seed survives only in its log.
Composite serde values compile but fail formatting at runtime. Placed values become placement-specific Build/NotSent without canaries. A caller-owned formatter can retain a partial prefix on failure; request-build errors discard it. Computed summaries, synthetic labels, selected fields and exact legacy formats remain manual: GitHub metadata/link/artifact summaries, Stripe context/cursors, GitLab byte-count/status, joined labels, RepoRef, duration units, provider prose and Stripe's form encoder. `Conditional<T>` stays manual to preserve bound-free Debug; deriving would require `T: Debug`. Serde-free SDKs such as S3 need a dependency to adopt I1.

## What it rules out

Serialize is the canonical scalar spelling. Different JSON and placement spellings need manual Display or a wrapper; context-dependent behavior stays Rust. Arrays/maps/records/tagged payloads/None/unit do not have derived scalar Display. Compound wire rules would require another design choice. Text/Binary-only users retain manual formatting unless enabling a serde codec.
Exact legacy logging competes with derivation. Option presence and D1's defaults become observable contracts. A custom formatting language would add concepts. SDK values own these facts; endpoint errors, envelopes, cursor placement and sends remain outside this direction.
`serde_plain::derive_display_from_serialize!` is a credible **0-Concord-concept** alternative with another dependency and panic-on-conversion-failure behavior, as discussed in the source. A Rust wrapper could compete on error handling too. I1's narrower case is streaming by inspection, renamed facades and tested fmt::Error → Build/NotSent placement errors; matching names and line savings alone do not justify it.

## Tests

Lane commands used `--locked --offline`, CARGO_BUILD_JOBS=**6**, CARGO_INCREMENTAL=**0**, target `/home/canarit/projects/Concord_Next/work/targets/slot-sdk-types`. Full-workspace validation remains with the coordinator. Results overlap and must not be summed.

| Selection | Recorded result |
|---|---|
| Merged → extended direction | **1,406 → 1,416 passed; 166 → 172 suites; 2 existing ignores; exit 0; 0 final warnings; 14/14 SDK/control canaries**. concord 494 → 494; codecs 227 → 227; macros 37 → 37; SDK/control counts above. Ignores: measure_throughput and Encoded docs. |
| Exact concord/macros/codecs | **539 passed, 1 ignore, 63 suites** |
| Minimal-feature Debug | **8 passed**, including UI runner |
| Renamed consumer | Wire-display **4 passed**; extras-only default **3 passed**; retained-manual-Display probe E0119, **exit 101**; temporary source removed |
| Checks | Selected clippy with warnings denied and fmt pass; source reports diff/lockfile checks clean. No independent round-1 clippy/rustdoc run; M8 unmeasured. |
| Fix selection | GitLab **66 passed**, Elasticsearch **44 passed**; **110 total, 0 failed, 0 ignored, 16 suites, exit 0, 0 warnings** before mutations and after restoration. Each has **2** canary tests (**4** total); GitLab includes **9** doctests. |
| Mutations | Removing NextPage sensitivity or renaming Action::Index to index_mutated each gives **exit 101, 1 failed test** at the new guard. Sources restored byte-for-byte; formatting/diff checks pass. |

The extension added **6** sdk_types.rs files / **10** tests. Frozen literals include GitHub **40**, Cloudflare **41**, both Kubernetes ResourceVersionMatch literals, GitLab closed/status/open values and Elasticsearch's **3** Refresh literals. The fix freezes **4** Action literals: Index=`index`, Create=`create`, Update=`update`, Delete=`delete`. Corrected total: **106**, replacing **102**. GitLab's real continuation retains its canary through raw_url(), hides it in compact/pretty Debug, sends it on the wire, reaches a terminal page, and makes exactly **2** requests. Stripe covers raw/JSON/ignored-width spelling, keys and error redaction; GitHub covers `WithMeta<Unprintable>`. Base literal tables are independent of serde metadata.
Exactly **2** pre-existing expected values changed in D1; the extension/fix change **0**, weaken **0** tests and change **0** diagnostic snapshots:

- Salesforce `src/ids.rs::locators_are_redacted_in_debug`: `"QueryLocator(..)"` → `"QueryLocator(<redacted>)"`. Adjacent wire assertion stays `"01gD0000002HU6KIAW-2000"`.
- Twilio `tests/protocol.rs::message_debug_shows_identity_and_outcome_but_not_contents`: `Message { sid: "SM_fixture_message", status: Undelivered, error_code: Some(30007), .. }` → `Message { sid: "SM_fixture_message", account_sid: <redacted>, status: Undelivered, body: Some(<redacted>), from: Some(<redacted>), to: Some(<redacted>), error_code: Some(30007), error_message: Some(<redacted>) }`. Canary/response assertions remain.

New golden files: **7** ui_debug cases (kept_manual_impl, marker_before_derive, markers, missing_debug, public, std_derive_marker, union) and **2** ui_json cases (wire_display_attribute, wire_display_missing_serialize). The inherited unsized-field correction retains the &Opaque bound note. Trybuild requires every expected diagnostic; existing request/wire/validation/oracle/canary assertions stay fixed. The initial abbreviated-package invocation and ContinueToken::new fixture failed; corrected package names/ContinueToken::from_server produced the recorded passes.

## Lane's assessment

Broader adoption removes **605** source / **333** glue lines versus **537** / **286** for published parts, with **3** unchanged controls. The mechanisms are additive. One sensitivity marker and one serde spelling replace repeated formatters while calls stay fixed. D1 earns its place; I1 has the narrower error-contract case and a credible external alternative.

## Review

### Round 1

[ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/direction-sdk-types-review-1.md). Independent session, same provider. CHARTER: C1=y C2=y C3=y C4=y C5=n C6=y C7=y C8=y C9=n.

- Blocking: NextPage sensitivity had no retained guard; obtain a real secret-bearing continuation and check both Debug modes.
- Blocking: Action spelling was not frozen; ActionLine wire tests exercise another enum. Freeze all four base literals.
- Non-blocking: Serialize inventory omitted Action (**47**, not **46**); Xero Grant's pretty expansion was omitted. Correct both.
- Non-blocking: missing-Serialize help exposed `concord::__macro::Serialize`; keep the C5 limit until public diagnostics replace it.

### Fixes after round 1

`6eb75e68` adds GitLab `continuation_debug_withholds_the_link_in_both_modes` and Elasticsearch `base_action_literals`, with both mutations failing the new assertions. Only two test files change; production/lockfile/diagnostic/Xero files stay fixed. Inventory replacements record **47** Serialize additions, Action, Xero compact/pretty behavior and **106** spellings. The fix lane's starting/uncommitted `b40523e5` note is superseded by the reviewed commit.

### Round 2 (last)

[ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/direction-sdk-types-review-2.md). Independent session, same provider. CHARTER: C1=y C2=y C3=y C4=y C5=n C6=y C7=y C8=y C9=y. Both blocking findings are fixed.

- Non-blocking: main inventory still had 46/102 and the wrong Xero statement when reviewed; the supplied direction source applied corrections afterwards. This record uses its later **47/106** inventory and the fix result.
- Non-blocking: private Serialize diagnostic remains. Recorded limits also include runtime scalar refusal/partial formatter output, fail-open unmarked Debug fields and optional-presence changes, manual summary/generic-bound cases, serde feature/dependency costs, unmeasured M8 and no full-workspace gate. Reviewers did not execute builds/tests/mutations independently; final **110** passes are lane evidence.
