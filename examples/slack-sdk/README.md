# Slack Web API SDK slice

The declaration owns the provider error document: `error crate::wire::Envelope<serde_json::Map<String, serde_json::Value>> in concord::IgnoreContentType<concord::Json>;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

`concord-sdk-slack` covers GET `users.list` (one page, lazy pages, items, and a standard Rust
item stream) and GET `conversations.info` (channel and direct-message shapes). It handles bearer
authentication, Slack's `ok`/`error` envelope, response metadata, and HTTP 429 with `Retry-After`.
It excludes sending messages, automatic retries, token acquisition/refresh, and workspace
administration. All fixture tests use the loopback server; they are not vendor conformance tests.

Construct a client with a raw token and a transport configured by your application. A workspace
token selects its workspace; organization tokens can set `UsersOptions::team_id`. `users:read`
is required for listing; conversation scopes depend on the conversation type (`channels:read`,
`groups:read`, `im:read`, or `mpim:read`). Scope grants and inaccessible IDs are provider errors.

```rust
use concord::reqwest;
use concord_sdk_slack::{Client, ClientConfig};

let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(15))
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let slack = Client::new(http, ClientConfig {
    token: "synthetic-slack-token".into(), // Replace with application-supplied auth.
});
// Offline tests use slack.with_base_url("http://127.0.0.1:PORT/api")?.
# Ok::<(), Box<dyn std::error::Error>>(())
```

Workflow A pages through users, preserving warnings and request IDs per page. A configured cap
is an error when there is more work, including when the last returned page was empty.

```rust
use std::num::NonZeroUsize;
use concord_sdk_slack::{Client, PageSize, UsersOptions};

async fn list_users(slack: &Client) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let options = UsersOptions {
        limit: PageSize::new(100).unwrap(),
        include_locale: Some(true),
        ..UsersOptions::default()
    };
    let mut pages = slack.users(options).max_pages(NonZeroUsize::new(50).unwrap());
    let mut names = Vec::new();
    while let Some(page) = pages.next_page().await? {
        let _request_id = page.meta.http.request_id();
        names.extend(page.items.into_iter().filter_map(|user| user.name));
    }
    Ok(names) // Only provider exhaustion produces Ok(None).
}
```

For an all-or-partial item collection, use
`slack.users(UsersOptions::default()).items().collect_complete().await`.
The error's `collected` vector keeps the prefix, and `error` distinguishes a provider failure
from a bound. For streaming, call `.items().into_stream()` and use
`futures_util::StreamExt::next`; each item is `Result<User, pagination::PagesError<Error, Cursor>>`.
The stream yields a terminal error before ending. A later stream `None` alone is not proof of
successful completion. Dropping a traversal cancels its pending future; no task is detached.

For manual paging, `users_page(&options, None).await?` returns `Response<UsersPage>`; pass
`response.response_metadata.as_ref().and_then(|m| m.next_cursor.as_ref())` into the next call.
Only an empty, null, or missing cursor terminates the chain; fewer users than `limit`, or zero
users, does not. Every nonempty cursor, including whitespace, is preserved byte for byte and
query-encoded once. Never decode or pre-encode it. Keep the same workspace and options, use it
promptly, and restart from the beginning after long pauses. Slack says not to persist cursors for
hours or days. This SDK supplies no persistence or automatic expiry policy.

The SDK owns query encoding for both manual and lazy paging. For example, raw cursor `==`
travels as `cursor=%3D%3D`, and literal cursor text `%3D` travels as `cursor=%253D`; `+` travels
as `%2B`, separately from a space (`+` on the wire). Those nonempty values are continuations,
even on an empty page. This follows Slack's [pagination contract](https://docs.slack.dev/apis/web-api/pagination/);
pass the returned `Cursor` directly to `users_page`. The reserved-character regression tests
also cover empty-page chains, each supported terminal shape, cap checkpoints, and typed
`invalid_cursor` failures. See [MAINTENANCE.md](MAINTENANCE.md) for the maintenance evidence.

Workflow B retrieves a conversation and surfaces a failure even when HTTP status is 200.

```rust
use concord_sdk_slack::{Client, ConversationOptions, Error, ErrorCode};

async fn conversation(slack: &Client) -> Result<Option<String>, Error> {
    match slack.conversation_info("C_SYNTHETIC_GENERAL", ConversationOptions {
        include_num_members: Some(true),
        ..ConversationOptions::default()
    }).await {
        Ok(response) => Ok(response.value.name), // DMs may have no name.
        Err(Error::Api { error, meta }) if error.code == ErrorCode::ChannelNotFound => {
            assert!(meta.status.is_success());
            let _request_id = meta.request_id();
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
```

`Error::Api` contains typed known codes and `ErrorCode::Other` for future codes, optional
required/granted scopes, warnings, and body metadata. `Error::RateLimited` contains
`RetryAfter::Seconds(Duration)`, `Missing`, or `Invalid`, the original headers, and a bounded
error body. The documented header is an integer number of seconds; repeated, negative,
overflowing, or HTTP-date values are invalid. A malformed 429 body still yields `RateLimited`.
The caller decides when to try the same method/workspace again. The SDK never waits or retries.

`Error::Http` handles other non-2xx responses. `Error::Concord` retains the original build,
transport, content-type, or decoding error; `meta()` is present after response headers arrive,
including on a truncated body. `Error::Envelope` rejects missing success payloads, missing error
codes, or contradictory success/error fields. A missing or non-boolean `ok` is a decoding error.
Status/metadata failures retain Concord's unread response where available. Non-2xx error bodies
are consumed once, up to 64 KiB; a read failure remains inspectable without replacing the status.

Success bodies are finite JSON, fully buffered per page, up to `SUCCESS_BODY_LIMIT` (16 MiB, an
SDK safety bound rather than a Slack limit): a larger 2xx body is `Error::Concord` with
`ErrorKind::Limit` and the response metadata, and nothing beyond the limit is read. Item streaming
is between buffered pages, not incremental JSON parsing. Unknown user and conversation fields are
kept in `extra`; only a small set of fields is projected into named properties. Pages keep HTTP
and body metadata; flattened item traversal discards page metadata. There is no snapshot or
deduplication guarantee if the workspace changes while listing. There is no implicit page cap or
repeated-cursor detector; applications can set `max_pages`. Checkpoints are short-lived;
`Cursor(String)` carries no listing context, so the caller must reuse it only with the same
listing and options and must not store it long term.

Tokens and cursors are redacted in the SDK's client/cursor debug output, and HTTP metadata debug
prints header names only. Explicit header/body access and domain objects contain server data.
Transport settings, including redirects and any transport-level retry policy, belong to the
provided `reqwest::Client`.

The implementation uses `concord::api!`, the public `Endpoint`/`Received` response boundary,
`concord::Json` through the facade, and the ordinary `concord-pagination` facade. It never executes
through a vendor SDK. The existing PokéAPI metadata adapter and GitHub declarations were read as
references; the Slack envelope and pagination adapter were written for this slice.

The source contracts are pinned by access date in [PROVENANCE.md](PROVENANCE.md), and authoring
observations are in [FRICTION.md](FRICTION.md).
