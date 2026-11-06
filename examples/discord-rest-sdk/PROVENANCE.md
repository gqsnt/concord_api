# Sources and fixture provenance

Official documentation was read on **2026-11-07** with a read-only web fetch tool. That tool
returns a model-processed rendering of each page (summaries and answers to field-level
questions), not raw HTML; every value used below was asked for field by field, but the
rendering is not a byte-exact copy of the page. Discord publishes no page revision; the pin is
the content observed on that date for API **v10** (listed "Available" on the reference page).

| Key | Official page | Sections used | Read |
|---|---|---|---|
| RL | <https://docs.discord.com/developers/topics/rate-limits> | Per-route and global limits, top-level resources, header format (`X-RateLimit-Limit/-Remaining/-Reset/-Reset-After/-Bucket/-Global/-Scope`), example headers, exceeding a limit (three 429 examples, body fields), global limit (50 req/s), invalid request limit | 2026-11-07 |
| G | <https://docs.discord.com/developers/resources/guild> | Get Guild Channels (route, threads excluded, 2026-11-16 visibility warning) | 2026-11-07 |
| M | <https://docs.discord.com/developers/resources/message> | Message object fields, example message, Get Channel Messages (query parameters, permissions, ordering), Get Channel Message, Create Message (permissions, content limit, `nonce`, `enforce_nonce`) | 2026-11-07 |
| R | <https://docs.discord.com/developers/reference> | Base URL and API versions, authentication (`Authorization: Bot TOKEN`), snowflakes (strings), User-Agent format, error messages (50035 examples), nullable/optional notation | 2026-11-07 |
| S | <https://docs.discord.com/developers/topics/opcodes-and-status-codes> | HTTP response codes table, JSON error codes (0, 10003, 10004, 10008, 20016, 20028, 40001, 50001, 50006, 50008, 50013, 50014, 50035), example JSON error | 2026-11-07 |
| C | <https://docs.discord.com/developers/resources/channel> | Channel object fields, channel types table, example guild text channel, example guild category | 2026-11-07 |

S and C are **not** in the capsule's list; they are official Discord pages read for the error
codes and the channel object that G and M reference. No other source (no library, no forum, no
live API) was used. The real Discord API was never called.

## Facts and where they come from

| Fact used by the SDK | Source | Status |
|---|---|---|
| Base `https://discord.com/api/v10` | R | documented |
| `Authorization: Bot TOKEN` | R | documented |
| `User-Agent: DiscordBot ($url, $versionNumber)` required | R | documented |
| Snowflakes are JSON strings | R | documented; the SDK refuses a JSON number |
| `GET /guilds/{guild.id}/channels`, no threads | G | documented |
| `GET /channels/{channel.id}/messages`: `around`/`before`/`after` (one at most), `limit` 1–100 default 50, newest to oldest, `VIEW_CHANNEL`, no messages without `READ_MESSAGE_HISTORY` | M | documented |
| `GET /channels/{channel.id}/messages/{message.id}` needs `VIEW_CHANNEL` and `READ_MESSAGE_HISTORY` | M | documented |
| `POST /channels/{channel.id}/messages`: `SEND_MESSAGES`, content ≤ 2000 characters, `nonce` ≤ 25 characters, `enforce_nonce` returns the existing message "in the past few minutes" | M | documented |
| Per-route buckets, `X-RateLimit-Bucket` excludes the top-level resource (channel, guild, webhook) | RL | documented |
| `X-RateLimit-Reset-After` and body `retry_after` may be fractional | RL | documented |
| `X-RateLimit-Scope`: `user`, `global`, `shared`; `X-RateLimit-Global` only on a global 429 | RL | documented |
| HTTP status classes 400/401/403/404/405/429/502/5xx | S | documented |
| `Retry-After` header may be fractional | capsule contract note | **unverified**: the RL examples show whole seconds (65, 1337); the SDK accepts decimal seconds |
| Gate on the larger of `Retry-After` and body `retry_after` | RL examples (65 vs 64.57) | SDK choice (never shortens a minimum) |
| A page shorter than `limit` is the end of the history | — | **SDK inference**, not stated on M |
| `after` listings are also newest to oldest | M ("newest to oldest on success") | inference from the general statement; only the fake relies on it |
| Content length counted in Unicode scalar values | — | **unverified**: M says "2000 characters" without a unit |
| Create Message answers `200` | — | **unverified**: M says it returns a message object; the SDK accepts any 2xx |
| A `POST` whose connection was refused was never sent; any other pre-head failure may have been | reqwest's error taxonomy (`is_connect`) | SDK classification, not a Discord fact |

## Fixtures

All fixtures are compact UTF-8 JSON (one HTML page) with a final LF; `.gitattributes` keeps
them byte-exact. None was captured from Discord. "Documented" means the values are those the
page shows; "synthetic" means authored for a test.

| Fixture (`fixtures/`) | Source | Derivation |
|---|---|---|
| `guild-channels.json` | C / example guild category, example guild text channel; G | The category example verbatim. The text-channel example with its `guild_id` changed from `41771983423143937` to the category's `290926798629997250` (one guild; its `parent_id` already is the category). A synthetic voice channel (`155101607195836416`, type 2) between them. |
| `message.json` | M / example message | Documented values, keys in the page's order, compact. |
| `message-numeric-id.json` | M / example message (negative mutation) | `id` as a JSON number, reactions removed, `avatar` null: must fail decoding (R: snowflakes are strings). |
| `message-truncated.json` | M (negative mutation) | Synthetic message prefix cut inside a string. |
| `rate-limit-user.json` | RL / user 429 example | Body verbatim; headers scripted in `tests/support/mod.rs::documented_429` with the page's values. |
| `rate-limit-shared.json` | RL / shared 429 example | Body verbatim; headers scripted likewise (`Remaining: 9` is the page's value). |
| `rate-limit-global.json` | RL / global 429 example | Body verbatim; headers scripted likewise. |
| `error-invalid-form-body.json` | R / object-error example (50035) | Documented values, compact. Its field (`access_token`) is the page's example, not a Create Message field. |
| `error-invalid-token.json` | S / example JSON error | Documented values (`50014`, "Invalid authentication token"), compact. |
| `error-missing-access.json` | S / JSON error codes (50001) | Synthetic document: the table's code and description as `message`. |
| `error-missing-permissions.json` | S / JSON error codes (50013) | Synthetic, likewise. |
| `error-unknown-channel.json` | S / JSON error codes (10003) | Synthetic, likewise. |
| `error-unknown-message.json` | S / JSON error codes (10008) | Synthetic, likewise; also reused as a 500 body (a synthetic pairing). |
| `error-empty-message.json` | S / JSON error codes (50006) | Synthetic, likewise. |
| `error-method-not-allowed.json` | S / HTTP 405 and code 0 | Synthetic message text (`405: Method Not Allowed`); not documented. |
| `error-marker.json` | S (shape only) | Synthetic message `SYNTHETIC-PROVIDER-TEXT ...` used to prove provider text is never printed. |
| `error-foreign-shape.json` | — | Synthetic non-Discord JSON (`{"error": ..}`): an error document of the wrong shape. |
| `bad-gateway.html` | S / 502 (status only) | Synthetic proxy HTML page, used as a 502 body, a mislabeled 200 and a Cloudflare-style 429 body. |

Generated in the tests, not stored: synthetic message objects (`tests/support/mod.rs::
message_json`, shape of M's example, invented IDs `155117677105512445..=155117677105512449` whose
newest equals C's `last_message_id`, invented author `synthetic-bot`), route headers
(`with_bucket`, the RL example header values with scripted `remaining`/`reset-after`), the
oversized success body (64 KiB chunks of spaces past `SUCCESS_BODY_LIMIT`), the oversized error
document (past `ERROR_BODY_LIMIT`), cut-off bodies, a hang-up, a refused connection, a held
response for the timeout, and the delay fallbacks (`retry-after: 2.5`, `soon`, `{}`, a negative
`retry_after`). These are robustness cases, not claims about Discord behaviour. The bot token
`synthetic-bot-token.Concord.000` is invented; the documented example token on R was not used.

No expectation was copied from the implementation: expected targets, bodies, delays and codes
in the tests are written from the pages above (and the arithmetic of the scripted clock).

## Dependencies

No new crate. The package uses `concord` (feature `json`), `concord-pagination`, `serde`
(`derive`), `serde_json`, and for tests `concord-example-testserver` and `tokio` (`macros`, `rt`,
`time`), all already in the workspace's `Cargo.lock` at the versions there. The only lockfile
change is this package's own entry.
