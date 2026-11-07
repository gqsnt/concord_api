# First use — SDK33 Square Payments (fresh consumer)

Role: fresh consumer of `concord-sdk-square-payments`, which I did not write.
Date: 2026-11-07. Live provider calls: **NOT_RUN** (every call below goes to
`examples/testserver` on 127.0.0.1).

## What I read before predicting (in order)

1. `examples/square-payments-sdk/README.md`.
2. `examples/square-payments-sdk/Cargo.toml` (to know the dev-dependencies a test
   target gets; it is package metadata, not `src/`).
3. Directory *names* only of `tests/` (`protocol.rs`, `support/`, `workflows.rs`) and
   `fixtures/`, to make sure `tests/first_use.rs` did not already exist. No file
   contents opened.
4. Rustdoc generated with `cargo doc --locked --offline -p concord-sdk-square-payments --no-deps`
   (crate page, `Square`, `SquareConfig`, `SquarePayments`, `SquarePaymentsCreate/Get/List`,
   `Error`, `ProviderError`, `ErrorDocument`, `SquareError`, `CreatePayment`, `Money`,
   `Payment`, `PaymentResponse`, `PaymentsPage`, `InputError`, `CredentialError`,
   `bearer`, `transport`, `SquarePolicy`, `API_VERSION`, `Pages`, `PagesError`, `Resume`).
5. Rustdoc of `concord-example-testserver` (`TestServer`, `Recorded`, `Reply`, `Body`,
   `Script`, `Gate`) and of `concord` (`Error`, `ResponseMeta`, `ErrorKind`, `Operation`,
   `Delivery`).
6. Official Square documentation, read 2026-11-07 (read-only, through a fetch tool that
   summarises pages):
   - https://developer.squareup.com/reference/square/payments-api — lists
     `POST /v2/payments`, `GET /v2/payments/{payment_id}`, `GET /v2/payments`.
   - https://developer.squareup.com/docs/build-basics/common-api-patterns/idempotency —
     same key + same request returns the first response; same key + different request
     returns an error ("might vary depending on the API"). No status code named there.
   - https://developer.squareup.com/reference/square/payments-api/create-payment —
     `idempotency_key` required, min 1, max 45 ("multi-byte characters may reduce the
     allowed count"), `source_id` required, `amount_money` required, `autocomplete`
     default true; example headers `Square-Version: 2026-09-16`,
     `Authorization: Bearer ACCESS_TOKEN`, `Content-Type: application/json`.
   - https://developer.squareup.com/reference/square/payments-api/list-payments —
     `cursor`, `limit` (default and max 100, larger values ignored), default time range of
     one year, main location by default; "if empty, the response is the final one".
   - https://developer.squareup.com/reference/square/enums/ErrorCode —
     `IDEMPOTENCY_KEY_REUSED`: "The provided idempotency key has already been used."
     (no HTTP status listed).

## Predictions

Written before any test code. "Base" = `TestServer::url()` (`http://127.0.0.1:<port>`);
the README says replacing the base keeps the declared `/v2` prefix.

### Setup (both workflows)

- Calls: `bearer("sandbox-token")?` → `HeaderValue` `Bearer sandbox-token` (sensitive);
  `transport(Duration)?` → a reqwest `Client` without redirects/retries;
  `Square::new(client, SquareConfig { authorization })` then `.with_base_url(base)?`.
  No I/O.
- Failures: `bearer("")` and a token with a space or control character →
  `Err(CredentialError::Token)`; nothing about the token is printed. A base with a query
  or user info → `with_base_url` error (a `concord::Error`, kind `Build`, I expect).
- Secrets: `format!("{:?}", api)` does not contain the token.

### Workflow 1 — create with caller key, then retrieve

1. `CreatePayment::new(key, "cnon:card-nonce-ok", Money { amount: 1000, currency: "USD".into() })?`
   - Local validation only. 46-byte key, empty key, empty source, amount `0`/negative,
     `"usd"`/`"US"` → `InputError::{IdempotencyKey, Source, Amount, Currency}`; nothing sent.
2. `api.payments().create(body)`; keep `operation.clone()` before `send()`.
   - Wire: `POST /v2/payments`, no query. Headers: `authorization: Bearer sandbox-token`,
     `square-version: 2026-09-16`, `content-type: application/json`,
     `accept: application/json` (I expect the JSON format sends an Accept). Body JSON:
     `{"idempotency_key": key, "source_id": "cnon:…", "amount_money": {"amount":1000,"currency":"USD"}, "autocomplete": true}`
     (field order unknown; I will compare parsed JSON). No idempotency header.
   - Returns `PaymentResponse { payment: Payment { id, amount_money, status } }`;
     unknown fields such as `card_details`/`receipt_url` are ignored. Status string verbatim,
     e.g. `"COMPLETED"`.
3. `api.payments().get(&created.id).send().await?`
   - Wire: `GET /v2/payments/{id}` (id percent-encoded as one segment, `a/b` → `a%2Fb`),
     same `authorization`/`square-version`/`accept`, no `content-type`, no body.
   - Returns the same `PaymentResponse` shape.
- Failures I expect:
  - Response delayed past the transport timeout → `Error::Concord(e)` with
    `e.kind() == ErrorKind::Transport`, `e.transport().unwrap().is_timeout()`,
    `e.delivery() == Delivery::Unknown`, and `err.create_outcome_unknown() == true`.
    The SDK does not retry: exactly one request on the wire.
  - Explicit replay of the retained clone → second `POST` whose body bytes equal the first
    (same key). Square (my fixture) returns the original payment (200).
  - Same key, changed body (e.g. amount 1500) → Square returns **400** with
    `{"errors":[{"category":"INVALID_REQUEST_ERROR","code":"IDEMPOTENCY_KEY_REUSED",…}]}`
    → `Error::Provider(p)`, `p.meta.status() == 400`, `p.is_idempotency_mismatch()`,
    `create_outcome_unknown() == false` (4xx is an answer).
  - 401 `UNAUTHORIZED` / 404 `NOT_FOUND` → `Error::Provider(p)` with typed
    `p.document == Ok(ErrorDocument { errors: [..] })`, codes retained as strings.
  - 5xx on create → `Error::Provider`, `create_outcome_unknown() == true`.
  - 502 with an HTML body → `Error::Provider(p)`, status 502 kept, `p.document` is `Err`
    (a Concord metadata/decode error).
  - 200 with a malformed JSON body on create → `Error::Concord`, kind `Decode`,
    `create_outcome_unknown() == true` ("unreadable success").
  - 200 with `content-type: text/html` → `Error::Concord`, kind `Metadata`.
  - Secrets: `Debug`/`Display` of every error and of the operation do not contain the token,
    the idempotency key, the source token or provider `detail` strings.
- Metadata: on success the ordinary `send()` returns only the model; I expect **no**
  response head (no request id) on the success path. Errors carry `ResponseMeta`
  (`p.meta.status()`, `p.meta.headers()`, `p.meta.operation()` named
  `Square.payments.create`).

### Workflow 2 — page by cursor, inspect one payment

1. `api.payments().list().set_limit(2u32).pages(Resume::Start, NonZeroUsize::new(10).unwrap())`
   - Building the traversal sends nothing.
   - `next_page()` #1 → `GET /v2/payments?limit=2` (no `cursor`), headers
     `authorization`, `square-version`, `accept`. Returns `Ok(Some(page))`,
     `page.items: Vec<Payment>`.
   - `next_page()` #2 → `GET /v2/payments?cursor=<c1>&limit=2` (rustdoc lists query order
     `cursor`, `limit`; the cursor is percent-encoded). Limit kept from the template.
   - An empty page **with** a cursor → `Ok(Some(page))` with no items, traversal continues.
   - Last page: `cursor` absent or `""` → that page is `Ok(Some(..))`, the following call is
     `Ok(None)` and stays `Ok(None)`; no extra request.
2. Inspect one payment: `api.payments().get(id).send().await?` → `GET /v2/payments/{id}`.
- Failures I expect:
  - `max_pages` reached while a cursor remains → `Err(PagesError::Bounded { resume: Resume::At(cursor), .. })`,
    later calls `PagesError::Ended`.
  - Page 2 answered 500/429 → `Err(PagesError::Provider { error: Error::Provider(p), failed: Resume::At(c1), .. })`;
    pages I already pulled are kept by me; `Resume::At(c1)` restarts at the failed page;
    nothing is retried.
  - `collect_complete()` on a failing listing → `Err(Incomplete { collected, error })` with
    the prefix.
  - Malformed page JSON → `PagesError::Provider { error: Error::Concord(kind Decode), .. }`.
  - Secrets: `Debug` of `Pages`/`PagesError` never prints the cursor.
- Single page: `list().maybe_cursor(Some(c)).send().await?` → `PaymentsPage { payments, cursor }`;
  an omitted `payments` field → empty `Vec`.

## First working call

- 04:42:21Z rustdoc generated; predictions written; 04:45:47Z the first test
  (`first_working_call`, a create against `TestServer`) compiled and passed on the first
  attempt. It took one file and these imports: `Square`, `SquareConfig`, `CreatePayment`,
  `Money`, `bearer`, `transport` (SDK), `Recorded`, `Reply`, `TestServer` (testserver) and
  `std::time::Duration`. No unneeded import or type.
- Build: `cargo test --locked --offline -p concord-sdk-square-payments --test first_use`.
  `--locked` worked from the first build because the package was already in the workspace
  `members` and in `Cargo.lock` (author commit `a8e4a003`); my work changed neither file
  and added no dependency.
- Observed wire of that call: `POST /v2/payments`, headers in order `authorization`,
  `square-version: 2026-09-16`, `content-type: application/json`, `accept: application/json`,
  `host`, `content-length`; body
  `{"source_id":…,"idempotency_key":…,"amount_money":{"amount":1000,"currency":"USD"},"autocomplete":true}`.
- That scratch test was then replaced by the application tests below.

## The application (`tests/first_use.rs`)

- `workflow_1_create_with_caller_key_replay_after_timeout_then_retrieve` — a stateful
  Square stand-in commits the first create and answers it after 3 s; the client timeout is
  400 ms. The app's `charge()` routine keeps `operation.clone()`, sees the timeout and sends
  the retained clone once. Asserts: local `InputError`s send nothing; 2 POSTs with
  byte-identical bodies and the caller key in the JSON body (no idempotency header);
  exactly one payment committed; timeout error is `ErrorKind::Transport`,
  `Delivery::Unknown`, operation `Square.payments.create`, `create_outcome_unknown()`;
  `get(id)` wire and equality; a rebuilt body (amount 1500) under the same key is
  `Error::Provider` with `is_idempotency_mismatch()`, status 400, typed document, sent once,
  not retried, no second payment; an ID containing `/` is one path segment; token, key,
  source and provider detail never appear in Debug/Display.
- `workflow_2_page_account_payments_by_cursor_and_inspect_one` — listing of 3 pages
  (2 payments + cursor, empty page + cursor, 1 payment + no cursor) with base64-like
  cursors containing `+ / =`. Asserts: building `pages()` sends nothing; page sizes
  `[2, 0, 1]`; `Ok(None)` is sticky with no extra request; exact query pairs per request;
  cursor percent-encoded; `get()` of a returned payment; single-page `maybe_cursor`/`set_cursor`;
  `max_pages = 1` → `Bounded { resume: At(C1) }` then `Ended`; a one-time 503 on page 2 →
  `PagesError::Provider { error: Error::Provider(p), failed: At(C1) }` with status,
  `retry-after` header and operation `Square.payments.list`, page 1 kept by the app, resume
  from `failed` with the same limit completes the listing without re-reading page 1;
  `collect_complete()` returns the 1-page prefix with the error; cursor never in diagnostics.
- `workflow_2_empty_final_cursor_completes` — `"cursor": ""` ends the traversal; no `limit`
  unless the caller sets one.
- `predicted_failure_shapes` — credential/base-URL refusal, 500/502-HTML/malformed-200 on
  create (`create_outcome_unknown()` true for each), 401 typed and redacted, `text/html` 200
  is `Metadata`, unknown category/code and two entries retained, malformed list page is
  `PagesError::Provider { error: Error::Concord(Decode), failed: Start }`.

Results (2026-11-07, worktree `target/`, `CARGO_BUILD_JOBS=4`, one cargo process at a time):

- `cargo test --locked --offline -p concord-sdk-square-payments` — first_use 4/4, plus the
  author's protocol 11/11, workflows 4/4, doctest 1/1 (I ran them, did not read them).
- `cargo clippy --locked --offline -p concord-sdk-square-payments --all-targets -- -D warnings` — clean
  (after fixing two `err_expect` lints in my own test).
- Teeth check: temporarily swapping the expected query order of page 2 made the workflow-2
  test fail with the observed `[("cursor", …), ("limit", "2")]`; restored.

## Friction incidents, in order

1. *Reading rustdoc.* Every generated endpoint page leads with framework mechanism (format
   statements, Content-Type/`ACCEPT` rules, `prepare()` versus policy `send()`), and the client
   page leads with `validate_header_names()`. The caller facts (method, path, query, headers)
   are there but buried. Reading cost only.
2. *Reading rustdoc.* `Error::Concord(Error)` and `ProviderError::document: Result<ErrorDocument, Error>`
   show the inner type as bare `Error` on the SDK's own `Error` page. I inferred
   `concord::Error` from the README's `error.transport()` and Concord's docs.
3. *Surprising re-exported names.* `bearer` says "Provider-specific token grammar is
   deliberately left to the SDK" on the SDK's own page; `CredentialError` lists Basic
   `Username`/`Password` variants that cannot occur here. Harmless.
4. *Misleading generated text.* `PagesError` variants carry rustdoc's `#[non_exhaustive]`
   boilerplate "cannot be matched against", contradicting the README's
   `PagesError::Bounded { resume, .. }`. Matching with `..` compiles and works (observed).
5. *Partial re-export.* `Pages`, `PagesError`, `Resume` are re-exported; `Page`, `Incomplete`,
   `Progress` are not, although the README says pagination types are re-exported. Field access
   was enough (`page.items`, `incomplete.collected`), so I added no dependency; naming
   `Page<Payment, String>` in an application signature would need `concord-pagination` directly.
6. *Three setup error types.* `transport()` → reqwest error, `bearer()` → `CredentialError`,
   `with_base_url()` → `concord::Error`. A function returning the SDK `Error` cannot `?` the
   first two; I used `Box<dyn std::error::Error>` like the README, and downcast to
   `concord_sdk_square_payments::concord::Error` to assert the base-URL refusal is `Build`.
7. *Timeout detection is two layers deep.* `Error::Concord(e)` → `e.transport()` → reqwest
   `is_timeout()`. The match guard moves the error into the binding, so to keep it I rebuilt
   `Error::Concord(error)`. There is no SDK-level `is_timeout()`; `create_outcome_unknown()`
   is broader (5xx, redirects, unreadable success). I followed the README's narrower guard
   because the required failure is "after a timeout".
8. *Opaque provider Display.* The idempotency mismatch displays as `Square HTTP 400`, the
   same shape as 401/404/502; Debug shows `SquareError(<redacted>)`. A log line cannot tell
   `IDEMPOTENCY_KEY_REUSED` from any other 400; the caller must call `is_idempotency_mismatch()`
   or read `document`. Concord-stage Display names the operation
   (`POST Square.payments.create: transport error (timeout)`); provider Display does not
   (Debug does, via `meta.operation`). Deliberate redaction, but a diagnosis cost.
9. *No success metadata on the ordinary path.* `send()` returns only `PaymentResponse` /
   `PaymentsPage`, and `pages()` uses `M = ()`. A success response head (e.g. a request-id
   header) is reachable only through `prepare()?.receive()`, which the README says bypasses
   the SDK error policy. I did not use it, so success-path metadata is asserted on the wire only.
10. *Generated `maybe_*` asymmetry.* `set_cursor` takes `impl IntoValue<String>` (accepts
    `&str`); `maybe_cursor` takes `Option<String>`, so `.to_owned()` was needed.

Not friction (checked): the README's `set_limit(25u32)` suffix is not required —
`set_limit(2)` also compiles (temporary edit, `cargo check`, reverted).

Source lookups: **none**. I did not open `src/`, `tests/`, `fixtures/` contents,
`FRICTION.md`, `PROVENANCE.md` or `VERIFICATION.md`.

## Predictions versus observed

| Prediction | Observed | Verdict |
|---|---|---|
| `bearer("")`, space, newline → `CredentialError::Token` | same | match |
| base URL with query → `concord::Error`, kind `Build` ("I expect") | same | match |
| `Debug` of client hides token | `Square { base: "http://127.0.0.1:…/", authorization: <redacted> }` | match |
| `InputError` for empty/46-byte key, empty source, amount 0, `usd` | same; 23×`é` (46 bytes) refused, 45 ASCII bytes accepted | match |
| create wire: `POST /v2/payments`, `authorization`, `square-version: 2026-09-16`, `content-type` and `accept: application/json`, key in body, `autocomplete: true`, no idempotency header | same (field order `source_id, idempotency_key, amount_money, autocomplete`; I predicted "order unknown") | match |
| response returns `id/amount_money/status`, extra fields ignored | same | match |
| `get` wire: `GET /v2/payments/{id}`, no content-type/body, `/` → `%2F` | same (`R2B3%2F..%2Frefunds`) | match |
| timeout → `Concord`, `Transport`, `is_timeout`, `Delivery::Unknown`, outcome unknown, 1 request | same; Display `POST Square.payments.create: transport error (timeout)` | match |
| explicit replay byte-identical; fixture returns original payment | same; one payment committed | match |
| changed body under same key → `Provider`, 400, `is_idempotency_mismatch()`, outcome known | same | match (the 400 is **my fixture's choice**: the official pages I read name the code but no status) |
| 401/404 typed documents, strings retained | same | match |
| 5xx create → `Provider`, outcome unknown | same, single attempt | match |
| 502 HTML → `Provider`, status kept, `document` `Err` ("metadata/decode") | `Err` of kind `Metadata` | match (narrowed) |
| 200 malformed create → `Decode`, outcome unknown | same; `head()` status 200 retained | match |
| 200 `text/html` → `Metadata` | same | match |
| diagnostics hide token, key, source, detail | same | match |
| no success metadata on ordinary `send()` | same | match (recorded as friction 9) |
| provider Display content | `Square HTTP <status>` without operation or code | not predicted (friction 8) |
| list #1 `?limit=2`, #2 `?cursor=<c1>&limit=2`, cursor percent-encoded | same; `cursor=bGFzdD1HUTNa%2Bpage%2F2%3D%3D&limit=2` | match |
| empty page with cursor continues; absent/`""` cursor ends; `Ok(None)` sticky, no extra request | same | match |
| `max_pages` reached with cursor → `Bounded { At(c1) }`, then `Ended` | same | match |
| page-2 503 → `PagesError::Provider { Error::Provider, failed: At(c1) }`, prefix kept, nothing retried | same; `retry-after` header readable | match |
| `collect_complete()` → prefix + error | same (1 page) | match |
| malformed page → `PagesError::Provider { Error::Concord(Decode) }` | same, `failed: Start` | match |
| `Pages`/`PagesError` Debug never prints the cursor | `Provider { failed: At(..), progress: … }`; Display `a page request failed (after 1 pages emitted)` | match |
| single page: omitted `payments` → empty `Vec` | same | match |

No prediction was contradicted. Not exercised: live Square (NOT_RUN), Square's real status
for `IDEMPOTENCY_KEY_REUSED`, redirects, oversized bodies, process-restart persistence of
the retained operation (the app re-creates the body from the persisted key instead).

## Assistance label

`docs-only` — README, `Cargo.toml` of the package, generated rustdoc of
`concord-sdk-square-payments`, `concord-example-testserver`, `concord` and
`concord-pagination`, and the official Square pages listed above (read 2026-11-07).
