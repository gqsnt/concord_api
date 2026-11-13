Turns one description of an HTTP API into a typed client.

This page is the whole language. Every example on it compiles, and the ones with a
`main` run against `concord::fake`, a function in place of the network.

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

The signature of each node is for the caller. The block says where each input goes on
the wire. `json`, `Json` and `Secret` are ordinary items of `concord`: the macro knows no
format, no scheme and no pager. What stands after `body`, `read`, `paginate`, `middleware`,
`idempotent` and `idle_timeout` is Rust, and a reader, writer or pager of yours takes the
same place. A type in a description is at least as visible as the client: a `pub client`
takes `pub` types.

# The shape

A description holds one client. The client holds statements, then parts, then scopes and
endpoints. A scope holds statements, then scopes and endpoints. An endpoint holds
statements only.

| Node | Written | Gives |
|---|---|---|
| Client | `pub client Shop(region: String, key: Secret) { … }` | The type `Shop`, with `Shop::new(region, key)`, and the module `shop`. |
| Scope | `scope books { … }`, `scope repo(owner: String) { … }` | A method `shop.books()` that returns a handle. Nothing is sent. |
| Endpoint | `GET get(isbn: String) -> Book { … }`, or `…;` with no block | A method that returns a call. `.await` sends it. |
| Part | `part page(limit?: u32) { … }`, in the client's block | Nothing by itself. A node that names it (`..page`) has its inputs and statements. |

The methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, and
`METHOD "PURGE"` for another one. A name is lower snake case; a client name is upper camel
case. `///` lines go to the generated pages. A node may also take `#[cfg(…)]`; a scope or
an endpoint `#[deprecated]` and `#[doc(hidden)]`.

**A node sets, a child inherits.** `base`, `read` and `idle_timeout` hold one value: the
nearest one is used. An entry of `query` or `headers`, and an arm of `status`, has a key:
a key set above is set again only with `override`, and taken away only with `remove`.
`path` only adds.

# Inputs

| Written | Meaning |
|---|---|
| `isbn: String` | Required: an argument of the method, in the order written. |
| `year?: u16` | Optional: a method `.year(1979)` on the call (or on the client or the handle, for an input of theirs). Not sent when it was not set. `.year(None)` un-sets it. |
| `year: Option<u16>` | Required, and may be `None`. |
| `tags: Vec<String>` | A list. In a query, the key is sent once for each item. |
| `fields?: Vec<String>` | An optional list: `.fields(["name", "country"])`. Not sent when not set, nor when empty. |
| `key: Secret` | Text that is never printed. Its header is marked sensitive. |
| `..page` | The inputs of the part `page`: on the method, after the node's own. On the wire, its entries come before the node's own ("The order on the wire", below). |

An argument takes the type, a reference to it (it is cloned), text for a `String` or a
`Secret`, and an array or a slice for a `Vec` (`["sf", "classic"]`). An input owns its
data, so its type is `Clone` and borrows nothing. There is no written default: a default
belongs to the provider. `milestone?: Option<u64>` has three states: not set, set to
nothing (`.milestone(None::<u64>)`), set to a number.

**Where an input can be used.** At its own node and below. A bare name is an input of the
node that writes the statement; an input of a node above is named with its owner:
`client.key`, `books.shelf`. An optional input cannot fill a hole of `base` or `path`.
Every input is used somewhere, or the description does not compile.

A scope's input fills a hole of the scope's own `path`, and the endpoints below name it
with its owner when they use it again:

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
in reach where it is written. Under another `base` further down, the entry is written
again (`override`) or removed, or the description does not compile. A caller's
`with_base(…)` is not such a `base`: it replaces the whole `base` (scheme, host, port and
first path), keeps every `path`, and sends every entry, secrets included, to the new server
as to the old one. It is the caller's own word.

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

| Statement | Says | Client | Part | Scope | Endpoint |
|---|---|---|---|---|---|
| `base` | Where the URL starts. | yes | yes | yes | yes |
| `path` | The piece of path that this node adds. | yes | no | yes | yes |
| `query { }` | Query pairs, for this node and below. | yes | yes | yes | yes |
| `headers { }` | Headers, for this node and below. | yes | yes | yes | yes |
| `body` | The request body. | no | yes | no | yes |
| `read` | How the body of an answer is read. | yes | yes | yes | yes |
| `status { }` | What a status means here. | yes | yes | yes | yes |
| `problem` | The type of the API's own error body. | yes | no | no | no |
| `paginate` | This endpoint has pages. | no | yes | no | yes |
| `idempotent` | Whether the request may be sent twice. | no | yes | no | yes |
| `middleware` | What every request goes through. | yes | no | no | no |
| `idle_timeout` | How long the calls below may be silent. | yes | yes | yes | yes |

Each is written at most once in a node, except `middleware`. In the examples below, the
lines around an endpoint are hidden; each one compiles inside a client with
`base "https://api.test"; read Json;`.

**`base`** is a template: the scheme, the host, maybe a port and a first path. A hole in the
host is one label. `base url;` takes one input of type `concord::Base` (a server that the
user hosts).

```rust
# use concord::Json;
# concord::api! { client Shop(region: String) { read Json;
base "https://{region}.shop.example/v2";
# GET ping() -> String; } }
```

**`path`** adds pieces, joined with one `/`. `{x}` is one segment, encoded; `{*x}` may
hold `/`. The URL ends with `/` when the last piece does.

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

**`body`** is an expression that gives a `concord::Body` (`json(x)`, `form(x)`, or
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

**Multipart.** With the feature `multipart` of `concord`, `multipart()` starts a
`multipart/form-data` body: `.text(name, value)` and `.file(name, file_name,
content_type, bytes)` add parts in the order written, and `.finish()` gives
`Result<Body, MultipartError>`, which `body` takes as it is (an `Err` is a Build error, and
nothing is sent). On the wire each part is a line `--boundary`, its header lines spelled
exactly as below, an empty line, then its bytes; a line `--boundary--` ends the body, and
every line ends with `\r\n`. A quote in a name is written `%22`. A body held in memory takes the boundary
`concord-00000000000000000000000000000000` (the next number when the parts hold that text);
a body with a stream part takes a random one.

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

**`read`** names a reader: `Json`, `Text`, `Binary` or `concord::read::Stream`, or one of
yours. There is no format by default: a client says `read Json;` once, and an endpoint
that differs says its own.

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

**`problem`** names the type of the API's own error body, on the client: a type that the
reader reads (`Deserialize`, for `Json`). `e.problem()` gives it, for every refused call.
It is read by the client's reader, or by the one on its line: `problem S3Problem read Xml;`.

```rust
# use concord::Json;
# #[derive(Debug, serde::Deserialize)] struct Problem { code: String }
# concord::api! { client Shop { base "https://api.test"; read Json;
problem Problem;
# GET ping() -> String; } }
```

**`paginate`** gives the call `.pages()`. In it, an input names its *place* on the call,
not its value. The pagers of `concord` are `by_offset`, `by_offset_limit`, `by_page`,
`by_cursor`, `by_link` and `by_next_url`; a closure of yours fits too.

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
set, and nothing more: with no `.offset(…)` it has no `offset` pair (it counts as 0), with no
`.limit(…)` no `limit` pair. Each next request carries `offset` plus the number of items
that came. The pages end with an empty page, or with a page shorter than the `limit` that
the caller set; with no `limit` set, only an empty page ends them.

**`idempotent`** marks an endpoint that may be sent twice. `GET`, `HEAD`, `OPTIONS`,
`PUT` and `DELETE` are so already; `idempotent false;` takes it away. It is any `bool`
expression.

```rust
# use concord::{json, Json};
# concord::api! { client Shop { base "https://api.test"; read Json;
POST order(key: String) -> String { headers { "idempotency-key" = key; } idempotent; }
# } }
```

**`middleware`** stands on the client, as often as needed: a value with
`concord::Middleware` (a signer, a token that is renewed). It is made once, in `new`,
from the required inputs of the client.

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

**`idle_timeout`** sets the clock for silence of the calls below, for a long poll. It is a
`std::time::Duration` expression.

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

**`part`** is a named group of inputs with the statements that place them. It is written in
the client, after its statements. It holds no `path`, `problem`, `middleware` or child.

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

| Written | Meaning |
|---|---|
| `year;` | The key `year`, the input `year` of this node. |
| `"startTime" = start_time;` | The key as written; the value is an input. |
| `"v" = "2";`, `"x-version" = "v{version}";` | Fixed text, or a template with holes. |
| `"labels" = labels.join(",");` | Any Rust expression. |
| `override "k" = …;` | Replace an entry set above (not in a body). |
| `remove "k";` | Stop sending an entry set above (not in a body). |

The value is written by `serde`: a list repeats its key, and `None` writes nothing. A
template that reads a `None` is not sent. In `headers`, `user_agent;` is refused: write
`"user-agent" = user_agent;`. `host` is not a key; the host is the `base`.

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

**The order on the wire.** Query pairs and headers go out in the order of the tree: the
client's, then each scope's from the top down, then the endpoint's; at each node, the
entries of the parts it names (in the order it names them) come before its own, and inside
one block, the order written. An `override` keeps the place of the entry it replaces.

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

A 2xx is the success: its body is read into the type after `->` by the nearest reader.
Any other status is a Refused error. An arm gives a meaning to a status, and a status that
an arm takes is not read by that rule (a `HEAD` reads nothing: see below):

| Written | Meaning |
|---|---|
| `404 => None` | An exact status, 100 to 599. Nothing is read. |
| `4xx { status } => …` | A class, `1xx` to `5xx`. An exact status wins over a class. |
| `200 { body } => Some(body)` | `body` is read by the reader in reach. Its type is found by Rust. |
| `201 { body, headers } => …` | `headers` is a `&concord::Headers`; `status` an `http::StatusCode`. |
| `200 { body: book } => …` | The same field under another name. A field not named is not there. |
| `override 404 => …`, `remove 404` | Replace or take away an arm from above. |

**After the arrow, `Option<T>` and `Result<T, E>` are read as `T`.** A 2xx that no arm
takes is read as a `T` and given as `Some` or `Ok`. The arms give the rest:
`404 => None`, `409 { body } => Err(body)`. So the 200 needs no arm.

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

**No arrow gives `()`.** The body of a 2xx is not read: `DELETE delete(isbn: String);`
takes a 204, or any 2xx. Under an arrow, an arm gives a value for an empty answer without
reading it: `204 => None`. There is no reader for "nothing".

**A `HEAD` with an arrow reads nothing,** so it needs no reader, and its arms give every
value. It writes an arm for a 2xx (an exact status, or the class `2xx`), or it does not
compile. A status that no arm takes is not read either: a 2xx is a Mismatch (`no arm takes
this status`), any other status a Refused error. So `status { 200 => true, 404 => false }`
makes a `204` a Mismatch; `2xx => true` takes every 2xx.

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
as Refused. Take the class `2xx`, or every other 2xx is read into `Created` itself.

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

A function of yours may build the value: it returns `Result<T, concord::Fail<P>>`, where
`P` is the problem type, and the arm writes `created(body, headers)?`. An arm is not
`async`.

# What an expression sees

Wherever Rust is written (an entry, `body`, an arm, `idempotent`, `idle_timeout`), an input
is a variable that **borrows** the input: `&T` for `title: T`, and `&Option<T>` for
`year?: T`. So `body Body::new(png.clone(), …)`, and `wait.unwrap_or(0)` on a `&Option<u64>`.
`client.key` is a reference too. Two places differ: in `paginate` an input names its place
on the call; in `middleware` only the required inputs of the client are there. A bare name
that is not an input is what Rust finds in your module, and an input hides an item of its
name: write that item with its path (`body concord::form(form);`).

`?` in an entry or a `body` ends the call with a Build error. In an arm, it ends the call
with the API's problem, or with a Mismatch on a `concord::Fail`. It does not compile in
`paginate`, `idempotent`, `idle_timeout` and `middleware`.

# What is not in a description

A description holds facts of the API. Choices of the caller are settings, not statements:
there is no `timeout`, `retry` or `override read` statement.

| What | Where |
|---|---|
| A timeout for the whole call | `with_timeout(Duration)` on the client or on a call. No limit until set. |
| Retries | `with_retry(Retry::standard())` on the client or on a call. Off until set. A call's policy replaces the client's. The author of a client who knows the API's own rule ships it as a function beside the macro (`pub fn retry() -> shop::Retry`), and the caller passes it: `with_retry(retry())`. A worked example: the page of `concord::Retry`. |
| The clock for silence | `with_idle_timeout(…)`; 30 seconds by default. The description's `idle_timeout` comes between the call's and the client's. |
| Another server | `with_base(…)` on the client. |
| What really sends | `with_transport(…)`: `concord::Http`, a `reqwest::Client`, `concord::fake(…)`. |
| A middleware of the caller | `with_middleware(…)`; it runs before the description's. |
| The request as built | `request()`, `edit_request(…)` on a call. |
| The status and headers beside the value | `send_full()` gives a `concord::Answer`. |
| Another reader for one endpoint | `read` on that endpoint: the nearest one is used. `override` is for keys only. |

Defaults for every call go on the client once:

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

# What the macro writes

For `pub client Shop`, with a scope `books` and an endpoint `get` in it:

| Item | What it is |
|---|---|
| `Shop` | The client: `Send + Sync + Clone + Debug`. Clones share the transport and the middlewares. |
| `Shop::new(…)` | Takes the required inputs of the client, in order. |
| `with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware` | The settings of the client. Each returns the client, changed. |
| `shop.books()` | A method for each child. A setter for each optional input of the client. |
| `shop::Books` | The handle of the scope. |
| `shop::books::Get` | The call. `.await` (or `send()`) sends it; `send_full()`, `send_raw()`, `request()`, `into_request()`, `edit_request(…)`, `read_response(…)`, `with_retry`, `with_timeout`, `with_idle_timeout`; `send_if_none_match(etag)` on a `GET` or `HEAD`; with `paginate`, `pages()` and no `.await`. Calls are `Send + Sync + Clone`. |
| `shop::Error` | `concord::Error<P>`, `P` the `problem` type (`()` without one). Every endpoint fails with it. |
| `shop::Result<T>` | `Result<T, shop::Error>`. |
| `shop::Retry` | `concord::Retry<P>`, the policy that `with_retry` takes. Only the type: it holds no policy of the author. |

A generated client is shown in full on the page of the module `concord::sample`.

**A conditional `GET`.** `send_if_none_match(etag)` writes `If-None-Match` with the text as
given: pass the ETag as it arrived, quotes included (`"\"v1\""`, `W/"v1"`). It gives a
`concord::Answer<Option<T>>`: `None` for a `304` that no arm takes, `Some(value)` for any
other answer, and the headers in both, so the next ETag.

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

**Names that begin with `__` are Concord's.** The generated code uses them for its own
methods, fields and types. In a module that holds `concord::api!`, declare no item whose
name begins with `__`.

**The names an input may not have.** Every name of an input, a scope, an endpoint or a part
is lower snake case: a letter `a`–`z` first, then `a`–`z`, `0`–`9` and `_`, with no `__`
and no `_` at the end. It is no keyword of Rust, strict or reserved (`type`, `ref`, `try`,
`gen`, `async` among them), and no raw identifier (`r#type`). An input is not named
`client`, nor like a scope that holds it (the scope itself included), nor like another
input of its node. Those are the only rules for a required input: it is an argument. An
optional input is a method of its receiver, as a child is, so neither may take a name of
that receiver's methods:

| Receiver | Its children and its optional inputs may not be named |
|---|---|
| Any | `clone`, and any name that starts with `with_` |
| The client | `new`, `call`; and a child of the client not `error`, `result` or `retry` (`shop::Error`, `shop::Result`, `shop::Retry`) |
| A scope | `call` |
| An endpoint (optional inputs) | `send`, `send_full`, `send_raw`, `send_if_none_match`, `request`, `into_request`, `edit_request`, `read_response`, `pages`, `at_url`, `into_future` |

An optional input and a child of one node may not share a name. An input of a part is
checked at each node that names it. Every other name is free, the words of the language
too:

```rust
# use concord::Json;
# concord::api! { client Shop { base "https://api.test"; read Json;
GET search(from?: String, to?: String, query?: String, path?: String, body?: String,
           status?: u16, read?: bool, call: String) -> Vec<String> {
    query { from; to; query; path; body; status; read; call; }
}
# } }
```

# When a description is wrong

The macro's own messages say what to write, all of them in one build. A mistake inside
your Rust (a type that is not `Deserialize`, a reader that does not fit) is `rustc`'s, and
comes after those are fixed, maybe more than once for one mistake.
