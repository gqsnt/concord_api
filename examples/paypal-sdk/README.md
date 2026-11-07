# concord-sdk-paypal

A bounded PayPal REST client for the **sandbox**, built on [Concord](../../README.md): an access
token from client credentials, then two connected workflows.

1. **Create an order and obtain its current state** — Create order
   (`POST /v2/checkout/orders`), Show order details (`GET /v2/checkout/orders/{id}`).
2. **Capture an approved order and retrieve the resulting capture** — Capture payment for order
   (`POST /v2/checkout/orders/{id}/capture`), Show captured payment details
   (`GET /v2/payments/captures/{id}`).

Live provider calls: **NOT_RUN**. Every test runs against local fixtures
(`cargo test -p concord-sdk-paypal`); nothing here was checked against a PayPal host. Sources
and fixture derivations are in [PROVENANCE.md](PROVENANCE.md), Concord costs in
[FRICTION.md](FRICTION.md).

## Calling it

```rust,no_run
use concord_sdk_paypal::{
    CaptureRequest, Credentials, Error, Money, OrderRequest, OrderStatus, PayPal,
    PurchaseUnitRequest, RequestId, http_client,
};

async fn checkout(client_id: &str, secret: &str) -> Result<(), Error> {
    let http = http_client().expect("a TLS client");
    let paypal = PayPal::sandbox(http, Credentials::new(client_id, secret)?);

    // Workflow 1: create, then read the current state.
    let unit = PurchaseUnitRequest::new(Money::new("USD", "100.00")?)?.reference_id("cart-17")?;
    let order = paypal
        .orders()
        .create(OrderRequest::capture(vec![unit])?)
        .set_request_id(RequestId::new("create-cart-17")?)
        .send()
        .await?;
    let _send_the_buyer_to = order.approval_url(); // buyer approval is outside this SDK
    let current = paypal.order(order.id.clone()).get().send().await?;

    // Workflow 2: once the buyer approved, capture and read the capture.
    if current.status == OrderStatus::Approved {
        let captured = paypal
            .order(order.id.clone())
            .capture(CaptureRequest::new())
            .set_request_id(RequestId::new("capture-cart-17")?)
            .send()
            .await?;
        for capture in captured.captures() {
            let shown = paypal.capture(capture.id.clone()).get().send().await?;
            println!("{} {}", shown.id, shown.status.as_str());
        }
    }
    Ok(())
}
```

- `PayPal::sandbox(http, credentials)` targets `https://api-m.sandbox.paypal.com`.
  `PayPal::builder(..)` chooses another base (`.base_url(..)`: live, a local fixture), the
  repetition bounds (`.reads(..)`, `.writes(..)`, as `concord_retry::Policy`), the token refresh
  margin and the token clock.
- Pass a `reqwest::Client` without transport retries and without redirects — `http_client()`
  builds one (30 s per request). With retries or redirects in the client, the write semantics
  below do not hold.
- The generated client is the API: `paypal.orders().create(..)`, `paypal.order(id).get()`,
  `paypal.order(id).capture(..)`, `paypal.capture(id).get()`, each with its generated setters
  and `send()`, which returns `Result<_, concord_sdk_paypal::Error>`. `prepare()` returns the
  declared request **without** `Authorization` (the token is applied by `send()`).

## Tokens

Tokens come from `POST /v1/oauth2/token` with HTTP Basic client ID and secret and the form
`grant_type=client_credentials`. There is no refresh token: the SDK never asks for or uses one.

- One token per client (and its clones), cached until `expires_in` minus the refresh margin
  (default 60 s), then obtained again with the same grant.
- Concurrent calls that need a token wait for one shared acquisition (single flight); if it
  fails, they all receive that failure (`Error::Token`) and nothing is cached, so the next call
  tries again. A token document that is not `Bearer`, or whose lifetime is not longer than the
  margin, is refused (`TokenError::Unusable`).
- A `401` on an API call discards the token that call used; see below for whether the call is
  repeated.
- The token and every API request go to the origin given at construction. A client moved with
  the generated `with_base_url` sends nothing and fails with `Error::Destination`.

## Failure semantics

| Outcome | Reads (show order, show capture) | Create / capture **with** `RequestId` | Create / capture **without** `RequestId` |
|---|---|---|---|
| Invalid input (IDs, money, units) | `Error::Invalid`, nothing sent | same | same |
| Token not obtainable | `Error::Token`, API request not sent | same | same |
| Transport failure, no response (timeout, reset) | repeated within the read bounds, then `Error::Concord` | repeated with the same ID and body, then `Error::Uncertain` | sent once: `Error::Uncertain` |
| `5xx` | repeated, then `Error::Http` | repeated, then `Error::Uncertain` | sent once: `Error::Uncertain` |
| `401` | repeated once with a new token, then `Error::Http` | same | sent once: `Error::Http`; token discarded |
| other `4xx` (`400`, `403`, `404`, `409`, `415`, `422`, `429`) | `Error::Http`, not repeated | `Error::Http` on the first attempt; after an uncertain attempt `Error::Uncertain` (`LaterRefusal`) | `Error::Http` |
| `2xx` whose body cannot be read (wrong media type, malformed, truncated, over 1 MiB) | `Error::Concord` (`Metadata`, `Decode`, `Transport`, `Limit`) | `Error::Uncertain` (`UnreadableSuccess`) | same |
| Overall deadline cancels an attempt in flight (only if you set one) | `Error::Deadline` (or the last answer) | `Error::Uncertain` (`Interrupted`) | same |

- **Only Create order and Capture payment for order document `PayPal-Request-Id`**, and only a
  call that carries one may be repeated. PayPal keeps the ID for 6 hours by default: within
  that window the same call with the same ID is not performed twice, and a repeated capture
  answers `200` with the stored result. Default write bounds: 2 attempts (PayPal's responses
  page recommends repeating a timed-out or `5xx` capture at least once with the same ID).
- **A timed-out capture is uncertain.** `Error::Uncertain` says which operation, how many
  attempts, the last observation, and the `RequestId` if there was one. With an ID, send the same
  call again with it (`uncertain.request_id()`) within the retention window to learn the stored
  outcome. Without one, read the order (`paypal.order(id).get()`): `COMPLETED` with a capture
  means it happened. Never send a capture without an ID twice blindly.
- **`429` is never repeated.** PayPal publishes no rate-limit quota and documents no
  `Retry-After`; it limits traffic it considers abusive until it is satisfied. `HttpError` reports
  `is_rate_limited()` and any `Retry-After` that does arrive (`retry_after()`), and the caller
  decides. Caching tokens (done here) and not polling are PayPal's own advice.
- `Error::Http` keeps the status even when the error document cannot be read
  (`document()` is then the Concord error: wrong media type, malformed, over 64 KiB).
  `name()`, `has_issue("ORDER_NOT_APPROVED")` and `debug_id` come from the document.
- A `COMPLETED` order is not proof of payment: check each capture's status
  (`order.captures()`), as PayPal's order status reference says.

## Redaction

`Debug` of `Credentials`, the client, its session and `Builder` shows no client ID, secret or
token. `Error`'s `Display` and `Debug` show statuses, error names, issue codes, `debug_id`,
operation names and Concord's redacted errors, never a credential, an access token, a body, a
provider `message`, `description` or echoed `value` (read them with the `*_unredacted`
accessors). Payer and payment-source details in orders are not decoded.

## Excluded

Buyer approval UI (the SDK only returns `approval_url()`), refunds, payouts, authorize intent
and authorization captures, order updates (`PATCH`), payment sources and vaulting, webhooks,
partner headers (`PayPal-Partner-Attribution-Id`, `PayPal-Auth-Assertion`), `Prefer:
return=minimal` (every write asks for the full representation), and blanket idempotency: no
other endpoint is treated as accepting `PayPal-Request-Id`.
