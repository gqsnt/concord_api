## Slice-specific notes

This is the wave's simple control: keep the SDK as small as the slice allows. CreatePayment's
`idempotency_key` is supplied by the caller and travels in the JSON body; after a timeout the caller
may retry only with the exact key and body, and a mismatched replay is an error, not a new payment.
