# Independent maintenance capsule — SDK27 Mailgun

You maintain the SDK in `examples/mailgun-sdk` (package `concord-sdk-mailgun`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK27 · Mailgun (medium; messaging and CRM)

Connected workflows (both required):
1. Send a text email with a file attachment, then retain the returned message identifier.
2. Search and page domain events, then inspect delivery status for that message.

Contract notes: HTTP Basic api:key; regional API endpoints; messages use multipart, events use provider links.
Failure that must be exercised: A non-rewindable attachment or ambiguous send timeout must not be automatically replayed.
Excluded: Inbound routes, MIME construction and any claim that accepted means delivered.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-http
- https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages
- https://documentation.mailgun.com/docs/mailgun/user-manual/events/events
- https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `497bf896cc63855095a1d2310ffdcb6ef697d85e`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/mailgun-sdk/` plus adding `"examples/mailgun-sdk"` to the root `Cargo.toml`
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

Callers need to tag sends and attach custom data for later event searches. Mailgun's Messages API
documents `o:tag` (up to 3 tags per message, sent as a repeated form field) and custom variables
`v:<name>` (JSON values, returned in events as `user-variables`) — read the send-message reference
and the tagging / custom-variables pages, cite them with the date. Add both to sends so they work
with in-memory and with streamed (non-rewindable) attachments alike, refuse a fourth tag or an
invalid variable name before any I/O, and let a caller filter domain events by tag. Keep the
existing send, events paging and delivery-status behavior unchanged.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/mailgun-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
