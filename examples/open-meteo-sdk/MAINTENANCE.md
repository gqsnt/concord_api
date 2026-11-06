# SDK13 maintenance: precipitation variable and CSV column

Date: 2026-11-06. Ticket: add a selected variable/CSV column and verify
header-based mapping and caller-visible unknown columns. Requested runner:
Codex gpt-6-astra / xhigh; no independent runtime-setting verification is
claimed. Starting SDK HEAD: `1f6ec3e0`, with Concord frozen at `0983a349`.

Added `HourlyVariable::precipitation()`, selecting the provider's
`precipitation` name. It represents total rain, showers and snow during the
preceding hour; the default unit is `mm`. Official evidence and the synthetic
fixture transformations are in [PROVENANCE.md](PROVENANCE.md). No forecast
endpoint was called, and no credentials were used.

## Edits by kind

| Kind | Change |
| --- | --- |
| Declaration | None. `wire::ForecastApi::forecast` already accepts the comma-separated `hourly` string. |
| Adapter | None. `OpenMeteo::forecast` obtains names from the selected variables; CSV mapping derives names and units from headers, with no list of known variables. |
| Model | One new convenience constructor and its rustdoc in `src/model.rs`. JSON arrays and CSV numeric values already use maps; no field or serde rename was needed. |
| Tests | New `tests/maintenance.rs`: four loopback tests covering selected precipitation, unselected precipitation, equivalent JSON/CSV observations, and missing selected precipitation as a typed response failure. Existing tests are unchanged. |
| Fixtures | Two new hand-authored files, `fixtures/maintenance-precipitation.csv` and `.json`. Existing `.gitattributes` already applies `fixtures/** -text`; Git reports `text: unset` for both additions. |
| Docs | README construction/consumption example and selection semantics; provenance inventory and source recheck; this maintenance record. |

## Repeated facts and locality

There is **one production definition** of the new constructor's wire spelling.
No second variable list had to change in the declaration, request adapter,
CSV adapter or JSON model. The generic `HourlyVariable::named("precipitation")`
path already worked; the added method makes that selection discoverable.
Returned units are still read from the response rather than hardcoded in a
decoder. No caller change is required.

Counting independently maintained files that now describe the new variable,
there are **seven**: the model, README, test file, two fixtures, provenance
record and this report. Five are implementation/example/verification artifacts;
two are evidence records. The tests explicitly spell expected query names,
headers, units and values independently of the constructor, and the fixtures
independently encode provider-shaped keys/headers. These descriptions must
stay consistent; they are not generated from a shared schema. Thus the change
touches several descriptions of the same fact, but only one runtime definition.
The production change felt local: five added lines, with no transport or
decoding changes. Most of the work was specifying and verifying compatibility.

## What was read

- Campaign `AGENTS.md`, `START_HERE.md`, `PROMPT.md` and `AUTHORITY.md`, applying
  this task's narrower SDK-only scope and frozen-library instruction.
- SDK `Cargo.toml`, README and provenance; `src/lib.rs`, `src/model.rs`,
  `src/csv.rs`, error definitions, workflow/protocol tests and first-use test
  entry points. These located the constructor, request name serialization,
  header mapping and existing failure checks. Neither `FRICTION.md` nor
  `FIRST_USE.md` was read, before or after the first attempt.
- Root README, relevant caller/custom-format guide sections, core/extras crate
  READMEs, and generated public rustdoc for `RawTable`, `CsvContext` and
  `Received`. Rustdoc was built using the requested `csv,jsonl` features.
- Official Open-Meteo forecast documentation for precipitation and its default
  unit; the pinned official CSV writer for envelope, headers and missing-value
  representation. The raw-source web fetch missed its cache; Python HTTPS
  retrieval succeeded and matched the existing recorded SHA-256.

No other provider SDK supplied the maintenance solution. Existing Open-Meteo
tests supplied the local fixture-server setup; the SDK's earlier author
attributions remain in its README.

## Attempt and verification

1. The unchanged baseline passed 30 integration tests and two README doc tests.
2. Added the constructor, fixtures, README/provenance updates and four wire
   tests, then formatted the package. The first changed-code test run passed
   all 34 integration tests and both doc tests. No compile or test repair was
   needed, and no generated-name surprise occurred.
3. Clippy passed with warnings denied. Existing first-use tests (8), workflow
   tests (6), protocol tests (16) and the new maintenance tests (4) all pass.
4. The coordinator reported an intervening session termination with exit 101
   and `No space left on device`, and then confirmed space had been freed.
   On resumption, `df -h` showed 200 GiB available on the workspace/cache
   filesystem. Final inspection added an explicit four-row assertion to the
   unselected-column test so an empty or truncated result cannot pass its loop.
   The complete requested test command was rerun successfully: 34 integration
   tests and two doc tests passed, with no failures or ignored tests. Clippy
   with `--all-targets -- -D warnings` and package formatting check also exited
   0. No production-code repair was necessary for the environmental failure.

The new CSV fixture puts precipitation first although requested last, time in
the middle and temperature last. Tests verify `0.00`/`1.20` as numeric selected
values, `NaN`/empty as `None`, exact original bytes, unit labels, quoted unknown
notes and unselected pressure text. The same precipitation column remains
exact text, including trailing zeros and missing-value spelling, for an old
temperature/wind-only selection. JSON and CSV agree for every selected value
and timestamp. Missing precipitation fails both formats and retains status and
operation metadata. The full existing suite covers typed provider errors,
limits, malformed late rows and incomplete transport termination.

All cargo invocations used
`CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a`
and the repository's pinned 1.99.1 toolchain:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo doc -p concord --no-deps --features csv,jsonl
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-open-meteo
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-open-meteo -- --check
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-open-meteo
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-open-meteo --all-targets -- -D warnings
```

No new Concord workaround, private access, rebuilt execution, handwritten
operation identity or native escape was needed. The existing CSV unmatched-quote
guard and its reproduction test remain unchanged. No library, dependency,
lockfile, other example or existing fixture changed. Fixture tests establish
offline SDK behavior, not live-provider conformance.
