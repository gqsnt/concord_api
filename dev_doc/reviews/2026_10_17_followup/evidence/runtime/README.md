# Runtime and inspector native evidence

This packet preserves the compiled before/after controls behind the
[runtime and contract-diff review](../../runtime_review.md). The reviewed
baseline is `570eec460dee1d83828ae48e35396ae07652aabc` on `concord-v3`.

The [portable index](index.json) records each command, outcome, exit code,
source state, and the SHA-256 and byte length of both its log and receipt.
[SHA256SUMS](SHA256SUMS) covers every retained file in this packet except the
checksum file itself. Logs and receipts are copied byte for byte; they retain
the historical workspace paths and execution times. Relative paths and
`portable_argv` in the index provide navigation and commands independent of
that workspace location.

## Compiled controls and results

| Native run | Result | Log | Original receipt |
| --- | --- | --- | --- |
| `csv-before` | Three new unit regressions fail on the old decoder; three existing selected unit tests pass | [log](native/csv-before.log) | [receipt](native/csv-before.json) |
| `csv-before-streams-native` | Both new managed-loopback regressions fail on the old decoder, after local listeners start successfully | [log](native/csv-before-streams-native.log) | [receipt](native/csv-before-streams-native.json) |
| `csv-after` | Six CSV unit tests and eight integration contracts pass | [log](native/csv-after.log) | [receipt](native/csv-after.json) |
| `inspect-sites-before` | Both generated-contract regressions fail: all five endpoints are attributed instead of two or three | [log](native/inspect-sites-before.log) | [receipt](native/inspect-sites-before.json) |
| `inspect-sites-after` | All 19 unit and 18 integration tests pass | [log](native/inspect-sites-after.log) | [receipt](native/inspect-sites-after.json) |
| `rustdoc-contract-diff` | Original Catalog CLI reports ten provenance changes but attributes eight endpoints, including untouched root authentication | [log](native/rustdoc-contract-diff.log) | [receipt](native/rustdoc-contract-diff.json) |
| `inspect-sites-catalog-isolated` | Corrected CLI reports the same ten changes and exactly the seven regional endpoints | [log](native/inspect-sites-catalog-isolated.log) | [receipt](native/inspect-sites-catalog-isolated.json) |

The [CSV formatting receipt](native/csv-format.json) and
[inspector formatting receipt](native/inspect-sites-format.json) retain the
pinned formatting steps. Their empty logs mean successful formatting with no
diagnostic output; they are not test results.

The [Catalog comparison](catalog_comparison.json) records that all report
facts other than endpoint-attribution lists match. The original and corrected
JSON, unchanged input snapshots and the one-clause source edit are retained
in the [sibling Rustdoc evidence](../rustdoc/index.json); their hashes are also
listed in this packet's index.

## Commands and environment

The native controls used Rust 1.97.1, locked dependencies, four Cargo build
jobs, no incremental compilation and zero debug information in development
and test profiles. Exact original commands are in the receipts; portable
argument arrays and environment settings are in the index. The principal
commands, run from the repository root with the corresponding source state,
are:

```sh
cargo test -p concord-extras --all-features --locked csv_
cargo test -p concord-extras --all-features --locked --test contracts contracts::record_streams::csv::csv_
cargo test -p concord-inspect --locked --test generated_contract generated_dependency_sites
cargo test -p concord-inspect --all-features --locked
```

For the public Catalog replay:

```sh
cargo run --locked --offline -p concord-xtask --features examples -- contract diff dev_doc/reviews/2026_10_17_followup/evidence/rustdoc/catalog_before.contract.json dev_doc/reviews/2026_10_17_followup/evidence/rustdoc/catalog_after.contract.json --format json
```

The successful corrected Catalog replay used its own Cargo target directory.
CSV integration tests need permission to create local loopback listeners.
They do not contact providers or production endpoints.

## Actual source states

[source_states.json](source_states.json) identifies the exact reviewed file
bytes with SHA-256 hashes. Each state is represented by a patch against the
baseline commit:

- [CSV before](source/csv-before.patch): the old production decoder plus the
  three new unit and two new integration regressions.
- [CSV after](source/csv-after.patch): the corrected decoder and the same
  regressions, after the recorded formatting step.
- [Inspector before](source/inspect-before.patch): the old diff implementation
  plus the two generated-contract regressions.
- [Inspector after](source/inspect-after.patch): corrected attribution and
  the same regressions, after the recorded formatting step.

Every patch was reconstructed in a separate temporary directory during
consolidation. Inspector states were compared with the source snapshots
captured before their native runs. The corrected CSV state was compared with
the preserved correction and regression additions after pinned formatting.
Both corrected states match the final four owned repository files. This was
an artifact-integrity check; no new broad test run was performed.

Apply a selected patch to an isolated checkout of the baseline when examining
or replaying that source state. The before and after patches each apply to
the baseline, not to one another. These patches cover the reviewed files;
the original receipts retain the complete historical worktree status and
tracked-diff hashes, including concurrent public V3 work. These development
controls are not presented as clean-commit qualification runs.

## Attempts excluded from defect evidence

The following original logs and receipts are retained under
`environment-only/`. The index marks all three as `environment_only` with
`product_defect_evidence: false`:

- [Sandboxed CSV attempt](environment-only/csv-before-streams.log),
  [receipt](environment-only/csv-before-streams.json): listener creation fails
  with `PermissionDenied` before the managed-stream behavior can be exercised.
  Its seven test failures are **not** evidence of seven product defects. The
  subsequent permitted two-test control is `csv-before-streams-native`.
- [First reused-target Catalog build](environment-only/inspect-sites-catalog-after.log),
  [receipt](environment-only/inspect-sites-catalog-after.json), and
  [immediate retry](environment-only/inspect-sites-catalog-retry.log),
  [receipt](environment-only/inspect-sites-catalog-retry.json): compilation
  stops with E0463 before either command produces a diff. The fresh-target
  replay succeeds; the two failed builds are not counted as diff results.

All retained material comes from public project code, public fixture data and
local native executions. This packet contains no account credentials,
production captures or private authority documents.
