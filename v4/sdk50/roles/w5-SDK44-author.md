# SDK author capsule — SDK44 Backblaze B2 Native

You are the **author** of a bounded SDK for Backblaze B2 Native built on the Concord Rust library
(your working directory is a git worktree of the Concord workspace at the frozen baseline).
Read `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md` and the rustdoc of the crates you use. You may read
existing SDKs under `examples/*-sdk` as examples: **record every example file you consult and
what you took from it** (this makes the result example-assisted, which is fine, but must be
labelled).

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

## Deliver (package `concord-sdk-backblaze-b2-native` in `examples/backblaze-b2-native-sdk/`)

- `src/`: the declaration (`concord::api!`), the SDK's public types, its error type and any send
  policy, adapters for pagination/auth/retry/rate limits as the slice needs. Use Concord's
  ordinary declaration and generated `send()` path first; shared capabilities where they help
  (`concord-pagination`, `concord-retry`, `concord-auth`, `concord-ratelimit`, `concord-codecs`
  features — read each crate's README before deciding; if one does not fit, say why in
  `FRICTION.md` rather than bending the SDK to it); a native escape only where
  the provider contract needs it, documented. Keep provider models and rules in the SDK.
- `tests/`: `protocol.rs` (wire-level request shapes, every documented error class, malformed /
  truncated / oversized / wrong-media bodies, redaction of secrets in `Debug`/`Display`),
  `workflows.rs` (both connected workflows end to end against fixtures, including the required
  failure). Fixtures under `fixtures/`, each derived from a cited documentation example or
  authored and labelled synthetic; never copy expectations from your own implementation.
- `README.md` (public docs: what the slice covers, how to call it, failure semantics, what is
  excluded), `PROVENANCE.md` (every doc page + retrieval date, fixture derivation table),
  `FRICTION.md` (each Concord cost you hit: observable evidence, what you had to write by hand,
  provider semantics vs generic mechanics, the smallest Concord change that would remove it;
  include the examples you consulted).
- `cargo test --locked -p concord-sdk-backblaze-b2-native` passes; `cargo clippy --locked -p concord-sdk-backblaze-b2-native --all-targets -- -D
  warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-backblaze-b2-native --no-deps` pass.

Do not shrink the slice to a single list endpoint; implement both workflows completely.

## Slice-specific notes

The application key authorizes the account and returns the API URL and download URL to use; an
upload first obtains an upload URL with its own token, and a 503 can require a new upload URL. An
upload carries its exact length and SHA-1. A consumed stream cannot be replayed, and a new upload
URL does not prove that an earlier ambiguous upload did not succeed: surface uncertainty, never
claim exactly-once.

## Report (final message)
Files, line counts (src vs tests vs fixtures), the Concord features you used and for what, every
friction item (short), examples consulted, commands and results.
