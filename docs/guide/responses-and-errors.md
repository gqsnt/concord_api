# Read responses and errors, send bodies

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Call a client](calling.md) · next: [Credentials and middleware](credentials.md)*

This page shows how a response becomes the value that a call returns, or its error: the simple rule, the reader, a status with a meaning of its own, the API's own error. It also shows, in the other direction, how the description builds a request body.

## The simple rule

Three rules, together called the simple rule, cover most endpoints. A client needs one statement for them, written once: `read Json;`.

1. **A 2xx status is a success.** The client's reader reads its body into the endpoint's return type. When the return type is an `Option<T>` or a `Result<T, E>`, the body is read as `T` and returned as `Some` or `Ok`.
2. **Any other status is an error.** The error keeps the status, the headers and the start of the body.
3. **With no return type, a success returns `()`.** The body is ignored, without exception ([the API's own error](#the-apis-own-error) explains what guarantees this).

The rest of this page is for endpoints where one of these three rules does not hold.

**There is no default format** ([rules](../reference/rules.md), rule 11). `Json` is an ordinary value among the shipped items, and an API that uses XML declares `read Xml;` in the same place, with its own reader. An endpoint that reads a body, through the simple rule or through an arm that binds `body`, needs a reader that applies to it (its own or one from above), or it does not compile. A `HEAD exists(…) -> bool` with its two arms reads nothing, so it needs no reader.

## How a response is read: status, reader, arm

A response goes through three steps, always in this order:

| Step | Looks at | Decides |
|---|---|---|
| 1. The status | The status only | Which arm handles it. If no arm was written for it: the simple rule for a 2xx, and a Refused error for any other status. |
| 2. The reader | The body | How the bytes become a value. If they do not fit the type: a Mismatch for a 2xx; for an arm on another status, a Refused error with that status, its headers and its bytes, and no problem. |
| 3. The arm | What its pattern binds: the value, the headers, the status | What the call returns. |

The status is decided before any byte of the body is read. A body is never read "to see what it is".

## Reading a success

A reader is a Rust value with one job: turning the body into a value. The description names it with `read`, and the nearest one applies.

| Reader | The return type | Reads |
|---|---|---|
| `Json` | Any type that `serde` can deserialize | The whole body, as JSON |
| `Text` | `String` | The whole body, as UTF-8 text |
| `Binary` | `bytes::Bytes` | The whole body, unchanged |
| `Stream` | `concord::ByteStream` | Nothing: it hands you the body to read as it arrives ([below](#a-body-that-is-large-or-that-never-ends)) |
| Your own reader | Whatever it declares | Whatever it needs |

```rust
GET readme(owner: String, repo: String) -> String {
    path "repos/{owner}/{repo}/readme";
    headers { override "accept" = "application/vnd.github.raw+json"; }
    read Text;
}
```

| | |
|---|---|
| **The size limit** | `Json`, `Text` and `Binary` read the whole body and stop at 10 MiB, counted as the bytes arrive. Past that, the call fails with a Mismatch and nothing more is read. A different limit is set on the reader: `read Json.limit(50 * concord::MIB);`. (Concord's own HTTP client does not decompress unless you turn that on in `Http::with`: the body arrives as it was sent, and the limit counts the bytes that the transport delivers. With decompression on, those are the decoded bytes: [compression, proxies and TLS](calling.md#compression-proxies-and-tls).) |
| **The content type** | `Json` reads a 2xx whatever its `Content-Type` says. An HTML page where JSON was promised fails anyway when it is decoded, and the error then adds that the content type was not the one the reader expected either (your own reader reports this with `.label_was_not_expected()`). To hold a server to its content type, write `read Json.strict();`, which rejects a wrong one before decoding. |
| **`Accept`** | Concord does not send one. A reader says how to read a body; it does not negotiate. For an API that responds with XML unless the request asks for JSON, add one line to the client: `headers { "accept" = "application/json"; }` ([scope and limits](../design/scope.md)). |
| **Text** | `Text` accepts UTF-8 only: bytes that are not valid UTF-8 are a Mismatch, and are never silently replaced. `Json` skips a byte order mark at the start. |
| **An empty body** | What an empty body means is up to the reader: `Text` returns an empty string, `Binary` empty bytes, and `Json` a Mismatch, `the body is empty`. So a `204` on an endpoint with `-> T` and `read Json;` is a Mismatch, unless an arm handles it (`204 => None`). |
| **Why it did not decode** | The printed error names the location (`data.items[3].kind`), built from your field names and from indexes. The reader provides the location, and `Json` provides it wherever `serde` does (not through a flattened or a tagged enum). Concord builds `e.source()` from the location and the kind of failure, so it never holds a value from the response. The decoder's message, which can quote the body, is available through `e.details()`, limited to 1 KiB; `e.reason()` is `None` for a body that did not decode, because only a message from the author's code sets it ([what a call returns](calling.md#what-a-call-returns); see [the messages](../reference/messages.md) for the exact texts). |

**Your own reader** is a type that implements one trait, `concord::Reader<T>`. An XML API writes one and names it once, on the client: `read Xml;`.

```rust
use concord::read::{Incoming, ReadError, Reader};

/// `read Xml;`: the whole body as XML, into any type that `serde` can read. 10 MiB at most.
#[derive(Clone, Copy)]
pub struct Xml;

impl<T: serde::de::DeserializeOwned + Send> Reader<T> for Xml {
    async fn read(&self, body: Incoming) -> Result<T, ReadError> {
        let bytes = body.bytes(10 * concord::MIB).await?;        // too large, or broken: already the right error
        match quick_xml::de::from_reader(&bytes[..]) {
            Ok(value) => Ok(value),
            // the decoder's message can quote the body: it is kept for `e.details()`, and is never a cause
            Err(e) => Err(ReadError::not_the_type(bytes).with_details(e.to_string())),
        }
    }
}
```

| | |
|---|---|
| `Incoming` | The response, with its body not yet read. `status()`, `headers()` and `content_type()` inspect the head. Then the body is taken, once: `bytes(limit)` returns it whole, and `stream()` returns it as it arrives. |
| `ReadError` | Applying `?` to the result of `bytes` already produces the right error. `ReadError::not_the_type(bytes)` says that the whole body arrived and does not fit the type. Then, as needed: `.with_details(text)` keeps the decoder's message for `e.details()` (Concord truncates it at 1 KiB); `.at(Place::root().field("items").index(3).field("kind"))` sets the location, a `concord::read::Place` built from parts: a name that your program knows, an index, and `.key()` or `.variant()` for a map key or an enum variant that was read (their text is data, so only `[key]` and `[variant]` are kept). `quick_xml` provides no location, so `Xml` sets none; `.label_was_not_expected()` adds that the content type was not the one the reader expected either. **A reader passes Concord no cause of its own, and no free text that gets printed:** Concord builds `source()` from the parts of the location, so no reader can put a value from the response there. Free text goes in the details. `ReadError::wrong_content_type()` is for a reader that holds the server to its content type. |
| Testing | A reader is tested on its own, without a client: `let doc: Doc = Xml.read(Incoming::new(concord::reply(200, "<a/>"))).await?;` |

The shipped readers implement this same trait: [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs).

A reader is a value, so an input can choose it. Here, one endpoint responds with JSON, or with a stream of events when the request asked for one:

```rust
POST create(request: ChatRequest) -> ChatReply {
    body json(request);
    read ChatRead(request.stream);          // your reader: `Json` or `Stream`, by this flag
}
```

## A body that is large, or that never ends

A 200 MB file or a feed of server-sent events does not fit in one value. With `read Stream;`, the call returns as soon as the head has arrived with a success status, and hands you the body to read:

```rust
GET pdf(id: InvoiceId) -> concord::ByteStream {
    path "invoices/{id}/pdf";
    read Stream;
}
```

```rust
let mut pdf = billing.invoices().pdf(&id).await?;
while let Some(chunk) = pdf.try_next().await? {
    file.write_all(&chunk).await?;
}
```

- No size limit applies.
- Both timeouts of every call keep running while you read the stream ([timeouts](calling.md#timeouts)). The idle timeout runs while a `try_next()` is waiting, not between two calls to it. (If you read through `impl Stream` with `next()`, it runs from the first poll that found nothing until the next chunk arrives, even between two polls.) A feed that is idle for minutes on purpose needs a longer `idle_timeout`: its author sets it on the endpoint (`idle_timeout …;`), or a caller sets `.with_idle_timeout(…)`. The call timeout, `timeout`, when you set one, covers the whole call, including reading the stream.
- With a success, the attempt ends at the head. An error response is read like any other: up to 64 KiB of its body, and the problem type.
- A failure after the head is reported by `try_next()` as a `concord::read::StreamError`: a Transport error that keeps the status and the headers. `?` converts it into the client's error (`billing::Error`), or into a `ReadError` inside your own reader.
- Once a stream has been handed to you, it is never retried.

**Records.** Concord does not split a stream into records. Server-sent events, JSON Lines and CSV each have a crate that reads a stream of bytes, and `ByteStream` is such a stream. A record inside the stream that reports an error is yours to handle.

## When a status means something

A `404` may mean "no such player", and an `ETag` may belong in the returned value. These are facts about an endpoint, so they go in the description, as status arms:

```rust
/// `None` when there is no such player.
GET by_riot_id(game_name: String, tag_line: String) -> Option<Account> {
    path "by-riot-id/{game_name}/{tag_line}";
    status { 404 => None }
}

/// A document with its version, for the `If-Match` of the next call.
GET get(id: DocId) -> Versioned<Doc> {
    path "{id}";
    status { 2xx { body, headers } => Versioned { etag: headers.get("etag").map(String::from), value: body } }
}

/// Is it there?
HEAD exists(id: DocId) -> bool {
    path "{id}";
    status { 2xx => true, 404 => false }
}
```

**Arms add to the simple rule.** A status that has an arm returns what the arm produces. A 2xx without an arm is still a success. In `by_riot_id`, no arm is written for `200`: its body is read as an `Account` and returned as `Some`. So `None` stands for one exact status: a `200` whose body is `null` does not silently become `None`. And an arm for one 2xx status leaves the other 2xx statuses to the simple rule, which reads them into the endpoint's return type: with only `200 { body, headers } => Versioned { … }`, a `203` would be read as a `Versioned<Doc>`, which would then need `Deserialize`. An arm that builds the value from the head should handle the whole class, as `get` does: `2xx { body, headers } => …`.

**"Not modified" needs no endpoint of its own.** Every call whose method is `GET` or `HEAD` has `send_if_none_match(etag)`. It takes the `ETag` as a `&str` and sets `If-None-Match` when the request is built, replacing any value from the description, before any `edit_request` runs. A `304` that no arm handles is then not an error:

```rust
let answer = docs.get(&id).send_if_none_match(etag).await?;     // an `Answer<Option<Doc>>`
match answer.value() {
    Some(doc) => …,                      // changed: `answer.headers()` has the new `ETag`
    None      => …,                      // `304`: the headers are there too
}
```

**An arm is ordinary Rust, in one fixed place.** Its pattern binds what it uses, and its right side is an expression of the endpoint's return type. The macro copies the expression as `Ok({ EXPR })` into a function that returns `Result<A, concord::Fail<E>>` ([contracts](../reference/contracts.md) C5.3). See [when an expression runs](../reference/grammar.md#when-an-expression-runs) for when it runs and what a `?` inside it does.

| In an arm | What happens |
|---|---|
| `200 { body } => …` | The body, read by the nearest reader before the arm runs. If it does not decode: on a 2xx, a Mismatch; on another status (`4xx { body } => Err(body)?`), a Refused error with that status, its headers and its bytes, and `problem()` returns `None`, so retry still sees an error response. Binding `body` is what makes the arm read it: `404 => None` reads nothing, and is the result whatever the body holds. The compiler infers the type of `body`: in `409 { body } => Err(body)` for a `Result<Charge, Declined>`, it is a `Declined`. When Rust cannot infer it, the arm declares it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`. |
| `201 { body, headers } => …`, `4xx { status } => …` | `headers` is a `&concord::Headers`, and `status` is an `http::StatusCode`. They exist only when the pattern binds them. `headers.get("etag")` returns one header as text; `headers.as_map()` returns all of them. |
| `200 { body: task } => …` | The same field, bound to a name of your choice. Fields you do not need can be left out without `..`. |
| A value of a type other than the return type | Does not compile. |
| `?` on a `Result` whose error is the API's problem type | The call ends with that problem, as a Refused error ([the API's own error](#the-apis-own-error)). |
| `headers.require("location")?` | The header, as text. If it is missing, the call ends with a Mismatch: the response is not what was promised. |
| `.await` | Does not compile: an arm is not `async`. |

**Your own function, used in an arm,** needs a type for its failure: `concord::Fail<E>`, where `E` is the API's problem type. `concord::mismatch(…)` and `headers.require(…)` return one, and `?` converts a problem into one.

```rust
// plain Rust, beside the macro
pub fn created(task: Task, headers: &concord::Headers) -> Result<Created, concord::Fail<TaskProblem>> {
    Ok(Created { task, at: headers.require("location")?.to_owned() })
}

// in the description
POST create(task: NewTask) -> Created {
    body json(task);
    status { 2xx { body, headers } => created(body, headers)? }
}
```

**The rules** ([rule 13](../reference/rules.md); [grammar](../reference/grammar.md)).

| | |
|---|---|
| Keys | A status (`404`) or a class (`2xx`). An exact status takes precedence over a class. |
| Where | On any node. A child inherits the arms above it; it replaces one with `override` and drops one with `remove`. Most arms are on an endpoint. |
| Arms are compared by the statuses they cover | An arm is written, overridden and removed by its key. The statuses it covers matter only to decide whether `override` is needed. An inherited class arm covers every status of its class, so an exact status below it is written with `override`: `override 404 => None` under a client's `4xx { body } => …` takes the `404` away from the class arm. `override 2xx => …` over inherited exact arms (`200`, `201`) is rejected until each of them is overridden or removed. The other direction needs no keyword: a class arm below an inherited exact arm is written plainly, and the exact arm keeps its status. |
| In one node | A class and an exact status of that class may both be written; the exact one takes precedence. `remove 4xx` drops the arm written as `4xx`, and nothing else: an exact arm of that class, such as a `404` inherited from above, stays. |
| An `Option` or a `Result` as the return type | A 2xx is read as the inner type, then wrapped. The arms produce the other variant: `404 => None`, `409 { body } => Err(body)`. No arm is needed for the success. |
| `HEAD` | A response to a `HEAD` has no body: its arms cannot bind one, and an inherited arm that binds one does not apply to it (the `override` and `remove` statements on the way down are applied first). With a return type there is nothing to read, so the arm for a 2xx is written out (`2xx => true`). |
| A redirect | Concord's own client follows none: a `3xx` is a response like any other, and `302 => …` can be its arm. Without an arm, it is a Refused error that keeps `Location`. |
| A `404` is not always "nothing" | It can also mean a wrong URL, or a server that hides what you are not allowed to see. So an arm is a statement about one endpoint, made by its author. |

## The API's own error

Most APIs return errors in one shape: a body with a code and a message. That shape is a fact about the whole API, so it is declared once, at the root:

```rust
pub client Riot(key: Secret) {
    headers { "x-riot-token" = key; }
    read Json;
    problem RiotProblem;
    …
}
```

```rust
match asia.get(id).await {
    Ok(game) => …,
    Err(e) => match e.problem() {
        Some(p) => eprintln!("{}", p.status.message),     // typed; no type name at the call
        None    => …,                                     // no response, or a body that is not Riot's shape
    },
}
```

- The error type of this client is `riot::Error` (that is, `concord::Error<RiotProblem>`), together with `riot::Result<T>` and `riot::Retry` (`concord::Retry<RiotProblem>`). Without a `problem` line there is no typed problem: `problem()` returns `None`, and the bytes are in `e.body()`.
- On a status that has no arm, Concord tries to decode the kept body as a `RiotProblem`. If it does not decode, `problem()` returns `None`; the status and the bytes are still available. This never causes a second error.
- **The problem is read by the reader declared on the client**, whatever an endpoint below declares with its own `read`, because the way an API reports errors is a fact about the whole API. So `read Xml;` and `problem S3Problem;` on a client work together with nothing more. An API whose errors use a different format from its successful responses names that reader on the same line: `problem Refusal read Json;` ([rules](../reference/rules.md), rule 14).
- Concord never looks inside the problem, and never prints it.

**No enum of every possible error.** `RiotProblem` is the single error shape of the whole API. An error that only one endpoint can return goes in that endpoint's own type, through its arms:

```rust
POST charge(new: NewCharge) -> Result<Charge, Declined> {
    path "charges";
    body form(new);
    status { 402 { body } => Err(body) }
}
```

The `200` is read as a `Charge` and returned as `Ok`. A caller of `charge` handles `Declined`; a caller of any other endpoint never sees it.

**One endpoint whose errors have another shape.** `problem` is written once, on the client, because a client has one error type. When a single endpoint returns errors in a shape of its own, its arm reads that shape and converts it into the client's problem: `4xx { body } => Err(RiotProblem::from_upload(body))?`. The difference stays where it is written, and the caller still handles one error type.

**An error inside a `200`.** Some APIs always respond with `200` and a body like `{ "error": […], "result": … }`. The status reports success and the body reports an error. That is handled once, on the client, with an arm that can fail:

```rust
pub client Vault {
    base "https://api.vault.example";
    read Json;
    problem VaultProblem;
    status { 2xx { body } => Reply::into_result(body)? }
    …
}

// plain Rust, beside the macro
#[derive(Deserialize)]
pub struct Reply<T> { error: Vec<String>, result: Option<T> }

impl<T> Reply<T> {
    pub fn into_result(self) -> Result<T, VaultProblem> {
        match self.result {
            Some(result) if self.error.is_empty() => Ok(result),
            _ => Err(VaultProblem(self.error)),
        }
    }
}
```

Every endpoint below still returns its own type. An arm on the client that binds a body applies to every endpoint below, which has two consequences. **An endpoint with no return type then does not compile,** because the arm would read a body into nothing. Give it a return type (`-> Ignored`, your own type that accepts any body), or remove the arm (`remove 2xx`). **An endpoint whose return type is an `Option` or a `Result` does not compile either,** because under the arm a `null` in the body would silently become a `None`. Such an endpoint writes the arm itself (`override 2xx { body } => Some(unpack(body))`), or removes it. An endpoint below that handles a `200` in its own way writes `override 200 { … } => …`. The arm does not apply to a `HEAD`, which has no body: a `HEAD` under this client behaves like any other `HEAD`. An error reported inside the `200` is a Refused error with `problem()` set, like any other error response. (An API that sends an error *together with* a result of another shape needs your own `Deserialize` implementation that looks at `problem` first: with this simple struct, such a body does not decode at all, and the call is a Mismatch.)

## What an error keeps of a body

Everything an error keeps from a response is bounded. It keeps at most 64 KiB of the body, in a buffer of its own, so a 200 MB error page does not stay in memory. It keeps at most 1 KiB of a reader's own message, available through `details()`. `e.retry_after()` returns the wait that was requested: the `Retry-After` of the response or the wait from a `when` rule, whichever is longer ([the standard policy](retries.md#the-standard-policy)). `kept()` says in what state the body was kept: whole, truncated at 64 KiB, or interrupted while it was read; it is `None` when no byte of a body was kept (for example, when the announced size was over the limit). `details()` describes the state in words: `body (complete, 18 bytes)`, `body (cut at 64 KiB)`, `body (interrupted, 120 bytes)`. `problem()` is decoded only from a whole body. An arm that ends with `?` produces an error with the typed problem and no bytes, because the body was already read into a value.

**A known failure stays known.** A `503` whose body then breaks while it is read is still a Refused error with status 503. Reading the details of a failure can never replace the original failure. See C5.6 to C5.8 of the [contracts](../reference/contracts.md) for the exact rules.

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
| `.file(name, file_name, content_type, body)` | One file: the field name, the file name, its content type, and its bytes: a `Body`, or anything that converts into one (a `Vec<u8>`, `Bytes`, a `String`). In the expression an input is a reference, and an input that is a `Body` is passed as it is (`photo`): a reference to a `Body` becomes a `Body` that shares its stream, as a clone does. It returns the builder. |
| `.finish()` | A `Result` of a `Body`, whose content type is `multipart/form-data` with its boundary. Its error is a `MultipartError`: a content type that is not a valid header value. Nothing panics. When `body` receives an `Err`, it is a Build error like any other, and nothing is sent. |
| The boundary | It is not random when every part is in memory: it is the first one, from a fixed list, that no part contains. So the same inputs produce the same bytes, and such a body is retried, and compared between two pages, like any other bytes. With a part that is a stream, the boundary is random (128 bits): such a body is sent only once anyway. |
| A part that is a stream | The whole body is then a stream: one attempt, one send. A stream that was already sent is a Build error before anything is sent, and `request()` rejects such a body without touching any part. |

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

**For a body in either form:**

| | |
|---|---|
| The inputs are borrowed | In the expression, an input is a reference: `task` is a `&NewTask`. `json` and `form` take anything that `serde` can serialize, including a `serde_json::Value`. An input that is a `Body` is taken as it is. |
| The content type travels with the bytes | A `Body` carries its `Content-Type`. A `content-type` header that applies to the endpoint takes precedence: `headers { "content-type" = "application/merge-patch+json"; }` with `body json(patch);`. |
| A body that cannot be serialized | The expression may return a `Result`, with any error that converts into `Box<dyn Error + Send + Sync>`. An `Err` is a Build error: nothing is sent, and your writer's error is `e.source()`. |
| One body per endpoint | A body is not inherited from a node above: each endpoint declares how its own body is built. (A part that it names may add entries.) |
| `form` is flat | It writes pairs of strings. A nested form (`metadata[key]=value`) is a convention of one particular API: write your own function that returns a `Body`. |
| `null` or absent | **With `body json(x)`, the body follows its type, not the rule for entries.** In an entry, a `None` sends nothing; in a path placeholder it is rejected. In a body built by an expression, a `None` is whatever the type serializes it as: `json!({ "cursor": cursor })` sends `null`, and a struct leaves it out with `#[serde(skip_serializing_if = "Option::is_none")]`. A body of entries is the shorter way. |
| Retries | On a retry, bytes are sent again unchanged. **A body that is a stream is sent once:** a call that has one gets one attempt, whatever the retry setting says. A clone of the call shares the stream of an input, and the first send consumes it: a second send of that same `Body`, through a clone or for the next page of a paginated call, is a Build error, and nothing is sent. (An expression that creates a new stream each time it runs gives each send its own stream.) A large upload that must survive a failure needs a loop in the caller's code. |
| A stream has no length | When the server needs one, the description sets it: `headers { "content-length" = size; }`. For a stream, it is sent as written. For a body in memory, it must equal the length of the body, or the request is rejected when it is built. |

## Types that survive a growing API

The types are yours, and Concord does not look inside them. One piece of advice comes from every SDK that has lasted: **a value that the provider can extend must have room for what you do not know yet.**

```rust
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Queue { RankedSolo, RankedFlex, Aram, #[serde(other)] Unknown }
```

Without the last variant, the day Riot adds a queue, every response that contains it stops decoding. `serde` already ignores unknown fields. Treat an id as a string, even when it looks like a number.

## What the macro generates for an endpoint

See [the macro page](../internals/macro.md) for how the macro works inside: the steps from a description to code, where the generated items live, how your own names reach your Rust code, and the list of what is generated. [`tests/tour/src/riot.rs`](../../tests/tour/src/riot.rs) shows the same thing as real code: the Riot client of the tour, written by hand the way the macro generates it, with its description in a comment above it.

Two points are worth remembering. **The whole request is built in one function**, from the root to the leaf: what an endpoint inherits is resolved when the macro runs, and `request()` shows the result. **Every right side is copied as written:** `Json` after `read`, `json(task)` after `body`, the expression of an arm. The macro does not look inside them, your names and your imports are in scope, and the compiler checks each one against a trait of the core.
