# Provenance

Where every provider fact and fixture of this SDK comes from. **Live provider calls: NOT_RUN.**
No request of this crate or its tests has reached Datadog; every behaviour below is the
documentation's claim or an authored assumption, tested only against a local stand-in.

## Documentation read

All three pages were read on **2026-11-07** (downloaded 06:29–06:30 UTC with `curl`, then read
as text; each page also offers a Markdown rendering at the same URL with `.md`, which was read
too: its tables, parameters and examples are the ones the HTML shows). Nothing else of Datadog's documentation was read: links from
these pages (key management, the logs pagination guide, the "Validate API key" page, scopes)
were not followed.

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

Bodies built inside the tests (truncated, oversized, mislabelled, a document echoing both keys)
are synthetic and described where they are built. The keys in the tests are synthetic strings
(`synthetic-...`), never real credentials.

## Dependencies

No new crate: `concord` (feature `json`), `concord-pagination`, `serde`, `serde_json` and, for
tests, `concord-example-testserver` and `tokio` were already in `Cargo.lock`. Adding the package
added only its own `[[package]]` entry to the lock file; no other package changed version.
