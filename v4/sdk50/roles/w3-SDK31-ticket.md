Callers need manual capture. With manual capture configured, an authorised payment is captured
later with `POST /payments/{paymentPspReference}/captures` (the merchant account, the amount — the
full authorised amount or less for a partial capture — and an optional reference), sent with an
`Idempotency-Key`. Adyen answers `201` with status `received`: the capture is accepted for
processing and its final outcome arrives asynchronously (webhooks are out of scope). Read the
captures endpoint reference and the capture guide and cite them with the date:
- https://docs.adyen.com/api-explorer/Checkout/71/post/payments/(paymentPspReference)/captures
- https://docs.adyen.com/online-payments/capture

Add capture so a caller gets a typed "capture received" result (its PSP reference and the amount
requested), never a claim that money moved. A capture obeys the same identical-request rule as a
payment: a retry reuses its key with identical bytes; a changed amount under a reused key is
refused before sending; a timed-out capture is uncertain and is never replayed automatically
unless the SDK's existing guarantee rules allow it. Keep payment methods, payments and details
unchanged for existing callers.
