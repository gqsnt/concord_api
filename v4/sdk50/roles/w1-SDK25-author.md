# SDK author capsule — SDK25 FRED

You are the **author** of a bounded SDK for FRED built on the Concord Rust library
(your working directory is a git worktree of the Concord workspace at the frozen baseline).
Read `AGENTS.md`/`CLAUDE.md`, `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md` and the rustdoc of the crates you use. You may read
existing SDKs under `examples/*-sdk` as examples: **record every example file you consult and
what you took from it** (this makes the result example-assisted, which is fine, but must be
labelled).

## Provider slice: SDK25 · FRED (medium; public data)

Connected workflows (both required):
1. Search series and retrieve the selected series metadata.
2. Read observations as XML or JSON and export the supported zipped CSV.

Contract notes: FRED v1 query API key; XML default, JSON and ZIP containing CSV for observations; limit/offset.
Failure that must be exercised: Compressed CSV is not a plain CSV body; reject oversized archives and preserve missing-value strings.
Excluded: FRED v2 bulk release archive and XLSX parsing.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://fred.stlouisfed.org/docs/api/fred/series_search.html
- https://fred.stlouisfed.org/docs/api/fred/series_observations.html
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `56a16ee3`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/fred-sdk/` plus adding `"examples/fred-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.

## Deliver (package `concord-sdk-fred` in `examples/fred-sdk/`)

- `src/`: the declaration (`concord::api!`), the SDK's public types, its error type and any send
  policy, adapters for pagination/auth/retry/rate limits as the slice needs. Use Concord's
  ordinary declaration and generated `send()` path first; shared capabilities where they help
  (`concord-pagination`, `concord-retry`, `concord-codecs` features); a native escape only where
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
- `cargo test --locked -p concord-sdk-fred` passes; `cargo clippy --locked -p concord-sdk-fred --all-targets -- -D
  warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-fred --no-deps` pass.

Do not shrink the slice to a single list endpoint; implement both workflows completely.

## Slice-specific notes

FRED serves observations as XML (default), JSON, and a zipped CSV export. Concord's codecs have no
XML or ZIP support: choose the simplest correct route (an XML parser dependency, a custom
`Decode` format, a native escape), keep it bounded (archive and decompressed-size limits), and
record the cost in `FRICTION.md` — this is evidence for whether a shared XML codec is justified.
The API key is a query parameter and must never appear in `Debug`/`Display` or error text.

## Report (final message)
Files, line counts (src vs tests vs fixtures), the Concord features you used and for what, every
friction item (short), examples consulted, commands and results.
