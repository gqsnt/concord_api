# concord-sdk-discord-rest

For `DiscordChannel::history_from`, use `concord_sdk_discord_rest::pagination::Resume` for the
position. `Resume::Start` repeats the channel's newest-message page with `limit`;
`Resume::At(key)` takes the `MessageId` in a returned page's `next` (its oldest message),
listing messages before that ID.

A bounded Discord REST (API v10) slice built with Concord, for a bot token:

| Operation | Route | SDK call |
|---|---|---|
| List a guild's channels (threads excluded) | `GET /guilds/{guild.id}/channels` | `discord.guild(id).channels()` |
| List a channel's messages | `GET /channels/{channel.id}/messages` | `discord.channel(id).messages()` |
| Fetch one message | `GET /channels/{channel.id}/messages/{message.id}` | `discord.channel(id).message(message_id)` |
| Create a text message | `POST /channels/{channel.id}/messages` | `discord.channel(id).create_message(body)` |

plus `channel.history(limit)`, a bounded walk of a channel's history, and rate admission for
Discord's per-route buckets and global limit. Every call ends with `send()`, which
returns `Result<T, concord_sdk_discord_rest::Error>`.

Live provider calls: **not run**. Everything is tested against a local fake
(`examples/testserver`); see [Testing](#testing).

## Construct

```rust,no_run
use std::time::Duration;

use concord_sdk_discord_rest::concord::reqwest;
use concord_sdk_discord_rest::{Discord, DiscordConfig, RateLimiter};

fn client(token: String) -> Result<Discord, reqwest::Error> {
    // Transport settings are yours: the SDK builds no client and sets no timeout. Turn off
    // redirects and retries: with either, a status or error can describe a later request than
    // your write (a performed POST redirected to a target answering 429 would look refused).
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()?;
    // One RateLimiter per bot token: Discord's limits are per bot.
    Ok(Discord::new(http, DiscordConfig { token, limiter: RateLimiter::new() }))
}
```

- `token` is the bot token **without** `Bot `; the SDK sends `Authorization: Bot {token}`.
  Concord never prints it (`Debug` shows `token: <redacted>, limiter: <redacted>`), and a token
  that cannot be a header value fails before any I/O without being echoed.
- `User-Agent` defaults to `DiscordBot (https://github.com/gqsnt/concord_api, 0.0.0)`
  (`DEFAULT_USER_AGENT`); set your bot's own with `discord.set_user_agent(..)`.
- Clones of the client (and every scope or request made from it) share the limiter.
  `with_base_url` points the client at a test server (`http://127.0.0.1:port/api/v10`).

## Workflow 1: channels, then messages, then one message

Discord permits only one of `around`, `before`, and `after` in a request
([Get Channel Messages](https://docs.discord.com/developers/resources/message#get-channel-messages),
read 2026-11-07). Use `anchor(Anchor::Around(id))`, `Anchor::Before(id)`, or
`Anchor::After(id)`; a later `anchor` replaces the earlier one. Use
`maybe_anchor(None)` to clear it. `set_anchor` is also available.

```rust,no_run
use std::num::NonZeroUsize;

use concord_sdk_discord_rest::{Anchor, Discord, Error, GuildId, MessageLimit};

async fn browse(discord: &Discord, guild: GuildId) -> Result<(), Error> {
    let channels = discord.guild(guild).channels().send().await?;
    let Some(text) = channels.iter().find(|c| c.kind.is_guild_text()) else {
        return Ok(());
    };
    let channel = discord.channel(text.id);

    // One page, by hand: at most one of before / after / around is sent.
    let newest = channel
        .messages()
        .set_limit(MessageLimit::new(10).expect("1..=100"))
        .send()
        .await?;

    // One page around a known message; choosing another anchor replaces this one.
    if let Some(center) = newest.first() {
        let around = channel.messages().anchor(Anchor::Around(center.id)).send().await?;
        println!("{} messages around the selected ID", around.len());
    }

    // The whole history, newest first, at most 10 requests of 100 messages.
    let walked = channel
        .history(MessageLimit::MAX)
        .max_pages(NonZeroUsize::new(10).unwrap())
        .items()
        .collect_complete()
        .await;
    match walked {
        Ok(messages) => println!("{} messages", messages.len()),
        Err(incomplete) => {
            // The prefix read so far, and where to continue (`history_from(resume, limit)`),
            // e.g. after the delay of a rate limit (`incomplete.error.provider_error()`).
            println!("stopped after {}: {}", incomplete.collected.len(), incomplete.error);
        }
    }

    if let Some(first) = newest.first() {
        let one = channel.message(first.id).send().await?;
        assert_eq!(one.id, first.id);
    }
    Ok(())
}
```

`history` sends `GET .../messages?before={oldest id of the previous page}&limit={limit}` and
ends after a page shorter than `limit` (an empty page included). A page that is not ordered
newest to oldest below its anchor, or longer than `limit`, is `Error::Protocol` rather than a
silent loop. Without `READ_MESSAGE_HISTORY` Discord returns no messages: an empty history, not
an error.

## Workflow 2: create a message and read it back

```rust,no_run
use concord_sdk_discord_rest::{ChannelId, CreateMessage, Discord, Error, Nonce};

async fn announce(discord: &Discord, channel: ChannelId) -> Result<(), Error> {
    let channel = discord.channel(channel);
    // A nonce lets you recognize the message later, and with `enforce_nonce` (set by
    // `with_nonce`) a deliberate resend returns the existing message instead of a duplicate.
    let body = CreateMessage::text("deploy 42 finished")
        .expect("1 to 2000 characters")
        .with_nonce(Nonce::new("deploy-42").expect("1 to 25 characters"));
    match channel.create_message(body).send().await {
        Ok(created) => {
            let read = channel.message(created.id).send().await?;
            assert_eq!(read.id, created.id);
        }
        Err(Error::AmbiguousWrite(write)) => {
            // It may exist. Read before deciding: list recent messages and look for the nonce,
            // or resend the same body (same nonce) within Discord's few-minute window.
            eprintln!("{} - reconcile before resending", write.route);
        }
        Err(Error::Gated(gated)) => {
            // Never sent; safe to send after `gated.retry_after`.
            eprintln!("wait {:?}", gated.retry_after);
        }
        Err(other) => return Err(other),
    }
    Ok(())
}
```

## Failure semantics

Every error names the operation and the route's channel or guild (`error.resource()`).
`Display`/`Debug` show those, the status, Discord's numeric code and delays — never the token,
a URL, a provider message or a body (read those deliberately through the fields).

| `Error` variant | Sent? | Meaning, and what the caller may do |
|---|---|---|
| `Gated` | **no** | The limiter refused locally: a global 429 is still running, or this route bucket for this channel/guild has 0 remaining until its reset. `retry_after` says how long. Sending later is safe. |
| `RateLimited` | yes, refused | HTTP 429. `scope` (`User`, `Global`, `Shared`, `Unspecified`), `retry_after` (the larger of `Retry-After` and the body's fractional `retry_after`, else `X-RateLimit-Reset-After`, else `None`), `bucket`, the body. The delay is now a gate. |
| `Api` | yes | Another non-2xx. `class()` (400, 401, 403, 404, 405, 5xx, other), `code()` (Discord's JSON code: 10003 unknown channel, 10008 unknown message, 50001 missing access, 50013 missing permissions, 50035 invalid form body, ...), `document` (or why it could not be read: HTML, too large, cut off). The status is authoritative either way. After a 401, stop using the token. |
| `AmbiguousWrite` | **maybe** | A `POST` failed after it may have reached Discord: no response (timeout, dropped or refused connection: `accepted == None`), or a 2xx whose body could not be read (`accepted == Some(..)`: created, ID unknown). **Never resent by the SDK.** Reconcile by reading; resend only deliberately, with the same nonce. |
| `Protocol` | yes | A message page broke the ordering or size rule `history` relies on. |
| `Concord` | depends | Concord's error: `Build` (nothing sent), `Transport` on a read (on a write any transport failure, a refused connection included, is `AmbiguousWrite`), and for a 2xx read the success `Content-Type` (`Metadata`), body limit (`Limit`) or decoding (`Decode`); `error.head()` keeps the response head. |

The SDK never sleeps, queues or retries. `concord::Error` and the pagination crate are
re-exported (`concord_sdk_discord_rest::concord`, `::pagination`).

### Rate limits

The client's send policy is the `RateLimiter` (`send limiter -> Result<_, Error>;` in the declaration). Per request:
admit (no I/O), execute once, record `X-RateLimit-Bucket` / `-Remaining` / `-Reset-After` for
`(bucket hash, channel or guild)` — the hash excludes the top-level resource, so two channels
that share it keep separate windows — and on a 429 gate the global scope or that window.
Routes that share a bucket hash (learned from responses) share a window per resource.
Responses overlap, so while a window is open a later observation keeps the lower remaining count
and the later reset: a success sent before an exhaustion cannot reopen it early.

Not done: proactive pacing to the documented 50 requests per second; serializing concurrent
requests on one window (two admitted requests can both meet a 429); Cloudflare's invalid-request
limit (10,000 401/403/429 per 10 minutes) beyond not retrying; `X-RateLimit-Reset` (absolute
epoch) is ignored in favour of `Reset-After`. Bodies are bounded: 4 MiB for a success
(`SUCCESS_BODY_LIMIT`, an SDK bound), 64 KiB for an error document (`ERROR_BODY_LIMIT`).

### Permissions

Listing needs `VIEW_CHANNEL` (without `READ_MESSAGE_HISTORY`, no messages are returned);
fetching one message needs both; creating needs `SEND_MESSAGES` (`SEND_TTS_MESSAGES` with
`tts`). Missing permissions are `Error::Api` with class `Forbidden` and code 50001 or 50013.
Discord documents that from 2026-11-16 the channel listing omits channels the bot cannot view.

## Excluded

Gateway (events, intents), OAuth installation, reactions, attachments and files, embeds,
components, stickers, polls, replies, webhooks, threads, editing and deleting messages, other
Discord resources, and any live message creation. `Message` decodes only the fields listed in its
docs and ignores the rest.

## Testing

```sh
cargo test --locked -p concord-sdk-discord-rest
cargo clippy --locked -p concord-sdk-discord-rest --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-discord-rest --no-deps
```

`tests/protocol.rs` checks the wire (targets, headers, bodies), every documented error class,
the documented 429 examples and delay fallbacks, malformed / truncated / oversized /
mislabeled bodies and redaction. `tests/workflows.rs` runs both workflows against a stateful fake
and the required failures: two channels sharing a bucket hash, a global 429, and a timeout after
a create (never resent; reconciled by reading the nonce). Fixture sources are in
[PROVENANCE.md](PROVENANCE.md); Concord costs met while writing it are in
[FRICTION.md](FRICTION.md).

