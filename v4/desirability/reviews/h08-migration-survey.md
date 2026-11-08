# H08 migration scope survey (on `dz/h08`; read-only, Claude Explore subagent; not a review)

Used only if H08 is decided WIN: the card requires every SDK whose policy classifies operations by
name, path or table to adopt the mechanism, with the list fixed during migration and reviewed.

## Include (a forgotten entry is a silent default today)
| SDK | Where | What is classified | Mechanism today |
|---|---|---|---|
| paypal-sdk | `src/policy.rs:127-135` (`accepts_request_id`), `:187-188` | which writes may be replayed with `PayPal-Request-Id` | `==` chain over 5 endpoint `operation()`s + `method() != GET` |
| zendesk-support-sdk | `src/policy.rs:120`, `:169` | 409 → `Error::Collision` for the update; read/write | comparison with a **string literal** of the dotted name (not compiler-checked) + method rule |
| datadog-events-sdk | `src/api.rs:89-102` | write uncertainty; 202 required for the legacy post | `E::operation() == <DatadogApiEventsPost …>::operation()` (method cannot decide: POST search is a read) |
| square-payments-sdk | `src/error.rs:25-44` | error advice (replay an unchanged create vs re-read) | `operation() == Some(SquarePaymentsCreate::operation())` + POST-means-write (not provider-wide: Square searches are POST) |
| github-sdk (no policy) | `src/paging.rs:70-123,384,443`; `src/pages.rs:50-192` | listing id stored in continuation bindings | free-string operationIds paired with endpoint types by hand + `LISTINGS` table (`WORKFLOW_RUNS` listed, no factory) |

Notes: Square declaring `operations` on a client with `send` moves `SquarePolicy` from `SendWith`
to `SendOperation` though it ignores the variant (cost to record). GitHub keeps its id strings
(persisted `github-link/1` format); `LinkPages::new(operation_id: &str, …)` would change or
validate its argument (a public-signature decision for the migration review).

## Exclude (with reason)
- Discord, Cloudflare, Xero: method or URL rules with no per-operation exception today (latent
  hazards, noted).
- Kubernetes: type-level trait table with associated types; a forgotten entry is already a compile
  error.
- Microsoft Graph: own data-carrying enum serialized into stored links; low value.
- Distribution registry: scope chosen per call site and carries data.
- S3, Adyen, Elasticsearch, Postmark, Europe PMC: output-type traits, already compile-checked.
- All others: `Operation` used for display/logging only, or no per-operation logic.
