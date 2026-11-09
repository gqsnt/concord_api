# The construction problem (open)

The `controls { … }` statement (H06, H06c) was removed before PR #6 merged (owner decision,
2026-11-09): it lists generated controls to turn off, which is a statement about Concord rather
than a fact about the provider (charter rule C1). The problem it addressed is real and is open
again on `main`. Family C of the Concord Next mission must close it with a design that follows the
charter.

## The problem

Every client value gets a public generated setter (`set_x`, `maybe_x`), every client gets a
public `new` and `with_base_url`, and every scope value gets a public `set_x`. When an SDK
documents that two values belong together (a token and its host, a credential and its rate
state), these setters let a caller separate them. Concord cannot express "fixed at construction"
or "replaced only together".

## Invariant breaks demonstrated by H06 (baseline delivers each forbidden request)

| ID | SDK | Move | Effect |
|---|---|---|---|
| FM1 | Shopify | `admin().shop(a, token_a).set_shop(b)` | shop A's token is sent to shop B |
| FM2 | Zendesk | `client.set_subdomain(other)` | the account's OAuth token is sent to another account |
| FM3 | Salesforce | `api().clone().with_base_url(evil)` | the org's token is sent to any host |
| FM4 | Salesforce | `set_version(v)` with `v < ApiVersion::MINIMUM` | bypasses the version check of `OrgContext::new` |

Mastodon keeps its generated client private for the same reason (its `with_base_url` would move
the token to another host), so callers cannot reach the generated endpoint values through it.

## Generated controls that contradict a documented SDK invariant (H06c audit, 55 SDKs)

| SDK | Control | Documented invariant it breaks |
|---|---|---|
| adyen-checkout | `Checkout::set_api_key`, `set_policy` | a credential shares its company's idempotency ledger |
| datadog-events | `DatadogApi::new`, `set_api_key`, `set_application_key` | keys are checked together by `Credentials::new` |
| digitalocean | `DigitalOcean::with_base_url` | non-production origins only through `for_fixture` (loopback) |
| discord-rest | `Discord::set_token`, `set_limiter` | one rate limiter per bot token |
| mastodon | (generated client kept private) | the token is bound to its instance |
| quickbooks-online | `QuickBooks::with_base_url` | a request to any origin other than the two official hosts carries the connection's token (T4 already makes `send()` pick the connection's environment host, so `new` and `set_connection` are correct) |
| salesforce | `SalesforceApi::new`, `with_base_url`, `set_version` | FM3, FM4 |
| sentry | `Sentry::set_authorization`, `set_policy`, `with_base_url` | admission state belongs to one caller and deployment |
| shopify | `ShopifyAdminShop::set_shop` | FM1 |
| usgs-earthquake-catalog | `Usgs::new`, `with_base_url` | the capsule runs only against local fixtures |
| zendesk-support | `Zendesk::set_subdomain`, `with_base_url` | FM2 |

The other 44 SDKs had no contradiction (their wrappers, private generated clients or runtime
guards already hold the invariant; PayPal and Strava refuse a moved destination before I/O).
About 22 SDKs wrap the generated client in their own struct, partly for this reason.

## Cases a design must keep working

- QuickBooks: `set_connection` replaces realm, credential source and policy together (T4 keeps
  its send-time environment selection).
- Same-account credential rotation: Shopify `set_access_token`, Zendesk `set_authorization`,
  Mastodon `with_token`, S3 and Kubernetes.
- Proxies and test fixtures need another base URL; ClickHouse and Kubernetes have useful defaults.
- `reqwest` stays reachable (`prepare()`, `into_parts()`, the caller's `reqwest::Client`).

## Records (archive branch)

On [`archive/desirability-loop-1`](https://github.com/gqsnt/concord_api/tree/archive/desirability-loop-1):
`v4/desirability/measurements/H06.md` (FM1–FM4, arms A `#[fixed]` and B `controls`),
`v4/desirability/impact/H06.md` and `impact/H06c.md` (all 55 SDK decisions),
`v4/desirability/cards/H06.md` (the card; its task bank was sealed and is not in the repository).
