# SDK25 FRED maintenance — observation controls

Date: **2026-11-07 (Europe/Paris)**. Starting HEAD and frozen shared baseline:
`e1995ff7d60426e921da88b96f7c994606da79ae`. Work remains uncommitted.
**Live provider calls: NOT_RUN.** Every executed workflow used
`TestServer::start(|request: &Recorded| -> Reply)` on loopback (including equivalent
named fixture functions). Only public documentation was fetched remotely.

## Prior exposure and first attempt

I did not author the SDK or its first consumer, and had no earlier implementation
context in this session beyond the maintenance capsule and environment instructions.
Before the first implementation edit I read/inspected:

- Ancestor `AGENTS.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`,
  `AUTHORITY.md`, `contracts/PHASES.md` and `contracts/RESOURCES.md`. The bounded
  user capsule supersedes their broader campaign scope and commit permissions.
- The cloud-environment runtime `SKILL.md`. Its environment-status tool was not
  available; local resource checks and offline builds sufficed for this task.
- Root `Cargo.toml`; this package's `Cargo.toml`, `README.md`, `PROVENANCE.md`,
  all five pre-existing `src/*.rs` files, and
  `tests/{first_use,protocol,workflows}.rs` / `tests/support/mod.rs`.
- `docs/AUTHOR_GUIDE.md`'s parameter-group example and rules, searches for group
  declarations in `crates/concord/tests`, and
  `crates/concord/tests/ui/param_group_two_groups_same_value.rs`. These were
  read-only references; no shared implementation or other SDK source was changed.
- Both official pages linked below. An initial broad filename inventory listed
  other worktrees but did not open their SDK contents.

This is source/documentation-assisted maintenance, not a docs-only independent
consumer exercise. The new consumer was written by the same maintainer.
**Neither `FRICTION.md` nor `FIRST_USE.md` was opened until the implementation,
three regression tests, new consumer, and first full test run had passed.**
They were then read for comparison and left unchanged. Their XML/ZIP/media and
representation-duplication findings agree with the inspected implementation.

## Provider contract and decision

Official pages read **2026-11-07**:

- [Series search](https://fred.stlouisfed.org/docs/api/fred/series_search.html):
  search, query API key, default XML/explicit JSON and limit/offset.
- [Series observations](https://fred.stlouisfed.org/docs/api/fred/series_observations.html):
  `units`, `frequency`, `aggregation_method`, omitted defaults and CSV-in-ZIP.

Accepted codes are exactly:

| Control | Codes |
|---|---|
| `units` | `lin`, `chg`, `ch1`, `pch`, `pc1`, `pca`, `cch`, `cca`, `log` |
| `frequency` | `d`, `w`, `bw`, `m`, `q`, `sa`, `a`, `wef`, `weth`, `wew`, `wetu`, `wem`, `wesu`, `wesa`, `bwew`, `bwem` |
| `aggregation_method` | `avg`, `sum`, `eop` |

The observations page explicitly restricts frequency aggregation to XML/JSON.
The ticket's identical controls across all forms are implemented as identical
validated **query forwarding**, including CSV. Neither this implementation nor
its fixtures claim the provider actually aggregates CSV. There is no local
aggregation fallback. Native-series compatibility remains a provider decision.

Each control is optional and absent by default, preserving existing request
shapes. Clearing a setter with `maybe_*(None)` restores omission. Values remain
`String` in XML/JSON and exact member bytes in CSV: `.` is never replaced by zero,
null or a floating-point value. Existing ZIP limits and failure behavior remain.

## Files and locations edited

All paths below are relative to `examples/fred-sdk/`.

| File/location | Change |
|---|---|
| `src/options.rs` (new), `query_option!`, `Units`, `Frequency`, `AggregationMethod` | Closed enums; one mapping per type generates exact-code validation, `new`, `FromStr`, `as_str` and `Display`; provider citation and variant docs |
| `src/lib.rs`, module/reexports, `params ObservationOptions`, `observations`, `observations_json`, `export_csv` | Public types and shared optional query group included by all three endpoints; CSV caveat in generated API docs |
| `src/error.rs`, `Error::InvalidParameter` and `Display` | Identifies the rejected parameter without retaining or formatting supplied input |
| `tests/observation_options.rs` (new), all three tests | Independent complete code/variant expectations; rejects unknown/case/whitespace codes; verifies every code on all forms, Accept/path/query multiplicity, exact values/CSV/README bytes, and clearing |
| `tests/maintenance_use.rs` (new), `transformed` and consumer test | Searches, retrieves selected metadata, reads XML with bounded resume, reads JSON, exports ZIP CSV, and checks controls survive each operation and offset |
| `README.md`, opening baseline, Observation controls, failure table, exclusions and verification link | Usable shared-setter example, invalid-code behavior, omitted defaults, resume rules and provider limitation; no longer excludes the added transforms |
| `PROVENANCE.md`, Maintenance ticket section | Exact sources/date, synthetic-fixture scope, dependency/lock status and limitation |
| `MAINTENANCE.md` (new) | This observable maintenance record |

`Cargo.toml` already contained the workspace member; `Cargo.lock` already contained
the package and dependencies. Both remain byte-for-byte unchanged. No dependency
was added, no package version changed, and no new license justification is needed.
Every pre-existing test file, fixture, shared crate, check, script and other example
is unchanged, including every assertion in `tests/first_use.rs`.
The client → scope → endpoint hierarchy and native escape remain intact.
Adding `Error::InvalidParameter` can require downstream exhaustive `Error` matches
to add an arm; the preserved first consumer needs no change.

## Repeated facts and omission detection

The three parameter names/types are declared once in `ObservationOptions`.
Their group inclusion still had to be edited in **three endpoint declarations**.
Wire names also appear as safe validation-error labels in the option definitions.
Allowed values are each written once in implementation; validation and formatting
share that mapping. Tests deliberately repeat the official value/variant table
to catch a wrong mapping instead of deriving expectations from implementation.
README, rustdoc and provenance/report repeat user-visible semantics and the CSV
restriction. Introducing a new public error required its enum and formatter edits.

The first implementation compiled and passed without an accidental omitted edit.
To test omission detection, I temporarily removed `..ObservationOptions` **only**
from `export_csv`, then ran:

```sh
CARGO_BUILD_JOBS=4 cargo check --offline --locked -p concord-sdk-fred --test maintenance_use
```

It failed with **E0277**, `FredSeriesExportCsv: ObservationOptions` not satisfied,
at `transformed(selected.export_csv())`; the diagnostic listed XML/JSON as
implementors. This was an intentional negative experiment, not a passing check.
The inclusion was restored immediately. The library declaration alone cannot
infer that a forgotten representation was intended to share a group; the consumer
bound supplies that evidence. The full restored suite subsequently passed.

The warning-denying rustdoc run separately caught a bare source URL in the new
module (`rustdoc::bare_urls`). Converting it to an angle-bracket link fixed the
diagnostic, and the rerun passed. No shared code fix was needed.

## Verification and resources

Commands ran sequentially with `CARGO_BUILD_JOBS=4` and the worktree's default
`target/`; no `CARGO_TARGET_DIR` override. No real-service request or commit.

| Check | Result |
|---|---|
| `cargo build --offline -p concord-sdk-fred` (initial, deliberately unlocked) | PASS; existing target reused; lock unchanged |
| `cargo test --offline --locked -p concord-sdk-fred` (final restored implementation and README) | PASS: 24 integration tests and 3 compile-only doctests; no unit tests |
| `cargo clippy --offline --locked -p concord-sdk-fred --all-targets -- -D warnings` | PASS |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-fred --no-deps` | PASS after the URL fix |
| `rustfmt --check --edition 2024 --config skip_children=true` on the five edited/new Rust files | PASS; no formatting of existing consumers/codecs |
| `git diff --check` and comparison with `git show HEAD:<path>` | PASS; original tests, root manifest and lock unchanged |

The integration total comprises 2 original first-use, 12 protocol, 6 workflow,
3 new regression and 1 new consumer tests. The regression sends 48 local requests
to exercise every documented code in all formats; it does not test economic
calculations. Existing tests exercise plain CSV rejected under both CSV/ZIP media,
over-1-MiB wire archives, over-4-MiB extracted archives, aggregate/entry limits,
malformed archives and string preservation. The new consumer verifies both
connected workflows and retained controls across a bounded stop and resume.

Resource measurement at 2026-11-06 23:38 UTC (2026-11-07 local): `df -h . /tmp`,
`free -h`, `nproc`, `du -sh target` and process-name inspection showed 358 GiB
available on `/dev/nvme0n1p2`, 24 GiB available RAM, 20 CPUs, and no other visible
Cargo/rustc jobs. Existing target was 1.6 GiB and ended at 2.0 GiB; available disk
remained 358 GiB at report time. No cleanup was necessary. Source/tests/docs are
preserved; the reused target is reproducible build output, not clean-build evidence.

## Remaining responsibilities and candidate Concord changes

- **Macros/declarations:** parameter groups already remove the repeated setters
  and permit one generic consumer function; retain that facility. Representation
  endpoints still repeat route, limit, group membership and format/query pairing.
  A candidate generated-doc inventory of included groups and concrete Accept
  values would make omissions and wire contracts easier to review. A larger
  representation-family abstraction needs unlike-provider evidence before adding
  a new declaration concept; none is introduced under this freeze.
- **External codecs:** strict singleton media/charset checks, XML parsing limits
  and bounded ZIP/CRC extraction are generic mechanics still handwritten here.
  Candidates are an exposed strict-media helper and bounded archive reader in
  optional capabilities, plus an XML codec evaluated with another provider.
  FRED must retain its envelope names, numeric-attribute allowlist, allowed ZIP
  members and exact CSV-export projection. Shared plain CSV decoding cannot
  replace ZIP decoding.
- **Execution/pagination:** `SendWith` already gives ordinary generated sends
  bounded success/error bodies. A reusable bounded finite-response policy and
  checked offset-arithmetic helper could remove remaining mechanics; HTTP error
  interpretation, envelope projection and continuation meaning remain SDK-owned.
- **SDK/application:** provider codes and defaults, CSV aggregation limits,
  missing markers, query credentials, no-redirect transport and safe provider
  error formatting remain explicit responsibilities. The application chooses
  controls, keeps the same query on resume and chooses where exported bytes go.
  No core change should silently coerce values or invent unsupported aggregation.

These are recorded candidates only. No shared crate, macro or declaration engine
was changed; the actual improvement uses the frozen group/setter machinery.
