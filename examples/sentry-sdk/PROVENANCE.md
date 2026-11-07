# Provenance and observable work record

Baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`, confirmed with
`git rev-parse HEAD`. Authoring/retrieval date: **2026-11-07**.
Classification: **example-assisted**. Live provider calls: **NOT_RUN**.
Only documentation was retrieved from Sentry; every executable request targets loopback.
No commits were attempted. Shared source crates, checks, scripts and other examples were not edited.

## Official Sentry pages read

The four endpoint pages are linked directly from the supplied Events & Issues index.
All URLs below were retrieved on **2026-11-07**.

| Exact page | Facts used |
| --- | --- |
| https://docs.sentry.io/api/pagination/ | Link continuations; results=false ends traversal even when a next cursor exists; cursor in URL and optional cursor parameter. |
| https://docs.sentry.io/api/ratelimits/ | Caller/endpoint quota scope, fixed frequency windows, independent concurrency limit, five X-Sentry-Rate-Limit fields, epoch reset units; tokens do not partition caller quotas. |
| https://docs.sentry.io/api/events/ | Read-only endpoint selection and links to the four reference pages below. |
| https://docs.sentry.io/api/events/list-an-organizations-issues/ | Organization-scoped list route, bearer authentication, limit maximum 100, cursor/query, default unresolved search, empty search for all statuses, issue field names, 400/401/403/404 responses. |
| https://docs.sentry.io/api/events/retrieve-an-issue/ | Organization/issue lookup route, issue field names, bearer auth and 401/403/404 classes. |
| https://docs.sentry.io/api/events/list-an-issues-events/ | Organization/issue event listing, per_page maximum 100, cursor/query, summary eventID versus id/groupID, bearer auth and 400/401/403/404 classes. |
| https://docs.sentry.io/api/events/retrieve-an-issue-event/ | Organization/issue/event lookup, special event selectors, full entries field, bearer auth and 400/401/403/404 classes. |

The docs publish no universal numeric frequency or concurrency quota. Defaults (5/s, 1
simultaneous, 3 attempts, 10s scheduling deadline), a 50ms concurrency recheck floor, 1 MiB/16 KiB
body bounds, the 2048-byte next-cursor cap, strict scheduling-header refusals, 429/502/503/504
retries, headless transport retries, and no-jitter backoff are **SDK policies**, not sourced Sentry
guarantees. Retry-After parsing is a generic HTTP courtesy using Concord's existing parser.
Remote ConcurrentRemaining does not specify when another process's request completes; this SDK
uses a cooldown plus its own owned leases and cannot coordinate all provider callers.

## Fixture derivation table

**All fixtures are authored synthetic data**, not captured responses and not verbatim Sentry
examples. Field names and header grammar were informed by the pages above. Values, identifiers,
titles, errors, response sequences and failure cases were chosen independently of SDK code.
Tests assert literal documented routes, independent ids and fixture values; they do not call
SDK URL builders to create their expected wire targets.

| File under fixtures/ | Derivation and purpose |
| --- | --- |
| `issues-1.json` | Synthetic first issue, using list-organization-issues field names; id 101. |
| `issues-2.json` | Synthetic second issue, same field vocabulary; id 202 is selected for detail. |
| `issue.json` | Synthetic detail for 202; retrieve-an-issue fields, extra project/count fields test tolerant decoding. |
| `events-1.json` | Synthetic event summary; list-issue-events fields; storage-a differs from the 32-character eventID. |
| `events-2.json` | Synthetic second event summary; storage-b differs from eventID. |
| `event.json` | Synthetic second event detail, retrieve-issue-event vocabulary; authored exception entry and secret sentinel. |
| `problem.json` | Synthetic `detail` error body. Only HTTP status classes are sourced; this text/shape is an SDK-supported fixture, not a claim all Sentry errors have it. |
| `wrong-shape.json` | Synthetic numeric id and missing required fields; rejects success and error-document schema mismatch. |
| `malformed.json` | Authored invalid JSON with secret text to test safe parse diagnostics. |
| `truncated.json` | Authored unfinished JSON without a final newline/closing brace. |
| `empty.json` | Authored empty array; an empty page with a true continuation still advances. |
| `next.link` | Pagination-doc grammar rewritten as relative references; previous=false, next=true, URL/parameter cursor agreement. Offset 1 is synthetic, not an inferred provider cursor algorithm. |
| `last.link` | Synthetic next cursor with results=false; verifies no terminal fetch. |
| `concurrency.headers` | Rate-limit-doc field names; synthetic quota numbers, past reset=1, concurrent limit=1/remaining=0, generic Retry-After=0. Frequency capacity remains available while concurrency is exhausted. |

Runtime-authored synthetic variants in `tests/protocol.rs`: 400/401/403/404/429/5xx/418/302,
missing/duplicate/wrong media, non-UTF8/empty bytes, full and chunked oversized spaces,
unterminated chunked bodies (`Body::FailAfter`), disconnect before a head, invalid/overflowing
quota fields, malformed/foreign/conflicting Link values, and redaction sentinels.
Runtime-authored variants in `tests/workflows.rs`: empty advancing page, multiple Link lines,
URL-only cursor, terminal foreign target that is never followed, failed second page,
gated/stalled heads/bodies, epoch reset from test wall time, rate-limit attempt exhaustion,
transport/gateway recovery, Retry-After minimum, and deadlines during backoff/admission.
These variants are deliberately adversarial fixture contracts, not additional provider claims.

## Local documentation and rustdoc read

- Required: root `README.md`; `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
  `docs/CHEATSHEET.md`, `docs/RECIPES.md` (including embedded example snippets).
- Crate READMEs: `concord-core`, `concord-macros`, `concord-pagination`, `concord-retry`,
  `concord-auth`, `concord-ratelimit`, `concord-codecs`; root facade Cargo feature definitions.
- Rustdoc read in source: `crates/concord/src/lib.rs`; `concord-core/src/{lib,prepared,response,
  error,field}.rs` for Endpoint/SendWith/Received/error/metadata/field contracts;
  `concord-codecs/src/{lib,json}.rs`; `concord-macros/src/lib.rs`;
  `concord-pagination/src/{lib,pages,types,link}.rs`;
  `concord-retry/src/{lib,concord}.rs`; `concord-ratelimit/src/lib.rs`;
  `concord-auth/src/{lib,credentials}.rs` (its crate rustdoc includes its README).
- Implementation portions inspected to verify observable boundary behavior:
  `concord-core/src/__private.rs` path/trailing-slash assembly;
  `concord-ratelimit/src/lib.rs` checked admission arithmetic and lease lifecycle.
- Build metadata read: root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and manifests
  for the shared crates used. Filesystem inventory found no `AGENTS.md` in this worktree.

## Every example file consulted

| Exact file | What was taken |
| --- | --- |
| `examples/frankfurter-sdk/src/lib.rs` | One generated-send policy for the whole client; bounded `Received::document::<Json, Problem>` on non-2xx; `decode_with_meta` on success; boxed provider errors. No provider fields, fixtures or expectations copied. |
| `examples/testserver/src/lib.rs` | Required `TestServer::start` responder API, Recorded wire accessors, Reply/Body/Gate scripting, wait_until counters, cancellation/cleanup semantics. Used the existing harness without modifying it. |
| `examples/testserver/Cargo.toml` | Dev-dependency package name and compatible Tokio test features. |

No other SDK example source file was opened. Names of other examples appeared in repository
inventory and in required guide links/snippets; those files were not consulted directly.

## Dependency and scope audit

No new registry crate was added. All dependencies already existed in Cargo.lock:
Concord facade/auth/pagination/retry/ratelimit, serde, serde_json, Tokio and the local testserver.
The lockfile delta adds only `concord-sdk-sentry` version 0.1.0 with its dependency list;
no other package version changed. Thus no new crate name/version/license justification is needed.
The workspace edit adds only `examples/sentry-sdk` to `members`.
No native request URL edits, raw sends, new codec or duplicated retry/admission engine were needed.

## Validation record

All cargo commands ran sequentially from the worktree root, with `CARGO_BUILD_JOBS=4`, using
the default `target/`; no CARGO_TARGET_DIR override was used.

- `cargo test --offline -p concord-sdk-sentry`: initial unlocked bootstrap passed, resolving
  the new local package entry offline (library and README compilation before integration tests).
- `cargo fmt -p concord-sdk-sentry`: passed.
- `cargo test --locked -p concord-sdk-sentry`: passed with 14 protocol tests, 15 workflow tests,
  and one compile-only README doctest. An earlier run passed 13+14 integration tests before
  the final admission/attempt-bound cases were added.
- `cargo clippy --locked -p concord-sdk-sentry --all-targets -- -D warnings`: passed,
  no warnings.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-sentry --no-deps`:
  passed, generated the SDK's public API docs.
- `git diff --check`: passed. A TOML package-record comparison verified that every prior
  Cargo.lock package record is identical, with only the new local package added. A manifest
  comparison verified the workspace member is the only semantic root-manifest change.

Final authored-file counts (all lines, including comments/blank lines; an unterminated final
fixture line counts once): `src/`: 6 files, **785 lines**; `tests/`: 3 files, **1185 lines**
(protocol 550, workflows 608, support 27); `fixtures/`: 14 files, **80 lines**.
The package also supplies Cargo.toml, README.md, this provenance and FRICTION.md.

No compiler or test failures occurred during the initial build and test runs. The queued
admission test was strengthened from an at-most-one-dispatch check to a stalled-body fixture
that proves exactly zero dispatches before the waiting hook's deadline.

## Maintenance: issue status update (2026-11-07)

Added by the independent maintainer (not the author or the first consumer); the full record
is `MAINTENANCE.md`. Live provider calls remain **NOT_RUN**. Classification of the new code:
**docs-assisted** (Concord guides, Concord source/rustdoc and the existing SDK); no other SDK
example was opened.

Official pages read on **2026-11-07** through a page-fetch tool that returns a converted,
summarized page (it declined verbatim reproduction, so facts below are as it reported them):

| Exact page | Facts used |
| --- | --- |
| https://docs.sentry.io/api/events/update-an-issue/ | `PUT /api/0/organizations/{organization_id_or_slug}/issues/{issue_id}/`; "only the attributes submitted are modified"; all body parameters optional; `status` one of `resolved`, `unresolved`, `ignored`, `resolvedInNextRelease`, `muted`; top-level integer `ignoreDuration` (minutes), `ignoreCount`, `ignoreWindow` (minutes over which `ignoreCount` is measured, maximum 7 days), `ignoreUserCount`, `ignoreUserWindow` (maximum 7 days); `statusDetails` described only as additional resolution details, with **no sub-fields listed**; bearer token with `event:write` or `event:admin`; response codes 200/400/401/403/404 with **no response body example**. |
| https://docs.sentry.io/api/events/bulk-mutate-an-organizations-issues/ | Linked from the Events & Issues index; read only to look for `statusDetails` sub-fields. Same status/ignore parameters; `inRelease`, `inNextRelease`, `inCommit` appear only inside a **response** example, so they were not adopted as request fields. |
| https://docs.sentry.io/api/ratelimits/ | Re-read: limits per caller and endpoint; frequency and concurrency are separate; the page does not distinguish reads from writes. |
| https://docs.sentry.io/api/pagination/ | Re-read: unchanged; the update returns no Link continuation. |
| https://docs.sentry.io/api/events/ | Re-read: index lists "Update an Issue" as its own endpoint page. |

Not sourced (SDK policy or ticket statement): the 200 body being the full updated issue (the
ticket says so; the page shows no body), refusing zero counts/durations/windows, offering
one ignore condition per change, omitting `muted`/`substatus`/`snoozeDuration`, a separate
local bucket for the update, retrying only 429 for the update, and treating a headless update
as uncertain. A 429 being a rejection rests on the rate-limit page's statement that requests
over the limits are rejected.

| File under fixtures/ | Derivation and purpose |
| --- | --- |
| `ignored.json` | Synthetic issue 202 after an ignore; `status`/`substatus`/`statusDetails` field names from the pages above, values authored; extra fields test tolerant decoding. Not a captured response. |

Runtime-authored variants: `tests/update.rs` (status echo from the existing `issue.json`, hang-up,
held heads for client timeout and SDK deadline, 429 with Retry-After, 5xx with `problem.json`,
invalid quota field on a 200, provider window exhaustion) and `tests/status_update_use.rs`
(a stateful synthetic issue store that applies a PUT before answering and holds one answer).

No dependency was added; `Cargo.lock` and the root `Cargo.toml` are unchanged (the package was
already a workspace member). `serde_json` was already a normal dependency of this package.
