# Writing a declaration

This guide is for the person who describes an HTTP API with `concord::api!`. It goes pattern by
pattern; the complete grammar and the precise rules are in [`v4/SEMANTICS.md`](../v4/SEMANTICS.md).

Every Rust block in this file is compiled and run as a doctest of `examples/quickstart`
(`cargo test --doc -p concord-example-quickstart`); the assertions show what goes on the wire.

## 1. Model the API as a tree

A declaration is one **client**, any number of nested **scopes**, and **endpoints**:

- the **client** is one API: its base URL and the values every request shares (a key, an API
  version);
- a **scope** is a level you navigate through — a tenant, a repository, a region — and owns what
  its endpoints share: a path prefix, a subdomain label, query parameters, headers, values;
- an **endpoint** is one operation: method, its own path, values, an optional body and a declared
  output.

```rust
use concord::reqwest::{self, header::{ACCEPT, AUTHORIZATION}};
use serde::Deserialize;
use concord::Json;

#[derive(Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
}

concord::api! {
    /// A slice of a code-hosting API.
    pub client Forge(#[sensitive] header(AUTHORIZATION) token: String) {
        base "https://api.forge.example";
        headers { ACCEPT = "application/json"; }
        response Json;

        /// One repository.
        scope repo(owner: String, name: String) {
            path "repos/{owner}/{name}";

            /// Lists issues.
            GET issues(query state?: String) -> Vec<Issue> { path "issues"; }
            /// One issue.
            GET issue(number: u64) -> Issue { path "issues/{number}"; }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let forge = Forge::new(reqwest::Client::new(), ForgeConfig { token: "Bearer t".into() });
    let repo = forge.repo("rust-lang", "rust");

    let prepared = repo.issue(42).prepare()?;
    let url = prepared.request().url().as_str();
    assert_eq!(url, "https://api.forge.example/repos/rust-lang/rust/issues/42");
    Ok(())
}
```

The generated types are `Forge`, `ForgeConfig` (the client's required values), `ForgeRepo` (the
scope) and one request type per endpoint, `ForgeRepoIssues` and `ForgeRepoIssue`. Navigation
methods have the child's name: `forge.repo(..)`, `repo.issue(..)`.

Rules of the structure:

- One `api!` declares exactly one client, with exactly one `base`.
- Each owner (client, scope, endpoint) has at most one `path`, `subdomain`, `query { }`,
  `headers { }`, `request` and `response` statement (`format F;` counts as both), and on clients
  and scopes they come before the child declarations. `request`/`response`/`format` give the
  formats of the bodies and outputs below, the nearest one winning
  ([section 5](#5-bodies-and-responses-formats)).
- Scopes may omit their parameter list (`scope admin { .. }`); endpoints always have one, possibly
  empty (`GET ping();` or `GET ping() { path "ping"; }`).
- Methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.

## 2. Values: required, optional, initialized

Every input is a **value** declared in a parameter list. Its kind is decided by `?` and `= expr`,
never by its Rust type:

| Declaration | Starts as | Supplied / changed with |
|---|---|---|
| `x: T` | required | client: a field of `{Client}Config`; scope or endpoint: a positional argument; then `set_x(v)` |
| `x: T = expr` | `expr` | `set_x(v)` |
| `x?: T` | absent | `set_x(v)` (present), `maybe_x(o: Option<T>)` (exactly `o`) |
| `x?: T = expr` | present, `expr` | `set_x(v)`, `maybe_x(o)` |
| `body b: T` | required | positional argument; then `set_b(v)` |
| `body b?: T` | absent (no body) | `set_b(v)`, `maybe_b(o)` |

```rust
use concord::reqwest;

concord::api! {
    pub client Shop(
        #[sensitive] header("x-api-key") key: String, // required
        header("accept-language") lang?: String = "en", // optional, present by default
    ) {
        base "https://shop.example.com";

        GET search(
            query q: String,       // required
            query limit: u32 = 20, // initialized
            query cursor?: String, // optional, absent by default
        ) {
            path "search";
        }
    }
}

fn main() -> Result<(), concord::Error> {
    // The generated signatures, checked here by the compiler. Positional arguments and `set_*`
    // take `impl IntoValue<T>`: the exact `T`, `&str`/`&String` where `T` is `String`, or
    // `&HeaderValue` where `T` is `HeaderValue`.
    let _: fn(reqwest::Client, ShopConfig) -> Shop = Shop::new;
    let _: fn(Shop, String) -> Shop = Shop::set_key;
    let _: fn(Shop, String) -> Shop = Shop::set_lang;
    let _: fn(Shop, Option<String>) -> Shop = Shop::maybe_lang;
    let _: fn(&Shop, String) -> ShopSearch = Shop::search;
    let _: fn(ShopSearch, String) -> ShopSearch = ShopSearch::set_q;
    let _: fn(ShopSearch, u32) -> ShopSearch = ShopSearch::set_limit;
    let _: fn(ShopSearch, String) -> ShopSearch = ShopSearch::set_cursor;
    let _: fn(ShopSearch, Option<String>) -> ShopSearch = ShopSearch::maybe_cursor;

    let shop = Shop::new(reqwest::Client::new(), ShopConfig { key: "k".into() });
    let first = shop.search("lamp").prepare()?;
    assert_eq!(first.request().url().query(), Some("q=lamp&limit=20"));
    let next = shop.search("lamp").set_limit(5).set_cursor("c2").prepare()?;
    assert_eq!(next.request().url().query(), Some("q=lamp&limit=5&cursor=c2"));
    Ok(())
}
```

What to know about values:

- **`?` is Concord's presence marker.** `x: Option<T>` is a *required* value whose type happens
  to be an `Option`; `x?: T` is an optional value. Concord never infers meaning from a type name.
- **Required client values** are the fields of the generated `{Client}Config` struct, in
  declaration order, with their documentation. Fields take exactly `T`, so a literal needs
  `.into()`; a missing field is reported by name (`E0063`). A client without required values is
  created with `Client::new(http)`.
- **Scope and endpoint arguments** are positional, in declaration order (a required body takes its
  place among them). A value that the API requires stays required, however long the argument
  list gets: never make it optional or give it a default just to shorten calls. To keep calls
  readable, move inputs shared by several endpoints to a meaningful scope, group related inputs
  in a domain type (a body type, a newtype), or add a convenience method of your own that takes a
  named input record and calls the endpoint.
- **Initializers** (`= expr`) are any Rust expression, evaluated once each time the owner is
  created — `new` for the client, each scope entry, each endpoint call — even if a setter replaces
  the value right after (so keep side effects out of initializers whose value callers often
  replace). Cloning, setters, `prepare` and `send` never evaluate them. The
  expression is compiled in a nested function: it can use items of the module (constants,
  functions), not other values or arguments.
- **No reset.** `maybe_x(None)` on an initialized optional value makes it *absent*; it does not
  bring back `expr`. Keep the original handle, or call the endpoint again, to start over.
- **`Clone`.** Client and scope values must be `Clone` (handles are copy-on-write snapshots).
  Endpoint values need not be: a request is `Clone` exactly when all its values and its body are.
- **Every value must be used** — placed, or referenced from a template or a block. An unused value
  is a compile error.

An initializer that must be fresh per call, such as an idempotency key, is just an expression:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use concord::reqwest;

static NEXT: AtomicU64 = AtomicU64::new(1);

fn request_id() -> String {
    format!("req-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

concord::api! {
    pub client Payments {
        base "https://pay.example.com";
        POST charge(header("idempotency-key") key: String = request_id()) { path "charges"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let api = Payments::new(reqwest::Client::new());
    let charge = api.charge(); // `request_id()` runs here, once
    let retry = charge.clone(); // a clone keeps the same key
    let key = |p: concord::Prepared<()>| p.request().headers()["idempotency-key"].clone();
    assert_eq!(key(charge.prepare()?), "req-1");
    assert_eq!(key(retry.prepare()?), "req-1");
    assert_eq!(key(api.charge().prepare()?), "req-2"); // a new call evaluates it again
    Ok(())
}
```

## 3. Base, paths and subdomains

**Base.** `base "https://api.example.com/v2";` is checked at compile time: `http` or `https`, an
ASCII domain name or an IP address, an optional port, an optional path — no user information,
query, fragment, `%` escapes or `xn--` labels. Callers replace it at runtime with
`with_base_url(..)`, which returns a `Result` and accepts more than that static subset: any
`http`/`https` URL with a host and no user information, query, fragment or port 0, including IDN
hosts and a percent-escaped path, whose escapes are kept as written (`https://example.com/a%2Fb`).
`with_base_url` replaces the **whole** base, path included: a version prefix that must survive
(`/v1` for a test server at `http://127.0.0.1:port`) belongs in the client's `path "v1";`, not in
`base`.

**Credentials and redirects.** reqwest drops `Authorization`, `Cookie`, `Cookie2`,
`Proxy-Authorization` and `WWW-Authenticate` when a redirect changes the host, port or scheme, but
**not** a custom credential header (`PRIVATE-TOKEN`, `X-Api-Key`). An SDK with such a header documents (or builds) a client
whose redirect policy refuses a cross-origin hop — `reqwest::redirect::Policy::custom` comparing
`attempt.url().origin()` with `attempt.previous()[0].origin()` — or disables redirects. The
application owns its `reqwest::Client`; Concord never changes it.

**Paths.** Each owner's `path` template is relative (no leading `/`) and is appended after the base
path, root first. Literal text is sent verbatim; each `{value}` is formatted with `Display` and
percent-encoded so that it is always exactly **one** segment (`a/b` becomes `a%2Fb`). A segment
that contains values must not come out empty, `.` or `..`: that is a build error from `prepare()`,
and nothing is sent (`.x` is an ordinary segment). A trailing `/` is allowed only on an endpoint's
own path. Optional values cannot appear in a path.

**Subdomains.** Each owner's `subdomain` template is split on `.` into labels that are prepended to
the base host, the deepest owner's labels leftmost. Labels are lowercased and must be 1–63
letters, digits or inner hyphens; the base host must then be a domain name, not an IP address.

```rust
use std::fmt;

use concord::reqwest;

#[derive(Clone, Copy)]
pub enum Region {
    Eu,
    Us,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Region::Eu => "eu",
            Region::Us => "us",
        })
    }
}

concord::api! {
    pub client Storage {
        base "https://example.com/v2";
        subdomain "storage";

        scope region(region: Region) {
            subdomain "{region}";

            scope bucket(name: String) {
                path "buckets/{name}";

                GET object(key: String) { path "objects/{key}"; }
                GET listing() { path "objects/"; }
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let bucket = Storage::new(reqwest::Client::new()).region(Region::Eu).bucket("photos");

    let object = bucket.object("2024/cat.jpg").prepare()?;
    assert_eq!(
        object.request().url().as_str(),
        "https://eu.storage.example.com/v2/buckets/photos/objects/2024%2Fcat.jpg"
    );
    let listing = bucket.listing().prepare()?;
    assert_eq!(
        listing.request().url().as_str(),
        "https://eu.storage.example.com/v2/buckets/photos/objects/"
    );

    // A value that is empty, `.` or `..` cannot be a segment: a build error, nothing is sent.
    let error = bucket.object("..").prepare().unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    assert!(bucket.object(".x").prepare().is_ok());

    // A runtime base may carry percent escapes; they are kept as written.
    let escaped = Storage::new(reqwest::Client::new()).with_base_url("https://example.com/a%2Fb")?;
    let object = escaped.region(Region::Us).bucket("photos").object("k").prepare()?;
    assert_eq!(
        object.request().url().as_str(),
        "https://us.storage.example.com/a%2Fb/buckets/photos/objects/k"
    );
    Ok(())
}
```

## 4. Query parameters and headers

### 4.1 Placing a value

The shortest form places a value directly in its parameter list:

| Placement | Sends |
|---|---|
| `query limit: u32` | `limit=<value>` |
| `query("per-page") per_page: u32` | `per-page=<value>` |
| `header("x-api-version") version: String` | `x-api-version: <value>` |
| `header(AUTHORIZATION) token: String` | `authorization: <value>` (a native `HeaderName` constant, §4.8) |
| `header(ACCEPT) accept: HeaderValue` | `accept: <the native value, byte for byte>` |
| `query("tag") each tags: Vec<String>` | one `tag=<item>` per item (any `IntoIterator` whose items are `Display`) |
| `header("x-tag") each tags: Vec<HeaderValue>` | one `x-tag` line per item, in order |

### 4.2 Blocks, templates and references

`query { .. }` and `headers { .. }` blocks set keys from literals, values or templates. **The left
of `=` is a key**: a literal, or for a header a Rust path to a `HeaderName` (§4.8). **The right of
`=` is a value**: a template (`"Bearer {token}"`) or the name of a declared value. A bare name
refers to a value **of the current owner only**; a value of an enclosing owner is written
`client.x` or `<scope>.x`. A Rust constant is not a value: write its text as a literal, or declare
a value initialized from it (`header(ACCEPT) accept: &'static str = JSON`).

```rust
use concord::reqwest;

concord::api! {
    pub client Notes(#[sensitive] token: String, account: String) {
        base "https://notes.example.com";
        headers { "authorization" = "Bearer {token}"; }

        scope folder(folder: String) {
            path "folders/{folder}";
            query { "account" = client.account; }

            GET list(query("page-size") size: u32 = 50) {
                path "notes";
                query { "scope" = "{client.account}/{folder.folder}"; "format" = "compact"; }
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let config = NotesConfig { token: "t".into(), account: "acme".into() };
    let notes = Notes::new(reqwest::Client::new(), config);
    let prepared = notes.folder("inbox").list().prepare()?;
    assert_eq!(
        prepared.request().url().query(),
        Some("account=acme&page-size=50&scope=acme%2Finbox&format=compact")
    );
    assert_eq!(prepared.request().headers()["authorization"], "Bearer t");
    Ok(())
}
```

Within one owner, direct placements (in parameter order) come before the block's statements (in
source order). Header names are case-insensitive and sent in lowercase; query keys are
case-sensitive.

### 4.3 What an owner may do with an inherited key

Each key is set at most once along the chain unless an operation says otherwise. The operations,
written inside `query { }` or `headers { }`:

| Statement | Requires the key to be set further up | Effect |
|---|---|---|
| `"k" = v;` or a direct placement | must **not** be set | sets `k` to `v` |
| `append "k" = v;` | — | adds `v` after the existing occurrences |
| `override "k" = v;` (`v` always present) | must be set | `k` becomes exactly `v` |
| `override "k" = v else inherit;` (`v` can be absent) | must be set | `v` when present; otherwise the inherited occurrences stay |
| `remove "k";` | must be set | drops the inherited occurrences; the key may then be set again |

A source is *present* when every optional value it reads is present. Mistakes — setting an
inherited key without an operation, overriding or removing a key nobody sets — are compile errors
(see [Diagnostics](DIAGNOSTICS.md)); with native header constants, the ones that depend on which
header a constant holds are build errors from `prepare()` (§4.8). A header key in these statements
is a literal or a native constant (`override AUTHORIZATION = token;`, `remove "accept";`).

### 4.4 Overriding with an optional value: `else inherit`

When an override reads an optional value, the declaration must say what absence means. `else
inherit` keeps the inherited value; leaving it out is a compile error that names both choices.

### 4.5 Exact replacement: `remove`, then set

"Send my value, or nothing at all" is `remove "k";` plus a new `"k" = v` — or, shorter, `remove`
plus a direct placement of the same key, because within one owner `remove` acts first, on what
the owner inherits.

### 4.6 Letting the caller choose: inherit, omit or replace

`remove` is static. To let each call choose between the inherited value, no value, and its own
value, declare an optional `Option<T>` and override with `each ... else inherit`: absent inherits,
`None` sends nothing (zero items), `Some(v)` replaces. The choice exists where it is declared:
give each endpoint that offers it its own value and `override` (declared on a scope instead, it is
made once for every request created from that scope handle).

```rust
use concord::reqwest;

concord::api! {
    pub client Docs(query locale: String = "en") {
        base "https://docs.example.com";

        // 4.4: the page's locale when given, otherwise the client's.
        GET page(page_locale?: String) {
            path "page";
            query { override "locale" = page_locale else inherit; }
        }

        // 4.5: exactly the given locale, or none.
        GET raw(query locale?: String) {
            path "raw";
            query { remove "locale"; }
        }

        // 4.6: the caller chooses per call.
        GET search(locale?: Option<String>) {
            path "search";
            query { override "locale" = each locale else inherit; }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let docs = Docs::new(reqwest::Client::new());
    let query = |p: concord::Prepared<()>| p.request().url().query().map(str::to_owned);

    assert_eq!(query(docs.page().prepare()?).as_deref(), Some("locale=en"));
    assert_eq!(query(docs.page().set_page_locale("fr").prepare()?).as_deref(), Some("locale=fr"));

    assert_eq!(query(docs.raw().prepare()?), None);
    assert_eq!(query(docs.raw().set_locale("fr").prepare()?).as_deref(), Some("locale=fr"));

    let inherit = docs.search(); // `maybe_locale(None)`: absent, the default
    let omit = docs.search().set_locale(None);
    let replace = docs.search().set_locale(Some("fr".to_owned()));
    assert_eq!(query(inherit.prepare()?).as_deref(), Some("locale=en"));
    assert_eq!(query(omit.prepare()?), None);
    assert_eq!(query(replace.prepare()?).as_deref(), Some("locale=fr"));
    Ok(())
}
```

### 4.7 Order on the wire

- **Query:** each key keeps the position of its first appearance on the chain (root first), and
  its occurrences are in contribution order. The query string is exactly that sequence.
- **Headers:** the values of one name are sent in contribution order. The order *between*
  different header names is not specified (the native `HeaderMap` groups by name, and reqwest and
  hyper add their own headers).
- Repeats interleaved across keys (`f=a&op=eq&f=b`) cannot be declared; append them to the native
  URL instead — see the [interleaved-query recipe](RECIPES.md#interleaved-query-parameters).
- Moving a key to another owner moves its position: [§11.1](#111-moving-a-default-into-a-scope)
  shows the edit and what it changes for each sibling.

```rust
use concord::reqwest;

concord::api! {
    pub client Feed(query("tag") each tags: Vec<String> = Vec::new()) {
        base "https://feed.example.com";
        query { "v" = "2"; }

        GET items(query limit: u32 = 10, extra?: String) {
            path "items";
            query { append "tag" = extra; }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let tags = vec!["a".to_owned(), "b".to_owned()];
    let feed = Feed::new(reqwest::Client::new()).set_tags(tags);
    let prepared = feed.items().set_extra("c").prepare()?;
    // `tag` keeps the client's position; the endpoint's occurrence follows the client's.
    assert_eq!(prepared.request().url().query(), Some("tag=a&tag=b&tag=c&v=2&limit=10"));
    Ok(())
}
```

### 4.8 Header names and values

**Names.** A header key is a literal name or a Rust path to a native
`reqwest::header::HeaderName` constant or static — a standard one or your own:

```text
header(AUTHORIZATION) token: String            headers { ACCEPT = "application/json"; }
header(X_RIOT_TOKEN) key: String               headers { override AUTH = fresh; remove "accept"; }
header("x-request-id") id: String              headers { header::ACCEPT_LANGUAGE = "en"; }
```

- A path is resolved by rustc where it is written (imports, aliases, re-exports, qualified and
  absolute paths) and must name a `HeaderName`; a string constant is not converted ("`&str` is
  not a native header name"). Concord never reads a constant's identifier: a local constant
  named `AUTHORIZATION` that holds `x-other` is the header `x-other`. Messages and the generated
  docs name a native key as you wrote it (`X_RIOT_TOKEN`). `HeaderName::from_static` itself
  accepts only lowercase names; `from_static("X-Api-Key")` is rustc's `E0080` from `http`.
- A literal name is checked by `http::HeaderName::from_bytes` when the macro expands (invalid
  names are compile errors) and normalized to lowercase. `host`, `content-length`,
  `transfer-encoding`, `connection`, `te`, `trailer`, `upgrade` and `keep-alive` are rejected (the
  transport owns them) — through a native constant too, as a build error when the request is
  built.
- **Composition follows the header, not its spelling** (§4.3): `"authorization"`, `AUTHORIZATION`
  and an alias of it are one header, so `override "Authorization" = ..` replaces an inherited
  `header(AUTHORIZATION)`. When every key of an endpoint's chain is a literal, the macro composes
  it and every mistake is a compile error. When a chain names a native constant, the macro cannot
  see which header the constant holds: it still reports the mistakes it can prove (literals on
  both sides, an optional `override` without `else inherit`), and the others — such as setting
  `"authorization"`, or `AUTHORIZATION` again, below `header(AUTHORIZATION)` without `override` —
  are a `Build` error from
  `prepare()` naming the declaration (and, for a header set twice, the earlier one). Names are
  compared before values, so such an endpoint fails even when its value is absent:
  `Client::validate_header_names()` in a unit test finds these mistakes for every endpoint at
  once, without building anything (§4.9). Which header each contribution names is decided on the names
  alone, before any value is converted: a removed or replaced value is never formatted.

**Values.** A value placed directly (`header(K) x: T`, `K = x`, `K = each xs`) goes through
`concord::ToHeaderValue`, a closed set: a native `HeaderValue` (sent byte for byte with its
sensitivity flag; a `static` or `from_static` value is cloned without copying), text (`String`,
`&str`), the integers and `bool`, and `&T`/`Box`/`Rc`/`Arc`/`Cow` of these. Send another type's
`Display` text with a template (`X_SORT = "{sort}"`), or build a `HeaderValue`.

- Text — a literal, a template's result, a `String` value — is whatever
  `http::HeaderValue::from_str` accepts, and `http`'s rule is the authority: visible ASCII,
  space, horizontal tab and UTF-8 beyond ASCII are accepted; every other control byte (a line
  break, NUL, DEL) is rejected. Literal text and each literal fragment of a template are checked
  when the macro expands, after Rust escapes and `{{ }}` are decoded, so a line break in a literal
  fragment is a compile error that no value can repair. Formatted text is checked when the
  request is built: a build error names the header, never the value.
- A native value is not text: it cannot fill a path, a query value or a template hole. So the
  only value that can carry its own sensitivity flag is never written anywhere else, and a value's
  header and text placements are classified alike.
- `remove` and an absent key concern Concord's own contributions: headers configured on the
  `reqwest::Client` (default headers, user agent) are still added by reqwest. That holds for
  `Accept` too: a request header, authored or suggested, replaces a client default, and when
  Concord sends none, reqwest's own goes out (5.8).

```rust
use concord::reqwest::{self, header::{self, ACCEPT, AUTHORIZATION, HeaderName, HeaderValue}};

/// A vendor header, declared the native way.
pub const X_TRACE: HeaderName = HeaderName::from_static("x-trace");
static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");

concord::api! {
    pub client Feed(
        header(AUTHORIZATION) token: String,
        header(ACCEPT) accept: HeaderValue = &ACCEPT_JSON,
        header(X_TRACE) trace?: HeaderValue,
    ) {
        base "https://feed.example.com";
        GET items(header("x-page") page: u32) { path "items"; }
        GET public() {
            path "public";
            headers { remove "authorization"; }        // the header `AUTHORIZATION` names
        }
        GET localized() {
            path "localized";
            headers {
                header::ACCEPT_LANGUAGE = "fr, café"; // UTF-8 is a valid value
                "x-note" = "a\tb";                    // and so is a horizontal tab
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let mut secret = HeaderValue::from_static("t-123");
    secret.set_sensitive(true); // kept: Concord's output shows `<redacted>`
    let feed = Feed::new(reqwest::Client::new(), FeedConfig { token: "Bearer t".into() })
        .set_trace(secret);
    let prepared = feed.items(2).prepare()?;
    let headers = prepared.request().headers();
    assert_eq!(headers["accept"], "application/json");
    assert!(headers["x-trace"].is_sensitive());
    // `authorization` is on Concord's list of sensitive names, whatever the spelling.
    assert!(headers["authorization"].is_sensitive());
    assert_eq!(headers["x-page"], "2");
    assert!(format!("{prepared:?}").contains("x-trace: <redacted>"));

    let public = feed.public().prepare()?;
    assert!(public.request().headers().get("authorization").is_none());
    let localized = feed.localized().prepare()?;
    assert_eq!(localized.request().headers()["accept-language"].as_bytes(), "fr, café".as_bytes());
    assert_eq!(localized.request().headers()["x-note"].as_bytes(), b"a\tb");
    Ok(())
}
```

### 4.9 Checking header names in a test

Every generated client has an associated function `validate_header_names()`. It checks, without
a client, a configuration, endpoint values, a network or an async runtime — it reads only the
native `HeaderName` constants — the header-name rules `prepare()` applies when it composes the
headers, on every endpoint compiled under the current `cfg`: a header introduced twice, an
`override`, `else inherit` or `remove` with nothing to act on, a header field reqwest manages,
and a `content-type` header next to a body's declared media type. Their verdict does not depend
on which optional values are present, so no request of a reported endpoint can be built. Its
`prepare()` returns this problem unless a step that runs before header composition fails first
(encoding the body, formatting the host, path or query: the build order is in
[SEMANTICS §6](../v4/SEMANTICS.md)). `Ok(())` means no `prepare()` fails on header names.
Values, their conversion, credentials and servers are not checked. Chains with literal names
only were already checked when the declaration compiled.

```rust
use concord::reqwest::header::{AUTHORIZATION, HeaderName};

/// The credential header, under another name.
pub const AUTH: HeaderName = AUTHORIZATION;

concord::api! {
    pub client Vault(#[sensitive] header(AUTHORIZATION) token: String) {
        base "https://vault.example.com";
        GET status() { path "status"; headers { remove AUTH; } }
        /// A mistake: `AUTH` is `authorization`, which the client already sets.
        GET secrets() { path "secrets"; headers { AUTH = "Bearer other"; } }
    }
}

fn main() {
    // In a crate, `#[test] fn header_names() { Vault::validate_header_names().unwrap(); }`.
    let errors = Vault::validate_header_names().unwrap_err();
    let [problem] = errors.problems() else { panic!("{errors}") };
    assert_eq!(problem.endpoint(), "Vault.secrets");
    assert_eq!(problem.declaration(), "`AUTH` on `Vault.secrets`");
    assert_eq!(problem.earlier(), Some("`AUTHORIZATION` on `Vault`"));
}
```

It returns one problem per endpoint (the one that endpoint's `prepare()` reports when it
composes the headers), in declaration order, each with `endpoint()`, `declaration()`, `earlier()` (for a header introduced
twice) and `reason()`. `Display` lists them one per line, and `Debug` (what `unwrap()` prints)
shows the same lines, with declarations and endpoint names, never values:

```text
Vault.secrets: `AUTH` on `Vault.secrets` sets a header that an earlier declaration already sets (both name the same header); use `override` or `append`, or `remove` it first (earlier: `AUTHORIZATION` on `Vault`)
```

An endpoint may not be named `validate_header_names`.

## 5. Bodies and responses: formats

A body and a typed output are declared with Rust types only — `body x: T` (or `body x?: T`) in the
parameter list, `-> T` after it — exactly as the generated method takes and returns them. How
they travel is their **format**, said by statements that any owner can write, with the same words
on the client, a scope or an endpoint:

```text
format F;                 // both directions
request F;                // how a body is encoded (its media type: F's own)
request F "media/type";   // ... with that media type instead
response F;               // how a typed output is decoded from a 2xx response
```

A mostly-JSON API says JSON once and writes each exception on the endpoint it concerns:

```rust
use concord::bytes::Bytes;
use concord::reqwest;
use serde::{Deserialize, Serialize};
use concord::{Binary, Json, Text};

#[derive(Deserialize)]
pub struct Product {
    pub id: u64,
}

#[derive(Serialize)]
pub struct NewProduct {
    pub name: String,
}

#[derive(Serialize)]
pub struct ProductPatch {
    pub name: Option<String>,
}

concord::api! {
    pub client Shop {
        base "https://shop.example.com/v1";
        format Json; // bodies and typed outputs below are JSON

        GET product(id: u64) -> Product { path "products/{id}"; }
        POST create(body product: NewProduct) -> Product { path "products"; }
        PATCH patch(id: u64, body patch: ProductPatch) -> Product {
            path "products/{id}";
            request Json "application/merge-patch+json";
        }
        GET description(id: u64) -> String {
            path "products/{id}/description";
            response Text; // raw text, not a JSON string
        }
        PUT image(id: u64, body png: Bytes) {
            path "products/{id}/image";
            request Binary "image/png";
        }
        DELETE delete(id: u64) { path "products/{id}"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let shop = Shop::new(reqwest::Client::new());

    let create = shop.create(NewProduct { name: "pen".into() }).prepare()?;
    assert_eq!(create.request().headers()["content-type"], "application/json");
    let body = create.request().body().and_then(|b| b.as_bytes());
    assert_eq!(body, Some(&br#"{"name":"pen"}"#[..]));

    let patch = shop.patch(7, ProductPatch { name: None }).prepare()?;
    assert_eq!(patch.request().headers()["content-type"], "application/merge-patch+json");

    let image = shop.image(7, Bytes::from_static(b"\x89PNG")).prepare()?;
    assert_eq!(image.request().headers()["content-type"], "image/png");

    let _raw_text: concord::Prepared<String> = shop.description(7).prepare()?;
    let delete = shop.delete(7).prepare()?; // no body: no `Content-Type`
    assert!(delete.request().headers().get("content-type").is_none());
    Ok(())
}
```

The generated signatures are the declared ones (`create(product: NewProduct)`,
`send() -> Result<Product, Error>`). Each endpoint's rustdoc — on its navigation method and on its
request type, where `prepare` and `send` are — states the body's format and media type, the
output's format, and which statement they come from.

### 5.1 The formats

| Format | Body (`Encode<T>`) | Output (`Decode<T>`) and its check of the response head | `Accept` it suggests |
|---|---|---|---|
| `Json` (feature `json`, on by default) | any `T: Serialize`; `application/json` | any `T: DeserializeOwned`; exactly one `Content-Type`, `application/json` or `application/*+json`, `charset` absent or once and UTF-8 or US-ASCII; anything else is refused before the body is read | `application/json` |
| `Text` (feature `extras`) | `String`; `text/plain; charset=utf-8` | `String`; any `Content-Type`, also a missing, repeated or malformed one; the one refusal is a single well-formed `Content-Type` declaring a `charset` other than UTF-8 or US-ASCII (also among repeated `charset` parameters); invalid UTF-8 is a decode error | none |
| `Binary` (feature `extras`) | `Bytes` or `Vec<u8>`; `application/octet-stream` | `Bytes` or `Vec<u8>`, unchanged; nothing is checked (`image/png` is fine) | none |
| `Form` (feature `form`) | any `T: Serialize` with flat fields (a struct, a map, pairs); `application/x-www-form-urlencoded` | any `T: DeserializeOwned`; exactly one `application/x-www-form-urlencoded`, `charset` absent or once and UTF-8 or US-ASCII | `application/x-www-form-urlencoded` |
| `IgnoreContentType<F>` (feature `extras`) | `F`'s | `F`'s, without its `Content-Type` check | `F`'s |
| a type path: `Csv`, `codecs::Csv` | your `Encode<T>` ([5.7](#57-custom-formats)) | your `Decode<T>` | your `ACCEPT`, if any |

The check of the response head runs on a 2xx response **before the body is read**. A refusal is an
error of kind `Metadata` that keeps the response unread; a body that does not decode is a
`Decode` error ([caller guide](CALLER_GUIDE.md#6-errors)). `Json` is strict on purpose: a response
labelled `text/html`, or not labelled at all, is not parsed as JSON. Where a server cannot be
fixed, select the adapter on the one endpoint (or scope) that needs it, not on the client:
`response IgnoreContentType<Json>;`. It skips only the `Content-Type` check; the status, the
other head checks, the body decoding and the `Accept` suggestion are unchanged
([5.8](#58-what-goes-on-the-wire-accept-and-content-type)).

A format is named by its Rust type path, resolved by rustc where the statement is written, like any
type: `Json` after `use concord::Json;`, `concord::Json`, an alias (`use concord::Json as Wire;`),
a qualified, absolute or generic path (`codecs::Csv`, `::my_sdk::Vendor`). Concord's own formats
have no privilege over yours and no name is reserved: a value or a payload type may be called
`json`, and so may a format type of yours. A format that only `cfg`-disabled endpoints name is
never resolved.

### 5.2 Where formats come from

- **One rule.** For each endpoint and each direction, the nearest statement wins: the endpoint's
  own, then its scopes from the inside out, then the client. Siblings are unaffected by an
  endpoint's statement. `format F;` counts as both `request F;` and `response F;`; an owner states
  each direction at most once.
- **A statement replaces format and media type together.** `request Json;` on an endpoint under
  `request Json "application/vnd.api+json";` sends `application/json`; to keep a media type, restate
  it (`request Json "application/vnd.api+json";`).
- **Mistakes are compile errors.** A body or typed output that no statement reaches is reported at
  its type ("say `request Json;` on this endpoint, a scope or the client"). On an endpoint, a
  `request` without a body or a `response` without `->` has nothing to apply to and is an error;
  `format` needs one of the two. Statements on clients and scopes that nothing uses are fine (and
  resolve nothing, so a format behind a disabled feature costs nothing until used).
- **No clearing.** A default cannot be cleared for a subtree. For an API whose parts differ, put
  the defaults on the uniform scopes rather than on the client (or state the format on each
  endpoint).

The format, not the Rust type, decides the wire: under `format Json;`, `-> String` decodes a JSON
string (`"2.4.1"`, not raw `2.4.1`) and `body note: String` sends one. Raw text is `response Text;`
/ `request Text;`. Because a payload type such as `String` or `()` is accepted by several formats,
moving an endpoint under another default can change its wire meaning without a compile error: when
you move a group, state its formats on it (`response Json;`) if they must not change, and check
the endpoint rustdoc, which names each format and its origin.

### 5.3 Media types and `Content-Type`

A body's `Content-Type` is its media type, sent exactly when the body is sent: the format's own
(`Encode::CONTENT_TYPE`: `application/json`, ...) or the one its `request` statement declares
(`request Json "application/merge-patch+json";`). Declared media types are checked by the macro
(`type/subtype`, optional `; name=value` parameters, visible ASCII) and become compile-time
constants; a format's `CONTENT_TYPE` is checked at compile time too, and not used where a declared
media type replaces it.

A `content-type` header contribution that reaches the endpoint (placed or set in a `headers`
block, conditional or not) is for values chosen at runtime. It replaces a format's own media type
and is sent whether or not there is a body:

| Body sent | Header sent | `Content-Type` |
|---|---|---|
| no | no | none |
| no | yes | the header |
| yes | no | the format's media type |
| yes | yes | the header (once) |

An empty header value counts as sent; an `each` over an empty collection sends nothing. A
declared media type and a `content-type` header reaching the same endpoint are a compile error
("keep one"), or a build error when the header is named by a native constant the macro cannot
see through; `remove CONTENT_TYPE;` in the endpoint or an enclosing scope resolves it. The
`Content-Type` Concord inserts carries the `content-type` header's sensitivity (`#[sensitive]`,
sticky composition). The response is the other way round: its `Content-Type` is checked by the
response format (5.1), never compared with any request header, so `response` takes no media type.
A format may suggest an `Accept`, and an `ACCEPT` header you declare always wins (5.8).

```rust
use concord::reqwest;
use concord::reqwest::header::{CONTENT_TYPE, HeaderValue};
use concord::{Binary, Text};

concord::api! {
    pub client Files(header(CONTENT_TYPE) content_type?: HeaderValue) {
        base "https://files.example.com";
        request Binary;

        PUT upload(name: String, body data: Vec<u8>) { path "files/{name}"; }
        scope notes {
            headers { remove CONTENT_TYPE; }
            PUT note(name: String, body note: String) {
                path "notes/{name}";
                request Text "text/markdown";
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let files = Files::new(reqwest::Client::new());
    let default = files.upload("a.bin", vec![0, 1]).prepare()?;
    assert_eq!(default.request().headers()["content-type"], "application/octet-stream");

    let typed = files.clone().set_content_type(HeaderValue::from_static("image/webp"));
    let chosen = typed.upload("a.webp", vec![0, 1]).prepare()?;
    assert_eq!(chosen.request().headers()["content-type"], "image/webp");

    let note = typed.notes().note("a", "# A").prepare()?; // the header is removed here
    assert_eq!(note.request().headers()["content-type"], "text/markdown");
    Ok(())
}
```

### 5.4 Absent body, `null`, empty body

```rust
use concord::reqwest;
use serde::Serialize;
use concord::{Json, Text};

#[derive(Serialize)]
pub struct Discount {
    pub percent: u8,
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        request Json;

        PUT discount(body discount: Option<Discount>) { path "discount"; }
        POST search(body filter?: Discount) { path "search"; }
        PUT note(body note: String) { path "note"; request Text; }
    }
}

fn main() -> Result<(), concord::Error> {
    let store = Store::new(reqwest::Client::new());
    let body = |p: &concord::Prepared<()>| p.request().body().and_then(|b| b.as_bytes()).map(<[u8]>::to_vec);

    // A required `Option` body: `None` is JSON `null`, with its `Content-Type`.
    let null = store.discount(None).prepare()?;
    assert_eq!(body(&null), Some(b"null".to_vec()));
    assert_eq!(null.request().headers()["content-type"], "application/json");

    // An optional body that is absent: no body and no `Content-Type`.
    let absent = store.search().maybe_filter(None).prepare()?;
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());

    // An empty text body is a body: it has its `Content-Type`.
    let empty = store.note("").prepare()?;
    assert_eq!(body(&empty), Some(Vec::new()));
    assert_eq!(empty.request().headers()["content-type"], "text/plain; charset=utf-8");
    Ok(())
}
```

A body cannot have an initializer (`body b: T = ..` is a compile error): its presence is decided by
`?` only. An optional `Option<T>` body (`body b?: Option<T>`) has three states: absent
(`maybe_b(None)`), `null` (`set_b(None)`) and a value (`set_b(Some(v))`).

### 5.5 No output versus `-> ()`

- **No `->`**: the endpoint has no typed output. Any 2xx succeeds, the response body is drained
  without being buffered, and the output is `()`. A `response` statement does not apply (and is an
  error on the endpoint itself).
- **`-> ()`** is a typed output like any other, decoded with its format: under `response Json;` the
  response must be labelled JSON and its body must be JSON `null`. A 204 without a `Content-Type`
  is refused by the head check (`Metadata`), and an empty body labelled JSON is a decode error.
  `-> Unit` for `type Unit = ()` behaves the same. An endpoint that returns nothing omits `->`;
  one that answers either JSON or nothing (`200` or `204`) needs a format of its own that decides
  on the status ([custom formats](CUSTOM_FORMATS.md#7-strict-json-and-the-explicit-adapter)).
- Any 2xx status is success unless the format refuses the response head (`Metadata`); a non-2xx
  status is an error of kind `Status`. Both keep the unread response (see the
  [caller guide](CALLER_GUIDE.md#6-errors)). `-> Option<T>` gives no meaning to 404: `null`
  decodes to `None`, a 404 is still an error.

```rust
use concord::reqwest;
use concord::Json;

concord::api! {
    pub client Jobs {
        base "https://jobs.example.com";
        response Json;

        DELETE cancel(id: u64) { path "jobs/{id}"; }     // any 2xx; the body is drained
        POST ping() -> () { path "ping"; }               // the body must be `null`
    }
}

fn main() -> Result<(), concord::Error> {
    let jobs = Jobs::new(reqwest::Client::new());
    let _: concord::Prepared<()> = jobs.cancel(1).prepare()?;
    let _: concord::Prepared<()> = jobs.ping().prepare()?;
    Ok(())
}
```

### 5.6 Forms

`Form` is built in behind the `form` feature (`concord = { .., features = ["form"] }`), like
reqwest's own `form` feature: `request Form;` encodes any `Serialize` value with flat fields,
`Vec<(String, String)>` keeps pair order and repeated names, and `response Form;` decodes a form
response, once its head is checked: exactly one `Content-Type`,
`application/x-www-form-urlencoded`. An OAuth token endpoint takes a form and returns JSON.
Excerpt of [`crates/concord/tests/forms.rs`](../crates/concord/tests/forms.rs):

```rust,ignore
    concord::api! {
        /// A token endpoint.
        pub client OAuth {
            base "http://oauth.invalid";
            request Form;
            response Json;
            POST token(body credentials: Login) -> Token { path "token"; }
        }
    }
```

A value the form encoding cannot express (a nested structure, a sequence field) is a `Build`
error before anything is sent.

### 5.7 Custom formats

Any other format is a type of your own implementing `Encode<T>` and/or `Decode<T>`, named by its
path in a statement (`format Lines;`, `request codecs::Ndjson;`). There is no registry. The
[format guide](CUSTOM_FORMATS.md) covers encoders, decoders, a format that reads a header,
NDJSON with positions and testing; the essentials:

- On a 2xx response a decoder runs in this order: `check_content_type` (its checks of the response
  `Content-Type`; accepts by default), `context` (every other check of the head, producing an
  owned `Context`), then Concord reads the body and calls `decode_body(context, bytes)`.
  `decode_body` never sees the head, only the `Context`. A refusal in the first two steps is
  `ErrorKind::Metadata` and keeps the response unread; a failure in `decode_body` is
  `ErrorKind::Decode`, after the body was read, so refusals that depend only on the head belong in
  the first two steps.
- A failure is a `CodecError`. `CodecError::new` takes a `&'static str` (a `String` does not
  compile), `.at("line", n)` adds a numeric position, and `.with_source(e)` attaches the
  underlying error, which may quote the body. `Display` and `Debug` never show it; it is
  reachable only through `Error::codec_error()` and `CodecError::source_unredacted()`. Static
  labels are a safer default, not a proof: a format is trusted code.
- `const ACCEPT` suggests a request `Accept` (5.8). `decode_body` called directly decodes bytes
  without any head check: the route for an API's error document.

A stateless format names its context `()`:

```rust
use concord::bytes::Bytes;
use concord::{CodecError, Decode, Encode, ResponseHead, reqwest};

/// One item per line.
pub struct Lines;

impl Encode<Vec<String>> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: Vec<String>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.join("\n")))
    }
}

impl Decode<Vec<String>> for Lines {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Vec<String>, CodecError> {
        let text = String::from_utf8(body.into())
            .map_err(|e| CodecError::new("the response is not UTF-8").with_source(e))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

concord::api! {
    pub client Words {
        base "https://words.example.com";
        POST normalize(body words: Vec<String>) -> Vec<String> {
            path "normalize";
            format Lines;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let words = vec!["b".to_owned(), "a".to_owned()];
    let prepared = Words::new(reqwest::Client::new()).normalize(words).prepare()?;
    let request = prepared.request();
    assert_eq!(request.headers()["content-type"], "text/plain; charset=utf-8");
    assert_eq!(request.body().and_then(|b| b.as_bytes()), Some(&b"b\na"[..]));

    let decoded = <Lines as Decode<Vec<String>>>::decode_body((), Bytes::from_static(b"x\ny\n"))?;
    assert_eq!(decoded, ["x", "y"]);
    Ok(())
}
```

A request stream with a fixed media type can use a custom `Encode<T>` that moves its payload
into a streaming `reqwest::Body`; enable reqwest's `stream` feature (in your own `Cargo.toml`) for
those constructors. The encoder runs during preparation, but it need not read the stream then:
[`examples/native/src/upload.rs`](../examples/native/src/upload.rs) prepares a non-`Clone`
one-shot source without polling it, and one `send()` reads it once.

`Encode::CONTENT_TYPE` is static, and the encoder does not return per-body media headers. A
`request F "type/subtype";` statement or a `content-type` header replaces that static value (5.3,
5.8). For a media type that varies with the body (a multipart boundary), use the documented native
request-construction route through `into_parts` and reqwest's multipart support. An explicit
matching `content-type` header can also carry it when the endpoint or SDK controls that
body/header pair.

For a response consumed incrementally, use `send_raw` and native reads; typed `Decode` receives
the collected body bytes. `examples/native` has NDJSON and pair-encoded form formats and a
one-shot request stream, and `examples/multipart` a multipart upload ([recipes](RECIPES.md)).

### 5.8 What goes on the wire: `Accept` and `Content-Type`

**The request `Accept`.** Concord adds a format's suggestion (`Decode::ACCEPT`) after the headers
you declared are composed, and only as a fallback. The cases, as `crates/concord/tests/accept.rs`
checks them on prepared requests and on the wire:

| Endpoint and header chain | `Accept` Concord sends |
|---|---|
| typed output (`-> T`), the format suggests one (`Json`, `Form`), no `accept` declared | the suggestion (`application/json`) |
| typed output, the format suggests none (`Text`, `Binary`, a format of yours without `ACCEPT`) | none |
| no `->`, whatever the inherited format | none |
| an `accept` value survives the chain: a literal, a native `HeaderValue`, an empty value, repeated values, q-values | exactly what you declared, in order, as written; never merged with the suggestion |
| an optional `accept` value that is absent, or `each` over an empty list (nothing is placed) | the suggestion |
| the chain ends with `remove accept` (below an authored value too) | none |
| `remove accept`, then set again from an optional value | the value when present; the suggestion when absent (the chain no longer ends with a removal) |
| the suggestion, when `accept` is sensitive on this endpoint (a `#[sensitive]` value is placed under it, even one that is absent) | the suggestion, flagged sensitive: `accept: <redacted>` in Concord's `Debug` |

The suggestion is a request preference. The response is judged by the format's own check (5.1),
never by comparing its `Content-Type` with `Accept`, and the suggestion never selects another
format.

**The reqwest boundary.** Concord's `Accept`, authored or suggested, replaces a default `Accept`
of your `reqwest::Client` when the request is executed (reqwest fills in only the names a request
lacks). An API that selects its version through a client-wide default `Accept` therefore needs it
declared in `headers` once a typed endpoint's format suggests one. When Concord sends none, reqwest sends its
own: `*/*`, or the default you configured on the client. So suppressing Concord's suggestion does
not mean that no `Accept` reaches the wire. To drop only the suggestion for an endpoint, give it a
format that suggests none (a small wrapper of yours that forwards the wrapped format's checks and
decoding, see the [format guide](CUSTOM_FORMATS.md#8-the-accept-suggestion)); `remove ACCEPT` on a header nobody set
is an error, as for any key (4.3). After `prepare()?.into_parts()`, the native request already
carries the suggestion: `RequestBuilder::header(ACCEPT, v)` appends a second line, while
`request.headers_mut().insert(ACCEPT, v)` replaces it.

**The request `Content-Type`** (5.3), when a body is sent or a `content-type` header is declared:

| Body sent | `request F "media/type";` | `content-type` header reaches the endpoint | `Content-Type` sent |
|---|---|---|---|
| no | — | no | none |
| no | — | yes | the header |
| yes | no | no | `F`'s `CONTENT_TYPE` (none when it is `None`) |
| yes | yes | no | the declared media type |
| yes | no | yes | the header, once |
| yes | yes | yes | a compile error ("keep one"), or a build error with a native constant |

**The response `Content-Type`** is checked by the response format on a 2xx before the body is
read (the last column of 5.1). `IgnoreContentType<F>` skips that check and nothing else. A non-2xx
status is a `Status` error before any format is consulted.

Declarations checked on prepared requests (the example is compiled and run):

```rust
use concord::reqwest::{self, header::ACCEPT};
use concord::{Binary, IgnoreContentType, Json, Text};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Item {
    pub id: u64,
}

concord::api! {
    pub client Catalog {
        base "https://catalog.example.com";
        response Json;

        GET item() -> Item { path "item"; }                    // the suggestion
        GET legacy() -> Item {                                 // check skipped, suggestion kept
            path "legacy";
            response IgnoreContentType<Json>;
        }
        GET text() -> String { path "text"; response Text; }   // Text suggests nothing
        GET blob() -> Vec<u8> { path "blob"; response Binary; }
        GET ping() { path "ping"; }                            // no `->`: no suggestion
        GET vendor() -> Item {                                 // authored: sent as written
            path "vendor";
            headers { ACCEPT = "application/vnd.acme.v2+json"; }
        }

        scope acme {
            headers { "accept" = "application/vnd.acme.v2+json"; }
            GET item() -> Item { path "acme"; }
            GET raw() -> Item {                                // removed: nothing from Concord
                path "raw";
                headers { remove "accept"; }
            }
        }
    }
}

fn accepts<T>(prepared: &concord::Prepared<T>) -> Vec<&str> {
    let values = prepared.request().headers().get_all("accept");
    values.iter().map(|v| v.to_str().unwrap()).collect()
}

fn main() -> Result<(), concord::Error> {
    let api = Catalog::new(reqwest::Client::new());
    assert_eq!(accepts(&api.item().prepare()?), ["application/json"]);
    assert_eq!(accepts(&api.legacy().prepare()?), ["application/json"]);
    assert!(accepts(&api.text().prepare()?).is_empty());
    assert!(accepts(&api.blob().prepare()?).is_empty());
    assert!(accepts(&api.ping().prepare()?).is_empty());
    assert_eq!(accepts(&api.vendor().prepare()?), ["application/vnd.acme.v2+json"]);
    assert_eq!(accepts(&api.acme().item().prepare()?), ["application/vnd.acme.v2+json"]);
    assert!(accepts(&api.acme().raw().prepare()?).is_empty());
    Ok(())
}
```

### 5.9 Streaming outputs: `-> stream`

An endpoint whose useful result is read as it arrives (a watch, an export, a log) declares it.
`-> stream T` names the item and needs a format implementing `Incremental<T>` (`Csv<..>`,
`JsonLines`, `Ndjson`, `JsonStream` for consecutive JSON values, or your own): `send()` checks the
status and the head, then returns the format's reader **without reading the body**. `-> stream`
without an item returns the unread `Incoming` body, for a reader whose options are known only at
run time. Neither drains: draining is what an endpoint without `->` does, and a watch declared that
way would discard its events.

Excerpt of `checks/phase-a/src/lib.rs`:

```rust,ignore
        GET rows(id: String) -> stream Row {
            path "reports/{id}";
            response ReportCsv;
        }
        /// Change events (consecutive JSON values).
        GET events(#[sensitive] query("token") resume?: String) -> stream Event {
            path "events";
            response JsonStream;
        }
        /// Audit lines (JSON Lines).
        GET audit() -> stream Row { path "audit"; response JsonLines; }
        /// A job export whose dialect is known only at run time.
        GET export(id: String) -> stream { path "jobs/{id}/export"; }
```

The reader names the endpoint in every later error (`ReadError::operation`), with no string to
repeat. `completion()` says whether the input ended cleanly (`EndOfInput`), failed, or is still
being read; only the first proves every record was seen. `into_stream()` turns any reader into a
standard `futures_core::Stream` with the same errors (a recoverable JSON Lines line error does not
end it) and a cancellation-safe pending read. Reconnecting a watch, resuming an export and
deduplicating replays stay the SDK's or the application's policy.

### 5.10 Bodies whose media type is known at run time

When the bytes and their media type are produced together — a CSV dialect chosen per job, a
generated multipart boundary — the body still belongs in the declaration. Give it a type that
carries both and the `Encoded` format: `stream::Upload` (from `CsvWriter::upload` or
`JsonLinesWriter::upload`) or `EncodedBody`.

Excerpt of `checks/phase-a/src/lib.rs`:

```rust,ignore
        POST upload(id: String, body export: Upload) -> Row {
            path "jobs/{id}/export";
            request Encoded;
        }
```

A format of yours does the same by overriding `Encode::encode_body` (see
[CUSTOM_FORMATS](CUSTOM_FORMATS.md)). The media type stays bound to the bytes: a declared media
type (`request Encoded "m";`) or a `content-type` header that differs is a `Build` error before
anything is sent; an equal one is sent once; `remove content-type` keeps the runtime value. The
body's own length decides `Content-Length` versus chunked transfer, and preparing the request polls
a lazy source zero times.

### 5.11 Reading response metadata in an SDK

A provider's rate-limit counter, request id or served version is read from the response head,
between sending and consuming the body: `Prepared::receive()` returns a `Received<T>` for any
status, and `decode()` then applies the declared handling. Capture what must survive a body
failure before decoding, into a value the SDK owns.

Excerpt of `checks/phase-a/tests/received.rs`:

```rust,ignore
async fn summary_with_meta(
    api: &Reports,
    id: &str,
) -> Result<(Row, Meta), (concord::Error, Option<Meta>)> {
    let received = api
        .summary(id)
        .prepare()
        .map_err(|e| (e, None))?
        .receive()
        .await
        .map_err(|e| (e, None))?;
    let meta = Meta::capture(&received.head());
    match received.decode().await {
        Ok(row) => Ok((row, meta)),
        Err(e) => Err((e, Some(meta))),
    }
}
```

A provider rule on the head (a served API version) refuses by returning the SDK's error with
`received.into_response()`, unread. A request edited natively after `into_parts()` (a `Link` URL)
is executed with `Decoder::execute(&client, request)`, which returns the same `Received<T>`.

### 5.12 One adapter for every operation: `concord::Endpoint`

An SDK that returns its own error type and response metadata does not need a wrapper method per
operation. Every generated endpoint implements `concord::Endpoint<T>` for its output `T`
(`prepare()` and `operation()`), so one trait of the SDK, implemented for every `E: Endpoint<T>`,
serves all of them; callers import it once. The output is a parameter, so private output types
stay allowed in a public client.

Excerpt of `checks/phase-a/tests/received.rs`:

```rust,ignore
    pub trait Call<T>: Endpoint<T> {
        fn call(self) -> impl Future<Output = Result<(T, Meta), SdkError>>;
    }

    impl<T, E: Endpoint<T>> Call<T> for E {
        async fn call(self) -> Result<(T, Meta), SdkError> {
            let prepared = self.prepare().map_err(|e| SdkError::Concord(e, None))?;
            let received = prepared
                .receive()
                .await
                .map_err(|e| SdkError::Concord(e, None))?;
            let meta = Meta::capture(&received.head());
            match received.decode().await {
                Ok(value) => Ok((value, meta)),
                Err(e) => Err(SdkError::Concord(e, Some(meta))),
            }
        }
    }
```

It pays where the generated client is the client callers hold. An SDK that keeps a wrapper (a
validated credential type at construction, an input rule per operation) keeps its domain methods:
the trait cannot host a per-operation rule. A generated operation is not a future: code that passes
`api.op(..)` where a future is expected (`tokio::time::timeout(d, api.op(..))`) needs the adapter's
call (`api.op(..).call()`), and rustc's E0277 does not suggest it.

A provider error document on a non-2xx response is read with `received.bytes_limited(n)`: the body
up to `n` bytes, `ErrorKind::Limit` beyond (nothing more is read); parse it with the format's
`decode_body`. `<E as Endpoint<T>>::operation()` names the endpoint without building a request.

### 5.13 The SDK's own `send()`: `send Policy;`

When every call of a client should return the SDK's result — its error type with the provider's
error document, response metadata — or every request must be signed after it is built, name a
**send policy** on the client. A policy is a type of the SDK implementing `concord::SendWith<T>`
(usually generically over `T`); each generated `send()` then calls it with the endpoint and returns
its `Output`. Callers keep one terminal method and need no import; `prepare()` still returns the
native request with Concord's `Error`.

Excerpt of `checks/phase-a/tests/send_policy.rs`:

```rust,ignore
concord::api! {
    pub client Store {
        base "http://store.invalid";
        send Policy;
        format concord::Json;
        GET item(id: u64) -> Item { path "items/{id}"; }
        GET events() -> stream { path "events"; }
    }
}
```

A signing policy builds, signs and executes (the signature covers the final URL, so it is computed
after `prepare()`):

Excerpt of `checks/phase-a/tests/send_policy.rs`:

```rust,ignore
impl<T: Send> SendWith<T> for Signed {
    type Output = Result<T, concord::Error>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        let digest = format!("sig({} {})", request.method(), request.url().path());
        let mut value = HeaderValue::try_from(digest).expect("a header value");
        value.set_sensitive(true);
        request.headers_mut().insert("authorization", value);
        decoder.execute(&client, request).await?.decode().await
    }
}
```

Use a send policy when the result type of the whole client changes; keep domain methods for an
input rule of one operation (an identifier that must not be empty). Without `send`, generated
`send()` returns `Result<T, concord::Error>` as before.

## 6. Wire formatting: `Display` and newtypes

Every value written as text — path segment, subdomain label, query value, template hole — is
written with its `Display` implementation, then encoded for its placement. A value placed directly
as a header is not formatted: it goes through `ToHeaderValue` (§4.8), a closed set, so a type
that only has `Display` is sent as a header through a template (`X_SORT = "{sort}"`).
Consequences:

- `bool` is `true`/`false`; numbers use their `Display`.
- `Option<T>` and `Vec<T>` have no `Display`: place them with `each` (zero or more occurrences) or
  wrap them in a newtype that decides the spelling. Placing one directly is a compile error
  pointing at the type.
- A foreign type, or a type whose `Display` is meant for humans, gets a newtype with the wire
  spelling.
- A `Display` that returns `fmt::Error` makes the request a build error naming the placement;
  nothing partial is sent.
- Concord's generated `Debug` also shows values through `Display`, so a `u32` appears as `"20"`;
  a value placed only as a header is shown as its header value (`"caf\xe9"` for bytes that are
  not text, `<redacted>` for a natively sensitive one).

```rust
use std::fmt;

use concord::reqwest;

/// Sort order; `Display` is the wire spelling.
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

/// A comma-separated list in one query value.
#[derive(Clone)]
pub struct Csv(pub Vec<String>);

impl fmt::Display for Csv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(","))
    }
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        GET products(query sort?: Sort, query fields?: Csv, query("id") each ids?: Vec<u64>) {
            path "products";
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let fields = Csv(vec!["id".to_owned(), "name".to_owned()]);
    let request = Store::new(reqwest::Client::new())
        .products()
        .set_sort(Sort::PriceAsc)
        .set_fields(fields)
        .set_ids(vec![3, 5]);
    let prepared = request.prepare()?;
    assert_eq!(
        prepared.request().url().query(),
        Some("sort=price_asc&fields=id%2Cname&id=3&id=5")
    );
    Ok(())
}
```

## 7. Sensitive values

Mark a value `#[sensitive]` when Concord should never print it. It is still sent. Concord then
withholds it — and every query or header key, path segment and host label it feeds — from the
output Concord produces: the generated `Debug` of clients, scopes and requests, `Prepared`'s
`Debug`, and `Error`'s `Display` and `Debug`. Sensitive header values are also flagged with
`HeaderValue::set_sensitive`.

- A list of well-known names is a safety net, not a guarantee: keys named `authorization`,
  `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`, `api-key`, `apikey`,
  `x-auth-token`, `auth-token`, `access-token`, `refresh-token`, `id-token`, `token`,
  `client-secret`, `secret`, `password`, `signature`, `session`, `api-token`, `x-api-token`,
  `x-access-token`, `private-token`, `x-goog-api-key`, `ocp-apim-subscription-key` or
  `x-amz-security-token` (compared lowercase, `_` as `-`) are treated as sensitive without the
  marker. A vendor's own name (`X-Riot-Token`) is not on it.
- Prefer `#[sensitive]` on every secret even when its key is on this list: the name rule follows
  the key, so renaming the header to a name not on the list would silently stop the redaction,
  while the marker follows the value.
- Sensitivity is sticky along the chain: a later public override does not make a key public.
- Classification looks at every declared owner, also those behind a `#[cfg]` that is off: a
  literal key in a compiled-out endpoint can still withhold a value it reads (more redaction, not
  less). A native key there has no accessor, so it classifies nothing.
- A value that feeds a sensitive key is itself withheld, everywhere it appears.
- A native `HeaderValue` that is already flagged (`set_sensitive(true)`) stays flagged and is
  withheld from Concord's output, marker or not; Concord only ever adds the flag, per value.
- `#[sensitive]` applies to values, not bodies: Concord never renders bodies.

```rust
use concord::reqwest;

concord::api! {
    pub client Vault(#[sensitive] tenant: String, header("x-api-key") key: String) {
        base "https://vault.example.com";

        scope secret(name: String) {
            path "tenants/{client.tenant}/secrets/{name}";
            GET read();
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let config = VaultConfig { tenant: "acme".into(), key: "k-9".into() };
    let vault = Vault::new(reqwest::Client::new(), config);
    let prepared = vault.secret("db").read().prepare()?;

    let shown = format!("{prepared:?}");
    assert!(shown.contains("/tenants/<redacted>/secrets/db"));
    assert!(shown.contains("x-api-key: <redacted>")); // sensitive by name
    assert!(!shown.contains("acme") && !shown.contains("k-9"));
    assert!(!format!("{vault:?}").contains("acme"));
    Ok(())
}
```

What this does **not** cover — the native request and response `Debug`, errors of requests you
execute yourself, redirects, logs and proxies — is listed in the
[caller guide](CALLER_GUIDE.md#8-what-concord-redacts-and-what-it-does-not).

## 8. Attributes

| Attribute | Where | Effect |
|---|---|---|
| `///` documentation | everywhere | rendered on the generated items (types, navigation methods, config fields, setters) |
| `#[sensitive]` | values | see §7 |
| `#[cfg(..)]` | scopes, endpoints | applied to every generated item of that subtree and to the parent's navigation method |
| `#[deprecated(..)]` | scopes, endpoints | applied to the navigation method, so the warning appears where callers navigate |

Any other attribute is a compile error. Generated names are checked as if every `cfg` were
enabled, so two cfg-exclusive endpoints cannot share a name.

```rust
use concord::reqwest;

concord::api! {
    pub client Api {
        base "https://api.example.com";

        /// Only in builds with the `beta` feature.
        #[cfg(feature = "beta")]
        GET preview() { path "preview"; }

        /// The old listing.
        #[deprecated(note = "use `items`")]
        GET list() { path "list"; }

        GET items() { path "items"; }
    }
}

fn main() {
    let api = Api::new(reqwest::Client::new());
    #[allow(deprecated)]
    let _old = api.list();
    let _new = api.items();
}
```

## 9. Generated names and collisions

| Declared | Generated |
|---|---|
| `client Forge` | `Forge`; `ForgeConfig` when the client has a required value |
| `scope repo` under `Forge` | `ForgeRepo`, reached with `forge.repo(..)` |
| `GET list_issues` under `scope repo` | `ForgeRepoListIssues`, reached with `repo.list_issues(..)` |
| value `x` | `set_x`; `maybe_x` when optional |

- Within one generated type, navigation methods, value controls and the built-ins (`new`,
  `with_base_url`, `send`, `prepare`, `clone`) must not collide; a collision is a compile error
  naming both origins. Rename the endpoint or the value.
- Value and body names may not start with `__`; a client value may not be named `http`; a scope
  may not be named `client`; template holes cannot be `self`, `Self`, `super`, `crate` or `_`.
- Two `api!` invocations in one module share its namespace. Generated names are concatenations,
  so client `Shop` with scope `api` and a second client `ShopApi` both define `ShopApi` (rustc
  `E0428`). Give each client its own module, or names that cannot concatenate to each other.
  Generated names can also collide with your own types (`ForgeIssue`); a module per API avoids
  both:

```rust
mod forge {
    concord::api! {
        pub client Api { base "https://api.forge.example"; GET ping(); }
    }
}
mod shop {
    concord::api! {
        pub client Api { base "https://shop.example.com"; GET ping(); }
    }
}

fn main() {
    let http = concord::reqwest::Client::new();
    let (_forge, _shop) = (forge::Api::new(http.clone()), shop::Api::new(http));
}
```

- Naming a scope like its value (`scope tenant(tenant: String)`) is legal; references then read
  `tenant.tenant`.

## 10. Limits

- **Depth.** Each scope level nests the generated types. With the default `recursion_limit`, a
  chain of 48 scopes compiled and 56 failed (`E0320`, overflow while adding drop-check rules,
  reported on the whole invocation); `#![recursion_limit = "256"]` made 64 compile. These points
  were measured with rustc 1.97.1 and re-checked with 1.99.1; other compilers can differ. The
  limit is per crate: a crate that navigates a deep chain and prepares its requests can need its
  own `#![recursion_limit]`, not only the crate that declares it, and `cargo doc` can reach the
  limit before compilation does (a 32-scope chain compiled, but documenting it failed with
  `E0275` at the default limit; 256 fixed it). Real APIs are far shallower.
- **Very large declarations.** Generated code grows linearly with endpoints, inherited
  contributions and override chains, and quadratically only for endpoints at every level of a deep
  chain. A crash threshold measured with rustc 1.97.1, before the 1.99.1 pin, and not re-measured
  since: 1,000 nested optional overrides of one key (500 compiled). Evidence:
  `v4/refinement/evidence/tracks/compiler.md`.
- **Hosts.** IDNA is not performed; a declared base cannot use punycode (`xn--`) labels or `%`
  escapes in its path — pass such a URL to `with_base_url`.
- **Interleaved query repeats** across keys and per-call removal through `remove` are not
  declarable; see §4.6 and §4.7.

## 11. Changing a declaration

The generated methods follow the declaration (§9), so some edits change the caller's code. What
each edit does to code that calls the endpoint:

| Edit | Calling code |
|---|---|
| Add an optional or initialized value (`x?: T`, `x: T = e`) | unchanged argument list; `set_x` is new. **Exception:** the request is `Clone` only when every value type is (§9), so adding a value whose type is not `Clone` removes `Clone` from the request even while the value is absent, and code that clones the request (a pagination template, say) stops compiling (E0599). Keep added value types `Clone` when callers may clone requests |
| Add a required value (`x: T`) | breaks: a new positional argument (or a `{Client}Config` field) |
| Required → optional (`x: T` → `x?: T`) | breaks: the argument goes; callers write `.set_x(v)` |
| Optional → required | breaks: a new positional argument; `maybe_x` disappears |
| Required → initialized (`x: T` → `x: T = e`) | breaks: the argument goes; `set_x(v)` still replaces it |
| Reorder required values | breaks only if the types differ; with equal types it still compiles and **silently swaps the values**: rename instead |
| Add a required body (`body b: T`) | breaks: a new positional argument, where the body is declared |
| Add an optional body (`body b?: T`) | unchanged; no body is sent until `set_b` (same `Clone` exception as an added value) |
| Change the body or output type | breaks where the old type was used |
| Change a format or media type (`request`, `response`) | unchanged code; **different bytes on the wire**, a different check of the response, and possibly a different `Accept` |
| Move an endpoint to another scope | breaks: navigation goes through the new scope |
| Rename an endpoint, scope or value | breaks: the method names follow |
| Move a value between owners | breaks: it is supplied where it is now declared |
| Move a literal query or header entry into the enclosing scope | unchanged code; every endpoint below inherits it and the query order can change ([§11.1](#111-moving-a-default-into-a-scope)) |

`#[deprecated]` on a scope or endpoint (§8) lets callers migrate before a removal.

The table covers the generated methods only. Inferred traits (`Clone`, `Send`, `Sync`) follow the
declared value types, so check them with a representative downstream caller (one that clones or
sends requests across tasks), not only with `.send()`.

**Your SDK's own input records evolve by Rust's rules, not by the table.** An SDK that groups
several declared values into a public struct (a stored filter such as `IssueFilter { state,
labels, .. }`) breaks callers that write exhaustive struct literals or patterns as soon as it adds
a field (E0063, E0027), even though the generated `set_x` for the new value is additive; and a new
declared value that the SDK forgets to map from its record compiles and is silently never sent.
Decide the record's evolution rule when you publish it: private fields with a constructor and
setters (a builder) for records that will grow, or documented `..Default::default()` construction
with `#[non_exhaustive]` — and map a record exhaustively (destructure it without `..`) so that the
compiler points at the mapping when a field is added. (Observed in the 4.4 maintenance exercises:
`checks/maintenance-github/MAINTENANCE.md`, `checks/maintenance-stripe/MAINTENANCE.md`.)

### 11.1 Moving a default into a scope

Several endpoints of a scope send the same `per_page=30` and `x-cache` header; `commits` sends its
own `per_page`, `readme` neither. Declaring the shared default once on the scope changes three
things, which the edit has to decide sibling by sibling:

- **Every endpoint below inherits it**, also one that sent nothing: without an edit of its own,
  `readme` would start sending `per_page=30`, with no error. Keep such a sibling as it was with
  `remove`.
- **A sibling that sets the key itself no longer compiles** ("query key `per_page` is already set
  by `Forge.repo`"). `override` keeps its own value; `override .. else inherit` with an optional
  value keeps a per-call choice (`pulls().set_per_page(50)` still compiles: an optional value
  has `set_per_page` too, and `maybe_per_page` is new).
- **The query order changes.** A key keeps the position of its first appearance on the chain
  (§4.7): `per_page` now comes from the scope, so it moves ahead of the endpoint's own keys, also
  where an endpoint overrides it. The pairs are the same; a server or a request signature that
  depends on their order sees the difference. The values of one header name stay in contribution
  order: the scope's, then an endpoint's `append`.

The example below compiles both versions and checks each sibling:

```rust
use concord::reqwest;

mod before {
    concord::api! {
        pub client Forge {
            base "https://api.forge.example";
            query { "v" = "2"; }

            scope repo(owner: String, name: String) {
                path "repos/{owner}/{name}";

                GET issues(query state?: String) {
                    path "issues";
                    query { "per_page" = "30"; }
                    headers { "x-cache" = "max-age=60"; }
                }
                GET pulls(query state?: String, query per_page: u32 = 30) {
                    path "pulls";
                    headers { "x-cache" = "max-age=60"; append "x-cache" = "no-transform"; }
                }
                GET commits(query sha?: String) {
                    path "commits";
                    query { "per_page" = "100"; }
                }
                GET readme() { path "readme"; }
            }
        }
    }
}

mod after {
    concord::api! {
        pub client Forge {
            base "https://api.forge.example";
            query { "v" = "2"; }

            scope repo(owner: String, name: String) {
                path "repos/{owner}/{name}";
                query { "per_page" = "30"; }             // every endpoint below inherits it
                headers { "x-cache" = "max-age=60"; }

                GET issues(query state?: String) { path "issues"; }
                GET pulls(query state?: String, per_page?: u32) {
                    path "pulls";
                    query { override "per_page" = per_page else inherit; } // still per call
                    headers { append "x-cache" = "no-transform"; }
                }
                GET commits(query sha?: String) {
                    path "commits";
                    query { override "per_page" = "100"; } // keeps its own value
                    headers { remove "x-cache"; }
                }
                GET readme() {
                    path "readme";
                    query { remove "per_page"; }          // keeps sending neither
                    headers { remove "x-cache"; }
                }
            }
        }
    }
}

fn query(prepared: concord::Prepared<()>) -> String {
    prepared.request().url().query().unwrap_or_default().to_owned()
}

fn cache(prepared: concord::Prepared<()>) -> Vec<String> {
    let values = prepared.request().headers().get_all("x-cache");
    values.iter().map(|v| v.to_str().unwrap().to_owned()).collect()
}

fn main() -> Result<(), concord::Error> {
    let old = before::Forge::new(reqwest::Client::new()).repo("acme", "app");
    let new = after::Forge::new(reqwest::Client::new()).repo("acme", "app");

    // `issues` inherits: the same pairs, but `per_page` now comes first (the scope's position).
    assert_eq!(query(old.issues().set_state("open").prepare()?), "v=2&state=open&per_page=30");
    assert_eq!(query(new.issues().set_state("open").prepare()?), "v=2&per_page=30&state=open");
    assert_eq!(cache(new.issues().prepare()?), cache(old.issues().prepare()?));

    // `pulls`: the same calling code, the same per-call value, the same two cache lines in order.
    assert_eq!(query(old.pulls().set_per_page(50).prepare()?), "v=2&per_page=50");
    assert_eq!(query(new.pulls().set_per_page(50).prepare()?), "v=2&per_page=50");
    assert_eq!(query(new.pulls().set_state("open").prepare()?), "v=2&per_page=30&state=open");
    assert_eq!(cache(old.pulls().prepare()?), ["max-age=60", "no-transform"]);
    assert_eq!(cache(new.pulls().prepare()?), ["max-age=60", "no-transform"]);

    // `commits` keeps its own value, now at the scope's position, and no cache header.
    assert_eq!(query(old.commits().set_sha("abc").prepare()?), "v=2&sha=abc&per_page=100");
    assert_eq!(query(new.commits().set_sha("abc").prepare()?), "v=2&per_page=100&sha=abc");
    assert!(cache(new.commits().prepare()?).is_empty());

    // `readme` sends neither, as before; without its two `remove`s it would inherit both.
    assert_eq!(query(new.readme().prepare()?), query(old.readme().prepare()?));
    assert!(cache(new.readme().prepare()?).is_empty());
    Ok(())
}
```

Moving a *value* to the scope instead of a literal entry also moves its setter to the scope
handle, which changes calling code (the table above).
