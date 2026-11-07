# Fresh consumer capsule — SDK44 Backblaze B2 Native

You are a **fresh consumer** of the SDK in `examples/backblaze-b2-native-sdk` (package `concord-sdk-backblaze-b2-native`), which you did not write.
Read only its public documentation first: `examples/backblaze-b2-native-sdk/README.md` and the rustdoc you generate
(`cargo doc -p concord-sdk-backblaze-b2-native --no-deps`), plus Concord's public docs if needed. Do **not** read
`examples/backblaze-b2-native-sdk/src`, its `tests/`, `FRICTION.md` or `PROVENANCE.md` before your first working call. If
you must, write down what and why first — it changes the evidence label, not your permission.

## Provider slice: SDK44 · Backblaze B2 Native (complex; infrastructure and observability)

Connected workflows (both required):
1. Authorize account and obtain an upload URL, then upload a small file with required length and SHA-1.
2. Download that file by ID and verify its returned checksum.

Contract notes: Native application-key auth returns API/download URLs; upload URL has a distinct token; 503 can require a new upload URL.
Failure that must be exercised: A new upload URL alone cannot replay a consumed stream or prove an earlier ambiguous upload did not succeed.
Excluded: Large-file parts, lifecycle rules, S3 compatibility, server-side encryption variants and silent exactly-once guarantees.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://www.backblaze.com/apidocs/b2-authorize-account
- https://www.backblaze.com/apidocs/b2-get-upload-url
- https://www.backblaze.com/apidocs/b2-upload-file
- https://www.backblaze.com/apidocs/b2-download-file-by-id
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/backblaze-b2-native-sdk/` plus adding `"examples/backblaze-b2-native-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.
- Read nothing outside your worktree except the official documentation pages above and the
  Rust toolchain / cargo registry sources: in particular, not the campaign files in its parent
  directories (mission, contracts, records, other roles' briefs or worktrees).

## Before writing code: predict

Write `examples/backblaze-b2-native-sdk/FIRST_USE.md` with a section "Predictions" answering: for each workflow, which calls
you will make, what they send on the wire (method, path, query, headers), what they return, and
when/how they fail (status errors, provider errors, malformed bodies, pagination end, secrets).

## Then build a small application (in `examples/backblaze-b2-native-sdk/tests/first_use.rs`)

Two caller tasks, one per workflow, written as tests against the loopback server with realistic
values and assertions on wire requests, errors, metadata, completion and partial results. Use the
ordinary caller path; use advanced APIs only where the task needs them.

Append to `FIRST_USE.md`: your first working call (what it took), every friction incident in
order (unneeded imports or types, unclear completion, surprising generated names, misleading
diagnostics, source lookups), a comparison of each prediction with observed behaviour, and the
assistance label (`docs-only` or `source-assisted: <files>`). Do not edit the SDK's `src/`.

## Report (final message)
Test results, prediction mismatches, friction list, assistance label.
