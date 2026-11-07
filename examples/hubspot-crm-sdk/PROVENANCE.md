# Provenance

Baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Example-assisted: see FRICTION.md for
all consulted SDK examples and observations. Live provider calls: **NOT_RUN**. All dates below
are the read date, 2026-11-07. Web research was read-only.

- Requested [OAuth token management URL](https://developers.hubspot.com/blog/production-ready-oauth-token-management-hubspot-integrations): inaccessible through the research tool on that date. The article was located and read at [its current URL](https://developers.hubspot.com/blog/oauth-token-management-hubspot-integrations): Bearer tokens, proactive refresh, potential rotation, form body and coordinating refresh.
- [Contacts object article](https://developers.hubspot.com/blog/a-developers-guide-to-hubspot-crm-objects-contacts-object): create, search, and lookup examples and contact properties.
- [Search contacts reference](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/search/search-contacts): search result envelope, `paging.next.after`, maximum page size.
- [Manage OAuth tokens, 2026-09](https://developers.hubspot.com/docs/api-reference/latest/authentication/manage-oauth-tokens): endpoint, refresh form fields and token response.
- [OAuth v1 deprecation](https://developers.hubspot.com/changelog/v1-oauth-api-deprecation): migration from legacy endpoint to date-based API.
- [OAuth v3 endpoint and error changes](https://developers.hubspot.com/changelog/new-oauth-v3-api-endpoints-and-standardized-error-responses?hs_amp=true): form credential placement and standardized error fields.

| Fixture | Derivation |
| --- | --- |
| `oauth.json` | Synthetic credentials and lifetime, shaped by the OAuth management response example. |
| `search-first.json` | Synthetic result and cursor, shaped by the search reference; contact email/name from the contacts article; the untrusted link is synthetic. |
| `search-next.json` | Synthetic second page and contact, shaped by the search reference. |
| `contact.json` | Synthetic lookup result matching the synthetic second page, shaped by the contacts/search examples. |
| `created.json` | Synthetic ID and result using the Anna properties from the contacts article. |
| Inline test responses | Synthetic, including failures, cursor cases, OAuth rotations, and timeouts; authored to exercise behaviors, not copied from SDK output. |

The package adds no new third-party package to Cargo.lock; dependencies and all versions were
already present. The only lock change is the new workspace package entry.
