# Families E and F: idea lane (provider: claude)

Repository read: `work/wt/rv-p1` at `ba10189b`. Every quoted declaration and every `file:line`
range below is from that tree (`examples/<name>-sdk/src/...`, written `<name>/file.rs:a-b`).
Line counts are non-blank lines in the cited ranges, counted by hand. They are estimates of what
disappears, not measurements.

`work/inventory/FRICTION.md` did not exist, so the witness SDKs' own `FRICTION.md` and
`MAINTENANCE.md` files were read instead. They are cited where an SDK author asked for the same
change.

**Measurement caveat.** The classifier reports `glue:operation` = 44 lines in total. That number is
a lower bound and is partly noise. Its largest excerpt, `paypal/lib.rs:232-248`, is the builder's
`reads()`/`writes()` setters, which the classifier matched by their names. The real
classification tables sit in `policy.rs` files, which the classifier labels `behavior`:
`strava/auth.rs:259-308` (49 lines), `sentry/policy.rs:88-117` (30), `paypal/policy.rs:129-158`
(29), `zendesk-support/policy.rs:161-189` (28), `datadog-events/api.rs:88-97` (10) and
`github/paging.rs:69-151` (80). The E numbers below therefore count those ranges by hand.
`glue:paging` (4,006 lines) is a more faithful measure.

---

## What the witnesses restate (the evidence the designs start from)

**Operation facts (E).**

| Fact restated by hand | Where |
|---|---|
| read or write | `strava/auth.rs:261` `read`; `sentry/policy.rs:93` `write`; `paypal/policy.rs:132` `write`; `zendesk-support/policy.rs:164` `read`; `datadog-events/api.rs:92-97` `posting`; `discord-rest/limiter.rs:85-88` `is_write` by method; `square-payments/error.rs:28,32` `method() == Some("POST")`; `quickbooks-online/policy.rs:38`, `cloudflare-zones/policy.rs:152-154`, `xero-accounting/policy.rs:46` and `spotify-web-api/session.rs:339` compare method strings |
| "the method lies" (POST reads) | `datadog-events/api.rs:37-38` ("the method cannot tell, the search is a `POST` read"); Kraken's `scope account` (3 POST reads, `kraken-spot-rest/lib.rs:50-66`); Notion `POST query`; Intercom `POST search` |
| deduplicated by a key | `paypal/policy.rs:133-135` `accepts_request_id`, then the header is read back by name at `policy.rs:211-216`; `zendesk-support/policy.rs:104-109` reads `"idempotency-key"` by name; `adyen-checkout/policy.rs:306-314` does the same; `square-payments/error.rs:39-55` `create_outcome_unknown` (the key is in the body) |
| uncertain after a timeout | `strava/auth.rs:268-269` `uncertain` equals `!read` in every arm; Zendesk, PayPal and Sentry derive it from write + no answer. It is a **derivation**, not a separate fact. |
| paged | `sentry/policy.rs:94-95` `paged`; `github/paging.rs:116-142` `listing_id` |
| provider-specific facts | Strava OAuth scope and quota bucket (`auth.rs:265-271`); Kraken signing realm by scope (`policy.rs:86-112`); Zendesk "409 on update is a collision" (`policy.rs:165-166`) |

Square's MAINTENANCE.md (lines 82-88) counts "six statements of the same fact" for which writes are
uncertain.

**Paging facts (F).**

| Shape | Request side (cursor goes) | Response side (next comes from) | SDKs |
|---|---|---|---|
| opaque token | query | body field | Zendesk `page[after]` ← `meta.after_cursor` while `meta.has_more`; Notion children `start_cursor` ← `next_cursor`; Square `cursor` ← `cursor`; Intercom list `starting_after` ← `pages.next.starting_after`; Datadog list `page[cursor]` ← `meta.page.after`; Kubernetes `continue` ← `metadata.continue` |
| opaque token | **body** | body field | Datadog search `page.cursor`; Notion query `start_cursor`; Intercom search `pagination.starting_after`; Elasticsearch `search_after` ← last hit `sort` |
| item key | query | an item of the page | Stripe `starting_after` ← `data.last.id` while `has_more`; Discord `before` ← oldest id, ends on a short page |
| provider URL | none | `Link` header | GitHub (follow verbatim); Sentry (cursor parameter inside the link, plus `results=`) |
| provider URL | none | body field | DigitalOcean `links.pages.next`; Mailgun `paging.next` (ends on an empty page); Graph `@odata.nextLink`; PokeAPI `next` |
| number | query or body | computed | Strava `page`+1 until empty; Cloudflare `page`+1 while `< total_pages`; Kraken `ofs`+len until `count` |

Every family-F SDK writes the same four pieces:

1. a `_from` twin (18 pairs);
2. a "did not advance" check (Stripe, GitHub, Graph, Zendesk, Intercom, Sentry, Elasticsearch, Kubernetes, Notion, Datadog);
3. a projection from the envelope into `Page`;
4. for resumable listings, a "binding" that proves a cursor belongs to this listing (Stripe, GitHub, Graph, Mailgun, DigitalOcean).

There are 28 copies of the H09c sentence "Use `…::Resume` for the position…".

---

# Family E: operation facts

## E1. Reads by method, `#[read]` where the method lies (the "smaller than you think" design)

**Fact it expresses:** whether an operation only reads. It follows from the written HTTP method
(GET, HEAD and OPTIONS read; everything else writes), except where the provider documents a
POST that only reads.

**Declaration (author writes):** Datadog Events, `datadog-events/api.rs:36-39` and `:60`.

Before:
```rust
send SendPolicy -> Result<_, Error>;
/// Every operation of [`DatadogApi`], as [`SendPolicy`] classifies it (the legacy post or
/// a read: the method cannot tell, the search is a `POST` read).
operations DatadogApiOperation;
...
POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }
```
After:
```rust
send SendPolicy -> Result<_, Error>;
...
#[read] POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }
```

Kraken, `kraken-spot-rest/lib.rs:50-52`: `scope account {` → `#[read] scope account {`. The
three signed POSTs inherit it, and the nearest owner wins, as for `limit`
(`Model::nearest_limit`, `concord-macros/src/model.rs:1786`).

Rules:

- GET, HEAD and OPTIONS read. Every other method writes.
- `#[read]` on an endpoint or a scope makes the non-read methods below it reads.
- `#[read]` is an error where it changes nothing (a GET, or a scope whose endpoints all read).
- Idempotency is **not** inferred from PUT or DELETE. Strava (`lib.rs:101-103`), Sentry
  (`lib.rs:74-75`) and Zendesk refuse to resend their PUTs, so the RFC 9110 "idempotent method"
  is not what SDKs mean.

**Generated:** `concord::Operation::is_read() -> bool`, false for a label. It is carried in
`EndpointMeta` and in `Endpoint::operation()` (today `__private::declared(name, method)`,
`concord-core/src/__private.rs:336`). Equality of `Operation` is unchanged.

**Call site (caller writes).** The facts' callers are the SDK policies and the applications that
classify errors.

Datadog, `datadog-events/api.rs:92-107`. Before:
```rust
fn posting(operation: DatadogApiOperation) -> bool {
    match operation {
        DatadogApiOperation::EventsList | DatadogApiOperation::EventsSearch => false,
        DatadogApiOperation::EventsPost => true,
    }
}
impl<T: Send> SendOperation<T, DatadogApiOperation> for SendPolicy {
    async fn send<E: Endpoint<T> + Send>(&self, operation: DatadogApiOperation, endpoint: E) -> Self::Output {
        let posting = posting(operation);
```
After:
```rust
impl<T: Send> SendWith<T> for SendPolicy {
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let posting = !E::operation().is_read();
```

Discord, `discord-rest/limiter.rs:85-88`: `fn is_write(&self) -> bool { !matches!(self.operation.method(), Some("GET" | "HEAD")) }`
becomes `!self.operation.is_read()`, and the function is deleted.

Applications, Square `error.rs:28`: `error.operation().and_then(|op| op.method()) == Some("POST")`
becomes `error.operation().is_some_and(|op| !op.is_read())`. This is correct for a future
search POST, which the method test gets wrong.

**SDK code that disappears:**
- `datadog-events/api.rs:37-39` (3), `:88-97` (10) and the operation parameter at `:99-107` (3): about 16 lines, plus the `operations` statement.
- `discord-rest/limiter.rs:85-88`: 4 lines.
- `strava/auth.rs:261-262, 268-269, 276, 279, 294-295, 301, 303, 323, 326`: the `read`/`uncertain` fields, about 12 lines.
- `sentry/policy.rs:91-93, 104, 108, 112`: the `write` field, about 6 lines.
- `zendesk-support/policy.rs:161-189` (`Class`, 28 lines) and `lib.rs:58-60` (`operations`, 3 lines). The 409 rule becomes `E::operation() == ZendeskTicketUpdate::operation()`, an identity compare that already exists today. About 30 lines.
- `cloudflare-zones/policy.rs:152-154` (3). The method literals in QuickBooks, Xero, Spotify and Square are replaced at no line cost, and those SDKs gain correctness for POST reads.

Total: about 70 lines in 9 SDKs. Datadog and Zendesk no longer need `operations`.

**Also removes glue in:**
- **A:** read/write branches in error classification (Square `outcome_unknown`, Discord `failed_before_head`, PayPal `HttpError::read` vs write).
- **J:** a span field without a table.
- **F:** with F2, `is_paged()` comes from the same carrier.

**Hardest counterexample: Kraken (POST reads).** `#[read] scope account` states the fact once for
all three POSTs. Kraken's other per-scope rule, signing (`policy.rs:86-112`), is not read/write,
and E1 deliberately does not make "read" mean "unsigned" (see E4).

The Square and Datadog searches are covered: `#[read] POST search` is the provider's own sentence.

**Adyen:** every endpoint is a keyed POST, so a write. E1 says nothing about replay (see E2).

**`#[cfg]`-gated endpoints:** the fact sits in each endpoint's meta and travels with its cfg.

**Simple control (Nager):** unchanged. All three controls are GET-only, so `is_read()` is true and
no word is added.

**Charter check:**
- **C1:** `#[read]` is the provider's documented statement. It contains no negation.
- **C2:** it is a fact only; retry and uncertainty stay in policies.
- **C3:** Nager, PokeAPI and Frankfurter change by 0 lines.
- **C4:** the default is the method written on the same line, and nothing else is inferred (idempotency explicitly not).
- **C5:** a redundant `#[read]` is an error at the attribute ("`get` is a GET: it already reads").
- **C8:** the request bytes do not change.
- **C9:** tests: a compile-fail for a redundant attribute; a unit matrix of method × attribute × scope inheritance × cfg.

**New concepts:** the attribute `#[read]` (on endpoints and scopes) and `Operation::is_read()`.

**Breaking changes:** none (additive).

**Why it might be wrong:** "read" mixes two things: it does not change state, and it is safe to
repeat. A rate-charged POST read (the Datadog search) may still not deserve automatic repetition.
A provider whose GET has side effects needs a symmetric `#[write]` that no witness has needed yet.

---

## E2. `#[idempotency_key]`: deduplication is a role of a value

**Fact it expresses:** the provider deduplicates this write by the value of this header. For a
key carried inside the body, the fact is that the endpoint is deduplicated (`#[idempotent]`).

**Declaration (author writes):** PayPal, `paypal/lib.rs:82-92`. The same change applies to
`:108-118`, `:122-133`, `:142-152` and `:157-163`.

Before:
```rust
POST create(
    body order: OrderRequest,
    /// `PayPal-Request-Id`: PayPal performs the create at most once per ID within
    /// its retention (6 hours by default).
    request_id?: RequestId,
) -> Order {
    headers {
        "paypal-request-id" = "{request_id}";
        "prefer" = "return=representation";
    }
}
```
After:
```rust
POST create(
    body order: OrderRequest,
    /// `PayPal-Request-Id`: PayPal performs the create at most once per ID within
    /// its retention (6 hours by default).
    #[idempotency_key] header("paypal-request-id") request_id?: RequestId,
) -> Order {
    headers { "prefer" = "return=representation"; }
}
```
`paypal/lib.rs:71-73` (`operations PayPalOperation;`) is removed.

Square, `square-payments/lib.rs:69`: `POST create(body payment: CreatePayment)` becomes
`#[idempotent] POST create(body payment: CreatePayment)`, because the key is a body field
(`models.rs:20`).

Rules:

- The role is allowed only on `header(..)` placements. Every witness key is a header.
- `#[idempotent]` is the endpoint-level form for keys inside a body.
- Both forms are errors on a read.
- Concord never makes a key. Initializers stay the explicit way, and they run at endpoint
  construction, so clones keep the same key.

**Generated:**
- `Operation::idempotent() -> bool`: the endpoint declares a key, by role or by `#[idempotent]`.
- `Prepared::idempotency_key() -> Option<&HeaderValue>`: the key as it is about to be sent. `None` when an optional key was not set, or when the key is in the body.

**Call site (caller writes):** PayPal, `paypal/policy.rs:203-217`. Before:
```rust
let Class { write, accepts_request_id } = Class::of(operation);
let prepared = endpoint.prepare()?;
...
let request_id = prepared.request().headers().get("paypal-request-id")
    .and_then(|v| v.to_str().ok())
    .and_then(|v| RequestId::new(v).ok());
let permitted = !write || (request_id.is_some() && accepts_request_id);
```
After:
```rust
let write = !E::operation().is_read();
let prepared = endpoint.prepare()?;
...
let request_id = prepared.idempotency_key().and_then(RequestId::from_header);
let permitted = !write || request_id.is_some();
```

Square, `square-payments/error.rs:42-55`, where a 14-line `match` over `SquareOperation` becomes:
```rust
pub fn create_outcome_unknown(&self) -> bool {
    self.outcome_unknown() && self.operation().is_some_and(|op| op.idempotent())
}
```
End callers keep the same method names.

**SDK code that disappears:**
- **PayPal:**
  - `paypal/policy.rs:129-158` (29), `:203-206` (4), `:211-216` (5 net);
  - `SendOperation` becomes `SendWith` at `:195-202` (3);
  - `lib.rs:71-73` (3) and `lib.rs:89,115,130,149,162` (5 header lines).

  About 45 lines.
- **Square:**
  - `square-payments/lib.rs:57-59` (3);
  - `lib.rs:120-133`: the 13-line `SendOperation` adapter that ignores its operation;
  - `error.rs:42-55` (11 net).

  About 27 lines.
- **Zendesk:** `zendesk-support/policy.rs:104-109` (5 net), `lib.rs:87` (1). About 6 lines.
- **Adyen:** `adyen-checkout/policy.rs:306-309`, the header-name lookup (3 net).
- **Stripe:** `stripe/lib.rs:231, 245, 290, 304, 393, 438`, six header lines (6).

Total: about 85 lines in 5 SDKs. PayPal and Square no longer need `operations`.

**Also removes glue in:**
- **J:** log "keyed: yes" without the value.
- **A:** Zendesk's `UncertainWrite` and PayPal's `Uncertain` carry the key by role, not by header name.
- **D:** sensitivity stays a separate `#[sensitive]`. This keeps the Zendesk friction note (`FRICTION.md:26-36`, a literal header promoted to sensitive) from coming back.

**Hardest counterexample: Adyen (identity of the final request).** Adyen's ledger compares the
exact bytes sent under a key (`policy.rs:324-329`). E2 keeps Concord out of that comparison:

- the key is an ordinary declared placement;
- `prepare()` is byte-identical;
- the policy still builds `Identity` from the final `reqwest::Request`.

Concord only stops the SDK from spelling the header name. A design that minted the key inside
`send()` would differ between attempts and break the ledger. E2 forbids that by construction.

**Square:** the key is in the body, so the accessor is `None`. `#[idempotent]` states only that
the endpoint deduplicates.

**Simple control (Nager):** unchanged.

**Charter check:**
- **C1:** "PayPal deduplicates by `PayPal-Request-Id`" is the provider's contract.
- **C2:** whether to resend stays the policy's decision.
- **C4:** no header name is recognized implicitly (an `"idempotency-key"` literal stays an ordinary header).
- **C5:** the role on `query(..)`, on a body, or on a read is an error at the attribute.
- **C7/C8:** the bytes are unchanged, and there is no generated key.
- **C9:** wire test: `idempotency_key()` equals the declared header. Canary: a `#[sensitive]` key stays withheld from `Prepared`'s `Debug` (Stripe, Adyen).

**New concepts:** `#[idempotency_key]`, `#[idempotent]`, `Operation::idempotent()` and
`Prepared::idempotency_key()`.

**Breaking changes:** none (additive).

**Why it might be wrong:** providers keep keys for different windows (Zendesk 2 h, PayPal 6 h,
Adyen 7 days, company-wide). A "deduplicated" bit hides a window that a policy needs, so that
window is still restated in Rust. `#[idempotent]` cannot see whether Square's body actually
carries the key.

---

## E3. Capability crates read the facts: `Replay::of`, `Error::unanswered_write`

**Fact it expresses:** no new fact. Behavior reads E1 (read), E2 (key sent) and `Delivery`. This is
exactly the charter's split: "Retry, replay, traversal" read "Operation is a read, write,
idempotent…".

**Declaration (author writes):** unchanged from E1 and E2. There is no syntax.

**Generated / API:**
```rust
// concord-retry, feature `concord`
impl Replay {
    /// `Permitted` for a declared read, or a write whose declared idempotency key is sent;
    /// `Never` otherwise (labels included).
    pub fn of<T>(prepared: &concord::Prepared<T>) -> Replay;
}
// concord-core, next to `delivery()`
impl Error {
    /// A declared write that may have reached the provider without an answer:
    /// `!is_read() && delivery() == Delivery::Unknown`. A 5xx or an unreadable 2xx stay the SDK's rule.
    pub fn unanswered_write(&self) -> bool;
}
```

**Call site (caller writes):**

PayPal, `paypal/policy.rs:217-223`. Before:
```rust
let permitted = !write || (request_id.is_some() && accepts_request_id);
let replay = if permitted { Replay::Permitted } else { Replay::Never };
```
After: `let replay = Replay::of(&prepared);`.

Xero, `xero-accounting/policy.rs:46-60`. Before: `let read = request.method() == Method::GET; …
Retry::new(retry, if read { Replay::Permitted } else { Replay::Never })`. After:
`Retry::new(retry, Replay::of(&prepared))`.

Discord, `discord-rest/limiter.rs:93-94`: `if self.is_write() { AmbiguousWrite … }` becomes
`if error.unanswered_write()`.

**SDK code that disappears:**
- `paypal/policy.rs:217-223` (6 net).
- `strava/auth.rs:347-358`: 10 lines, together with moving its hand-rolled `run_tokio` template onto `receive_with`.
- `xero-accounting/policy.rs:46-57` (7).
- `discord-rest/limiter.rs:93-104` (4).
- `square-payments/error.rs:27-30` (3).
- Cloudflare's `create_failure` read gating (about 4).

Total: about 35-45 lines in 6 SDKs. The uncertainty rule (`uncertain`) is no longer hand-derived
anywhere.

**Also removes glue in:** A (Uncertain variants are built from one predicate) and J (the
attempt/replay decision becomes loggable).

**Hardest counterexample: Sentry repeats a *write* after a 429** (`sentry/policy.rs:186-190`:
"only its documented rate-limit rejection (429) is repeated"). `Replay::of` says `Never` for an
unkeyed write, so Sentry keeps `Replay::Permitted` and its classifier. "A 429 was not processed"
is a status fact (family A), not an operation fact. E3 must not guess it, and the gap shows where
A and E meet.

**Simple control (Nager):** unchanged.

**Charter check:**
- **C2:** behavior stays in capability crates, which only read facts.
- **C8:** `Replay::of` answers a question; nothing retries unless the SDK builds a `Retry`.
- **C6:** one sentence each.
- **C9:** a unit matrix (method × `#[read]` × key present/absent × label).

**New concepts:** `Replay::of` and `Error::unanswered_write`.

**Breaking changes:** none.

**Why it might be wrong:** every SDK's uncertainty rule differs (5xx? unreadable 2xx? a redirect
answer?). A core helper that covers only `Unknown` will be read as the whole rule. `Replay::of`
may also look like *permission* to retry a `#[read]` POST that the provider charges per call.

---

## E4. `class`: SDK-typed provider facts beside the endpoints

**Fact it expresses:** provider-specific per-operation facts that no standard word covers: a
quota bucket, a required OAuth scope, a signing realm, the meaning of a status for this operation.
They are stated where the endpoint is declared, as a value of the SDK's own type.

**Declaration (author writes):** Strava, `strava/lib.rs:80-116`.

Before:
```rust
send policy -> Result<Response<_>, Error>;
/// Every Strava operation, as the send policy classifies it (scope, quota windows,
/// retries, uncertainty).
operations StravaOperation;
scope athlete { path "athlete"; GET activities(..) -> Vec<Activity> { path "activities"; } }
scope activities { path "activities";
    GET get(..) -> Activity { path "{id}"; }
    PUT update(id: u64, body changes: ActivityUpdate) -> Activity { path "{id}"; request Json; } }
scope uploads { path "uploads";
    POST create(body file: UploadFile) -> UploadStatus { request StreamMultipart; }
    GET status(id: u64) -> UploadStatus { path "{id}"; } }
```
After:
```rust
send policy -> Result<Response<_>, Error>;
class Class;
scope athlete { path "athlete"; class Class::READ; GET activities(..) -> Vec<Activity> { path "activities"; } }
scope activities { path "activities"; class Class::READ;
    GET get(..) -> Activity { path "{id}"; }
    PUT update(..) -> Activity { path "{id}"; request Json; class Class::UPDATE; } }
scope uploads { path "uploads";
    POST create(body file: UploadFile) -> UploadStatus { request StreamMultipart; class Class::UPLOAD; }
    GET status(id: u64) -> UploadStatus { path "{id}"; class Class::POLL; } }
```

Kraken: `scope public { class Signing::PUBLIC; … }` and `scope account { class Signing::PRIVATE; … }`.

Rules:

- `class T;` on the client names the type, which must be `Copy`.
- Each endpoint takes the class of the nearest owner, by the same lookup as `limit`.
- There is **no client default**: an endpoint without a class is a compile error *at the endpoint*.
  This keeps H08's purpose (a new endpoint must be classified) while moving the error from the
  SDK's `match` to where the endpoint is written.
- The policy keeps the existing `SendOperation<T, Class>`, receiving the class instead of a variant.
- `Strava::class_of(Operation) -> Option<Class>` is generated for errors.

**Call site (caller writes):** Strava, `strava/auth.rs:283-307` and `:322-328`. Before:
```rust
fn of(operation: StravaOperation) -> Self {
    use StravaOperation as Op;
    match operation {
        Op::AthleteActivities | Op::ActivitiesGet => Self::READ,
        Op::UploadsStatus => Self { retry_delay: Duration::from_secs(1), ..Self::READ },
        Op::ActivitiesUpdate => Self { read: false, uncertain: true, scope: Some("activity:write"), ..Self::READ },
        Op::UploadsCreate => Self { read: false, non_upload: false, uncertain: true, ..Self::READ },
    }
}
... let Class { read, retry_delay, non_upload, uncertain, scope } = Class::of(operation);
```
After:
```rust
async fn send<E: Endpoint<T> + Send>(&self, class: Class, endpoint: E) -> Self::Output {
    let Class { retry_delay, non_upload, scope, .. } = class;
```
The `const`s `Class::UPDATE`, `UPLOAD` and `POLL` replace the match arms. With E1, `read` and
`uncertain` leave `Class` altogether.

**SDK code that disappears:**
- `strava/auth.rs:283-307` (23 lines, minus 5 statements in the declaration) and `lib.rs:81-83` (3).
- `kraken-spot-rest/policy.rs:86-90` (the match arms become `if class == Signing::PRIVATE`, 4 lines), `lib.rs:38-39` (2), and `prepare_signed` uses `Kraken::class_of`.
- Zendesk's collision class, if not done through E1.

Net: about 25-35 lines in 3 SDKs. The main gain is not the count but the location: a new endpoint
is one edit, at the endpoint (C5).

**Also removes glue in:** rate limiting (Strava's quota bucket is a declared value that the
`concord-ratelimit` keys can read) and auth placement (Kraken's signing realm per scope).

**Hardest counterexample: `#[cfg]`-gated endpoints.** A missing class on a cfg'd endpoint must fail
in every configuration. It is checked "as if every cfg were on", as names already are (author
guide §8). Policies that ignore operations (Square, Nager) write no `class`, so nothing is
generated. Sentry still needs *identity* (its limiter is keyed by operation,
`sentry/policy.rs:70-83`), so it keeps `operations`. The two statements are mutually exclusive on
one client.

**Simple control (Nager):** unchanged.

**Charter check:**
- **C1:** borderline. The word `class` is plumbing; its values (Strava's upload exemption, `activity:write`) are provider facts.
- **C2:** the values are data; the policy decides.
- **C4:** nearest wins, as for `limit` and `format`.
- **C5:** "`uploads.create` has no `class`: write `class Class::…;` on it or on `scope uploads`", reported at the endpoint.
- **C6:** one rule.
- **C9:** compile-fail for a missing class, including a cfg'd endpoint.

**New concepts:** `class T;`, `class <const expr>;` and the generated `Client::class_of`.

**Breaking changes:** `operations` and `class` are exclusive per client. SDKs that move from one to
the other change their policy's `O`.

**Why it might be wrong:** it moves a 25-line table into the declaration more than it deletes
one. An SDK-invented `Class` also invites behavior into the declaration: Strava's `retry_delay`
is a `Duration` inside `Class::POLL`, which is the V2/V3 drift the charter forbids.

### Seeds rejected or folded (E)

- **Seed 1, "always generate the enum": rejected.**
  - **C3:** Nager, PokeAPI and Frankfurter would gain a public enum in their rustdoc.
  - With `send`, the enum would force `SendOperation` on Nager's concrete `impl SendWith<Vec<Holiday>> for KeepHead` (`nager-holidays/lib.rs:62`).
  - After E1, E2 and F2, only Strava, Sentry, Kraken and GitHub still need an enum or a class.
- **`#[uncertain]`: rejected as a fact.** In every witness, uncertainty is write ∧ no answer
  (∧ the SDK's own 5xx rule); Strava's `uncertain` equals `!read` in all five arms. It is E3's
  derivation. Datadog's "202 required" (`api.rs:121`) is a success-status fact for family A/B.
- **Seed 3, "groups on scopes": folded** into E1 (`#[read]` on a scope) and E4 (`class` on a scope).

---

# Family F: paging facts

## F1. `Pages` starts anywhere and refuses repeated keys (the "smaller than you think" design; no DSL change)

**Fact it expresses:** none in the declaration. It removes two restatements that every paging SDK
writes: "a traversal can start at a position" (the `_from` twin) and "a cursor the provider
repeats is a protocol error". Datadog's FRICTION.md §2 (lines 51-62) proposes exactly the second
one: "Refusing a key the traversal already requested is generic. Smallest change … an opt-in
`Pages::refuse_repeated_keys()`".

**Declaration (author writes):** unchanged.

**API (concord-pagination):**
```rust
impl<T, K, M, E> Pages<T, K, M, E> {
    /// The same traversal started at `from` instead (the factory is kept, counters reset).
    pub fn starting_at(self, from: Resume<K>) -> Self;
}
impl<T, K: Eq + Hash + Clone, M, E> Pages<T, K, M, E> {
    /// A page whose `next` is a key this traversal already requested ends it with
    /// `PagesError::Repeated { resume }` (no request is made for it).
    pub fn refuse_repeated_keys(self) -> Self;
}
```

**Call site (caller writes):**

Datadog, `client.rs:154-161`. Before: `client.search_pages_from(request, Resume::At(cursor))`.
After: `client.search_pages(request).starting_at(Resume::At(cursor))`.

Notion. Before: `notion.children_from(id, size, err.resume().cloned().unwrap())`. After:
`notion.children(id, size).starting_at(err.resume().cloned().unwrap())`.

The SDK writes one method per listing. The H09c prose "Use `…::Resume` for the position.
`Resume::Start` repeats … `Resume::At(key)` takes …" moves to one sentence in concord-pagination.

**SDK code that disappears:**
- **Twins:** about 85 lines in 12 SDKs.
  - `datadog-events/client.rs:148-157` (7)
  - `notion/client.rs:135-139, 165-169` (10)
  - `digitalocean/pagination.rs:27-30, 45-53, 67-75, 89-97` (28)
  - `discord-rest/api.rs:119-131` (6)
  - `github/paging.rs:389-400` (6)
  - `stripe/pages.rs:419-433` (6)
  - `microsoft-graph/paging.rs:181-189` (5)
  - `cloudflare-zones/pagination.rs:55-77` (10)
  - `distribution-registry/pages.rs:95-116` (10)
  - `gitlab/pages.rs:101-110` (5)
  - `spotify-web-api/pages.rs:57-67` (4)
- **Repeat checks:** about 45 lines in 9 SDKs.
  - `datadog-events/client.rs:163-172` (8)
  - `notion/client.rs:151-158, 181-188, 210-211` (12)
  - `zendesk-support/pagination.rs:47-49` (3)
  - `stripe/pages.rs:475-480` (6)
  - `github/paging.rs:634-638` (5)
  - `microsoft-graph/paging.rs:153-155` (3)
  - `sentry/pagination.rs:129-134` (6)
  - `elasticsearch/pit.rs:466-468` (3)
  - `kubernetes/list.rs:312-314` (3)
- **Docs:** 28 copies of the `Resume` sentence.

Total: about 130 code lines in 14 SDKs. The error variants `CursorCycle`, `RepeatedCursor`,
`CursorDidNotAdvance`, `LinkDidNotAdvance`, `ContinueDidNotAdvance` and `StalledCursor` collapse
into one `PagesError::Repeated`.

**Also removes glue in:** H (the Resume docs), G (twins are forwarders) and A (six SDK error
variants with their `Display` arms).

**Hardest counterexample: Mailgun (eager cursor validation).** `mailgun/events.rs:263-271` refuses
a cursor of another search or credential *before the traversal exists*, returning
`Err(LinkError::Context)`. `starting_at` has nowhere to check eagerly. Mailgun either keeps its one
`pages_from`, or moves the check into the factory, where it fails at the first `next_page()`:
still before any I/O, but later than today. F5 restores eagerness.

**"A continuation that fails after records were accepted":** unchanged. `Incomplete` keeps the
prefix, and `starting_at(failed)` resumes from the failed page.

**Simple control (Nager):** unchanged. None of the three controls has a paging helper.

**Charter check:**
- **C2:** traversal behavior lives in the paging crate.
- **C6:** removes the H09c `Resume` paragraph from every SDK.
- **C7:** `Pages::new(from, factory)` is unchanged for engine users.
- **C9:** tests: a repeated key ends with `Repeated` and its resume position, after the right number of requests; `new(from, f)` ≡ `new(Start, f).starting_at(from)`.

**New concepts:** `Pages::starting_at`, `Pages::refuse_repeated_keys`, `PagesError::Repeated` and
`Outcome::Repeated`.

**Breaking changes:** a new `PagesError` variant. The enum is not `#[non_exhaustive]`
(`pages.rs:691`), so exhaustive matches in callers need an arm.

**Why it might be wrong:**
- Remembering every requested key costs memory proportional to the pages read (bounded by `max_pages`, unbounded without it).
- A provider that legitimately repeats a position (a tail-polling cursor) must simply not opt in, so the check stays an SDK choice rather than a guarantee.

---

## F2. `#[cursor]`: the cursor is a role of a request value

**Fact it expresses:** which request value carries the position: a query value, a header, an
exclusive `query(..)` set, or the body.

**Declaration (author writes):**

Zendesk, `zendesk-support/lib.rs:71-72`. Before:
```rust
/// `page[after]`: the previous page's `meta.after_cursor`.
query("page[after]") after?: String,
```
After:
```rust
/// `page[after]`: the previous page's `meta.after_cursor`.
#[cursor] query("page[after]") after?: String,
```

Square, `square-payments/lib.rs:78`: `GET list(#[sensitive] query cursor?: String, query limit?: u32)`
becomes `GET list(#[cursor] #[sensitive] query cursor?: String, query limit?: u32)`.

Kubernetes, `kubernetes/api.rs:69` (inside `params ListOptions`): `#[cursor] #[sensitive] query("continue") continue_token?: ContinueToken`.

Body cursors (Intercom `lib.rs:93`, Datadog `api.rs:60`, Notion `api.rs:23`):
`#[read] POST search(#[cursor] body search: EmailSearch) -> ContactList { path "search"; }`. The
body type says where the cursor sits inside it:
```rust
impl concord::CursorField for EmailSearch {         // replaces intercom/models.rs:81-84
    type Key = String;
    fn set_cursor(&mut self, key: Option<String>) { self.pagination.starting_after = key; }
}
```

**Generated:**
- `impl concord::Cursor for ZendeskTicketsList { type Key = String; fn at(self, key: Option<String>) -> Self }`. This is a role-named setter, which also gives Stripe's FRICTION.md the "getter" it lacked (`has_cursor()`, lines 118-121).
- `Operation::is_paged() -> bool`.
- In concord-pagination: `Pages::over(template, project)` for `E: Cursor + Sends<O> + Clone` (or `Endpoint<T>` without a policy). It sets the key on a fresh clone for each page, sends it through the client's policy, and calls `project`.

**Call site (caller writes):** Intercom, `intercom/pages.rs:19-43`, where two traversals differ
only by cursor placement (the comment at `:45-46` says so). Before:
```rust
impl IntercomContactsList {
    pub fn pages(self, from: Resume<String>, max_pages: NonZeroUsize) -> ContactPages {
        contact_pages(from, max_pages, move |cursor| self.clone().maybe_starting_after(cursor).send())
    }
}
impl IntercomContactsSearch {
    pub fn pages(self, from: Resume<String>, max_pages: NonZeroUsize) -> ContactPages {
        contact_pages(from, max_pages, move |cursor| {
            let mut op = self.clone();
            op.search = op.search.with_starting_after(cursor);
            op.send()
        })
    }
}
```
After (one function for both; the start position comes from F1):
```rust
pub fn contact_pages<E>(template: E) -> ContactPages
where E: concord::Cursor<Key = String> + Sends<Result<Response<ContactList>, Error>> + Clone + Send + 'static
{ Pages::over(template, project) }
```
Application callers are unchanged, apart from F1's `starting_at`.

**SDK code that disappears:**
- `stripe/pages.rs:126-193`: the `ListRequest` trait and `list_request!` for seven operations (61 lines). They become generated `Cursor` impls plus a 10-line exclusive `Position` value. About 50 lines.
- `sentry/pagination.rs:15-43` (the `listing!` macro becomes one generic fn: 18) and `sentry/policy.rs:94-95, 103-114, 275-284` (`paged` becomes `E::operation().is_paged()`: 8).
- `intercom/pages.rs:19-43` (15).
- `square-payments/lib.rs:135-159` (17).
- `datadog-events/query.rs:259-268` (6 net).
- `kubernetes/list.rs:67-70` (3).

Total: about 115 lines in 7 SDKs.

**Also removes glue in:**
- **E:** `is_paged` removes Sentry's `paged`, and the "is a listing" half of GitHub's `listing_id`.
- **D/J:** the cursors already marked `#[sensitive]` (Sentry, Square, Kubernetes) keep their redaction through the role.
- **G:** per-listing wrappers.

**Hardest counterexample: Discord (unordered pages).** `#[cursor] query(..) anchor?: Anchor`
(`discord-rest/api.rs:91`) works: the key is `Anchor::Before(id)`. The order check and the
"short page ends" rule (`api.rs:162-177`) stay in Discord's `project`; F2 only standardizes how the
cursor is set.

**Kubernetes:** a continued request must drop `resourceVersion`/`resourceVersionMatch`
(`list.rs:17-19`). The provider's exclusivity becomes a type rule with one exclusive value,
`#[cursor] query(..) position?: ListPosition` (author guide §4.1a), instead of
`with_resource_version(None, None)`. The snapshot check stays in `into_page`.

**Simple control (Nager):** unchanged. PokeAPI's `offset` is not marked, and marking it would be
opt-in.

**Charter check:**
- **C1:** the provider's own doc line ("`page[after]`: the previous page's `meta.after_cursor`").
- **C2:** the loop stays in concord-pagination.
- **C4:** no name convention (`cursor`, `after` and `starting_after` are not recognized).
- **C5:** two `#[cursor]` on one endpoint is an error ("use one `query(..)` value for exclusive cursors"), and so is `#[cursor]` on a required value ("the first page has no cursor").
- **C7:** generated setters are unchanged.
- **C9:** unit: `Cursor::at` produces the same request as the setter; compile-fail for the two errors above.

**New concepts:** `#[cursor]`, `concord::Cursor`, `concord::CursorField`, `Operation::is_paged`
and `Pages::over`.

**Breaking changes:** none (additive). Stripe changes its public setters only if it adopts an
exclusive `Position`.

**Why it might be wrong:** the projection, half of the paging glue, stays hand-written. Body
cursors need a second mechanism (`CursorField`) for the same fact, so authors must learn which
one applies.

---

## F3. `page items …, next …, more …`: where the continuation sits in the response

**Fact it expresses:** in the decoded page, where the records are, where the next cursor is, and
which field says whether more pages exist.

**Declaration (author writes):** Zendesk, `zendesk-support/lib.rs:68-77`.

Before:
```rust
GET list(
    query("page[size]") page_size: PageSize = PageSize::MAX,
    query("page[after]") after?: String,
    query sort?: TicketSort,
    query external_id?: String,
) -> TicketPage;
```
After:
```rust
GET list(
    query("page[size]") page_size: PageSize = PageSize::MAX,
    #[cursor] query("page[after]") after?: String,
    query sort?: TicketSort,
    query external_id?: String,
) -> TicketPage {
    page items tickets, next meta.after_cursor, more meta.has_more;
}
```

Other witnesses, one line each:
- Stripe `lib.rs:189-210`: `page items data, next data[last].id, more has_more;` (forward direction).
- Intercom `lib.rs:87-93`: `page items data, next pages?.next?.starting_after;` (both `list` and `search`).
- Datadog `api.rs:45-60`: `page items data, next meta?.page?.after;`.
- Notion `api.rs:23-31`: `page items results, next next_cursor, more has_more;`.
- Square `lib.rs:78`: `page items payments, next cursor;`.

Grammar:

- Paths are **Rust field paths of the declared output type**, with `?` through an `Option` and
  `[first]`/`[last]` on a `Vec`.
- `next` ends in `K` or `Option<K>`; absent means the last page.
- `items` defaults to the output itself only when the output is a `Vec`, which is visible in `->`.
- The generated code is plain field access, spanned at the path, so `meta.after_cursr` is rustc's
  E0609 at the declaration token.

**Generated:** `impl concord::Paged for ZendeskTicketsList { type Item = Ticket; fn split(TicketPage) -> Result<(Vec<Ticket>, Option<String>), PageProblem> }`.
Its only built-in checks are `more`/`next` consistency: `more` with no `next` is
`MissingCursor`, and `next` with `more == false` is `Inconsistent`. These are the checks Notion
`client.rs:206-207` and Stripe `pages.rs:468-472` write by hand.

concord-pagination's `Paginate` trait (re-exported by the SDK) gives every `Paged` endpoint
`.pages()`.

**Call site (caller writes):**
- Square. Before: `api.payments().list().pages(Resume::Start, cap)`. After: `api.payments().list().pages().max_pages(cap)`.
- Zendesk. Before: `ticket_pages(api.tickets().list(), Resume::Start, cap)`. After: `api.tickets().list().pages().max_pages(cap)`.

**SDK code that disappears:**
- `zendesk-support/pagination.rs:1-61` (56; the `links.next` cross-check at `:50-58` goes or becomes a hook).
- `notion/client.rs:196-222` (project and validate, 25) and `:119-133` (cursor plumbing, 6).
- `square-payments/lib.rs:135-159` (22).
- `intercom/pages.rs:1-79` (65).
- `datadog-events/client.rs:179-198` (18) and `model.rs:37-53` (`after`/`next_cursor`, 12).
- `stripe/pages.rs:456-494` (`project`, 35). Stripe's backward direction and binding stay.

Total: about 230 lines in 6 SDKs, overlapping F2 on Square and Intercom. F2 and F3 together come
to about 300.

**Also removes glue in:** B (the `items` path is the list-envelope unwrap that family B counts as
result glue), E (`is_paged` for free) and H (one shared paging page instead of per-SDK prose).

**Hardest counterexample: Elasticsearch (partial results).** A page with `timed_out` or
`_shards.failed > 0` (`elasticsearch/pit.rs:446-452`) would be split by generated code as an
ordinary page: a silent partial listing. Elasticsearch must not use `page`. It keeps a
hand-written `Paged` impl, which it needs anyway, because its cursor is the last hit's `sort`
plus a PIT id that each page replaces (`pit.rs:442-445`).

The general rule is that a partial-result signal is refused by the send policy before paging sees
the page, as Notion already does (`client.rs:105-114`).

- **Kubernetes:** `next metadata.continue` works, but snapshot consistency (`list.rs:304-309`) is
  a cross-page rule, so Kubernetes keeps a hand impl.
- **Discord:** the end is a short page, so it keeps a hand impl.
- **Empty cursors** are provider-specific: Square ends on one (`lib.rs:153`), while Datadog
  refuses one (`client.rs:181-186`). `page` builds in neither; the model's serde decides
  (`Option` vs a non-empty newtype).

**Simple control (Nager):** unchanged. PokeAPI's `next` is a URL (see F4) and it does not opt in.

**Charter check:**
- **C1:** each clause copies a provider doc sentence ("`has_more` decides whether another page exists").
- **C3:** 0 lines for the controls.
- **C4:** the only default (`items` = the output `Vec`) is visible.
- **C5:** a wrong path is rustc's error at the path token.
- **C6:** one statement, three clauses.
- **C8:** `pages()` is lazy, and nothing is sent until it is polled.
- **C9:** fixture tests for `more`/`next` contradictions, and compile-fail for a wrong path.

**New concepts:** the `page` statement (`items`, `next`, `more`), `concord::Paged` and
`Paginate`.

**Breaking changes:** none (additive). It needs F2's `#[cursor]`. It also needs the policy's
output to expose the decoded value: `Result<T, E>` works today, while `Result<Response<T>, E>`
needs family B's core `Response<T>` or a 3-line impl per SDK.

**Why it might be wrong:** field paths tie the declaration to Rust model names, so a serde
rename is invisible in the declaration. The clause vocabulary will be asked to grow (`more
nonempty`, totals, `[first]` for backward paging) until it becomes an expression language.

---

## F4. `page next link` / `page next url <path>`: provider-issued URLs, followed through the policy

**Fact it expresses:** the next page is a URL the provider issues, in a `Link: rel="next"` header
or in a body field. It is requested verbatim as the same endpoint.

**Declaration (author writes):**

GitHub, `github/api.rs:80-81` (its doc already says "follow the `Link` header", `api.rs:11-12`).
Before:
```rust
/// `GET /repos/{owner}/{repo}/labels` (`issues/list-labels-for-repo`).
GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; }
```
After:
```rust
/// `GET /repos/{owner}/{repo}/labels` (`issues/list-labels-for-repo`).
GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; page next link; }
```

DigitalOcean, `digitalocean/lib.rs:39`: `GET list(..Paging, query tag_name?: TagName) -> Droplets;`
becomes `GET list(..Paging, query tag_name?: TagName) -> Droplets { page items droplets, next url links?.pages?.next; }`.

Mailgun, `mailgun/lib.rs:83-93`: `page items items, next url paging.next, more nonempty;` (an
empty page is the last, `events.rs:94`).

Graph, `microsoft-graph/api.rs:15-17`: `page items value, next url next;`.

**Generated / Concord:**
- The key is `concord::NextUrl`. It is opaque, and its `Debug` is withheld.
- A followed page is the same endpoint through `Prepared::follow`, **and is sent through the
  client's send policy** (`Followed<E>: Sends<O>`). Auth refresh, rate admission and error
  documents therefore apply to every page.
- The trust rule is deterministic and comes from the declaration: same origin as the endpoint;
  path at or below the client's declared base path, which is GitHub's `is_api_url`
  (`context.rs:205-213`); no userinfo; no fragment.
- A link equal to the requested URL is F1's `Repeated`.
- `Link` parsing (RFC 8288) moves from `concord-pagination`'s `link` feature into core, with
  unquoted parameter values (Sentry FRICTION.md lines 7-14: "Parameter exposes only `raw_value`").

**Call site (caller writes):**
- GitHub. Before: `repo.label_pages(repo.handle().labels()).pages()`. After: `repo.handle().labels().pages()`.
- DigitalOcean. Before: `api.droplet_pages_from(size, cap, Resume::At(link))`. After: `api.droplets().list().set_per_page(size).pages().max_pages(cap).starting_at(Resume::At(link))`.

**SDK code that disappears:**
- **GitHub:**
  - `github/link.rs:1-175` (159);
  - `paging.rs:589-646` (link extraction, foreign-link and advance checks: about 50);
  - `pages.rs:47-198` (the 13 per-listing factories: about 150, except the child-listing binding, see F5).

  About 330 lines.
- **Sentry:** `sentry/pagination.rs:45-136` (92). About 12 lines stay for Sentry's own
  `results="false"` parameter, so about 80 go.
- **DigitalOcean:** `digitalocean/pagination.rs:112-195` (the `Listing` trait, `pages` and
  `validate`: about 80). About 10 lines stay for `tag_name` (see F5), so about 70 go.
- **Mailgun:** `mailgun/events.rs:273-338` (about 60, of which about 10 stay), so about 50 go.
- **Graph:** `microsoft-graph/paging.rs:95-179` (the link parts) and `links.rs:98-122`. About 40 lines.

Total: about 570 lines in 5 SDKs. GitHub dominates.

**Also removes glue in:**
- **A/B:** DigitalOcean (`pagination.rs:150`), Mailgun (`events.rs:289`) and PokeAPI (`lib.rs:216`) call their policy's finisher by hand because a followed `Prepared` cannot go through `send()`. That bypass is error and result glue.
- **G:** GitHub's 13 factories are forwarders.
- **D:** the hand-written `NextLink(<redacted>)` and `Links(<redacted>)` `Debug` impls.

**Hardest counterexample: Sentry.** Sentry's `Link` is always present, with `results="true|false"`
(`pagination.rs:94-98`), and the SDK uses the cursor *parameter*, never the URL (`model.rs:45`).
F4 handles the grammar, trust and repetition. Sentry keeps a 10-line hook for `results`, and
either accepts following the URL or keeps F2 plus that hook.

**GitHub:** links may address the repository by id (`/repositories/1300192/issues?page=2`,
`paging.rs:6-8`). "Below the base path" admits them; a same-path rule would not.

**DigitalOcean** today checks more: the same path and the same `tag_name` (`pagination.rs:175-183`).
Adopting F4 relaxes that unless it keeps the hook or uses F5.

**Simple control (Nager):** unchanged. PokeAPI could opt in and delete `lib.rs:208-217`, but under
C3 it is not required to and stays as is.

**Charter check:**
- **C1:** "GitHub paginates with `Link: rel="next"`" is the provider's pagination guide.
- **C2:** the follow is a request of the same declared endpoint; the loop is concord-pagination's.
- **C4:** the trust rule is derived from the declared base path and has no knob.
- **C7:** `Prepared::follow` stays public for SDKs with other rules.
- **C8:** one request per page, through the SDK's own policy.
- **C9:** canary: a followed URL is withheld in `Debug`; wire: cross-origin and outside-base links are refused before I/O; a test proves the policy runs on page 2.

**New concepts:** `page next link`, `page next url <path>`, `concord::NextUrl`,
`concord::Followed<E>` and the `Link` parser in core.

**Breaking changes:** `concord_pagination::link` moves into core (a re-export keeps the old path).

**Why it might be wrong:**
- "Below the base path" is weaker than what DigitalOcean, Sentry and Graph check today (the same collection path), so adoption silently relaxes their rules unless they keep hooks.
- Sending a followed URL through the policy assumes that the policy never rewrites the URL. QuickBooks' `select_environment` rewrites the host (`quickbooks-online/policy.rs:35-36`).

---

## F5. The listing's identity is the declared request minus its cursor

**Fact it expresses:** no new fact. It is derived from F2's `#[cursor]` or F4's `page next ..`:
every placement that is not the cursor defines which listing a cursor belongs to. This is the
"binding" that Stripe (FRICTION.md lines 118-121), GitHub, Graph, Mailgun and DigitalOcean each
re-derive. concord-pagination documents it as "not detected" (`lib.rs` module docs, "Progress and
storage").

**Declaration (author writes):** unchanged. It requires F2 or F4.

**Generated / API:**
- `Prepared::listing() -> concord::Listing` is a digest of the method, the path, every non-cursor query pair and every non-cursor header.
- Sensitive values enter only an in-memory digest with a per-process salt, never the persistable form.
- concord-pagination keys become `Bound<K>` = (key, listing).
- `Pages::starting_at(Resume<Bound<K>>) -> Result<Self, Foreign>` refuses **eagerly**, before any I/O.
- `Bound::to_stored()` and `restore()` handle the versioned form, which carries only the non-sensitive part.

**Call site (caller writes):** Mailgun, `mailgun/events.rs:258-271`. Before:
```rust
pub fn pages_from(self, from: Resume<EventCursor>, max_pages: NonZeroUsize) -> Result<EventPages, Error> {
    let prepared = self.clone().prepare()?;
    let original = prepared.request().url().clone();
    let credential = prepared.request().headers().get("authorization").cloned();
    if from.key().is_some_and(|key| key.original != original || key.credential != credential) {
        return Err(Error::Link(LinkError::Context));
    }
```
After: `events.pages(cap)?.starting_at(from)?`, which is the same eager refusal, now
`PagesError::Foreign`.

**SDK code that disappears:**
- **Stripe:** `stripe/pages.rs:204-252` (the binding in `Cursor`: about 20), `:279-294, 310-313, 364-375` (about 20) and `:496-520` (`has_cursor` and `fingerprint`: 22). About 60 lines.
- **GitHub:** `github/paging.rs:153-280` (`Binding`, `NextLink`, `to_stored`, `parse_stored`: about 110), `:432-488` (`restore`: about 45) and `:490-579` (`binding_of`, `below`, `repo_part`: about 75). About 40 stay for id-addressed paths, so about 190 go.
- **Graph:** `microsoft-graph/links.rs:9-62`. About 10 stay for the auth-context part, so about 35 go.
- **Mailgun:** `mailgun/events.rs:239-245, 263-271` (about 15).
- **DigitalOcean:** `digitalocean/pagination.rs:22-24, 180-182, 189-195` (`FILTERS`/`values`: about 12).

Total: about 310 lines in 5 SDKs.

**Also removes glue in:**
- **D:** stored continuations no longer need hand-written redacted `Debug` (Graph `links.rs:88-92`, Stripe `pages.rs:245-252`).
- **Construction:** Stripe's binding includes `Stripe-Account`, which the "account override" handles today by hand.

**Hardest counterexample: Mailgun (eager cursor validation).** Handled exactly: `Foreign` is
returned at `starting_at`, before the traversal exists.

**GitHub:** a stored link may address the repository by id. The binding is computed from the
*template*, not from the link URL, so it still matches. GitHub's own check that a stored URL names
the same collection (`paging.rs:490-539`) is a provider rule that it keeps in a smaller form.

**Kubernetes:** snapshot consistency is a different invariant (consecutive pages agree), not
identity, and is unchanged.

**Simple control (Nager):** unchanged.

**Charter check:**
- **C2:** the identity is derived; the refusal is the paging crate's decision.
- **C4:** deterministic: the identity is the declared request minus the declared cursor.
- **C8:** no request is made to validate.
- **C9:** tests: a cursor from a filtered listing is refused on an unfiltered one before I/O. Canary: neither the stored form nor `Debug` contains a sensitive value.

**New concepts:** `concord::Listing`, `Bound<K>` and `PagesError::Foreign`.

**Breaking changes:** the stored continuation formats of GitHub (`github-link/1`) and Graph
change.

**Why it might be wrong:** "every header but the cursor" is not always the provider's identity. A
rotated token must not invalidate a GitHub continuation, but must invalidate Mailgun's, and
salting sensitive values per process makes the first case fail in-process. A persisted digest is
also a format Concord must then keep stable forever.

---

## F6. Numbered positions: `#[cursor(page)]`, `#[cursor(offset)]`

**Fact it expresses:** the position is a 1-based page number or an item offset that the client
computes. The end is signalled by an empty page or by a declared total.

**Declaration (author writes):**

Strava, `strava/lib.rs:89-92`. Before:
```rust
GET activities(query page: u32 = 1, query per_page: u16 = 30,
    query before?: u64, query after?: u64) -> Vec<Activity> {
    path "activities";
}
```
After:
```rust
GET activities(#[cursor(page)] query page: u32 = 1, query per_page: u16 = 30,
    query before?: u64, query after?: u64) -> Vec<Activity> {
    path "activities";
    page more nonempty;
}
```

Cloudflare: `#[cursor(page)]` on `page` inside its `params Paging`, and
`page items result, total result_info?.total_pages;`.

Kraken: the offset sits in the form body (`TradesHistoryOptions.ofs`), so it uses `CursorField`
with `#[cursor(offset)]`, plus `page items trades, total count;`. The arithmetic is
concord-pagination's existing `next_offset` (`offset.rs:53`).

**Generated:**
- The next key is the page + 1, or the offset + `items.len()`.
- The end comes from `more nonempty` or `total`.
- With `total`, the caller can choose `first_page_then` (independent pages); that choice stays the caller's.

**Call site (caller writes):** Strava. Before: `athlete.pages(per_page, Resume::Start, cap)?`
(`lib.rs:216-221`). After: `athlete.activities().set_per_page(per_page).pages().max_pages(cap)`.

**SDK code that disappears:**
- `strava/lib.rs:213-265`: about 45 lines, of which about 8 stay (the `per_page` range check and `PageMeta`). About 37 go.
- `cloudflare-zones/pagination.rs:79-140`: about 55 lines, of which about 20 stay (the `result_info.page == requested` echo check, `:114-122`). About 35 go.
- `kraken-spot-rest/trades.rs:65-94`: about 27 lines, of which about 10 stay. About 17 go.

Total: about 90 lines in 3 SDKs.

**Also removes glue in:** H, since Strava's and Kraken's Resume prose (`lib.rs:215`,
`trades.rs:44`) goes.

**Hardest counterexample: Discord (unordered pages).** Its key is an item id, not a number, so F6
does not apply, and its ordering check stays in Rust.

**Kraken:** offsets over a live list shift when new trades arrive, so an item can appear twice
(`trades.rs:42-44`). F6 cannot fix this (there is no snapshot) and the docs must still say so.

**Simple control (Nager):** unchanged. PokeAPI's `limit`/`offset` would be a natural opt-in, but
under C3 it does not adopt it.

**Charter check:**
- **C1:** "page numbers start at 1" and "an empty page is the last" are provider documentation.
- **C4:** no arithmetic beyond +1 and +len.
- **C5:** `#[cursor(page)]` on a non-integer is an error at the attribute.
- **C9:** unit tests for overflow (`page.checked_add(1)`, today hand-written at `strava/lib.rs:243-244`).

**New concepts:** the `page` and `offset` kinds of `#[cursor]`, the clauses `more nonempty` and
`total <path>`.

**Breaking changes:** none (additive).

**Why it might be wrong:** two more clause words push `page` toward a small language (see F3).
Numbered paging is also exactly where providers add quirks: Cloudflare's echoed page, Strava's
"empty page is conservative" (FRICTION.md line 23), Kraken's shifting offsets.

---

## How the designs combine, and what crosses families

| Design | New words | SDK lines | Families it also removes glue in |
|---|---|---|---|
| E1 `#[read]` + `is_read()` | 1 attribute, 1 method | ~70, 9 SDKs | A, J, F (via `is_paged`) |
| E2 `#[idempotency_key]` | 2 attributes, 2 methods | ~85, 5 SDKs | A, J, D |
| E3 `Replay::of`, `unanswered_write` | 2 functions | ~40, 6 SDKs | A, J |
| E4 `class` | 1 statement | ~30 net, 3 SDKs | rate limits, auth placement |
| F1 `starting_at`, `refuse_repeated_keys` | 2 methods, 1 error variant | ~130 + 28 doc lines, 14 SDKs | H, G, A |
| F2 `#[cursor]` | 1 attribute, 2 traits | ~115, 7 SDKs | E, D/J, G |
| F3 `page items/next/more` | 1 statement | ~230, 6 SDKs | B, E, H |
| F4 `page next link/url` | 2 clause forms, `NextUrl` | ~570, 5 SDKs | A/B (policy bypass), G, D |
| F5 listing identity | `Listing`, `Bound<K>` | ~310, 5 SDKs | D, C |
| F6 numbered positions | 2 cursor kinds | ~90, 3 SDKs | H |

The counts overlap: F2 and F3 both count Square and Intercom, and F4 and F5 both count GitHub's
`paging.rs`.

**Dependencies:**
- F3, F4 and F6 need F2's role.
- F5 needs F2 or F4.
- E3 needs E1 and E2.
- F3 and F4 need family B to let concord-pagination reach the decoded value inside an SDK result wrapper (`Result<Response<T>, E>`).

**Smallest coherent first step:** E1 together with F1. The pair adds one attribute and four
library items, needs no new statement, and removes about 200 lines and 28 doc sentences in 20
SDKs. After it, `operations` survives only in Strava, Sentry, Kraken and GitHub, where per-operation
identity or provider-specific classes are real.

**Mechanism shared across families:**
- `concord::Operation` already travels in every `Prepared`, `Received`, `Error` and `ResponseMeta`. Putting E1/E2/F2 facts (`is_read`, `idempotent`, `is_paged`) on it lets error classification (A), logging (J), retry (E3) and paging (F) read one carrier, with no enum and no new trait.
- F4's "followed request goes through the policy" closes the one gap that forces DigitalOcean, Mailgun and PokeAPI to finish responses by hand. The same gap blocks generated paging in every link-paged SDK.
