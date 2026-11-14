Turns one description of an HTTP API into a typed client.

This page describes the whole language. Every example on it compiles, and those with a
`main` run against `concord::fake`, which replaces the network with a function.

# A first description

```rust
use concord::{fake, json, reply, Json, Secret};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Book { pub isbn: String, pub title: String }

#[derive(Clone, Serialize)]
pub struct NewBook { pub title: String }

concord::api! {
    /// The bookshop.
    pub client Shop(region: String, key: Secret) {
        base "https://{region}.shop.example/v2";
        headers { "x-api-key" = key; }
        read Json;

        scope books {
            path "books";

            /// One book; `None` when there is none.
            GET get(isbn: String) -> Option<Book> {
                path "{isbn}";
                status { 404 => None }
            }
            POST create(book: NewBook) -> Book {
                body json(book);
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> shop::Result<()> {
    let shop = Shop::new("eu", "the-key").with_transport(fake(|request| {
        assert_eq!(request.url().as_str(), "https://eu.shop.example/v2/books/123");
        assert_eq!(request.headers()["x-api-key"], "the-key");
        Ok(reply(200, r#"{ "isbn": "123", "title": "Kindred" }"#))
    }));
    let book = shop.books().get("123").await?;
    assert_eq!(book.unwrap().title, "Kindred");
#   fn shared<T: Send + Sync + Clone + std::fmt::Debug>(_: &T) {}
#   shared(&shop);
#   shared(&shop.books());
#   shared(&shop.books().get("1"));
    Ok(())
}
```

The signature of each node is what the caller sees; the block says how each input is sent
on the wire. `json`, `Json` and `Secret` are ordinary items of `concord`: the macro does not
define any data format, authentication scheme or pager. What follows `body`, `read`, `paginate`, `middleware`,
`idempotent` and `idle_timeout` is Rust, so you can supply your own reader, writer or pager
there. A type used in a description must be at least as visible as the client: a
`pub client` needs `pub` types.

# The structure of a description

A description holds one client. The client holds statements, then parts, then scopes and
endpoints. A scope holds statements, then scopes and endpoints. An endpoint holds
statements only.

| Node | Syntax | Generates |
|---|---|---|
| Client | `pub client Shop(region: String, key: Secret) { … }` | The type `Shop`, with `Shop::new(region, key)`, and the module `shop`. |
| Scope | `scope books { … }`, `scope repo(owner: String) { … }` | A method `shop.books()` that returns a handle. Nothing is sent. |
| Endpoint | `GET get(isbn: String) -> Book { … }`, or `…;` with no block | A method that returns a call. `.await` sends it. |
| Part | `part page(limit?: u32) { … }`, in the client's block | Nothing on its own. A node that names it (`..page`) gets its inputs and statements. |

The methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE` and `OPTIONS`;
`METHOD "PURGE"` names any other. Names are lower snake case, and a client name is upper
camel case. `///` lines become the generated documentation. Any node may also carry `#[cfg(…)]`, and a
scope or an endpoint may carry `#[deprecated]` and `#[doc(hidden)]`.

**A node sets a value, and its children inherit it.** `base`, `read` and `idle_timeout` hold
one value, and the nearest one is used. An entry of `query` or `headers`, and an arm of
`status`, has a key: a key set above can be set again only with `override`, and removed only
with `remove`.
`path` appends segments to the paths above.

# Inputs

| Syntax | Meaning |
|---|---|
| `isbn: String` | Required: an argument of the method, in the order written. |
| `year?: u16` | Optional: a setter `.year(1979)` on the call (or on the client or the handle, for one of their inputs). Not sent when it is not set. `.year(None)` unsets it. |
| `year: Option<u16>` | Required, and may be `None`. |
| `tags: Vec<String>` | A list. In a query, the key is sent once for each item. |
| `fields?: Vec<String>` | An optional list: `.fields(["name", "country"])`. Not sent when not set, nor when empty. |
| `key: Secret` | Text that is never printed. Its header is marked sensitive. |
| `..page` | The inputs of the part `page`: on the method, they come after the node's own. On the wire, its entries come before the node's own ("The order on the wire", below). |

An argument accepts the type itself, a reference to it (which is cloned), text for a
`String` or a `Secret`, and an array or a slice for a `Vec` (`["sf", "classic"]`). An input
owns its data: its type implements `Clone` and borrows nothing. A description declares no
defaults: a default belongs to the provider. `milestone?: Option<u64>` has three states: not
set, set to nothing (`.milestone(None::<u64>)`), and set to a number.

**Where an input can be used.** In its own node and in every node below it. A bare name is an input of the
node where the statement appears; an input of a node above is named with its owner:
`client.key`, `books.shelf`. An optional input cannot fill a placeholder in `base` or
`path`. Every input must be used somewhere, or the description does not compile.

A scope's input fills a placeholder in the scope's own `path`, and the endpoints below qualify
it with its owner (`shelf.id`) when they use it again:

```rust
# use concord::{fake, reply, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
scope shelf(id: String) {
    path "shelves/{id}";
    GET books() -> Vec<String> { path "books"; }
    GET label() -> String { path "label"; query { "of" = shelf.id; } }
}
# } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
# let shop = Shop::new().with_transport(fake(|request| {
#     assert_eq!(request.url().as_str(), "https://api.test/shelves/7/books");
#     Ok(reply(200, "[]"))
# }));
shop.shelf("7").books().await?;   // GET https://api.test/shelves/7/books
# Ok(())
# }
```

A **secret** is placed like any value: `headers { "authorization" = "Bearer {token}"; }`,
`query { "api_key" = key; }`, `body form { client_secret; }`. It is bound to the `base`
that applies where it is written. Under another `base` further down, the entry must be
written again (`override`) or removed, or the description does not compile. A caller's
`with_base(…)` is not such a `base`: it replaces the whole `base` (scheme, host, port and
path prefix), keeps every `path`, and sends every entry, secrets included, to the new server
just as to the old one. It is the caller's own decision.

```rust
# use concord::{fake, reply, Json, Secret};
# concord::api! { client Shop(key: Secret) { base "https://api.test/v2";
#     headers { "x-api-key" = key; } read Json; GET books() -> Vec<String> { path "books"; } } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new("k").with_base("http://books.intra:8080/shop");
# let shop = shop.with_transport(fake(|request| {
#     assert_eq!(request.url().as_str(), "http://books.intra:8080/shop/books");
#     assert_eq!(request.headers()["x-api-key"], "k");
#     Ok(reply(200, "[]"))
# }));
shop.books().await?;   // GET http://books.intra:8080/shop/books, with x-api-key
# Ok(())
# }
```

# The twelve statements

| Statement | Declares | Client | Part | Scope | Endpoint |
|---|---|---|---|---|---|
| `base` | Where the URL starts. | yes | yes | yes | yes |
| `path` | The path segments that this node adds. | yes | no | yes | yes |
| `query { }` | Query pairs, for this node and below. | yes | yes | yes | yes |
| `headers { }` | Headers, for this node and below. | yes | yes | yes | yes |
| `body` | The request body. | no | yes | no | yes |
| `read` | How the response body is read. | yes | yes | yes | yes |
| `status { }` | What a status means here. | yes | yes | yes | yes |
| `problem` | The type of the API's own error body. | yes | no | no | no |
| `paginate` | This endpoint is paginated. | no | yes | no | yes |
| `idempotent` | Whether the request may be sent twice. | no | yes | no | yes |
| `middleware` | A value that every request goes through. | yes | no | no | no |
| `idle_timeout` | How long the server may stay silent during the calls below. | yes | yes | yes | yes |

Each statement appears at most once in a node, except `middleware`. The examples below hide
the lines around each endpoint; each one compiles inside a client with
`base "https://api.test"; read Json;`.

**`base`** is a template: the scheme, the host, an optional port and an optional path prefix.
A placeholder in the host fills one label, or part of one (`"https://api-{region}.example"`).
`base url;` takes one input of type `concord::Base` (a server whose address the caller
provides).

```rust
# use concord::Json;
# concord::api! { client Shop(region: String) { read Json;
base "https://{region}.shop.example/v2";
# GET ping() -> String; } }
```

**`path`** adds path segments, joined with a single `/`. `{x}` fills one segment, or part of
one, encoded; `{*x}` fills whole segments and may contain `/`. The URL ends with `/` when the
last `path` does, or the `base` when no `path` follows.

```rust
# use concord::Json;
# concord::api! { client Shop { base "https://api.test"; read Json;
GET file(owner: String, key: String) -> String { path "files/{owner}/{*key}"; }
# } }
```

**`query { }`** and **`headers { }`** hold entries (the next section).

```rust
# use concord::Json;
# concord::api! { client Shop { base "https://api.test"; read Json;
GET search(author: String, year?: u16, tag: Vec<String>) -> Vec<String> {
    path "books";
    query { author; year; tag; "sort" = "title"; }
    headers { "accept-language" = "en"; }
}
# } }
```

**`body`** is an expression that returns a `concord::Body` (`json(x)`, `form(x)`, or
`Body::new(bytes, "image/png")`), or a body of entries: `body json { title; author; }`.
In a body of entries, a `None` writes nothing.

```rust
# use concord::{json, Body, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
POST create(title: String, author: String, year?: u16) -> String {
    body json { title; author; year; }      // {"title":…,"author":…} and "year" when set
}
PUT cover(isbn: String, png: concord::bytes::Bytes) {
    path "books/{isbn}/cover";
    body Body::new(png.clone(), "image/png");   // `png` is a `&Bytes`: clone it
}
# } }
```

**Multipart.** With the `multipart` feature of `concord`, `multipart()` starts a
`multipart/form-data` body. `.text(name, value)` and `.file(name, file_name,
content_type, bytes)` add parts in the order written. `.finish()` returns
`Result<Body, MultipartError>`, which `body` accepts as it is; an `Err` is a Build error, and
nothing is sent. On the wire, each part is a line `--boundary`, its header lines spelled
exactly as below, an empty line, then its bytes. A line `--boundary--` ends the body, and
every line ends with `\r\n`. A quote in a name is encoded as `%22`. A body held in memory uses the boundary
`concord-00000000000000000000000000000000`, or the next number when the parts contain that
text; a body with a stream part uses a random boundary.

```rust
# use concord::{fake, multipart, reply, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
POST photo(caption: String, file_name: String, jpeg: concord::bytes::Bytes) -> String {
    path "photos";
    body multipart()
        .text("caption", caption)
        .file("photo", file_name, "image/jpeg", jpeg.clone())
        .finish();
}
# } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new().with_transport(fake(|request| {
    let boundary = "concord-00000000000000000000000000000000";
    let content_type = format!("multipart/form-data; boundary={boundary}");
    assert_eq!(request.headers()["content-type"], content_type);
    let body = request.body().and_then(|body| body.as_bytes()).unwrap();
    let wire = concat!(
        "--concord-00000000000000000000000000000000\r\n",
        "Content-Disposition: form-data; name=\"caption\"\r\n",
        "\r\n",
        "The mast\r\n",
        "--concord-00000000000000000000000000000000\r\n",
        "Content-Disposition: form-data; name=\"photo\"; filename=\"mast.jpg\"\r\n",
        "Content-Type: image/jpeg\r\n",
        "\r\n",
        "<the bytes of the file>\r\n",
        "--concord-00000000000000000000000000000000--\r\n",
    );
    assert_eq!(body, wire.as_bytes());
    Ok(reply(201, r#""ph-7""#))
}));
let jpeg = concord::bytes::Bytes::from_static(b"<the bytes of the file>");
shop.photo("The mast", "mast.jpg", jpeg).await?;
# Ok(())
# }
```

**`read`** names a reader: `Json`, `Text`, `Binary` or `concord::read::Stream`, or your
own. There is no default format: a client declares `read Json;` once, and an endpoint
that differs declares its own.

```rust
# use concord::{Json, Text, ByteStream};
# concord::api! { client Shop { base "https://api.test"; read Json;
GET readme() -> String { path "README"; read Text; }
GET export() -> ByteStream { path "export"; read concord::read::Stream; }
# } }
```

**`status { }`** holds arms (see "Status arms" below).

```rust
# use concord::Json;
# concord::api! { client Shop { base "https://api.test"; read Json;
HEAD exists(isbn: String) -> bool { path "books/{isbn}"; status { 2xx => true, 404 => false } }
# } }
```

**`problem`**, on the client, names the type of the API's own error body: a type that the
reader can read (`Deserialize`, for `Json`). `e.problem()` returns it for every call that
ends in a Refused error. The client's reader reads it, unless the line names another
reader: `problem S3Problem read Xml;`.

```rust
# use concord::Json;
# #[derive(Debug, serde::Deserialize)] struct Problem { code: String }
# concord::api! { client Shop { base "https://api.test"; read Json;
problem Problem;
# GET ping() -> String; } }
```

**`paginate`** adds `.pages()` to the call. Inside it, an input refers to its *slot* on
the call, not to its value. `concord` ships the pagers `by_offset`, `by_offset_limit`, `by_page`,
`by_cursor`, `by_link` and `by_next_url`; your own closure works too.

```rust
# use concord::{by_cursor, Json};
# #[derive(serde::Deserialize)] struct Page { items: Vec<String>, next: Option<String> }
# concord::api! { client Shop { base "https://api.test"; read Json;
GET list(cursor?: String) -> Page {
    query { cursor; }
    paginate by_cursor(cursor, |page: &Page| page.next.clone());
}
# } }
```

With `paginate by_offset_limit(offset, limit);` the first request carries what the caller
set and nothing more: without `.offset(…)` it has no `offset` pair (the offset counts as 0),
and without `.limit(…)` no `limit` pair. Each following request carries `offset` increased by
the number of items received. The pages end with an empty page, or with a page shorter than the
`limit` that the caller set; when no `limit` is set, only an empty page ends them.

**`idempotent`** marks an endpoint whose request may be sent twice. `GET`, `HEAD`,
`OPTIONS`, `PUT` and `DELETE` already are; `idempotent false;` removes the mark. Its value
is any `bool` expression.

```rust
# use concord::{json, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
POST order(key: String) -> String { headers { "idempotency-key" = key; } idempotent; }
# } }
```

**`middleware`** declares a value that implements `concord::Middleware`, such as a signer
or a token that is renewed. It appears on the client, as often as needed, and each value is
built once, in `new`, from the client's required inputs.

```rust
# use concord::{Json, Middleware, Next, Secret};
# use concord::transport::{TransportError, TryInfo};
# struct Signed(Secret);
# impl Middleware for Signed {
#     async fn handle(&self, request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
#         -> Result<concord::reqwest::Response, TransportError> { next.send(request, info).await }
# }
# concord::api! { client Vault(key: Secret) { base "https://api.test"; read Json;
middleware Signed(key.clone());
# GET ping() -> String; } }
```

**`idle_timeout`** sets the idle timeout of the calls below, for example for a long poll.
Its value is a `std::time::Duration` expression.

```rust
# use concord::Json;
# use std::time::Duration;
# concord::api! { client Shop { base "https://api.test"; read Json;
GET updates(wait?: u64) -> Vec<String> {
    query { wait; }
    idle_timeout Duration::from_secs(wait.unwrap_or(0) + 10);
}
# } }
```

**`part`** is a named group of inputs with the statements that use them. It is declared in
the client, after the client's statements. It cannot hold `path`, `problem`, `middleware` or
a child.

```rust
# use concord::{by_page, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
part paged(page?: u32, per_page?: u32) {
    query { page; per_page; }
    paginate by_page(page, 1);
}
GET authors(..paged) -> Vec<String> { path "authors"; }
GET titles(author: String, ..paged) -> Vec<String> { path "titles"; query { author; } }
# } }
```

# Entries, `override` and `remove`

An entry of `query`, `headers` or a body of entries is one of these:

| Syntax | Meaning |
|---|---|
| `year;` | The key `year`, with the value of this node's input `year`. |
| `"startTime" = start_time;` | The key as written; the value is an input. |
| `"v" = "2";`, `"x-version" = "v{version}";` | Fixed text, or a template with placeholders. |
| `"labels" = labels.join(",");` | Any Rust expression. |
| `override "k" = …;` | Replace an entry set above (not in a body). |
| `remove "k";` | Stop sending an entry set above (not in a body). |

The value is serialized by `serde`: a list repeats its key, and `None` writes nothing. An
entry is not sent when a placeholder of its template reads a `None`. In `headers`, `user_agent;` is rejected:
write `"user-agent" = user_agent;`. `host` cannot be a key, because the host comes from the
`base`.

```rust
# use concord::{fake, reply, Json, Secret};
concord::api! {
    client Shop(key: Secret) {
        base "https://api.test";
        headers { "x-api-key" = key; "x-trace" = "on"; }
        read Json;

        GET public() -> Vec<String> {
            headers { remove "x-api-key"; override "x-trace" = "off"; }
        }
    }
}
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new("k").with_transport(fake(|request| {
    assert!(request.headers().get("x-api-key").is_none());
    assert_eq!(request.headers()["x-trace"], "off");
    Ok(reply(200, "[]"))
}));
shop.public().await?;
# Ok(())
# }
```

**The order on the wire.** Query pairs and headers are sent in the order of the tree:
the client's, then each scope's from the top down, then the endpoint's. At each node, the
entries of the parts it names (in the order it names them) come before its own, and within
one block, entries keep the order in which they are written. An `override` keeps the
position of the entry it replaces.

```rust
# use concord::{fake, reply, Json};
concord::api! {
    client Shop {
        base "https://api.test";
        query { "a" = "client"; }
        headers { "x-a" = "client"; }
        read Json;

        part view(lang?: String) {
            query { lang; }
            headers { "x-c" = "part"; }
        }

        scope books {
            path "books";
            query { "b" = "scope"; }
            headers { "x-b" = "scope"; }

            GET list(author: String, ..view) -> Vec<String> {
                query { author; }
                headers { "x-d" = "endpoint"; }
            }
        }
    }
}
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new().with_transport(fake(|request| {
    assert_eq!(request.url().query(), Some("a=client&b=scope&lang=fr&author=Butler"));
    let names: Vec<&str> = request.headers().keys().map(|name| name.as_str()).collect();
    assert_eq!(names, ["x-a", "x-b", "x-c", "x-d"]);
    Ok(reply(200, "[]"))
}));
shop.books().list("Butler").lang("fr").await?;
# Ok(())
# }
```

# Status arms

A 2xx is a success: the nearest reader reads its body into the type after `->`.
Any other status is a Refused error. An arm handles a status itself, and a status that an
arm matches is not read by this rule (a `HEAD` reads nothing: see below):

| Syntax | Meaning |
|---|---|
| `404 => None` | An exact status, 100 to 599. Nothing is read. |
| `4xx { status } => …` | A class, `1xx` to `5xx`. An exact status takes precedence over a class. |
| `200 { body } => Some(body)` | `body` is read by the nearest reader. Rust infers its type. |
| `201 { body, headers } => …` | `headers` is a `&concord::Headers`; `status` an `http::StatusCode`. |
| `200 { body: book } => …` | The same field under another name. A field that is not named is not available. |
| `override 404 => …`, `remove 404` | Replace or remove an arm set above. |

**After the arrow, `Option<T>` and `Result<T, E>` are read as `T`.** A 2xx that no arm
matches is read as a `T` and returned as `Some` or `Ok`. The arms produce the rest:
`404 => None`, `409 { body } => Err(body)`. So a 200 needs no arm of its own.

```rust
# use concord::Json;
# #[derive(serde::Deserialize)] struct Book { title: String }
# #[derive(serde::Deserialize)] struct Taken { by: String }
# concord::api! { client Shop { base "https://api.test"; read Json;
GET find(isbn: String) -> Option<Book> { path "books/{isbn}"; status { 404 => None } }
PUT hold(isbn: String) -> Result<Book, Taken> {
    path "books/{isbn}/hold";
    status { 409 { body } => Err(body) }
}
# } }
```

**An endpoint with no arrow returns `()`.** The body of a 2xx is not read:
`DELETE delete(isbn: String);` accepts a 204, or any 2xx. With an arrow, an arm produces a
value for an empty response without reading it: `204 => None`. There is no reader for "nothing".

**A `HEAD` with an arrow reads nothing,** so it needs no reader, and its arms produce every
value. It must have an arm for a 2xx (an exact status, or the class `2xx`), or it does not
compile. A status that no arm matches is not read either: a 2xx is a Mismatch (`no arm takes
this status`), any other status a Refused error. So `status { 200 => true, 404 => false }`
makes a `204` a Mismatch; `2xx => true` matches every 2xx.

```rust
# use concord::{fake, reply, Json};
# use concord::error::Kind;
# concord::api! { client Shop { base "https://api.test"; read Json;
HEAD exists(isbn: String) -> bool { path "books/{isbn}"; status { 200 => true, 404 => false } }
# } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let answers = std::sync::Mutex::new(vec![500, 204, 404, 200]);
let shop = Shop::new().with_transport(fake(move |_| {
    Ok(reply(answers.lock().unwrap().pop().unwrap(), ""))
}));
assert!(shop.exists("1").await?);                                // 200
assert!(!shop.exists("2").await?);                               // 404
assert_eq!(shop.exists("3").await.unwrap_err().kind(), Kind::Mismatch);   // 204
assert_eq!(shop.exists("4").await.unwrap_err().kind(), Kind::Refused);    // 500
# Ok(())
# }
```

**A body and a header together.** Bind both in one arm. `headers.require("location")?`
ends the call with a Mismatch when the header is missing; `?` on the API's problem ends it
with a Refused error. Match the whole class `2xx`: with an arm for `201` alone, any other 2xx
would be read into `Created` itself.

```rust
# use concord::{fake, json, reply_with, Json};
# use serde::Deserialize;
#[derive(Debug, Deserialize)]
struct Book { title: String }

#[derive(Debug)]
struct Created { book: Book, location: String }

concord::api! {
    client Shop {
        base "https://api.test";
        read Json;

        POST create(title: String) -> Created {
            path "books";
            body json { title; }
            status {
                2xx { body, headers } => Created {
                    book: body,
                    location: headers.require("location")?.to_owned(),
                },
            }
        }
    }
}
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new().with_transport(fake(|_| {
    Ok(reply_with(201, &[("location", "/books/7")], r#"{ "title": "Kindred" }"#))
}));
let created = shop.create("Kindred").await?;
assert_eq!((created.book.title.as_str(), created.location.as_str()), ("Kindred", "/books/7"));
# Ok(())
# }
```

Your own function may build the value: it returns `Result<T, concord::Fail<P>>`, where
`P` is the problem type, and the arm calls `created(body, headers)?`. An arm cannot be
`async`.

# Names in an expression

Wherever you write Rust (an entry, `body`, an arm, `idempotent`, `idle_timeout`), an input
is a variable that **borrows** the input: `&T` for `title: T`, and `&Option<T>` for
`year?: T`. This is why the examples write `body Body::new(png.clone(), …)`, and call
`wait.unwrap_or(0)` on a `&Option<u64>`. `client.key` is a reference too. There are two
exceptions: in `paginate` an input refers to its slot on the call, and in `middleware` only
the client's required inputs are available. A bare name
that is not an input resolves to whatever Rust finds in your module, and an input shadows an
item with the same name: refer to that item by its path (`body concord::form(form);`).

`?` in an entry or a `body` ends the call with a Build error. In an arm, it ends the call
with the API's problem, or with a Mismatch on a `concord::Fail`. It does not compile in
`paginate`, `idempotent`, `idle_timeout` and `middleware`.

# Settings that are not in a description

A description holds facts about the API. The caller's choices are settings, not statements:
there is no `timeout`, `retry` or `override read` statement.

| What | Where |
|---|---|
| A timeout for the whole call | `with_timeout(Duration)` on the client or on a call. No limit unless set. |
| Retries | `with_retry(Retry::standard())` on the client or on a call. Off until set. A call's policy replaces the client's. An author who knows the API's own retry rule ships it as a function next to the description (`pub fn retry() -> shop::Retry`), and the caller passes it: `with_retry(retry())`. See the documentation of `concord::Retry` for a worked example. |
| The idle timeout | `with_idle_timeout(…)`; 30 seconds by default. The description's `idle_timeout` takes precedence over the client's, and the call's over both. |
| Another server | `with_base(…)` on the client. |
| What actually sends the request | `with_transport(…)`: `concord::Http`, a `reqwest::Client`, `concord::fake(…)`. |
| The caller's own middleware | `with_middleware(…)`; it runs before the description's. |
| The request as built | `request()`, `edit_request(…)` on a call. |
| The status and headers alongside the value | `send_full()` returns a `concord::Answer`. |
| Another reader for one endpoint | `read` on that endpoint: the nearest one is used. `override` is for keys only. |

Defaults for every call are set once, on the client:

```rust
# use concord::{fake, reply, Json, Retry};
# use std::time::Duration;
# concord::api! { client Shop { base "https://api.test"; read Json;
#     GET list() -> Vec<String>;
#     PUT upload(data: String) { body concord::Body::new(data.clone(), "text/plain"); } } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new()
    .with_timeout(Duration::from_secs(5))
    .with_retry(Retry::standard());
# let shop = shop.with_transport(fake(|_| Ok(reply(200, "[]"))));
# shop.list().await?;
shop.upload("big").with_timeout(Duration::from_secs(60)).await?;   // this call only
# Ok(())
# }
```

# What the macro generates

For `pub client Shop`, with a scope `books` and an endpoint `get` in it:

| Item | What it is |
|---|---|
| `Shop` | The client: `Send + Sync + Clone + Debug`. Clones share the transport and the middleware. |
| `Shop::new(…)` | Takes the required inputs of the client, in order. |
| `with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware` | The client's settings. Each returns the changed client. |
| `shop.books()` | A method for each child. A setter for each optional input of the client. |
| `shop::Books` | The handle of the scope. |
| `shop::books::Get` | The call. `.await` (or `send()`) sends it; `send_full()`, `send_raw()`, `request()`, `into_request()`, `edit_request(…)`, `read_response(…)`, `with_retry`, `with_timeout`, `with_idle_timeout`; `send_if_none_match(etag)` on a `GET` or `HEAD`; with `paginate`, `pages()` and no `.await`. Calls are `Send + Sync + Clone`. |
| `shop::Error` | `concord::Error<P>`, where `P` is the `problem` type (`()` without one). Every endpoint returns this error type. |
| `shop::Result<T>` | `Result<T, shop::Error>`. |
| `shop::Retry` | `concord::Retry<P>`, the policy type that `with_retry` accepts. It is only the type: it holds none of the author's retry rules. |

See the module `concord::sample` for a generated client shown in full.

**A conditional `GET`.** `send_if_none_match(etag)` sends `If-None-Match` with the text exactly
as given: pass the ETag as it arrived, quotes included (`"\"v1\""`, `W/"v1"`). It returns a
`concord::Answer<Option<T>>`: `None` for a `304` that no arm matches, `Some(value)` for any
other response, and the headers in both cases, so you also get the next ETag.

```rust
# use concord::{fake, reply_with, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
#     GET book(isbn: String) -> String { path "books/{isbn}"; } } }
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> shop::Result<()> {
let shop = Shop::new().with_transport(fake(|request| {
    assert_eq!(request.headers()["if-none-match"], "\"v1\"");
    Ok(reply_with(304, &[("etag", "\"v1\"")], ""))
}));
let answer = shop.book("123").send_if_none_match("\"v1\"").await?;
assert_eq!(answer.status(), 304);
assert_eq!(answer.value(), &None);                         // unchanged
assert_eq!(answer.headers().get("etag"), Some("\"v1\""));
# Ok(())
# }
```

**Names that begin with `__` are reserved for Concord.** The generated code uses them for its own
methods, fields and types. In a module that contains `concord::api!`, do not declare any
item whose name begins with `__`.

**Names that you may not use.** Every name of an input, a scope, an endpoint or a part
is lower snake case: a letter `a`–`z` first, then `a`–`z`, `0`–`9` and `_`, with no `__`
and no trailing `_`. It cannot be a Rust keyword, strict or reserved (`type`, `ref`, `try`,
`gen`, `async` among them), or a raw identifier (`r#type`). An input cannot be named
`client`, nor take the name of a scope that encloses it (the scope that declares it included),
nor the name of another input of its node. Those are the only rules for a required input,
because it is an argument. An optional input becomes a setter on its receiver, and a child becomes a method on it, so
neither may take the name of a method that the receiver already has:

| Receiver | Its children and its optional inputs may not be named |
|---|---|
| Any | `clone`, and any name that starts with `with_` |
| The client | `new`, `call`; and a child of the client also not `error`, `result` or `retry` (`shop::Error`, `shop::Result`, `shop::Retry`) |
| A scope | `call` |
| An endpoint (optional inputs) | `send`, `send_full`, `send_raw`, `send_if_none_match`, `request`, `into_request`, `edit_request`, `read_response`, `pages`, `at_url`, `into_future` |

An optional input and a child of one node may not share a name. An input of a part is
checked at each node that names the part. Every other name is allowed, including the keywords of the
language itself:

```rust
# use concord::Json;
# concord::api! { client Shop { base "https://api.test"; read Json;
GET search(from?: String, to?: String, query?: String, path?: String, body?: String,
           status?: u16, read?: bool, call: String) -> Vec<String> {
    query { from; to; query; path; body; status; read; call; }
}
# } }
```

# When a description does not compile

The macro reports all its own messages in one build, and each one says what to write. A
mistake inside your own Rust (a type that is not `Deserialize`, a reader that does not fit) is
reported by `rustc` once those are fixed, sometimes more than once for a single mistake.
