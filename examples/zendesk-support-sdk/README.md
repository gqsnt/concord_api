# concord-sdk-zendesk-support

A bounded Zendesk Support (Ticketing API v2) SDK built on Concord. It covers two connected
workflows:

1. **List tickets with cursor pagination, then retrieve a selected ticket**
   (`GET /api/v2/tickets?page[size]=..&page[after]=..`, `GET /api/v2/tickets/{id}`).
2. **Create a ticket, then retrieve its resulting state**
   (`POST /api/v2/tickets` with an `Idempotency-Key`, then `GET /api/v2/tickets/{id}`).

It also supports collision-protected status updates with a comment, based on a previously read ticket.

Every test runs against local fixtures (`examples/testserver`). **No call to the real Zendesk
service has been made** (live provider calls: NOT_RUN).

## Calling it

The declared client is `Zendesk`; endpoints are reached by navigation and sent with the
generated `send()`, returning `Result<T, Error>`:

| Call | Request | Output |
|---|---|---|
| `api.tickets().list()` | `GET tickets?page[size]=100` (+ `set_page_size`, `set_after`, `set_sort`, `set_external_id`) | `TicketPage` |
| `api.tickets().create(new_ticket, key)` | `POST tickets`, `{"ticket": {..}}`, `Idempotency-Key: key` | `CreatedTicket` (the ticket and `x-idempotency-lookup`) |
| `api.ticket(id).show()` | `GET tickets/{id}` | `TicketResponse` (`.ticket`) |
| `api.ticket(ticket.id).update(TicketUpdate::new(&ticket, status, comment))` | `PUT tickets/{id}` with `safe_update` and the read ticket's `updated_at` | `TicketResponse` (`.ticket`) |
| `ticket_pages(list, from, max_pages)` | every page of a list request | `concord_pagination::Pages<Ticket, String, (), Error>` |

```rust,no_run
use std::num::NonZeroUsize;

use concord_sdk_zendesk_support::concord_pagination::Resume;
use concord_sdk_zendesk_support::{
    Error, IdempotencyKey, NewTicket, PageSize, Priority, Status, Zendesk, http_client,
    TicketUpdate, ticket_pages,
};

async fn example(oauth_token: &str) -> Result<(), Box<dyn std::error::Error>> {
    // https://acme.zendesk.com/api/v2, `Authorization: Bearer {token}` (never printed).
    let api = Zendesk::connect(http_client()?, "acme", oauth_token)?;

    // Workflow 1: every ticket, 100 per page, then the current state of one of them.
    let list = api.tickets().list().set_page_size(PageSize::MAX);
    let tickets = ticket_pages(list, Resume::Start, NonZeroUsize::new(50).unwrap())
        .items()
        .collect_complete()
        .await
        .map_err(|incomplete| incomplete.error)?;
    if let Some(on_hold) = tickets.iter().find(|t| t.status == Status::Hold) {
        let current = api.ticket(on_hold.id).show().send().await?.ticket;
        println!("ticket {} is {:?}", current.id, current.status);
    }

    // Workflow 2: create with a key the application stored first, then read the result.
    let key = IdempotencyKey::new("order-1234-ticket")?;
    let new = NewTicket::new("The export button does nothing.")
        .subject("Export broken")
        .priority(Priority::High)
        .external_id("order-1234");
    let created = match api.tickets().create(new.clone(), key.clone()).send().await {
        Ok(created) => created,
        // The create may have happened. Never send it with a new key: replay it unchanged
        // (within two hours Zendesk returns the original ticket, `IdempotencyLookup::Hit`),
        // or look it up with `api.tickets().list().set_external_id("order-1234")`.
        Err(Error::Uncertain(_)) => api.tickets().create(new, key).send().await?,
        Err(other) => return Err(other.into()),
    };
    let state = api.ticket(created.ticket.id).show().send().await?.ticket;
    println!("created ticket {} ({:?})", state.id, state.status);

    // Use the id and stamp from the same read. On collision, re-read and let the caller
    // decide whether the change and comment still make sense.
    match api.ticket(state.id)
        .update(TicketUpdate::new(&state, Status::Pending, "Investigating the export."))
        .send().await
    {
        Ok(updated) => println!("updated ticket {}", updated.ticket.id),
        Err(Error::Collision(_)) => {
            let fresh = api.ticket(state.id).show().send().await?.ticket;
            println!("ticket {} changed; review {:?}", fresh.id, fresh.status);
        }
        Err(Error::Uncertain(_)) => {
            // Re-read and investigate; resending can duplicate a comment.
            println!("update outcome unknown; investigate before retrying");
        }
        Err(other) => return Err(other.into()),
    }
    Ok(())
}
```

`Zendesk::connect` validates the token with `concord_auth::bearer` (no whitespace, controls or
non-ASCII) and uses the default `SendPolicy`; `Zendesk::new(http, ZendeskConfig { subdomain,
authorization, policy })` takes all three explicitly, and `set_policy` replaces the policy.
Obtaining and refreshing the OAuth token is the application's.

`http_client()` builds a transport without reqwest-level retries or redirects and with a 30 s
timeout per request. A caller-supplied client must also disable reqwest's retries and redirects:
with either, a write may be sent again, or a status may answer a later request than the write (a
performed create redirected to a target answering `429` would look refused).

## Failure semantics

Generated `send()` goes through `SendPolicy` (`prepare()` bypasses it). Read or write, and
which write turns a `409` into a collision, are one exhaustive `match` over the generated
`ZendeskOperation` (`src/policy.rs`): an endpoint added to the declaration does not compile
until it is classified there.

- **Reads** (`GET`) may be repeated. A `429`, or a `503` carrying `Retry-After`, is sent again
  after exactly the server's delay (seconds or HTTP date, parsed by
  `concord_retry::http::retry_after`), at most 3 attempts in all, and only if that delay is at
  most 60 s. Configure with `SendPolicy::read_attempts`, `max_retry_after` and `read_deadline`
  (an overall bound; a wait that would not fit is not started). A missing, invalid or too-long
  `Retry-After`, any other status and any transport failure are returned at once: there is no
  blanket retry.
- **Writes** (create `POST` and update `PUT`) are sent exactly once.
  - An update `409`: `Error::Collision`, retaining the `HttpError`; re-read the ticket and
    decide whether to submit a fresh change. Other `4xx` (including `429`) are `Error::Http`.
    A 429's `retry_after()` says when a request may be sent again; for creates retain the key.
  - No response (timeout, connection lost or refused), any `5xx`, or a
    `2xx` whose head or body cannot be read: `Error::Uncertain(UncertainWrite)` with the
    request's `idempotency_key()` when present and the observed `cause()`. Reconcile creates as
    above; for an update, re-read and investigate before deciding, because a repeated comment
    can be duplicated.
  - A refused connection is uncertain too: with a caller-supplied client that follows
    redirects, a performed create can end in a redirect target that refuses. Replaying with
    the same `IdempotencyKey` is safe.
- `Error::Http(HttpError)` keeps the authoritative status, `attempts()`, `retry_after()`,
  `request_id()` (`X-Zendesk-Request-Id`) and the body read with a 64 KiB bound as
  `document()`: a JSON error document (`error`, `description`, `details`), JSON `null`, or
  `text/plain`; or, if unreadable (another media type, oversized, malformed, truncated), the
  `concord::Error` saying why.
- Success bodies are bounded (8 MiB for a list page, 1 MiB for one ticket) and must be labelled
  `application/json`; otherwise the read fails with `Error::Concord` (`Metadata`, `Decode`,
  `Limit` or `Transport`).
- `ticket_pages` ends exactly when a page says `meta.has_more: false`. A page cap reached while
  more pages exist is `PagesError::Bounded` with the cursor to resume from. A page that has more
  but no usable `after_cursor`, that repeats the cursor that produced it, or whose `links.next`
  names another `page[after]`, is `Error::Pagination`.
- Inputs outside their range are refused before anything is sent: `PageSize` (1–100),
  `IdempotencyKey` (visible ASCII, no spaces), the bearer token (`Error::Credential`), an
  invalid subdomain label (`Error::Concord` of kind `Build`).

No `Debug` or `Display` of the client, a prepared request or an error prints the token. Error
descriptions, details and text bodies can echo submitted content: they are withheld from
formatting and reached with `*_unredacted()` accessors. Ticket models derive `Debug` and
print their content.

## Excluded

Help Center, incremental exports, attachments, ticket deletion and bulk endpoints, offset
pagination, side-loading (`include`), comment listing, OAuth token acquisition or
refresh, rate-limit admission or cooldown shared across calls, and blanket automatic retries.

## Files

- `src/lib.rs`: the declaration, limits, `Zendesk::connect`, `http_client`.
- `src/policy.rs`: `SendPolicy` (reads through `concord-retry`, writes once).
- `src/codecs.rs`: `CreatedTicketFormat` (JSON plus `x-idempotency-lookup`), error bodies.
- `src/pagination.rs`: `ticket_pages` over `concord-pagination`.
- `src/models.rs`, `src/error.rs`: provider models, inputs and the error type.
- `tests/protocol.rs`, `tests/workflows.rs`, `fixtures/`: see `PROVENANCE.md`.
