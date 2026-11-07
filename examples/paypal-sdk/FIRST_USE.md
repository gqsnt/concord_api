# SDK32 PayPal — fresh consumer

## Predictions

Written on 2026-11-07 before application code or any SDK call. Evidence so far:
`README.md`, package manifest, generated public rustdoc (not its Source links),
Concord's root README, and the public rustdoc of the local test server and retry
policy. No PayPal SDK source, existing tests, FRICTION.md or PROVENANCE.md read.

### Workflow 1: client credentials → create → current state

Construct `PayPal::builder(http_client()?, Credentials::new(..)?)`, choose the
loopback server with `base_url(..)`, then call
`orders().create(OrderRequest::capture(vec![PurchaseUnitRequest::new(Money::new("USD", "100.00")?)?.reference_id("cart-2026-117")?])?)`
with `set_request_id(RequestId::new("create-cart-2026-117")?)` and `send().await`.
Use the returned typed `OrderId` in `order(id).get().send().await`.

| Call | Predicted wire | Predicted result |
| --- | --- | --- |
| Acquire token | POST `/v1/oauth2/token`, no query, Basic client ID/secret, Accept JSON, Content-Type `application/x-www-form-urlencoded`, body `grant_type=client_credentials` | Internal Bearer token, `expires_in` lifetime; no refresh token needed |
| Create | POST `/v2/checkout/orders`, no query, Bearer token, Accept and Content-Type JSON, `Prefer: return=representation`, chosen `PayPal-Request-Id`; JSON CAPTURE intent, purchase unit reference and USD 100.00 amount | `Order`, CREATED, order ID and approval URL |
| Current state | GET `/v2/checkout/orders/{id}`, no query/body, Bearer token, Accept JSON, no request ID | `Order`, fixture advances to APPROVED; no captures yet |

One token should serve the create and subsequent reads, including through a
clone. With a fake monotonic token clock, a 120-second token and 60-second margin,
the token should renew at elapsed 60 seconds using **client_credentials again**.
An API 401 should invalidate the token and permit one read replay with a newly
acquired token. A token-service refusal should be `Error::Token` and send no API
request; a token without a usable lifetime should be refused.

Invalid IDs/money/units fail locally (`Error::Invalid`). Provider 4xx becomes
`Error::Http`, retaining status, response metadata, debug ID and typed issues.
429 is returned once; there is no fixed quota and no automatic wait/retry, even
if the fixture supplies Retry-After. Malformed error JSON still preserves status
and metadata, with `document()` exposing the decode error. Malformed 2xx JSON on
a read becomes `Error::Concord` (Decode); wrong media type becomes Metadata.
Successful ordinary `send()` returns the domain object, with no response-head
wrapper. No pagination or continuation exists in this slice: one decoded object
finishes each call. The caller retains a created order if its later read fails.

### Workflow 2: approved order → capture → captured payment

Read the approved order, call
`order(id).capture(CaptureRequest::new()).set_request_id(RequestId::new("capture-cart-2026-117")?).send().await`,
iterate the returned order's `captures()`, then call
`capture(capture.id.clone()).get().send().await` for accounting details.

| Call | Predicted wire | Predicted result |
| --- | --- | --- |
| Capture | POST `/v2/checkout/orders/{id}/capture`, no query, Bearer token, Accept and Content-Type JSON, `Prefer: return=representation`, capture-specific request ID, body `{}` | `Order` COMPLETED with embedded captures; each capture's status must be checked |
| Capture detail | GET `/v2/payments/captures/{capture_id}`, no query/body, Bearer token, Accept JSON, no request ID | `Capture`, amount, status, final_capture and related order ID when supplied |

Capture with an ID may replay after 5xx or a lost response, retaining exactly the
same ID and body; a stored replay may return 200. Capture **without** an ID must
make one attempt only and return `Error::Uncertain` after 5xx/lost response. The
application then reads the order to reconcile, never blindly recaptures. A
malformed successful capture body is Uncertain/UnreadableSuccess, even with an
ID. An unapproved order is a non-retried 422 with ORDER_NOT_APPROVED. A later
refusal after an uncertain attempt must preserve uncertainty (LaterRefusal).

If one capture-detail read fails, already obtained capture details and the
captured order remain available in caller-owned collections. An order marked
COMPLETED can contain a PENDING capture; order completion alone is not payment
completion. Iterating captures ends at the embedded list's end, without network
pagination or following HATEOAS links.

For both workflows: credentials/client/builder/session and Error Debug/Display
must exclude client ID, secret, Bearer token, bodies and provider text that can
echo secrets. Safe error names/issues/debug IDs remain visible. Unredacted
accessors are explicit. All credentials and identifiers used below are fixtures.

## Official read-only research

Read **2026-11-07**; these are documentation fetches only. Live provider calls:
**NOT_RUN**. No approval UI, refund, payout or other provider operation is run.

- [Authentication](https://developer.paypal.com/api/rest/authentication): Basic client credentials, token lifetime and obtaining another token on expiry.
- [API requests](https://developer.paypal.com/api/rest/requests/): Bearer/JSON headers; verify request-ID support per operation.
- [Rate limiting guidelines](https://developer.paypal.com/api/rest/reference/rate-limiting/): no published quota; cache tokens and avoid polling.
- [Orders integration](https://developer.paypal.com/api/rest/integration/orders-api): create, buyer approval, capture, and show order.
- [Create order](https://developer.paypal.com/api/orders/v2/orders-create) and [Capture payment for order](https://developer.paypal.com/api/orders/v2/orders-capture): endpoint-specific request-ID support and representation preference.
- [Show captured payment details](https://developer.paypal.com/api/payments/v2/captures-get): capture retrieval and related order ID.

## Setup observations

- Starting worktree HEAD: `95a245e64727562d7fc438db806a0e5838e6bdb7`.
  Shared frozen baseline requested: `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`.
- Workspace membership and the SDK lock entry already existed. Initial unlocked
  build: `CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-paypal --no-deps --offline`
  passed, changed no lock entries. Subsequent cargo commands use `--locked`.
- Default worktree `target/` only; CARGO_TARGET_DIR never set. Initial target
  1.4 GiB; filesystem 430 GiB available, memory 25 GiB available, 20 logical CPUs.
  No other cargo/rustc observed; one cargo command at a time, four build jobs.
- Cloud runtime skill read; no environment_status tool was available.
- Local documentation extraction initially tried unavailable BeautifulSoup;
  switched to Python standard-library HTMLParser without installing anything.

## First working call and results

First working call: the first targeted test command passed on its first compile
and run (1 test, 0 failures, 2026-11-07). Its first `send()` obtained a token and
created order `5O190127TN364715T` with USD 100.00 and request ID
`create-cart-2026-117`. It needed the README construction recipe, the documented
loopback `base_url`, rustdoc signatures for typed inputs and the test server,
and an endpoint-specific `set_request_id`. No generated request type, config
type, explicit SendWith trait import or native request API was needed.

Command: `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-paypal --test first_use client_credentials_create_order_and_obtain_current_state --offline --locked -- --exact`.
The test also passed cache/clone sharing, renewal at the 60-second margin,
401 read recovery, 429 without retry, malformed successful/error documents,
wrong media type, local validation, token refusal/recovery and unusable lifetime.
The first compile emitted unused-import/dead-code warnings for capture helpers
staged for workflow 2; those are application staging warnings, not SDK-required
ceremony. No compile errors or failed SDK assertions occurred.

Assistance label: **docs-only**. No SDK source or pre-existing tests, FRICTION.md
or PROVENANCE.md have been read, including after the first working call.

The second caller task also passed on its first compile and run. Both tasks live
in `tests/first_use.rs` and use `TestServer::start(|request: &Recorded| -> Reply)`
through a small ordered-response fixture helper. Every client is constructed
with the server's loopback URL; response links are data and are never followed.
The capture task reads an approved two-unit USD 100.00 order, receives a 500 on
its ID-bearing capture, then obtains the stored result on the permitted replay.
It reads capture `3C679366HH908993F` for USD 60.00 and keeps that result when the
USD 40.00 capture's detail request fails. The embedded second capture is PENDING,
so the application does not equate a COMPLETED order with fully completed funds.

## Friction incidents, in order

1. Setup navigation: the handoff instructions named `PHASES` and `RESOURCES.md`
   at their root; those paths did not exist. Read `contracts/PHASES.md` and
   `contracts/RESOURCES.md` instead. An initial chained file-discovery command
   also stopped when `rg` found no nested AGENTS.md; reran the intended public
   README/manifest reads. These were setup issues, not SDK diagnostics.
2. Documentation extraction: BeautifulSoup was unavailable (`ModuleNotFoundError`).
   Used standard-library HTMLParser. Rustdoc's embedded script data initially
   polluted one text extraction; excluded script tags. No dependency installed.
3. Generated public rustdoc lists low-level `new`, `PayPalConfig`,
   `with_base_url`, `prepare`, `SendWith` and their generic explanations alongside
   the SDK construction recipe. In particular `with_base_url` advertises a local
   test-server example although this SDK's session rejects moving to another
   origin. The README and builder docs explain the correct `builder.base_url`
   path, so this caused no failed request. No surprising endpoint/setter names
   or required generated type annotations were encountered.
4. Fixture and diagnostic assertions needed additional **public** documentation:
   generated testserver docs for lifecycle/wire inspection, concord-retry docs
   for a two-attempt zero-delay policy, and Concord Error/ResponseMeta docs for
   error heads after failed decoding. The application imports ErrorKind and
   Policy for those assertions and deterministic failure exercises; ordinary
   successful calls require neither. No Source links or implementation files
   were opened. No SDK source lookup was necessary.
5. First compile: six warnings reported staged capture-only imports, constants
   and helpers as unused while only workflow 1 existed. Completing workflow 2
   consumed them; final compilation has no warnings. There were no compiler
   errors, misleading compiler diagnostics, runtime assertion failures or
   unneeded SDK-mandated imports.
6. Response metadata: as predicted, ordinary `send()` gives Order/Capture rather
   than a successful-response metadata wrapper. Error metadata is directly
   accessible, including after malformed bodies. Reading successful HTTP headers
   would require another public design choice; `prepare().receive()` bypasses
   this SDK's token/replay policy. The requested tasks did not need that escape.

## Predictions compared with observations

| Prediction | Observed evidence from the fresh caller tests | Outcome |
| --- | --- | --- |
| Token POST uses Basic, form client_credentials, JSON Accept, no query or request ID | Exact wire assertions on all acquisitions in workflow 1; all token replies omit refresh_token | Matched |
| Token cached across calls/clones, replaced at expires_in minus margin | No extra acquisition at fake second 59; a new client_credentials POST at second 60; a 401 read causes a third acquisition and one replay | Matched |
| Failed acquisition sends no API request and is not cached; too-short lifetime refused | Token 401/invalid_client returns Token::Refused after one wire call; next logical read reacquires and works; lifetime equal to margin returns Unusable with no API request | Matched |
| Create request wire and typed CREATED order/approval URL | Exact JSON, method, target, headers, amount/reference/invoice and request ID asserted; returned ID/CREATED/approval URL asserted | Matched |
| Show current state returns APPROVED, uses same token, no write request ID/body/query | Exact GET assertions; no embedded captures before capture | Matched |
| Invalid inputs fail locally | Invalid order ID, currency and empty units rejected, request count unchanged | Matched |
| 429 once, caller inspects metadata and any Retry-After | One 429 attempt despite two-attempt read bounds; duration 7 s, status, operation, provider debug ID, header debug ID and issue retained | Matched; no fixed quota inferred |
| Malformed read/error JSON and wrong success media type remain distinct | Successful bad JSON is Concord/Decode with 200 head; bad 404 JSON remains Http with Decode document failure and headers; text/html success is Metadata | Matched |
| Capture uses capture-specific ID, JSON {}, representation preference and stable replay | Approved order read, 500 followed by stored 200; both POST attempts have identical request ID/body; GET capture details has no request ID | Matched |
| No-ID capture is never blindly repeated | Separate connection-loss and 500 scenarios each send one capture and yield Uncertain with no request ID; connection-loss scenario reconciles through GET order | Matched |
| Unapproved capture refused, unreadable success uncertain, later refusal preserves earlier ambiguity | 422/ORDER_NOT_APPROVED once; malformed 201 is UnreadableSuccess once even with ID; 500 then 422 becomes LaterRefusal with two attempts and stable ID | Matched |
| Completion is per object/capture, with caller-owned partial results | Two embedded captures exhausted without pagination; one COMPLETED, one PENDING; completed order and first accounting detail survive the next detail's 404 | Matched |
| Secrets excluded from formatted clients/errors; explicit unredacted provider access | Credential/builder/client Debug and representative token/HTTP/Concord/Uncertain errors checked; provider text contains fixture secrets; explicit message_unredacted returns it | Matched for tested surfaces |

**Prediction mismatches: none in exercised behavior.** The new tests do not claim
to exercise every documented bound or failure: concurrent token single-flight,
wrong token type, oversized/truncated bodies, overall deadlines, exhausted read
retries and direct Session Debug are outside this fresh caller matrix. The full
existing package suite runs separately below. Fixture replay proves SDK wire
behavior; it does not independently verify PayPal's live retention or policies.
No refresh-token grant is ever attempted. Buyer approval is fixture state, not UI.

## Verification and ownership

- `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-paypal --test first_use --offline --locked`:
  **2 passed, 0 failed**, including both connected workflows and their failure branches.
- `rustfmt --edition 2024 examples/paypal-sdk/tests/first_use.rs`, followed by
  `rustfmt --edition 2024 --check examples/paypal-sdk/tests/first_use.rs`: passed.
- `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-paypal --offline --locked`:
  **39 passed, 0 failed**: 3 unit, 2 first-use, 22 protocol, 11 existing workflow
  tests and 1 compile doctest. No final compiler warnings. Existing test names
  and outcomes are visible in cargo output; their source was not opened or copied.
- Cargo documentation commands also passed for concord-example-testserver,
  concord/concord-retry and concord-core, all `--no-deps --offline --locked`.
- Only FIRST_USE.md and tests/first_use.rs are added. SDK src, all shared crates,
  other examples, scripts, workspace manifest and Cargo.lock remain unchanged.
  No new crates, dependency versions or licenses need a PROVENANCE addition.
- No shared change was necessary. A proposed shared macro-doc improvement would
  qualify `with_base_url`/`prepare` examples where a custom send policy owns
  destination/authentication. Successful metadata through a send policy deserves
  a separate comparison if a caller actually needs it; no such change is made.
- No commits, external messages, provider calls or source-assisted fixes.
  Final assistance label: **docs-only**. Live provider calls: **NOT_RUN**.
- Final target size: 1.6 GiB, roughly 0.2 GiB growth from the initial cache;
  filesystem still reports 430 GiB available. Artifacts reused the default
  worktree target; this was not a clean-build claim. No cleanup was needed.

## Public documentation read inventory

SDK README and Cargo.toml; root Cargo.toml/Cargo.lock (workspace/dependency
identity only), root README, applicable handoff instructions listed in setup.
Generated SDK rustdoc HTML pages under `target/doc/concord_sdk_paypal/`:
`index`, `struct.Builder`, `struct.PayPal`, `struct.Order`, `struct.Capture`,
`struct.HttpError`, `enum.Error`, `struct.Uncertain`, `enum.UncertainCause`,
`struct.PayPalOrdersCreate`, `struct.PayPalOrderGet`, `struct.PayPalOrderCapture`,
`struct.PayPalCaptureGet`, `struct.OrderRequest`, `struct.PurchaseUnitRequest`,
`struct.Money`, `struct.ApiError`, `enum.TokenError`, `struct.OrderId`,
`struct.CaptureId`, `struct.RequestId`, `enum.CaptureStatus`, `struct.StatusDetails`.
These were rendered API documentation, never the generated `src/` HTML.

Other public rustdoc: concord-example-testserver index, TestServer, Recorded,
Reply, Cleanup; concord-retry Policy; Concord Error and ResponseMeta. The
std::time::Instant rustdoc link established the token-clock type without a source
lookup. Read-only provider pages and their read date are cited above.
