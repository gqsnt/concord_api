# FIRST_USE — fresh consumer of `concord-sdk-adyen-checkout` (SDK31)

Role: fresh consumer, run as an Agent-tool subagent. Observed model: the harness system prompt
for this run names model ID `claude-opus-5-5`; the requested/observed reasoning effort was not
visible to the run and is not recorded here. I did not write this SDK.

Material read before any code, in order (2026-11-07):

1. `examples/adyen-checkout-sdk/README.md`, `examples/adyen-checkout-sdk/Cargo.toml`.
2. Rustdoc generated with `cargo doc --locked --offline -p concord-sdk-adyen-checkout --no-deps`
   (every page of the crate: index, `Checkout`, `CheckoutConfig`, `CheckoutV71*`, models, errors,
   `SendPolicy`, constants).
3. Rustdoc for the fixture server: `cargo doc -p concord-example-testserver --no-deps`
   (`TestServer`, `Recorded`, `Reply`, `Body`, `Script`, `Gate`).
4. Concord public rustdoc (`cargo doc -p concord -p concord-retry -p concord-core --no-deps`):
   `ResponseMeta`, `Json`, `concord_retry::{Report, Policy}` — because the SDK's rustdoc names
   `ResponseMeta`, `Report` and `Policy` without links (`--no-deps`) and does not re-export them.
5. Official Adyen documentation (read-only, read 2026-11-07):
   - <https://docs.adyen.com/api-explorer/Checkout/latest/overview> — latest is **v72**; test
     base `https://checkout-test.adyen.com/v72`; `X-API-Key`; `POST /paymentMethods`,
     `POST /payments`, `POST /payments/details`.
   - <https://docs.adyen.com/api-explorer/Checkout/71/post/payments>,
     <https://docs.adyen.com/api-explorer/Checkout/71/post/payments/details>,
     <https://docs.adyen.com/api-explorer/Checkout/71/post/paymentMethods> — v71 endpoints,
     optional `Idempotency-Key` header (max 64 chars), `details.redirectResult`, `paymentData`,
     `channel` in {iOS, Android, Web}, `amount.{currency,value}` (pages were only partly
     retrievable; response schemas were not visible).
   - <https://docs.adyen.com/development-resources/api-idempotency/> — `idempotency-key`
     header, 64-char max, company-account scope, 7–14 day validity, echo of the key on the
     response, `transient-error: true` = retry with the same key, 703 (HTTP 503, transient),
     704 (HTTP 422 or 409, already processed / in progress).
   - <https://docs.adyen.com/online-payments/build-your-integration/payment-result-codes/> —
     result codes (Received, Pending, PresentToShopper, Authorised, Refused, Cancelled, Error,
     PartiallyAuthorised, ChallengeShopper, IdentifyShopper, RedirectShopper, ...).
   - <https://docs.adyen.com/development-resources/response-handling/> — error body
     `status`, `errorCode`, `message`, `errorType`, optional `pspReference`.

Not read before the first working call: `src/`, `tests/`, `fixtures/`, `FRICTION.md`,
`PROVENANCE.md` of this SDK.

## Predictions

Common to every call (from README + rustdoc):

- Construction: `Checkout::sandbox("<api key>", SendPolicy::default())?.with_base_url(server.url())?`,
  then `let checkout = api.v71();`. `with_base_url` keeps the `/v71` scope prefix ("replacing
  the base does not remove the version"), so paths are `/v71/...` on the loopback server.
- Every endpoint is `POST`, has no query string, and is sent with headers
  `x-api-key: <api key>`, `idempotency-key: <key argument>`, `content-type: application/json`
  and (Json fallback) `accept: application/json`. No `transient-error`/retry headers on the
  request. Body is JSON; **wire field names are camelCase** — this is my inference from Adyen's
  docs (`merchantAccount`, `countryCode`, `returnUrl`, `paymentMethod`, `paymentData`), the
  rustdoc does not state the serde names. I predict `None` fields are omitted, not `null`, and
  `Web::Web` serialises as `"Web"`.
- Success: `Ok(Response { value, meta, retry })`, `meta.status() == 200`,
  `retry.attempts == 1` without retries. The success response **must echo exactly one
  `idempotency-key` equal to the sent key**; without the echo I predict
  `Error::Reconciliation` with `reason == GuaranteeNotConfirmed` and `meta: Some(..)` — even for
  `paymentMethods` (the policy appears to be endpoint-agnostic).
- Non-2xx: `Error::Provider(ProviderFailure { class, document: Ok(Problem{..}) | Err(..),
  transient, retry, meta })`; `class` follows the HTTP status, not the body's `status`. A
  malformed error body still gives `Error::Provider` with `document: Err(..)`.
- A 2xx with an undecodable body (bad JSON, wrong content type): `Error::Reconciliation`
  (README: "Unreadable success documents ... also require reconciliation"); I predict reason
  `UncertainDelivery` with `cause: Some(concord error)`.
- Secrets: `format!("{err:?}")`, `format!("{err}")` and `format!("{response:?}")` never contain
  the API key, the idempotency key, the `refusalReason` or provider `message` text.

### Workflow 1 — list payment methods, submit a sandbox payment

1. `checkout.payment_methods("methods-<uuid>", MethodsRequest { merchant_account, amount,
   country_code: Some("NL"), shopper_locale: Some("nl-NL"), channel: Web::Web }).send().await`
   - Wire: `POST /v71/paymentMethods`, no query, the common headers, body
     `{"merchantAccount":"TestMerchantNL","amount":{"currency":"EUR","value":2495},"countryCode":"NL","shopperLocale":"nl-NL","channel":"Web"}`.
   - Returns `MethodsResponse { payment_methods: Vec<AvailableMethod> }`; `kind` is the
     provider's `type` ("scheme", "ideal"), `name` the display name, everything else
     (`brands`, `issuers`) in `attributes`. One complete response, no pagination end to handle.
     A response without `paymentMethods` gives an empty vector.
   - Fails: 401 `{"status":401,"errorCode":"000","errorType":"security"}` →
     `Error::Provider` with `class == Authentication`, `transient == false`,
     `retry.attempts == 1`, no automatic retry.
2. Choose iDEAL issuer from `attributes["issuers"][i]["id"]`, then
   `checkout.payments("payment-<uuid>", PaymentRequest { ..., payment_method:
   SandboxMethod::Ideal { issuer }, channel: Web::Web })`, keep the operation and `.clone().send()`.
   - Wire: `POST /v71/payments`, body
     `{"merchantAccount":..,"amount":{..},"reference":"order-7731","returnUrl":"https://shop.example/checkout/return?order=7731","paymentMethod":{"type":"ideal","issuer":"1121"},"channel":"Web"}`.
   - Returns `resultCode: "RedirectShopper"` plus `action { type:"redirect", method:"GET", url,
     paymentMethodType:"ideal" }` → `state() == ActionRequired`; `action.kind == "redirect"`,
     URL/method in `action.fields`; `psp_reference` may be absent.
   - Card path: `SandboxMethod::test_card("4111111111111111", ..)` → `Err(Error::InvalidInput)`
     with no wire request; `test_card("test_4111111111111111", "test_03", "test_2030",
     "test_737")` → `paymentMethod: {"type":"scheme","encryptedCardNumber":"test_4111...",...}`.
     A refused card in HTTP 200 → `Ok` with `state() == Unsuccessful`, `refusal_reason` set but
     not in Debug.
   - Required failure: first attempt `503` + `transient-error: true` + `errorCode 703` →
     automatic retry after ≥100 ms with **byte-identical body, same key header, same api key**;
     second attempt 200 → `Ok`, `retry.attempts == 2`. If every attempt (3 by default) gets 703 →
     `Error::Reconciliation { reason: GuaranteeUnavailable, provider: Some(..) }`.
   - Caller retry with the same key but a changed amount → `Error::Reconciliation { reason:
     ChangedRequest }` and **no** new wire request. Same key, identical clone → allowed (sent).

### Workflow 2 — continue an existing payment with `/payments/details`

1. From the payment's action: `let mut d = DetailsRequest::redirect(redirect_result);
   d.payment_data = action.payment_data;` then
   `checkout.details("details-<uuid>", d).send().await`.
   - Wire: `POST /v71/payments/details`, the common headers with a **new** key, body
     `{"details":{"redirectResult":"<opaque>"}}` (plus `"paymentData"` only if `Some`).
   - Returns `PaymentResponse`: `resultCode: "Authorised"`, `pspReference` → `state() ==
     Authorised`; or `"Received"`/`"Pending"` → `Pending` (asynchronous; do not resubmit);
     an unknown code ("PartiallyAuthorised") → `Other`; an action in the continuation →
     `ActionRequired` regardless of resultCode.
   - Fails:
     - 422 `errorCode 704` (already processed / in progress), no `transient-error` →
       `Error::Reconciliation { reason: InProgress, provider: Some(ProviderFailure{class:
       Validation, ..}) }`, attempts 1.
     - The server drops the connection without a head (transport failure) on every attempt →
       automatic retries with identical bytes, then `Error::Reconciliation { reason:
       UncertainDelivery, cause: Some(..) }`, `retry.attempts == 3`. The caller then may replay
       the **identical** operation clone with the same key (allowed, reaches the wire), but a
       changed body under that key is refused locally (`ChangedRequest`).
     - Expired guarantee: with `SendPolicy::default().with_guarantee_window(short)`, a replay
       after the window → `Error::Reconciliation { reason: GuaranteeExpired }` and no wire
       request.
     - A concurrent call with the same key while the first is in flight →
       `Error::Reconciliation { reason: InFlight }`, no second wire request.

## First working call

`first_call_lists_methods` (later folded into
`workflow1_methods_then_ideal_payment_survives_transient_703`) compiled and passed on the first
build: `Checkout::sandbox(API_KEY, SendPolicy::default())?.with_base_url(server.url())?.v71()
.payment_methods(key, MethodsRequest{..}).send().await`. What it took:

- The README example's shape, copied almost verbatim (import list included).
- Knowing from the README that a **successful response must echo the idempotency key**, so the
  loopback fixture answers `Reply::json(200, ..).header("idempotency-key", <request key>)`.
  Without that echo the very first call would have failed with
  `Reconciliation(GuaranteeNotConfirmed)` (confirmed later).
- `TestServer::url()` as the base; no `reqwest` import, no `concord` import, no source lookup.

Observed wire for that call (header lines in wire order):

```
POST /v71/paymentMethods
x-api-key: <api key> | idempotency-key: methods-... | content-type: application/json
accept: application/json | host: 127.0.0.1:<port> | content-length: 112
{"merchantAccount":"TestMerchantNL","amount":{"currency":"EUR","value":2495},"countryCode":"NL","channel":"Web"}
```

## Application tests (`tests/first_use.rs`, 13 tests, all against `examples/testserver`)

Workflow 1 (methods → payment):
- `workflow1_methods_then_ideal_payment_survives_transient_703` — methods list (scheme, ideal,
  klarna), issuer chosen from `attributes["issuers"]`, iDEAL payment whose first attempt is
  503/703 `transient-error: true`; asserts 2 attempts, first wait ≥ 100 ms, byte-identical body
  and headers on both attempts, echoed key in `meta`, `RedirectShopper` → `ActionRequired`,
  redirect fields; a deliberate identical caller retry reaches the wire; a changed amount under
  the same key is refused locally (`ChangedRequest`); Debug/Display redaction.
- `workflow1_card_tokens_refusal_in_200_and_input_guards` — raw PAN refused with no I/O; `test_`
  tokens serialised as `scheme` + `encrypted*`; `Refused` in HTTP 200 → `Unsuccessful`, refusal
  data retained but not in Debug.
- `workflow1_methods_failures_are_classified` — 401 → `Provider{Authentication}` with parsed
  document, 1 attempt; 200 without echo → `GuaranteeNotConfirmed` with head retained; 200 with
  non-JSON body → `UncertainDelivery` with cause; `{}` → empty list.
- `workflow1_exhausted_703_revokes_the_key_and_other_inputs_are_refused` — 3×703 →
  `GuaranteeUnavailable` with the final provider failure; a later identical retry →
  `GuaranteeRevoked` (no wire); a second credential sharing the policy with the same key and
  body → `ChangedRequest` (no wire); keys `""`, `"has spaces"`, 65 chars → `InvalidInput`; a
  64-char key is accepted; a transient 500 with a truncated document stays `Error::Provider`
  with `document: Err(..)` after 3 attempts.

Workflow 2 (continue with `/payments/details`):
- `workflow2_redirect_return_is_submitted_after_a_dropped_connection` — payment → redirect
  action; details with `redirectResult` and a new key; first attempt hung up (no head), second
  200 `Received` → `Pending`; identical bytes on both attempts; body has no `paymentData`.
- `workflow2_payment_data_is_forwarded_and_a_chained_action_is_surfaced` — action with
  `paymentData` forwarded; continuation `Pending` + `await` action → `ActionRequired`; second
  details (`payload`, new key, step-2 `paymentData`) → `Authorised` with `pspReference`.
- `workflow2_704_in_progress_requires_reconciliation` / `..._as_http_409_is_a_conflict_in_progress`
  — 704 under 422 and 409 → `InProgress`, class follows real status (`Validation`/`Conflict`),
  body `status` not authoritative, 1 attempt, provider message not in Debug/Display.
- `workflow2_uncertain_delivery_allows_only_identical_replay` — 3 hang-ups →
  `UncertainDelivery`, `meta: None`, `stop: Attempts`; changed body under that key refused; the
  identical clone later completes `Authorised`; all 4 wire requests identical.
- `workflow2_expired_guarantee_refuses_replay` — window 400 ms; replay after 500 ms →
  `GuaranteeExpired`, nothing sent; extending the window to 30 days is rejected.
- `workflow2_guarantee_expiring_during_backoff_stops_before_dispatch` — window 50 ms, default
  100 ms backoff → `GuaranteeExpired` with one wire send and `attempts == 2`.
- `workflow2_concurrent_same_key_is_refused_while_in_flight` — first call held by a `Gate`;
  second same-key call → `InFlight`, not sent; first completes after the gate opens.
- `workflow2_cancelled_call_keeps_the_binding_and_can_be_replayed` — caller timeout drops the
  future; a changed body under the key is refused; the identical operation is then accepted.

Results (2026-11-07, `CARGO_BUILD_JOBS=4`, worktree `target/`):
`cargo test --locked --offline -p concord-sdk-adyen-checkout` → first_use 13/13, the author's
protocol 13/13 and workflows 11/11, doc-test 1/1; `--test first_use` repeated 8× without a
failure; `cargo clippy --locked --offline -p concord-sdk-adyen-checkout --all-targets -- -D
warnings` clean; `rustfmt --check` clean. `Cargo.lock` unchanged (package already a member and
locked; no new crates). Live provider calls: **NOT_RUN**.

## Friction incidents (in order)

1. **Unlinked, non-re-exported types.** The SDK rustdoc (`--no-deps`) shows `ResponseMeta`,
   `Report`, `Policy` and the Concord `Error` as bare names. Only `concord` is re-exported, so
   `concord_retry::{Policy, Report, StopReason}` (needed for `SendPolicy::with_retry` and to
   assert `retry.stop`) require the application to depend on `concord-retry` directly with a
   compatible version. I generated `concord`/`concord-retry` rustdoc to find them.
2. **Two `Error`s.** Endpoint `prepare()` returns `concord::Error` (docs: bare `Error`,
   `ErrorKind::Build`) while `send()` returns this SDK's `Error`; `Error::Concord(Box<Error>)`
   reads as recursive in rustdoc.
3. **Wire field names undocumented.** No SDK doc states the JSON names (`merchantAccount`,
   `returnUrl`, `paymentMethod.type`, `encryptedCardNumber`, `details.redirectResult`,
   `paymentData`) or that `None` is omitted; I predicted them from Adyen's docs (all correct).
4. **Generated naming/boilerplate.** The endpoint argument is `key` ("key: required value") next
   to an `api_key` client value; only `set_key` says "Sent as: header idempotency-key". The
   generated `send()` text "with the client's send policy the client value policy" is
   ungrammatical.
5. **Key + echo on a read-like call.** `payment_methods` also needs a unique idempotency key, and
   its success must echo it, else `Reconciliation(GuaranteeNotConfirmed)`. Stated in the README,
   not on the endpoint; any proxy that strips the header turns method listings into
   reconciliation errors.
6. **Display wording.** A methods-list failure displays as "payment requires reconciliation
   (...)" although no payment was involved.
7. **Ambiguous README sentence.** "Missing echoes on errors do not grant replay" — I could not
   predict from it; observed: a transient 703 *without* an echo is still retried automatically.
8. **703 exhaustion silently revokes the key.** After `GuaranteeUnavailable`, an identical retry
   later gives `GuaranteeRevoked` with no wire send. Neither the README nor
   `ReconciliationReason` docs say that 703 exhaustion revokes; Adyen's idempotency page
   suggests pausing and retrying later. The application has no SDK path to "retry later with the
   same key" and must reconcile and choose a new key itself.
9. **Escape path looks like validation.** I first asserted `payments(64-char key).prepare().is_ok()`
   to show the key is accepted, then remembered the README: `prepare()` skips SDK input policy,
   so it proves nothing. Replaced with a real `send()`.
10. **Evidence dropped on expiry during backoff.** `GuaranteeExpired` raised before the second
    attempt carries `provider: None`, `meta: None`, `cause: None` although a 503/703 was
    received; `retry.attempts == 2` with one wire send (attempt counting is documented).
11. **5xx on a payment is not flagged.** An exhausted transient 500 on `POST /payments` is
    `Error::Provider` and `err.reconciliation()` is `None`; an application that uses
    `reconciliation()` as its "unsafe to submit a fresh payment" signal would treat it as
    definitive. The README's general warning covers it, the API shape does not.
12. **Minor.** `DetailsRequest` has only `redirect()`; other detail keys (`payload`) need a struct
    literal. Issuers/brands are untyped `attributes` JSON (`["issuers"][0]["id"]`).

No source lookup was needed; no SDK `src/` was edited. Shared crates were not edited; what I
would change there (recorded only): generated rustdoc should name the endpoint argument's wire
role at the method (item 4) and avoid the "the client value policy" phrasing.

## Predictions vs observed

| Prediction | Observed |
| --- | --- |
| `POST /v71/...`, no query, `x-api-key`, `idempotency-key`, `content-type`/`accept: application/json` | Matched exactly (header order: x-api-key, idempotency-key, content-type, accept, host, content-length) |
| camelCase body, `None` omitted, `"Web"` | Matched for all three endpoints |
| iDEAL `{"type":"ideal","issuer":..}`; card `scheme` + `encrypted*` | Matched |
| Raw PAN → `InvalidInput`, no I/O | Matched |
| Success needs one identical echo, else `GuaranteeNotConfirmed` (also for methods) | Matched, `meta` retained (200); a differing echo gives the same |
| Malformed 2xx → `UncertainDelivery` with cause | Matched; a `text/html` 200 does the same; no retry |
| 401 → `Provider{Authentication}`, 1 attempt | Matched |
| Missing `paymentMethods` → empty vec | Matched |
| 703 transient → retry ≥100 ms, identical bytes, `attempts == 2` | Matched (default waits 100 ms, 200 ms) |
| 3×703 → `GuaranteeUnavailable` with provider | Matched |
| (not predicted) later identical retry after 703 exhaustion | **`GuaranteeRevoked`, nothing sent** |
| (not predicted) transient 703 without echo | Still retried automatically |
| Changed body, same key → `ChangedRequest`, no wire | Matched (payments, details, after cancellation) |
| (not predicted) other credential, same key/body, shared policy | `ChangedRequest`, nothing sent |
| (not predicted) key validation | 1..=64 visible ASCII, else `InvalidInput` |
| Details body `{"details":{..}}` + `paymentData` only when `Some` | Matched |
| `Received`/`Pending` → `Pending`; action wins → `ActionRequired`; `PartiallyAuthorised` → `Other`; `Refused` in 200 → `Unsuccessful` | Matched |
| 704/422 → `InProgress`, `Validation`, 1 attempt | Matched; 704/409 → `Conflict` |
| Hang-ups → `UncertainDelivery`, 3 attempts; identical replay allowed | Matched; `meta: None`, `stop: Attempts` |
| Replay after window → `GuaranteeExpired`, nothing sent | Matched |
| (not predicted) expiry during backoff | `GuaranteeExpired`, provider/meta/cause dropped |
| Concurrent same key → `InFlight` | Matched |
| Malformed error body → `Provider` with `document: Err` | Matched (transient 500, 3 attempts, no reconciliation) |
| Debug/Display withhold API key, idempotency key, refusal reason, provider message | Matched for every case asserted (also the redirect URL query) |

## Assistance label

**docs-only.** Read: the SDK README and generated rustdoc; testserver, `concord` and
`concord-retry` rustdoc; the Adyen pages cited above. Not read: this SDK's `src/`, `tests/`
contents, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`. Seen incidentally: the *file names* under
`tests/` (`common/`, `protocol.rs`, `workflows.rs`) and the `fixtures/` directory name from an
`ls`, and the author commit subject in `git log`. The author's tests were run (not read) as part
of `cargo test -p`.
