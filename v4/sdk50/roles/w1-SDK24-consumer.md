# Fresh consumer capsule — SDK24 Discord REST

You are a **fresh consumer** of the SDK in `examples/discord-rest-sdk` (package `concord-sdk-discord-rest`), which you did not write.
Read only its public documentation first: `examples/discord-rest-sdk/README.md` and the rustdoc you generate
(`cargo doc -p concord-sdk-discord-rest --no-deps`), plus Concord's public docs if needed. Do **not** read
`examples/discord-rest-sdk/src`, its `tests/`, `FRICTION.md` or `PROVENANCE.md` before your first working call. If
you must, write down what and why first — it changes the evidence label, not your permission.

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
  client → scope → endpoint is fixed. The frozen baseline is commit `d188ef77`: **do not edit
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

## Before writing code: predict

Write `examples/discord-rest-sdk/FIRST_USE.md` with a section "Predictions" answering: for each workflow, which calls
you will make, what they send on the wire (method, path, query, headers), what they return, and
when/how they fail (status errors, provider errors, malformed bodies, pagination end, secrets).

## Then build a small application (in `examples/discord-rest-sdk/tests/first_use.rs`)

Two caller tasks, one per workflow, written as tests against the loopback server with realistic
values and assertions on wire requests, errors, metadata, completion and partial results. Use the
ordinary caller path; use advanced APIs only where the task needs them.

Append to `FIRST_USE.md`: your first working call (what it took), every friction incident in
order (unneeded imports or types, unclear completion, surprising generated names, misleading
diagnostics, source lookups), a comparison of each prediction with observed behaviour, and the
assistance label (`docs-only` or `source-assisted: <files>`). Do not edit the SDK's `src/`.

## Report (final message)
Test results, prediction mismatches, friction list, assistance label.
