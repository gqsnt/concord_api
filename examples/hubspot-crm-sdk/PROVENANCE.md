# Provenance

Baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Example-assisted: see FRICTION.md for
all consulted SDK examples and observations. Live provider calls: **NOT_RUN**. All dates below
are the read date, 2026-11-07. Web research was read-only.

- Requested [OAuth token management URL](https://developers.hubspot.com/blog/production-ready-oauth-token-management-hubspot-integrations): inaccessible through the research tool on that date. The article was located and read at [its current URL](https://developers.hubspot.com/blog/oauth-token-management-hubspot-integrations): Bearer tokens, proactive refresh, potential rotation, form body and coordinating refresh.
- [Contacts object article](https://developers.hubspot.com/blog/a-developers-guide-to-hubspot-crm-objects-contacts-object): create, search, and lookup examples and contact properties.
- [Search contacts reference](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/search/search-contacts): search result envelope, `paging.next.after`, maximum page size.
- [Manage OAuth tokens, 2026-09](https://developers.hubspot.com/docs/api-reference/latest/authentication/manage-oauth-tokens): endpoint, refresh form fields and token response.
- [OAuth v1 deprecation](https://developers.hubspot.com/changelog/v1-oauth-api-deprecation): migration from legacy endpoint to date-based API.
- Batch read maintenance (2026-11-07, read-only). The requested reference path
  `.../legacy/crm/objects/contacts/batch/read-contacts` returned 404; the reference was read at
  [Retrieve a batch of contacts (legacy v3)](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/batch/get-contacts.md):
  `POST /crm/v3/objects/contacts/batch/read`, `inputs[].id`, required `properties` and
  `propertiesWithHistory`, optional `idProperty`, `archived` query; `200` and `207` envelopes;
  `StandardError` with `category`, `context` (map of string arrays) and an `id` described as the
  error instance's ID. Compared with the
  [2026-03 reference](https://developers.hubspot.com/docs/api-reference/2026-03/crm/objects/contacts/batch/get-contacts.md)
  (same schema, versioned path; not adopted, to keep the SDK's legacy routes consistent).
- [Contacts guide (legacy)](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/guide):
  batch read by record ID needs no `idProperty`; "Batch operations are limited to 100 records".
- [Error handling (legacy)](https://developers.hubspot.com/docs/api-reference/legacy/error-handling.md):
  multi-status envelope example (`status`, `results`, `numErrors`, `errors[]` with `category`,
  `message`, `context`), all error fields optional, retry guidance for 429 and 5xx (left to the caller).
- Secondary, not official: a web search on 2026-11-07 summarized a HubSpot Community thread
  ("Question about batch read API"; the page itself answered 403 to the research tool) reporting
  `OBJECT_NOT_FOUND` errors naming IDs in `context.ids`, and a merged contact returned under a new ID.
  The SDK's `context.ids` attribution and `unmatched` list rest on this observation.
- [OAuth v3 endpoint and error changes](https://developers.hubspot.com/changelog/new-oauth-v3-api-endpoints-and-standardized-error-responses?hs_amp=true): form credential placement and standardized error fields.

| Fixture | Derivation |
| --- | --- |
| `oauth.json` | Synthetic credentials and lifetime, shaped by the OAuth management response example. |
| `search-first.json` | Synthetic result and cursor, shaped by the search reference; contact email/name from the contacts article; the untrusted link is synthetic. |
| `search-next.json` | Synthetic second page and contact, shaped by the search reference. |
| `contact.json` | Synthetic lookup result matching the synthetic second page, shaped by the contacts/search examples. |
| `created.json` | Synthetic ID and result using the Anna properties from the contacts article. |
| `batch-read-complete.json` | Synthetic `200` batch envelope (results in non-request order), shaped by the batch read reference; contacts match the search fixtures. |
| `batch-read-partial.json` | Synthetic `207` envelope with an `OBJECT_NOT_FOUND` item error naming `999` in `context.ids`, shaped by the batch read reference, the error-handling multi-status example and the secondary `context.ids` observation; the message is a synthetic marker for redaction tests. |
| Inline test responses | Synthetic, including failures, cursor cases, OAuth rotations, timeouts, and batch attribution/contradiction cases; authored to exercise behaviors, not copied from SDK output. |

The package adds no new third-party package to Cargo.lock; dependencies and all versions were
already present. The only lock change is the new workspace package entry.
