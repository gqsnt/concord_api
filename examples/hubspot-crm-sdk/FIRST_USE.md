# SDK30 HubSpot CRM — fresh consumer

Date: 2026-11-07. Frozen HEAD: `a5e03c44173ca3f4ee44248fb6152938c3d94f42`.
Live provider calls: **NOT_RUN**. All executable requests use `TestServer::start` on loopback.
This record contains observable actions and decisions, not private reasoning.

## Predictions

Written before application code or a working call. Discovery so far: package README,
generated public rustdoc (including generated endpoint documentation), public Concord
pagination documentation, and public testserver documentation. No SDK source, existing
SDK tests, FRICTION.md or PROVENANCE.md has been read.

### Common setup and authentication

- Construct `HubSpot::new(http, client_id, client_secret, refresh_token)` using a finite
  timeout and explicitly disabled reqwest redirects/retries, then `with_base_url` to the
  local server **before cloning**. No endpoint traits or generated endpoint types should
  be necessary on the ordinary caller path.
- The first awaited CRM operation sends `POST /oauth/2026-09/token`, no query, with
  `Content-Type: application/x-www-form-urlencoded`, `Accept: application/json`, and form
  fields `grant_type=refresh_token`, `client_id`, `client_secret`, `refresh_token`.
  No CRM bearer Authorization header should be sent to the token endpoint. Credentials
  should be form-encoded correctly, never placed in URLs.
- A successful token body supplies `access_token`, optional replacement `refresh_token`,
  and `expires_in`. Clones share one refresh in flight and publish the replacement token
  before allowing CRM requests. Competing callers must never restore an older token.
  A second refresh after the 30-second margin should submit the replacement, not the
  original token; a response without another replacement should keep the current one.
- CRM calls send `Authorization: Bearer <access token>` and `Accept: application/json`.
  No implicit retry, redirect, or 401 replay. Successful JSON must have JSON media type
  and fit the documented 1 MiB limit.
- Refresh 400/401/403 failures are definite `Error::Refresh { status, uncertain: false }`.
  An unusable successful token response or 5xx/transport failure is uncertain and blocks
  further refresh with `RefreshUncertain`, including token snapshot access. SDK Debug
  and error Display/Debug must not expose credentials, bodies or contact values.
  `refresh_token()` deliberately returns secret material for coordinated persistence.

### Workflow 1: search, follow continuation, retrieve

- Call `sdk.search(SearchRequest { query: Some("Anna"), properties: ["email",
  "firstname", "lastname", "phone"], limit: Some(2), ..Default::default() }, Resume::Start)`
  with a finite page cap. Use `next_page` to retain page metadata, and
  `items().collect_complete()` for the ordinary flattened/partial-result examples.
- Each page sends `POST /crm/v3/objects/contacts/search`, no query, JSON content type.
  JSON uses HubSpot field spelling (including `filterGroups`/`propertyName` when filters
  are present), preserves query/properties/limit and puts continuation in body `after`.
  Start has no effective `after`; an input `SearchRequest.after` is overridden by traversal.
  The next request uses exactly `paging.next.after`; an optional provider `link` is ignored.
- Returns `Page<Contact, String, u64>`: selected contacts, next cursor, and `total` metadata.
  Null property values become `None`. `total` may change between pages and does not decide
  completion. An empty page with a next cursor must still be followed. No next cursor
  ends the listing; `next_page()` then returns `Ok(None)`, with `Outcome::Exhausted`.
- Select one result and call `sdk.get(&contact.id)`: `GET /crm/v3/objects/contacts/{id}`,
  no query or body, bearer/Accept headers, returning a `Contact`. Successful get/create
  return models without HTTP response headers; page metadata contains only `total`.
- A cap with more pages returns an incomplete prefix with a resume cursor, not success.
  A page error preserves earlier contacts and identifies the failed page. A fresh traversal
  with the same query and `Resume::At` can explicitly resume after resolving the error.
  Empty/immediately repeated cursors fail rather than silently complete.
- HTTP 429/401/404/5xx surface as `Error::Concord` status failures with status and headers;
  provider JSON errors are not parsed into a typed HubSpot envelope. Invalid JSON on 2xx
  is a decode error; wrong successful media is a metadata error. No later page is sent
  after failure. Error formatting must omit planted response-body secrets.

### Workflow 2: create, later retrieve by ID

- Build `CreateContact.properties` for Anna Smith (`anna.smith@example.com`, first and
  last name, company), then `sdk.create(input).await`. Send exactly one
  `POST /crm/v3/objects/contacts`, no query, JSON `{"properties": {...}}`, bearer/Accept
  and JSON Content-Type. A valid 201 response yields the created `Contact`.
- Later `sdk.get(&created.id)` sends the same GET shape as workflow 1 and returns the
  matching ID/properties. The access token should be reused without another refresh.
- Explicit non-408 4xx (e.g. 409 duplicate) is a Concord status error with response head,
  no parsed provider error. Timeout after dispatch, 408, 5xx or unusable 2xx response
  is `CreateUncertain`; no automatic second create and no invented ID. A fixture will
  record a created contact but withhold its reply, proving timeout does not mean absence.
  Only read-based reconciliation using an independently known fixture ID is appropriate.
  Exactly-once creation and durable cross-process token transactions remain out of scope.

## Public sources and initial actions

- Read `examples/hubspot-crm-sdk/README.md` first (2026-11-07).
- Ran `CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-hubspot-crm --no-deps --offline`
  without `--locked`: passed. Workspace membership and lock entry already existed;
  no manifest or lock change was needed. Subsequent cargo commands use `--locked`.
- Generated public documentation for `concord`, `concord-pagination`, and
  `concord-example-testserver` with `--no-deps --offline --locked`; passed. Only rendered
  API pages were read, never rustdoc's source pages. Supplemental docs are needed for
  lossless pagination/error inspection and the mandatory fixture API.
- SDK rustdoc pages read before predictions: crate index, `HubSpot`, `SearchRequest`,
  `CreateContact`, `Contact`, `Error`, `RefreshRequest`, `RefreshResponse`, `SearchResponse`,
  `Filter`, `FilterGroup`, `Next`, `CrmApiContactsSearch`, `CrmApiContactsGet`,
  `OAuthApiOauthRefresh`.
- Pagination public pages read: `Pages`, `Page`, `Resume`, `PagesError`, `Incomplete`,
  `SequentialReport`, `Outcome`; also inspected `Items` before following the correct
  `Pages::items` return type (`PageItems`). Fixture public pages: crate index,
  `TestServer`, `Recorded`, `Reply`, `Gate`.
- Read [A Developer's Guide to HubSpot CRM Objects: Contacts Object](https://developers.hubspot.com/blog/a-developers-guide-to-hubspot-crm-objects-contacts-object)
  on **2026-11-07**: basis for contact properties, legacy CRM routes and create/get flow.
- Requested [the supplied OAuth article URL](https://developers.hubspot.com/blog/production-ready-oauth-token-management-hubspot-integrations)
  on **2026-11-07**; direct retrieval failed twice. Search located the same titled article
  at [Production-Ready OAuth Token Management for HubSpot Integrations](https://developers.hubspot.com/blog/oauth-token-management-hubspot-integrations),
  read **2026-11-07**. It supports form-body credentials, latest refresh-token use,
  refresh coordination, and caution about retrying timed-out creates. Its `/oauth/v3/token`
  example differs from this SDK's documented `/oauth/2026-09/token`; fixture wire
  expectations follow the SDK's bounded contract. No live endpoint validation is claimed.
- Administrative reads: applicable ancestor `AGENTS.md`, its requested mission/authority/
  phase/resource documents, root and package manifests, git status/HEAD, managed-runtime
  skill/network guidance. No environment-status tool or network-policy file was available.
  The narrow consumer task supersedes broader campaign work. No delegation or source lookup.
- Existing target cache was reused at the default worktree `target/` (1.2 GiB).
  Capacity observed: 203 GiB disk available, 23 GiB available RAM, 20 logical CPUs.
  `CARGO_BUILD_JOBS=4`; one cargo process at a time. No target override or cleanup.

## Execution log

1. Created `tests/first_use.rs` from the documented `HubSpot` path, with synthetic
   credentials and Anna Smith contacts. First run:
   `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-hubspot-crm --test first_use --offline --locked`.
   **1 passed, 0 failed**, no compiler corrections, 0.01s test runtime.
2. First working call: `sdk.search(..., Resume::Start).max_pages(...).next_page().await`
   returned the first page after one local OAuth exchange. The same test then followed
   `002` through an empty page to `004`, observed changing totals, reached explicit
   exhaustion at a cap exactly equal to the page count, and retrieved contact `80101`.
   Wire assertions passed for form encoding, bearer/Accept/Content-Type headers, JSON
   field spelling, cursor placement and absent query strings. Five requests total:
   one OAuth, three search pages, one get. No SDK source or existing tests were read.
3. Supplemental public docs read for assertions: Concord `Error`, `ErrorKind`,
   `ResponseHead`; pagination `Progress`, `PageItems`. These exposed response-head
   retention after decoding failures and the exact progress counter names.
4. Expanded to two caller tests, each with local success/failure scenarios, and formatted
   only the new test file. Second test run compiled without warnings but **both tests
   failed** (1.19s). Create's no-replacement refresh fixture omitted `token_type` and
   returned `Refresh { status: None, uncertain: true }` before CRM dispatch. Search reached
   the refresh-failure scenarios, where the assumed second-call `RefreshUncertain`
   assertion failed. Earlier search, partial/error metadata and concurrent rotation
   assertions had completed. These first failures are preserved here before corrections.
5. Added `token_type: "bearer"` to the no-replacement and short-lifetime fixtures and
   added a redacted diagnostic for the later-refresh assertion. Third run: **1 passed,
   1 failed**, 1.19s. The entire create workflow (including timeout reconciliation and
   six failure cases) passed. After an initial HTTP 503 refresh, the next get returned
   `Refresh { status: None, uncertain: true }`, not the dedicated `RefreshUncertain`.
   Adjusted the consumer assertion to this observed variant while continuing to require
   zero additional wire requests and refusing token snapshot persistence. Added an
   explicit missing-`token_type` negative fixture to preserve the discovered requirement.
6. Fourth targeted run: **2 passed, 0 failed**, 1.21s. Every authored scenario completed.
   Both required workflows use ordinary `HubSpot` methods; `search_page` is used only to
   stage a competing caller in the refresh test. No generated endpoint constructors,
   `prepare`, `Received`, codec implementations, or source lookups were needed.
7. Final package regression command:
   `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-hubspot-crm --offline --locked`.
   **13 tests passed**: 1 existing unit test, 2 fresh consumer tests, 6 existing protocol
   tests, 4 existing workflow tests; 0 doctests. Fresh consumer tests took 1.22s.
   Existing tests were executed only after the consumer tests passed; their files were
   never opened or used as implementation guidance.
8. `rustfmt --edition 2024 --check examples/hubspot-crm-sdk/tests/first_use.rs` and
   `git diff --check` passed. Final status contains only the two new consumer files.
   No SDK source, shared crate, other example, workspace manifest, lockfile or existing
   test file was modified. No dependencies were added; no PROVENANCE.md edit was needed.

## Friction (ordered)

1. The supplied OAuth research URL was inaccessible; official-site search found the same
   article under a shorter URL. Research link issue, not an SDK failure.
2. SDK-only `cargo doc --no-deps` does not supply the pagination/fixture API details needed
   to inspect metadata and partial results. Generated additional public crate docs;
   no implementation source was needed.
3. The pagination crate contains both `Items` and `PageItems`; reading the `Pages::items`
   signature disambiguated the ordinary type. No extra type annotation is expected.
4. `RefreshResponse` rustdoc hides all fields; README names replacement/lifetime behavior
   but does not specify the full successful OAuth body. A minimal fixture without
   `token_type` failed as an uncertain refresh. Next action: add the conventional
   `token_type: "bearer"` while still omitting the optional replacement, then rerun.
5. The second operation after an uncertain refresh did not match the predicted
   `RefreshUncertain`. Next action: inspect the redacted error variant and request count
   through the public API, preserving the stronger no-second-refresh assertion.
   Diagnostic result: second get yields `Refresh { status: None, uncertain: true }`,
   losing the initial HTTP 503 status. Keep the first error if its HTTP status matters.
6. A patch against the rustfmt-expanded test text did not match one line; the patch
   tool rejected it without modifying the test. Read only the newly authored test's
   affected lines and reapplied successfully. Editing friction, not SDK assistance.

No unneeded SDK imports, surprising generated names, compiler diagnostics or implementation
source lookups occurred. `Resume` and `NonZeroUsize` were expected from the README; the
additional error/progress types were needed for assertions. Fixture concurrency uses
`Gate`, `poll_fn`, and atomics, all outside the ordinary application path. Explicit
completion and resume APIs became clear after the public pagination docs were read.

## Predictions compared with observations

| Prediction | Observed behavior and evidence | Result |
| --- | --- | --- |
| One initial form refresh, then cached bearer authentication | First workflow made exactly 1 token request, 3 search requests, 1 get. Form values with spaces, `+`, `&`, `=`, `/` round-tripped. OAuth had no bearer header; all expected Accept, body media, method, path and absent-query assertions passed. | Matched |
| Token JSON provides access token, lifetime and optional replacement | Adding `token_type: "bearer"` alone made the no-replacement fixture succeed. Missing token type is now a negative case. Optional replacement absence preserves the existing refresh token. Public `RefreshResponse` fields were opaque. | **Incomplete prediction: token type is required** |
| Clones cannot overwrite a newer replacement | Two rounds each put a search caller and an explicitly polled get caller behind a held OAuth reply. Only one token exchange occurred per round. A 31-second token crossed the documented 30-second margin after 1.1s; the second exchange submitted the first replacement. Both clones retained the second replacement, including after a further old-clone read. Exactly 2 exchanges and 5 CRM calls. | Matched in the documented single-instance/clone scope |
| Definite refresh 400/401/403 vs uncertain failures | 400, 401, 403 returned `Refresh { status: Some(code), uncertain: false }`. 503 returned the same shape with `uncertain: true`. Malformed JSON, absent token type and a 30-second lifetime returned `Refresh { status: None, uncertain: true }`. No CRM request followed failure. | Matched |
| A later operation after uncertain refresh returns `RefreshUncertain` | Subsequent get instead returns `Refresh { status: None, uncertain: true }`; snapshot access returns `RefreshUncertain`. Each fixture received only one token request even after the later get. The initial 503 is absent from the later error. | **Variant/status mismatch; no-repeat safety holds** |
| POST search JSON field spelling, continuation in body, input `after` overridden | `filterGroups`/`propertyName`, query, selected properties and page size were correct. First traversal request omitted `after`; later ones sent exactly `002`, then `004`. Provider link was ignored. | Matched |
| Total is metadata, null properties are allowed, empty pages can continue | Totals changed 2 → 3; the middle page had no items but still led to the final page. Null phone became `None`. Completion depended on no next cursor, not total. | Matched |
| Explicit end and cap semantics | Cap equal to 3 actual pages ended with repeatable `Ok(None)`, `Outcome::Exhausted`, 3 successes, no resume. A one-page cap with more results returned a one-contact incomplete prefix and `Resume::At("002")`; an explicit new traversal completed the tail. | Matched |
| Page failure preserves prefix, error head and failed cursor | A second-page 429 returned one previously collected contact, admitted=2/succeeded=1/failed=1, retry-after/correlation headers and failed cursor `002`. Same-request explicit resumption completed without duplicating the prefix. No retry occurred automatically. | Matched |
| Invalid pages fail rather than falsely complete | Malformed JSON and a 200 provider-error envelope produced Decode; HTML produced Metadata; >1 MiB produced Limit; HTTP 503 produced Status. Error heads retained status and correlation header even after Decode/Limit. Empty/repeated cursors produced Invalid. All cases retained the earlier page, reported Failed, then Ended on another poll, without a third request. | Matched |
| Get by selected/created ID, status errors, no 401 replay | Selected `80101` and created `90201` were retrieved by GET with no query/body/content type. Read 401 and 404 surfaced as Concord Status, one request each, without another refresh. | Matched |
| Create once, then save ID and retrieve | Expected properties JSON was sent once; 201 yielded ID `90201`; a later get returned the same model. No additional OAuth exchange. | Matched |
| Timed-out create remains uncertain | Fixture accepted and recorded one create but held its reply indefinitely. The finite client timeout produced CreateUncertain. Independently known fixture ID `90202` was then readable; request count stayed at one create. This does not supply a production reconciliation ID or exactly-once guarantee. | Matched |
| 409 definite; 408/5xx/bad success/transport uncertain | 409 retained status and correlation header. 408, 503, malformed 201 JSON, 201 HTML and dropped connection all returned CreateUncertain. Every case had exactly one token and one create request. | Matched |
| Public metadata and secret boundaries | Search preserved only `total` as page metadata; successful create/get returned `Contact`. Error heads exposed headers deliberately. SDK/error formatting omitted planted credentials, contact email and provider-body text; Incomplete and HubSpot formatting were also checked. Model Debug was not claimed to redact contact data. | Matched within tested formatting surfaces |

## Changes I would propose (not applied)

- **SDK documentation:** show a complete minimal OAuth response including `token_type`;
  explain that later CRM calls after uncertain refresh return the generic uncertain
  `Refresh` variant while `refresh_token()` uses `RefreshUncertain`, and that callers
  should retain the first error's status. These are provider-wrapper/documentation
  responsibilities, not evidence for a core or macro change.
- **Public pagination documentation/discovery:** link the `HubSpot::search` example
  directly to `PageItems`, incomplete-prefix recovery and page metadata examples.
  The existing ordinary APIs were sufficient after those pages were available.
- **Core/macros:** retain the frozen client → scope → endpoint model. Generated endpoint
  rustdoc accurately predicted the wire shapes; this consumer produced no demonstrated
  need to change a shared crate. No shared edits were made.
- **Application boundary:** durable token rotation persistence, coordination across
  independent installations/processes, and reconciliation of uncertain writes remain
  application work. The tests prove clone coordination and local wire behavior only.

## Evidence limits

No live provider behavior, cross-process locking, crash-safe persistence, exactly-once
creation, longer cursor cycles, marketing automation or other object types were tested.
The article/SDK OAuth route difference is documented above, not resolved through a live
request. The required concurrent-refresh case tests that duplicate refreshes are prevented;
it does not inject two conflicting responses through private cache hooks. Invalid successful
body/media/size handling is exercised on search; create separately exercises malformed
success/media and transport ambiguity. Cancellation and all possible refresh transport
failures were not exhaustively enumerated.

## Assistance label

**docs-only** (final). README examples and generated public rustdoc were the only SDK
implementation guidance. No SDK source, existing test source, FRICTION.md, PROVENANCE.md,
or rustdoc source pages were read at any point. No author coaching or sub-agent help.
The only SDK test file read was this consumer's own newly authored `tests/first_use.rs`,
after the first working call. Official read-only research and fixture public API docs
are listed above. Live provider calls: **NOT_RUN**.
