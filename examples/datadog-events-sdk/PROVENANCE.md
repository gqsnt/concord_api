# Provenance

Where every provider fact and fixture of this SDK comes from. **Live provider calls: NOT_RUN.**
No request of this crate or its tests has reached Datadog; every behaviour below is the
documentation's claim or an authored assumption, tested only against a local stand-in.

## Documentation read

For the original read-only implementation, all three pages were read on **2026-11-07** (downloaded 06:29–06:30 UTC with `curl`, then read
as text; each page also offers a Markdown rendering at the same URL with `.md`, which was read
too: its tables, parameters and examples are the ones the HTML shows). Nothing else of Datadog's documentation was read: links from
these pages (key management, the logs pagination guide, the "Validate API key" page, scopes)
were not followed.

### Independent posting maintenance (2026-11-07)

The maintainer re-read the authentication, list and search pages above on **2026-11-07**
and read the [Events reference](https://docs.datadoghq.com/api/latest/events/) and its
[Post an event page](https://docs.datadoghq.com/api/latest/events/post-an-event/) on the same
date. The Events reference now links to a separate posting page; its HTML and same-page
`.md` rendering were downloaded with `curl` and the v1 model, examples and response were read.
The web reader rejected the four supplied URLs as unsupported content; no API was called.

| Provider fact | Implementation / decision |
|---|---|
| v1 `POST https://api.<site>/api/v1/events` accepts `title`, `text`, `tags`, `alert_type`, `aggregation_key`, `source_type_name` | `src/api.rs` keeps the same site host and scope; `NewEvent::source` writes `source_type_name`. Other optional request fields are omitted. |
| v1 text is limited to 4,000 characters; aggregation key to 100 | `NewEvent` validates both before any request exists. The docs do not specify how Unicode characters are counted; Unicode scalar values are the SDK's explicit interpretation, exercised at and above each limit with multibyte strings. |
| v1 alert types: `error`, `warning`, `info`, `success`, `user_update`, `recommendation`, `snapshot` | Closed `AlertType` enum. No arbitrary string setter or unchecked request deserializer. |
| v1 title has no stated limit and the second request example has 100+ characters; v1 tags/source have no stated limits | No invented length cap or v2 limits. Source vocabulary remains a provider responsibility; its linked vocabulary page was not read. |
| v1 returns HTTP 202 with optional `status` and `event`; its schema describes numeric IDs and `id_str` to avoid precision loss | `EventAcceptance` preserves status and the opaque event object. Only 202 is acceptance; the SDK makes no claim about indexing or search visibility and sends no follow-up. The reference's `"integer"` placeholders decode unchanged because event schemas remain raw JSON. |
| v1 aggregation keys group events in the Event Stream | This is not a documented idempotency guarantee; an uncertain submission cannot safely be retried merely because it has the same aggregation key. |
| v1 is deprecated and only available to organizations that previously used it; v2 uses `event-management-intake.<site>` and category-specific attributes | The ticket's fields and same-host rule select v1. README and public API docs disclose this limitation. v2 intake is excluded; no claim of availability for new organizations. |
| Authentication reference requires an API key for writes and an application key for reads | Posting deliberately retains both keys, as required by this maintenance ticket. This is an SDK contract, not a claim that the provider requires both for writes. |

Timeout uncertainty is an SDK rule requested by the ticket: a transport timeout does not prove
that ingestion failed. `Error::PostUncertain` preserves the safe error and any response head;
missing heads, unusable 202 bodies and unexpected 2xx are conservatively uncertain. Non-2xx
classification remains shared with the reads. No automatic retry is performed. A provider
5xx is returned as a provider error and is not proof that a write did not happen.

No new crate or feature was added. `PostJson` delegates encoding to Concord's `Json` and wraps
the resulting body using reqwest's existing `Body::wrap` to prevent replay. This choice was
checked against the installed **reqwest 0.13.4** source (`async_impl/body.rs`, `retry.rs`) and
**http-body 1.0.1** source (`src/lib.rs`): wrapped bodies are not cloneable for retries.
Regression tests also supply an always-retry policy, including timeout and 429/503 cases,
and assert exactly one recorded request. The tests exercise loopback HTTP/1.1; TLS, HTTP/2,
provider ingestion, regional enforcement and real search consistency remain **NOT_RUN**.

| Page | Used for |
|---|---|
| <https://docs.datadoghq.com/api/latest/authentication/> | every request must be authenticated; reads "require full access and also require an application key"; the site list (`app.datadoghq.com` → `api.datadoghq.com`, ..., `us2.ddog-gov.com` → `api.us2.ddog-gov.com`); clients default to the US site |
| <https://docs.datadoghq.com/api/latest/events/get-a-list-of-events/> (v2 section) | `GET https://api.<site>/api/v2/events`; query strings `filter[query]`, `filter[from]`, `filter[to]` ("in milliseconds"), `sort` (`timestamp`, `-timestamp`), `page[cursor]`, `page[limit]` (default 10); `events_read` permission; responses 200 (schema and example), 400, 403, 429 with the `{"errors": [..]}` schema (`errors` required); `meta.page.after` "use the same parameters with the addition of the `page[cursor]`"; the per-site endpoint table and the site selector labels (US1 ... US2-FED); Rust client example run with `DD_API_KEY` / `DD_APP_KEY` |
| <https://docs.datadoghq.com/api/latest/events/search-events/> (v2 section) | `POST https://api.<site>/api/v2/events/search`; body `filter.{query,from,to}` (defaults `*`, `now-15m`, `now`; "date math and regular timestamps in milliseconds"), `options.{timeOffset,timezone}` ("either ... but not both, otherwise the query fails"), `page.{cursor,limit}` (`limit` int32, default 10), `sort`; two example bodies; the same 200/400/403/429 responses; the `DD_SITE` values listed in the curl example's comment |

### Facts the SDK uses that the cited pages do **not** state

| Fact | Where it comes from | Risk |
|---|---|---|
| The header names `DD-API-KEY` and `DD-APPLICATION-KEY` | The author's prior knowledge of Datadog's API (its OpenAPI security schemes). The cited pages say both keys are required but, in the 2026-11-07 rendering, never name the headers: their curl examples use `Authorization: Bearer ${DD_BEARER_TOKEN}` and their client examples read `DD_API_KEY` / `DD_APP_KEY` environment variables. | If the names were wrong, every request would be a 403. **Unverified** against the cited pages and never exercised live. |
| 401 is an authentication error | HTTP semantics; the pages document only 403 ("Not Authorized") | none for classification; the stand-in's 401 is synthetic |
| A 403 for wrong-region keys, with body `{"errors": ["Forbidden"]}` | the author's assumption of how a site answers keys it does not know; the pages document 403 with the example body `{"errors": ["Bad Request"]}` | the SDK classifies by status, not by text, so the body's wording does not matter |
| The selector label of each table row (`EU` for `app.datadoghq.eu`, ...) | inferred: the page lists the labels and the table rows in the same order | cosmetic (`Site::label`, `Display`) |
| An empty or repeated `meta.page.after` must not be followed | SDK rule: sending it back would restart or loop (not stated by the pages) | a provider that legitimately repeats a cursor would end the traversal with `Error::Protocol` |

### Documentation quirks observed

- The 200 example writes the placeholder strings `"integer"` into `int64` fields of the
  event-specific attributes (`date_happened`, `duration`, `monitor.id`, `monitor_id`, ...). The
  SDK keeps that object as raw JSON (event schemas are excluded from the slice), so the
  documented example decodes verbatim (`the_documented_page_decodes_verbatim`).
- The 400, 403 and 429 examples all show `{"errors": ["Bad Request"]}`.
- `links.next` in the example points at `https://app.datadoghq.com/api/v2/events?...` — the web
  host, not the API host — and at the GET endpoint even on the search page. The SDK keeps it as
  data and continues with `meta.page.after`.
- The search page's two example responses (OK, OK with pagination) share one 200 example body,
  identical to the list page's.

## Fixtures

`*.doc.json` fixtures were extracted mechanically from the Markdown renderings (the v2 section,
`json` code blocks) with a script kept outside the crate; it asserted that the list and search
200 examples are byte-identical and that the six error examples are identical, before writing
them unchanged. `*.synthetic.json` fixtures were written by the author, following the documented
schema; their values (ids, cursors, messages, hosts) are invented.

| Fixture | Derivation | Used by |
|---|---|---|
| `events-200.doc.json` | 200 example of *Get a list of events* (v2), verbatim; identical to the 200 example of *Search events* | `protocol.rs`: `list_request_on_the_wire`, `search_request_on_the_wire`, `the_documented_page_decodes_verbatim`, `unusable_success_bodies` (as the body of mislabelled replies), redirect target |
| `error.doc.json` | 400 / 403 / 429 example of both pages, verbatim | `protocol.rs`: `documented_error_statuses_are_classified`, `undocumented_statuses_keep_their_class` (reused for synthetic statuses), `error_documents_that_cannot_be_read_keep_the_status`; `workflows.rs`: the 429 page |
| `search-request-1.doc.json` | "Search events returns OK response" request body, verbatim | `search_bodies_match_the_documented_examples` |
| `search-request-2.doc.json` | "Search events returns OK response with pagination" request body, verbatim | `search_bodies_match_the_documented_examples` |
| `sites.doc.json` | the v2 endpoint table of *Get a list of events* (site, API host), the `DD_SITE` values of the *Search events* curl comment, paired by order; labels paired by order (inferred, see above). The script asserted `api_host == "api." + dd_site` for every row. | `sites_match_the_documented_table` |
| `list-range.synthetic.json` | synthetic: two events in the documented 200 shape, no `meta.page.after` | workflow 1 |
| `search-page-1.synthetic.json` | synthetic: two events, `meta.page.after = synthetic-cursor-1`, a `links.next` in the documented form | workflow 2; `a_repeated_cursor_ends_the_traversal_before_a_loop` (served for every request) |
| `search-page-2.synthetic.json` | synthetic: two events, `after = synthetic-cursor-2`, one warning copied from the documented example's `warnings` | workflow 2 |
| `search-page-3.synthetic.json` | synthetic: one event, `meta` without `page` (the last page) | workflow 2 |
| `search-page-empty-cursor.synthetic.json` | synthetic: no events, `after = ""` | `an_empty_cursor_ends_the_traversal_as_a_protocol_error` |
| `error-403-forbidden.synthetic.json` | synthetic: `{"errors": ["Forbidden"]}` (see the table above) | the wrong-region and unknown-application-key stand-in |
| `post-request.doc.json` | first v1 request example from *Post an event*, verbatim | `post.rs`: documented wire request |
| `post-long-title.doc.json` | v1 long-title request example from *Post an event*, verbatim | `post.rs`: title longer than 100 characters stays supported |
| `post-202.doc.json` | v1 HTTP 202 example from *Post an event*, verbatim, including `"integer"` placeholders | `post.rs`: acceptance envelope and unusable-response fixtures |

Bodies built inside the tests (truncated, oversized, mislabelled, a document echoing both keys)
are synthetic and described where they are built. The keys in the tests are synthetic strings
(`synthetic-...`), never real credentials.

Posting fixtures authored in `tests/post.rs` and `tests/post_consumer.rs` are synthetic:
the latter returns `status: ok`, an ID above JavaScript's exact-integer range and an EU web URL;
these are test values, not captured responses or a guarantee of provider status text. Local
401/403 errors deliberately reflect synthetic keys. Gates and stalled bodies simulate timeouts
after receipt, before or after HTTP 202; unexpected statuses, oversized/malformed bodies and
connection loss test uncertainty. No returned web URL is followed.

## Dependencies

No new crate: `concord` (feature `json`), `concord-pagination`, `serde`, `serde_json` and, for
tests, `concord-example-testserver` and `tokio` were already in `Cargo.lock`. Adding the package
added only its own `[[package]]` entry to the lock file; no other package changed version.
