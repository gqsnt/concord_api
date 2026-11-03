# Fresh-use trial: `concord` (candidate docs), regional catalog client

**Model and effort.** I ran as Claude Opus 5.5 (`claude-opus-5-5`). My reasoning-effort setting is not observable from inside the session, so I can't confirm "medium". This was an agent-based simulation, not a human study.

**Inputs.** I read README, QUICKSTART, AUTHOR_GUIDE, CALLER_GUIDE, RECIPES and SEMANTICS in full. I listed the item names in the rustdoc index and grepped `examples/shapes` once, for `subdomain`. I didn't read DIAGNOSTICS before T10, and I didn't read any library source. I didn't expand the macro. I ran `cargo doc --no-deps` on my own crate once, after all ten tasks had a first attempt.

**Code.**
- Client: `/home/canarit/projects/concord_v4_refine_work/fresh/candidate/app/src/lib.rs`
- Tests: `/home/canarit/projects/concord_v4_refine_work/fresh/candidate/app/tests/catalog.rs`. The file has 9 tests and all of them pass.

No dev-dependencies were added and Cargo.lock is unchanged. I didn't need a direct `reqwest` dependency because `concord::reqwest` is re-exported.

**Command.** Every run used:
```
CARGO_TARGET_DIR=.../app/target CARGO_BUILD_JOBS=4 cargo test --test catalog -- --nocapture
```
Final result: `test result: ok. 9 passed; 0 failed`.

**How the tests reach the fixture.** The tests use `reqwest::ClientBuilder::resolve("eu.catalog.example.test", server.addr)` together with `with_base_url("http://catalog.example.test:{port}/v1")`. I chose this so the declared `subdomain "{region}"` is exercised on the wire: the `host` header is `eu.catalog.example.test:PORT`. Pointing the client at `server.url()` would not work with a subdomain declaration. The docs say this, and I confirmed it by accident in T9.

## Final declaration (abridged)

```rust
concord::api! {
    pub client Catalog(
        header("x-api-key") api_key: String,              // sensitive by name
        region: String,
        header("accept-language") lang?: String = "en",
    ) {
        base "https://catalog.example.test/v1";
        subdomain "{region}";
        GET health() -> String via Text { path "health"; }
        scope tenant(tenant: String) {
            path "tenants/{tenant}";
            GET search(query category?: String, query page_size: u32 = 20,
                       query("tag") each tags: Vec<String> = Vec::new(), query cursor?: String,
                       language?: Option<String>) -> Page via Json {
                path "products";
                headers { override "accept-language" = each language else inherit; }
            }
            GET product(id: String, language?: Option<String>) -> Product via Json {
                path "products/{id}";
                headers { override "accept-language" = each language else inherit; }
            }
            PUT update(id: String, body change: ProductUpdate via Json) -> Product via Json { path "products/{id}"; }
            DELETE delete(id: String) { path "products/{id}"; }
        }
    }
}
```

`lib.rs` also contains `update_product(&CatalogTenant, id, ProductUpdate) -> Result<Product, ApiFailure>`. It keeps the status together with the decoded `Problem`.

## Task by task

| Task | Prediction (written before running) | Actual | Edit/compile cycles |
|---|---|---|---|
| T1 | Minimal declaration: `client Catalog(header("x-api-key") api_key, region) { base; subdomain "{region}"; GET health() -> String via Text { path "health"; } }`. Wire: `GET /v1/health`, host `eu.catalog.example.test:PORT`, key sent, returns `"ok"`. | Exact match. reqwest also adds `accept: */*`. | 1 |
| T2 | (a) `page_size=20`; (b) `category=lighting&page_size=50`; (c) `category=&page_size=20`; (d) `[]` gives `page_size=20`, `[red,new]` gives `page_size=20&tag=red&tag=new`. | Exact match for all five. | 1 |
| T3 | Scope entry makes one Arc snapshot: an Arc clone of the client plus the moved tenant value. An endpoint call increments a refcount. The tenant value is never cloned, and it is formatted at `prepare`. A setter on a shared handle copies once. `org/7` becomes `org%2F7`. | Match, checked with a Clone- and Display-counting tenant type. Entry: 0 clones. Two endpoint calls: 0 clones. Two prepares: 2 formats. Handle clone: 0. `set_tenant` on a shared handle: 1. | 1, plus one fix for a `#[must_use]` warning on an unused `prepare()` |
| T4 | Add `language?: Option<String>` and `override ... = each language else inherit`. (a) `set_language(Some("de"))` sends `de`; (b) `set_language(None)` sends no header; (c) the next call sends `en`. Deriving a handle with `api.clone().set_lang("de")` / `.maybe_lang(None)` also works. | Match for both approaches. Headers observed: `[de] [] [en] [de] [] [en]`. | 1 |
| T5 | `set_api_key` exists. The old tenant handle sends the old key; a new handle sends the new key. | Match. | 1 |
| T6 | The initializer is evaluated once per endpoint call, even when a setter replaces it immediately. `clone` and both `prepare`s evaluate nothing. Both copies send `page_size=20`. | Match. Counter: call = 1, clone plus 2 prepares = 0, 2 more calls = 2. `set_page_size(5)` still consumed one evaluation. | 1 |
| T7 | `send()` returns a Status error with status 409 and Display `PUT Catalog.tenant.update: HTTP status 409 Conflict`. `into_response()` then `<Json as Decode<Problem>>::decode(bytes)` decodes the document. The body is `{"name":"Lamp","category":null}`. | Exact match. | 1 (plus a cosmetic rewrite of my own awkward `Ok(Err(..)?)`) |
| T8 | `prepare()?.into_parts()`, then `timeout_mut`, then `headers_mut().insert("x-trace", ..)`, then `decoder.finish(client.execute(req).await)`, giving a `Product`. A 200 ms timeout against a slow responder gives `Transport` with `is_timeout()`. | Match. Display: `GET Catalog.tenant.product: transport error (timeout)`, and `endpoint()` is preserved. | 1 |
| T9 | The key doesn't appear in Debug of the client, request or prepared request, nor in error Display or Debug. It is sent on the wire. Bonus guess: the native `Request` Debug shows `Sensitive`. | Match, including the bonus guess. The key is absent from 11 outputs: client, config, tenant, request, prepared, native request, status error Display and Debug, transport error Display and Debug, `transport()` Debug. It is present on the wire. | 2 (see wrong guess below) |
| T10 | (a) An unknown bare `tenant` suggests `tenant.tenant`. (b) A duplicate value is reported at the second name. (c) `tenant(42)` gives E0277 `IntoValue<String>`. | All three point at the cause. See the messages below. | One compile per case |

### Observed Debug output (T9)

```
Catalog { base: "http://catalog.example.test:44419/v1", api_key: <redacted>, region: "eu", lang: Some("en") }
CatalogTenant { tenant: "acme" }
CatalogTenantSearch { category: Some("lighting"), page_size: "20", tags: [], cursor: None, language: None }
Prepared { endpoint: "Catalog.tenant.search", method: "GET", url: "http://eu.catalog.example.test:44419/v1/tenants/acme/products?category=lighting&page_size=20", headers: [x-api-key: <redacted>, accept-language: en], body: None }
native: headers: {"x-api-key": Sensitive, "accept-language": "en"}
GET Catalog.tenant.search: HTTP status 500 Internal Server Error
GET Catalog.health: transport error (connect)
```

### Compiler messages (T10)

- **(a)** `error: \`tenant\` is not a value of \`M.tenant.product\`; to use the value of an enclosing owner, write \`tenant.tenant\``. The span is the bare `tenant`. It is precise and gives the fix.
- **(b)** `error: duplicate value \`page_size\` in \`M.search\`` plus `first declared here`. Both spans are correct.
- **(b2)** Same key in a scope and in an endpoint: `query key \`tag\` is already set by \`M.tenant\`; use \`override\` or \`append\` ..., or add \`remove "tag";\``. Precise, and it lists the options.
- **(c)** `error[E0277]: \`{integer}\` is not accepted for a value of type \`String\``. The span is `42`, with a note: "Concord accepts the exact declared type, or `&str`/`&String` for `String`". This is good. The noise is that rustc also prints library source excerpts (`value.rs`, the `api_impl!` line in `lib.rs`) and the whole `api!` invocation as "required by a bound".

## Friction

**Doc lookups.** Reading about 2,300 lines of guides up front was enough. Every task compiled on the first try. Generated rustdoc (`cargo doc`) was excellent: it states the initial value, "evaluated once when this request is created", and the `else inherit` meaning of each setter.

**Wrong guesses (1).** In T9 I built a dead-port client with `with_base_url("http://127.0.0.1:1/v1")`. It produced a `Build` error, "subdomain requires a base URL with a domain name, not an IP address", instead of a transport error. The docs do state this rule (QUICKSTART §6); I simply forgot it, so this was not a doc gap. The fix was `resolve()` to `127.0.0.1:1`.

**Workarounds and verbosity.**
- `set_language(Some("de".to_owned()))`: `IntoValue` doesn't reach inside `Option`, so `&str` doesn't work there.
- `set_tags` needs an exact `Vec<String>`. An array or a `&[&str]` doesn't work, even though `each` iterates any `IntoIterator`.
- The per-call language override must be written twice, once per endpoint. There is no way to share it at scope level so that each endpoint gets a `language` control.
- In the error-document path, wrapping `response.bytes()`'s reqwest error required me to call `without_url()` myself (the recipe mentions this).

**Direct use of `reqwest`.** Only the documented `into_parts` / `execute` / `Decoder::finish` path in T8, plus `ClientBuilder::resolve` and `HeaderValue`, all through `concord::reqwest`.

## Three most confusing things

1. **The doubled `Option` for "inherit / omit / replace"** (`language?: Option<String>`). It produces `set_language(None)` meaning send nothing, and `maybe_language(None)` meaning inherit, and `maybe_language(Some(None))` meaning omit.
   - *Classification:* coherent rule, but surprising to read at the call site. It is well explained in the guides.
   - *Smallest fix:* in generated rustdoc, list the three call spellings with their effects in a table on the setter. The current text "Sets language (present)" next to "`None`: send none" requires mental unpacking.
2. **Initializers are evaluated even when a setter replaces them at once** (T6). `api.search().set_page_size(5)` still runs `default_page_size()`.
   - *Classification:* coherent rule, clearly stated in the docs, but surprising for side-effecting initializers. The idempotency-key example shows it as a feature.
   - *Smallest fix:* one sentence in AUTHOR_GUIDE §2: "Don't put expensive or side-effecting work in an initializer you expect callers to override; it runs anyway."
3. **What a scope or request `Debug` shows.** `CatalogTenant { tenant: "acme" }` omits the parent (client, base, region), and request Debug omits the tenant. It isn't obvious what a log line identifies.
   - *Classification:* coherent but unexplained. The guides only say "values through Display".
   - *Smallest fix:* state in CALLER_GUIDE §8 that generated `Debug` shows only the owner's own values, and that `Prepared` Debug is the full redacted view.

**Honourable mention.** Header sensitivity by name (`x-api-key`) means `#[sensitive]` is optional for my key. This is convenient, but a user who renames the header to `x-catalog-key` silently loses redaction.
- *Classification:* surprising rule that is clearly documented.
- *Smallest fix:* recommend always writing `#[sensitive]` on secrets in the docs, or emit a lint note when a value named `*key*` / `*token*` feeds a non-listed header without the marker.

## Misleading names

- **`maybe_x` vs `set_x`.** `maybe_` reads as "set if Some", but `maybe_x(None)` actively clears the value, including an initialized default ("no reset"). Calling it `set_x_presence` or `with_x_option` would be more honest. The docs explain it, so the name is the only problem.
- **The `transport()` accessor and `ErrorKind::Transport`.** A per-request timeout I set myself is a "transport" error. That's reasonable, but `transport()` returning the raw reqwest error, which is not redacted, reads as safe to log. The docs warn about this.
- **`Prepared::request()`** shows the native request, whose Debug the docs call unredacted. In practice the key printed as `Sensitive` because of `set_sensitive`. The docs should say that sensitive headers do stay hidden in native Debug; only URL-borne values (query or path) leak.

## Smallest changes, by problem

| Problem | Type | Change |
|---|---|---|
| Doubled-Option call spellings | coherent, poorly surfaced at the call site | Generated rustdoc table: absent / `set_x(None)` / `set_x(Some(v))` |
| `set_language(Some("de".to_owned()))` verbosity | ergonomic gap | `impl IntoValue<Option<String>> for Option<&str>` and for `&str` (wrap). Or document the spelling in the setter's rustdoc. |
| `set_tags` needs exact `Vec<String>` | ergonomic gap | Generated rustdoc should say "one `tag=` per item" (it currently says only "Sent as: query parameter tag") and show the exact spelling |
| Repeated per-endpoint language override | missing feature or explanation | A RECIPES note: "a per-call override on many endpoints must be declared on each one" |
| Native Debug redaction wording | docs understate actual behaviour | CALLER_GUIDE §8: "sensitive header values print as `Sensitive` even in the native Debug; URL-borne secrets do not" |
| Library source shown in E0277 notes | diagnostic noise | Nothing in docs. Possibly `#[diagnostic::do_not_recommend]` on the `&str` impls to suppress the "other types implement" list. |
