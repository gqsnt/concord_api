# FIRST_USE — fresh consumer of `concord-sdk-xero-accounting` (SDK35)

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Role: fresh consumer (Claude Opus 5.5 subagent; harness-reported model id `claude-opus-5-5`; requested effort
not visible to me in harness metadata — recorded as *not observed*). Date: 2026-11-07.

Inputs read before the first working call (in order):

1. `examples/xero-accounting-sdk/README.md` (full).
2. `examples/xero-accounting-sdk/Cargo.toml` (to learn the dependency set and package name).
3. Rustdoc generated with `cargo doc --locked --offline -p concord-sdk-xero-accounting --no-deps`, every public
   item page (`Xero`, `Tenant`, `TenantId`, `Grant`, `FileStore`, `ListInvoices`, `GetInvoice`, `CreateInvoice`,
   `InvoicePage`, `PageCursor`, `Pagination`, `Invoice`, `InvoiceRef`, `InvoicePages`, `Error`, `ErrorKind`, `Class`,
   `Problem`, `ProviderFailure`, `Amount`, `LineItem`, `DraftInvoice`, `ContactRef`, `BODY_LIMIT`).
4. Concord public rustdoc (generated `--no-deps`): `concord-example-testserver` (`TestServer`, `Recorded`, `Reply`,
   `Body`), `concord-pagination` (`Pages`, `PageItems`, `Incomplete`, `PagesError`, `Resume`, `Progress`, `Page`),
   `concord-core` (`ResponseMeta`, `Error` method list, `ErrorKind` variants). `docs/CALLER_GUIDE.md` was grepped for
   `meta`/`status()`/`header` only.
5. Official Xero documentation (read 2026-11-07 via the in-app browser; `WebFetch`/`curl` returned only the
   JavaScript shell):
   - <https://developer.xero.com/documentation/api/accounting/requests-and-responses/> — XML default, JSON via
     `Accept: application/json`; paging on Invoices with `?page=` and `pageSize` (default 100, min 1, max 1000,
     out-of-range values are *adjusted by Xero*); the lowercase `pagination` object
     (`page`, `pageSize`, `pageCount`, `itemCount`) supersedes "fetch until an empty page"; PUT creates, POST
     creates-or-updates; with `summarizeErrors=false` a validation error comes back as HTTP 200 with
     `StatusAttributeString: "ERROR"` and `ValidationErrors`; HTTP 400 carries an `ApiException`.
   - <https://developer.xero.com/documentation/guides/oauth2/token-types/> — access token ≤ 30 minutes; refresh
     token only with `offline_access`, ≤ 60 days, a **new refresh token on every use** ("You must save both
     tokens"); refresh = `POST https://identity.xero.com/connect/token`, `Authorization: Basic
     base64(client_id:client_secret)`, form body `grant_type=refresh_token` (+ `client_id`, `refresh_token`);
     30-minute grace period to retry with the old refresh token; API calls send `Authorization: Bearer …` and
     `xero-tenant-id`.
   - <https://developer.xero.com/documentation/guides/oauth2/scopes/> — `offline_access` is required for a refresh
     token; broad `accounting.transactions(.read)` are deprecated in favour of granular
     `accounting.invoices(.read)` (broad scopes remain until September 2027).

Not read before the first working call: `src/`, `tests/`, `FRICTION.md`, `PROVENANCE.md`, `VALIDATION.md`,
`fixtures/` (only their file names were listed by `ls`).

## Predictions

Written before any test code. "Accounting" = the `accounting` origin given to `Xero::with_origins`, "OAuth" =
the `oauth` origin.

### Setup (both workflows)

Calls: `TenantId::new(uuid)?` → `Grant::new(tenant.clone(), client_id, access, refresh,
"offline_access accounting.invoices", expires_at)?` → `FileStore::create(new_dir, grant)?` →
`Xero::with_origins(store, secret, &server.url(), &server.url())?` → `xero.tenant(&tenant_id)?`.
None of these sends a request (README: "No constructor sends a request"). `FileStore::create` on an existing
directory fails (`ErrorKind::Storage` or `Input`). `Grant::new` without `offline_access`, or without an invoice
scope, fails with `ErrorKind::Input`. A non-UUID tenant fails `TenantId::new` with `Input`.

### Token refresh (first call with an access token inside the 30 s margin)

- Wire: `POST {OAuth}/connect/token`; headers `authorization: Basic base64(client_id:secret)`,
  `content-type: application/x-www-form-urlencoded`, probably `accept: application/json`; form body
  `grant_type=refresh_token&refresh_token=<old>` (I expect the SDK *not* to repeat `client_id` in the body since
  Basic carries it — uncertain).
- Expected response: 200 JSON `{access_token, refresh_token, expires_in (31..=1800), token_type: "Bearer",
  scope}`. The whole new pair is journaled (`pending.json` → rename over `state.json`) before any Accounting
  request; the old refresh token disappears from disk.
- Failures: `400 {"error":"invalid_grant"}` → `ErrorKind::Authentication`, and because the `Refreshing` marker
  replaced the pair beforehand, the next attempt reports `RecoveryRequired` (fail closed; the old refresh token is
  never resent). `expires_in` > 1800, a non-Bearer `token_type` or a changed `scope` → `Authentication` wrapping a
  `Contract`-like error. A storage failure before the exchange → `Storage`, no OAuth request at all. A failed
  rename after `pending.json` is durable → operation fails, `recover_pending()` publishes the pair without
  network I/O, and a restarted client uses it without another refresh.
- A 401 from Accounting is returned (`Class::Unauthorized`), not replayed; the *next* operation refreshes again,
  sending the **rotated** refresh token.
- Secrets: `Debug`/`Display` of `Error`, `Grant`, `FileStore`, `Xero` never contain token strings, the client
  secret, the tenant ID or invoice IDs.

### Workflow 1 — page invoices, retrieve a chosen invoice

Calls: `tenant.pages(None, 2, NonZeroUsize::new(10)?)?` then `while let Some(page) = pages.next_page().await?`;
alternatively `.items().collect_complete().await`. Then `tenant.get(&chosen.reference)?.send().await?`.

- List wire: `GET {Accounting}/api.xro/2.0/Invoices?page=1&pageSize=2`; headers `authorization: Bearer <access>`,
  `xero-tenant-id: <tenant uuid>`, `accept: application/json`; no body. Page 2 is
  `?page=2&pageSize=2`.
- List response: `200 application/json` `{"Invoices":[{InvoiceID, Type, Status, InvoiceNumber, …}],
  "pagination":{"page","pageSize","pageCount","itemCount"}}` → `InvoicePage { invoices, next:
  Some(PageCursor{page: 2}), pagination: Some(..) }` (`next_page()` yields `concord_pagination::Page` with
  `items`, `next`, `meta`). The page whose `page == pageCount` has `next: None`; the following `next_page()` returns
  `Ok(None)` = complete.
- Legacy (no `pagination`): a full page and a short nonempty page both continue; an empty page ends.
- Get wire: `GET {Accounting}/api.xro/2.0/Invoices/<InvoiceID>`, same three headers; response
  `{"Invoices":[{…}]}` with exactly one invoice whose `InvoiceID` matches → `Invoice { reference (bound to this
  tenant), status, invoice_type, invoice_number }`. Another ID in the body → `ErrorKind::Contract`.
- Failures: `tenant.list(0, 2)` and `list(1, 1001)` → `Input` before I/O (the SDK refuses Xero's clamping). 404 →
  `Provider` / `Class::NotFound`. `503 text/plain` → `Provider` / `Class::Unavailable` with `document: Err(..)`. XML
  `content-type` on a 200 → `ErrorKind::Concord` (Concord `Metadata`). Truncated or malformed JSON →
  `Concord` (`Decode`). Inconsistent `pagination` (e.g. `page` ≠ requested) → `Contract`. A cap hit while
  `next` exists → `PagesError::Bounded { resume: Resume::At(cursor) }`; `collect_complete` → `Incomplete {
  collected: <prefix>, error }`. A failing page → `PagesError::Provider { error, failed }` and the prefix is kept.
  429 + `Retry-After: 1` on a GET → silently repeated once (two wire requests); `Retry-After` absent → returned.

### Workflow 2 — create a draft invoice, retrieve its state

Calls: `tenant.contact(contact_uuid)?`, `LineItem::new("Consulting", 3, Amount::from_minor_units(12_500)?)?`,
`tenant.draft(contact, vec![line])?`, `tenant.create(draft)?.send().await?`, then
`tenant.get(&created.reference)?.send().await?`.

- Create wire: `PUT {Accounting}/api.xro/2.0/Invoices` (PUT = create-only; I'd accept POST), no query (perhaps
  `summarizeErrors=false` given the README's "2xx invoice with validation errors"); headers `authorization:
  Bearer`, `xero-tenant-id`, `accept: application/json`, `content-type: application/json`; body
  `{"Invoices":[{"Type":"ACCREC","Status":"DRAFT","Contact":{"ContactID":"…"},"LineItems":[{"Description":
  "Consulting","Quantity":3,"UnitAmount":125.00}]}]}` (possibly without the `Invoices` envelope).
- Create response: `200 {"Invoices":[{InvoiceID, Status:"DRAFT", Type:"ACCREC", InvoiceNumber}]}` →
  `Invoice` whose reference is bound to the tenant. Get: as in workflow 1, `status == "DRAFT"`.
- Failures: `400 {"ErrorNumber":10,"Type":"ValidationException","Message":…,"Elements":[…]}` → `Provider` /
  `Class::Validation`, `write_uncertain() == false`, `Problem` fields reachable deliberately. `200` with
  `StatusAttributeString: "ERROR"` / `ValidationErrors` → `ErrorKind::InvoiceValidation`, not uncertain. `500`,
  `503`, connection hang-up, unreadable 2xx, or `Status: "AUTHORISED"` in the result → `write_uncertain() ==
  true`, exactly one wire request (no replay).

### Tenants

- `xero.tenant(&other_tenant)` → `ErrorKind::TenantMismatch`, no I/O.
- An `InvoiceRef`, `ContactRef`, `DraftInvoice` or `PageCursor` produced in tenant A and passed to tenant B (a second
  `Xero` over a second journal) → `TenantMismatch` before I/O; the server sees zero requests.
- `FileStore::open(dir_of_A, tenant_B, client)` → refused (`TenantMismatch` or `RecoveryRequired`).

## Additional inputs after the first working call

- `concord-core` rustdoc `enum.Delivery.html` (to judge F11).
- Disclosure: before the first call I ran `grep -rn 'pub struct ResponseMeta' crates/` once, which printed one
  Concord source line (`crates/concord-core/src/response.rs:512`). I used it only to learn which crate's rustdoc
  to generate; no Concord or SDK source was read. No SDK `src/`, `tests/`, `FRICTION.md`, `PROVENANCE.md`,
  `VALIDATION.md` or `fixtures/` content was read at any point.

## First working call

`tests/first_use.rs` (first version, since grown into the suite): `TenantId::new` → `Grant::new(…, "offline_access
accounting.invoices", now + 20 min)` → `FileStore::create(new dir, grant)` → `Xero::with_origins(store, secret,
&server.url(), &server.url())` → `xero.tenant(&tenant_id)` → `tenant.list(1, 100)?.send().await`. It compiled and
passed on the first attempt (elapsed time not measured; all preparation was reading the docs above). Wire: `GET
/api.xro/2.0/Invoices?page=1&pageSize=100` with `xero-tenant-id`, `accept: application/json`, `authorization: Bearer
access-token-1`. It needed 4 SDK imports (`FileStore, Grant, TenantId, Xero`) plus the test server's, and no
types I didn't use.

## Application (tests/first_use.rs)

12 tests, all against `concord-example-testserver`. Real Xero: **NOT_RUN**.

| Test | What it covers |
|---|---|
| `workflow1_pages_invoices_then_retrieves_the_chosen_one` | expired access → one refresh (exact Basic auth, form body, accept) → two modern pages (metadata asserted, explicit `Ok(None)` end, sticky) → pick invoice `INV-0003` → GET it; the rotated pair is on disk and the consumed token is gone; journal dir is 0700; a restart via `FileStore::open` uses the rotated access token with no second refresh |
| `workflow1_partial_results_failures_and_legacy_paging` | local bounds rejected before I/O; cap → `Incomplete{collected: 2}` + `Bounded{Resume::At(page 2)}`; resuming from that cursor fetches only page 2; a cursor reused with another page size → `Input`; legacy paging continues past a short page until an empty one; a page-2 503 text body keeps the 4-item prefix, `Class::Unavailable`, status 503, `document: Err`; inconsistent metadata → `Contract`; XML and truncated JSON → `Concord`; 404 → `NotFound`, no replay |
| `workflow1_retrieval_contract_and_rate_limit` | a body naming another invoice → `Contract`; 429 + `Retry-After: 1` → one repeat after ≥ 1 s; 429 with no hint → returned once, `retry_after: Ok(None)` |
| `workflow2_creates_a_draft_then_retrieves_its_state` | exact create wire (method, target, headers, body shape, two-decimal numbers on the wire), then GET of the created invoice: `DRAFT`/`ACCREC`, same reference and tenant |
| `workflow2_create_failures_and_write_uncertainty` | 400 problem (`number`, `exception_type`, `elements` reachable; not uncertain; message text withheld from Display/Debug); 200 + `ValidationErrors` → `InvoiceValidation` (not uncertain); 500, hang-up, `AUTHORISED` result and a two-invoice result → uncertain; exactly one wire request per attempt; local draft validation |
| `rotation_after_unauthorized_uses_the_rotated_refresh_token` | 401 returned without replay; the next call presents the **rotated** refresh token; only the newest pair is on disk; clones share the cache |
| `rotation_storage_failure_before_exchange_sends_nothing` | read-only journal dir → failure with zero requests; once fixed, the original refresh token is still valid and used |
| `rotation_failed_activation_is_recovered_without_a_second_exchange` | a real failed rename (fixture swaps `state.json` for a non-empty dir during the exchange) → no Accounting dispatch, a retry does not re-exchange (`RecoveryRequired`), operator repair + `recover_pending()` is local, the restarted client uses the rotated pair |
| `rotation_lost_before_journal_fails_closed` | `pending.json` blocked → pair lost; the consumed token is never on disk or presented again; `RecoveryRequired` from send/`remaining_lifetime`/`Xero::with_origins` after restart; `recover_pending()` → `Storage` (F7) |
| `rotation_refused_or_failed_exchange_fails_closed` | `invalid_grant`, `expires_in` 3600, `token_type` mac, narrowed scope, missing `refresh_token`, identity 503, refused connection: each makes exactly one exchange and no Accounting request, then `RecoveryRequired` (F11) |
| `tenant_bound_values_never_cross_tenants` | `xero_a.tenant(B)`, `FileStore::open(A dir, B)`, B using A's `InvoiceRef`/imported ref/`PageCursor`/`ContactRef`/`DraftInvoice` → `TenantMismatch` with zero requests; each client sends only its own bearer to its own tenant; the same raw ID imported in two tenants gives unequal refs |
| `grant_scopes_and_redaction` | scope validation (granular and broad accepted; missing `offline_access` or invoice scope refused); handle `Debug` redacted; read-only grant create refused locally (F9); origin validation; the `TenantId` serde gap pinned (F8) |

Observed checks (this worktree, 2026-11-07, `CARGO_BUILD_JOBS=4`, default `target/`):
- `cargo test --locked --offline -p concord-sdk-xero-accounting --test first_use`: 12 passed; 5 consecutive
  runs of the binary all 12/12 (≈1.1 s, including the 1 s `Retry-After`).
- `cargo test --locked --offline -p concord-sdk-xero-accounting`: lib 0, first_use 12, protocol 17, workflows 8,
  doctest 1, all passed (the last three are the author's suites, run but not read).
- `cargo clippy --locked --offline -p concord-sdk-xero-accounting --all-targets -- -D warnings`: clean.
- `rustfmt --check` on `first_use.rs`: clean. `Cargo.toml`/`Cargo.lock` unchanged: the workspace member and
  lock entry were already present in the worktree's HEAD (`39d878b5`); no new crates.

## Friction incidents, in order

- **F1 (docs access).** All three official Xero pages are client-rendered; `WebFetch` and `curl` returned only the
  JS shell, so I read them in the in-app browser. Not SDK friction.
- **F2 (rustdoc, misleading names).** With `--no-deps`, foreign types render as a bare, unlinked `Error`:
  `ErrorKind::Concord(Box<Error>)`, `Admission(Error)`, `Error::concord_error() -> Option<&Error>`, two
  `impl From<Error> for Error`, `ProviderFailure.document: Result<Problem, Box<Error>>`. Meanwhile
  `Authentication(Arc<Error>)` is the SDK's own `Error`, linked. I couldn't tell the crates apart without
  generating Concord's rustdoc. Proposal: name them in prose (`concord::Error`, the rate limiter's error).
- **F3 (two names for one page).** `list().send()` gives `InvoicePage { invoices, next, pagination }`;
  `pages().next_page()` gives `concord_pagination::Page { items, next, meta }`, with the same data under
  different field names. Matching `PagesError`/`Resume` needs `concord_sdk_xero_accounting::concord_pagination::…`.
  The re-export works, but the README doesn't show it. `ProviderFailure.meta.status()` is only documented in
  concord-core's rustdoc.
- **F4 (first-call ceremony, minor).** Five steps before the first request. The tenant ID is passed to `Grant::new`
  and again to `xero.tenant(&id)`, where only one value can succeed. This is a deliberate visible assertion, but
  redundant. `pages()` takes a bare `NonZeroUsize`, so it needs an `unwrap()` that `max_pages(impl
  Into<NonZeroUsize>)` avoids.
- **F5 (my own fixture bugs, and how they surfaced).** (a) Page 1 of `pageSize=4` with `itemCount 6` had only 2
  items. The SDK correctly refused it as `Contract("inconsistent invoice pagination metadata")`, and I only saw
  `Incomplete{collected: []}` + `Provider` counters, because `PagesError`'s Debug withholds the provider error by
  design. I found the cause from the README's "page, size, count and total must agree". (b) My responder panicked
  on a GET without `page`, which surfaced as a transport failure (`class() == None`). (c) Reparsing the request
  body with `serde_json::Value` printed `125.0`; the wire carries `125.00`. None of these are SDK defects.
- **F6 (nested credential errors).** Every credential-stage failure from `send()` is
  `ErrorKind::Authentication(inner)`: storage failure, `RecoveryRequired`, OAuth 400/503, invalid token
  lifetime/type/scope, transport. Display is the same string ("credential acquisition failed") for all of them,
  and the outer `class()` is `None` even when the identity server answered 400/503. To decide "fix storage /
  reauthorize / retry later", I had to write a `credential_kind()` unwrap helper. The README's "Contention returns
  a typed error" and `ErrorKind::RecoveryRequired` don't say that from `send()` they arrive nested. Proposal: an
  `Error::credential_kind()` accessor, or a documented nesting rule.
- **F7 (wrong signal from `recover_pending`, observed).** With a `Refreshing` marker and no pending pair (the
  lost-response case), `FileStore::recover_pending()` returns `ErrorKind::Storage`. `remaining_lifetime()`,
  `send()` and `Xero::with_origins` report `RecoveryRequired`, and the README/rustdoc route this state to "a fresh
  authorization grant in a new directory". `Storage` reads as "filesystem problem, retry", which is the wrong
  action. Proposal: return `RecoveryRequired` here. The test pins the observed `Storage` with a comment.
- **F8 (validation bypass, observed).** `TenantId` implements `Deserialize` without the UUID check of
  `TenantId::new`. `serde_json::from_str::<TenantId>("\"not-a-tenant\"")` succeeds, and the value flows through
  `Grant`, `FileStore`, `tenant()` and onto the wire as `xero-tenant-id: not-a-tenant`. An application loading the
  tenant from its own config via serde skips the documented validation. Proposal:
  `#[serde(try_from = "String")]`. Pinned in `grant_scopes_and_redaction`.
- **F9 (where scope is checked).** With a read-only grant, `tenant.create(draft)` builds fine and `send()` refuses
  locally with `Input("invoice write scope is required")`, no I/O and not uncertain. Every other local check
  (`list` bounds, tenant binding, cursor size) fails at build time. A minor inconsistency.
- **F10 (naming).** `FileStore::open(dir, same tenant, "another-client")` reports `TenantMismatch`, although the
  mismatch is the OAuth client ID.
- **F11 (operational, design trade-off, observed).** *Every* refresh that does not end with a complete validated
  journaled pair makes the journal require reauthorization. That includes an identity-server **503** and a
  **refused connection** where no HTTP request was delivered, as well as `invalid_grant` and malformed or
  out-of-policy token responses. The old refresh token is erased before the exchange, so even a manual use of
  Xero's documented 30-minute grace retry ("you can retry using your existing refresh token for a grace period of
  30 minutes", token-types page) is impossible. For an app, a transient identity outage or network blip therefore
  means sending the user through consent again. The README states the fail-closed rule and that the grace retry
  is deliberately unused, but not this consequence for 5xx/connect failures. Concord's `Delivery` classifies a
  refused connection as `Unknown` (possible redirect), but this SDK disables redirects, so the SDK, not Concord,
  owns that call. Proposal for the owner, not a decision: keep the old refresh token in the `Refreshing` marker
  with its timestamp, and allow one bounded retry inside the grace window when no pair was issued (an `Answered`
  non-2xx, or a connect error with redirects off). Or at least document the 5xx/connect consequence.
- **F12 (misleading diagnostic).** For a refresh-response decode failure and for the refused connection, the inner
  error's `Debug` stage is `"invoice request or response failed"`, although the failing operation is
  `OAuth.refresh`. Concord's own message correctly names `POST OAuth.refresh`.
- **F13 (wire, prediction mismatch).** Create is `POST /api.xro/2.0/Invoices?summarizeErrors=true`. Xero documents
  PUT as create-only and POST as create-or-update. The draft body never carries an `InvoiceID`, so POST creates,
  but PUT would make "create exactly one" a provider-enforced property rather than a body-shape property. This is
  a question for the author, not a defect. `summarizeErrors=true` is Xero's default; the SDK still handles a 2xx
  with `ValidationErrors` (`InvoiceValidation`).
- **F14 (test hygiene).** Fault injection needed the README-documented journal file names (`state.json`,
  `pending.json`) and filesystem changes from inside the responder; there is no public fault hook. That is
  acceptable for advanced tests. Without a tempdir crate, journals accumulate under `target/tmp/xero-first-use`
  (154 dirs / 1.3 MB of fake tokens, measured mid-session).

## Predictions compared with observed behaviour

| Prediction | Observed | Verdict |
|---|---|---|
| Constructors send nothing | Zero requests until the first `send()` | match |
| Refresh: `POST /connect/token`, Basic `base64(client:secret)`, form, `accept: application/json` | exactly that | match |
| Refresh body: `grant_type` + `refresh_token`, without `client_id` | `grant_type`, `client_id`, `refresh_token`, as in Xero's example | **mismatch** (mine) |
| Whole pair journaled before Accounting; old token gone | yes (0700 dir; old token absent from every file) | match |
| `invalid_grant` → `Authentication`, then `RecoveryRequired` | yes (inner `Provider`, `Class::Validation`, outer `class() == None`) | match (nesting: F6) |
| Bad `expires_in`/`token_type`/scope → `Authentication` wrapping a contract error | inner `Contract(..)` for each | match |
| (not predicted) identity 503 / refused connection | `RecoveryRequired` after one attempt | **unpredicted** (F11) |
| Storage failure before exchange → `Storage`, no OAuth request | `Authentication(inner Storage)`, zero requests | partial (nesting) |
| Failed rename → fail; `recover_pending()` local; restart without refresh | yes; an in-process retry → `RecoveryRequired`, no re-exchange | match |
| Lost pending pair → `RecoveryRequired` | send/`remaining_lifetime`/`with_origins` yes; `recover_pending()` → `Storage` | **mismatch** (F7) |
| 401 returned, not replayed; next op refreshes with the rotated token | yes | match |
| Redaction of tokens/secret/tenant/IDs | yes (`<redacted>`/`<withheld>`; problem text withheld) | match |
| List wire `GET /api.xro/2.0/Invoices?page=N&pageSize=S` + 3 headers, no body | exactly | match |
| Modern metadata, `next` until `page == pageCount`, then `Ok(None)` | yes, sticky | match |
| Legacy continues past a short page until empty | yes (3 requests) | match |
| Get `GET /Invoices/{id}`; other ID → `Contract` | yes | match |
| Bounds `list(0,_)`, `list(1,1001)` → `Input` before I/O | yes (also `list(1,0)`, `pages(None,1001,_)`) | match |
| 404/503-text/XML/truncated/inconsistent metadata | `NotFound` / `Unavailable` + `document: Err` / `Concord` / `Concord` / `Contract` | match |
| Cap → `Bounded{Resume::At(cursor)}`, `Incomplete` keeps the prefix | yes; resume fetches only the next page | match |
| Failing page → `Provider`, prefix kept | yes | match |
| 429 + `Retry-After` → one repeat; no hint → returned | yes (≥ 1 s wait; `retry_after: Ok(None)`) | match |
| Create `PUT /api.xro/2.0/Invoices`, no query | `POST …/Invoices?summarizeErrors=true` | **mismatch** (F13) |
| Create body `{"Invoices":[{Type,Status,Contact{ContactID},LineItems[…]}]}`, two-decimal amount | exactly; only those 4 keys; `125.00`, `42.05` | match |
| 400 → `Validation`, not uncertain, `Problem` reachable | yes | match |
| 200 + `ValidationErrors` → `InvoiceValidation`, not uncertain | yes | match |
| 500 / hang-up / `AUTHORISED` → uncertain, one request | yes (also a two-invoice result) | match |
| Read-only grant create refused before I/O at build | refused before I/O at `send()` | partial (F9) |
| Cross-tenant use → `TenantMismatch` before I/O | yes for every bound value; `open(A, B)` → `TenantMismatch` | match |
| (not predicted) `open(A, other client)` | `TenantMismatch` | naming (F10) |
| `TenantId::new` rejects non-UUIDs | yes, but `Deserialize` does not | **gap** (F8) |

## Assistance label

`docs-only`: SDK README and rustdoc, Concord public rustdoc and `docs/CALLER_GUIDE.md`, official Xero pages.
I read no SDK source, tests, FRICTION/PROVENANCE/VALIDATION or fixtures. The disclosed one-line Concord source
grep is listed above.
