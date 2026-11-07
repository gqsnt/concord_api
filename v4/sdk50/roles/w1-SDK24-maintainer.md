# Independent maintenance capsule — SDK24 Discord REST

You maintain the SDK in `examples/discord-rest-sdk` (package `concord-sdk-discord-rest`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

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
  client → scope → endpoint is fixed. The frozen baseline is commit `9fbd2aca`: **do not edit
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

## Ticket

Discord's "Get Channel Messages" accepts `around` as a third, mutually exclusive alternative to
`before` and `after` (Discord message resource documentation; cite it). Add it so a caller can page
around a message id, make the three cursors mutually exclusive in the SDK's API (a caller cannot
send two), and keep the existing before/after paging and rate-limit behavior unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/discord-rest-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
