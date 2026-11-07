# Provenance

## Maintenance research — 2026-11-07

The maintainer reread these exact official pages using read-only browser retrieval on
**2026-11-07**. No other external documentation was consulted:

- [Create DNS Record](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/):
  POST path, JSON request, `DNS Write`, zone-ID/name lengths, address and target content,
  TTL ranges, TXT syntax, response envelope and created `result`.
- [List Zones](https://developers.cloudflare.com/api/resources/zones/methods/list/):
  Bearer authentication, account filtering, paging and envelope retained.
- [List DNS Records](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/list/):
  reconciliation listing, per-endpoint paging and read contract retained.
- [Rate limits](https://developers.cloudflare.com/fundamentals/api/reference/limits/):
  concurrent global/service limits and Retry-After observation retained.

Create accepts the documented automatic TTL or 60–86400 seconds; 30–59 requires the explicit
Enterprise constructor. The application asserts the zone's plan. The create input name is
bounded to 1–255 ASCII/Punycode bytes. IPv4/IPv6 content and CNAME self-target restrictions
follow the reference. Quoted TXT is checked locally; long strings are passed for provider
splitting. Owner/target DNS-label checks, requiring preconverted Punycode, supporting six
content-based types and allowing `proxied: true` only for A/AAAA/CNAME are conservative SDK
choices; the reference's shared schema itself exposes `proxied` on other types. MX and URI
require priority, and other types need more validation/fields, so this change refuses them.
The allowed reading list supplies no complete plan/proxy capability contract. Provider-side
permissions, entitlements and cross-record conflicts remain authoritative.

No idempotency or safe POST replay guarantee was found on these pages. Treating transport
loss as uncertain and reconciling by listing are SDK safety decisions required by the ticket,
not claims that Cloudflare guarantees exactly-once creation or immediate list consistency.
Duplicate fixture code **81057** and its message are synthetic test data, not a documented
stable code contract; callers retain every provider code/message without special-casing it.

Added fixtures: `dns_record_created.json` uses the create response envelope and a minimal
record with synthetic values; `dns_record_exists.json` is a synthetic refusal with an
intentionally wrong-shaped result. Inline create-consumer replies, timeouts, dropped
connections and stalled bodies are synthetic. All run through the local fixture server.

The only new **direct dependency** is already-locked `percent-encoding` **2.3.2**,
**MIT OR Apache-2.0** (license verified in its local cargo-registry manifest). It decodes the
prepared zone path segment before checking its character count, avoiding incorrect rejection
of percent-encoded IDs. No new crate package or version is added to the lock; cargo adds
only this dependency edge to the SDK package entry. No other package version changes.
The earlier sections below describe the original authoring pass and were retained as history.

Live provider calls: **NOT_RUN**. Nothing in this package has been sent to
`api.cloudflare.com`; every request in the tests goes to `concord-example-testserver` on
127.0.0.1. Agreement with the real service is not established by these tests.

## Documentation read

All pages were retrieved on **2026-11-07** (06:15–06:22 UTC) with `curl`, as HTML and, for the
API reference pages, also as their `index.md` rendering (the "View as Markdown" link of the same
page). Copies were kept under the worktree's `target/tmp/docs` for reading only; they are not part
of the package.

| # | Page | Listed in the brief | Used for |
|---|---|---|---|
| 1 | <https://developers.cloudflare.com/api/resources/zones/methods/list/> | yes | `GET /zones`, query parameters, `Zone` schema, `result_info`, the 200 example |
| 2 | <https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/list/> | yes | `GET /zones/{zone_id}/dns_records`, query parameters, record schema, `result_info`, the 200 example |
| 3 | <https://developers.cloudflare.com/fundamentals/api/reference/limits/> (shows "Last updated Aug 25, 2026") | yes | global and per-service limits, `Ratelimit`, `Ratelimit-Policy`, `retry-after`, 429 |
| 4 | <https://developers.cloudflare.com/api/resources/zones/methods/get/> | **no** | `GET /zones/{zone_id}` (Zone Details), its 200 example |
| 5 | <https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/get/> | **no** | `GET /zones/{zone_id}/dns_records/{dns_record_id}` (DNS Record Details), its 200 example |

Pages 4 and 5 go beyond the three pages the brief lists. Both workflows require fetching one
object ("fetch one zone by ID", "fetch one record"), which pages 1–2 only link to; the two pages
are the same official reference, linked from pages 1 and 2. This is a deliberate, recorded
deviation from the brief's reading list; nothing else outside the worktree was read except the
Rust toolchain and cargo registry sources.

## Facts taken, and what is inferred

Observed on the pages:

- Base `https://api.cloudflare.com/client/v4` (curl samples on pages 1, 2, 4, 5); the SDK
  declares `base "https://api.cloudflare.com"; path "client/v4";` so a test base URL keeps the
  version prefix.
- Authentication: "API Token … `Authorization: Bearer …`", the preferred scheme (pages 1, 2, 4,
  5). The legacy `X-Auth-Email`/`X-Auth-Key` scheme is not implemented.
- Envelope: `errors` and `messages` (arrays of `{code ≥ 1000, message, documentation_url?,
  source?: {pointer?}}`), `success`, `result` (optional), `result_info` (optional, listings).
  Zones pages declare `success: boolean`; DNS pages declare `success: true`.
- The 200 examples of all four endpoints carry one `errors` entry beside `success: true`. The SDK
  therefore treats `success` as authoritative and keeps such entries as non-fatal
  (`Response::errors`).
- List Zones: `account { id, name }`, `direction`, `match`, `name`, `order` (`name`, `status`,
  `account.id`, `account.name`, `plan.id`), `page` (minimum 1), `per_page` (5–50), `status`
  (`initializing`, `pending`, `active`, `moved`), `type`. "Listing zones across more than 500
  accounts is currently not allowed."
- List DNS Records: `zone_id` (maxLength 32), `type` (21 record types), `proxied`, `order`
  (`type`, `name`, `content`, `ttl`, `proxied`), `direction`, `page` (minimum 1), `per_page`
  (1–5,000,000), and filters not implemented here (`comment.*`, `content.*`, `name.*`, `tag.*`,
  `search`, `match`, `tag_match`, shadow metadata).
- `result_info { count, page, per_page, total_count, total_pages }`, all optional, on both
  listings; `total_count` is "Total results available without any search parameters".
- Zone fields (`id`, `account`, `activated_on`, `created_on`, `development_mode`, `name`,
  `name_servers`, `original_name_servers`, `paused`, `status`, `type`, ...); record fields
  (`id`, `name`, `type`, `content`, `ttl` with "1 means automatic", `proxied`, `proxiable`,
  `comment`, `tags`, `created_on`, `modified_on`, ...).
- Limits page: "Client API per user/account token 1200/5 minutes", "Client API per IP
  200/second"; some API calls "have their own limits"; exceeding the global limit returns HTTP 429
  for the following five minutes. Headers: `Ratelimit` (`"default";r=50;t=30`),
  `Ratelimit-Policy` (`"burst";q=100;w=60`), `retry-after` (seconds, rounded up, only when a limit
  was exceeded).

Inferred, not stated on the pages (each is a test-only assumption until checked live):

- **`account.id` on the wire.** Page 1 renders the parameter as the object `account { id, name }`
  and does not show its query serialization. The SDK sends `account.id=<id>`, the dotted spelling
  the same page uses for the `order` value `"account.id"`.
- **Error statuses carry the envelope.** The pages show 200 examples only. The SDK reads any
  non-2xx JSON body as `{errors, messages}` and keeps the status authoritative when the body is
  absent, HTML or malformed.
- **A 2xx `success: false` body** is not exemplified; its shape is taken from the schema
  (`success: boolean` with `errors`). Its fixtures are synthetic.
- **`Ratelimit`/`Ratelimit-Policy` syntax.** The examples are RFC 8941 list items
  (`"name";key=value`); the SDK parses that subset (string or token names, integer `r`, `t`, `q`,
  `w`) and refuses the rest rather than guessing.

## Fixtures

Every fixture is either derived from a cited 200 example (same fields and nesting; values
replaced by synthetic ones: identifiers, `*.example.com` names, documentation addresses
192.0.2.0/24 and 2001:db8::/32) or authored and labelled synthetic. No expectation was copied
from the implementation: each test asserts values written in the fixture or on the wire.

| Fixture | Derivation |
|---|---|
| `zones_page1.json` | Page 1, "200 example". First zone keeps every field of the example (synthetic values); four further zones carry a subset. Synthetic choices: `activated_on: null` and `original_name_servers: null` on a pending zone, all five statuses. `result_info` page 1 of 2, `per_page` 5. |
| `zones_page2.json` | Page 1, "200 example" shape; one zone, page 2 of 2. |
| `zone_details.json` | Page 4, "200 example": every field, synthetic values; keeps one `errors` and one `messages` entry (code 1000) beside `success: true`, as the example does. |
| `dns_records_page1.json` | Page 2, "200 example": the first record keeps every field (synthetic values); a second, `AAAA` with `ttl: 1`. Page 1 of 2, `per_page` 2. |
| `dns_records_page2.json` | Page 2, "200 example" shape; one `TXT` record with `comment: null` (synthetic). |
| `dns_record_details.json` | Page 5, "200 example": every field, synthetic values. |
| `failure_success_false.json` | **Synthetic.** HTTP 200 body with `success: false`, two `errors` (codes 1001, 1002, one with `source.pointer`), one `messages` entry, `result: null`. Shape from the documented `ResponseInfo` schema. |
| `failure_unexpected_result.json` | **Synthetic.** `success: false` whose `result` is an object and `result_info` a string. |
| `error_403.json` | **Synthetic.** A non-2xx envelope (code 1004); inferred, see above. |
| `error_429.json` | **Synthetic.** The 429 envelope (code 1005); its `Ratelimit`, `Ratelimit-Policy` and `retry-after` headers are set by the tests from page 3's examples. |
| `success_without_result.json` | **Synthetic.** `success: true`, `result: null`. |
| `page_without_result_info.json` | **Synthetic.** A listing without `result_info`. |
| `page_mismatch.json` | **Synthetic.** `result_info.page` 3 (the test requests page 2). |
| `empty_listing.json` | **Synthetic.** `result: []`, `total_pages: 0`. |
| `undocumented_values.json` | **Synthetic.** A zone with an undocumented `status` and `type` and an unknown field. |

Bodies written inline in `tests/protocol.rs` (HTML pages, an empty 500, malformed and truncated
JSON, an error body over 64 KiB, a success body over 16 MiB, a missing `content-type`, malformed
rate-limit headers) are synthetic.

## Dependencies

No crate was added to `Cargo.lock`; the only lock change is this package's own entry. All
dependencies were already workspace members or locked: `concord`, `concord-auth`,
`concord-pagination`, `concord-retry` (feature `concord`), `serde`, `serde_json`, `tokio`
(feature `time`), and for tests `concord-example-testserver` and `tokio`. The `serde_json`
feature `raw_value` is enabled (features are not recorded in the lock): it keeps `result` unparsed
until `success` is known.

## Examples consulted

Read in this worktree, as examples of Concord usage (the result is example-assisted):

| File | Taken from it |
|---|---|
| `examples/zendesk-support-sdk/Cargo.toml`, `src/lib.rs`, `src/policy.rs` | A stateful send policy as a client value (`send policy;`), `concord_auth::bearer` for the `Authorization` value with a `connect` constructor, `concord_retry::Retry::receive` with a `Retry-After`-only classifier and zero local backoff, an `http_client()` without retries or redirects. |
| `examples/slack-sdk/Cargo.toml`, `src/lib.rs` | An envelope whose 2xx can be a failure (`ok: false`); the decision to keep the HTTP status authoritative and read error bodies bounded. Its manual `execute` function was not copied (this SDK uses the send policy). |
| `examples/world-bank-sdk/src/lib.rs` (lines 150–260) | A declared output that is the wire envelope with `impl SendWith<WireResponse>` returning a domain result, under plain `send SendPolicy;`. |
| `examples/fred-sdk/src/pagination.rs`, `src/lib.rs` (lines 70–110) | A page walker generic over `Sends<..>` and a parameter-group trait (`params ...`), setting the page key on a clone of the generated request. |

Also read: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`,
`docs/RECIPES.md`, the READMEs of `concord-pagination`, `concord-retry`, `concord-auth`,
`concord-ratelimit`, `concord-codecs`, `examples/testserver/src/lib.rs`, and parts of the sources
of `concord-core` (`SendWith`, `Sends`, `Received`, `ResponseMeta`, `IntoValue`),
`concord-macros` (`group_traits`, `with_output`), `concord-pagination` (`Pages`, `PagesError`,
`Incomplete`), `concord-retry` (`Policy`, `Run`, `Report`, `receive`, `http::retry_after`),
`concord-auth` (`bearer`, `CredentialError`) and `concord-ratelimit` (`Limiter`, `Observation`).
