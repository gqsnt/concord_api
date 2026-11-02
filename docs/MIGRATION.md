# Migration

Two independent paths:

- [From the V4 alpha to the refined V4](#from-the-v4-alpha-400-alpha1-at-8f7adc2-to-the-refined-v4)
  — a set of targeted syntax and API changes to the same design. Most declarations need
  mechanical edits only.
- [From V3 to V4](#from-concord-v3-to-v4) — a redesign; declarations are rewritten by hand.

Code blocks marked "does not compile" show the old form. Every other Rust block in this file is
compiled and run as a doctest of `examples/quickstart` (`cargo test --doc -p
concord-example-quickstart`).

## From the V4 alpha (4.0.0-alpha.1 at 8f7adc2) to the refined V4

The version number is unchanged (`4.0.0-alpha.1`); the refinement is the history after commit
`8f7adc2` (tag `concord-v4-alpha1-baseline`). The reasons for each change are recorded in
`v4/refinement/DECISIONS.md` (D01–D18). Breaking changes:

| Alpha | Refined | Fix |
|---|---|---|
| `Client::new(http, a, b)` | `Client::new(http, ClientConfig { a, b })` | name the required client values |
| `-> Json<T>`, `body b: Json<T>` | `-> T via Json`, `body b: T via Json` | name the payload type, then the representation |
| `-> Text`, `-> Binary`, `body b: Binary` | `-> String via Text`, `-> Bytes via Binary`, `body b: Bytes via Binary` | same |
| `-> ()` | no `->` | delete it; the output is `()` and the body is drained |
| `impl Encode for R { type Value = T; .. }` | `impl Encode<T> for R { .. }` | traits are generic over the payload; `Decode::DISCARD` is gone |
| `Prepared<Json<T>>`, `Decoder<Json<T>>` | `Prepared<T>`, `Decoder<T>` | name the output type |
| `override "k" = optional;` | `override "k" = optional else inherit;` | required when the source can be absent |
| `"k" = v; remove "k";` in one owner | compile error | `remove` acts first now; delete both statements |
| `error.into_response() -> Option<Response>` | `-> Result<Response, Error>` | `let response = error.into_response()?;` |
| no setter for required values | `set_x` for every value | rename an endpoint that is called `set_<value>` |

Additions that need no change: `#[cfg(..)]` and `#[deprecated(..)]` on scopes and endpoints,
`Decoder::finish`, `BodyError` implements `std::error::Error`, endpoint requests are `Clone`
exactly when their values and body are.

### Root construction: `{Client}Config`

Required client values were positional, so two values of one type could be swapped silently
(a secret ending up in a host label). They are now the fields of a generated struct, checked by
name.

V4 alpha (does not compile any more):

```rust,ignore
let api = Search::new(reqwest::Client::new(), "app-1", "secret");
```

Refined:

```rust
use concord::reqwest;

concord::api! {
    pub client Search(header("x-app-id") app_id: String, #[sensitive] header("x-api-key") key: String) {
        base "https://search.example.com";
        GET health() { path "health"; }
    }
}

fn main() {
    let config = SearchConfig { app_id: "app-1".into(), key: "secret".into() };
    let api = Search::new(reqwest::Client::new(), config);

    // Every value now has a setter, so rotating a key keeps `with_base_url` and the other values.
    let _rotated = api.set_key("new-secret");
}
```

Config fields take exactly the declared type, so string literals need `.into()`. A missing field
is one `E0063` error naming it. Scope and endpoint arguments stay positional. A client without
required values is still `Client::new(http)`.

### Representations: `T via R`

V4 alpha (does not compile any more):

```rust,ignore
GET item(id: u64) -> Json<Item> { path "items/{id}"; }
POST create(body item: Json<NewItem>) -> Json<Item> { path "items"; }
GET readme() -> Text { path "readme"; }
GET archive() -> Binary { path "archive"; }
PUT upload(body data: Binary) { path "upload"; }
DELETE remove(id: u64) -> () { path "items/{id}"; }
```

Refined:

```rust
use concord::bytes::Bytes;
use concord::{Binary, Json, Text, reqwest};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Serialize)]
pub struct NewItem {
    pub name: String,
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        GET item(id: u64) -> Item via Json { path "items/{id}"; }
        POST create(body item: NewItem via Json) -> Item via Json { path "items"; }
        GET readme() -> String via Text { path "readme"; }
        GET archive() -> Bytes via Binary { path "archive"; }
        PUT upload(body data: Bytes via Binary) { path "upload"; }
        DELETE remove(id: u64) { path "items/{id}"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let store = Store::new(reqwest::Client::new());
    let _: concord::Prepared<Item> = store.item(1).prepare()?;
    let _: concord::Prepared<String> = store.readme().prepare()?;
    let _: concord::Prepared<()> = store.remove(1).prepare()?;
    Ok(())
}
```

`Binary` also accepts `Vec<u8>` in both directions. Generated signatures now name the payload
directly (`send(self) -> Result<Item, Error>`) instead of `<Json<Item> as Decode>::Output`.

The compiler recognizes the old forms and names the edit: `-> Json<Item>` gets "the response
representation is written after the payload type: `Item via Json`" (likewise `String via Text`
and `Bytes via Binary`), and `-> ()` gets "an endpoint without typed output has no `->`: remove
`-> ()`" (`crates/concord/tests/ui/representation_alpha_spelling.stderr`).

### Custom `Encode` / `Decode`

The traits are generic over the payload type and implemented on a marker type; there are no
associated types and no `DISCARD`.

V4 alpha (does not compile any more):

```rust,ignore
pub struct Lines;

impl Encode for Lines {
    type Value = Vec<String>;
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");
    fn encode(value: Vec<String>) -> Result<reqwest::Body, BodyError> { .. }
}

impl Decode for Lines {
    type Output = Vec<String>;
    fn decode(body: Bytes) -> Result<Vec<String>, BodyError> { .. }
}

// declaration: POST normalize(body words: Lines) -> Lines;
```

Refined:

```rust
use concord::bytes::Bytes;
use concord::{BodyError, Decode, Encode, reqwest};

pub struct Lines;

impl Encode<Vec<String>> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: Vec<String>) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value.join("\n")))
    }
}

impl Decode<Vec<String>> for Lines {
    fn decode(body: Bytes) -> Result<Vec<String>, BodyError> {
        let text = String::from_utf8(body.into()).map_err(|_| BodyError::new("not UTF-8"))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

concord::api! {
    pub client Words {
        base "https://words.example.com";
        POST normalize(body words: Vec<String> via Lines) -> Vec<String> via Lines {
            path "normalize";
        }
    }
}
```

- A generic alpha representation such as `MyFormat<T>` with a `PhantomData` becomes a plain
  marker with a generic impl: `impl<T: Bound> Decode<T> for MyFormat`.
- A representation that set `DISCARD = true`: declare the endpoint without `->` (the body is
  drained without buffering), or decode and ignore the buffered bytes.

### `Prepared` and `Decoder` name the output type

V4 alpha (does not compile any more):

```rust,ignore
fn log(prepared: &concord::Prepared<Json<Item>>) { .. }
```

Refined:

```rust
pub struct Item;

fn log(prepared: &concord::Prepared<Item>) {
    println!("{prepared:?}"); // redacted rendering
}
```

### Optional overrides say what absence means

The behaviour is unchanged; the declaration must now spell it. An `override` whose source reads
an optional value ends with `else inherit` (absent keeps the inherited value). If you meant "my
value or nothing", write `remove` and set the key again.

V4 alpha (does not compile any more):

```rust,ignore
GET page(page_locale?: String) { query { override "locale" = page_locale; } }
```

Refined:

```rust
use concord::reqwest;

concord::api! {
    pub client Docs(query locale: String = "en") {
        base "https://docs.example.com";

        // Absent: the client's locale is sent (the alpha behaviour).
        GET page(page_locale?: String) {
            path "page";
            query { override "locale" = page_locale else inherit; }
        }

        // Absent: no locale at all.
        GET exact(query locale?: String) {
            path "exact";
            query { remove "locale"; }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let docs = Docs::new(reqwest::Client::new());
    assert_eq!(docs.page().prepare()?.request().url().query(), Some("locale=en"));
    assert_eq!(docs.exact().prepare()?.request().url().query(), None);
    Ok(())
}
```

`else inherit` on a source that is always present is an error too.

### `remove` acts first within an owner

`remove "k"` now applies to what the owner inherits, before the owner's own placements and
statements. Two consequences:

- `remove` plus a direct placement of the same key in one endpoint is exact replacement (the
  `exact` endpoint above); the alpha rejected it.
- `"k" = v; remove "k";` in the same owner — which in the alpha silently dropped the value it had
  just set — is now an error ("no enclosing owner sets it"). Delete both statements.

### `Error::into_response` returns `Result`

V4 alpha (does not compile any more):

```rust,ignore
if let Some(response) = error.into_response() {
    // decode the error document
}
```

Refined — a non-status error is returned unchanged instead of being dropped:

```rust
async fn problem_title(error: concord::Error) -> Result<String, Box<dyn std::error::Error>> {
    let response = error.into_response()?; // any other kind of error propagates
    let bytes = response.bytes().await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
```

### Header values: `ToHeaderValue`, not `Display`

A value placed directly as a header (`header("k") x: T`, `"k" = x`, `"k" = each xs`) is converted
with `concord::ToHeaderValue` instead of being formatted with `Display`. A native
`reqwest::header::HeaderValue` is now accepted and sent byte for byte with its sensitivity flag.
`String`, `&str`, the integers `u8`–`u64`/`usize`/`i8`–`i64`/`isize` and `bool` work as before,
with the same spelling. Path segments, labels, query values and templates are unchanged.

A header value of another `Display` type (a domain enum, `Uuid`, `IpAddr`, a float) is now a
compile error at its type: "`T` cannot be sent as a header value". Two explicit fixes:

- send its `Display` text with a template (same spelling and visible-ASCII check as before);
- implement `ToHeaderValue` for your own type (no formatting; `from_static` per variant).

Before (does not compile any more):

```rust,ignore
GET search(header("x-sort") sort: Sort);
```

After:

```rust
use std::fmt;

use concord::reqwest::{self, header::HeaderValue};
use concord::ToHeaderValue;

#[derive(Clone, Copy)]
pub enum Sort {
    Newest,
    PriceAsc,
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Newest => "newest",
            Sort::PriceAsc => "price_asc",
        })
    }
}

/// Or skip `Display`: the header spelling, without formatting.
impl ToHeaderValue for Sort {
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static(match self {
            Sort::Newest => "newest",
            Sort::PriceAsc => "price_asc",
        }))
    }
}

concord::api! {
    pub client Shop {
        base "https://shop.example.com";
        // A template formats with `Display`, as before:
        GET search(sort: Sort) { path "search"; headers { "x-sort" = "{sort}"; } }
        // `ToHeaderValue` places it directly:
        GET browse(header("x-sort") sort: Sort) { path "browse"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let shop = Shop::new(reqwest::Client::new());
    let a = shop.search(Sort::PriceAsc).prepare()?;
    let b = shop.browse(Sort::PriceAsc).prepare()?;
    assert_eq!(a.request().headers()["x-sort"], "price_asc");
    assert_eq!(b.request().headers()["x-sort"], "price_asc");
    Ok(())
}
```

The generated `Debug` of a value placed only as a header shows its header value rather than its
`Display` text (the same text for strings, integers and `bool`).

### Behaviour changes without a syntax change

- A value whose `Display` fails is a build error naming its placement, in every placement (the
  alpha could truncate a segment or panic).
- Literal header names are validated with the native validator at compile time (an over-long name
  panicked in `prepare`), and a request that would exceed the native header-map capacity is a
  build error instead of a panic.
- Only the order of values *within* one header name is part of the contract; the alpha also
  promised an order across names. Query order is unchanged.
- A bodyless endpoint with a move-only value is now accepted (its request is simply not `Clone`);
  an endpoint with a body is `Clone` when the body is.

## From Concord V3 to V4

V4 is a redesign, not a compatible update. It keeps V3's core modeling ideas — one declaration per
API tree, explicit `?` presence, local names with explicit ancestor qualification, compile-time
checked query/header composition, sensitive values — and removes the managed execution runtime.
There is no converter; declarations are short enough to rewrite by hand. V3 is preserved at tag
`concord-v3-preserved`.

### What was removed, and what to do instead

| V3 | V4 |
|---|---|
| `credentials { ... }`, `auth { ... }`, `SecretString`, token refresh | a `#[sensitive]` header or query value: `#[sensitive] header("authorization") token: String`. Refresh by calling `set_token(..)` on the client handle (requests created earlier keep the old value). |
| `rate_limits`, `rate_limit` | outside Concord (e.g. a semaphore around `send()`; see the [recipes](RECIPES.md)). |
| retry policies, `effect` | application code around `send()`; whether a request may be replayed is your decision ([recipes](RECIPES.md)). |
| `paginate`, `.pages()`, `.collect_all()` | call the endpoint again with the next cursor (`maybe_cursor(next)`). |
| `configure_http`, `configure_runtime`, `timeout` | configure the `reqwest::Client` you pass to `new`; per request: `prepare()?.into_parts()` + `timeout_mut()`. |
| built-in base URL only | `base "..."` plus `with_base_url(..)` at runtime. |
| `Client::builder()...build()?` | `Client::new(http, ClientConfig { .. })` — required values are fields of the generated config struct, checked by the compiler. |
| `.await` on a request | `.send().await` (explicit verb). |
| `response Json;` defaults, `via Json` | name the representation on each endpoint: `-> T via Json`, `body b: T via Json`. |
| `absent on 404` | `match` on `e.status()` ([caller guide](CALLER_GUIDE.md#6-errors)), or `send_raw()`. |
| `status [...]` | any 2xx is success; other statuses are `ErrorKind::Status` with the unread response. |
| `plan()` | `prepare()` gives a native `reqwest::Request`; `Prepared`'s `Debug` is redacted. |
| `replace_x`, `set_x_if_some`, `omit_x`, `reset_x` | `maybe_x(Option<T>)` (exact), `if let`, `maybe_x(None)`, the original handle. |
| `values { ... }` block | client parameters: `client Name(values) { ... }`. |
| `subdomain [region, "api"]` | `subdomain "{region}.api"` (a template). |
| `concat [...]`, `RequestAtom`, `via Adapter` | templates (`"Bearer {token}"`) and `Display` (use a newtype for a custom spelling). |
| `replace "k" = v`, `when_present "k" = v` | `override "k" = v else inherit` (absent keeps the inherited value); exact replacement is `remove "k"; "k" = v;`. |
| `query off`, `path replace/off`, `subdomain replace/off` | not available; restructure scopes. |
| `#[default] x: T` | `x: T = Default::default()`. Initializers may be any expression. |
| Authorization/Cookie/Accept/Content-Type forbidden as public headers | ordinary headers (a declared `Content-Type` wins over the body's). |

### Before and after

V3 (does not compile with V4):

```rust,ignore
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
use concord::{Json, reqwest};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Item {
    pub id: String,
}

#[derive(Deserialize)]
pub struct SearchPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct NewItem {
    pub name: String,
}

concord::api! {
    pub client ShopApi(#[sensitive] token: String) {
        base "https://api.example.com/v1";
        headers { "authorization" = "Bearer {token}"; }

        GET search(query q?: String, query limit: u32 = 20) -> SearchPage via Json { path "items/search"; }
        POST create(body item: NewItem via Json) -> Item via Json { path "items"; }
    }
}

async fn search(token: String) -> Result<SearchPage, concord::Error> {
    let api = ShopApi::new(reqwest::Client::new(), ShopApiConfig { token });
    api.search().set_q("shoe").send().await
}
```

### Running V3 and V4 side by side

Depend on V4 under another name; `api!` refers to its own crate through `$crate`:

```toml
concord4 = { package = "concord", path = "...", version = "4.0.0-alpha.1" }
```

Excerpt of [`examples/renamed/src/lib.rs`](../examples/renamed/src/lib.rs), which compiles and
tests exactly this:

```rust,ignore
concord4::api! {
    /// A client declared through the renamed crate.
    pub client Renamed(query("v") version: u32 = 4) {
        base "http://renamed.invalid";
        GET ping() -> String via concord4::Text { path "ping"; }
    }
}
```
