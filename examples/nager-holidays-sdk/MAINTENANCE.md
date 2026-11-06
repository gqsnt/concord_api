# SDK01 maintenance: unknown field and null subdivisions

Date: 2026-11-06. Ticket: “add an unknown optional field and a null subdivision
list to a fixture; callers retain known fields.”

The existing SDK already accepts both changes. `Holiday` derives Serde
deserialization without rejecting unknown fields, and `subdivision_codes` is
`Option<Vec<String>>`. No runtime or caller API change was necessary. This
maintenance adds a stronger wire regression and clarifies the public contract.

## Starting point and first attempt

- The clean worktree started at `9b99ef0728b8ac9155b8f718b74dd6152156d6d8`:
  SDK author commit `38dbaa29` and first-use commit `9b99ef07` follow frozen
  library commit `94d13527`. The library was not edited.
- Observed compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; edition 2024.
- Before editing, the requested full package suite passed: 25 integration tests
  (9 first-use, 13 protocol, 3 workflow) and 4 doc tests.
- The original AT perturbation already added an unknown object/null field, but
  its subdivision lists were null before and after. The new DE fixture changes
  a previously populated regional list to null while retaining its false national
  flag. Other entries still exercise populated, empty and omitted lists.
- First attempt: added the fixture, protocol assertions and documentation;
  formatting and all package tests passed immediately. There were no compile
  failures, corrective iterations or required interventions. `FRICTION.md` and
  `FIRST_USE.md` were not read before this attempt or afterward. The existing
  `tests/first_use.rs` was inspected and run, without edits.

## Edits by kind

| Kind | Files and changes |
| --- | --- |
| Declaration | None. The existing `Vec<Holiday>` output and `Json` format remain sufficient. |
| Adapter | None. `.fetch()` still uses public `prepare()?.receive()`, captures the head and decodes once. |
| Model | `src/models.rs`: rustdoc clarifies unknown-field tolerance and null subdivisions with a false national flag. No field, type, Serde attribute or deserializer change. |
| Fixtures | Added `fixtures/holidays-de-2027-perturbed-synthetic.json`: one populated subdivision list becomes null; `futureOptionalField` is an object on that entry and null on the following entry. Other known values are unchanged. |
| Tests | `tests/protocol.rs`: new DE comparison asserts complete model equality after changing only the expected subdivision value, plus response metadata and exact requests. The existing AT comparison now also exercises generated `.send()` and checks all three requests and connection cleanup. |
| Docs | `README.md` explains unchanged caller behavior and the regression; `PROVENANCE.md` identifies the hand-authored fixture and its limits; this file records the maintenance evidence. |

The existing `.gitattributes` remains `fixtures/** -text`; `git check-attr` reports
`text: unset` for the new fixture. Its bytes use LF with a final newline.
The original workflow tests, first-use tests and their fixtures are unchanged.

## Independent descriptions and locality

There is **one production response model** for both `.fetch()` and `.send()`;
**zero runtime locations** had to change. Neither the declaration nor the adapter
duplicates the subdivision or unknown-field decoding rule.

The newly introduced regional null value has **two independently authored test
descriptions**: its JSON fixture and the Rust expected-value assignment. The
comparison reuses the baseline's decoded holidays instead of restating all six
fields in another expected model. Four prose locations describe the compatibility
case: model rustdoc, README, provenance and this record. The expanded AT test uses
its existing fixture and baseline equality, without another response schema.

Six files were added or edited in total. The change felt local: fixture/test/docs
maintenance, with no synchronized changes across independently maintained runtime
descriptions of the same fact and no caller migration.

## Reading and surprises

Read the SDK README and provenance, `src/lib.rs`, `src/models.rs`, the protocol,
workflow and first-use test code, test support and relevant fixtures. These located
the single response model, the `.fetch()` adapter, existing tolerance coverage and
public caller paths. Repository guidance and the workspace README, relevant
`docs/AUTHOR_GUIDE.md` sections (formats and response metadata), caller/recipe
references, and core/extras/macros READMEs supplied the public API context.
Generated Concord rustdoc for `Json`, `Received` and `Prepared` was also inspected.
None of the five existing provider SDKs supplied this maintenance solution; they
were not opened for this task.

The official [Holiday API response model and preview](https://nagerholidays.com/api)
were re-read on 2026-11-06, as were Serde's
[attributes](https://serde.rs/attributes.html) and
[unknown-field behavior](https://serde.rs/container-attrs.html#deny_unknown_fields).
The preview includes null subdivisions. The new DE data and unknown field are
synthetic, not an obtained example or captured provider response. No live provider
endpoint, credential or secret was used. All test HTTP runs against the existing
path dev-dependency `examples/testserver` on loopback.

No generated-name or compile surprise occurred. The existing adapter targets
`NagerHolidaysHolidays`; its spelling did not need to change. No private reach-through,
rebuilt execution, handwritten operation name, native escape or new workaround
was needed, so there is no new Concord friction entry.

## Verification

All commands below succeeded with the required shared build cache:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo doc -p concord --no-deps --features csv,jsonl
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-nager-holidays -- --check
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-nager-holidays
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-nager-holidays --all-targets -- -D warnings
```

Final package results: **26 integration tests** (9 first-use, 14 protocol,
3 workflow) and **4 doc tests passed**, with no failures or ignored tests.
Clippy emitted no warnings. A separate JSON comparison confirmed exactly the
documented fixture changes; `git diff --check` passed. These results establish
local compatibility, not live provider conformance.

## Between waves 2-3 (coordinator integration)

The record above is unchanged. `fetch()` was replaced by the client's send policy
(`INTEGRATION.md`, "Between waves 2-3"), so the generated `send()` is now the SDK path. In
`tests/protocol.rs` the `.fetch()` sites read `.send()`, and the AT comparison's third read,
which exercised the Concord path ("generated `.send()`" above), now reads
`.prepare().unwrap().send()`: the same Concord execution and `Vec<Holiday>`, compared with the
same `original.holidays`. The one production response model is still shared by both paths.
