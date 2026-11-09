# SDK34 QuickBooks Online — fresh consumer

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All token and Accounting API
traffic in this capsule must use `concord-example-testserver` loopback fixtures.

## Predictions

Written before application code or any working provider call. Evidence is the SDK
README and generated public rustdoc, not its implementation or existing tests.

### 1. Query customers, then fetch a selected customer in the same realm

- Configure `OAuthApp::new(Environment::Sandbox, ...)`, override its token base
  with the fixture URL, parse `AuthorizationCallback::from_query`, and call
  `exchange_code(&http, callback, expected_state)`. This sends one
  `POST /oauth2/v1/tokens/bearer`, Basic client credentials,
  `Accept: application/json`, `Content-Type: application/x-www-form-urlencoded`,
  and `grant_type=authorization_code`, `code`, `redirect_uri` in the form. The
  callback's `realmId` binds the returned `Connection`; the code exchange does
  not send that realm as an extra form parameter. State mismatch fails unsent.
- `QuickBooks::connect(http, connection.clone()).with_base_url(fixture)` retains
  the realm. `customers().query(query).send()` sends
  `GET /v3/company/9341456292345678/query?minorversion=75&query=...`, with
  `Authorization: Bearer <realm access token>`, `Accept: application/json`, no
  body. Query is URL encoded SQL from `CustomerQuery`: active customers,
  `ORDERBY DisplayName STARTPOSITION 1 MAXRESULTS 2`. Return is
  `CustomerQueryResponse { page, time }`, with typed customer IDs, exact decimal
  balances, optional entity metadata, and optional pagination metadata.
- Select an ID from that response and call `customer(id).read().send()`:
  `GET /v3/company/9341456292345678/customer/{id}?minorversion=75`, the same
  bearer credential and JSON Accept header, returning `CustomerResponse`.
- `customer_pages(&qbo, query)` advances STARTPOSITION by page size. A full page
  requires another request, including an empty terminal page when applicable;
  a short page completes traversal. A cap or later error must preserve already
  received customers and distinguish partial traversal from completion.
- Non-2xx errors return SDK `Error::Provider` with status, parsed Fault codes,
  elements, and `intuit_tid` (even malformed faults retain status). A 200 Fault
  is also a provider error. Malformed successful reads are `Error::Concord`
  (Decode; wrong media is Metadata). A read 401 refreshes and retries once.
- Resume via `app.connection(realm, RefreshToken)` refreshes on first use with
  the same token path, Basic/JSON headers, form `grant_type=refresh_token` and
  `refresh_token`. A replaced/expired token rejected as `400 invalid_grant`
  returns `AuthorizationKind::GrantRejected`, the affected realm and
  `needs_reauthorization() == true`, without sending an Accounting request.
  Repeated calls remain local failures until recovery. Another realm stays
  usable with its own token. A callback for that other realm fails unsent;
  replacing with a newer same-realm refresh token recovers existing clones.

### 2. Create a sandbox invoice for a customer, then retrieve it

- Start from a realm-bound sandbox connection and fetch the customer before
  billing. Build `NewInvoice::new(customer.id, SalesItemLine::new(...))`, using
  decimal cents and a real-shaped item ID. Optional description, quantity,
  unit price, and DocNumber are built through public setters.
- `invoices().create(invoice).set_request_id(id).send()` sends exactly one
  `POST /v3/company/9341456292345678/invoice?minorversion=75&requestid=...`,
  `Authorization: Bearer <same realm token>`, JSON Accept and Content-Type,
  and JSON containing CustomerRef.value, Line[].Amount, DetailType,
  SalesItemLineDetail.ItemRef, plus selected optional fields. A generated
  request ID is available by default; a fixed ID makes fixture assertions exact.
- Return is `InvoiceResponse { invoice, time }`; use the returned invoice ID
  in `invoice(id).read().send()` for
  `GET /v3/company/9341456292345678/invoice/{id}?minorversion=75`. Verify
  customer ID, amounts, document number, SyncToken, and entity timestamps.
- Validation failures are `Error::InvalidInput` before dispatch. A 400 Fault
  or 200 Fault is `Error::Provider`. A 401 is recoverable
  `AccessTokenRejected` and is not replayed. A 5xx or malformed successful
  create is `Error::Uncertain` with realm, operation, and the sent request ID;
  it is never retried automatically. A malformed retrieved invoice is a
  Concord read error, while the already created invoice remains available to
  the caller. No automatic invoice replay is part of this application.

### Secrets and metadata (both workflows)

Fake fixture credentials occur only on their intended token/API requests.
SDK Debug/Display errors withhold secrets, authorization codes, query text,
provider messages, bodies, and URLs. Explicit `*_unredacted` access is opt-in.
Ordinary successful send exposes provider `time` and entity `MetaData`; rustdoc
does not expose successful response headers on these response structs.

## Reading and setup record

- Read SDK `README.md`, generated rustdoc (public type, method, and model
  documentation), root workspace manifest, package manifests, and applicable
  campaign instructions. No SDK `src/`, existing `tests/`, `FRICTION.md`, or
  `PROVENANCE.md` read.
- Initial worktree was clean, HEAD `4cefd333b0a0fb35ad62dc1e30412c510ff3ba20`;
  shared frozen baseline supplied by the task is
  `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. The package was already a
  workspace member; no root manifest edit was needed.
- First build: `CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-quickbooks-online
  --no-deps --offline` succeeded. Subsequent Cargo commands use `--locked`,
  `--offline`, jobs 4, and the worktree's default `target/`; no target override.
  The existing lock already contains this package and its dependencies.
- Resource check: `df -h .` reported 430 GiB available; `free -h` reported
  24 GiB available RAM. One Cargo command at a time; cached documentation build.
  `du -sh target` was 1.4 GiB before the expanded checks and 1.6 GiB afterward;
  `nproc` reported 20. No cleanup was needed.
- Generated public docs for the fixture server and pagination helper to avoid
  source discovery. Cloud runtime skill was read; its environment-status tool
  is unavailable in this session. Local Cargo tooling is available.

## Official documentation research

Read on **2026-11-07**:

- [OAuth 2.0](https://static.developer.intuit.com/output_html/qbo/docs/develop/authentication-and-authorization/oauth-2.0.html):
  realm-bound authorization, single authorization-code exchange, form/Basic token
  requests, refresh rotation, and reauthorization after refresh-token expiry.
- [Customer](https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/customer)
  and [Invoice](https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/invoice):
  opened the exact supplied URLs, but the research tool received only
  “Compiling and pre-filling your Intuit info...”. Entity fixture expectations
  therefore come from public SDK docs and are **synthetic, not independently
  verified against the entity reference**. This limitation is retained in the
  evidence label, not replaced with claims of live verification.
  Later attempts to open the corresponding static `customer.html` and
  `invoice.html` reference paths were also inaccessible. No entity-reference
  content was obtained from those paths.

## First working call

`CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-quickbooks-online --test first_use
--locked --offline` compiled on the first attempt and passed both caller tests
(2 passed, 0 failed). On the customer path, the first successful request was the
local authorization-code exchange, followed by the first Accounting call
`qbo.customers().query(query()).send().await`. It returned Amy's Bird Sanctuary
(ID 58) in realm 9341456292345678; the subsequent customer read used the same
realm and bearer token. The invoice path read customer 58, created invoice 145
with request ID `billing-2026-0042`, then fetched that returned ID.

What it took: the README's ordinary `send()` calls, public rustdoc discovery of
`OAuthApp::with_token_base_url` and `QuickBooks::with_base_url`, typed input
constructors, fixture/pagination public rustdoc, and dependencies already present
in the package. No added dependencies, imports of execution traits, generated
endpoint types, `prepare()`, SDK source lookups, or compiler fixes were needed.
This is the preserved first result. Failure/partial-result cases were added
afterward and also passed on their first compile/run.

## Friction incidents (in order)

1. Official Customer/Invoice reference URLs returned only a JavaScript bootstrap
   placeholder to the research tool. Kept the fixture provenance explicitly
   synthetic rather than claiming their entity contract was independently read.
2. The README demonstrates live-shaped configuration but does not show the
   token fixture override. Public `OAuthApp` rustdoc supplied
   `with_token_base_url`; `QuickBooks` rustdoc supplied `with_base_url`.
3. The README says a short page ends traversal and mentions a cap, but does not
   show how a caller distinguishes a cap/error from completion or preserves
   partial results. `customer_pages` linked to `concord_pagination::Pages`;
   generated pagination docs supplied `next_page`, `collect_complete`,
   `PagesError`, `finish`, and `Outcome` before any pagination code was written.
4. A documentation extraction command guessed a page named `struct.Report.html`,
   which did not exist. The actual public names, linked from `Pages`, are
   `SequentialReport` and `Outcome`. This was a documentation lookup mistake;
   there was no Rust diagnostic and no need for an SDK implementation lookup.
5. After the first working call, the Concord facade's local rustdoc was absent
   when looking up `ErrorKind`/error metadata. Generated it with
   `CARGO_BUILD_JOBS=4 cargo doc -p concord --no-deps --offline --locked` and
   read its public `Error`, `ResponseMeta`, and `Operation` docs. No implementation
   or existing test source was opened.
6. During comparison, the generated `QuickBooksInvoicesCreate` rustdoc's
   “Request” URL shows the declaration's production default, although
   `QuickBooks::connect` selects the Sandbox base for this application. The
   `connect` documentation explains environment selection; both endpoint bases
   were overridden to loopback in the tests. This is a documentation clarity
   issue, not an observed wrong-environment request.

No unnecessary caller import/type, surprising generated endpoint name,
misleading compiler diagnostic, source lookup, or first-call compile failure
occurred. The extra `concord::ErrorKind` and pagination imports are used only
where these tasks need error classification and completion/partial-result
assertions. Neither connected happy path needs a Concord execution trait.

## Predictions compared with observations

| Prediction | Observation |
|---|---|
| Authorization code is exchanged once using Basic credentials, JSON Accept, and the specified form; callback realm binds the connection. | Matched. The first successful customer workflow had exactly three requests: token exchange, query, customer read. Exact form fields were asserted, without an extra realm field. A state mismatch sent nothing. |
| Customer query and selected customer read use the same realm and bearer token, GET with no body, minorversion 75, and the builder's SQL. | Matched. Exact paths, decoded query statement, headers, and ID 58 were asserted. Both workflow token and Accounting hosts were loopback. |
| Typed query/customer results expose page metadata, provider time, SyncToken, decimal balances, and entity timestamps. | Matched for supplied fields. The fixture's serialized balance is `125.5`, and the SDK preserves that wire spelling. The raw query response retains `time`; `customer_pages` has metadata type `()` and does not expose that timestamp per page. |
| A full page requires another query; a short/empty page completes; caps and errors preserve partial results. | Matched. A full page of two customers at position 1 was followed by an empty page at position 3. Repeated `Ok(None)` and `Outcome::Exhausted` caused no further requests. A one-page cap preserved both customers with `Bounded` and resume position 3, without dispatching page 2. A late 429 preserved both in `Incomplete`, the typed provider error, counters, and resume position. A late malformed body returned Decode; another call was `Ended`, not successful completion. |
| Replaced/expired refresh token becomes a recoverable, realm-specific authorization failure; it cannot silently use another realm. | Matched. Synthetic `400 invalid_grant` returned `GrantRejected`, realm A, status 400, and reauthorization required. The next call through an existing clone sent neither token nor API request. Realm B refreshed and queried independently. Wrong-realm reauthorization failed unsent. Replacing A's refresh token recovered both the clone and original client; B's rotated token stayed unchanged. All six requests and each token/path pairing were asserted. |
| Read 401 refreshes and retries once; a second 401 returns `AccessTokenRejected`. | Matched for both eventual success and repeated rejection. Each scenario made exactly four requests: code exchange, read, refresh, read. The rotated refresh token was used and persisted. |
| Invoice creation sends a JSON sales-item invoice, customer reference, document number, minorversion and stable request ID, then reads the returned invoice ID. | Matched. The application builds the invoice from the fetched customer's ID. Asserted POST/GET paths, JSON media, request ID, customer/item references, line amount, quantity, and detail type. Created/retrieved invoice 145 retained customer 58, document number, SyncToken, total, provider time, and entity creation time. |
| Faults retain provider status/code/element and `intuit_tid`; malformed successful reads produce Concord errors. | Matched. Exercised 400 and 200 Faults, a 429 unreadable error document (status retained), malformed 200 bodies (Decode), and wrong-media 200 (Metadata). Error response metadata retained operation, status, and trace header. |
| Invoice creates never replay automatically; 5xx, malformed 2xx, and no response have uncertain outcomes with the sent request ID. | Matched. For 400 Fault, 200 Fault, 500 Fault, malformed 200, 401, and a dropped response, exactly one create reached the fixture. Uncertain errors retained realm, `QuickBooks.invoices.create`, request ID, and the specific cause. A create 401 returned `AccessTokenRejected` without refreshing or resending that create. No invoice replay is implemented by this caller. |
| A failed retrieval must not erase the invoice already created. | Matched. A malformed GET after successful creation returned Decode with read operation/response metadata; the caller still retained invoice 145, its customer reference and provider timestamp. Only one POST occurred. |
| Invalid IDs, query size, request ID and DocNumber fail before dispatch; default diagnostics withhold sensitive content. | Matched for the exercised constructors and callbacks. Validation left the wire count unchanged. Error Debug/Display and source-chain formatting withheld fake tokens, credentials, codes, provider messages/bodies, customer text, and local URLs. Explicit unredacted Fault access returned the expected private marker. Connection/client/refresh-token/query Debug and response-metadata Debug withheld their tested secret values. |

**Behavioral prediction mismatches: none in the exercised cases.** Successful
response headers are not part of the ordinary typed response, as public model
rustdoc predicted; failure metadata remains accessible. The generated production
URL label and unavailable entity documentation are clarity/evidence limitations,
not claims that the fixture proves live behavior. This capsule does not add
Payroll, tax, full SQL, or invoice replay.

## Verification and ownership

- Initial happy-path command: `CARGO_BUILD_JOBS=4 cargo test -p
  concord-sdk-quickbooks-online --test first_use --locked --offline` — 2 passed,
  0 failed, no compiler edits. The expanded failure scenarios passed under the
  same command on their first attempt.
- Final command: `CARGO_BUILD_JOBS=4 cargo test -p
  concord-sdk-quickbooks-online --locked --offline` — 1 unit test, 2 new caller
  tests, 21 existing protocol tests, 8 existing workflow tests, and 2 doctests
  passed: **34 passed, 0 failed**. Existing test sources were not read; they
  were executed only after the first working call. The two registered new tests
  cover the two happy workflows plus 19 fixture scenarios across pagination,
  recovery/isolation, read failures, and create/retrieval failures.
- `rustfmt --edition 2024 --check
  examples/quickbooks-online-sdk/tests/first_use.rs` and `git diff --check`
  passed. Toolchain: rustc/Cargo 1.99.1, rustfmt 1.10.0-stable.
- `cmp Cargo.lock /tmp/sdk34-first-use-Cargo.lock.before` succeeded: **no
  lockfile changes and no new crates**. The existing package dependencies cover
  the caller and fixtures; a new dependency provenance entry was unnecessary.
- `git diff --exit-code 14b9e1e0a89775dbe5cbb9f7837bde677ea5878e -- crates
  checks scripts` succeeded. SDK `src/` was not edited. Only this record and
  `tests/first_use.rs` were added. No commit was attempted.
- Live provider calls: **NOT_RUN**. All new test servers were explicitly shut
  down, with `open_after == 0` asserted. No real OAuth exchange or Accounting
  request occurred.

## Changes I would propose (not applied)

- **SDK documentation:** add a complete loopback configuration example covering
  both base overrides, plus a short pagination example showing complete,
  bounded, and failed results without losing the received prefix.
- **Macro-generated public docs:** describe the shown URL as the declared
  default, with a note that client construction/base overrides determine the
  actual origin. Preserve the client → scope → endpoint hierarchy and ordinary
  `send()` behavior.
- **SDK metadata surface:** if consumers need per-page provider timestamps,
  consider retaining `CustomerQueryResponse.time` in `customer_pages` metadata.
  Its current public metadata type is `()`; changing that belongs to the SDK's
  API design, not a shared-core fix inferred from this test.

No shared behavior change is justified by these passing fixture results, and no
shared file was changed.

## Assistance label

**docs-only**. The application was authored from the SDK README and generated
public SDK, fixture-server, pagination, and Concord rustdoc. Public manifests
were read for package/dependency setup. No SDK implementation, existing test
source, `FRICTION.md`, or `PROVENANCE.md` was read at any point. Rustdoc source
links were not followed. The official OAuth page was readable; the entity
reference limitation is documented above. No author coaching or sub-agents were
used.
