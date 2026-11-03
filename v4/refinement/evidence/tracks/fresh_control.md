# Fresh-use report: concord (control docs)

Model: Opus 5.5 (claude-opus-5-5). Requested "Claude Opus at medium effort": effort setting is NOT observable from inside the agent (no tool or prompt field exposes it).

## Running log (predictions written before each run)

### T1 prediction (written before running)
Smallest declaration: client with `#[sensitive] header("x-api-key") key: String` and `region: String`,
`base "https://catalog.example.test/v1"; subdomain "{region}";`, and `GET health() -> Text { path "health"; }`.
`Catalog::new(http, key, region)`; `.health().send().await` returns `String` "ok".
Fixture reach: subdomains need a DNS-name base (SEMANTICS §4: "labels require a domain-name base, not an IP address"),
so `with_base_url(server.url())` (127.0.0.1) would fail at build time. Use `reqwest::Client::builder().resolve("eu.catalog.example.test", addr)`
and `with_base_url("http://catalog.example.test:PORT/v1")`. Prediction: with_base_url returns Result (doc says it returns an error).

### T1 actual
Passed first compile and first run. `with_base_url(&String)` accepted, returns Result. Host header `eu.catalog.example.test:PORT`, target `/v1/health`, `x-api-key` sent.

### T2 prediction (written before running)
Declaration on a `tenant(tenant: String)` scope: `GET search(query category?: String, query page_size: u32 = 20, query("tag") each tags: Vec<String> = Vec::new(), query cursor?: String)`.
Query order = parameter order. Expected:
(a) `page_size=20`; (b) `category=lighting&page_size=50`; (c) `category=&page_size=20`;
(d) tags [] -> `page_size=20` (no `tag`); ["red","new"] -> `page_size=20&tag=red&tag=new`.
Unsure whether `each` accepts `Vec<String>` initializer `Vec::new()` (should, initializer is any expr).

### T2 actual
All five query strings exactly as predicted; first compile, first run. Navigation (`t.search()`) takes `&self` (no clone needed to reuse the tenant handle).

### T3 prediction (written before running)
`tenant("acme/eu")` builds an `Arc` snapshot holding a clone of the client (an Arc bump) plus the tenant String.
`t.product(..)` / `t.search()` borrow `&self` and the request captures a clone of the tenant handle = Arc refcount bump;
the tenant String itself is NOT cloned per request. Path: `/v1/tenants/acme%2Feu/products/p-1`.
To observe "what is copied" I'll use a clone-counting newtype for the tenant in a test-local declaration.
Prediction for counter: 0 clones of the tenant value across 1 product + 1 search + prepare/send.

### T3 actual
Path `/v1/tenants/acme%2Feu/products/p-1` as predicted. Clone counter: entry=0, endpoint creation=0, request clone=0, send=0, scope-handle clone=0.
So nothing of the tenant value is ever copied; handles and requests share it (consistent with "immutable Arc snapshots").
Note: the docs do not say explicitly that endpoint requests share the parent by Arc (only "plain structs, Clone"); I had to measure it.

### T4 prediction (written before running)
Declaration: client `header("accept-language") lang?: String = "en"`; search gets `lang?: String` with
`headers { override "accept-language" = lang; }` (absent -> keeps inherited `en`); product gets `lang?: Option<String>` with
`headers { override "accept-language" = each lang; }` so `set_lang(None)` (present None) clears the header, absent keeps `en`.
Requests: (a) search `.set_lang("de")` -> exactly one `accept-language: de`; (b) product `.set_lang(None)` -> no accept-language;
(c) plain product/search -> `accept-language: en`. Also alternative for (b): `api.clone().maybe_lang(None)` at client level -> none, original `api` still `en`.
Risk: endpoint value named `lang` shadowing client `lang` might be rejected; doc says an endpoint may share a name with an enclosing scope.

### T4 actual
Exactly as predicted: `[["de"], [], ["en"], ["en"], [], ["fr"]]`. First compile. The tri-state `lang?: Option<String>` + `override ... = each lang`
is expressive but needed careful reading of SEMANTICS §3 and §5 (the README/rustdoc alone do not mention the `each` on Option trick).

### T5 prediction (written before running)
`key` is a required value (`x: T`); per the values table required values are only positional, so no `set_key` is generated.
Prediction: `api.set_key("new")` fails to compile (no method). The only way to "change" the key is a new `Catalog::new(..)`;
a tenant handle created from the old client keeps the old key (snapshot). If I make the key initialized instead
(`key: String = String::new()`), `set_key` would exist and return a NEW client; the old tenant still sends the old key.

### T5 actual
First attempt `api.set_key("new-key")` -> `error[E0599]: no method named set_key found for struct Catalog` (as predicted; the library does not let you change a required value).
Second attempt with a new `Catalog::new`: old tenant handle and a request created from it both send `old-key`; new client sends `new-key`.
Also verified copy-on-write with the settable `lang`: `api.clone().set_lang("de")` affects only new handles. 1 edit/compile cycle (deliberate).

### T6 prediction (written before running)
Initializer `= 20` runs once per endpoint call (`t.search()`), not on clone, setter, prepare or send (SEMANTICS §3).
Test with a counting initializer function in a test-local declaration: after `t.search()` counter = 1; after `.clone()` still 1;
after preparing both copies still 1; both prepared URLs carry the same value. A second `t.search()` -> 2.
Also: a client-level initializer (lang = "en") runs on `Catalog::new` only, not on each scope entry.

### T6 actual
Exactly as predicted: client initializer ran once on `new` (0 extra on two scope entries); endpoint initializer ran once on `t.search()`,
0 times on clone, 0 times on two prepares; both prepared URLs `page_size=20`; a second endpoint call ran it again (`page_size=21`).
Note: an initializer function named `lang()` next to a value named `lang` worked (initializer is compiled in a nested fn item).

### T7 prediction (written before running)
`t.update("p-1", ProductUpdate{..}).send().await` -> `Err(e)` with `e.kind() == ErrorKind::Status`, `e.status() == Some(409)`,
`e.endpoint() == Some("Catalog.tenant.update")`; `e.into_response()` -> `Some(reqwest::Response)` unread; `response.bytes().await` then
`serde_json::from_slice::<Problem>` (I avoid `response.json()` because I don't know whether reqwest's `json` feature is on).
Body on the wire: `{"name":"Lamp","category":null}` with `content-type: application/json`. Display: `PUT Catalog.tenant.update: 409 Conflict`-ish.

### T7 actual
As predicted. Display `PUT Catalog.tenant.update: HTTP status 409 Conflict`; Debug
`Error { kind: Status, endpoint: Some("Catalog.tenant.update"), status: 409, message: ... }`. Status read before `into_response()` (which consumes the error);
the response is still unread, Problem decoded from its bytes. Wire body `{"name":"Lamp","category":null}`, content-type application/json. First compile.

### T8 prediction (written before running)
`t.product("p-1").prepare()?.into_parts()` -> `(client, request, decoder)`; `*request.timeout_mut() = Some(2s)`;
`request.headers_mut().insert("x-trace", HeaderValue::from_static("t1"))`; `client.execute(request).await?`; `decoder.decode(resp).await?` -> `Product`.
Wire: `x-trace: t1`, `x-api-key`, `accept-language: en`. The x-api-key HeaderValue on the native request is already `is_sensitive()`.
Alternative: `reqwest::RequestBuilder::from_parts(client, request).timeout(..).header(..).send()` — then decoder.decode works the same.

### T8 actual
As predicted, first compile. Both the `timeout_mut`/`headers_mut` route and `RequestBuilder::from_parts(..).timeout(..).header(..)` work;
`Decoder::decode` returned a typed `Product`. `x-api-key` HeaderValue on the native request is flagged sensitive.
Not verified: that the 2 s timeout actually fires (the fixture cannot delay); it is plain reqwest behaviour.

### T9 prediction (written before running)
Key `SEKRET-123`. Client `{:?}`: shows region/lang, key withheld (some placeholder). Request `{:?}`: endpoint values, no key.
Prepared `{:?}`: method, URL, headers with `x-api-key: <redacted>`. Errors: 500 status error, invalid-JSON decode error,
build error (tenant ".." -> dot segment), transport error (closed port): none contain the key. Key is sent on the wire.
`e.transport()` Debug is outside the guarantee but the key is a header, not in the URL, so I expect it absent there too.

### T9 actual
Key never appears in any Concord output; sent on all 3 wire requests. Output (port elided):
- client: `Catalog { base: "http://catalog.example.test:PORT/v1", key: <redacted>, region: "eu", lang: Some("en") }`
- tenant: `CatalogTenant { tenant: "acme" }` (parent not shown)
- request: `CatalogTenantSearch { category: Some("lighting"), page_size: "20", tags: [], cursor: None, lang: None }` (surprise: the u32 shows as the quoted string `"20"`)
- prepared: `Prepared { endpoint: "Catalog.tenant.product", method: "GET", url: "http://eu.catalog.example.test:PORT/v1/tenants/acme/products/p-1", headers: [x-api-key: <redacted>, accept-language: en], body: None }`
- status: `GET Catalog.tenant.product: HTTP status 500 Internal Server Error` (server body containing the key is not read/printed)
- decode: `... response not decoded: JSON decode error (syntax) at line 1, column 2`; `body_source()` = serde_json error (unredacted by design)
- build (id ".."): ``... request not built: path segment `{id}` is empty, `.` or `..` ``
- transport (port 1): `... transport error (connect)`; `transport()` Debug shows `127.0.0.1:1` but no key.
Also `DELETE` -> 204 -> `()` OK.

### T10 prediction (written before running)
(a) Endpoint template `{tenant}` inside scope `tenant` where only the scope declares `tenant`: compile error "no value `tenant` on this endpoint", suggesting `{tenant.tenant}`, pointing at the path string literal.
(b) `GET search(query category?: String, query category: String)`: compile error "duplicate value `category`" pointing at the 2nd.
(c) `api.tenant(42)`: E0277 `the trait IntoValue<String> is not implemented for {integer}` pointing at `42`, plus a note listing implementors.

### T10 actual (all compile errors, as predicted; files parked in `app/mistakes/*.rs.txt`)
(a) `error: \`tenant\` is not a value of \`A.tenant.product\`; to use the value of an enclosing owner, write \`tenant.tenant\`` — points at the whole path string literal (not at the hole). Cause identified correctly; the suggestion `tenant.tenant` reads oddly because I named the scope and its value the same.
(b) `error: duplicate value \`category\` in \`B.search\`` at the 2nd name + `error: first declared here` at the 1st. Excellent.
(c) `E0277: \`{integer}\` is not accepted for a value of type \`String\`` at `42`, `help: the trait IntoValue<String> is not implemented for {integer}`, a custom note ("Concord accepts the exact declared type, or &str/&String for String ..."), then implementor list pointing into library source paths and a macro-backtrace note. Points at the cause; trailing noise is rustc's.
Extra (t10d): direct `header("accept-language")` on an endpoint when the client already places it -> ``header `accept-language` is already set by `D`; use `override`, `append` or `remove` ...``; unused value -> ``value `unused` is never used; place it ...``. Both precise and actionable.

## Final code
- Declaration: `/home/canarit/projects/concord_v4_refine_work/fresh/control/app/src/lib.rs`
- Shared test helpers: `app/tests/common/mod.rs` (reqwest client with `.resolve("eu.catalog.example.test", server.addr)`, `with_base_url("http://catalog.example.test:PORT/v1")`)
- Tests: `app/tests/t1.rs` ... `app/tests/t9.rs` (T3/T6 contain test-local `api!` declarations with counting Clone / counting initializers)
- Deliberate compile failures: `app/mistakes/t10a.rs.txt` ... `t10d.rs.txt`

Final declaration:
```rust
concord::api! {
    pub client Catalog(
        #[sensitive] header("x-api-key") key: String,
        region: String,
        header("accept-language") lang?: String = "en",
    ) {
        base "https://catalog.example.test/v1";
        subdomain "{region}";
        GET health() -> Text { path "health"; }
        scope tenant(tenant: String) {
            path "tenants/{tenant}";
            /// `set_lang(Some(l))` sends `l`, `set_lang(None)` sends no `accept-language`.
            GET product(id: String, lang?: Option<String>) -> Json<Product> {
                path "products/{id}";
                headers { override "accept-language" = each lang; }
            }
            GET search(
                query category?: String,
                query page_size: u32 = 20,
                query("tag") each tags: Vec<String> = Vec::new(),
                query cursor?: String,
                lang?: String,
            ) -> Json<Page> {
                path "products";
                headers { override "accept-language" = lang; }
            }
            PUT update(id: String, body update: Json<ProductUpdate>) -> Json<Product> { path "products/{id}"; }
            DELETE delete(id: String) { path "products/{id}"; }
        }
    }
}
```
Fixture choice: `.resolve()` + `with_base_url` on the DNS name, because subdomain labels "require a domain-name base, not an IP address" (SEMANTICS §4), so `with_base_url(server.url())` (127.0.0.1) cannot work with `subdomain "{region}"`; this also lets the tests assert the real `Host: eu.catalog.example.test:PORT`.

## Commands
All with `CARGO_TARGET_DIR=.../app/target CARGO_BUILD_JOBS=4`, no `cargo update`, no new dependencies:
`cargo test --test tN [-- --nocapture]` per task; `cargo test` (all green: 10 tests in 9 files); `cargo clippy --all-targets` (only `needless_borrows_for_generic_args` in my tests).
Edit/compile cycles: T1–T4, T6–T9 compiled and passed first time; T5 one deliberate failing compile (`set_key`); T10 four deliberate failures;
one layout fix (`tests/common.rs` was compiled as its own test target -> moved to `tests/common/mod.rs`; my mistake, not the library's).
`cargo doc` on my crate: never needed/used. Macro expansion: never used. Native reqwest used only in T8 (by design) and T7 (`Response::bytes()` on the error's response).

## Friction
- Doc lookups: README + rustdoc crate page were enough for T1–T3, T7–T9. T4 (per-call header omission) and T6 (initializer timing) needed SEMANTICS §3/§5; the `x?: Option<T>` + `override "k" = each x` tri-state is only described in SEMANTICS.
- Guesses that held without docs: `with_base_url` signature/return type (Result) and that navigation takes `&self` — the generated API has no rustdoc in the supplied docs.
- T3 "what is shared" could only be determined by measuring with a clone-counting newtype; docs say scopes are Arc snapshots but say nothing about how endpoint request structs hold their parent.
- README's links (`v4/SEMANTICS.md`, `docs/MIGRATION.md`, `examples/shapes`) don't resolve in the supplied doc set; README example uses an undefined `Region` type.
- Request Debug printed `page_size: "20"` (a u32 shown as a quoted string) — looked like a wrong type until I realised Debug uses Display.
- No workaround needed anywhere; no reqwest use beyond the intended escape hatches.

## Three most confusing things
1. **Required values can never be changed (T5).** No `set_key`; to rotate the key you rebuild the client, and there is no accessor to get the `reqwest::Client` back. *Coherent rule, poorly explained*: the Values table implies it (required = positional only) but never says "cannot be changed afterwards; declare `x: T = ...` if you need a setter". Smallest fix: one sentence under the Values table in the rustdoc + SEMANTICS.
2. **Per-call omission of an inherited header (T4b).** `remove` is static, so dropping `accept-language` for one call needs `lang?: Option<String>` + `override "accept-language" = each lang` and `set_lang(None)` (or a client snapshot with `maybe_lang(None)`). *Coherent rule, poorly explained* (the tri-state is in SEMANTICS §3/§5 only). Smallest fix: a 6-line "per-call override / per-call omission" example in the crate docs' Composition section.
3. **`with_base_url` "for a test server" vs subdomains.** Rustdoc suggests `with_base_url(..)` for a test server, but with `subdomain` the base must be a DNS name, so `server.url()` fails at build time; you need `ClientBuilder::resolve`. *Coherent rule, poorly explained / mildly surprising*. Smallest fix: one line next to `with_base_url` in the rustdoc: "with `subdomain`, keep a DNS base and use `reqwest::ClientBuilder::resolve` to reach a loopback server".
(Runner-up, *surprising*: generated Debug renders non-string values via Display in quotes, e.g. `page_size: "20"`. Fix: document it or use Debug for those fields.)

## Misleading names
- `override`: reads as "always replace", but an absent optional leaves the inherited value (documented, and useful in T4a, but surprising first read).
- `remove`: only removes Concord's own contributions; reqwest default headers still appear (documented in SEMANTICS §5 only).
- `maybe_x` vs `set_x(None)` on an `Option<T>` value: two different meanings of "None" (absent vs present-None); coherent but easy to mix up.
- Naming a scope and its value the same (`scope tenant(tenant: String)`) makes the diagnostic suggestion `tenant.tenant` look like a typo — my choice, but the docs' own examples (`scope org(org: String)`) encourage it.

## Prediction accuracy
All runtime predictions (T1–T9) matched exactly; compile-error predictions (T5 first attempt, T10 a–c) matched in substance. Only unpredicted observations: quoted `"20"` in request Debug, scope Debug omitting the parent, and the transport-error `transport()` Debug exposing `127.0.0.1:1` (outside the stated guarantee, no secret).
