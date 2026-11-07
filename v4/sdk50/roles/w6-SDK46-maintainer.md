# Independent maintenance capsule — SDK46 Anthropic Messages

You maintain the SDK in `examples/anthropic-messages-sdk` (package `concord-sdk-anthropic-messages`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK46 · Anthropic Messages (complex; streaming and bulk)

Connected workflows (both required):
1. Create a non-streaming message with text input and inspect the returned blocks.
2. Create a streaming message and incrementally consume events through message_stop.

Contract notes: API-key authentication; SSE event types carry JSON payloads; error events can occur after successful HTTP setup.
Failure that must be exercised: A midstream error or EOF before message_stop is incomplete, not a successful final message or safe automatic restart.
Excluded: Tool execution, batches, extended thinking interpretation and deterministic output equivalence.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://platform.claude.com/docs/en/build-with-claude/streaming
- https://platform.claude.com/docs/en/api/errors
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/anthropic-messages-sdk/` plus adding `"examples/anthropic-messages-sdk"` to the root `Cargo.toml`
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

## Ticket

Callers need to count a request's input tokens before sending it: the token-counting endpoint
(`POST /v1/messages/count_tokens`) takes the same model, system, messages and tools as a message
request (no `max_tokens`, no streaming) and returns `input_tokens`. Read the token-counting page
and the errors page and cite them with the date:
- https://platform.claude.com/docs/en/build-with-claude/token-counting
- https://platform.claude.com/docs/en/api/errors

Add token counting so a caller passes the parts of an existing message request it already built
(without re-typing them) and gets the count; it uses the same key, version header, error
documents and request-id handling as message creation; a counting call never becomes a
generation. Keep message creation and streaming unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/anthropic-messages-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
