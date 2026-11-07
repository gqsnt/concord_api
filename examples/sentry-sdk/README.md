# Sentry issue and event reads and issue status updates

`concord-sdk-sentry` is an **example-assisted**, bounded SDK over Concord's generated
client → organization/issue scopes → endpoints. It lists organization issues by cursor,
retrieves a chosen issue, pages that issue's events, and retrieves an event by `eventID`.
Event `id` and `eventID` are distinct fields. It also changes an issue's status (resolve,
ignore, unresolve; see "Status updates"). Live provider calls: **NOT_RUN**.

```no_run
use concord_sdk_sentry::{http_client, Resume, SendPolicy, Sentry};
use std::num::NonZeroUsize;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let api = Sentry::with_token(http_client()?, "your-token", SendPolicy::default())?;
// Tests always add .with_base_url(server.url()) pointing at a local fixture.
let organization = api.organization("acme");
let cap = NonZeroUsize::new(20).unwrap();
let mut issues = organization.issues().set_query("").pages(Resume::Start, cap);
while let Some(page) = issues.next_page().await? {
    for summary in page.items {
        let issue = organization.issue(&summary.id);
        let details = issue.get().send().await?;
        let mut events = issue.events().pages(Resume::Start, cap);
        while let Some(page) = events.next_page().await? {
            for summary in page.items {
                let event = issue.event(&summary.event_id).send().await?;
                assert_eq!(event.value.group_id, details.value.id);
            }
        }
    }
}
# Ok(()) }
```

Single pages use `.issues().maybe_cursor(cursor).send()` or
`.events().maybe_cursor(cursor).send()`. Responses include `value`, `meta`, `rates`,
`next_cursor` and `retry`. Organization issue page size uses `limit`; events use `per_page`.
Both default to 100. This slice leaves additional filters and page-size validation to Sentry;
it exposes optional search `query`, including the empty query to list all issue statuses.
The normal unresolved issue default applies when query is omitted.

Pagination parses all Link fields using `concord-pagination/link`. Only a unique next link
with `results=true` can continue. `results=false` stops even if a cursor/target is present;
absent next also stops. Malformed syntax, missing/invalid flags, ambiguous links, changed
origin/collection/filter, conflicting cursors and immediate cursor loops are errors. The
adapter projects a cursor into another generated operation, preserving its filters; it never
dispatches the server's URL. A missing non-cursor query parameter in a link retains the template's
value. `pages(from, cap)` requires a page bound. Hitting it with more data yields
`PagesError::Bounded` and a resume position, never silent exhaustion. Use the returned cursor
only with the same organization, issue, query and page size. Multi-page cycles end at the cap.
An empty page with results=true continues. Page bounds count logical calls; retries count separately.

## Status updates

`issue.update_status(change).send()` sends `PUT .../organizations/{org}/issues/{issue}/` with a
JSON body holding `status` and only that change's documented details, and returns the updated
`Issue` (`next_cursor` is always `None`). No other issue attribute and no `null` is sent.

| `StatusUpdate` | Body |
| --- | --- |
| `Unresolved` | `{"status":"unresolved"}` |
| `Resolved` | `{"status":"resolved"}` |
| `ResolvedInNextRelease` | `{"status":"resolvedInNextRelease"}` |
| `Ignored(Ignore::FOREVER)` | `{"status":"ignored"}` |
| `Ignored(Ignore::for_minutes(m)?)` | adds `"ignoreDuration": m` |
| `Ignored(Ignore::until_seen(n, w)?)` | adds `"ignoreCount": n`, and `"ignoreWindow"` when `w` is `Some` |
| `Ignored(Ignore::until_users(n, w)?)` | adds `"ignoreUserCount": n`, and `"ignoreUserWindow"` when `w` is `Some` |

A status outside this closed set cannot be written. Durations and windows are whole minutes; a
zero count, duration or window, or a window over 7 days (`MAX_IGNORE_WINDOW_MINUTES`), is
`Error::InvalidUpdate` from the `Ignore` constructor, before any request exists.

```no_run
use concord_sdk_sentry::{http_client, Error, Ignore, SendPolicy, Sentry, StatusUpdate};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let api = Sentry::with_token(http_client()?, "your-token", SendPolicy::default())?;
let issue = api.organization("acme").issue("4567891230");
let change = StatusUpdate::Ignored(Ignore::until_seen(100, Some(60))?);
match issue.update_status(change).send().await {
    Ok(updated) => assert_eq!(updated.value.status, "ignored"),
    // It may have been applied: learn the outcome by reading, never by resending blindly.
    Err(Error::Uncertain { .. }) => println!("now {}", issue.get().send().await?.value.status),
    Err(other) => return Err(other.into()),
}
# Ok(()) }
```

The update has its own endpoint bucket under the same `Settings` and header observations as the
reads: every attempt takes a concurrency lease, then frequency admission. Only a 429 (Sentry's
rate-limit rejection) is retried, after its Retry-After and both admissions again. An update
that received no response head (the transport's timeout, a reset or refused connection, or the
scheduling deadline expiring in flight) is `Error::Uncertain` and is never resent: re-read the
issue. A 5xx answer is `Error::Provider` and is not retried; it does not prove that the change
was not applied either. A deadline during 429 backoff is `Error::RetryStopped` with the 429 head
(not applied). A 2xx with invalid quota fields is `Error::RateMetadata` with that 2xx head: the
change was applied.

All calls use a sensitive bearer header. `with_token` validates token syntax; direct generated
`Sentry::new`/setters are lower-level interfaces accepting a complete native header.
For exact request accounting use `http_client()`: redirects and reqwest retries are disabled.
A caller-supplied transport must do likewise; it owns TLS, proxies and timeouts. `prepare()`
builds the declared request but bypasses the send policy if you execute it yourself.

`SendPolicy` clones share five buckets (four reads and the status update) for one **caller
identity and service deployment**.
Reuse the policy across tokens representing that caller: Sentry does not key limits by token.
Separate callers/deployments should use separate policies. Endpoint keys deliberately combine
all organizations and issue ids for that caller. Default local limits are 5 requests/second
and one simultaneous request **per endpoint**, with at most three attempts and a 10s scheduling
deadline. These are conservative SDK choices, not published Sentry quota values.
`Settings` can change these local bounds. Headers update frequency remaining/reset and cap
local concurrency to the reported concurrent limit. ConcurrentRemaining is a remote snapshot,
not a frequency count or an owned permit: zero adds a recheck cooldown of at least 50ms.
Other processes cannot be coordinated by this process-local policy.
The shared limiter's FIFO queues span endpoint keys, so a queued request can conservatively
delay another endpoint even when that endpoint's own capacity is available.

Every attempt, including retries, waits for a concurrency lease **then** frequency admission.
The lease covers headers and the final body's consumption; it is released before retry backoff.
Sentry frequency reset is an absolute UTC epoch timestamp, converted to the limiter's monotonic
epoch. Window observations and cooldowns combine conservatively. Retry-After (seconds or
IMF-fixdate) is a minimum. Safe reads retry headless transport failures and HTTP 429/502/503/504;
status updates retry only 429 (see above).
The backoff starts at 100ms and caps locally at 2s, without jitter. Malformed/repeated scheduling
headers stop repetition; no response body failure is replayed. Retry deadlines include admission,
dispatch and backoff. Final body reads use the transport's request timeout and a byte bound;
they are outside the retry deadline.

Every non-2xx returns `Error::Provider` with status, `HttpClass`, bounded typed JSON `Problem`
or a document diagnostic, and retry accounting. Classes are 400, 401, 403, 404, 429, 5xx and
other non-2xx. Authentication/access/not-found errors never retry. A deadline returns
`Error::RetryStopped` with its report and last head, if any. Invalid successful quota/Link
metadata has a dedicated error; Concord build/transport/JSON/media/size failures are wrapped
in `Error::Concord`. Success bodies are capped at 1 MiB, error documents at 16 KiB.
Strict JSON media checks apply to both; empty/malformed/truncated/wrong-shape JSON is an error.
HTTP failure status survives unreadable error documents. Extra model fields are ignored.

SDK Debug and error Display withhold tokens, cursors, search filters, response header values
and provider content. Provider models/Problem also withhold their fields in Debug. Native
request/URL access, public fields and explicit Concord body/source accessors disclose their
contents intentionally; do not log them as diagnostics.

Excluded: ingestion, releases mutation, organization administration, OAuth/token refresh,
issue attributes other than status (assignment, bookmarks, priority, merge, discard), the
`muted` status alias, release/commit resolution details, `substatus`, bulk issue mutation,
arbitrary event search/filter coverage, cross-process quota coordination and a complete Sentry
schema. All protocol/workflow tests use `examples/testserver`, without provider API calls.
Sources, synthetic fixture derivations and the consulted example are in `PROVENANCE.md`;
Concord adaptation costs are in `FRICTION.md`; the status-update maintenance record is in
`MAINTENANCE.md`.
