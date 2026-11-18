# Send a body

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Read responses and errors](responses-and-errors.md) · next: [Credentials](credentials.md)*

This page shows how the description builds the body of a request: the two forms of `body`, the writers that Concord ships, a body that is opened again for each attempt, and a body that is a stream.

## A body that is sent

The `body` statement has two forms. In `body <expression>;`, the expression is ordinary Rust in which the inputs are variables, and it evaluates to a `concord::Body`. The other form, `body json { … }`, builds the body from entries, for a body that consists of the endpoint's inputs.

| Written | Sent as |
|---|---|
| `body json(task);` | JSON, with `Content-Type: application/json` |
| `body form(task);` | A form, with `Content-Type: application/x-www-form-urlencoded` |
| `body file;` | An input that is already a `concord::Body`: its bytes, unchanged |
| `body json { query; cursor; "limit" = 50; }` | JSON built from entries, as in `query`: each input is a key (below) |
| `body multipart().text("chat_id", chat_id).file("photo", "a.jpg", "image/jpeg", photo).finish();` | A multipart form, with files (the `multipart` feature; below) |
| `body xml(task);` | Whatever your own function returns |

`json` and `form` are plain functions of `concord`, imported with `use concord::{json, form};`. `multipart` is one more, behind the `multipart` feature. The macro treats none of them specially, and your own function works the same way:

```rust
// plain Rust, beside the macro
pub fn xml<T: Serialize>(value: &T) -> Result<concord::Body, quick_xml::SeError> {
    Ok(concord::Body::new(quick_xml::se::to_string(value)?, "application/xml"))
}
```

A custom writer that combines bodies has three more tools: `Body::concat(parts)` returns one body made of several, in order; `body.as_bytes()` returns the bytes of a body held in memory; and `body.into_stream()` turns any body into a stream. A one-shot stream inside a combined body is taken along with the whole body when the request is built for sending, so a stream that was already sent is a Build error before anything is sent ([contracts](../reference/contracts.md) C2.15).

**A body made of entries.** In many APIs, the body of a `POST` consists of its inputs. The body is then written like a `query`:

```rust
part formatted(parse_mode?: ParseMode, disable_notification?: bool) {
    body json { parse_mode; disable_notification; }
}
POST send_message(chat_id: ChatId, text: String, ..formatted) -> Message {
    path "sendMessage";
    body json { chat_id; text; "reply_markup" = markup_of(text); }
}
```

| | |
|---|---|
| An entry | As in `query`: `name;` or `"key" = value;`. A template as a value (`"note" = "by {name}";`) works as it does there: its text is sent, a placeholder that reads nothing leaves the entry out, and a placeholder that reads several values is a Build error. There is no `override` and no `remove` here. |
| What is serialized | The entries form one value, `concord::body::Entries`, which `serde` serializes as a map, in the order written. The function named before the block is called with it: `body json { a; b; }` means `body json(entries)`. So `form`, or your own function that takes anything `serde` can serialize, works too, and the macro still has no built-in format. |
| The name before the block | A path to a function, resolved from where `api!` is invoked. An input with the same name does not shadow it here. (In an expression it does: `body concord::form(form);`.) |
| A part | It may contain `body json { … }`. The node that names it then writes `body json { … }` too, or no body at all: the entries are merged, the parts' entries first, in the order in which the node names the parts, then the node's own. A different function, the other form of `body`, or a key written by two of them is rejected as "written twice". |
| A `Secret` | It may be an entry, in the short form or as a value, and it is sent as its string: `body form { grant_type; client_id; client_secret; }` for a token endpoint. A struct that holds a `Secret` is still never serialized: `body json(task)` with a secret field does not compile. |

**A `None` sends nothing**, in a body of entries just as in `query` and in `headers`. One level of `Option` means presence: the entry is there, or it is not. What is inside is serialized by the format, with `serde` ([rules](../reference/rules.md), rules 8 and 9).

| Written | What goes out |
|---|---|
| `x?: T`, or `x: Option<T>`, with `x;` | Left out, or the value. |
| `milestone?: Option<u64>` with `milestone;` | Left out, `null`, or a number (JSON). |
| `"k" = value;` where the value is an `Option`: `"x" = client.x;`, `"ids" = ids.as_ref().map(join);` | Left out when it is `None`. |
| `"k" = Some(x);` with `x: Option<T>` | `null`, or the value. This is how you send a `None` as `null`. |

A form has no `null`, so in a form an inner `None` sends no pair either. A list or a struct as a form value is a Build error.

**A form with files: `multipart`.** Import it with `use concord::multipart;`. The body is built part by part, and `finish()` returns it ([contracts](../reference/contracts.md) C2.16):

```rust
POST send_photo(chat_id: ChatId, photo: concord::Body) -> Message {
    path "sendPhoto";
    body multipart()
        .text("chat_id", chat_id)
        .file("photo", "photo.jpg", "image/jpeg", photo)
        .finish();
}
```

```rust
// in `concord::body`, with the feature `multipart`
pub fn multipart() -> Multipart;

impl Multipart {
    pub fn text(self, name: &str, value: impl ToString) -> Multipart;
    pub fn file(self, name: &str, file_name: &str, content_type: &str, body: impl Into<Body>) -> Multipart;
    pub fn finish(self) -> Result<Body, MultipartError>;
}
```

| | |
|---|---|
| `multipart()` | An empty builder. The parts are sent in the order in which they are added. |
| `.text(name, value)` | One text field: its name, and a value that has `to_string()`. It returns the builder. |
| `.file(name, file_name, content_type, body)` | One file: the field name, the file name, its content type, and its bytes: a `Body`, or anything that converts into one (a `Vec<u8>`, `Bytes`, a `String`). In the expression an input is a reference, and an input that is a `Body` is passed as it is (`photo`): a reference to a `Body` becomes a `Body` that shares its one-shot source or repeatable factory, as a clone does. It returns the builder. |
| `.finish()` | A `Result` of a `Body`, whose content type is `multipart/form-data` with its boundary. Its error is a `MultipartError`: a content type that is not a valid header value. Nothing panics. When `body` receives an `Err`, it is a Build error like any other, and nothing is sent. |
| The boundary | It is not random when every part is in memory: it is the first one, from a fixed list, that no part contains. So the same inputs produce the same bytes, and such a body is retried, and compared between two pages, like any other bytes. With a part that is a stream, the boundary is random (128 bits), chosen once by `finish()`. |
| A part that is a stream | A part made by `Body::stream` makes the whole body a one-shot stream: one attempt, one send. A stream that was already sent is a Build error before anything is sent, and `request()` rejects such a body without touching any part. A part made by `Body::repeatable` keeps the body repeatable: each attempt opens the part again, with the same boundary and the same bytes ([a body that is opened again](#a-body-that-is-opened-again-for-each-attempt)). |

The builder is a value, so an optional field is handled by your own function, which adds the part or not:

```rust
// plain Rust, beside the macro: `body photo_form(chat_id, photo, caption);`
use concord::body::{multipart, Body, MultipartError};

pub fn photo_form(chat_id: &ChatId, photo: &Body, caption: &Option<String>) -> Result<Body, MultipartError> {
    let mut form = multipart().text("chat_id", chat_id);
    if let Some(caption) = caption {
        form = form.text("caption", caption);
    }
    form.file("photo", "photo.jpg", "image/jpeg", photo).finish()
}
```

**Parts with their own headers: `multipart_mixed` and `multipart_related`.** For a body other than a form, `multipart_mixed(parts)` and `multipart_related(parts)` (feature `multipart`) take ordered pairs of a `HeaderMap` and a buffered or repeatable `Body`. Each part has exactly one valid content type, and the first related part is the root, whose type the writer puts in the `type` parameter of the body's content type. The writer makes no `Content-ID` and rewrites no reference: they are yours, in the headers and in the root's bytes:

```rust
use concord::{multipart_related, Body};
use concord::http::{HeaderMap, HeaderValue};

// plain Rust, beside the macro: `body message(root, attachment);`
fn message(root: &Body, attachment: &Body) -> Result<Body, concord::MimeWriteError> {
    let mut json = HeaderMap::new();
    json.insert("content-type", HeaderValue::from_static("application/json"));
    let mut image = HeaderMap::new();
    image.insert("content-type", HeaderValue::from_static("image/png"));
    image.insert("content-id", HeaderValue::from_static("<image-1>"));
    multipart_related([(json, root.clone()), (image, attachment.clone())])
}
```

A one-shot stream part or a part without one valid content type is refused with its one-based position, as is an empty list; the call then ends with a Build error, and nothing is sent. Repeatable stream parts are accepted and opened during each attempt. A boundary collision detected while reading fails the attempt before the colliding bytes are yielded. The rustdoc of `multipart_related` has the whole example, with its description and a call.

**No body, or a body of no bytes.** An endpoint without a `body` statement sends no body. A body of no bytes is written like any other: `body concord::Body::from(Vec::new());` sends a body that is present and empty, with no content type (`Body::new(Vec::new(), "application/json")` gives it one). Over HTTP/1.1, the HTTP library sends a request without a body, or with an empty one, with no `Content-Length`. A server that refuses such a request (`411 Length Required`, for a `POST` or a `PUT` with nothing in it) needs the description to write it: `headers { "content-length" = "0"; }`. With no body, `0` is the only length that Concord accepts: any other is a Build error, and nothing is sent.

**A body that the caller may leave out.** The `body` statement always sends a body: its expression returns a `Body`, or a `Result` of one, never an `Option`. So for a body that is present only when the caller sets an input, the description names a function of your own that returns a body of no bytes for `None`. `json(options)` would send `null` instead. (An API that must receive no body at all in that case needs two endpoints, one with `body` and one without.)

```rust
use concord::body::WriteError;
use concord::Body;

concord::api! {
    pub client Store(token: concord::Secret) {
        base "https://store.example.com";
        headers { "authorization" = "Bearer {token}"; }

        /// No body, and a length all the same: the server refuses a `POST` without one.
        POST archive(id: u64) {
            path "items/{id}/archive";
            headers { "content-length" = "0"; }
        }
        /// A body that is present and holds no bytes.
        PUT touch(id: u64) {
            path "items/{id}/touched";
            body concord::Body::from(Vec::new());
        }
        /// A JSON body when the caller sets `options`, and no bytes when it does not.
        DELETE delete(id: u64, options?: DeleteOptions) {
            path "items/{id}";
            body optional_json(options);
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct DeleteOptions {
    pub grace_seconds: u32,
}

/// The JSON of the value, or a body of no bytes and no content type for `None`.
pub fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Body, WriteError> {
    match value {
        Some(value) => concord::json(value),
        None => Ok(Body::from(Vec::new())),
    }
}
```

Tested: [`empty_body.rs`](../../tests/real-apis/fixtures/recipes/empty_body.rs), run by [`writer_recipes.rs`](../../tests/real-apis/tests/writer_recipes.rs), which compares this block with the file.

**For a body in either form:**

| | |
|---|---|
| The inputs are borrowed | In the expression, an input is a reference: `task` is a `&NewTask`. `json` and `form` take anything that `serde` can serialize, including a `serde_json::Value`. An input that is a `Body` is taken as it is. |
| The content type travels with the bytes | A `Body` carries its `Content-Type`. A `content-type` header that applies to the endpoint takes precedence: `headers { "content-type" = "application/merge-patch+json"; }` with `body json(patch);`. |
| A body that cannot be serialized | The expression may return a `Result`, with any error that converts into `Box<dyn Error + Send + Sync>`. An `Err` is a Build error: nothing is sent, and your writer's error is `e.source()`. |
| One body per endpoint | A body is not inherited from a node above: each endpoint declares how its own body is built. (A part that it names may add entries.) |
| `form` is flat | It writes pairs of strings. For a nested form (`metadata[key]=value`), `bracket_form` (feature `form`) writes `a[b]` for a map and `a[0]` for every list. An API with another convention (Rack and Rails read `a[0]` as a map key) needs your own function that returns a `Body`. |
| `null` or absent | **With `body json(x)`, the body follows its type, not the rule for entries.** In an entry, a `None` sends nothing; in a path placeholder it is rejected. In a body built by an expression, a `None` is whatever the type serializes it as: `json!({ "cursor": cursor })` sends `null`, and a struct leaves it out with `#[serde(skip_serializing_if = "Option::is_none")]`. A body of entries is the shorter way. |
| Retries | On a retry, bytes are sent again unchanged ([what holds for every retry](retries.md#what-holds-for-every-retry)), and a `Body::repeatable` is opened again from its first byte ([a body that is opened again](#a-body-that-is-opened-again-for-each-attempt)). **A `Body::stream` is sent once:** a call that has one gets one attempt, whatever the retry setting says ([a body that is a stream](#a-body-that-is-a-stream)). A clone of the call shares the stream of an input, and the first send consumes it: a second send of that same `Body`, through a clone or for the next page of a paginated call, is a Build error, and nothing is sent. (An expression that creates a new stream each time it runs gives each send its own stream.) A large upload that must survive a failure needs a body that can be opened again, or a loop in the caller's code. |
| A stream has no length | When the server needs one, the description sets it: `headers { "content-length" = size; }`. For a `Body::stream`, it is sent as written. A `Body::repeatable` must give exactly that many bytes, or the attempt fails. For a body in memory, it must equal the length of the body, and without a body it must be `0`, or the request is rejected when it is built ([no body](#a-body-that-is-sent)). |

## The shipped writers

Besides `json`, `form` and `multipart`, Concord ships four writers for bodies that APIs often need. Each is an ordinary function of `concord`, behind a feature. `bracket_form`, `gzip` and `json_lines` return a body held in memory, which the engine sends again on a retry like any other bytes; `multipart_mixed` and `multipart_related` do too when every part is in memory, and return a repeatable stream, opened again for each attempt, when a part is repeatable.

| Body | Written | What to know |
|---|---|---|
| Nested forms | `body concord::bracket_form { a; b; }` (feature `form`) | One indexed-key convention; `bracket_pairs` supplies the unencoded pairs for a runtime query. |
| Gzip bodies | `body concord::gzip(concord::json(events)?);` (features `gzip-body` and `json`) | Buffered bodies only; the description sets `content-encoding`. |
| MIME multipart bodies | `multipart_mixed(parts)`, `multipart_related(parts)` (feature `multipart`) | Ordered buffered or repeatable parts with their own `HeaderMap`; the first related part is the root. |
| JSON records | `body concord::json_lines(records, max_bytes);` (feature `json`) | One JSON value per line, including the last LF; the bound covers the whole body. |

**Nested forms: `bracket_form`.** `bracket_form` (feature `form`, on by default) writes nested maps and lists as a form: a map as `a[b]`, and every list as `a[0]`, `a[1]`, a list of plain values included. An index keeps its place when an entry before it is left out. `None`, `()` and an empty collection send no pair, and maps and structs keep their order. A unit enum variant is written with its `serde` name; a variant that carries data is refused, and so is a key that is empty or contains a bracket. The value at the root must be a map or a struct. Like `form`, it accepts at most 128 levels of nesting, and it has no limit on bytes. It takes a body of entries (`body concord::bracket_form { metadata; expand; }`) or an expression. A shape that it cannot write is a Build error, in which `concord::BracketFormError::find(&e)` finds `Shape`. Rack and Rails read `a[0]=x` as a map with the key `"0"`, so those APIs need a writer of their own. For a query whose keys are known only at run time, `bracket_pairs` returns the same pairs, not yet encoded ([the block](describing-an-api.md#the-block) shows how to add them).

**One JSON value per line: `json_lines`.** `json_lines(records, max_bytes)` (feature `json`, on by default) writes each record of an iterator as compact JSON followed by an LF, the last one included, with the content type `application/x-ndjson`; an iterator with no record gives an empty body. `max_bytes` bounds the whole body, every LF included. A record that would go past it, or that does not serialize, is a Build error, in which `concord::JsonLinesError::find(&e)` finds the reason: no partial body is returned, and nothing is sent. The writer builds the whole body in memory, at once: it takes an iterator, not a stream. It writes the records as they are given: for the `_bulk` endpoint of Elasticsearch, the author supplies each action and its document as two records, and checks every item of the response.

For a compressed request, compose the writers and declare the encoding header:

```rust
concord::api! {
    pub client Logs {
        base "https://logs.example.com";
        POST write(events: Vec<String>) {
            headers { "content-encoding" = "gzip"; }
            body concord::gzip(concord::json(events)?);
        }
    }
}
```

Tested: [`gzip.rs`](../../tests/real-apis/fixtures/recipes/gzip.rs), run by [`writer_recipes.rs`](../../tests/real-apis/tests/writer_recipes.rs), which compares this block with the file.

`gzip` keeps the input body's content type and produces repeatable compressed bytes with no timestamp in their header. It refuses a stream without polling it. The description sets `Content-Encoding` because a writer returns a `Body`, which carries no request header except its content type. The transport can determine the compressed body's length; do not copy a `Content-Length` from the uncompressed input. The `gzip-body` feature writes requests independently of the `gzip` feature for response decompression.

**A writer that wraps another body.** A writer of your own may take a body that another writer built and return a new one: a compressed body, a signed envelope. `body.get_content_type()` returns the content type that the inner body carries, whole, with its parameters, so the new body can keep it; this is what `gzip` does. A compressing writer, from the rustdoc of `Body::get_content_type`:

```rust
// plain Rust, beside the macro: `body compressed(concord::json(events)?);`
use concord::Body;
use flate2::{write::GzEncoder, Compression};
use std::io::{self, Write};

fn compressed(body: Body) -> io::Result<Body> {
    let bytes = body.as_bytes().ok_or_else(||
        io::Error::new(io::ErrorKind::InvalidInput, "a buffered body is required"))?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    let mut output = Body::from(encoder.finish()?);
    if let Some(label) = body.get_content_type() {
        output = output.content_type(label.to_owned());
    }
    Ok(output)
}
```

It returns the body's label, not the header that is sent: a `content-type` header of the description still takes precedence. As for `gzip`, the description sets `content-encoding`, and does not copy a `content-length` from the input. This writer accepts only a body held in memory.

## A body that is opened again for each attempt

`Body::repeatable(make)` is a body that is read as a stream, like `Body::stream`, but that can be sent again: `make` is a function that opens the stream, and the engine calls it once for each attempt. Use it for a large upload whose bytes can be read again from where they are: a file, an object fetched again from another store. The rustdoc of `concord::Body` has the whole example; this part of it sends a file to a `PUT` endpoint, `PUT put(file: Body, size: u64) { path "snapshot"; headers { "content-length" = size; } body file; }` on a client named `Uploads`; `snapshot` is the path of the file (a `PathBuf`), and `returned_headers` the headers that the API returned with the upload URL (a `HeaderMap`):

```rust
use concord::{Body, Retry};
use std::{io, path::PathBuf};
use tokio_util::io::ReaderStream;           // crate `tokio-util`, feature `io`; `tokio::fs` needs tokio's `fs`

let size = tokio::fs::metadata(&snapshot).await?.len();
let body = Body::repeatable(move || {
    let snapshot = snapshot.clone();
    async move {
        let file = tokio::fs::File::open(snapshot).await?;
        Ok::<_, io::Error>(ReaderStream::new(file))
    }
}).content_type("application/octet-stream");

Uploads::new()
    .put(body, size)
    .edit_request(move |request| {
        request.headers_mut().extend(returned_headers.clone());
    })
    .with_retry(Retry::standard())
    .await?;
```

| | |
|---|---|
| What `make` must give | The same bytes, from the first byte, each time it is called, with a cursor of its own: two clones of a call sent at once open two streams. A path does not keep a file the same: leave the file unchanged until every call that sends it has finished. `make` and its future run inside each poll of the body (by the transport, or by a middleware that reads it), so they must return promptly and not block. |
| When it is opened | When the body is first read, for each attempt. Building, cloning, inspecting and dropping a call open nothing. `body.as_bytes()` returns `None`, as for any stream. |
| What still decides a retry | The same three questions as for a body of bytes ([three questions decide a retry](retries.md#three-questions-decide-a-retry)): the method, `idempotent`, and `not_applied_on`. A repeatable body makes another attempt possible, not allowed: an unmarked `POST`, or a `PUT` with `idempotent false;`, is still sent once after a failed connection. Each attempt sends the whole body from its first byte: an upload that was cut halfway is sent again whole, never resumed. |
| A failure of the body | An error from `make` or from the stream ends the attempt with a Transport error of class Other, with your error itself as `e.source()`, whatever the transport reported. The standard table does not retry Other; a `when` rule of yours may. After its first error the body ends. |
| What an edit sees | `edit_request` runs once, before the first attempt, and sees a placeholder body that cannot be read. An edit may add headers, as the example does, or change the URL. Reading or removing the placeholder, or a detected replacement by a different kind of body, is a Build error before any attempt. Wrappers and look-alikes cannot always be detected; edits must leave the body alone. If building succeeds, the declared repeatable body is sent. A middleware, a signer among them, sees each attempt's own stream. |
| The length | The `content-length` of the request, from the description or from an edit, is the length that the source must give: a source that gives fewer or more bytes fails with `io::ErrorKind::InvalidData`. A `content-length` that is not a number is a Build error, before anything is opened. |
| Time | While the transport sends the body, `timeout` and the idle timeout cover its opening and its reading, and each chunk sent restarts the idle timeout. A middleware that reads the body is bounded only by `timeout`, which has no default. |
| Multipart | A `multipart` body whose parts are in memory or repeatable is repeatable: `finish()` chooses the boundary once, and each opening gives the same bytes. One part made by `Body::stream` makes the whole body single-attempt. |
| `request()` and pages | `request()` returns the request without opening the body. `into_request()` returns one whose body opens when it is read, with none of Concord's retries or timeouts. Each page of a paginated call opens the body again; a next page with the same head and the same `Body` as the one just sent ends the pages with a Mismatch, before the body is opened again. |

## A body that is a stream

A call whose body is a `Body::stream` is sent once, whatever its retry policy says: it gets one attempt, and `e.tries()` returns 1. The engine builds the request once and gives each attempt a copy of it, and a stream cannot be copied: its bytes are read as they are sent ([C3.2](../reference/contracts.md)). The policy's rules are not called for such a call, but `e.retry_after()` still returns the `Retry-After` of the response.

**To know beforehand.** `body.as_bytes()` returns the bytes of a body held in memory, and `None` when the body is a stream or contains one (a `multipart` with a part made by `Body::stream`). It does not read the stream or use it up. A body from `Body::new`, `json` or `form`, and a `multipart` whose parts are all in memory, holds bytes, and is retried like any other. It also returns `None` for a `Body::repeatable`, which is not held in memory and can still be sent again.

**To send a large upload again**, use `Body::repeatable` when its source can be opened again ([above](#a-body-that-is-opened-again-for-each-attempt)). For a `Body::stream`, which the engine cannot open again, or for a write that the engine may not repeat (its method, or `idempotent false;`) but that you judge safe to send twice, build the call again, with a new stream: the file, opened again. Your own loop then makes the attempts, a bounded number of times, after the failures that you judge safe to repeat. Here it sends a file with an `upload` endpoint like the one of [the tour](tour.md#an-endpoint-that-differs-says-so-in-its-own-block), a `PUT` whose body is its input `file`, on a client named `Files` (`PUT upload(name: String, file: concord::Body) { path "files/{name}"; body file; }`), and returns its errors as `BoxError`, written `type BoxError = Box<dyn std::error::Error + Send + Sync>;`:

```rust
use concord::error::Class;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::io::ReaderStream;           // crate `tokio-util`, feature `io`; `tokio::fs` needs tokio's `fs`

/// Sends the file at `path` as `name`: at most three calls, within ten minutes.
async fn upload(files: &Files, name: &str, path: &Path) -> Result<(), BoxError> {
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut calls = 0;
    loop {
        let file = tokio::fs::File::open(path).await?;             // the file, opened again
        let body = concord::Body::stream(ReaderStream::new(file));  // a new stream for each call
        let left = deadline.saturating_duration_since(Instant::now());
        let error = match files.upload(name, body).with_timeout(left).await {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        calls += 1;
        // A `PUT` of the same file to the same name may be repeated; these failures are worth it.
        let again = error.class() == Some(Class::Connect) || error.is_status(503);
        let wait = error.retry_after().unwrap_or(Duration::from_secs(1));
        if !again || calls == 3 || wait >= deadline.saturating_duration_since(Instant::now()) {
            return Err(error.into());
        }
        tokio::time::sleep(wait).await;
    }
}
```

Tested: [`upload_stream.rs`](../../tests/real-apis/fixtures/recipes/upload_stream.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

Each call sends the whole file. The loop decides what the engine's retry would have checked:

| The engine checks | Your loop decides |
|---|---|
| May it be repeated? (The method, or `idempotent`.) | Whether this write may be sent twice. A `PUT` of the same file to the same name may; a `POST` that creates a file may create two, unless the API takes an idempotency key. |
| Is the failure worth another attempt? (The standard table, and your rules.) | Which errors to repeat: here a failed connection (`e.class()` says `Connect`) and a `503`. After a failed connection, part of the file may have reached the server. |
| How many attempts, and how long to wait. (A growing random wait, never shorter than `Retry-After`; no attempt after a wait over 60 seconds.) | A bound on the number of calls, and each wait: `e.retry_after()` returns the `Retry-After` of the response. |
| The whole time. (`timeout` covers every attempt and every wait.) | A deadline of your own: `timeout` on one call covers that call only, so the loop gives each call the time that is left. |
