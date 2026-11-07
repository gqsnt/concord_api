## Slice-specific notes

Use the Checkout API test environment base and a pinned API version. A payment submission carries
an `Idempotency-Key`: an automatic or caller retry must reuse the same key with the identical
request, and a changed body under a reused key, or a guarantee the provider no longer honours, must
surface as needing reconciliation — never as a fresh payment. A payment can end in a state that
needs a further action (`action`, then `/payments/details`): model it as data the caller acts on.
