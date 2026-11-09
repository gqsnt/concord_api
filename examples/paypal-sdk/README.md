# concord-sdk-paypal

A bounded PayPal REST client for the **sandbox**, built on [Concord](../../README.md): an access
token from client credentials, then the connected payment workflows below.

1. **Create an order and obtain its current state** — Create order
   (`POST /v2/checkout/orders`), Show order details (`GET /v2/checkout/orders/{id}`).
2. **Capture an approved order and retrieve the resulting capture** — Capture payment for order
   (`POST /v2/checkout/orders/{id}/capture`), Show captured payment details
   (`GET /v2/payments/captures/{id}`).
3. **Authorize now, capture later or release** — create with `OrderRequest::authorize`,
   authorize an approved order, then capture or void its authorization.

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

## Authorize, then capture or release

`OrderRequest::authorize(vec![unit])` requires exactly one purchase unit. After buyer approval,
authorize the order and retain each returned authorization ID and status. Order completion here
means the authorization step completed; check the later capture's status before fulfillment.
Authorize returns `AuthorizedOrder`; `order(id).get_authorized()` reads the same order with
authorization details. The existing `get()` and CAPTURE response types keep their original shape.

```rust,no_run
use concord_sdk_paypal::{
    AuthorizationCaptureRequest, AuthorizationId, AuthorizationStatus, AuthorizeRequest,
    Capture, Error, OrderId, PayPal, RequestId,
};

// Invoke after creating an AUTHORIZE order and completing buyer approval externally.
async fn authorize(paypal: &PayPal, order: OrderId) -> Result<Vec<AuthorizationId>, Error> {
    let result = paypal.order(order).authorize(AuthorizeRequest::new())
        .set_request_id(RequestId::new("authorize-cart-18")?).send().await?;
    Ok(result.authorizations()
        .filter(|authorization| authorization.status == AuthorizationStatus::Created)
        .map(|authorization| authorization.id.clone()).collect())
}

async fn capture_later(paypal: &PayPal, id: AuthorizationId) -> Result<Capture, Error> {
    let captured = paypal.authorization(id)
        .capture(AuthorizationCaptureRequest::new().final_capture(true))
        .set_request_id(RequestId::new("capture-authorization-cart-18")?).send().await?;
    paypal.capture(captured.id).get().send().await
}

// Alternative to capture: release an authorization that has not been fully captured.
async fn release(paypal: &PayPal, id: AuthorizationId) -> Result<(), Error> {
    paypal.authorization(id).void()
        .set_request_id(RequestId::new("void-cart-18")?).send().await
}
```

Authorization capture accepts an optional positive `amount(Money)` and `final_capture(bool)`.
The flag defaults to PayPal's `false` when omitted. Void sends no body and returns `()` on a
successful acknowledgement (`200` or `204`); it does not decode a returned representation.
Choose a fresh, persistent request ID for each logical operation; the example IDs are illustrative.
Current endpoint references and their read date are in [MAINTENANCE.md](MAINTENANCE.md).

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
  the generated client takes its base from the session. Native policy calls with a different
  origin send nothing and fail with `Error::Destination`.

## Failure semantics

| Outcome | Reads (show order, show capture) | Supported writes **with** `RequestId` | Supported writes **without** `RequestId` |
|---|---|---|---|
| Invalid input (IDs, money, units) | `Error::Invalid`, nothing sent | same | same |
| Token not obtainable | `Error::Token`, API request not sent | same | same |
| Transport failure, no response (timeout, reset) | repeated within the read bounds, then `Error::Concord` | repeated with the same ID and body, then `Error::Uncertain` | sent once: `Error::Uncertain` |
| `5xx` | repeated, then `Error::Http` | repeated, then `Error::Uncertain` | sent once: `Error::Uncertain` |
| `401` | repeated once with a new token, then `Error::Http` | same | sent once: `Error::Http`; token discarded |
| other `4xx` (`400`, `403`, `404`, `409`, `415`, `422`, `429`) | `Error::Http`, not repeated | `Error::Http` on the first attempt; after an uncertain attempt `Error::Uncertain` (`LaterRefusal`) | `Error::Http` |
| `2xx` whose body cannot be read (wrong media type, malformed, truncated, over 1 MiB) | `Error::Concord` (`Metadata`, `Decode`, `Transport`, `Limit`) | `Error::Uncertain` (`UnreadableSuccess`) | same |
| Overall deadline cancels an attempt in flight (only if you set one) | `Error::Deadline` (or the last answer) | `Error::Uncertain` (`Interrupted`) | same |

The unreadable-body row applies to typed responses. Void consumes only the successful status;
any returned representation is ignored.

- **Request-ID support is per endpoint.** This SDK permits repeats for create order, order
  authorize/capture, and authorization capture/void, only when the caller supplied an ID.
  Read or write, and request-ID support, are one exhaustive `match` over the generated
  `PayPalOperation` (`src/policy.rs`): an endpoint added to the declaration does not compile
  until it is classified there.
  The create/order-capture references describe 6-hour default retention. Do not assume that
  retention for Payments v2 or that every PayPal endpoint supports request IDs. Default write
  bounds remain 2 attempts with identical method, target, ID and body; check operation-specific
  retention before a caller-initiated replay outside these short automatic retries.
- **A timed-out authorize, capture or void is uncertain until resolved.** `Error::Uncertain` says which operation, how many
  attempts, the last observation, and the `RequestId` if there was one. With an ID, send the same
  call again with it (`uncertain.request_id()`) within the retention window to learn the stored
  outcome. Without one, read `paypal.order(id).get_authorized()` for authorizations and captures
  (`get()` keeps the existing CAPTURE response view); use provider reconciliation for later authorization actions that this slice
  cannot retrieve. Never blindly repeat a write without an ID.
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

Buyer approval UI (the SDK only returns `approval_url()`), refunds, payouts, reauthorization,
authorization detail retrieval, order updates (`PATCH`), payment sources and vaulting, webhooks,
partner headers (`PayPal-Partner-Attribution-Id`, `PayPal-Auth-Assertion`), `Prefer:
return=minimal` on typed writes (void consumes only the acknowledgement), and blanket idempotency: no
other endpoint is treated as accepting `PayPal-Request-Id`.

The credential session supplies the destination; a handle cannot select another base:

```compile_fail
use concord_sdk_paypal::PayPal;
fn redirect(api: PayPal) { let _ = api.with_base_url("https://evil.example"); }
```
