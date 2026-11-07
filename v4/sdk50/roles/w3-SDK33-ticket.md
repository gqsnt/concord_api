
Callers need delayed capture. Square's Payments API lets a payment be created with
`autocomplete: false` (an authorization only) and later completed with
`POST /v2/payments/{payment_id}/complete` or voided with `POST /v2/payments/{payment_id}/cancel`;
complete accepts an optional `version_token` so a stale caller cannot complete a payment that
changed. Read the CompletePayment and CancelPayment references and the delayed-capture guide and
cite them with the date:
- https://developer.squareup.com/reference/square/payments-api/complete-payment
- https://developer.squareup.com/reference/square/payments-api/cancel-payment
- https://developer.squareup.com/docs/payments-api/take-payments/card-payments/delayed-capture

Add: creating an authorization-only payment, completing it (with the version token when the caller
has one; a token mismatch is a typed error the caller can act on) and cancelling it. A complete or
cancel that times out is uncertain — the caller re-reads the payment; the SDK never retries it
automatically. Keep create, retrieve and listing unchanged for existing callers.
