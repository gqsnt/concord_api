# J. Observability: Concord logs safely — merged ideas
Sources: DJM-claude.md, DJM-codex.md. 7 lane designs → 5 entries: 3 KEEP, 1 KILL, 1 MERGED.

Evidence checked at `ba10189b` in `work/wt/rv-p1` (read-only); "checked" = opened or counted in this merge, "lane estimate" = not re-derived. Family J is a missing capability (no SDK logs today, checked: no `tracing`/`log` dependency in any `*-sdk/Cargo.toml`), so most entries remove ≈ 0 SDK lines; they are ranked by capability per concept, then glue.

## J1. One static call event under a `tracing` feature — KEEP (rank 1)
Sources: DJM-claude.md § J1, DJM-codex.md § J1; variant DJM-claude.md § J3 (entry J4).
Fact: nothing new — the declared identity (`Client.scope.endpoint`), method and route template, plus the observed status, timing and delivery.
Mechanism: optional feature `tracing` of concord-core, off by default. `Decoder::execute` (`crates/concord-core/src/prepared.rs:600-613`, checked) emits one event (target `concord`) when the head arrives or the transport fails. Fields are `&'static str` or integers only: `operation`, `method`, `route` (new `EndpointMeta::route`, the authored template, e.g. `2010-04-01/Accounts/{account_sid}/Messages/{sid}.json`), `status` or a fixed transport class, `elapsed_ms`. No URL, header, body or value can be written into an event. Variants: head-only (codex) | + one event when the body is consumed or fails (claude) | + a per-call span carrying the values `Debug` shows (J4).
Declaration: unchanged (Twilio `examples/twilio-sdk/src/lib.rs:23-30,42-44`). The application's `Cargo.toml` writes `concord = { …, features = ["json", "tracing"] }`.
Call site: the app installs a subscriber (`tracing_subscriber::fmt().with_env_filter("concord=info").init()`); `stripe-app/src/reconciliation.rs:405-413` passing `operation: "GET /v1/invoices"` by hand → `INFO concord: operation="Stripe.invoices.list" method="GET" route="v1/invoices" status=200 elapsed_ms=84`.
Glue removed: 0 SDK lines (checked: no SDK logs). Apps ≈ 60 lines (lane estimate; `github-app/src/requests.rs:11-33` restates operation names and status by hand, checked; its request-id half needs J2). Its value is the owner's criterion — "Concord logs itself" — for all 55 SDKs with no SDK edit.
Counterexample: zero cost when off (cfg removes the dependency, clock reads and fields); on but unsubscribed, the callsite interest check runs before the clock is read. Path holes and host labels are never logged (the route keeps `{name}`). S3 signs natively after `into_parts` yet still executes through `Decoder::execute` (`amazon-s3-sdk/src/client.rs:351-358`, checked), and no URL is logged. `EndpointMeta` has no route today (`crates/concord-core/src/__private.rs:19-44`, checked): the macro adds one `&'static str`.
Simple control: unchanged (declaration, constructor, tests).
New concepts: feature `tracing`; event target `concord` with a fixed field set.  Breaking: none (`EndpointMeta` is private).
Shares mechanism with: H (the route is the "method and URL template" doc line); M4 (the same composition text); E (operation identity).
Screen:
- C1: no declaration word (a Cargo feature). C2: whether and where to log is the app's subscriber. C3: unchanged.
- C7: nothing removed. C8: observation only, off by default, request bytes unchanged; the dispatcher is the app's choice, not Concord state.
- C9: a capturing subscriber over the canary/oracle corpus and the seven scenarios — no canary in any event; positive control: status present.
- Risk: template + status may be too sparse to diagnose ("which repository?"); adding fields casually turns it into J4.

## J2. Declared correlation header: `request_id "x-github-request-id";` — KEEP (rank 2)
Sources: DJM-claude.md § J4.
Fact: the provider identifies each response by this header, and support asks for it (GitHub `x-github-request-id`, S3 `x-amz-request-id` + `x-amz-id-2`, Stripe and Anthropic `request-id`, GitLab `x-request-id`).
Mechanism: a client or scope statement `request_id NAME, …;` (literal or `HeaderName` constant, checked like header keys). Core reads it through the operation: `ResponseMeta::request_id()`, `Received::request_id()`, `Error::request_id()`; `Error`'s `Display` appends `(request id …)`; J1 carries it as a field. It is the only response header value Concord ever prints.
Declaration: GitHub `examples/github-sdk/src/api.rs:50-68` →
```rust
pub client GitHub(#[sensitive] token: String, …) {
    base "https://api.github.com";
    request_id "x-github-request-id";        // new line
    headers { AUTHORIZATION = "Bearer {token}"; … }
```
Call site: `page.meta.response.request_id()` (SDK type) → `meta.request_id()` (Concord's); error messages carry it with no SDK code.
Glue removed: corrected. Lane: 131 non-doc lines in 20 SDKs, ≈ 100 removable. Checked: 157 non-comment `request.?id` lines in 18 SDKs, but PayPal (34), QuickBooks (21) and Snowflake (17) are request-side ids (`PayPal-Request-Id`, the `requestid` query, a statement request id), not response correlation. That leaves ≈ 85 lines in ≈ 15 SDKs (github `meta.rs:32,66,93,115-116,168`, anthropic `error.rs:104-105`, checked). Ids carried in bodies (S3's XML `RequestId`, Anthropic's error-document fallback) stay, so ≈ 50–70 removable (inferred).
Counterexample: S3's request id inside the XML error document is a body fact and stays SDK code (family A). A response header that carries a secret cannot print by accident: only declared names print.
Simple control: Nager declares none; unchanged.
New concepts: statement `request_id`.  Breaking: `Error`'s `Display` gains a suffix in adopting SDKs (tests comparing exact strings).
Shares mechanism with: B (declared response metadata); E/O (rate-limit and `retry-after` header names) — one "declared response headers" design should own this slot rather than a single-purpose statement; A (error `Display` and accessors).
Screen:
- C1: a provider fact. C2: what to do with it is the app's. C3: unchanged. C4: one statement.
- C5: an invalid header name is reported at the statement (and by `validate_header_names()`).
- C9: canary in an undeclared response header never printed; the declared one present.

## J3. The redacted rendering as a value, kept through `into_parts` — KEEP (rank 3)
Sources: DJM-claude.md § J2 (`Summary`), DJM-codex.md § J2 (`RequestDiagnostic`, conservative native view, `#[sensitive] query(..)`).
Fact: nothing new for declared placements; one widened fact — a `query(..)` contribution can be secret (ClickHouse's bound query parameters).
Mechanism: `prepared.summary()`, `received.summary()`, `error.summary()` and, after `into_parts`, a view built from the `Decoder` return one safe `Display` line (`GET https://api.github.com/repos/rust-lang/rust/issues?state=open -> 200`); `Prepared`'s `Debug` delegates to it, so the guarantee has one implementation. A natively edited request prints `https://host/<withheld>` (claude: the decoder keeps a URL fingerprint) or withholds the whole URL and all header values (codex: `native_request`). Variants for dynamic pairs: always withhold `query(..)` values (claude) | accept `#[sensitive] query(..)` and withhold that endpoint's whole query string (codex; forbidden today by AUTHOR_GUIDE §4.1a, checked).
Declaration: unchanged (claude). Codex variant, ClickHouse `examples/clickhouse-sdk/src/wire.rs:40-49` (checked) →
```rust
query(..) params?: Vec<(String, String)>,                // before
#[sensitive] query(..) params?: Vec<(String, String)>,   // after
```
Call site: `format!("{prepared:?}")` → `tracing::info!(call = %prepared.summary())`; S3's `SignedRequest` `Debug` delegates to the native view.
Glue removed: ≈ 6 lines (S3 `client.rs:362-378`, 17 lines → ≈ 11, checked); SDK error `Display`s that restate "operation: status" could delegate (counted under A). The value is closing two measured gaps: `Prepared`'s `Debug` prints dynamic pair values (`clickhouse-sdk/src/wire.rs:83-91` asserts `param_x=fixture-value` is shown, checked; FRICTION D2: clickhouse, hubspot-crm, mediawiki), and redaction stops at `into_parts`.
Counterexample: S3 native edit → path and query withheld (over-redaction, safe). Host labels are redacted as in `Prepared`'s `Debug`. Zero cost: nothing runs unless called. ClickHouse's test asserting the value is printed must flip (a corrected expectation, not a deleted test).
Simple control: unchanged.
New concepts: `concord::Summary` (one type) and `summary()` on four types; `#[sensitive]` accepted on `query(..)` (codex variant).  Breaking: `Prepared`'s `Debug` gets stronger for dynamic pairs; `Decoder` grows 8 bytes (claude variant).
Shares mechanism with: D (FRICTION D2: values outside `#[sensitive]`); K (query composition); A (error `Display` delegation).
Screen:
- C2: renders facts; the app decides to log. C4: sensitive dynamic queries and unproven native URLs are withheld whole.
- C7: redaction survives `into_parts`, which it does not today.
- C9: oracle check that the summary URL equals `Prepared`'s redacted URL, plus native-edit, duplicate-key and `query(..)` canaries.
- Risk: removes almost no lines and does not by itself make "Concord logs itself" true (J1 does); non-sensitive path values (an e-mail in a path) still print.

## J4. One span per call carrying the values `Debug` shows — MERGED into J1
Sources: DJM-claude.md § J3.
J1's feature plus a `concord.call` span entered by the generated `send()`, with one field per value that `{endpoint:?}` shows (sensitive values, `query(..)` pairs and flagged headers omitted); J1's events become its children, so a policy's retries are attempts of one call (apps ≈ 100 lines vs J1's ≈ 60, lane estimate). Shown as J1's richer variant: it answers "which repository?", but turns a local `Debug` into data shipped to log aggregators, so PII an author left unmarked leaks at scale, and the macro emits per-endpoint field lists (code size when on).

## J5. Explicit `Observer` at the Rust execution boundary — KILL (only moves glue)
Sources: DJM-codex.md § J3.
`execute_observed`/`receive_observed` hand an `ExecutionEvent` to an `Observer` trait: 0 lines removed, and observing every generated send requires each SDK to add the same `with_observer` method, observer field and observed calls to its policy (the lane says so). Three new concepts for what J1 gives with none, and Concord still does not log itself.
