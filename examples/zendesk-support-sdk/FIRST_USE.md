# SDK28 fresh consumer: Zendesk Support

Date: 2026-11-07. Frozen baseline: `a3764fdf5ee51600c04e952b7781c5ab14d657e1`.
Live provider calls: **NOT_RUN**. All application requests will use loopback fixtures.

## Predictions

Written before application code or a working call, from the package README and
generated public rustdoc (including Concord, pagination and testserver docs).

### Workflow 1: list tickets, retrieve a selected ticket

- Construct `Zendesk::connect(http, "acme", fake_token)`, replace the base with a
  local `/api/v2` URL. Rustdoc says a subdomain needs a domain base plus a transport
  DNS override: predict `http://localhost:<port>/api/v2` becomes
  `acme.localhost:<port>`, resolved explicitly to `TestServer::addr()`.
- Call `api.tickets().list().set_page_size(PageSize::new(2)?).set_sort(...)`,
  then `ticket_pages(list, Resume::Start, max_pages)`. Wire: `GET /api/v2/tickets`,
  query `page[size]=2`, `sort=-updated_at`; later requests additionally have
  `page[after]=<opaque cursor>`, percent encoded without changing its value.
  No offset parameters or request body. Headers: `Authorization: Bearer <token>`,
  `Accept: application/json`; no create idempotency key.
- Direct `list().send()` returns `TicketPage`; traversal returns tickets, cursors
  and unit page metadata. `collect_complete()` returns a whole `Vec<Ticket>` or
  `Incomplete { collected, error }`. `has_more=false` ends traversal even when a
  cursor remains. Empty pages with `has_more=true` continue. A cap is
  `PagesError::Bounded`, preserving prefix and resume cursor; a failed later page
  preserves the earlier prefix and failed cursor. Repeated/missing/inconsistent
  cursors are `Error::Pagination`, never successful completion.
- Choose a held ticket, call `api.ticket(id).show().send()`: `GET
  /api/v2/tickets/{id}`, no query/body, bearer plus JSON accept; returns
  `TicketResponse.ticket`, possibly with updated status.
- A safe read receiving `429` (or `503`) and usable `Retry-After` waits that delay
  before another attempt, default at most three total and at most 60 seconds per
  wait. Exhausted retries return `Error::Http` with attempts, status, parsed delay,
  request ID and response metadata. Missing/invalid/overlong delay and ordinary
  status/transport errors do not trigger blanket retries. Provider error JSON is
  exposed through `Problem`/`ErrorDocument`; HTTP status remains authoritative if
  its body is malformed. Malformed JSON on a successful read is `Error::Concord`
  of kind `Decode`; incorrect media is `Metadata`; oversized body is `Limit`.

### Workflow 2: create a ticket, retrieve its resulting state

- Store an application key first, build `IdempotencyKey::new(...)` and
  `NewTicket::new(comment).subject(...).priority(Priority::High).external_id(...)`.
  Call `api.tickets().create(new, key).send()`: `POST /api/v2/tickets`, no query,
  bearer authorization, JSON accept/content type and `Idempotency-Key`. Body is
  `{"ticket":{"comment":{"body":"..."},"subject":"...","priority":"high","external_id":"..."}}`
  with unset optional properties omitted. Expect `CreatedTicket { ticket,
  idempotency }`, including `x-idempotency-lookup` when supplied. Retrieve its ID
  with `api.ticket(created.ticket.id).show().send()` as above.
- A `4xx`, including `429` and validation `422`, is `Error::Http` after exactly
  one POST, retaining request ID, status, delay, typed provider error and details.
  A server `5xx`, lost response after sending, or unreadable `2xx` is
  `Error::Uncertain` carrying the original key, operation and cause, with no
  automatic second POST. Resolve a simulated committed-but-unacknowledged create
  by listing its external ID and retrieving the matching state; assert exactly
  one ticket was created. The filter is not a uniqueness guarantee: multiple or
  missing matches must remain unresolved rather than cause another create.
- SDK docs also promise deliberate same-key/body replay within two hours. The
  specified official Tickets page read today contains no `Idempotency-Key` text;
  that replay guarantee is **SDK-documented, not independently verified here**.
  This consumer will resolve uncertainty using safe reads without a second POST.

### Inputs and secrets (both workflows)

- Bad page size/key/token is rejected before I/O; invalid subdomain fails at
  request build. Normal client/request/prepared/error formatting must omit the
  bearer token. Provider descriptions/details/text can echo content and should
  appear only through explicitly unredacted accessors; ticket models themselves
  can print ticket content. Use only synthetic tokens and ticket data.
- Ordinary generated `send()` applies `SendPolicy` without an extension trait.
  `prepare()` is needed only to check local construction/redaction and bypasses
  that policy if sent. No native sends will execute these workflows.

## Reading and setup record

- Read package `README.md`, package/workspace manifests and lockfile metadata;
  workspace membership and package lock entry were already present, so no edits
  to them were needed. Initial working tree was clean.
- Generated package docs with `CARGO_BUILD_JOBS=4 cargo doc -p
  concord-sdk-zendesk-support --no-deps --offline` (first command, without
  `--locked`, succeeded); then generated Concord, concord-pagination and
  concord-example-testserver public docs with `--no-deps --locked --offline`.
  Used the default worktree `target/`; no target override or new dependency.
- Read generated public pages for the client/config, list/create endpoints,
  models, errors, policy, pagination helper; Concord overview/metadata/operation/
  error kinds; pagination Pages/PageItems/Incomplete/Progress/Outcome/Page;
  testserver TestServer/Recorded/Reply and overview. Source links were not followed.
- Read applicable ancestor `AGENTS.md` and its mission entry documents and
  resource/phase contracts. This capsule follows the user's narrower ownership.
- Tooling incident: BeautifulSoup was unavailable (`ModuleNotFoundError: bs4`);
  public HTML was read with Python's standard library instead. No dependency added.
- Resource snapshot before test compilation: filesystem 199 GiB available,
  `target/` 1.3 GiB, memory 22 GiB available, 20 logical CPUs. One cargo process
  at a time, four build jobs; existing artifacts are reused, not a clean build.

Official read-only research, read **2026-11-07**:

- [Tickets: List Tickets, Show Ticket, Create Ticket](https://developer.zendesk.com/api-reference/ticketing/tickets/tickets/).
  Confirms cursor list/sort/external-ID filter, show route, comment envelope;
  external IDs need not be unique. Does not establish the SDK's replay promise.
- [Pagination](https://developer.zendesk.com/api-reference/introduction/pagination/).
  Confirms cursor opt-in via `page[size]`, cursor continuation and `has_more` end.
- [Best practices for avoiding rate limiting](https://developer.zendesk.com/documentation/api-basics/best-practices/best-practices-for-avoiding-rate-limiting/).
  Confirms a `429` can carry `Retry-After` and calls should wait that interval.

## First working call

`cargo test -p concord-sdk-zendesk-support --test first_use --locked --offline`
passed (1 test) after one failed runtime attempt and the origin-only base fix.
The successful caller traversed two cursor pages, selected held ticket 4201 and
retrieved its updated open state. It asserted bearer/accept, method, path, query,
body absence, host, opaque-cursor round trip, exhaustion and three total requests.
It used generated `send()` and `ticket_pages` directly, no extension traits,
native sends or source inspection. Initial compilation had no diagnostics.

## Friction incidents (in observation order)

1. Local configuration is absent from the package README example. Generated
   `with_base_url` rustdoc supplies the necessary domain/DNS-override guidance;
   using a numeric loopback base would not fit this subdomain declaration.
2. Generated `send()` rustdoc describes its result as
   `<SendPolicy as SendWith<T>>::Output`; the README supplies the concrete
   `Result<T, Error>` behavior. Execution confirmed no trait import was necessary.
3. README replay advice has no corroborating idempotency contract on the three
   specified official pages read today. Chose read reconciliation for uncertainty.
4. First test compiled without extra traits but failed on the wire: supplying
   `/api/v2` in `with_base_url` produced `/api/v2/api/v2/tickets`. The declaration's
   path is appended, as the generic rustdoc says; the README's full production
   URL encouraged an incorrect replacement-base prediction. Corrected the caller
   to supply only `http://localhost:<port>`. No source lookup was needed.
5. That first failure's outer pagination diagnostic printed
   `Provider { failed: Start, progress: ..., .. }` without the underlying SDK
   cause. Public `provider_error()`/`into_provider_error()` accessors provide it;
   later assertions use `provider_error()` to inspect a failed page's status and
   metadata. No compiler diagnostic or generated-name surprise occurred.
6. Ordinary success results expose no general response head: `TicketPage` and
   `TicketResponse` contain models, `ticket_pages` projects page metadata to `()`,
   and `CreatedTicket` retains only the specific idempotency lookup. Error metadata
   is accessible and tested. A caller wanting success request IDs or rate-limit
   headers would need a different public path. This is a documented return-type
   limitation, not a failed assertion or a reason to bypass the policy here.

No unneeded imports/types or SDK source lookups were required. The caller's
transport builder exists to supply the domain-to-loopback mapping while disabling
transport retries, redirects and proxies; the generated hierarchy and terminal
`send()` remained the ordinary path throughout.

## Prediction comparison

| Prediction | Observed in independent caller tests |
|---|---|
| Local base `/api/v2` replaces the production API base | **Mismatch:** the declared path is appended; origin-only base is required. Domain substitution and explicit loopback resolution worked as predicted. |
| List/show method, paths, bearer/accept, no read body or key | Matched; exact requests asserted, including no offset query parameters. |
| Cursor, page size and descending sort survive navigation | Matched; opaque cursor containing `=`, `/`, `+` and a space round-trips unchanged. Default page size 100 also asserted during reconciliation. |
| Pagination ends on `has_more=false`, not page length/cursor absence | Matched; final fixture retains an unused cursor; a short page and an empty intermediate page both continue. Three logical pages, four list HTTP attempts after throttling, then one show. |
| Cap/failure retain prefix, resume position and explicit failure | Matched; cap preserves ticket 4201 and `Bounded`/cursor; later 403 preserves ticket 4201 and typed provider error/failed cursor. Repeated cursor yields `Pagination`, then `Ended`, with final outcome `Failed`. |
| A selected ticket's current state can differ from listing | Matched; held ticket 4201 is open on show. Successful traversal reports `Exhausted`, three admitted/succeeded pages, zero failures. |
| Safe reads wait Retry-After and stop at a budget | Matched; server-observed interval before the first retry is at least one real second, identical request target, then successful workflow. Repeated 429 stops at three attempts. Missing/invalid/61-second delays stop after one; a one-second wait outside a 100-ms overall deadline is not started. |
| HTTP errors preserve authoritative status and metadata | Matched; 403/404/429 and write 422 retain status, attempts, request ID, operation and provider error. Malformed 404 JSON retains HTTP status plus a nested `Decode` failure. |
| Malformed successful read JSON/media is Concord failure | Matched: `Decode` and `Metadata`, respectively; exactly one request each. |
| Create request uses JSON ticket/comment envelope and persisted key | Matched; exact JSON, headers, POST path and query absence asserted; unset fields omitted. 201 returns ticket 4301 and `IdempotencyLookup::Miss` from the fixture head. Show returns its pending state with matching external ID/comment. |
| Uncertain create is surfaced once and reconciled without duplicates | Matched independently for lost response, 503 (even with Retry-After), and malformed 201 JSON. Each carries operation/key/cause; fixture has one committed ticket. External-ID list and show recover ticket 4302; total sequence is exactly POST, GET, GET. |
| Write 422/429 has no automatic retry | Matched; one request each, typed provider document/details, delay and request ID remain available; no uncertainty classification. |
| Invalid size/key/token/subdomain fails before I/O | Matched; invalid subdomain fails when sent with `Build`, other validators refuse locally; fixture records zero requests. |
| Secrets/provider echoes stay out of formatting | Matched for client, generated request, prepared request, credential errors, HTTP/uncertain errors and response metadata. Explicit unredacted accessors retain echoed description/details. |
| Ordinary generated sends require no extra trait/advanced execution API | Matched; `prepare()` used only without I/O to assert redacted formatting, never to send. |

Bounds of this evidence: the new caller does not separately exercise HTTP-date
Retry-After, safe-read 503, plain-text/null error documents, over-limit bodies,
missing/mismatched pagination cursors, never-connected writes, request timeout,
or ticket Debug content. Those remain documentation predictions, not new
independent observations. Deliberate same-key replay, its two-hour window and
live provider deduplication are **not independently verified**. No assertion of
provider uniqueness is made for `external_id`; the fixture supplies one match,
and the caller requires exactly one before retrieving it. General success
metadata loss matched the public signatures but limited what could be asserted.

## Checks and ownership

- First caller run: compiled, then failed (exit 101) on duplicated `/api/v2`.
- Origin-only correction: same command passed, 1/1 first-use test.
- Expanded `--test first_use` run: **2/2 passed**, 1.11 seconds.
- Formatted only the new caller with `rustfmt --edition 2024`.
- Final `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-zendesk-support --locked
  --offline`: **28 integration tests passed** (2 first-use, 19 existing protocol,
  7 existing workflows), **1 compile doctest passed**, zero failures. Existing
  suites were executed without inspecting their source; their success is
  regression evidence, not additional independent fresh-consumer observations.
- Both documentation builds succeeded. All cargo invocations were sequential,
  with four build jobs and the default worktree target; no new crate or lockfile
  change. End snapshot: target 1.5 GiB, filesystem 198 GiB available.
- Only `FIRST_USE.md` and `tests/first_use.rs` were added. SDK source, shared
  crates/checks/scripts/other examples and workspace/lock metadata are unchanged.
  No commit was attempted. Live provider calls remain **NOT_RUN**.

Would-change record (not implemented under the freeze): SDK docs should show an
origin-only loopback setup and substantiate the replay contract with an exact
official reference; generated endpoint docs should make the policy's concrete
output easier to discover; pagination caller docs should show extracting the
provider error while preserving the partial prefix. If success rate headers are
needed, first evaluate an SDK projection through existing Concord metadata
mechanisms; this capsule supplies no evidence requiring a new core abstraction.
The fixed client → scope → endpoint hierarchy was sufficient for both tasks.

## Assistance label

**docs-only**. No SDK source, pre-existing tests, `FRICTION.md`,
`PROVENANCE.md` or existing fixtures were inspected, before or after the first
working call. Files read were public Markdown/rustdoc, manifest/lock/instruction
metadata, and this consumer's own newly written files. Compilers necessarily
compiled SDK source and existing tests; that was not consumer source assistance.
