# SDK24 maintenance: exclusive message cursors

Date: 2026-11-07. Baseline: `9fbd2acad2672f22e10b8785f2daa110fd93ab63`.
Live provider calls: **NOT_RUN**. The only HTTP calls were loopback fixtures.

This is a historical record of the wrapper implementation. The later `query(..)` adoption
replaced those wrappers while retaining `anchor` and `maybe_anchor`.

## Prior exposure and first attempt

Before implementing, I read the inherited `AGENTS.md` and handoff instructions, the SDK
`src/api.rs`, `src/lib.rs`, `src/model.rs`, manifest, provenance, `tests/first_use.rs`,
`tests/protocol.rs`, and `tests/workflows.rs`; I inspected relevant Concord macro syntax and
emission and the workspace manifest/status. I independently fetched the four official pages
listed below. I did **not** read `FRICTION.md` or `FIRST_USE.md` until after writing the first
implementation and starting `cargo test --offline --no-run` (which subsequently passed). The existing
code already had `Anchor::Around` and an `anchor` convenience method, but its public generated
`set_around`, `set_before`, and `set_after` and `maybe_` forms could bypass that method and send
conflicting cursors. The historical friction report subsequently confirmed that limitation.

## Provider evidence and decision

On 2026-11-07 I read Discord's [Get Channel Messages](https://docs.discord.com/developers/resources/message#get-channel-messages),
which explicitly allows at most one of `around`, `before`, and `after`; `around` selects messages
around the given message ID. I also read the [rate-limit page](https://docs.discord.com/developers/topics/rate-limits),
[Guild Resource](https://docs.discord.com/developers/resources/guild), and
[API Reference](https://docs.discord.com/developers/reference). Discord documents bucket hashes
excluding the top-level resource and separate global limits. These are provider facts, not inferred
from the implementation. The old wire declaration already included all three cursor names, so
adding `around` again would not close the problem; the caller contract needed to enforce it.

The SDK now wraps the generated client, channel, and message-listing request. The raw generated
handles are private, and a public listing request can only choose one `Anchor` at a time. A later
`anchor` replaces the first; `maybe_anchor(None)` clears it. The SDK retains the generated
endpoint, its original operation identity, `Endpoint`/`Sends`, `prepare`, and the same send
policy. History maps its optional `before` cursor through the public anchor API. This deliberately
removes the six raw cursor setters from the public API; callers must migrate to `anchor` or
`maybe_anchor`. Other forwarded client/channel methods retain the generated `IntoValue` inputs.
No changes to rate limiting or message creation were needed.

## Locations and repeated facts

- `src/api.rs`: put the unchanged generated declaration in a private module, forward client and
  channel navigation and controls, wrap the listing endpoint, implement safe anchor selection,
  forwarding traits, and change history's `before` call. A compile-fail example checks that the
  three raw setters are not visible. The three cursor variants are still represented in the raw
  declaration and in the wrapper's mapping: adding another provider cursor requires editing both.
- `src/lib.rs`: correct the statement that the public client is the generated client.
- `README.md`: document the cursor invariant, the migration from raw setters, and a working
  `around` example; correct the description of public versus generated calls.
- `PROVENANCE.md`: record this maintainer's dated read of the exact provider pages and synthetic
  fixture. The same cursor fact appears in the API docs, README, and provenance.
- `tests/cursor_regression.rs`: inspect all nine ordered pairs of cursor choices in prepared URLs,
  and clearing each choice, without I/O. The existing protocol test already covers `around`,
  `before`, and `after` wire shapes, but did not prove the raw setters were unavailable.
- `tests/around_consumer.rs`: a separate public consumer sends `around` with a limit to a local
  server and checks the decoded message, exact query and Bot authorization.
- `MAINTENANCE.md`: this record. `tests/first_use.rs` and all its assertions remain unchanged.
  The workspace member and lockfile package entry already existed at baseline; neither changed.

The first implementation compiled on its first `--no-run` build; no omitted edit was caught by
that compiler invocation. The complete test run passed on its first attempt; it did not catch an
omitted edit. Compile-fail tests specifically check that the unsafe setter surface stays absent;
the transition and consumer tests check the resulting requests. No omission/mutation experiment
was performed, so there is no claim that tests detected one.

## Remaining duplication and ownership

The wrapper forwards client constructors, settings, channel navigation, `prepare`, `send`, and
Concord traits even though Concord already generates most of those methods. It adds a separate
public layer around the client and channel solely to hide the raw listing handle, and mirrors
cursor names when translating the enum into generated optional fields. This is generic wrapper
and declaration-to-public-API duplication, not new Discord behavior. The policy still parses
resource context from the constructed URL; the limiter still implements generic window, alias,
and clock mechanics locally. Discord's cursor meaning, bucket/resource key, header interpretation,
permissions, and ambiguous-write semantics remain SDK responsibilities; durable reconciliation
and decisions about deliberate retries remain application responsibilities.

Candidate Concord improvements, **not implemented in the frozen shared crates**: support an
SDK-controlled visibility or safe public projection of generated operations, or a composable
single enum value projected to mutually exclusive query keys, while preserving client → scope →
endpoint and generated metadata. Compare both against this wrapper with real consumers before
adding syntax. Exposing redaction-aware scope context or a static path template to send policies
could avoid URL re-parsing, and a generic admission capability could separate window/alias mechanics
from Discord's key and header rules. These are candidates, not tested core changes.

## Verification

All Cargo commands used the default worktree `target/`, `CARGO_BUILD_JOBS=4`, offline mode,
and one Cargo command at a time. The first `cargo test --offline -p concord-sdk-discord-rest
--no-run` ran without `--locked` and passed; the lockfile stayed unchanged. Subsequent commands
used `--locked`. The full package suite passed, including the unchanged first-use workflows,
shared-bucket, global-429 and ambiguous-write tests. Clippy with `--all-targets -- -D warnings`
and rustdoc with `RUSTDOCFLAGS='-D warnings'` passed. No new dependencies or shared-crate edits.
