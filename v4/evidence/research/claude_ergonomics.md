<!-- Verbatim final report of a Claude subagent (Opus 5.5), extracted from its transcript. -->

## Concord V4 ergonomics baselines: hand-written reqwest vs V3

All three hand-written baselines compile and pass their tests, and are clippy-clean (`cargo test --offline`). All three V3 renderings compile against the V3 reference crate at a492d36. I also ran V3 compile-fail probes and runtime probes against a loopback mock, so the friction items below come from observed behaviour, not just from reading the docs. No repository files were edited, and I did not read `v4/`.

### Paths
Root: `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/ergonomics/`
- Baselines (one workspace, reqwest =0.13.4 with only the `json` and `query` features):
  - `.../ergonomics/s1_simple/src/lib.rs` (test: `.../ergonomics/s1_simple/tests/wire.rs`)
  - `.../ergonomics/s2_scoped/src/lib.rs` (test: `.../ergonomics/s2_scoped/tests/wire.rs`)
  - `.../ergonomics/s3_query/src/lib.rs` (test: `.../ergonomics/s3_query/tests/wire.rs`)
  - Shared loopback mock: `.../ergonomics/testsupport/mock.rs`
- V3 renderings (compiled): `.../ergonomics/v3_render/src/s1.rs`, `.../ergonomics/v3_render/src/s2.rs`, `.../ergonomics/v3_render/src/s3.rs`. The plan printout test is `.../ergonomics/v3_render/tests/plans.rs`.
- V3 probes: compile-time cases in `.../ergonomics/v3_probe/cases/p01..p15_*.rs`; runtime cases in `.../ergonomics/v3_probe/tests/runtime.rs`.
- LOC counter: `.../ergonomics/loc.py`

**Note:** `cargo check` and `cargo test` on `v3_render` and `v3_probe` print 8 deprecation warnings from V3 `concord-core` (`fetch_update` → `try_update`) on this toolchain (rustc 1.99.0).

### Shapes
- **S1:** base `https://api.example.com/v1/`, `GET health` returning JSON, and `GET items/{id}`.
- **S2:** `https://{region}.api.example.com/v2/orgs/{org}/catalog/...` with:
  - a sensitive `X-Api-Key` header;
  - JSON `Accept`;
  - `X-Api-Version` defaulting to `2024-06-01`, overridable;
  - a region-level `status` and org-level `products` / `product/{id}`.
- **S3:**
  - `GET items/search` with `q`, a repeated `tag`, `category`, `min_price`, `max_price`, `in_stock`, `sort` (enum), `cursor`, and `limit` defaulting to 20;
  - `POST items` with a JSON body, returning 201;
  - `DELETE items/{id}`, returning 204;
  - a bearer token on every request.

Each baseline has a typed client and accepts any `reqwest::Client` plus a base `Url`. S2 adds borrowed scope handles; S3 adds a search builder. S2 and S3 each have a native escape hatch: `CatalogApi::request(..)` in S2 and `SearchItems::into_request()` in S3, both returning a `reqwest::RequestBuilder`. Tests check the wire: path encoding (`a/b` → `a%2Fb`), the exact query `q=red+shoe&tag=a&tag=b&sort=price_asc&limit=50`, the default `limit=20`, the POST body, DELETE accepting 204, and that the key `HeaderValue` is marked sensitive and absent from `Debug`.

### LOC (FACT)
Effective LOC means non-blank, non-comment lines, counted after running rustfmt on copies of the files. The rustfmt does not reformat inside `api!`. Raw-source counts are in brackets.

| Shape | Hand-written, whole file | V3 `api!` declaration | V3 file (declaration + DTOs/atoms, excluding call site) |
|---|---|---|---|
| S1 | 49 [46] | 8 | 19 [14] |
| S2 | 130 [104] | 29 | 55 [42] |
| S3 | 182 [108] | 33 | 69 [51] |

How to read this table:
- **Shared code.** The DTOs and enums are the same on both sides: 11, 28 and 37 raw lines in the hand-written files.
- **Extra V3 support code.** V3 needs a `RequestAtom` impl for each enum used in a URL. In the hand-written files that work is a plain `fn label()`.
- **Hand-written extras.** The hand-written files include features V3 lacks: a runtime base URL, `reqwest::Client` injection and native request escape hatches.
- **Where S3's lines go.** About half of S3's 182 lines are the builder: 9 setters plus a 9-way `if let Some` that pushes query pairs. That is exactly the boilerplate Concord should delete.
- **Compile time.** An incremental `cargo check` after touching the file took about 0.06–0.15 s for both sides (n=3, i5-13600K, rustc 1.99.0). This is not a benchmark and only shows that neither side is slow.

### V3 declarations and call sites (compiled)
S2 declaration, abridged:
```rust
values { header("X-Api-Version") api_version: String = "2024-06-01"; }
credentials { secret api_key; credential key = api_key(secret.api_key); }
auth { header "X-Api-Key" = key; }
scope regional(region: Region) { subdomain [region];
  GET status() -> Status { path "status"; }
  scope org(org: String) { path "orgs/{org}/catalog";
    GET product(id: String, version?: String) -> Product {
      path "products/{id}"; headers { when_present "X-Api-Version" = version; } } ... } }
```
Call sites:
```rust
// S1
let api = SimpleApi::new()?; let item: Item = api.item("42").await?;
// S2
let api = CatalogApi::builder().api_key(SecretString::try_new(key)?).build()?;
let p: Product = api.regional(Region::Eu).org("acme").product("p-1").set_version("2025-01-01").await?;
// S3
let api = ShopApi::builder().token(SecretString::try_new(token)?).build()?;
api.search().set_q("red shoe").set_tags(vec!["a".to_owned(), "b".to_owned()])
   .set_sort(Sort::PriceAsc).set_limit(50).await?;
api.create_item(NewItem{..}).await?; api.delete_item(created.id).await?;
```
Hand-written S3 for contrast: `api.search().q("red shoe").tag("a").tag("b").sort(Sort::PriceAsc).limit(50).send().await?`.

Grammar sources: spec.md §2 (lines 35–165), §6.1–6.3, §8, §10, §11. The SafeProxy pattern comes from quick_start.md §3, and `when_present` from spec §8.2.

V3 concepts needed to make one call (OPINION-weighted count; the listed items are FACT):
- **S1 — 4:** client type, `new() -> Result<_, ClientBuildError>`, named request + `IntoFuture`, `ApiError`.
- **S2 — 8:** S1's four plus builder, `SecretString::try_new`, owned scope handles, optional controls (`set_version`).
- **S3 — about 10:** S2's eight plus the 5-verb control matrix (`set`/`replace`/`set_x_if_some`/`omit`/`reset`) and the closed `IntoValue` conversions (exact `Vec<String>`, positional exact body).
- **Hand-written:** about 3–5 (struct, method or builder, `send()`, `reqwest::Error`).

### Friction list (FACT unless marked OPINION)
1. **Base URL is a compile-time literal.** spec.md §7.2 says "No request control or runtime operation can supply another destination". Pointing at staging or a mock needs `SafeProxy` and an `http` base (quick_start.md §3 loopback test). The hand-written version is `with_client(http, base)`.
2. **No way to bring your own `reqwest::Client` or get native request/response access.** runtime.md:17 says "Neither closure receives a raw native client builder", and `HttpConfigBuilder` restates reqwest options (runtime.md table: proxy, timeouts, pool, TLS). `.response()` returns `ApiResponse<O>` (responses.md:50), not `reqwest::Response`. This conflicts with the charter's "avoid restating reqwest configuration".
3. **`Accept` and `Authorization` are forbidden public headers.** See `RESERVED_PUBLIC_HEADERS` at `crates/concord-macros/src/syntax/diagnostics.rs:234-253` and spec §8.6. Probes p02 and p15 fail with `[C3-PLACE] 'Accept' is not a generic public-header target`. A vendor media type (`application/vnd.x.v2+json`) needs a custom representation, and a plain bearer token needs the credentials graph.
4. **A plain API key is pushed through credentials machinery.** It takes `secret` + `credential` + `auth` (3 constructs) plus `SecretString::try_new`. Over `http` it fails at call time with the opaque `endpoint GET bearer.item failed (Configuration)` (runtime probe, `InvalidAuthentication`). Meanwhile `#[sensitive] header("X-Api-Key") api_key: String` compiles and works (probe p03; wire shows `x-api-key: k-123`; client Debug hides it). So there are two ways to do one thing.
5. **Surface generated only for excluded subsystems:**
   - builder `retry()` (`emit/client.rs:512`) and `configure_runtime` (rate limiter, pagination limits);
   - `credentials()` (`emit/client.rs:92-102`);
   - `bind`/`plan_runtime` on every handle (`emit/owner.rs:500-535`);
   - `pages`/`collect_prefix`/`collect_all`/`store_as_x` (`emit/endpoint.rs:751-784`);
   - `cancel_with`/`with_options(CallOptions{sends, replays,…})` (`emit/endpoint.rs:817-825`).

   `ApiError`'s Debug output (observed) lists `policy_retries`, `quota_admissions`, `authentication_recoveries`, `freshness_restarts`, `rate_limit_error`, `pagination_error`, `traversal` and `stream`. Request Debug shows `caps: CallOptions { sends, replays, … }`.
6. **Reserved method names collide with real endpoint names.** `CLIENT_METHODS` = `builder, new, unbound, contract, credentials, request, plan, bind, plan_runtime` (`semantic/names.rs:277-298`). Probe p01: `GET request(id)` (for example a support-tickets API) is rejected with `[C3-GENERATED]`. Probe p12: an endpoint `set_limit` collides with a scope value's control. OPINION: move framework methods off the user namespace, e.g. into a trait or a single accessor.
7. **Five-verb control matrix.** generated_client.md "Value controls" (`emit/owner.rs:160-250`). Probe p10: `set_q(Option<String>)` and `set_limit(Some(5))` are rejected. The caller must choose between `replace_q` and `set_q_if_some`. OPINION: correct semantics, but too much vocabulary for S3. The hand-written builder takes `cursor(Option<String>)` directly.
8. **`-> ()` with an inherited `response Json` compiles but fails at runtime on 204.** The runtime probe returned `ResponseDecoding`, status 204. You must remember to write `via Empty` or `via Discard` (spec §10 example). This should be a compile error or should just work.
9. **A path value containing `/` is rejected at runtime, and the error doesn't say why.** The Display text is only `endpoint GET catalog_api.regional.org.product failed (Configuration)`, with no value or reason (spec §7.3). The hand-written client encodes it as `%2F`. Whether to encode or reject is a V4 decision; either way the error must be specific.
10. **Unbound intents duplicate the whole navigation tree.** `X::unbound()`, `*Endpoint` types and `client.request/plan` (generated_client.md "Unbound intents"; `emit/owner.rs:618-720`). None of the three shapes needs them.
11. **Overriding a shared header per call must be declared on every endpoint.** It takes `version?: String` plus `when_present`, and `when_present` needs a prior owner (probe p11: `[C3-OVERRIDE]`). That costs 2 lines per endpoint. Hand-written is one `with_api_version()` on the client.
12. **Plans are hard to inspect.** `RequestPlan` has no Display, its Debug hides the URL (`"<explicit accessor only>"`), and headers appear only as a count. The only accessors are `observed_url()`, `endpoint_id()` and `runtime_binding()` (`concord-core/src/endpoint.rs:845-900`). Hand-written: `.build()` gives a `reqwest::Request` with `url()` and `headers()`.
13. **Minor:**
    - closed `IntoValue` rejects `Vec<&str>` for `Vec<String>` (probe p07);
    - requests with a body are not `Clone` (probe p08);
    - a hidden 30 s default timeout;
    - the generated-code contract version guard and the `__private` bridge.

**V3 strengths worth keeping (FACT):**
- owned scope handles with no lifetimes (hand-written S2 needs `Regional<'a>`/`Org<'a>`);
- coded, precise compile errors (`C3-ATOM`/`C3-PLACE`/`C3-NAME`);
- the liveness check (probe p05);
- `#[must_use]` on requests (warns when `.await` is forgotten);
- `?` vs `Option<T>`;
- `each`;
- deterministic query order. The S3 plan URL matched the hand-written order exactly: `?q=red+shoe&tag=a&tag=b&sort=price_asc&limit=20`.

### Must-express list (minimal, per shape)
- **S1**
  - M1: base URL with a base path, with a literal default and overridable when the client is built (OPINION: needed for tests and staging; V3 lacks it).
  - M2: relative path templates with each segment encoded separately, and a defined rule for `/` inside a value.
  - M3: typed JSON decode without guessing from type names.
  - M4: unit or empty 2xx responses (204) without a decode trap.
  - M5: a user-supplied `reqwest::Client`.
- **S2**
  - M6: subdomain label from a scope value, prepended to the base host.
  - M7: path prefix from a scope value.
  - M8: nested scopes as owned, cheap handles.
  - M9: a sensitive header from a required client value (not credentials), redacted in Concord's Debug.
  - M10: shared static headers, including a vendor `Accept`.
  - M11: a header with a default that can be overridden on the client or scope and per call, without declaring it on each endpoint.
  - M12: enum/newtype atoms with one canonical spelling.
- **S3**
  - M13: optional query `x?: T`, omitted when absent and settable from an `Option<T>` in one call.
  - M14: a repeated key from a `Vec`, ideally also appendable one item at a time.
  - M15: an initialized default (`limit = 20`) that can be overridden or reset.
  - M16: declaration order equals wire order.
  - M17: JSON request body.
  - M18: non-default success statuses (201, 204).
  - M19: a bearer `Authorization` header as plain sensitive user data.
  - M20: a native escape hatch that returns the composed `reqwest::RequestBuilder`/`Request`, plus raw `reqwest::Response` access.

**Not needed by any shape:**
- credential sources;
- rate policies and pagination;
- unbound intents and contract descriptions;
- `CallOptions` caps and cancellation tokens;
- effect classes;
- runtime endpoints, accepted bytes and streams.

`absent on 404` is plausible but none of the three shapes needs it.