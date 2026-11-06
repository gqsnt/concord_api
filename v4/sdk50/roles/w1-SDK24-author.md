# SDK author capsule — SDK24 Discord REST

You are the **author** of a bounded SDK for Discord REST built on the Concord Rust library
(your working directory is a git worktree of the Concord workspace at the frozen baseline).
Read `AGENTS.md`/`CLAUDE.md`, `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md` and the rustdoc of the crates you use. You may read
existing SDKs under `examples/*-sdk` as examples: **record every example file you consult and
what you took from it** (this makes the result example-assisted, which is fine, but must be
labelled).

## Provider slice: SDK24 · Discord REST (medium; messaging and CRM)

Connected workflows (both required):
1. List a guild’s channels, then page messages in a selected channel and fetch one message by ID.
2. Using a fake transport, create a message in a channel and read it back by returned ID.

Contract notes: Bot token via Authorization: Bot; channel-message reads require appropriate channel permissions. Per-route limits can share X-RateLimit-Bucket, whose identity excludes the top-level channel/guild resource; global limits are separate. HTTP 429 provides Retry-After or JSON retry_after, possibly fractional, and scope/global metadata.
Failure that must be exercised: Two channels sharing a bucket hash must retain their resource context; a global 429 gates the bot globally, and a timeout after message creation is an ambiguous write, not permission to resend.
Excluded: Gateway, OAuth installation, reactions, attachments, webhooks, generalized Discord coverage and live message creation.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.discord.com/developers/topics/rate-limits
- https://docs.discord.com/developers/resources/guild
- https://docs.discord.com/developers/resources/message
- https://docs.discord.com/developers/reference
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `56a16ee3`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/discord-rest-sdk/` plus adding `"examples/discord-rest-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.

## Deliver (package `concord-sdk-discord-rest` in `examples/discord-rest-sdk/`)

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
- `cargo test --locked -p concord-sdk-discord-rest` passes; `cargo clippy --locked -p concord-sdk-discord-rest --all-targets -- -D
  warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-discord-rest --no-deps` pass.

Do not shrink the slice to a single list endpoint; implement both workflows completely.

## Slice-specific notes

Discord's rate limits are part of this slice (see the rate-limits page): route buckets identified
by `X-RateLimit-Bucket` together with the route's major parameter (channel/guild id), separate
global limits, 429 bodies with `retry_after` (seconds, possibly fractional) and `global`, and
`X-RateLimit-Scope`. Concord has no rate-limiting crate yet: implement what the slice needs in the
SDK, as simply as you can, and record in `FRICTION.md` exactly what you had to write (this is
design evidence for a shared rate-admission capability). A timeout after a message creation is an
ambiguous write: never resend automatically. `concord-retry` exists if you want bounded retries
of safe reads.

## Report (final message)
Files, line counts (src vs tests vs fixtures), the Concord features you used and for what, every
friction item (short), examples consulted, commands and results.
