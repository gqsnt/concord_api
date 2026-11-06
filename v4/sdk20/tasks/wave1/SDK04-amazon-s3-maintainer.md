# SDK04 Amazon S3 — maintenance change

Requested runner: Codex gpt-6.1-terra (xhigh). You did not write this SDK.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree of the frozen candidate commit `94d13527`. Prefix every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b` (a shared build cache; if cargo says it is
  waiting for a lock, wait).
- **Frozen library:** do not edit `crates/`, `scripts/`, `docs/`, `checks/` or other examples.
  If Concord forces a workaround (private reach-through, rebuilt execution, handwritten operation
  names, a native escape you think should not be needed), keep the working code and record it as
  a friction entry with a reproduction; do not patch the library.
- Use Concord's public API (`concord`, `concord-pagination`, `concord-extras` through the facade)
  and its docs: `README.md`, `docs/*.md`, crate rustdoc (`cargo doc -p concord --no-deps
  --features csv,jsonl`), `crates/*/README.md`. The five existing provider SDKs under `examples/`
  are normal reading material (say so if one supplied your solution).
- No live provider calls, no credentials, no secrets: all auth values are synthetic and must not
  look like real credentials. Tests run offline against the loopback server in
  `examples/testserver` (a dev-dependency by path).
- Fixture bytes are hand-authored from the official documentation unless you actually obtained a
  documented example; record provenance honestly. Put byte-sensitive fixtures under
  `examples/amazon-s3-sdk/fixtures/` and keep `examples/amazon-s3-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The change ticket (chosen before you saw the SDK)

- Change Unicode key and signed range/header

The provider brief's perturbation paragraph, verbatim:

- Perturbation: change a key to include spaces/Unicode and change a signed range or content header; verify canonical request, signature, both list pages and unrelated calls. Unknown XML elements must not prevent decoding expected fields.

Do it in `examples/amazon-s3-sdk/` (source, README, tests) preserving the existing contract: the
existing workflows and first-use tests must keep passing unless the real provider contract
requires caller changes (then show and justify them). Add tests that exercise the change on the
wire. Do not read `FRICTION.md` or `FIRST_USE.md` before finishing a first attempt.

Write `examples/amazon-s3-sdk/MAINTENANCE.md`: the edits by kind (declaration, adapter, model,
tests, docs), how many independent places encode the same fact and had to change, what you had
to read to find them, any generated-name or compile surprise, and whether the change felt local.

Command: `cargo test --locked -p concord-sdk-amazon-s3` (all tests).
Final message: files changed, tests passing, and whether one semantic change touched several
independently maintained descriptions of the same fact.
