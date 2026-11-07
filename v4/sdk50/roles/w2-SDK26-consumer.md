# Fresh consumer capsule — SDK26 Postmark

You are a **fresh consumer** of the SDK in `examples/postmark-sdk` (package `concord-sdk-postmark`), which you did not write.
Read only its public documentation first: `examples/postmark-sdk/README.md` and the rustdoc you generate
(`cargo doc -p concord-sdk-postmark --no-deps`), plus Concord's public docs if needed. Do **not** read
`examples/postmark-sdk/src`, its `tests/`, `FRICTION.md` or `PROVENANCE.md` before your first working call. If
you must, write down what and why first — it changes the evidence label, not your permission.

## Provider slice: SDK26 · Postmark (small; messaging and CRM)

Connected workflows (both required):
1. Send a transactional email, then inspect its outbound message by returned ID.
2. List outbound messages by count and offset, then retrieve one item.

Contract notes: Server token header; JSON; API may return 429 and send acceptance differs from delivery.
Failure that must be exercised: Timeout after send reports unknown outcome rather than sending a second email.
Excluded: Inbound parsing, webhooks, batch sending and delivery guarantees.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://www.postmarkapp.com/developer/api/overview
- https://postmarkapp.com/developer/api/email-api
- https://postmarkapp.com/developer/api/messages-api
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `4f29a81730d9e1bc29b9b8b356aea305c9e12fd2`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/postmark-sdk/` plus adding `"examples/postmark-sdk"` to the root `Cargo.toml`
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

Write `examples/postmark-sdk/FIRST_USE.md` with a section "Predictions" answering: for each workflow, which calls
you will make, what they send on the wire (method, path, query, headers), what they return, and
when/how they fail (status errors, provider errors, malformed bodies, pagination end, secrets).

## Then build a small application (in `examples/postmark-sdk/tests/first_use.rs`)

Two caller tasks, one per workflow, written as tests against the loopback server with realistic
values and assertions on wire requests, errors, metadata, completion and partial results. Use the
ordinary caller path; use advanced APIs only where the task needs them.

Append to `FIRST_USE.md`: your first working call (what it took), every friction incident in
order (unneeded imports or types, unclear completion, surprising generated names, misleading
diagnostics, source lookups), a comparison of each prediction with observed behaviour, and the
assistance label (`docs-only` or `source-assisted: <files>`). Do not edit the SDK's `src/`.

## Report (final message)
Test results, prediction mismatches, friction list, assistance label.
