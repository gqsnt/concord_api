# Provenance

Live provider calls: **NOT_RUN**. No request was sent to any PayPal host. Every expectation below
comes from the official documentation listed here, or is labelled as an SDK rule, inferred or
synthetic.

## Documentation read (all retrieved 2026-11-07)

The pages were read through a fetch tool that returns a summary of the page, not its verbatim
text; facts are recorded as summarized, with short phrases only. No documentation JSON example
was copied into a fixture. D1–D4 are the slice's cited pages; the others were read to resolve
what those pages leave to the API reference.

| # | Page | Facts taken |
|---|---|---|
| D1 | https://developer.paypal.com/api/rest/authentication | Token endpoint `https://api-m.sandbox.paypal.com/v1/oauth2/token`, `POST`, HTTP Basic with client ID as user and secret as password, `Content-Type: application/x-www-form-urlencoded`, body `grant_type=client_credentials`. Response fields `scope`, `access_token`, `token_type` (`Bearer`), `app_id`, `expires_in` (seconds the token is valid), `nonce`. When the token expires, call the token endpoint again. API calls send `Authorization: Bearer <token>`. Not on the page: error responses, caching, refresh tokens. |
| D2 | https://developer.paypal.com/api/rest/requests/ | Bases: sandbox `https://api-m.sandbox.paypal.com`, live `https://api-m.paypal.com`. `Accept: application/json` for a response body, `Content-Type: application/json` for a request body. `PayPal-Request-Id`: optional, a caller-generated ID the server stores for a period, enforcing idempotency on POST; calls that fail with a network timeout or HTTP 500 can be retried while the ID is stored; check the API reference that an API supports it. Not on the page: `Prefer`, status codes, error format. |
| D3 | https://developer.paypal.com/api/rest/reference/rate-limiting/ | PayPal does not publish a rate-limiting policy; it may temporarily limit traffic that appears abusive, until satisfied. `429` with `RATE_LIMIT_REACHED` (the page labels it "Unprocessable Entity"). No error body example, no `Retry-After` or other header, no backoff guidance, no numbers. Advice: do not poll (use webhooks/IPN); cache OAuth tokens instead of requesting one per transaction. |
| D4 | https://developer.paypal.com/api/rest/integration/orders-api | Flow: create order (returns ID and approval URL), buyer approves at the approval URL, merchant captures. Endpoints `POST v2/checkout/orders`, `GET v2/checkout/orders/ORDER-ID`, `POST v2/checkout/orders/ORDER-ID/capture` (and authorize, update, confirm, track, outside this slice). Not on the page: bodies, statuses, `PayPal-Request-Id`. |
| D5 | https://developer.paypal.com/api/rest/integration/orders-api/errors/ | Common errors by status: 400 `INVALID_REQUEST`, 401 `AUTHENTICATION_FAILURE`, 403 `NOT_AUTHORIZED`, 404 `RESOURCE_NOT_FOUND`, 422 `UNPROCESSABLE_ENTITY`, 500 `INTERNAL_SERVER_ERROR`, 503 `SERVICE_UNAVAILABLE`. Bodies carry `name`, `message`, `debug_id`, `details` (`field`, `location`, `issue`, `description`, sometimes `value`) and `links` (`href`, `rel`, `method`); sample issues `INVALID_PARAMETER_VALUE` ("A parameter value is not valid."), `CARD_EXPIRED`, `PERMISSION_DENIED_FOR_DONATION_ITEMS`. |
| D6 | https://developer.paypal.com/api/rest/reference/orders/v2/errors/ | Issue codes and their categories: `ORDER_NOT_APPROVED` ("Payer has not yet approved the Order for payment.", `UNPROCESSABLE_ENTITY`); `ORDER_ALREADY_CAPTURED` ("If 'intent=CAPTURE' only one capture per order is allowed."); `INVALID_RESOURCE_ID` (`RESOURCE_NOT_FOUND`); `PREVIOUS_REQUEST_IN_PROGRESS` (`RESOURCE_CONFLICT`, on create, authorize, capture: wait and try again); `PERMISSION_DENIED` (`NOT_AUTHORIZED`); `AUTHENTICATION_FAILURE`; `ORDER_COMPLETION_IN_PROGRESS`. The page lists no numeric status codes. |
| D7 | https://developer.paypal.com/api/rest/responses/ | Status codes 200/201/202/204 success; 400 malformed; 401 authentication failure (missing or invalid token or client credentials); 403 no permission; 404; 405; 406; 409 conflicts with another request in progress, retry later; 415 media type not supported; 422 business validation; 429 rate limit; 500; 503 maintenance. Error body fields as D5. Identity errors use `error` and `error_description`; an invalid client ID returns 401 with `invalid_client` / "Client Authentication failed". A 5xx error or network timeout from `/capture` may still have created a transaction: repeat the same call at least once with the same `PayPal-Request-Id`. |
| D8 | https://developer.paypal.com/docs/api/orders/v2/orders-create | Create order headers: `PayPal-Request-Id` optional, 1–10000 characters, "for idempotency purposes"; `Prefer`: `return=minimal` (default: id, status, HATEOAS links) or `return=representation` (complete resource); body `application/json`; Bearer client-credentials auth. The page text was truncated before its responses. |
| D9 | https://developer.paypal.com/docs/api/orders/v2/orders-capture | Capture headers: `PayPal-Request-Id` optional, 1–10000, pattern any; `Prefer` as D8; path `id` required, pattern `^[A-Z0-9]+$`, 1–36 characters; body `application/json`, its `payment_source` optional. Truncated before responses and samples. |
| D10 | https://raw.githubusercontent.com/paypal/paypal-rest-api-specifications/main/openapi/checkout_orders_v2.json (PayPal's published Orders v2 OpenAPI specification; read in parts) | `orders.create`: 200 "successful idempotent request", 201 (minimal by default), 202, 400, 422; `PayPal-Request-Id`: "The server stores keys for 6 hours", up to 72 hours by request to an Account Manager, mandatory for single-step create with a payment source. `orders.get`: 200; optional `fields` query. `orders.capture`: request body not required, every request example is `{}`; 200 idempotent replay ("a duplicate retry returns 200"), 201, 202, 207 mixed outcomes, 400, 401, 403, 404, 409 conflict, 422, 500; same `PayPal-Request-Id` text. 201 example structure: `id`, `payment_source`, `purchase_units[].payments.captures[]` (`id`, `status`, `amount`, `final_capture`, `seller_protection`, `seller_receivable_breakdown`, `create_time`, `update_time`, `links` `self`/`refund`), `payer`, order `links` `self`. `checkout_payment_intent`: `CAPTURE`, `AUTHORIZE`. |
| D11 | https://developer.paypal.com/api/orders/v2/definitions/order_status | `CREATED`, `SAVED`, `APPROVED`, `VOIDED`, `COMPLETED` (check each capture's status before fulfilling), `PAYER_ACTION_REQUIRED` (follow `payer-action`). Found through a web search restricted to developer.paypal.com. |
| D12 | https://developer.paypal.com/api/orders/v2/definitions/money | `currency_code` required, exactly 3 characters; `value` required, pattern `^((-?[0-9]+)|(-?([0-9]+)?[.][0-9]+))$`, at most 32 characters. |
| D13 | https://developer.paypal.com/api/orders/v2/definitions/purchase_unit_request | `reference_id` 1–256 (PayPal uses `default` for a single unit without one; required for several units when patching); `amount` required and positive; `description` 1–3000 (kept to 127); `custom_id` 1–255; `invoice_id` 1–127, unique per account by default. |
| D14 | https://developer.paypal.com/api/orders/v2/definitions/order_request | `intent` required (`CAPTURE`, `AUTHORIZE`); `purchase_units` required, 1–10 items; `payer` deprecated; `payment_source` optional. |
| D15 | https://developer.paypal.com/api/orders/v2/definitions/order/ | `id`, `create_time`, `update_time` (RFC 3339), `payment_source`; the text was truncated before `status`, `purchase_units`, `links`. |
| D16 | https://developer.paypal.com/docs/api/payments/v2/captures-get | Show captured payment details: path `capture_id`; responses 200/401/403/404/500; example fields `id`, `amount`, `final_capture`, `seller_protection`, `seller_receivable_breakdown`, `invoice_id`, `status` (`COMPLETED`), `supplementary_data.related_ids.order_id`, `payee`, `create_time`, `update_time`, `links` `self`/`refund`/`up`. |
| D17 | https://raw.githubusercontent.com/paypal/paypal-rest-api-specifications/main/openapi/payments_payment_v2.json (PayPal's published Payments v2 OpenAPI specification; read in parts) | `captures.get` parameters are `capture_id` and authorization only (no `PayPal-Request-Id`); responses 200/401/403/404/500. `capture_status`: `COMPLETED`, `DECLINED`, `PARTIALLY_REFUNDED`, `PENDING`, `REFUNDED`, `FAILED` (no per-value descriptions); `capture_status_details.reason` enumeration (`PENDING_REVIEW`, `ECHECK`, `OTHER`, ...). |
| — | https://developer.paypal.com/docs/api/orders/v2/, https://developer.paypal.com/docs/api/orders/v2.md, https://developer.paypal.com/docs/api/payments/v2/captures-get.md | Read; index or summary pages pointing to the specification, none of the facts above. |

## Rules this SDK adds or infers

| Rule | Status |
|---|---|
| Only Create order and Capture payment for order may be repeated, and only when the call carries a `PayPal-Request-Id` | from D2 ("check the API reference"), D8, D9, D10; Show captured payment details has no such header (D17) |
| A timed-out or 5xx create/capture may have been applied: `Error::Uncertain` | D7 (capture); applied to create by the same reasoning (**inferred**) |
| With a request ID, a write is attempted twice by default | D7 ("at least once"); the count is SDK policy |
| In-process repetition lasts seconds, far inside the 6-hour retention | D10; SDK policy |
| Reads (`GET`) are repeated after a transport failure or a 5xx | SDK policy (safe method); PayPal does not state it |
| `429` is never repeated; any `Retry-After` is reported, not waited for | SDK policy over D3 (no quota, no header documented, limits until PayPal is satisfied) |
| A 4xx on a first write attempt is a definite refusal | **inferred** from D7's meanings (malformed, authentication, permission, not found, conflict with a request in progress, business validation, rate limit) |
| After an uncertain attempt, any later refusal keeps the write uncertain | SDK policy (conservative: a 422 `ORDER_ALREADY_CAPTURED` would even mean the earlier attempt succeeded) |
| A 401 discards the token it used; a repeatable call is sent once more with a new token | SDK policy; D7 says authorization errors are usually token problems |
| A token is replaced 60 s before `expires_in` runs out; one whose lifetime is not longer than that margin is refused | SDK policy over D1 |
| `token_type` must be `Bearer` | D1 |
| No refresh token is requested or used; expiry means a new client-credentials grant | D1 |
| Every write sends `Prefer: return=representation` | SDK choice among D8's values, so the create returns its links and the capture its captures |
| The capture body is `{}` with `Content-Type: application/json` | D10's request examples, D9 |
| Currency codes must be three ASCII capital letters | SDK rule (ISO 4217) beyond D12's length |
| `RequestId`: 1–10000 visible ASCII characters (no spaces) | D8/D9 length; the character rule is the SDK's |
| `CaptureId`: 1–64 visible ASCII characters | SDK rule; D16/D17 state no constraint |
| Several purchase units need distinct `reference_id`s | SDK rule, so captures can be attributed; D13 requires them only for `PATCH` |
| The token and every request must go to the origin given at construction | SDK policy (credential destination) |
| Unknown enumeration values are kept (`Other(..)`), unknown fields ignored; payer and payment-source data are not decoded | SDK choices (forward compatibility, personal data) |

## Fixtures

Every fixture was written for these tests. "Shape" means the field names and nesting follow the
cited page; all identifiers, amounts, tokens, names and addresses are synthetic.

| Fixture | Derivation | Used by |
|---|---|---|
| `token.json`, `token-renewed.json` | shape D1 (all six fields; `expires_in` 32400); synthetic values | workflows, protocol, fake |
| `token-not-bearer.json` | D1 shape with `token_type` `MAC` (synthetic) | protocol |
| `token-short-lived.json` | D1 shape with `expires_in` 30 (synthetic) | protocol |
| `token-invalid-client.json` | D7's `invalid_client` / "Client Authentication failed" identity error | protocol, fake |
| `create-order-request.json` | the expected wire body: D14 (`intent`, `purchase_units`), D13 (`reference_id`, `invoice_id`), D12 (`amount`); synthetic values | protocol |
| `order-created.json` | synthetic; shape D10/D15 with D11 `CREATED` and D4's approval link (`rel` `approve`, plus `self`, `update`, `capture`); unknown `payee` | workflows, protocol, fake |
| `order-approved.json` | synthetic; D11 `APPROVED`, `payment_source.paypal` and `payer` (personal data the SDK does not decode) | fake |
| `order-captured.json` | synthetic; structure of D10's 201 capture example (capture fields, `seller_protection`, `seller_receivable_breakdown`, capture links `self`/`refund`/`up`, order link `self`, `shipping`) | workflows, protocol, fake |
| `capture.json` | synthetic; D16 example fields with D17 `COMPLETED` and `supplementary_data.related_ids.order_id` | workflows, protocol, fake |
| `order-unknown-values.json` | synthetic; values outside D11/D14/D17 enumerations, an unknown field | protocol |
| `order-truncated.json` | synthetic; JSON cut off mid-document | protocol |
| `order-wrong-shape.json` | synthetic; an order without `id` | protocol |
| `error-invalid-request.json` | D5 400 shape and issue `INVALID_PARAMETER_VALUE` with its description; `field`, `value` (synthetic, checked never to be printed), `links` | protocol |
| `error-authentication-failure.json` | D5 401 name; D6 description paraphrased; synthetic `debug_id` | protocol, fake |
| `error-not-authorized.json` | D5 403 name; D6 `PERMISSION_DENIED` description | protocol |
| `error-resource-not-found.json` | D5 404 name; D6 `INVALID_RESOURCE_ID` description | protocol, fake |
| `error-previous-request-in-progress.json` | D6 `RESOURCE_CONFLICT` / `PREVIOUS_REQUEST_IN_PROGRESS`; status 409 from D7/D10 | protocol |
| `error-unsupported-media-type.json` | **synthetic** body for D7's 415 (no documented body; name and issue are assumptions) | protocol |
| `error-order-not-approved.json` | D5 422 name; D6 `ORDER_NOT_APPROVED` description | protocol, fake |
| `error-order-already-captured.json` | D5 422 name; D6 `ORDER_ALREADY_CAPTURED` description | fake |
| `error-rate-limit-reached.json` | D3 name `RATE_LIMIT_REACHED`; message **synthetic** (D3 has no body) | protocol |
| `error-internal-server-error.json` | D5 500 name and message; synthetic `debug_id` and link | workflows, protocol, fake |
| `error-service-unavailable.json` | D5 503 name; message synthetic | protocol |

Other test bodies (HTML pages, unlabelled bodies, oversized padding, a body cut off by the
server) are synthetic and written inline in `tests/protocol.rs`.

The fake service in `tests/support/mod.rs` implements, synthetically: D1's token grant check;
D7's 401 for a request without a valid token; D6's `ORDER_NOT_APPROVED` before approval and
`ORDER_ALREADY_CAPTURED` after; D10's `200` replay of a capture with the same request ID. Buyer
approval (D4) is simulated by the test (`FakePayPal::approve`).

## Concord examples consulted

Recorded in [FRICTION.md](FRICTION.md#examples-consulted). The result is example-assisted.
