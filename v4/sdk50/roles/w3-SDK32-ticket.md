Callers need authorize-then-capture. An order created with `intent: AUTHORIZE` is authorized after
buyer approval with `POST /v2/checkout/orders/{id}/authorize`, which returns an authorization; the
authorization is later captured with `POST /v2/payments/authorizations/{authorization_id}/capture`
or released with `POST /v2/payments/authorizations/{authorization_id}/void`. Read the Orders v2
authorize reference and the Payments v2 authorizations reference (capture, void, and which of
these accept `PayPal-Request-Id`) and cite them with the date:
- https://developer.paypal.com/docs/api/orders/v2/
- https://developer.paypal.com/docs/api/payments/v2/

Add: creating an order with intent AUTHORIZE, authorizing it (the caller gets the authorization id
and status), capturing an authorization and voiding one. Each write follows the SDK's existing
rule: repeated only when the operation documents a request id and the caller supplied one, with
identical bytes; a timed-out authorize, capture or void is uncertain. Keep the existing CAPTURE
flow (create → capture order → retrieve capture) unchanged for existing callers.
