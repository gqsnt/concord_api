# Migrating from Concord V3 to V4

V4 is a redesign, not a compatible update. It keeps V3's core modeling ideas — one declaration per
API tree, explicit `?` presence, local names with explicit ancestor qualification, compile-time
checked query/header composition, sensitive values — and removes the managed execution runtime.
There is no converter; declarations are short enough to rewrite by hand.

## What was removed, and what to do instead

| V3 | V4 |
|---|---|
| `credentials { ... }`, `auth { ... }`, `SecretString`, token refresh | a `#[sensitive]` header or query value: `#[sensitive] header("authorization") token: String`. Refresh by building a new client value. |
| `rate_limits`, `rate_limit` | outside Concord (e.g. a semaphore or governor around `send()`). |
| retry policies, `effect` | a loop around `request.clone().send()` (endpoints without a body are `Clone`), or a reqwest middleware of your choice. |
| `paginate`, `.pages()`, `.collect_all()` | call the endpoint again with the next cursor (`maybe_cursor(next)`). |
| `configure_http`, `configure_runtime`, `timeout` | configure the `reqwest::Client` you pass to `new`; per request: `prepare()?.into_parts()` + `timeout_mut()`. |
| built-in base URL only | `base "..."` plus `with_base_url(..)` at runtime. |
| `Client::builder()...build()?` | `Client::new(http, required...)` — required values are positional and checked by the compiler. |
| `.await` on a request | `.send().await` (explicit verb). |
| `response Json;` defaults, `via Json` | name the representation on each endpoint: `-> Json<T>`, `body b: Json<T>`. |
| `absent on 404` | `match e.status()` on the error, or `send_raw()`. |
| `status [...]` | any 2xx is success; other statuses are `ErrorKind::Status` with the unread response. |
| `plan()` | `prepare()` gives a native `reqwest::Request`; its `Debug` is redacted. |
| `replace_x`, `set_x_if_some`, `omit_x`, `reset_x` | `maybe_x(Option<T>)` (exact), `if let`, `maybe_x(None)`, the original handle. |
| `values { ... }` block | client parameters: `client Name(values) { ... }`. |
| `subdomain [region, "api"]` | `subdomain "{region}.api"` (a template). |
| `concat [...]`, `RequestAtom`, `via Adapter` | templates (`"Bearer {token}"`) and `Display` (use a newtype for a custom spelling). |
| `replace "k" = v`, `when_present "k" = v` | `override "k" = v` (absent keeps the inherited value); exact replacement is `remove "k"; "k" = v;`. |
| `query off`, `path replace/off`, `subdomain replace/off` | not available; restructure scopes. |
| `#[default] x: T` | `x: T = Default::default()`. Initializers may be any expression. |
| Authorization/Cookie/Accept/Content-Type forbidden as public headers | ordinary headers (a declared `Content-Type` wins over the body's). |

## Before and after

V3:

```rust
api! {
    client ShopApi {
        base "https://api.example.com/v1/";
        response Json;
        credentials { secret token; credential bearer_token = bearer(secret.token); }
        auth { bearer bearer_token; }

        GET search(query q?: String, query limit: u32 = 20) -> SearchPage { path "items/search"; }
        POST create(body item: NewItem via Json) -> Item { path "items"; status 201; }
    }
}

let api = ShopApi::builder().token(SecretString::try_new(token)?).build()?;
let page = api.search().set_q("shoe").await?;
```

V4:

```rust
concord::api! {
    pub client ShopApi(#[sensitive] token: String) {
        base "https://api.example.com/v1";
        headers { "authorization" = "Bearer {token}"; }

        GET search(query q?: String, query limit: u32 = 20) -> Json<SearchPage> { path "items/search"; }
        POST create(body item: Json<NewItem>) -> Json<Item> { path "items"; }
    }
}

let api = ShopApi::new(reqwest::Client::new(), token);
let page = api.search().set_q("shoe").send().await?;
```

## Running V3 and V4 side by side

Depend on V4 under another name; `api!` refers to its own crate through `$crate`:

```toml
concord4 = { package = "concord", path = "...", version = "4.0.0-alpha.1" }
```

```rust
concord4::api! { pub client Api { base "https://api.example.com"; GET ping(); } }
```

(`examples/renamed` compiles exactly this.)
