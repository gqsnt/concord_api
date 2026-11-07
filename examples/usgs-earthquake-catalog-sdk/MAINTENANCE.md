# SDK23 maintenance — magnitude bounds and ordering

Date: **2026-11-07**. Baseline: **`9f59f9a6`**. Package:
`concord-sdk-usgs-earthquake-catalog`. **Live provider calls: NOT_RUN.** All
workflow requests use `examples/testserver::TestServer` on loopback. No shared
crate, check, script or other example changed; no commit was attempted.

## Prior exposure and reading order

This visible session began as the maintainer with the supplied ticket and
environment context; it contains no earlier SDK authoring work. This is
source-assisted maintenance, not a fresh docs-only consumer measurement. No
subagents or outside code review were used.

Before implementation, inspected the owned package's `Cargo.toml`, `README.md`,
`PROVENANCE.md`, `src/{input,lib,policy,export}.rs`,
`tests/{first_use,protocol,workflows}.rs`, `tests/common/mod.rs`, and existing CSV
fixtures (`page-1.csv`, `page-2.csv`), in full or relevant excerpts. The existing
consumer was read to preserve its contract, including exact default query order.
Also inspected root `Cargo.toml`, relevant `Cargo.lock` entries, Git status/HEAD,
the official documentation below, and the ancestor handoff's `AGENTS.md`,
`START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`,
`contracts/{PHASES,RESOURCES}.md`. The user's bounded capsule supersedes that
handoff's broader campaign work.

The first handoff-root `RESOURCES.md`/`PHASES` lookup found no such paths; the
guidance was then read at its actual `contracts/` locations. After the focused
tests passed, inspected `src/models.rs` and further existing consumer excerpts
before running the complete suite.

Runtime guidance consulted: the installed cloud-environment `runtime/SKILL.md`
and `references/networking.md`; the installed PDF `SKILL.md` was inspected while
following the linked FDSN specification. No PDF artifact was created. The runtime
status tool was unavailable and `/etc/codex/network-policy.json` was absent;
Cargo ran offline and no credentials were inspected.

Only **after implementing the ticket, passing four new tests and passing the
complete suite including all eight existing consumer tests** did this session
open `FRICTION.md` and `FIRST_USE.md`. Afterwards it strengthened the new consumer
to cover nondefault ordering across pages, inspected `src/csv.rs` and
the relevant `window.geojson`, `empty.geojson`, and `bad-number.csv` fixture
contents, and reran the suite. Historical reports were left unchanged.

## Provider contract and caller behavior

Read [the exact USGS API page](https://earthquake.usgs.gov/fdsnws/event/1/)
on **2026-11-07**, especially Query method Parameters → Other. It documents
decimal `minmagnitude`/`maxmagnitude` and four `orderby` values. The linked
[FDSN v1.0 specification](https://www.fdsn.org/webservices/FDSN-WS-Specifications-1.0.pdf)
was also inspected that date, parameter table p. 5 and event descriptions p. 12.
Neither cited magnitude description supplies a numeric magnitude range or an
explicit min/max relation. The ticket's requested min ≤ max check is an SDK
guard; equality and negative finite values are allowed, without a scientific
validity claim.

`min_magnitude` already existed. Added `Window::max_magnitude`,
`Window::with_order_by` and the public `OrderBy` enum. Both formats inherit these
inputs from the same generated window scope; pagination and export reuse it on
every page. Kept the SDK's `time-asc` default, although the provider's default is
`time`. Existing request ordering and all existing assertions remain intact.

Both magnitude builders remain infallible for compatibility with existing
`min_magnitude` callers. An optional `Window::validate()` checks the relation;
generated `send()` always checks the final magnitude bounds before I/O, including
after generated scope setters. `Magnitude::new` still rejects nonfinite values.
The shared comparison function prevents the explicit and send-time checks from
drifting. Typed ordering prevents unsupported strings through the generated API.
Direct execution of `prepare()` still bypasses the SDK policy, as documented.

## Every edited location

Paths below are relative to `examples/usgs-earthquake-catalog-sdk/`.

| File/location | Change |
|---|---|
| `src/input.rs`: `Magnitude`, bound comparison, `OrderBy`, `Window` | Generalized magnitude documentation; added comparable bounds, one relation validator, typed ordering/display/default, maximum and ordering storage/builders/accessors, explicit validation |
| `src/lib.rs`: public exports, `catalog.window` declaration, `UsgsCatalog::query` | Exported ordering; declared optional maximum and configurable ordering once for both endpoints; mapped both new window fields; corrected validation wording |
| `src/policy.rs`: `Policy::send` before `receive` | Read final prepared magnitude values and reuse the relation check after existing time validation |
| `tests/filters.rs` (new) | Four regression/consumer tests: complete ordering/bound wire matrix; invalid builder/setter bounds with no I/O; connected paged query/lookup/incremental export; terminal bad-row behavior with filters |
| `README.md`: usage example, input contract, error table, prepared-request caveat | Showed both bounds/ordering/preflight; cited provider page/date and defaults; described deferred validation; corrected the existing missing-column error classification to match the unchanged consumer assertions |
| `PROVENANCE.md`: maintenance addendum | Recorded exact sources/date, fixture transformations, independence boundary and unchanged dependencies/lockfile |
| `MAINTENANCE.md` (new) | This observable maintenance record and Concord candidates |

Root `Cargo.toml` already included this member, and `Cargo.lock` already contained
the package. Both remained byte-for-byte unchanged. No new crate or version
justification is necessary. `tests/first_use.rs`, every other existing test and
every checked-in fixture remained unchanged.

## Facts requiring edits in several places

- Maximum magnitude needed `Window` storage/API, the generated scope parameter,
  the handwritten `query` projection and prepared-query policy inspection.
  Adding the declaration alone would expose a generated setter but would leave
  ordinary `Window` consumers without the filter.
- Ordering needed the enum/wire spelling and default in `input.rs`, the dynamic
  scope parameter replacing the old fixed query pair, and the `query` projection.
  The declaration now calls `OrderBy::default()` so its default is not duplicated
  as a second literal. Independent tests intentionally repeat expected wire text.
- The new caller contract appears in rustdoc, the README example and regression
  expectations. The min/max relation itself has one implementation, called from
  both preflight and send-time validation.

The initial implementation compiled and all tests passed; **neither compiler nor
tests caught an accidentally omitted implementation edit**. That is not evidence
the compiler enforces completeness: omitting an optional projection can compile.
The new exact wire matrix and second-page assertions check those runtime facts.
Package-only formatting was applied before final checks. No failed build or test
result is being omitted from this session's record.

## Remaining duplication, responsibilities and Concord candidates

Scope inheritance already eliminates separate GeoJSON/CSV filter definitions.
No changes to `CsvExport` or the shared CSV codec were required; its consumer
receives a row before a two-second delayed suffix, then preserves commas,
doubled quotes, embedded newline and UTF-8 split across individual bytes. Bad
rows still stop delivery with the valid prefix retained.

Remaining generic mechanics and proposed changes (recorded only; frozen shared
sources preserved):

1. **Core/macro validation boundary:** `Policy` reparses typed inputs from the
   prepared URL because generated setters can invalidate relations. A narrowly
   scoped owner validation hook with borrowed typed fields, also enforced by
   `prepare()`, could remove this round trip. Provider comparisons stay in the SDK.
2. **Macro projection ergonomics:** `Window` mirrors generated scope fields and
   `query` copies them manually. A typed reusable input projection could reduce
   missed optional fields while preserving client → scope → endpoint. This is a
   candidate requiring comparison, not a request for a general expression DSL.
3. **External pagination:** finite GeoJSON uses `Pages`, while incremental CSV
   owns another traversal state machine and different failure/counter shapes.
   A bounded reader-per-page adapter could share admission, cancellation and
   terminal-error mechanics without buffering complete CSV pages. Offset and
   exhaustion semantics must remain provider code.
4. **External CSV support:** the unchanged wrapper forwards head/context/open
   and checks required headers. Set-inclusion required-column support in the
   shared CSV profile could remove forwarding and give consistent header-first
   diagnostics. The existing consumer proves current missing-column errors can
   be `Protocol` or `Stream(Decode)`; only the misleading README table was fixed.

Provider/SDK responsibilities retained: supported filter names and ordering
spellings, magnitude finiteness/relation policy, 31-day scope bound, event lookup
separation, one-based mutable offsets, page interpretation, USGS column names,
status/no-data mapping and resource budgets. Applications retain incremental
processing, durable checkpoints and reconciliation when updates shift pages.
Concord cannot provide a snapshot absent provider support.

## Executed verification and resources

All commands used this worktree's default `target/`, `CARGO_BUILD_JOBS=4` and one
Cargo process at a time. `CARGO_TARGET_DIR` was unset. Initial offline unlocked
`cargo test --offline -p concord-sdk-usgs-earthquake-catalog --no-run` passed
using cached artifacts (0.06 s), without changing the lockfile. Subsequent Cargo
commands used `--locked`.

| Command (with `CARGO_BUILD_JOBS=4`) | Observed result |
|---|---|
| `cargo test --offline --locked -p concord-sdk-usgs-earthquake-catalog --test filters` | 4 passed on the initial implementation |
| `cargo test --offline --locked -p concord-sdk-usgs-earthquake-catalog` | Final implementation: 34 integration tests (8 original consumer + 13 protocol + 9 workflow + 4 new), 1 compile-only doctest; 0 failed/ignored |
| `cargo clippy --offline --locked -p concord-sdk-usgs-earthquake-catalog --all-targets -- -D warnings` | Passed without warnings |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-usgs-earthquake-catalog --no-deps` | Passed |
| `cargo fmt -p concord-sdk-usgs-earthquake-catalog --check` | Passed |
| `git diff --check`, baseline lock/manifest/old-tests equality and path audit | Passed; edits confined to the owned package |

Initial resource measurements: `df -h . /tmp`, `free -h`,
`getconf _NPROCESSORS_ONLN`, `du -sh target`: worktree on `/dev/nvme0n1p2`,
357 GiB free of 915 GiB; 30 GiB RAM, 21 GiB available; 20 logical CPUs;
existing target 1.7 GiB. Four build jobs and a single focused package fit the
existing 3 GiB worker output budget with ample filesystem reserve. No cleanup,
full-workspace gate, live service test, TLS test or publication was performed.
Final recheck: target 1.8 GiB, filesystem 353 GiB free. These are observations,
not peak measurements or an attribution of whole-filesystem growth to this task.
