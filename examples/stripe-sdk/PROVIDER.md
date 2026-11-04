# Stripe v1 — provider contract implemented by this SDK slice

This is a **bounded slice** of Stripe API v1, not a complete Stripe SDK. Every fact below comes
from Stripe's public documentation, retrieved on **2026-11-04**; nothing here was verified
against the live API (the live binaries were not run: no credentials were available, see the app
README). Some documentation pages were served localized (French) for the retrieval; their content
was read as the English reference.

## Version

| Item | Value | Source |
|---|---|---|
| Pinned version | `2026-10-28.endive` (`PINNED_API_VERSION`) | [V2] "The current version is 2026-10-28.endive." The handoff's earlier observation is the same value. |
| How it is sent | `Stripe-Version` on **every** request (client value initialized to the pinned version; `StripeContext::with_version` replaces it) | [V2]: requests "use your Stripe account's default API version ... unless you override it by setting the `Stripe-Version` header". The SDK never follows the account default silently. |
| Schema | Reference pages for the objects below at that version; the OpenAPI repository [O2] was **not** used to generate or check the models (no schema digest is claimed). | |

## Sources

| Id | URL | Used for |
|---|---|---|
| S1 | https://docs.stripe.com/api | object and endpoint reference (pages listed below) |
| S2 | https://docs.stripe.com/api/pagination | list envelope (`object: "list"`, `data`, `has_more`, `url`), `limit` 1–100 default 10, `starting_after`/`ending_before` "mutually exclusive", reverse chronological order |
| S3 | https://docs.stripe.com/api/expanding_objects | repeated `expand[]`, dotted paths, "Expansions on list requests start with the `data` property", maximum depth four levels |
| S4 | https://docs.stripe.com/rate-limits | `429`, `Stripe-Rate-Limited-Reason` values (`global-rate`, `endpoint-rate`, `global-concurrency`, `endpoint-concurrency`, `resource-specific`); a `429` without it is not a rate limit (lock timeout) |
| S5 | https://docs.stripe.com/api/idempotent_requests | `Idempotency-Key` up to 255 characters, saved status and body (including `500`), parameters compared with the original, keys pruned after at least 24 hours, `POST` only |
| S5b | https://docs.stripe.com/error-low-level | `Idempotent-Replayed: true`, `Stripe-Should-Retry`, `409` for a concurrent request with the same key, retry with the same key and parameters within 24 hours |
| S6 | https://docs.stripe.com/connect/authentication | `Stripe-Account: acct_...` per request with the platform's secret key |
| V2 | https://docs.stripe.com/api/versioning | pinned version, `Stripe-Version` |
| O2 | https://github.com/stripe/openapi | not used (see Version) |
| — | https://docs.stripe.com/api/authentication | Basic auth with the key as user name; "If you need to authenticate using bearer auth ... use `-H "Authorization: Bearer ..."`" (this SDK sends `Bearer`) |
| — | https://docs.stripe.com/api/errors, https://docs.stripe.com/error-codes | error object (`type`, `code`, `message`, `param`, `doc_url`), types `api_error`, `card_error`, `idempotency_error`, `invalid_request_error`; codes `rate_limit`, `lock_timeout`, `resource_missing`, `parameters_exclusive`, `idempotency_key_in_use` |
| — | https://docs.stripe.com/api/request_ids | `Request-Id` response header |
| — | https://docs.stripe.com/api/metadata | `metadata[key]=value`, unset a key with an empty value, unset all with `metadata=`, no `[`/`]` in keys, 50 keys / 40 / 500 characters |
| — | https://docs.stripe.com/api/customers/create, /api/customers/update, /api/customers/object (`?query=invoice_settings`), /api/payment_methods/object (`?query=card`), retrieved **2026-11-05** | `invoice_settings[default_payment_method]` on customer create and update ("ID of a payment method that's attached to the customer"); `invoice_settings.default_payment_method` on the customer object ("nullable, expandable"); the payment method object (`id`, `type`, `card.brand/last4/exp_month/exp_year`, `customer` expandable, `metadata` nullable) |
| — | https://docs.stripe.com/payments/payment-methods/migrate-from-legacy-cards-sources (served in French), retrieved 2026-11-05 | curl `-d "invoice_settings[default_payment_method]={{PAYMENTMETHOD_ID}}"`; the parameter mapping shows `invoice_settings.default_payment_method: "pm_xxx"`; Card or Source ids can no longer be passed as `payment_method` |
| — | https://docs.stripe.com/api/customers/list, /api/invoices/list, /api/invoices/invoice_lines, /api/invoices/object, /api/invoice-line-item/object, /api/subscriptions/object, /api/subscriptions/create, /api/subscriptions/update, /api/prices/create, /api/products/create, /api/balance_transactions/object | parameters, objects and curl spellings of the implemented operations |

## Supported operations (19)

| Family | Operation | Generated request type |
|---|---|---|
| Customers | `GET /v1/customers`, `GET /v1/customers/{id}`, `POST /v1/customers`, `POST /v1/customers/{id}` | `StripeCustomers{List,Retrieve,Create,Update}` |
| Subscriptions | `GET /v1/subscriptions`, `GET /v1/subscriptions/{id}`, `POST /v1/subscriptions`, `POST /v1/subscriptions/{id}` | `StripeSubscriptions{List,Retrieve,Create,Update}` |
| Invoices | `GET /v1/invoices`, `GET /v1/invoices/{id}`, `GET /v1/invoices/{id}/lines` | `StripeInvoices{List,Retrieve,Lines}` |
| Products | `GET /v1/products`, `GET /v1/products/{id}`, `POST /v1/products` | `StripeProducts{List,Retrieve,Create}` |
| Prices | `GET /v1/prices`, `GET /v1/prices/{id}`, `POST /v1/prices` | `StripePrices{List,Retrieve,Create}` |
| Balance transactions | `GET /v1/balance_transactions`, `GET /v1/balance_transactions/{id}` | `StripeBalanceTransactions{List,Retrieve}` |

Every operation takes a per-request `Stripe-Account` override (`OnAccount`), every list takes
`limit`, both cursors and `expand[]`, every write takes an application-supplied
`Idempotency-Key`. List filters: customers (`email`, `created[gte|lt]`), subscriptions
(`customer`, `price`, `status`, `collection_method`, `created`), invoices (`customer`,
`subscription`, `status`, `collection_method`, `created`), products (`active`, `ids[]`,
`created`), prices (`product`, `active`, `currency`, `type`, `lookup_keys[]`, `created`), balance
transactions (`type`, `currency`, `payout`, `source`, `created`).

Customer create and update also take `invoice_settings[default_payment_method]`
(`params::InvoiceSettingsParams`); the customer model decodes
`invoice_settings.default_payment_method` as `Expandable<PaymentMethod>`
(`Customer::default_payment_method()`), expanded with `expand[]=invoice_settings.default_payment_method`
(`data.invoice_settings.default_payment_method` on a list). The payment method is read only as an
expansion target; payment method operations remain unsupported.

## Deliberately unsupported

Deletes; search endpoints (`/v1/*/search`, which page with `next_page`, not object ids); invoice
creation, finalization, payment, voiding and the preview endpoint (`upcoming_in_...` ids);
subscription cancel/pause/resume, subscription items and schedules; payment methods, charges,
payment intents, refunds, payouts, transfers and webhooks; API v2 (`/v2`, different pagination and
idempotency); `created` filters other than `gte`/`lt`; `Stripe-Context`; OAuth / workload identity
federation; automatic retries (none, by design); request-level `Stripe-Version` override
(context-level only). Models decode the fields used by the SDK and its workflows; every other
field Stripe sends is ignored.

## Wire decisions and documented discrepancies

| Topic | Documentation | Decision |
|---|---|---|
| Form encoding of nested values | Curl examples send `items[0][price]=..`, `metadata[order_id]=6735`, `recurring[interval]=month`, `product_data[name]=..` and `"expand[]"=customer` with literal brackets (curl `-d` does not encode). | `StripeForm` percent-encodes keys like any `application/x-www-form-urlencoded` body (`items%5B0%5D%5Bprice%5D`); a form decoder yields the same keys. Not live-verified. |
| Arrays | Scalars are shown as repeated `expand[]=..`; arrays of objects as indexed `items[0][price]`. Stripe's own client libraries index scalars too (`expand[0]=..`). | Scalars `key[]` (repeated field), objects `key[i][..]`. Both documented spellings; not live-verified which is preferred. |
| Omitted vs empty | "Individual keys can be unset by posting an empty value to them. All keys can be unset by posting an empty value to `metadata`"; `default_tax_rates`: "Pass an empty string to remove previously-defined tax rates." | `Option::None` omits; `Patch::Unset` / `MetadataPatch::ClearAll` send `key=`; an empty `Vec` is omitted. |
| Idempotency mismatch status | The error type `idempotency_error` is documented ("re-used on a request that does not match the first request's API endpoint and parameters"); the HTTP status for this case is not stated on the pages consulted (`409` is documented for a concurrently executing request). | The SDK classifies by `error.type` (`Error::is_idempotency_error`), never by status. The test fake answers `400`. |
| Line item `invoice` and `subtotal` | Listed as attributes, absent from the reference's example objects. | `Option` in `InvoiceLineItem`; the fixtures include one line without `invoice`. |
| Invoice → subscription | The invoice's subscription is `parent.subscription_details.subscription` (expandable); there is no top-level `subscription` at this version. Line items carry `parent.subscription_item_details.subscription`. | Modelled as documented (`Invoice::subscription()`, `InvoiceLineItem::subscription()`). |
| Subscription periods | `current_period_start/end` appear on subscription **items** in the examples. | On `SubscriptionItem`, optional. |
| Line item ids | Examples show `il_tmp_...`. | Prefix rule `il_` only. |
| Product ids | "You can optionally override this ID". | `ProductId` has no prefix rule. |
| Payment method ids | Examples show `pm_...`; the legacy-cards migration guide maps defaults to `pm_xxx` and says Card or Source ids can no longer be passed as `payment_method`. | `PaymentMethodId` requires `pm_` when the application constructs it (received ids: no prefix rule, as for every id). |
| Unsetting `invoice_settings[default_payment_method]` | The reference documents an empty value only for `invoice_settings.custom_fields` ("When updating, pass an empty string to remove previously-defined fields"), not for `default_payment_method`. | No `Patch` for it: `InvoiceSettingsParams::default_payment_method` is `Option<PaymentMethodId>`, `None` leaves it unchanged. An `InvoiceSettingsParams` with nothing set sends nothing (same bytes as `None`). |
| Customer `invoice_settings` | Documented as an object (not nullable), present in every example. | `#[serde(default)]`: a response without it decodes as empty settings, so older fixtures still decode. |
| Response `Stripe-Version` | Not documented on the pages consulted. | Captured in `ResponseMeta::stripe_version` if present, never required. |
| Authentication | Basic auth documented first; Bearer documented as an alternative. | `Authorization: Bearer <key>`, flagged sensitive. |
| Idempotency key scope | Keys must identify "a single operation on your account"; scoping by `Stripe-Account` is an inference. | The application ledger refuses any reuse of a key for a different request, across accounts too. |

## Contexts used in tests

Two connected accounts (`acct_ALPHA`, `acct_BETA`; also `acct_TARGET`, `acct_DEFAULT`) and the
platform (no `Stripe-Account`), all synthetic. Secret keys are synthetic `sk_test_SYNTHETIC...`
strings. Request ids are synthetic `req_...` values produced by the fakes.
