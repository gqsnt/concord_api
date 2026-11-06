# SDK19 maintenance: empty delta pages and limited/unknown members

## Scope and first attempt

Maintenance began at SDK HEAD `e23f95c2`, on the frozen Concord library baseline
`47a25fbf`. The ticket requested an empty intermediate delta page with a next link,
and a member of another directory type or with limited visibility, preserving type
information and continuing pagination. No provider calls or credentials were used.

The baseline already implemented both behaviors. `Listing::fetch` validates delta
link cardinality independently of item count and hands the returned continuation
to public `concord_pagination::Pages`. `DirectoryObject` stores the exact type
string, accepts absent/null profile fields, retains additional properties, and
classifies unfamiliar types as `ObjectKind::Other`. Existing author workflow tests
already included an empty delta page, limited users/devices and a future type.
The baseline test command passed 44 integration tests and 3 doctests.

The first maintenance attempt added five focused wire tests and two fixtures;
all five tests compiled and passed without a repair. No runtime defect was found,
so no caller migration or behavior change was introduced. The SDK's `FRICTION.md`
and `FIRST_USE.md`, other SDKs' role reports, and `v4/sdk20/` were not read.

## Edits by kind and independently maintained facts

| Kind | Edits | Independent runtime descriptions changed |
| --- | --- | --- |
| Declaration | None; all three operation declarations and response types already cover the ticket. | 0 |
| Adapter | Added `Listing::fetch` rustdoc explaining empty pages and delta completion; traversal/link/error logic unchanged. | 0 |
| Model | Added `DirectoryObject` rustdoc for unknown and id/type-only members; fields and `kind()` unchanged. | 0 |
| Tests | New `tests/maintenance.rs` with five loopback wire regressions, using existing public SDK methods and test support. Added two byte-preserved fixture files. | 0 |
| Docs | Extended both relevant README workflows and verification notes, added two provenance rows and this report. | 0 |

No semantic change required updating several independently maintained runtime
descriptions of the same fact: **zero runtime locations changed**. Existing delta
behavior is split between the SDK's envelope-to-page adapter and the generic
pagination facade; member behavior is represented by the model and the client's
id/type validity check. None required a synchronized edit.

For review, each of the two ticket facts gained explanatory text in **two public
places** (its source rustdoc and its README workflow). Each has one new fixture
documented once in the provenance table. Expected wire paths also appear in test
assertions so URL normalization or query reconstruction would fail a test; those
are verification expectations, not additional runtime configuration. No new field,
type spelling, link schema or operation name was added to a production table.

## Regression evidence

`tests/maintenance.rs` exercises:

1. An initial delta sequence with page sizes `[2, 0, 0, 1]`, followed from a stored
   final delta link by an incremental sequence `[0, 0, 1]`. Intermediate pages
   never expose a final checkpoint. The final checkpoint URL and every request
   target are checked exactly.
2. A cap immediately after both empty intermediate pages. The three-page prefix
   has no completed delta checkpoint; a new client restores the unread next link
   and reads the final page successfully, with no replay of earlier pages.
3. A 429 after both empty pages. The prefix and failed continuation survive,
   including the provider code, actual status, request ID and Retry-After. An
   explicit retry uses the same link; subsequent traversal calls report `Ended`
   with `Outcome::Failed`. There is no automatic retry or reset.
4. A limited device plus unknown id/type-only and null-profile objects, all
   preserved through a bounded item collection. A new client restores the saved
   next link and reads later user, group and contact members. Both requests retain
   `ConsistencyLevel: eventual` when fields were selected.
5. An item stream emitting those limited/unknown objects before a typed 403 from
   the following page. The stream emits the error and reports failed completion;
   default selection sends no advanced-query header on either request.

Opaque URL probes include duplicate keys, `%2B`, `%2b`, `%2F`, `%252F` and a literal
`+`, including a fully qualified `/users/microsoft.graph.delta` next link. Tests
assert authenticated GETs with no body and exact server-returned request targets.
Fixture bytes remain covered by the existing `.gitattributes` rule
`fixtures/** -text`.

## Reading and provenance

Read the SDK README, `PROVENANCE.md`, all seven source modules, the existing
workflow tests/test support, and the first-use suite's public application setup
and delta fixtures. Baseline execution also covered every protocol and first-use
test. No other provider SDK supplied the solution.

Read Concord's root README, the pagination crate README, the extras crate README,
and relevant caller-guide/recipes sections for traversal completion, response
metadata and following returned links. Generated public rustdoc with
`cargo doc --locked -p concord --no-deps --features csv,jsonl` and inspected
`Prepared::{follow, receive}` and `Received`'s bounded consumers. These public
methods already support the SDK; no private reach-through, rebuilt execution,
handwritten operation identity or native escape was added.

Reopened these official Microsoft Learn pages on **2026-11-06**:

- [Delta query overview](https://learn.microsoft.com/en-us/graph/delta-query-overview),
  collection flow and opaque state tokens (displayed update: 2025-04-30).
- [List group members, v1.0](https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0),
  limited-information permission behavior and directoryObject responses
  (displayed update: 2025-07-23).
- [Graph paging](https://learn.microsoft.com/en-us/graph/paging), whole nextLink
  requests, empty pages and continuation headers (displayed update: 2025-04-03).

Both new fixtures were hand-authored from those documented shapes and behaviors.
No bytes were copied from a documented response or obtained from a tenant. The
future directory type is an invented robustness probe, not a documented Graph
type. All ids, tokens, query parameters and page arrangements are synthetic;
these tests are not vendor conformance evidence.

## Diagnostics, validation and locality

There was no generated-name or compile surprise in the maintenance attempt.
The existing generated operations and public paging/error types were sufficient.
The change felt local: two source documentation comments, one focused regression
suite, two fixtures and SDK documentation, with no library or other-example edits.
No new Concord workaround or friction entry was necessary.

Validation commands use the required prefix on every invocation:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-microsoft-graph
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-microsoft-graph --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-microsoft-graph -- --check
```

Final results: **49 integration tests and 3 doctests passed**, including all 18
unchanged first-use tests. Clippy passed with warnings denied; formatting and
`git diff --check` passed. Public rustdoc generation also passed. Verification was
offline against the loopback test server on Rust 1.99.1; no live Graph verification
was attempted.
