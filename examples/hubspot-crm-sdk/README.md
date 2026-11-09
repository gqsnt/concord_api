# HubSpot CRM contacts SDK (bounded, example-assisted)

This package covers OAuth refresh, contact search with `paging.next.after`, contact creation,
lookup by ID, and batch read by ID. It uses the legacy `/crm/v3/objects/contacts` routes from the
cited contacts article and the `/oauth/2026-09/token` refresh route. It is an offline-tested
example, not a full HubSpot SDK. Marketing automation, other CRM object types, auth-code
acquisition, and exactly-once contact creation are out of scope. Live provider calls: **NOT_RUN**.

```rust,no_run
use concord::reqwest;
use concord_pagination::Resume;
use concord_sdk_hubspot_crm::{HubSpot, SearchRequest, CreateContact};
use std::{num::NonZeroUsize, time::Duration};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .retry(reqwest::retry::never())
    .timeout(Duration::from_secs(10))
    .build()?;
let sdk = HubSpot::new(http, "client-id".into(), "client-secret".into(), "refresh-token".into())?;
let request = SearchRequest { query: Some("Anna".into()), limit: Some(50), ..Default::default() };
let contacts = sdk.search(request, Resume::Start)
    .max_pages(NonZeroUsize::new(20).unwrap()).items().collect_complete().await?;
if let Some(selected) = contacts.first() { let _contact = sdk.get(&selected.id).await?; }
let mut input = CreateContact::default();
input.properties.insert("email".into(), "anna.smith@example.com".into());
let created = sdk.create(input).await?;
let _again = sdk.get(&created.id).await?;
# Ok(()) }
```

A caller must supply a reqwest client with retries and redirects disabled and a finite timeout, as
shown. Arbitrary client settings can replay requests or forward secrets. Set `with_base_url` only
to a trusted endpoint; it overrides both OAuth and CRM destinations and exists for local fixtures.
Do not log native requests, response bodies, contact data, or credentials.

The first request refreshes the token; clones share an installation-specific single-flight cache.
The access token is reused with a 30-second refresh margin. Refresh responses with 30 seconds or
less lifetime are refused. A returned replacement refresh token supersedes the old one in the
shared process state before access-token publication. `refresh_token()` returns sensitive material
for application-managed secure persistence. It is an in-memory snapshot, not a durable transaction:
the application must coordinate all readers/writers, persist rotation safely, and coordinate
across processes; separate `HubSpot` instances do not synchronize. An interruption, server error,
malformed token response, or uncertain refresh outcome blocks further refresh until the caller
recovers credentials and constructs a new instance. A 400, 401 or 403 refresh refusal is reported
as definite; inspect its status and reauthorize as appropriate. No automatic 401 replay occurs.
Cancellation while a shared refresh is in flight leaves the cache's future available to resume;
process loss may still lose a rotated token. These constraints mean this example alone does not
provide crash-safe production token persistence.

`create` performs one application request. A request-build failure (`Delivery::NotSent`) is a
Concord error. Any transport failure (a refused connection or timeout included), 408, server
error, or unusable successful response is `CreateUncertain`: it may have created a contact. Reconcile
with HubSpot before deciding whether to create again. A caller that cancels the future after dispatch
must likewise treat the result as uncertain. Explicit non-408 client errors are returned as Concord
status errors. Search, get and batch read are also sent once; no retry or rate limiter is installed.
429 and other status errors are surfaced with the response head available through `concord::Error`, not
parsed as provider error documents. Successful JSON bodies are limited to 1 MiB and must have JSON
media type. Provider response bodies and values are never included in SDK error formatting.

Search traversals have no page cap unless `max_pages` is set. A cap or page error is not completion;
`collect_complete` returns an incomplete prefix and resume information. Reuse a cursor only with
the same query, installation and page parameters. Empty or immediately repeated cursors are refused;
choose a finite cap to contain longer provider cursor cycles. Search `total` is page metadata, not
a guarantee of a stable snapshot as data changes.

Add `concord-pagination` as a direct dependency and use `concord_pagination::Resume`.
`Resume::Start` repeats the search body without `after`, replacing `request.after`;
`Resume::At(key)` takes the `String` from `paging.next.after` in a returned page's `next`.

`batch_read` reads 1..=`BATCH_READ_MAX` (100, HubSpot's documented batch size) distinct contact
record IDs in one request; invalid input is refused before any request. A `200` or `207
Multi-Status` answer gives every requested ID, in request order, its contact or an `ItemError`:
`Provider { category }` when an item error's `context.ids` names the ID (for example
`OBJECT_NOT_FOUND`), otherwise `Unreported`. Item error messages are not retained. Returned
contacts whose ID was not requested (HubSpot may return a merged contact under its current ID; this
is not documented) are kept in `unmatched`; categories of errors naming no requested ID are kept in
`unattributed`. A response listing a requested ID twice, or as both found and failed, is
`Error::Invalid`. The `context.ids` key comes from observed HubSpot responses, not from the reference
schema, which types `context` only as a map of string arrays: an ID reported another way is
`Unreported`, never treated as found. A batch read is safe to repeat, but the SDK sends it once: a
status error (429 included), timeout or decode failure fails the whole call, and repeating it is the
caller's decision. Lookup by `idProperty` (email or another unique property), `archived` and
property history are not offered. The 1 MiB body limit applies to the whole batch; a `Limit` error
means fewer IDs or properties per request.
