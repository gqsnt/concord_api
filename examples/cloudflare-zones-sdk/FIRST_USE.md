# Fresh consumer: SDK40 Cloudflare Zones

Current API note (P4.2): this evidence records the earlier API. Paging `_from` twins now use the base factory followed by `.starting_at(Resume::At(key))`; Datadog/Notion traversal cycles use `PagesError::Repeated`. See [migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application requests used
`TestServer::start(|request: &Recorded| -> Reply)` on loopback. The SDK implementation,
existing tests, `FRICTION.md` and `PROVENANCE.md` have not been opened.

## Predictions

Written before application code or a provider call, from the package README and generated
public rustdoc.

### Workflow 1: account zones, then one zone

- Construct `Cloudflare::connect(http_client()?, token)?`, then replace the entire base URL
  with `server.url() + "/client/v4"`. Navigation is `cf.zones().list()` with
  `.set_account_id(AccountId::from(account))` and `.set_per_page(ZonesPerPage::new(5)?)`.
  `.pages()` starts at page 1; `.collect_complete()` returns pages with items and metadata.
  `.items().collect_complete()` instead returns a flat `Vec<Zone>`.
- Wire: bodyless `GET /client/v4/zones?account.id=<account>&page=1&per_page=5`, then page 2
  with the same filter and size (query ordering is not part of the prediction). Headers:
  `Authorization: Bearer <token>`, `Accept: application/json`, and the loopback `Host`.
  No legacy authentication headers or token in the URL/body.
- Fetch a selected response identifier through `cf.zone(zone.id.clone()).get().send()`:
  bodyless `GET /client/v4/zones/<zone-id>`, no query, same authentication and Accept.
  Return `Response<Zone>` with `result`, `messages`, non-fatal `errors` and `meta`.
  A single list `.send()` returns `PageResponse<Zone>` with optional `result_info`.
- End follows `result_info.page >= total_pages`, even with a short/empty intermediate page
  or misleading `total_count`. A complete walk returns `Ok`; a second `next_page()` after
  exhaustion stays `Ok(None)` without a request. A cap with pages remaining fails as
  `PagesError::Bounded`, preserving the prefix and resume position.

### Workflow 2: selected zone's DNS records, then one record

- Select the zone via the ordinary zone detail call, then use
  `cf.zone(zone.id.clone()).dns_records().list().set_record_type(RecordType::A)` and
  `.set_per_page(RecordsPerPage::new(1)?)`. Walk pages, retain metadata, and fetch an item
  with `cf.zone(zone.id).dns_record(record.id.clone()).get().send()`.
- Wire: bodyless `GET /client/v4/zones/<zone-id>/dns_records?type=A&page=1&per_page=1`,
  then page 2 with identical filters. Detail is bodyless
  `GET /client/v4/zones/<zone-id>/dns_records/<record-id>`, without query. The Bearer,
  Accept and Host headers match workflow 1.
- Return `PageResponse<DnsRecord>` for a single list send, `RecordPages` for traversal,
  and `Response<DnsRecord>` for detail. Records expose typed IDs/types, optional content,
  TTL (`1` means automatic), proxy flags, tags and timestamp text. The same completion,
  partial-result and resume rules apply as for zones.

### Failure, metadata and secret predictions for both workflows

- HTTP 200 with `success: false` yields `Error::Provider`, keeps status 200, ordered
  provider errors/messages and response metadata, even if `result` has the wrong shape.
  It never becomes an empty successful list. A failed second page makes collection return
  `Incomplete` with only page 1's items and a `PagesError::Provider` naming page 2.
  The default policy sends once; an explicit `pages_from(failed)` can resume.
- Non-2xx other than 429 yields `Error::Provider`; unreadable error bodies retain the HTTP
  status and expose their decode/metadata diagnostic via `document()`. A 429 yields
  `Error::RateLimited`, parsed `RetryAfter`, all named limits, and one attempt by default.
- Malformed JSON or malformed successful result is `Error::Concord`/`ErrorKind::Decode`;
  a non-JSON successful response is `Metadata`. `success: true` without `result` yields
  `Error::Envelope`/`MissingResult`. Missing page information or a page mismatch fails
  traversal, while a single `.send()` can return absent optional `result_info`.
- Global and service limits coexist as separate ordered named `Ratelimit` and
  `Ratelimit-Policy` entries. Metadata preserves status, operation and headers. An invalid
  rate-limit header fails only `rate_limits()`, not the call. No client bucket or automatic
  delay is implied by the headers.
- Invalid credentials fail at connection construction without sending. The token is sent
  only in the Bearer header and absent from client/scope/endpoint Debug and error
  Display/Debug. Metadata Debug hides header values, including cookies. Provider message
  text remains available deliberately and may appear in provider Debug.

## Public material read before first call

- `examples/cloudflare-zones-sdk/README.md` and package/root Cargo manifests, toolchain
  manifest and lock membership (build setup only). Workspace member and lock entry already
  existed; no manifest change was needed.
- `cargo doc -p concord-sdk-cloudflare-zones --no-deps --offline` with
  `CARGO_BUILD_JOBS=4`, default `target/`, without `--locked`: passed; lock unchanged.
- Generated SDK rustdoc: crate index; `Cloudflare`; generated list endpoints; `Paging`;
  `Zone`, `DnsRecord`, `PageResponse`, `PageInfo`, `PageMeta`, `ResponseMeta`, `Error`,
  `ProviderError` and rate-limit types. After writing the predictions, also read the public
  pages for `ZonesPerPage`, `RecordsPerPage`, `ZoneAccount`, `ResponseInfo`, `EnvelopeError`,
  `EnvelopeProblem`, `RetryAfter`, `Response` and both generated detail endpoints.
  Source hyperlinks were not followed.
- Supplemental public rustdoc generated with `cargo doc --locked --offline -p
  concord-pagination -p concord-example-testserver -p concord --no-deps`: passed.
  Read the testserver index, `TestServer`, `Recorded`, `Reply`, and pagination `Pages`,
  `Items`, `Incomplete`, `PagesError`, `Page` documentation. After writing the predictions,
  also read `Outcome`, `SequentialReport`, `Progress`, `Resume`, `PageItems` and Concord's
  `Operation` rustdoc. No implementation was read.
- Official pages read on **2026-11-07**:
  [List Zones](https://developers.cloudflare.com/api/resources/zones/methods/list/)
  (Bearer, account filter, endpoint envelope and 5–50 page size);
  [List DNS Records](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/list/)
  (zone path, type filter, envelope and 1–5,000,000 page size);
  [Rate limits](https://developers.cloudflare.com/fundamentals/api/reference/limits/)
  (global and specific limits, named response headers and Retry-After).

## First working call

The first run of `cargo test --locked --offline -p concord-sdk-cloudflare-zones --test
first_use account_zones_then_selected_zone -- --exact` compiled and decoded both zone pages
and zone detail through ordinary `.send()`/`.pages()` calls. The final wire assertion failed:
the request path was `/client/v4/client/v4/zones`, not `/client/v4/zones`. The scripted
fixture had answered by request order; receiving a decoded result alone was therefore not
accepted as a working provider-shaped call.

Changed only the caller setup to `.with_base_url(server.url())` and reran the same command:
**1 passed, 0 failed**. The first verified call was `listing.pages().next_page().await`:
`GET /client/v4/zones?account.id=023e105f4ecef8ad9ca31a8372d0c353&page=1&per_page=5`
(query compared without ordering), returning five typed zones plus page metadata. The same
test then received page 2, proved exhaustion and fetched the selected zone by its returned
ID. It took two test executions, one base-URL correction, no compiler type fixes, no SDK
changes and no implementation lookup. The staged warnings were removed by completing the
planned caller tests.

## Friction incidents (in observation order)

1. Generated endpoint `send()` rustdoc exposes the associated policy output instead of a
   concrete result type. The README's return-type table resolved it without a source lookup.
   Suggested shared generator improvement (not edited): link the policy's concrete output.
2. `--no-deps` SDK docs link to pagination types whose pages are not generated. Generated
   public docs for pagination and the fixture server separately, with `--locked`.
3. A documentation-only lookup guessed `concord_pagination::Completion`; that page does
   not exist. The actual `Pages` docs describe `Outcome` and explicit `Ok(None)` completion.
   A later lookup also guessed `struct.Resume` instead of `enum.Resume`; reading its actual
   public enum page resolved that. These were consumer lookup mistakes, not SDK diagnostics.
4. The staged first test compilation reported unused DNS/error helper imports and functions
   prepared for the second caller task. These were consumer scaffolding warnings, not
   required unused imports or SDK failures. Completing the second task used all of them;
   the final compilation has no warnings.
5. The first wire assertion found a duplicated `/client/v4` prefix. The generated endpoint
   docs render a complete request URL and `with_base_url` says declared paths append to its
   path, but the public client description did not make the base/path split obvious. My
   predicted replacement base was wrong. Suggested documentation improvement (not edited):
   show `with_base_url(server.url())` and the resulting request target in the README.

No additional friction appeared while completing both tasks. Completion and generated
navigation names were usable from the docs. No extension-trait import or explicit generated
endpoint type was required for an ordinary call; pagination/error types were imported only
for assertions about bounds, recovery and diagnostics.

## Prediction comparison

| Prediction | Observation in the independent caller tests |
|---|---|
| Base replacement includes `/client/v4` | **Mismatch:** the declared paths already include it. Supply the loopback origin alone. |
| Workflow 1 calls and wire shape | Matched after correcting the base. Both pages retain the account filter and page size; detail uses the returned typed ID. Every request is bodyless GET with Bearer, JSON Accept, loopback Host, and no legacy auth. |
| Workflow 1 outputs and metadata | Matched: typed zones, optional account/status fields, direct `PageResponse`, detail `Response`, response operation/status/headers and non-fatal errors on a successful detail. |
| Workflow 2 calls and wire shape | Matched: fetched the zone, listed its A records with size 1, then fetched the selected returned record ID. A third page was deliberately added after an empty intermediate page to exercise the predicted continuation rule. |
| Workflow 2 outputs | Matched: page metadata/messages, typed records and IDs, content, proxy flag, tags, timestamp text and automatic TTL. A direct send exposes `result_info`. |
| Completion | Matched: last-page metadata ends the walk despite `total_count=2000`; an empty intermediate DNS page continues. Repeated zone `next_page()` after exhaustion sends nothing, and `finish()` reports `Exhausted` with two successes. |
| Caps | Matched: the one-page zone cap returns `Bounded`, five collected items and resume page 2; it sends no second request. |
| HTTP 200 provider refusal | Matched on both listings' second pages and both detail endpoints. Wrong-shaped refusal `result` never hides ordered errors/messages or produces an empty success. Status remains 200. |
| Partial results and recovery | Matched for both workflows: `Incomplete` contains precisely the first page's items; counters report one successful and one failed page; `pages_from(failed)` fetches page 2 once and completes. |
| Status errors | Matched on both detail endpoints: 403 is a provider error; HTML 503 retains status plus a Metadata diagnostic from `document()`. |
| Rate limiting | Matched: both details return `RateLimited` for 429, `RetryAfter::After(3s)` and one attempt. A zero-remaining service limit on a successful page does not prevent the next normal request. No retry opt-in or client bucket was used. |
| Malformed bodies/envelopes | Matched on both details: invalid JSON and wrong successful object types are Decode, HTML 200 is Metadata, and missing result is MissingResult. Zone traversal refuses missing paging data and a mismatched page; after failure it returns Ended, not exhaustion. Direct listing send permits missing page info. |
| Named limit metadata | Matched: global and DNS entries retain order, remaining/reset, quotas/windows and exhaustion separately. Malformed DNS listing limit syntax fails only the accessor. |
| Secrets | Matched for synthetic credentials: invalid tokens fail without sending; client/scope/endpoint Debug and success/failure diagnostics omit the token; metadata Debug omits a synthetic cookie value. Deliberate header access and fixture wire capture retain values. Invalid `..` path construction also sends nothing. |

The prediction snippets used `new(5)?`/`new(1)?` as schematic validation. Rustdoc clarifies
these constructors return `Option`, so tests use `unwrap()` on fixed valid fixture sizes;
no constructor compile failure occurred. The only behavioural prediction mismatch was the
base/path split. These results establish local fixture behaviour, not live-provider parity.

## Validation and change scope

- `cargo test --locked --offline -p concord-sdk-cloudflare-zones --test first_use`:
  **2 passed, 0 failed**, without warnings. The two tasks assert 35 loopback wire requests
  across their scenarios, including zero requests on credential/build failures.
- `cargo test --locked --offline -p concord-sdk-cloudflare-zones`:
  **51 tests passed** (9 unit, 2 new caller, 29 existing protocol, 11 existing workflow),
  plus **1 doctest passed**. Existing test executables were run; their source was not read.
- `rustfmt --check --edition 2024 examples/cloudflare-zones-sdk/tests/first_use.rs`: passed.
- `git diff --check`: passed. All Cargo commands used `CARGO_BUILD_JOBS=4`, one at a time,
  with the default worktree `target/`; after the first offline doc build all used `--locked`.
- Only `FIRST_USE.md` and `tests/first_use.rs` were added under this SDK. No SDK `src/`,
  shared crate, checks, scripts, other example, existing test, README, provenance or friction
  file was changed. Root `Cargo.toml` and `Cargo.lock` remain unchanged because membership
  and dependencies already existed. No new crate, version change or commit was introduced.

## Assistance label

**docs-only**. No SDK source, existing SDK test source, `FRICTION.md`, `PROVENANCE.md`,
campaign file or parent-directory material was read. Only public docs, build manifests and
lock membership, generated documentation and the new consumer's own files were inspected.
