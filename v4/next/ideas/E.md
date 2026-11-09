# E. Operation facts — merged ideas
Sources: EF-claude.md, EF-codex.md (family E parts). 7 lane designs → 4 entries: 2 KEEP, 2 KILL, 0 MERGED.

Evidence: `glue:operation` = 44 lines is a lower bound (both lanes). The real tables sit in
`policy.rs` files the classifier labels behavior: Strava `auth.rs:260-307`, Sentry `policy.rs:88-117`,
PayPal `policy.rs:129-158`, Zendesk `policy.rs:161-189`, Datadog `api.rs:88-97`. 8 SDKs use
`operations` today (Datadog, GitHub, Kraken, PayPal, Sentry, Square, Strava, Zendesk). FRICTION E3
(9 SDKs): a new endpoint silently joins the wrong rule; E1 (16 SDKs): declared values are not readable
by the policy, so SDKs re-parse them by name; E2 (11 SDKs): "not sent" vs "maybe applied" is
classified by hand.

## E1. Read or write on the operation: reads by method, `#[read]` where the method lies — KEEP (rank 1)
Sources: EF-claude.md § E1, EF-codex.md § E1
Fact: whether an operation only reads provider state, independently of what its HTTP method suggests (Datadog `POST search`, Kraken's three signed account POSTs, Intercom `POST search`, Notion `POST query`).
Mechanism: `concord::Operation::is_read()`, carried by the generated descriptor everywhere `Operation` already travels (endpoint, `Prepared`, `Received`, errors). The default comes from the written method (GET, HEAD, OPTIONS read; everything else writes); `#[read]` on an endpoint or scope states a POST read (nearest owner wins, as for `limit`); a redundant `#[read]` is an error. Idempotency is never inferred from PUT or DELETE (Strava, Sentry and Zendesk refuse to resend their PUTs).
Variants: codex writes the fact explicitly on every endpoint whose policy reads it (`effect read;` / `effect write;`, `Operation::effect() -> Option<Effect>`), absence is unknown, and the method never supplies a default.
Declaration: Datadog (`api.rs:36-39`, `:60`):
```rust
        send SendPolicy -> Result<_, Error>;
-       /// Every operation of [`DatadogApi`], as [`SendPolicy`] classifies it (… the search is a `POST` read).
-       operations DatadogApiOperation;
-       POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }
+       #[read] POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }
// Kraken lib.rs:50: `scope account {` → `#[read] scope account {` (covers its 3 POSTs)
// codex form: `POST search(..) -> EventsPage { path "v2/events/search"; effect read; }`, and on every endpoint
```
Call site (SDK policy and app):
```rust
- impl<T: Send> SendOperation<T, DatadogApiOperation> for SendPolicy { .. let posting = posting(operation);
+ impl<T: Send> SendWith<T> for SendPolicy { .. let posting = !E::operation().is_read();
- error.operation().and_then(|op| op.method()) == Some("POST")      // Square error.rs:28
+ error.operation().is_some_and(|op| !op.is_read())
```
Glue removed: claude ≈70 lines in 9 SDKs (Datadog 16, Zendesk ≈30, Strava ≈12, Sentry ≈6, Discord 4, Cloudflare 3, plus method-string tests in QuickBooks, Xero, Spotify and Square); codex ≈10 primary lines plus Sentry's write column. Datadog and Zendesk drop `operations`. Checked: Datadog `posting` (`api.rs:88-97`, 10 lines), Discord `is_write` (`limiter.rs:85-88`), Zendesk `Class` (`policy.rs:161-189`, 29 lines; its other column, `collision_on_conflict`, becomes an identity compare that already exists). **Correction:** the "Square search POST" cited in IDEAS and EF-claude does not exist at this checkout (Square declares `create`, `get`, `list`, `complete`, `cancel`, `lib.rs:69-94`); codex is right. The POST-read witnesses are Datadog, Kraken, Intercom and Notion.
Counterexample: Kraken POST reads → one `#[read]` on the scope; signing stays per scope (read does not mean unsigned). Datadog's `posting` also means "202 required": it equals `!is_read()` only while the legacy post is Datadog's only write (codex). Adyen: every endpoint is a keyed POST write; E1 says nothing about replay (E2). `#[cfg]` endpoints: the fact travels in each endpoint's meta. A GET with side effects would need a symmetric `#[write]` that no witness has. Strava and Sentry keep `operations` for identity and quota buckets.
Simple control: unchanged (all GET: `is_read()` is true and no word is added).
New concepts: `#[read]` on endpoints and scopes, `Operation::is_read()` (codex: `effect read|write;`, `Effect`, `Operation::effect()`).  Breaking: none (additive); an SDK moving from `SendOperation` to `SendWith` changes its policy, not its callers.
Shares mechanism with: E2 and F2 (the same `Operation` carrier: `idempotent()`, `is_paged()`), A1 (uncertainty classification: Square `outcome_unknown`, Discord `failed_before_head`, PayPal read vs write), J (span field without a table).
Screen:
- C1: `#[read]` is the provider's documented sentence; no negation.
- C2: fact only; retry and uncertainty stay in policies.
- C3: 0 lines for Nager, PokeAPI and Frankfurter.
- C4: claude's default is the method on the same line (deterministic, visible); codex rejects any method default. Show both at the gallery. The method default makes a forgotten POST read a write (the safe side: no replay) but loses H08's compile-time "every endpoint is classified" check. Codex's `None` plus a runtime `MissingEffect` loses it too, and grows DSL lines per endpoint.
- C5: a redundant `#[read]` is an error at the attribute.
- C8: request bytes unchanged.
- C9: a matrix of method × attribute × scope × cfg. Codex's trap: `Operation` has two declared forms (`Endpoint(&EndpointMeta)` and `Declared(name, method)`, `concord-core/src/response.rs:35-41`); the fact must sit on both or `E::operation()` loses it.

## E2. The idempotency key is a role of a declared value — KEEP (rank 2)
Sources: EF-claude.md § E2, EF-codex.md § E3
Fact: the provider deduplicates this write by the value of this declared input (PayPal `PayPal-Request-Id`; Stripe, Adyen, Zendesk `Idempotency-Key`), or, when the key is inside the body, the endpoint is deduplicated (Square).
Mechanism: `#[idempotency_key]` on a `header(..)` input placement; `#[idempotent]` on an endpoint whose key is in the body. Generated `Operation::idempotent()` and `Prepared::idempotency_key() -> Option<&HeaderValue>`, the key as it is about to be sent. Concord never mints a key and the bytes do not change.
Variants: codex names an already-placed value with an endpoint statement (`idempotency request_id;`) and reads it strictly (absent, one, or repeated/invalid) · optional Rust `RequestSnapshot::capture(&request, bound)` for an exact final-request identity (Adyen, codex).
Declaration: PayPal (`lib.rs:82-92`; the same on 4 more endpoints):
```rust
        POST create(
            body order: OrderRequest,
-           request_id?: RequestId,
+           #[idempotency_key] header("paypal-request-id") request_id?: RequestId,
        ) -> Order {
-           headers { "paypal-request-id" = "{request_id}"; "prefer" = "return=representation"; }
+           headers { "prefer" = "return=representation"; }
        }
// Square lib.rs:69: `#[idempotent] POST create(body payment: CreatePayment) -> PaymentResponse;`
```
Call site (PayPal `policy.rs:211-216`; Square `error.rs:42-55`):
```rust
- let request_id = prepared.request().headers().get("paypal-request-id").and_then(|v| v.to_str().ok()).and_then(|v| RequestId::new(v).ok());
+ let request_id = prepared.idempotency_key().and_then(RequestId::from_header);
- pub fn create_outcome_unknown(&self) -> bool { /* 14-line match over SquareOperation */ }
+ pub fn create_outcome_unknown(&self) -> bool { self.outcome_unknown() && self.operation().is_some_and(|op| op.idempotent()) }
```
Glue removed: claude ≈85 lines in 5 SDKs (PayPal ≈45, Square ≈27, Zendesk ≈6, Adyen 3, Stripe 6 header lines); codex ≈7 sites in PayPal, plus 13 lines in Adyen with the snapshot helper. Checked: PayPal `Class` (`policy.rs:129-158`, 30 lines) has `accepts_request_id == write` in every arm, so PayPal's table goes with E1 and E2 together (not additive with E1); the header lookup by name is `policy.rs:211-216`. PayPal and Square drop `operations`.
Counterexample: Adyen compares the exact bytes sent under a key (`policy.rs:324-329`). E2 keeps Concord out of that: the policy still builds its identity from the final `reqwest::Request` (codex's snapshot helper is the optional shared form). Square: the key is in the body, so the accessor is `None`. Retention windows (Zendesk 2 h, PayPal 6 h, Adyen 7 days) are not expressed and stay in Rust. Key presence is not replay permission (codex); the policy decides.
Simple control: unchanged.
New concepts: `#[idempotency_key]` (or `idempotency v;`), `#[idempotent]`, `Operation::idempotent()`, `Prepared::idempotency_key()`; optional `RequestSnapshot`.  Breaking: none (additive); codex's strict reader rejects repeated keys that native edits allowed before.
Shares mechanism with: F2 (`#[cursor]` is the same "role on a declared value plus a generated accessor"; both answer FRICTION E1, 16 SDKs), B2 (output `header(..)` placements mirror these input placements), D (`#[sensitive]` stays a separate attribute; the key is withheld in `Prepared`'s `Debug`), A (Zendesk `UncertainWrite`, PayPal `Uncertain` carry the key by role), J ("keyed: yes" without the value), E1.
Screen:
- C1: "PayPal deduplicates by `PayPal-Request-Id`" is the provider's contract.
- C2: whether to resend stays the policy's decision.
- C4: no header name is recognized implicitly (an `"idempotency-key"` literal stays an ordinary header).
- C5: the role on `query(..)`, on a body, or on a read is an error at the attribute.
- C7/C8: bytes unchanged, no generated key, clones keep the key.
- C9: wire test that `idempotency_key()` equals the declared header; canary that a `#[sensitive]` key stays withheld (Stripe, Adyen).

## E3. Capability crates read the facts: `Replay::of(&prepared)`, `Error::unanswered_write()` — KILL (glue: each is a one-liner once E1 and E2 exist)
Sources: EF-claude.md § E3
≈40 lines in 6 SDKs for two new items, and each becomes a one-line SDK expression with E1/E2 (Xero `policy.rs:46` `request.method() == Method::GET` → `E::operation().is_read()`; `!op.is_read() && e.delivery() == Delivery::Unknown`). `Replay::of` would also make PayPal's "a keyed write may repeat" decision a library default, which Sentry's write replay after a 429 (`sentry/policy.rs:186-190`) and codex's "key presence is not replay permission" contradict; every SDK's uncertainty rule differs (5xx, unreadable 2xx). FRICTION E2 (11 SDKs) is served by E1's `is_read()` on the error's operation.

## E4. SDK-typed per-operation fact values (`class Class::UPDATE;` / `facts ZendeskContract { effect: .., conflict: .. }`) — KILL (glue: moves the table; C2)
Sources: EF-claude.md § E4, EF-codex.md § E2
Both lanes say it moves a ≈25–30-line table into the declaration more than it deletes one (claude ≈25–35 net in 3 SDKs; codex "not a claim of immediate net line savings"). Concord cannot check that a record field is a provider fact: Strava's `Class` carries `retry_delay: Duration` (`strava-sdk/src/auth.rs:264`), behavior that would ride into the declaration (the V2/V3 drift), and `class`/`facts` are words about Concord plumbing (C1). Its one real gain, a classification error at the endpoint rather than in a `match`, stays open; E1's method default covers the read/write half.
