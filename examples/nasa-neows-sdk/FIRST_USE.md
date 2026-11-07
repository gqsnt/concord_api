# Fresh consumer — SDK22 NASA NeoWs

Date: 2026-11-07. Baseline: `0c8444d6ff48ce58192888b353ee1321f06a0513`.
Live provider calls: **NOT_RUN**. Every executable workflow uses loopback fixtures.

## Predictions

Written before creating caller code or making a working call, from the README
and generated public rustdoc. These predictions remain unchanged below; later
sections compare them with observations.

### 1. Date-range feed → asteroid lookup

- Construct a reqwest client with redirects disabled and a timeout; construct
  `NeoWs::new(http, NeoWsConfig { api_key })`, replace its base with the fixture
  server using `with_base_url`, and enter `api.neows()`.
- `neo.feed("2015-09-07", "2015-09-08").send().await` sends one bodyless GET to
  `/neo/rest/v1/feed`, with exactly `api_key`, `start_date=2015-09-07`, and
  `end_date=2015-09-08` in the query. Query order is immaterial. Expect
  `Accept: application/json`, a transport-generated Host, no Authorization or
  API-key header, and no request Content-Type because there is no body.
- Return is `Result<Feed, concord_sdk_nasa_neows::Error>`: an `element_count`
  and a date-keyed map of asteroid lists. Select a returned ID, then call
  `neo.lookup(&object.id).send().await`: one bodyless GET to
  `/neo/rest/v1/neo/{id}`, with only the key in the query and the same headers.
  Return is `Result<Asteroid, Error>` with ID, name and optional hazardous flag.
- A successful send completes one response, not the whole date-range/inspection
  application. Keep the feed and already inspected objects when a later lookup
  fails; there is no automatic rollback or partial-body result. A valid empty
  feed is complete and requires no lookup.
- Non-2xx, including fixture key rejection (403) or quota rejection (429),
  returns `ErrorKind::Status` and the HTTP status, with no automatic retry.
  Provider error codes/messages are discarded rather than typed or inferred.
  A 2xx error-shaped body lacking the required feed fields should fail Decode.
  Malformed successful JSON should fail Decode; wrong Content-Type should fail
  Metadata; successful bodies exceeding 4 MiB should fail Limit. Expect the
  received status to remain available on these post-response errors.

### 2. Browse → inspect returned object

- `neo.browse().set_page(0).set_size(2).send().await` sends one bodyless GET to
  `/neo/rest/v1/neo/browse` with `api_key`, `page=0`, `size=2`, and the same
  headers as the feed. `neo.browse().send()` omits page and size entirely.
- Return is `Result<BrowsePage, Error>`, containing objects and `PageInfo`:
  `size`, `total_elements`, `total_pages`, and zero-based `number`. Inspect a
  returned object's ID through the same lookup call as workflow 1.
- No page is fetched implicitly. The caller may explicitly request the next
  numbered page. An ordinary valid terminal page satisfies
  `number + 1 >= total_pages`; an empty dataset with `total_pages == 0` is
  complete. Use checked arithmetic in caller code and never follow provider
  links (which can embed keys). A full page alone does not prove completion.
- On a later-page or lookup failure, retain prior pages and details but mark the
  application incomplete. Status/provider/decode/metadata/limit behavior should
  match workflow 1. No stream or paginator type is needed.

### Shared boundaries and secrets

The key must appear on the loopback wire as one encoded query value, but not in
SDK error Display/Debug or chained sources. Test with a synthetic key containing
reserved characters, and with fixture errors that echo it in bodies and headers.
Client/config/endpoint Debug should also redact sensitive configuration. Response
links and unmodeled fields should be ignored. The ordinary path returns domain
metadata (feed count/page counters) and error kind/status, not response headers;
rate-limit headers and Retry-After are therefore not expected to be available.
Dates/ranges are not locally validated. Transport/build errors are distinct;
they have no received HTTP status. Native/prepare APIs bypass the safe policy and
are unnecessary for these caller tasks.

## Public reading and setup before code

- Read `examples/nasa-neows-sdk/README.md` first for SDK usage, then the package
  and workspace manifests for build setup. The package is already a workspace
  member and its dependencies are already locked; no addition was needed.
- Generated SDK rustdoc with
  `CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-nasa-neows --no-deps --offline`
  (passed). Read the crate index, NeoWs, NeoWsConfig, NeoWsNeows, generated
  Feed/Lookup/Browse endpoint pages, Feed, Asteroid, BrowsePage, PageInfo and Error.
  Only rendered public HTML was read, never rustdoc's source pages.
- Ran `CARGO_BUILD_JOBS=4 cargo build -p concord-sdk-nasa-neows --offline`
  without `--locked` for initial dependency reconciliation. Later Cargo commands
  use `--locked`; no new dependencies are planned.
- Read ancestor `AGENTS.md` and its START_HERE, DESIGN_CHARTER, PROMPT, AUTHORITY,
  contracts/PHASES and contracts/RESOURCES instructions. The explicit consumer
  capsule bounds this task; no broader campaign work is being performed.
- Resource check: `df -h .`, `free -h`, `du -sh target
  /home/canarit/.cargo/registry`, `nproc`: 365 GiB free, 23 GiB memory available,
  20 CPUs, initial target about 1019 MiB. Use only this worktree's default
  `target/`, four Cargo build jobs, one Cargo command at a time, reused artifacts.
  No cache cleanup needed. `CARGO_TARGET_DIR` is unset.

### Official provider documentation

Read on **2026-11-07**:

- [NASA API catalog, Asteroids–NeoWs section](https://api.nasa.gov/): feed date
  queries, SPK-ID lookup, and browse routes. The text-only direct open initially
  showed only the signup shell; the search service's indexed version exposed
  the NeoWs section. No example API request links were executed.
- [NASA authentication and rate-limit documentation](https://api.nasa.gov/assets/html/authentication.html):
  DEMO_KEY is limited to 30 requests/IP/hour and 50/IP/day; the documented general
  default is 1,000/hour, with service variation. These are documentation facts,
  not measured quotas. No equality of demo and registered-key quotas is claimed.

NASA's basic browse documentation does not document page/size or the complete
response schema. Page counters and modeled fields below use the SDK's public
contract; fixture values are synthetic and do not establish live conformance.

## Friction log (chronological)

1. The NASA catalog's direct text fetch lacked the NeoWs content. Read the
   indexed official catalog and its static authentication page instead. This is
   research-tool friction, not an SDK failure.
2. Generated `send()` rustdoc displays an associated-type expression
   `<Bounded as SendWith<Feed>>::Output` instead of the concrete Result. The
   README makes ordinary usage clear, but error and metadata discovery takes an
   extra public-doc lookup. No extra trait import was needed so far.
3. First test run compiled without import/type errors but both workflows failed
   with safe `Status`/404 errors. The next run added only path diagnostics:
   observed `/feed` and `/neo/browse`, not the predicted full provider paths.
   `with_base_url(server.url())` replaces the prefix `/neo/rest/v1` too. Public
   rustdoc says declared paths are appended to the replacement, but displays only
   the composed production URL, hiding the base/path split. Correct the caller to
   `with_base_url(format!("{}/neo/rest/v1", server.url()))`. No SDK source lookup.
   This is a real prediction mismatch; the first two runs each had 0/2 passing.
4. Completion and partial results require application bookkeeping: two small
   caller report types retain feed/pages, inspected objects, completion and an
   error. There is no completion helper on PageInfo. This is documented and
   predicted, but the README's single-page example does not demonstrate terminal
   pages, empty datasets or retention after a later failure. The caller uses
   zero-based counters, not page length, to decide completion.
5. To test failure stages, read Concord's public ErrorKind rustdoc (generated
   with `CARGO_BUILD_JOBS=4 cargo doc -p concord-core --no-deps --offline --locked`)
   and the harness Body enum. The SDK error exposes kind/status, while quota
   headers such as Retry-After are intentionally discarded. This is a documented
   observability tradeoff for a caller that might schedule retries, not an
   unexpected failure or a reason to bypass the safe send path here.

No unneeded trait imports, surprising generated method names, compiler errors,
misleading compiler diagnostics, or implementation-source lookups occurred.
Long generated endpoint type names were visible in rustdoc but never needed in
caller signatures. ErrorKind and domain models were imported for assertions and
application report storage; they were not prerequisites for ordinary calls.

### Caller construction decisions

The browse fixture uses size **1** rather than the predicted example size 2,
so two small pages exercise explicit continuation and a full terminal page.
The query still uses the predicted `page` and `size` fields. Both tasks inspect
every returned object in their small fixture; this allows an actual later
failure to preserve an earlier detail. Caller reports retain feed/pages,
details, completion and errors. A three-page application budget bounds browse
work independently of provider counters. Transport setup also disables native
retry and proxies for deterministic loopback-only requests.

Read harness public rustdoc (crate index, TestServer, Recorded, Reply) generated
with `CARGO_BUILD_JOBS=4 cargo doc -p concord-example-testserver --no-deps
--offline --locked`. Read Bounded's public associated Output to confirm the
concrete Result type. No harness or SDK implementation was opened.

## Assistance

Final label: **docs-only**. SDK source, existing SDK test source, FRICTION.md and
PROVENANCE.md remained unread throughout. Harness public rustdoc supplied the
loopback API. No author coaching or additional agents were used. The final
package check executed existing tests after the first working call; their names
and results appeared in Cargo output, but their source was not inspected.

## First working call

The third targeted test run passed **2/2** caller tasks. The first successful
request was browse page 0, followed by lookup of its returned `2000433` ID; the
feed/date-range task also passed. Required changes: add `/neo/rest/v1` to the
loopback base URL, after two failed runs that exposed the base/path split.
All methods, setters, response models, and borrowed ID calls compiled on the
first attempt. No extension trait import, prepared/native execution, source
lookup, new dependency, manifest change or lockfile change was necessary.

Command: `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-nasa-neows --test first_use
--offline --locked -- --test-threads=1`. Passing assertions covered all requests,
both connected workflows, returned IDs/names/optional flags, feed count/date
groups, browse counters, explicit terminal-page completion, and request counts
(three for feed/inspection, four for two browse pages/inspection).

The initial success fixtures use real-looking designations and synthetic
date associations/page counts, not captured NASA responses. The same two caller
tests were subsequently extended with partial-result and failure scenarios.

## Predictions compared with observations

| Prediction | Observed evidence | Result |
| --- | --- | --- |
| Replacing the base with the bare fixture origin preserves `/neo/rest/v1` | First two runs sent `/feed` and `/neo/browse`; full paths worked only after including the prefix in the replacement URL | **Mismatch**: base replacement includes its path |
| Feed GET carries two dates and the query key; lookup GET carries the returned ID in its path and only the key in query | Wire assertions for both dates, both returned IDs and every query pair, including reserved characters in the key | Match after base correction |
| Browse GET carries explicit page/size, defaults omit both | Two explicit size-1 pages plus a default browse request; exact query assertions | Match; size 1 was a deliberate fixture choice |
| Bodyless requests, JSON Accept, Host, no Authorization, key header or Content-Type | Shared `assert_wire` checks method, path, decoded query pairs, headers, loopback host, empty body and non-chunked transfer | Match |
| Feed/Asteroid/BrowsePage domain returns and optional hazardous flag | Dates/count, IDs/names, `Some(false)` and missing flag `None`, page size/number/totals asserted | Match |
| Other fields and links are ignored | Extra asteroid properties decode; key-bearing feed/page links do not appear in model Debug and cause no requests | Match |
| One response per send, explicit pagination, terminal page need not be short | Success feed task makes 3 requests; browse task makes 4; page 1 is full and terminal, with no third page or link follow | Match |
| Empty feed/dataset completes without inspection | Empty feed makes one request; empty browse report retains one page, no details and complete=true | Match |
| Later failures retain completed work but not completion | Feed second lookup 429 retains feed and first detail; browse second-page 429/malformed JSON retains page 0/detail; second lookup 403/malformed schema retains both pages and only first detail | Match |
| 403/429 are Status, without provider-code inference or retries | Synthetic invalid-key and over-quota envelopes produce kind/status; exact request counts show no retry; echoed codes omitted from errors | Match |
| Non-success bodies are not read | 429 with a permanently stalled body still returns Status rather than timing out; zero body bytes sent | Match for this fixture |
| Malformed successful JSON and 200 provider error envelope fail Decode, with no partial response value | Truncated feed/page, wrong lookup field type, and error-only feed envelope all return Decode/200; prior responses survive | Match |
| Wrong media fails Metadata before reading body | 200 text/html with stalled body returns Metadata/200, zero body bytes sent | Match |
| Successful bodies beyond 4 MiB fail Limit | Valid JSON with a padding field exceeding the cap returns Limit/200 and no feed | Match; decompression was not exercised |
| Post-response errors retain status | Status, Metadata, Decode and Limit assertions compare the exact response status | Match |
| Errors do not expose key, body, URL, headers or chained source | Display, Debug, alternate Debug and source checked for status/decode/metadata/limit and refused-loopback Transport errors; synthetic key/body/header echoes used | Match within tested surfaces |
| Config/client/scope/endpoint Debug redacts configured key | Direct Debug assertions for config, client, scope and all three endpoints | Match |
| HTTP rate-limit headers are not part of ordinary return values | Public Bounded Output is `Result<T, Error>`; domain structs expose only their documented fields and Error only kind/status. Fixtures include rate headers, but the caller cannot recover them | Match by public type inspection, not a runtime header-access test |
| Invalid dates are not locally validated | `not-a-date` reaches the fixture unchanged and produces provider Status/400 | Match |
| Transport failure is distinct and has no received status | Lookup after shutting down the loopback server returns Transport/None with safe formatting | Match |
| Build failures occur before I/O | Not independently exercised in this consumer; the final existing package suite also reports its build-error test passing | Unverified by fresh caller |

The browse caller's three-page budget leaves `complete=false` if exhausted.
The cap and integer-overflow branch are defensive application choices, not
independently exercised fixtures. Counter consistency, changing live datasets,
every accepted JSON media subtype, decompression and all possible sensitive
provider fields are outside these assertions. In particular, safe error output
does not make arbitrary provider-controlled domain strings safe to log.

## Frozen-boundary recommendations (not implemented)

- **Macros/generated public docs:** show the declared base URL separately from
  endpoint paths, and put a path-preserving loopback example on with_base_url.
  The failed prediction is evidence for a documentation change, not a hierarchy
  change. No shared crate was edited.
- **Macros/generated public docs:** link policy-backed send directly to its
  associated Output contract, so callers can find errors/metadata without
  navigating through the generic policy type. No new caller trait is needed.
- **SDK public docs:** add the concrete `/neo/rest/v1` fixture-base example and
  a short example of terminal counters and retaining results on later failure.
  These recommendations are recorded here; README and SDK implementation are
  unchanged to preserve the consumed baseline.
- **SDK policy / application:** consider an explicitly safe projection of
  quota headers if a later scheduling task needs it. Retain application-owned
  completion and partial-result bookkeeping for this bounded slice; no evidence
  here requires a universal paginator or shared retry change.

## Final verification and scope

- Initial offline build without `--locked`: passed; subsequent Cargo commands
  used `--locked`, four build jobs and the default worktree target.
- First two targeted executions: **0/2**, fixture 404s from the base-path
  mismatch. Third execution: **2/2** first working caller tasks. Failure-expanded
  execution: **2/2**. These are successive versions, not repeated independent
  evidence for untouched first-pass success.
- Final `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-nasa-neows --offline
  --locked`: **8 integration tests passed** (2 fresh consumer, 4 existing
  protocol, 2 existing workflows), **1 compile-only doctest passed**, 0 unit
  tests, 0 failures. Existing tests were executed only after fresh success.
- SDK, harness and Concord-core public rustdoc generation passed. Only public
  rendered docs were read; generated source HTML was never opened.
- `rustfmt --edition 2024 --check examples/nasa-neows-sdk/tests/first_use.rs`
  and `git diff --check`: passed. Formatting was restricted to the new caller
  file, not the SDK source or shared packages.
- Workspace member and lock entry were already present. Cargo.toml/Cargo.lock
  remain unchanged; no crate was added or version changed, so no new dependency
  provenance is required. No source edits, shared edits or commits were made.
- Deliverables: this file and `tests/first_use.rs`. Live provider calls remain
  **NOT_RUN**; documentation research only used official read-only pages above.
