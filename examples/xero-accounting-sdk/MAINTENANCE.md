# Maintenance — SDK35 ticket: incremental "modified since" invoice listing

Date 2026-11-07. Role: independent maintainer, not the author or the fresh consumer. Observed model id
from the harness system prompt: `claude-opus-5-5`. Requested model and effort were not visible to me
and are recorded as *not observed*. Baseline `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e` plus the
author (`39d878b5`) and consumer (`ab78bdf1`) commits. Live Xero calls: **NOT_RUN**. No shared crate,
check, script, other example, `Cargo.toml` or `Cargo.lock` changed. Nothing was committed.

## Prior exposure

Before attempting the ticket I read: the role brief; the injected global/project `CLAUDE.md` and the
two-line memory index (not the memory files); all six `src/*.rs` files; `tests/support/mod.rs`,
`tests/workflows.rs` and `tests/protocol.rs` in full; `tests/first_use.rs` lines 1–615 and 1214–1303
plus a grep of its function names and API uses; `README.md`, `PROVENANCE.md`, `VALIDATION.md` and
`Cargo.toml`; a grep of the root manifest and lock. For Concord I read `docs/CHEATSHEET.md` in full,
`docs/AUTHOR_GUIDE.md` §2, §4.8 and §6, later also §4.2–4.6 and §9, the
`Pages`/`PagesError`/`Incomplete` definitions in `crates/concord-pagination/src/pages.rs`, the public
item list of `examples/testserver/src/lib.rs`, and a grep of `crates/concord-retry/src/http.rs`. I read
`FRICTION.md` and `FIRST_USE.md` only **after** the implementation, its tests and the first mutation
run were complete. I read nothing under `work/briefs`, `work/roles`, `work/records` or other `work/w3-*`
worktrees, and opened no other SDK.

## Provider grounding (read 2026-11-07, ~05:10 UTC; method and hashes in PROVENANCE keys XM, XR2)

- [Invoices](https://developer.xero.com/documentation/api/accounting/invoices): the "Modified After"
  filter is the `If-Modified-Since` HTTP header. Its value is a UTC timestamp `yyyy-mm-ddThh:mm:ss`
  (example `2009-11-12T00:00:00`), and invoices created or modified since that time are returned.
  Optimised ordering is by InvoiceId, UpdatedDateUTC or Date. The default order is
  `UpdatedDateUTC ASC, InvoiceId ASC`.
- [HTTP requests and responses](https://developer.xero.com/documentation/api/accounting/requests-and-responses/):
  the filter is "accurate to the second". Not every change updates `UpdatedDateUTC`. Paging and the
  `pagination` object work as before.
- Neither page says whether the bound is inclusive, whether a collection can answer `304`, or whether
  RFC 9110 HTTP-dates are accepted. The SDK relies on none of these.

## What changed for callers

New, additive API. Every existing signature, wire request and assertion is unchanged.

- `ModifiedSince`: a tenant-bound sync point (`Clone`, `Eq`). `at() -> SystemTime` returns the instant
  truncated to the second, for persistence. `tenant()` returns the owning tenant. `Debug` shows only
  the UTC text.
- `Tenant::modified_since(SystemTime)` is the only constructor. It truncates to the whole second and
  refuses instants before 1970 or after 9999-12-31T23:59:59, before any I/O (`Input`).
- `Tenant::list_modified_since(&since, page, size)` and
  `Tenant::pages_modified_since(&since, from, size, cap)` are the existing `list`/`pages` (same bounds,
  metadata validation, legacy continuation, cap, `Incomplete`, `Resume`) plus an `If-Modified-Since`
  header. A `since` or cursor from another tenant is refused with `TenantMismatch` before I/O.
- `PageCursor` now also records the traversal's sync point. A filtered cursor cannot resume `pages`,
  an unfiltered one cannot resume `pages_modified_since`, and a cursor cannot resume with a different
  `since` (`Input`). `since` is passed again on resume because a failed **first** page yields
  `Resume::Start`, which has no cursor to carry it.
- README section "Incremental sync", with a compiled `no_run` example: record `SystemTime::now()`
  before the first request, and advance only after completion.

Rejected alternatives:

- A text constructor or `Deserialize`. Text without an offset cannot prove it is UTC, and
  `Deserialize` would recreate the validation bypass FIRST_USE F8 found in `TenantId`.
- Rounding instead of truncating. Rounding up can skip changes within the starting second.
- Adding `order=InvoiceId` (see Open questions).
- `chrono`, which is already locked through other example SDKs. It would add a dependency edge for
  about 15 lines of arithmetic that are independently tested.

The first implementation declared `header("if-modified-since") modified_since?: String` and formatted
at the call site. It was then changed to `modified_since?: Utc` with
`headers {"if-modified-since"="{modified_since}";}`. Concord omits the header when the optional value
is absent (AUTHOR_GUIDE §4.3: a source is present when every optional value it reads is present).
With that change, even internal SDK code cannot put an unvalidated string on the wire.

## Files and locations edited

| File | Location | Change |
|---|---|---|
| `src/models.rs` | imports; new `Utc` (constructor, `Display`, `Debug`) and `ModifiedSince` | UTC range/truncation, Xero spelling, tenant binding |
| `src/lib.rs` | re-export list; `use` lines | `ModifiedSince` public, `Utc`/`SystemTime` imported |
| `src/lib.rs` | `wire` declaration, `GET list` | optional `modified_since?: Utc` + headers template |
| `src/lib.rs` | `Tenant::list` → `list_filtered`; new `modified_since`, `list_modified_since` | shared bounds check, tenant check |
| `src/lib.rs` | `Tenant::pages` → `traverse`; new `pages_modified_since` | resume filter check, closure passes `since` to every page |
| `src/lib.rs` | `PageCursor` field + rustdoc; `ListInvoices` field; `ListInvoices::send` setter call and cursor literal | filter carried by operation and continuation |
| `README.md` | intro sentence; new "Incremental sync" section with doctest | caller contract and provider caveats |
| `PROVENANCE.md` | appended "Maintenance sources" | XM/XR2 rows, retrieval method, hashes |
| `tests/modified_since.rs` (new) | 7 focused regression tests | wire text, bounds, same header on all pages and resume, cursor/filter binding, tenant isolation, unchanged existing wire, 304 |
| `tests/incremental_sync.rs` (new) | 2 maintainer-written consumer tests | full sync → restart → interrupted/resumed incremental sync → empty sync; cross-tenant refusal |

`tests/first_use.rs`, `tests/protocol.rs`, `tests/workflows.rs`, `tests/support/mod.rs` and fixtures
are untouched (no fixture was needed). Size: src +162/−8, README +36/−1, PROVENANCE +11, tests +595
new lines.

**The same fact appeared in several places.** "This traversal's filter" needed nine edits:

1. the declared value
2. its headers template
3. the `ListInvoices` field
4. that field's literal in `list_filtered`
5. the setter call in `send`
6. the `PageCursor` field
7. that field's literal in `send`
8. the closure argument
9. the resume comparison

Items 1–2 (Concord macro) and 4 and 7 (E0063, once the fields exist) are compiler-enforced. Items 5,
8 and 9 are not. "A sync point
belongs to one tenant" is checked separately in both new entry points. (Before this change, the SDK
already had four `check(..)` sites.) Paging bounds stayed in a single function: `list` and the
traversal both go through `list_filtered`. The header name and the UTC spelling each appear once.

## Did the compiler or the tests catch an omitted edit?

Each mutation was applied to the final source, with a backup under `target/tmp/mutations`. The suite
then ran, and the file was restored (`target/tmp/mutate.py`). All 13 mutations were caught: 3 by the
compiler and 10 only by the new tests. No pre-existing test failed for any of them, so the earlier
suite did not cover this behavior.

| Mutation | Caught by |
|---|---|
| M1 omit `.maybe_modified_since(self.since)` | tests: header text, every-page/resume, consumer |
| M2 omit `since` in the cursor literal | **compiler** E0063 |
| M3 traversal closure passes `None` | tests: every-page/resume, consumer |
| M4 drop the resume filter comparison | tests: continuation binding, every-page/resume |
| M5 / M6 drop the tenant check in `pages_modified_since` / `list_modified_since` | tests: tenant isolation (regression and consumer) |
| M7 civil year wrong for Jan/Feb | tests: header text table only (1970-01-01, 2000-02-29) |
| M8 drop the 9999 upper bound | tests: out-of-range refusal |
| M9 round to nearest second | tests: header text, bounds, every-page and (in this run) the consumer test, which by inference catches it only when a recorded start's sub-second part is ≥ 0.5 s |
| M10 append `Z` to the spelling | tests: four tests |
| M11 remove the declared value, keep template | **compiler** (Concord macro: "`modified_since` is not a value of `Accounting.tenant.list`") |
| M12 remove the template, keep the value | **compiler** (Concord macro: "value `modified_since` is never used") |
| M13 misspell the header name | tests: three tests |

The compiler covered the declaration's internal consistency (Concord macro) and struct exhaustiveness.
It did not catch any omission that changes behavior: forgetting to send the value, forgetting to carry
it per page, or forgetting to compare or bind it. Only the wire-level and binding tests catch those.

## Test results (this worktree, sequential, `CARGO_BUILD_JOBS=4`, absolute `CARGO_TARGET_DIR` = default `target/`)

Baseline before any edit: `cargo test --locked --offline -p concord-sdk-xero-accounting` gave
first_use 12, protocol 17, workflows 8 and doctest 1, all passing.

After the change:

| Command | Observed |
|---|---|
| `cargo test --locked --offline -p concord-sdk-xero-accounting` | PASS: lib 0, first_use 12, incremental_sync 2, modified_since 7, protocol 17, workflows 8, doctests 2 |
| `cargo clippy --locked --offline -p concord-sdk-xero-accounting --all-targets -- -D warnings` | PASS. The first run refused a hand-written leap-year `%` (`manual_is_multiple_of`) in the consumer test; fixed. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-xero-accounting --no-deps` | PASS |
| `rustfmt --edition 2024 --check` on owned `src/*.rs`, `tests/*.rs`, `tests/support/mod.rs` | PASS after `rustfmt` was applied to the new code |
| `git diff --check`; `git diff -- Cargo.toml Cargo.lock crates checks scripts examples/testserver` | clean; empty |
| new test binaries run 5× in a row | 5/5 each |

Expected header texts come from Python `datetime`, not from SDK output. The consumer test parses the
wire text back with its own day-counting oracle and compares it with the recorded start instants.

## Remaining generic duplication

1. **Continuation context binding is hand-written.** `PageCursor` repeats every non-page input of the
   request (tenant, page size, now the sync point). `traverse` compares each on resume, and the closure
   re-supplies each per page. Each future filter (Statuses, where, order, summaryOnly) would repeat the
   nine sites above. Three of them (send, carry per page, compare on resume) are enforced only by
   tests. `concord_pagination::Pages` takes an opaque key
   and cannot know that a resumed traversal is "the same listing" (extends FRICTION F5).
2. **Wrapper structs mirror generated handles.** `ListInvoices` keeps `page`, `page_size` and `since`
   next to the generated `AccountingTenantList`, which holds the same values. The SDK needs them back
   after `send` (metadata validation, next cursor), and generated handles expose only `set_*`/`maybe_*`
   (AUTHOR_GUIDE §9).
3. **Tenant-binding checks per entry point.** There are now six `check(..)` call sites (get, draft,
   create, cursor resume, and both new methods). This is FRICTION F2's wrapper cost, grown by two.
4. **Civil-date arithmetic.** About 15 lines in `Utc`'s `Display`. `concord-retry`'s private
   `parse_date` (HTTP-date → `SystemTime`) already contains the inverse calendar logic.

## Provider-specific responsibilities

In the SDK: Xero's header name and `yyyy-mm-ddThh:mm:ss` UTC spelling; truncation to the second; the
four-digit-year range; tenant binding of sync points and filtered cursors; refusing to resume across
filters; treating an undocumented `304` as a provider error (`Class::Other`) rather than an empty,
completed sync that a caller might use to advance its sync point.

Left to the caller, documented in README:

- **Upsert by invoice ID.** Inclusivity is undocumented, and truncation repeats changes within the
  starting second.
- **Advance the sync point only on completion,** to the time taken before that sync's first request,
  not the resume time.
- **Allow for clock skew.** A host clock ahead of Xero's can skip changes, so subtract a margin.
- **Changes that do not touch `UpdatedDateUTC`** are not returned.
- **Default-order gaps.** An invoice modified mid-traversal moves to the end. That can shift an unread
  invoice onto an already-read page, so this sync misses it, and the next sync misses it too because it
  is older than the new sync point. Periodic full syncs cover this.

## Candidate Concord improvements (record only; nothing shared was edited)

1. **concord-pagination: listing-bound continuations.** An optional constructor that takes a
   `Ctx: PartialEq` value along with the key, fixes it for every page, and refuses
   `Resume::At` from another context. The context would be stored inside the resume value. This would
   remove duplication item 1, and M3/M4 would become structural rather than test-only. Provider
   metadata and termination stay in the SDK.
2. **concord macros: read accessors on endpoint handles** (opt-in, never for `#[sensitive]` values), so
   SDK operation wrappers need not mirror declared values (item 2). Lower priority. Consider it only if
   other SDKs show the same mirroring.
3. **A small public UTC calendar helper** (for example next to `concord_retry::http`): fields of a
   `SystemTime` and the inverse, which concord-retry already computes privately. It would remove item 4
   without a chrono dependency.
4. **Docs.** AUTHOR_GUIDE §6 says a header sends a `Display` type "through a template". Adding an
   *optional* value to that example (absent → no header) would have saved one design round trip here.
   I found the presence rule in §4.3.

## Open questions for the owner (not decided here)

- Should `pages_modified_since` send `order=InvoiceId` (documented as optimised)? An immutable order
  should turn mid-traversal modifications into duplicates rather than gaps. However, that depends on
  invoices never leaving the filtered set, which is not documented or tested live, and it would add
  wire behavior beyond the ticket. Recorded as a proposal only.
- Whether Xero ever answers `304` to a filtered collection. The SDK currently fails closed; a live
  check would settle it.

## Limits of evidence

All behavior is shown against loopback fixtures. Live Xero acceptance of the exact header spelling,
the bound's inclusivity, ordering under concurrent edits and 304 behavior are **not established**.
Like `first_use`, the consumer test leaves synthetic journal directories under
`target/tmp/xero-incremental-sync`. Downloaded documentation data and mutation backups are under
`target/tmp/docs` and `target/tmp/mutations`, outside the package.
