# Independent maintenance capsule — SDK26 Postmark

You maintain the SDK in `examples/postmark-sdk` (package `concord-sdk-postmark`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

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
  client → scope → endpoint is fixed. The frozen baseline is commit `75139f9517d0c6cadab34c4b56df955677b106e3`: **do not edit
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

## Ticket

Callers need to send several transactional emails in one request. Postmark's Email API documents
batch sending (`POST /email/batch`, up to 500 messages per call; read the Email API page and cite
the section and date). Its successful HTTP response carries one result per message, each with its
own `ErrorCode` and `Message`: some messages can be accepted while others are rejected in the
same 200 response. Add batch sending to the SDK so a caller can tell, per message, accepted (with
its message id) from rejected (with the provider's code, without leaking provider text in
`Display`/`Debug`), and so a batch whose response is lost or unreadable is reported as an unknown
outcome for the whole batch and never resent. Keep single-message sending, listing and details
unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/postmark-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
