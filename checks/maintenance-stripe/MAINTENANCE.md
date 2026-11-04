# Stripe SDK maintenance exercise (DX-01, `DATA_AND_VALIDATION.md` §6)

## Task statement

Staged SDK evolution (not a claim that Stripe changed): **add an optional nested form field and a
typed expansion path to the Stripe slice** (`examples/stripe-sdk`). Evidence to retain:
omitted-field wire compatibility (the frozen consumer's request bytes must stay identical),
changed/requested form bytes, validation errors, and whether callers must learn internal encoding
details. Procedure: `v4/extensions-real-apis/MAINTENANCE_BRIEF.md` (three commits: frozen
consumer, SDK change, updated consumer).

Performed by a maintainer who did not write the SDK, reading only its rustdoc, `PROVIDER.md`,
the briefs and Stripe's public documentation (the SDK's `FRICTION.md` was not read before stage
3 was committed).

## Stage 1 — frozen initial consumer

`src/before.rs` is a CRM-mirroring application: `mirror_contact` (`POST /v1/customers`),
`correct_contact` (`POST /v1/customers/{id}`), `fetch_profile` (`GET /v1/customers/{id}`),
`find_by_email` (`GET /v1/customers?email=..&limit=..`), each through the SDK's `send()` so the
`Request-Id` and `Idempotent-Replayed` are kept. `tests/before.rs` runs it against a loopback
fake with two connected accounts (`acct_ALPHA`, `acct_BETA`) and asserts the exact form bodies
as literal byte strings, the decoded pairs, the target, `Stripe-Account`, `Stripe-Version`,
`Idempotency-Key`, `Content-Type`, and how a Stripe parameter error reaches the caller.

The response fixtures include every member of the documented customer object, notably
`invoice_settings` (`{"custom_fields":null,"default_payment_method":"pm_..",...}`), which the
SDK did not model at stage 1: the frozen consumer must keep decoding them once it does.

## Stage 2 — SDK change (recorded when made)

Customer create/update gain `invoice_settings[default_payment_method]`
(`params::InvoiceSettingsParams`, field `invoice_settings: Option<InvoiceSettingsParams>`), the
customer model gains `invoice_settings.default_payment_method: Option<Expandable<PaymentMethod>>`
with a `PaymentMethod` model, a `PaymentMethodId` (`pm_`) and `Customer::default_payment_method()`.
No change to the `concord::api!` declaration other than two `expand[]` doc comments, none to the
form encoder (`form.rs`), the expandable machinery, the context or `send()`. The frozen consumer
and the app compiled and passed unchanged (details at stage 3).
