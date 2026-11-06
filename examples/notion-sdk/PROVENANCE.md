# Sources and fixture provenance

Access date: **2026-11-06**. API contract/header pinned: **2026-03-11**.
The official pages below were opened and checked at kickoff. All three endpoint
references identify 2026-03-11 as their required version-header option; the versioning
page also uses that header value. This records the selected contract, not a promise
to track later changes automatically.

No fixture was obtained from a workspace, live API, vendor SDK, or credentials.
Every fixture is hand-authored from documented shapes. IDs, content, pagination
splits, timestamps, request IDs and errors are synthetic. Required fields for the
SDK's subset are represented; unrelated resource metadata is intentionally omitted.
Fixture bytes are protected by `.gitattributes` (`fixtures/** -text`).

## Official reference pages checked

| Reference | Sections used |
| --- | --- |
| [Query a data source](https://developers.notion.com/reference/query-a-data-source) | Request method/path/body; Headers; Overview (page and wiki data-source results); Pagination limit and request_status; Errors and additional_data |
| [Retrieve a page](https://developers.notion.com/reference/retrieve-a-page) | Headers; Parent and properties; Limits; error and page response shapes; separate block content and property-item calls |
| [Retrieve block children](https://developers.notion.com/reference/get-block-children) | Headers; paginated list response; first level of children; has_children/recursive retrieval guidance; capabilities and errors |
| [Introduction](https://developers.notion.com/reference/intro) | Conventions: API base, bearer auth, Notion-Version, JSON Content-Type, cursor continuation |
| [Versioning](https://developers.notion.com/reference/versioning) | Required Notion-Version; explicit 2026-03-11 examples |
| [Pagination](https://developers.notion.com/reference/pagination) | Requests: unchanged opaque cursor, POST body versus GET query, page_size 1..100; Responses: list, results, has_more, next_cursor |
| [Filter data source entries](https://developers.notion.com/reference/filter-data-source-entries) | Select / equals condition with a single option-name string |
| [Page property values](https://developers.notion.com/reference/page-property-values) | Attributes (id/type/type-named payload); title, rich_text, checkbox, number, select, date, relation pagination limits |
| [Rich text](https://developers.notion.com/reference/rich-text) | Text content/link, equation expression, mention payload and plain_text; annotations deliberately omitted by rendering |
| [Block](https://developers.notion.com/reference/block) | Block identity/type/has_children; paragraph, headings, bulleted_list_item, to_do, divider payloads |

All entries in the fixture mapping below use version **2026-03-11** and access date
**2026-11-06**. References are linked above by their exact page titles.

## Fixture mapping

| File under fixtures/ | Official page / section | Authorship and transformations |
| --- | --- | --- |
| `query-request-1.json` | Query a data source / request body; Filter data source entries / Select; Pagination / Requests | Hand-authored compact JSON: Priority equals Done and page_size=2. No start_cursor on first request. Exact serializer byte oracle; no final newline. |
| `query-request-2.json` | Same sections; Introduction / Pagination | Same filter and size, plus the synthetic cursor `query/cursor+two` copied unchanged. No final newline. |
| `query-1.json` | Query a data source / response and Pagination limit; Page property values / Title, Select | Hand-authored one-page result with synthetic content, complete request_status and continuation. Reduced metadata; first of an invented two-page split. |
| `query-2.json` | Query a data source / Overview (wiki children), response, Pagination limit | Second synthetic page and a synthetic data_source result. Invented `synthetic_wiki_child` tests retention of fields outside the typed page subset. Null cursor, complete status. |
| `page.json` | Retrieve a page / Parent and properties, Limits; Page property values / Attributes and six supported types; Rich text / Text, Equation, Mention | Combined synthetic page with Unicode, linked text, an equation and a page mention. `future_metric` is invented. Relation with has_more=true is an invented partial-value example of a documented shape. No actual page content included. |
| `children-1.json` | Retrieve block children / response and first-level guidance; Block / Headings, Paragraph; Rich text / Text | Synthetic heading_2 and paragraph. Paragraph has_children=true; children intentionally absent from the response. First page of an invented split. |
| `children-2.json` | Same list/continuation shape; Block / To do, Bulleted list item, Divider | Synthetic second page. `future_callout` and its badge/nested payload are invented; has_children=true is retained. Supported blocks follow the new type unchanged. |
| `nested-children.json` | Retrieve block children / recursive retrieval; Block / Paragraph | Synthetic separately fetched child of the known paragraph parent. |
| `unknown-children.json` | Same sections | Synthetic separately fetched child of the unknown parent. No claim that future_callout is an actual Notion block type. |
| `empty-more.json` | Pagination / Responses; Retrieve block children / fewer than page_size results | Synthetic zero-result page with a cursor. Edge case invented to prove that result count does not imply exhaustion. |
| `empty-last.json` | Pagination / Responses | Synthetic valid empty final page, also used as a base for malformed-envelope mutations. |
| `query-incomplete.json` | Query a data source / Pagination limit | Hand-authored terminal page with documented incomplete status/reason. One result substitutes for the documented 10,000-result limit to keep the fixture small. |
| `error-404.json` | Retrieve a page / 404 response and Errors | Documented object_not_found envelope with synthetic message and body request ID. |
| `error-429.json` | Retrieve a page and Retrieve block children / 429 response; Errors | Documented rate_limited envelope with synthetic message/request ID. Tests add Retry-After=3 or 5 as synthetic response-header metadata. |
| `error-503.json` | Query a data source / Errors, additional_data retry guidance | Hand-authored shortened service_unavailable envelope. Retains endpoint_name and one retry-guidance string; message is explicitly synthetic, not a verbatim official example. |

## Synthetic wire and failure perturbations in tests

`tests/protocol.rs` adds loopback response headers, uses two explicit version contexts,
tests CRLF in a version value, and supplies a path segment/cursor containing reserved
characters. Those are encoding/isolation probes, not documented valid Notion IDs.
The prior 2025-09-03 version is used solely as a distinct synthetic header value;
its response contract is not tested or claimed supported.

Other synthetic protocol cases: HTML gateway error; invalid JSON; HTTP/body status
mismatch; an invented future error code; null number/select/date values; malformed
supported number/text/block payloads; missing or wrong success Content-Type; a cut-off
HTTP body; and success/error bodies one byte beyond SDK limits. These are constructed
directly or by mutating the fixture's parsed JSON. No mutated response is attributed
to Notion.

`tests/workflows.rs` constructs contradictory has_more/cursor pairs, self-loop and
A/B/A cursors, an unfamiliar request_status, configured page caps, a failed second
page, and item-stream failure. All are synthetic scheduling/completion scenarios.
Requests and response content remain offline. Tests establish behavior against these
fixtures and Concord's loopback transport, **not vendor conformance**.

## Maintenance fixtures (2026-11-06)

The maintenance role rechecked [Retrieve block children](https://developers.notion.com/reference/get-block-children),
[Block](https://developers.notion.com/reference/block),
[Page property values](https://developers.notion.com/reference/page-property-values),
[Introduction](https://developers.notion.com/reference/intro),
[Pagination](https://developers.notion.com/reference/pagination), and
[Versioning](https://developers.notion.com/reference/versioning). The contract remains
2026-03-11. These new fixtures were hand-authored from the documented shapes, not
downloaded examples or live responses. Unmodeled resource metadata is omitted.

| File under fixtures/ | Documented shape and synthetic additions |
| --- | --- |
| `maintenance-page.json` | Page and property-value shapes; all six supported properties, text and equation fragments. `future_progress`, its nested payload/extra fields, and all IDs, dates, text and example.invalid links are invented. The percent-encoded property ID is an opaque retention probe. |
| `maintenance-children-1.json` | Block-children list with heading_1, paragraph and bulleted_list_item. Three results are fewer than requested page_size=5 but have a continuation. The opaque cursor includes leading space, a literal `%2F`, plus, reserved characters, Unicode, trailing space and tab; its entire value is synthetic. |
| `maintenance-children-2.json` | Final list page with invented `future_panel` followed by heading_2, heading_3, to_do and divider. The unknown block has_children=true. All payload data and the two-page split are synthetic. |

`tests/maintenance.rs` reuses `unknown-children.json` for that unknown parent's child
and `error-429.json` for a failed second page. It mutates the new second page's to_do
checked value to a string for a decoding failure. These cases are synthetic, not
provider examples. Reply gates force overlapping requests for two version contexts at both
page boundaries; 2025-09-03 is only a distinct header value, not a tested schema.
The synthetic response version header must not change subsequent request versions.
Cursor wire encoding is checked against a literal request target as well as the
decoded parameter. Returned links are compared unchanged and never fetched.
The existing `fixtures/** -text` rule protects all three new fixture files.

## Implementation reading and dependency choices

Concord README, author/caller/custom-format guides, concord-pagination and
concord-extras READMEs, public runtime rustdoc, and the GitHub SDK's api/meta/error/pages
source were consulted. `cargo doc -p concord --no-deps --features csv,jsonl` completed.
GitHub source supplied navigation and response-boundary patterns; no GitHub provider
payloads or campaign notes were used. Held-out campaign records and other SDK
FRICTION/FIRST_USE/MAINTENANCE/INTEGRATION files were not read.

Runtime dependencies are Concord's facade (JSON codec/reqwest), concord-pagination
(owned lazy traversal), serde (typed JSON models), and serde_json (unknown payload
retention and provider error parsing). This slice has no signing/XML/generic protocol
that would warrant another implementation dependency. HTTP execution always stays
with generated Concord operations. Test-only dependencies are the path testserver,
Tokio, and futures-util for item-stream verification. No new registry package was
added to the lockfile; only the new workspace package entry was added.
