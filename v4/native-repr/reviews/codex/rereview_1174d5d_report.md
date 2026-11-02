**Verdict: REVISE.** F1, F2 and F4 are resolved. F3 retains a P2 semantic-preservation defect.

Effective settings observed in this session’s `turn_context`: **`gpt-6.1-sol`, `xhigh`**, workspace-write sandbox, approval policy `never`. Executed with Rust **1.97.1** and MSRV **1.85.1**, private prefetched `CARGO_HOME`, and an initially absent `target/review-build`. [Environment receipt](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/environment-receipt.json).

Reviewed HEAD: `bed9fd29d336c30be4735a6d545d2c7ee19b6355`. Its commit identifies source `1174d5d8c894fa768546f6600cf0415faf4690c5`; that original object is unavailable locally.

| Finding | Disposition | Personally executed evidence |
|---|---|---|
| **F1 — P1 sensitivity leak** | **RESOLVED** | Original reproducer fails with Concord’s dual-use diagnostic. **40 compiler cases per toolchain** produced expected outcomes, covering optional/repeated inputs, wrappers, aliases, owner references, separate endpoints, override/append, subdomains, sensitive names, cfg and renamed dependency. **9 runtime checks per toolchain passed**, including ordinary text controls, declared-sensitive dual use, owner/Prepared Debug, build/transport/status errors, and raw-wire positive controls. |
| **F2 — P2 package target directory** | **RESOLVED** | Standalone package-check and verification’s package gate pass with the supplied `CARGO_TARGET_DIR`. Additional locked packaged-consumer runs pass on stable/default, stable/no-default and MSRV. |
| **F3 — P2 migration semantics** | **NOT RESOLVED** | Original `codec::Text` reproducer now passes; self-test passes all nine cases. Three additional import-resolution reproducers silently change the codec on both toolchains. |
| **F4 — P3 performance record** | **RESOLVED** | Recalculated archived confidence intervals and artifact sizes: documented exceptions are **43.24%** and **8.56%**; rlib changes span **+0.0306% to +1.1543%**, consistent with the corrected rounded claims. |

[Compiler results](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/compile-results.json), [stable runtime output](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/runtime-stable.txt), [MSRV runtime output](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/runtime-msrv.txt), [package output](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/package-check.txt), [performance reconciliation](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/perf-reconciliation.txt).

**New finding F3a — P2: file-wide import scanning still changes downstream codecs.**

[`imported_markers`](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/scripts/migrate_via.py:35) ignores Rust scope, cfg and comments. For example:

```rust
use crate::codec::Text;
mod unrelated { use concord::Text; }

// Inside the client declaration:
POST p(body b: String via Text);
```

The script emits `POST p(text b: String);`, although `Text` here resolves to the downstream codec. Both resulting format choices compile. The expected body is **`CUSTOM:value`**; the migrated body is **`value`**. A cfg-disabled Concord import and even a commented-out import trigger the same defect.

[Original fixture](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/migration/scoped_import-original.rs), [migrated fixture](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/migration/scoped_import.rs), [stable failures](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/migration-stable.txt), [MSRV failures](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/migration-msrv.txt).

Reproduce the semantic assertion failure:

```bash
CARGO_TARGET_DIR=target/probe-build cargo test --locked --offline \
  --manifest-path target/probes/migration/Cargo.toml \
  --lib tests::scoped_import -- --exact --nocapture
```

Preserve ambiguous bare names as format paths, or resolve their actual binding before replacing them with built-in words.

I personally ran `scripts/verify.sh --msrv --package` **once** from the fresh target directory. Exit **0**, with these exact gate results:

```text
PASS  fmt
PASS  records
PASS  migrate-via-self-test
PASS  clippy
PASS  test
PASS  test-no-default
PASS  test-json-only
PASS  test-reqwest-defaults-only
PASS  test-form-only
PASS  test-form-json
PASS  feature-isolation
PASS  test-multipart-alone
PASS  doc
PASS  doc-no-default
PASS  doc-json-only
PASS  doc-form-only
PASS  doc-form
PASS  msrv-concord
PASS  msrv-examples
PASS  msrv-multipart-alone
PASS  package-check
```

Workspace tests: **535 passed**. MSRV lanes: **416 / 96 / 11 passed**. [Verification output](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/verify-review.txt). Independent Rust probes used `--locked --offline` after offline lockfile generation. The package script runs unlocked consumer tests internally; the additional locked consumer runs all passed.

Inspected-only evidence includes the prior campaign’s measurements, mutation records and rendered-documentation claims. I inspected the changed implementation and records, and executed archive reconciliation; **I did not rerun benchmarks or nightly tests, or visually inspect rustdoc**.

All **806 tracked files**, including `Cargo.lock`, remain byte-identical. Every probe-server thread was joined; no background or documentation server was started. No push, publish, global-setting change or sandbox bypass occurred. [Integrity and cleanup receipt](/home/canarit/projects/concord_v4_native_work/snapshots/rereview-1174d5d/target/probes/source-integrity.json).