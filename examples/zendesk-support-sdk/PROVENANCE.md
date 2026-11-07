# Provenance

Live provider calls: **NOT_RUN**. No request was sent to any Zendesk host. Every expectation
below comes from the official documentation pages listed here or is labelled synthetic or
inferred.

## Documentation read (all retrieved 2026-11-07)

The pages were read through a fetch tool that returns a summary of the page, not its verbatim
text; the facts below are recorded as summarized, with short phrases only. No documentation JSON
example was copied into a fixture.

| # | Page | Facts taken |
|---|---|---|
| D1 | https://developer.zendesk.com/api-reference/ticketing/tickets/tickets/ (slice citation) | List Tickets `GET /api/v2/tickets`, allowed for agents; cursor pagination "(recommended)" and offset pagination, not combinable; at most 100 records per page; cursor `sort` accepts `updated_at`, `id`, `status`, `-` for descending; parameters include `external_id`. Show Ticket `GET /api/v2/tickets/{ticket_id}` returns `{"ticket": ..}`; the first comment is `description`. Create Ticket `POST /api/v2/tickets`, "The only required property is `comment`" (with `body`), response 201 with `{"ticket": ..}`. Ticket fields (`id`, `url`, `external_id`, `type`, `subject`, `description`, `priority`, `status`, `requester_id`, `submitter_id`, `assignee_id`, `organization_id`, `group_id`, `tags`, `created_at`, `updated_at`, ...); `status` new/open/pending/hold/solved/closed; `priority` urgent/high/normal/low; `type` problem/incident/question/task. Examples authenticate with `Authorization: Bearer {access_token}`. The page does not mention `Idempotency-Key`. |
| D2 | https://developer.zendesk.com/api-reference/introduction/pagination/ (slice citation) | Cursor pagination is selected by `page[size]` (most endpoints cap it at 100); omitting it falls back to offset pagination. Responses carry `meta.has_more`, `meta.after_cursor`, `meta.before_cursor`, `links.next`, `links.prev`; repeat "until the `meta[has_more]` property is false"; the docs tell clients to request `links.next`. Support for cursor or offset varies by endpoint; offset is limited to 100 pages / 10,000 records (400 beyond). Not stated: whether `links.next` is null on the last page, whether cursors are opaque. |
| D3 | https://developer.zendesk.com/documentation/api-basics/best-practices/best-practices-for-avoiding-rate-limiting/ (slice citation) | Rate limiting answers 429 and the request is not processed; `Retry-After` gives the wait in seconds (example `93`); clients should stop requesting until it has elapsed, then retry; exponential backoff is recommended; rate-limit headers (`X-Rate-Limit`, `ratelimit-*`, `zendesk-ratelimit-tickets-index`). Nothing about non-idempotent requests or 503. |
| D4 | https://developer.zendesk.com/api-reference/ticketing/introduction/ | Idempotency for ticket creation: header `Idempotency-Key: {unique_key}`; first request 201 with `x-idempotency-lookup: miss`; same key and body returns the cached response with `hit` and no second ticket; same key with another body is `400` with `"error": "IdempotentRequestError"` and the description "Request parameters don't match the given idempotency key."; keys expire after two hours (a later duplicate creates a new ticket); the key is not stored on the ticket. |
| D5 | https://developer.zendesk.com/api-reference/introduction/requests/ | Status codes: 200/201/204 success; 4xx failures, `Content-Type` `text/plain` for API-level errors or `application/json` for business-level errors; 403 permissions; 409 conflict (serialize requests); 422 unprocessable; 429 usage limit; 5xx internal, a 503 may carry `Retry-After` (database timeout or deadlock). Business-level errors have `error`, `description`, `details` (example `RecordInvalid` / "RecordValidation errors" with per-field entries). `Accept: application/json` on all requests, `Content-Type: application/json` on POST. `X-Zendesk-Request-Id` identifies a request for support. |
| D6 | https://developer.zendesk.com/api-reference/introduction/security-and-auth/ | OAuth access tokens are sent as `Authorization: Bearer {access_token}`; tokens can be up to 184 characters (not enforced by this SDK); the API is TLS-only. |
| D7 | https://developer.zendesk.com/api-reference/introduction/rate-limits/ | 429 with `Retry-After` in seconds; the rate-limit header families. |
| D8 | https://developer.zendesk.com/api-reference/introduction/introduction/ | Read; an index page with none of the facts above. |

## Rules this SDK adds or infers

| Rule | Status |
|---|---|
| `page[size]` is always sent (initialized to 100), so List Tickets is always in cursor mode | SDK policy from D1/D2 |
| The next page is the template with `page[after]` = `meta.after_cursor`, not a request to the `links.next` URL | **inferred** equivalent of D2's instruction; enforced at run time: a page whose `links.next` names another `page[after]` is refused (`InconsistentNextLink`) |
| A page with `has_more: true` and no cursor, or with the cursor that produced it, is refused | SDK policy (no documentation statement) |
| Reads repeat a 429, or a 503 with `Retry-After`, after exactly the server delay, at most 3 attempts and 60 s per delay by default | SDK policy over D3/D5/D7; HTTP dates are accepted by the shared parser although the pages only show seconds |
| A 429 on a write was not processed, so it is `Error::Http`, not uncertain | from D3 ("the request isn't processed") |
| A 5xx on a create may have committed it, so it is uncertain | **inferred**, conservative; not documented |
| A connection that was never established sent nothing | transport semantics (`reqwest::Error::is_connect`), not a Zendesk statement |
| The idempotency key is required by this SDK (Zendesk makes it optional) | SDK policy, so every create can be resolved without duplication (D4) |
| `IdempotencyKey` must be visible ASCII without spaces | SDK rule; D4 documents no syntax |
| An `error` member that is not a string makes the document unreadable (status kept) | follows the documented shape only (D5) |
| Unknown enumeration values are kept as `Other(..)`, unknown fields ignored | forward compatibility; D1 says a custom status reports a category |

## Fixtures

Every fixture is authored for these tests. "Shape" means the field names and nesting follow the
cited page; all values are synthetic.

| Fixture | Derivation | Used by |
|---|---|---|
| `tickets-page-1.json` | synthetic; shape D1 (ticket fields) + D2 (`meta`, `links`, `has_more: true`); extra unknown fields `raw_subject`, `custom_fields`, `via` | workflows (list, bounded, repeated cursor) |
| `tickets-page-2.json` | synthetic; shape D1 + D2, last page (`has_more: false`, a non-null `after_cursor`, `links.next: null` — D2 does not say what the last page carries) | workflows, protocol (list) |
| `ticket-103.json` | synthetic; Show Ticket envelope D1, the same ticket as on page 2 | workflows, protocol |
| `ticket-unknown-values.json` | synthetic; values outside D1's enumerations, optional fields absent | protocol |
| `ticket-malformed.json` | synthetic; truncated JSON | protocol |
| `ticket-wrong-shape.json` | synthetic; a ticket without `id` | protocol |
| `page-more-without-cursor.json` | synthetic; `has_more: true`, `after_cursor: null` | workflows |
| `page-inconsistent-next.json` | synthetic; `after_cursor` and `links.next` disagree | workflows |
| `error-record-invalid.json` | shape from D5's `RecordInvalid` example (`error`, `description` "RecordValidation errors", per-field `details`); field entry synthetic | protocol |
| `error-idempotent-mismatch.json` | derived from D4 (error code and description as summarized there) | protocol, fake service |
| `error-forbidden.json` | synthetic; D5 business-level shape for 403 | protocol |
| `error-not-found.json` | synthetic; D5 business-level shape | protocol, fake service |
| `error-null.json` | D1 shows a `null` body for 404/409 of Update Ticket; reused for 404, 409, 503 | protocol |
| `error-unauthorized.txt` | synthetic; D5 API-level `text/plain` error | protocol |

Responses built inside tests (not files), all synthetic: the fake service's created tickets
(`tests/support/mod.rs`, `FakeZendesk`: ids from 9001, status `new`, D4's miss/hit/mismatch
rules), rate-limited replies (`429`/`503` with `Retry-After` values chosen per test), oversized
and truncated bodies generated by the test server, and a `text/html` proxy page.

No expectation was copied from this SDK's own output: request shapes are asserted against
D1/D2/D4/D5, and response expectations against the fixture contents.
