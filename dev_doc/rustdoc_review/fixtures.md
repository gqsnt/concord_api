# Parent representation review fixture

This is the reproducible input for tasks 6–7. It specifies a candidate, a
maintained client, and one exact edit. It contains no expected endpoint list or
classification answer key.

## Reviewer scenario

Use the canonical `CatalogApi` described in the public
[modeling guide](../../docs/modeling_apis.md#the-canonical-catalog) and declared
in [`catalog_v3.rs`](../../crates/concord-examples/src/contracts/catalog_v3.rs).
Its registered inspection target is `catalog`.

In an isolated checkout of the recorded candidate, insert exactly
`response Json;` as the first clause inside the existing
`scope regional(...) { ... }` declaration. Retain every existing client,
scope, and endpoint clause. Record the exact patch.

For task 6, predict the complete affected endpoint set and the kind of each
effect from the public declaration, generated documentation, and before
snapshot. Save this prediction and its confidence before opening the after
snapshot or diff. For task 7, inspect those outputs and compare them with the
saved prediction. Do not silently revise a prediction after seeing a report.

## Prepare the handoff

The coordinator performs these steps in an isolated checkout. Record the full
candidate SHA, platform, toolchain, exact patch, command exit statuses, and
output hashes. Keep the review output directory outside the checkout so
exports do not obscure the source-state record.

1. Build the candidate's public Rustdoc with warnings denied, including the
   maintained examples. Give the reviewer its public entry points; no
   macro/core implementation tour is needed.
2. Export the unchanged canonical client:

   ```bash
   mkdir -p ../concord-rustdoc-review-output
   cargo run --locked -p concord-xtask --features examples -- contract export --target catalog --output ../concord-rustdoc-review-output/catalog_before.contract.json
   ```

3. Apply only the scenario's insertion to
   `crates/concord-examples/src/contracts/catalog_v3.rs`. Retain its patch:

   ```bash
   git diff --binary -- crates/concord-examples/src/contracts/catalog_v3.rs > ../concord-rustdoc-review-output/candidate.patch
   ```

4. Export the changed compiled client and produce the semantic diff:

   ```bash
   cargo run --locked -p concord-xtask --features examples -- contract export --target catalog --output ../concord-rustdoc-review-output/catalog_after.contract.json
   cargo run --locked -p concord-xtask --features examples -- contract diff ../concord-rustdoc-review-output/catalog_before.contract.json ../concord-rustdoc-review-output/catalog_after.contract.json --format json > ../concord-rustdoc-review-output/catalog_scope_default.diff.json
   ```

5. Hand over the scenario, public docs, and before snapshot first. Reveal the
   after snapshot and diff only after the reviewer has recorded task 6.
   Preserve both the initial prediction and any corrected interpretation.

Successful `export` and `diff` commands exit `0`. A diff's exit status does not
mean the contracts are equal: read its `verdict`, `axes`, per-field changes,
endpoint associations, and `unresolved` facts. The separate `check` command
uses exit `1` for a structural difference. See the public
[contract tooling reference](../../docs/planning_and_observation.md#contract-tooling).

If either export fails, retain the actual failure and repair preparation
before giving the reviewer task 7. A source-only hypothetical is useful as a
prediction, but it is not evidence of an executed semantic diff.

## Evidence boundaries

The export and diff are inert inspection operations. Their result establishes
what the represented structural contract changed; it does not execute a
provider request or prove the behavior of an opaque Rust dependency. Review
the per-field associations as well as the summary endpoint list, and record
any association that cannot be explained through public documentation.

The fixture can be performed by an agent or a human. Record that distinction
in the review report, and keep agent evidence out of the human-results form.
