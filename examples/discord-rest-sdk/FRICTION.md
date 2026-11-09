# SDK24 Discord REST — author observations

Current API note (P4.2): this evidence records the earlier API. Paging `_from` twins now use the base factory followed by `.starting_at(Resume::At(key))`; Datadog/Notion traversal cycles use `PagesError::Repeated`. See [migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Baseline: Concord `56a16ee3` (worktree `w1-sdk24-discord-rest`), Rust 1.99.1, edition 2024.
Changes: `examples/discord-rest-sdk/` and its root `Cargo.toml` member entry / `Cargo.lock`
package entry only. No shared crate, check, script or other example was edited. No live Discord
call, no real credential. Author: a Claude Code subagent; the run's system prompt states model
ID `claude-opus-5-5` (a harness statement, recorded as such, not verified here).

Line references are to this package. "Code lines" exclude blank and comment lines.

## What the core, macros and ordinary usage did

1. **The four routes are one 27-code-line declaration; there is no URL, header or body code.**
   Concord (positive). `src/api.rs:24–88`: a client with a `#[sensitive]` token in
   `AUTHORIZATION = "Bot {token}"`, a required `User-Agent` as an initialized client value
   (`header(USER_AGENT) user_agent: String = default_user_agent()`), `format Json;`, two scopes
   (`guild(guild_id)` → `guilds/{guild_id}`, `channel(channel_id)` → `channels/{channel_id}`)
   and four endpoints. Callers write `discord.channel(id).messages().set_limit(l).send()`.
   Evidence: `tests/protocol.rs:23,57,101,127` assert exact targets, headers and the JSON body
   on the wire. The client → scope → endpoint shape matched Discord's top-level resources
   exactly, which later mattered for rate limits (item 4).

2. **A stateful send policy carried the whole provider contract with one generic impl.**
   Concord (positive). `send limiter;` names a required client value (`RateLimiter`, an `Arc`),
   and `impl<T: Send + 'static> SendWith<T> for RateLimiter` (`src/limiter.rs:357`, 47 code
   lines) gives every generated `send()` admission, one execution, header observation, 429 and
   error-document mapping, and write-ambiguity classification. Unlike the S3 example, no macro
   per output type was needed (all outputs are plain JSON `T`). `received.document::<Json, D>(n)`
   read both the 429 body and other error documents bounded, with the status kept when the
   document was HTML, oversized, cut off or foreign (`tests/protocol.rs:314`). The generated
   `Debug` withholds both the token and the policy value (`Discord { .., token: <redacted>,
   limiter: <redacted>, .. }`, observed).

3. **Inherent methods on generated types cover provider rules the declaration cannot state.**
   Concord (positive, with a limit). Discord accepts at most one of `around`/`before`/`after`
   (M). Concord has no "one of" for query values, so `src/api.rs:92` adds
   `DiscordChannelMessages::anchor(Anchor)` (12 code lines) that sets one and clears the others,
   and `src/api.rs:118` adds `DiscordChannel::history(limit)` on the generated scope type. Both
   live next to the generated API in rustdoc. The raw `set_before`/`set_after` setters remain, so
   a caller can still send two anchors; Discord then decides. No Concord change proposed: an
   exclusivity construct would be a new language feature for one provider rule.

## Rate admission (design evidence for a shared capability)

4. **The policy cannot see the declared scope values, so the SDK re-parses the URL Concord just
   built to recover Discord's major parameter.** Concord API limitation. Discord keys limits by
   `X-RateLimit-Bucket` *plus* the top-level resource (channel/guild ID), which is exactly the
   scope value. Inside `SendWith::send` the policy has only `E::operation()` (static name and
   method) and the built `reqwest::Request`. `src/limiter.rs:130` `resource_of` walks the URL
   path for the first `channels/{id}` or `guilds/{id}` pair (22 code lines, plus a unit test);
   it would misread a replaced base URL containing those words (documented). The same gap means
   `history`'s protocol error takes the channel from the returned messages' `channel_id`
   (`src/api.rs:135`) because a generated scope handle has no getter for its own value.
   Smallest change: a static, value-free `Operation::path_template()` (e.g.
   `channels/{channel_id}/messages/{message_id}`) so an SDK can locate a named placeholder's
   segment in the built URL without guessing; or, larger, a redaction-aware accessor for a
   declared value's wire text on `Prepared`.

5. **The rate limiter itself is ~260 code lines written by hand.** SDK work; Concord has no
   admission crate. `src/limiter.rs:151–354` (165 code lines: windows, route→bucket map,
   global gate, `admit`, `observe`, `rate_limited`), `:355–405` (the policy, 47), `:406–466`
   (header/seconds helpers, 49). Split by kind:
   - *Generic mechanics* (would fit a shared crate): a map of keyed windows
     `{remaining, reset_at}`; a global gate; refuse-before-I/O with the remaining delay;
     never shortening a delay (max of competing minimums); pruning expired windows; an injected
     monotonic clock (`RateLimiter::with_clock`, used by every timing test); a key alias table
     (route → shared bucket) learned from responses.
   - *Provider semantics* (stay in the SDK): which headers carry the window
     (`X-RateLimit-Remaining`, `-Reset-After`, not the epoch `-Reset`); key = bucket hash +
     major parameter; scope rules (`X-RateLimit-Scope`, `X-RateLimit-Global`, body `global`,
     any one of them means global); delay precedence (larger of `Retry-After` and body
     `retry_after`, else `Reset-After`, else none — never invent zero).
   Evidence that the mechanics are load-bearing (temporary mutations, reverted): keying windows
   without the resource fails `two_channels_sharing_a_bucket_hash_keep_their_resource_context`;
   not gating a global 429 fails `a_global_429_gates_the_bot_globally`; preferring the body's
   64.57 s over the header's 65 s fails three workflow and two protocol tests.
   Smallest shared capability: an admission type generic over a key `K` with
   `admit(&K, now) -> Result<(), Wait>`, `record(K, remaining, reset_at)`, `alias(route, K)`,
   `gate_all(until)` and an injected clock, plus a hook in a send policy; the SDK supplies the
   key and the header projection. Not built: queueing/sleeping, proactive 50 req/s pacing, and
   serializing concurrent requests on one window (two admitted requests can both meet a 429).

6. **The shared `Retry-After` parser could not be used: Discord's delays are fractional.**
   Concord capability mismatch. `concord_retry::http::retry_after` accepts RFC 9110
   delay-seconds (integers) and IMF-fixdate and deliberately rejects fractions. Discord's
   `X-RateLimit-Reset-After` and body `retry_after` are documented as fractional (RL), and the
   capsule says `Retry-After` may be too. `src/limiter.rs:433` `decimal_seconds` (19 code
   lines, rounds sub-nanosecond digits up) parses all three header forms; the body value is an
   `f64` through `Duration::try_from_secs_f64`. Smallest change: a
   `concord_retry::http::decimal_seconds` (or a fractional mode) next to the strict parser.
   `concord-retry` is therefore not a dependency; the SDK retries nothing, so its engine was not
   needed either.

7. **`Operation` is `Eq` but not `Hash`, so a route map needs its own key.** Concord API, minor.
   `src/limiter.rs:155` `RouteKey { name, method }` copies the two static fields. Smallest
   change: derive `Hash` on `Operation` (its `PartialEq` already compares exactly those).

## Writes

8. **"May this write have been applied?" is decided from the method and reqwest's error
   taxonomy.** SDK classification over Concord + reqwest. `src/limiter.rs:92`
   `failed_before_head`: a non-`GET`/`HEAD` operation whose failure before a head is anything
   but `transport().is_connect()` is `Error::AmbiguousWrite { accepted: None }`; a 2xx whose body
   fails is `AmbiguousWrite { accepted: Some(status) }` (created, ID unknown). Nothing is resent;
   `CreateMessage::with_nonce` sets `enforce_nonce` so a *deliberate* resend within Discord's
   window returns the existing message. Evidence: `tests/workflows.rs:487` (a 300 ms client
   timeout against a held response: one `POST` on the wire, no resend within 500 ms, reconciled
   by `anchor(After(newest))` and the nonce, then one deliberate resend returning the same ID),
   `tests/protocol.rs:511` (a cut 2xx body and a hang-up), `tests/protocol.rs:544` (a refused
   connection is *not* ambiguous). Concord's `Prepared::send`/`receive` perform one `execute`,
   which made "never resent" a property of the SDK alone; the reqwest client in tests is built
   with `retry(reqwest::retry::never())` (advice taken from the `concord-retry` README) so wire
   counts are exact. Smallest change: none required; a documented note in the caller guide that
   `is_connect()` is the only "definitely not sent" signal would have saved the search.

## Pagination

9. **`concord-pagination::Pages` handled bounding and resumption; the SDK only derives the next
   anchor.** Concord (positive). Discord has no cursor: the next `before` is the oldest ID of
   the page, and a short page ends the walk (an SDK inference, see PROVENANCE). `src/api.rs:123`
   builds `Pages::new(from, |before| channel.messages().maybe_before(before).set_limit(limit)
   .send())` over the cloned scope handle (the generated handle is `Clone + 'static`, so the
   factory is three lines), and `history_page` (`src/api.rs:135`) checks order and size so a bad
   page is `Error::Protocol`, not a loop. A rate-limited page ends the walk with
   `PagesError::Provider { failed: At(anchor) }`; `history_from(failed, limit)` resumes after the
   delay with no gap or duplicate (`tests/workflows.rs:435`). 46 code lines in total.

## Small costs

10. Clippy's `double_must_use` refused `#[must_use]` on `anchor` because the generated request
    type is already `#[must_use]` (removed; observed on the first clippy run).
11. `AGENTS.md`/`CLAUDE.md`, named by the capsule, do not exist in this worktree (observed with
    `ls`); the README and docs were read instead.

## Examples and documents consulted

Example-assisted, as allowed. Files read, and what was taken:

| File | Taken |
|---|---|
| `examples/slack-sdk/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/wire.rs` | Dependency set; `Pages::new(Resume::Start, ..)` over a cloned client; the idea of a hand-written `Debug` that prints header names only. Its private-declaration-plus-wrapper shape was **not** adopted (a send policy kept the generated `send()` public). |
| `examples/slack-sdk/tests/support/mod.rs`, `tests/protocol.rs` (first ~140 lines) | Test support layout: a `client(server)` helper with `no_proxy()` and no redirects, `include_bytes!` fixture table, `assert_get` header checks. |
| `examples/slack-sdk/PROVENANCE.md`, `FRICTION.md` (first ~60 lines each), `.gitattributes` | Document structure; `fixtures/** -text`. |
| `examples/frankfurter-sdk/Cargo.toml`, `src/lib.rs` | `SendWith` with `received.document::<Json, Problem>(limit)` for non-2xx and decoding on 2xx. |
| `examples/amazon-s3-sdk/src/api.rs` (lines 1–80), `src/client.rs` (lines 1–200) | `send signer;` with a client value holding `Arc` state; withholding it from `Debug`. |
| `examples/twilio-sdk/src/error.rs` (lines ~103–126), `src/lib.rs` (grep) | Looked for an ambiguous-write pattern; nothing taken. |
| `examples/testserver/Cargo.toml`, `src/lib.rs` | `TestServer`, `Reply` (`json`, `text`, `hang_up`, `after(Gate)`), `Body::{FailAfter, Generate}`, `Recorded`, `wait_until`. |
| greps over `examples/*` for `retry_after`/`RateLimit`, `ambiguous`, `send ..;` | File lists only, to find the examples above. |
| `README.md`, `docs/QUICKSTART.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`, `docs/AUTHOR_GUIDE.md` (§1–3 start, §5.11–§11) | The language, send policies, `Received::document`, `Endpoint`, re-export advice (`pub use concord;`). |
| `crates/concord-core/src/{lib,response,prepared,error}.rs` (signatures, `Operation`, `Received`, `Error`) | Exact bounds and accessors (`head()`, `transport()`, `document`, `decode_limited`). |
| `crates/concord-retry/README.md`, `src/http.rs`; `crates/concord-pagination/README.md`, `src/types.rs`, `src/pages.rs` | Why the shared parser does not fit (item 6); `Pages`, `Resume`, `Incomplete`, `PagesError::resume`. |
