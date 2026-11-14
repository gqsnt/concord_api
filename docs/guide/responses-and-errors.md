# Read answers and errors, send bodies

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Call a client](calling.md) · next: [Credentials and middlewares](credentials.md)*

For the author and the user of a client: what happens to an answer (the simple rule, the reader, a status that means something, the API's own error), and, in the other direction, how the body of a request is written.

## The simple rule

Three lines cover most endpoints. A client writes one statement for them, once: `read Json;`.

1. **A 2xx is the success.** Its body is read into the type after the arrow, by the reader of the client. When the arrow is an `Option<T>` or a `Result<T, E>`, the body is read as `T`, and given as `Some` or as `Ok`.
2. **Any other status is an error.** It keeps the status, the headers and the start of the body.
3. **With no arrow, success gives `()`.** The body is ignored, with no exception ([the API's own error](#the-apis-own-error) says what keeps it so).

Everything else on this page is for the endpoint where one of these lines is not true.

**There is no format by default** ([rules](../reference/rules.md), rule 11). `Json` is an ordinary value of the shipped pieces, and an API that speaks XML writes `read Xml;` with a reader of its own, at the same place. An endpoint in which a body is read, by the simple rule or by an arm that binds `body`, needs a reader in reach, or it does not compile. A `HEAD exists(…) -> bool` with its two arms reads nothing, and needs none.

## The layers

An answer goes through three steps, always in this order:

| Step | Looks at | Decides |
|---|---|---|
| 1. The status | The status only | Which arm takes it. If none was written for it: the simple rule for a 2xx, a Refused error for the rest. |
| 2. The reader | The body | The bytes become a value. If they do not fit: a Mismatch for a 2xx; for an arm on another status, a Refused error with that status, its headers and its bytes, and no problem. |
| 3. The arm | What its pattern binds: the value, the headers, the status | What the call gives. |

The status is decided before a byte of the body is read. A body is never read "to see what it is".

## Reading a success

A reader is a Rust value with one job: turn the body into the value. The description names it with `read`, and the nearest one is used.

| Reader | The type after the arrow | Reads |
|---|---|---|
| `Json` | Any type that `serde` can read | The whole body, as JSON |
| `Text` | `String` | The whole body, as UTF-8 text |
| `Binary` | `bytes::Bytes` | The whole body, as it is |
| `Stream` | `concord::ByteStream` | Nothing: it hands over the body, to read as it comes ([below](#a-body-that-is-large-or-that-never-ends)) |
| A value of yours | What it says | What it wants |

```rust
GET readme(owner: String, repo: String) -> String {
    path "repos/{owner}/{repo}/readme";
    headers { override "accept" = "application/vnd.github.raw+json"; }
    read Text;
}
```

| | |
|---|---|
| **The limit** | `Json`, `Text` and `Binary` take the whole body and stop at 10 MiB, counted as the bytes arrive. Past it, the call is a Mismatch, and nothing more is read. Another limit is written on the reader: `read Json.limit(50 * concord::MIB);`. (Concord's own HTTP client does not decompress unless you turn that on in `Http::with`: a body is given as it came, and the limit counts the bytes that the transport gives.) |
| **The label** | `Json` reads a 2xx whatever its `Content-Type` says. A page of HTML where JSON was promised fails anyway, when it is decoded, and the error then says that the label was not the reader's either (a reader of yours says it with `.label_was_not_expected()`). For a server that must be held to its label: `read Json.strict();`, which refuses first. |
| **`Accept`** | Concord does not write one. A reader says how to read; it does not negotiate. An API that answers XML unless asked for JSON gets one line on the client: `headers { "accept" = "application/json"; }` ([scope and limits](../design/scope.md)). |
| **Text** | `Text` is UTF-8 and nothing else: bytes that are not UTF-8 are a Mismatch, never replaced silently. `Json` skips a byte order mark at the start. |
| **No bytes** | A body of no bytes is the reader's affair: `Text` gives an empty text, `Binary` no bytes, and `Json` a Mismatch, `the body is empty`. So a `204` under `-> T` with `read Json;` is a Mismatch, unless an arm takes it (`204 => None`). |
| **Why it did not decode** | The printed error names the place (`data.items[3].kind`), made of your field names and of indexes. The place is the reader's to give, and `Json` gives it where `serde` gives one (not through a flattened or a tagged enum). `e.source()` is made by Concord from the place and the kind of failure: it never holds a value of the answer. The message of the decoder, which can quote the body, is behind `e.details()`, 1 KiB of it at most; `e.reason()` is `None` for a body that did not read ([what comes back](calling.md#what-comes-back); the texts are in [the messages](../reference/messages.md)). |

**Your own reader** is a type with one trait, `concord::Reader<T>`. An XML API writes one, and names it once on the client: `read Xml;`.

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
| `Incoming` | The answer, with its body not read yet. `status()`, `headers()` and `content_type()` look at the head. Then the body is taken once: `bytes(limit)` gives it whole, `stream()` gives it as it comes. |
| `ReadError` | `?` on what `bytes` gives is already the right one. `ReadError::not_the_type(bytes)` says that the whole body arrived and is not the type. Then, as needed: `.with_details(text)` keeps the decoder's message for `e.details()` (Concord cuts it at 1 KiB); `.at(Place::root().field("items").index(3).field("kind"))` gives the place, a `concord::read::Place` made of pieces: a name that your program knows, an index, and `.key()` or `.variant()` for a key of a map or a variant that was read (their text is data, so only `[key]` and `[variant]` are kept). `quick_xml` gives no place, so `Xml` adds none; `.label_was_not_expected()` says that the label was not the reader's either. **A reader hands Concord no cause of its own, and no free text that is printed:** `source()` is made by Concord from the pieces of the place, so no reader can put a value of the answer there. Free text goes in the details. `ReadError::wrong_content_type()` is for a reader that holds the server to its label. |
| A test | A reader is tested alone, with no client: `let doc: Doc = Xml.read(Incoming::new(concord::reply(200, "<a/>"))).await?;` |

The shipped readers are written with this same trait: [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs).

A reader is a value, so an input can choose it. One endpoint that answers JSON, or a stream of events when the request asked for one:

```rust
POST create(request: ChatRequest) -> ChatReply {
    body json(request);
    read ChatRead(request.stream);          // your reader: `Json` or `Stream`, by this flag
}
```

## A body that is large, or that never ends

A 200 MB file and a feed of server-sent events are not a value. With `read Stream;` the call returns as soon as the head arrived and the status said yes, and gives the body to read:

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
- The two clocks of every call go on while you read the stream ([time](calling.md#time)). The clock for silence runs while a `try_next()` waits, not between two of them. (Read through `impl Stream`, with `next()`, it runs from the first poll that found nothing until the next piece, also between two polls.) A feed that is silent for minutes on purpose needs a longer `idle_timeout`: its author writes it on the endpoint (`idle_timeout …;`), or a caller sets `.with_idle_timeout(…)`. `timeout`, when you set one, is the whole call, the reading of the stream included.
- A success ends the try at the head. A refusal is read like any refusal: 64 KiB of its body, and the problem type.
- A failure after the head is reported by `try_next()`, as a `concord::read::StreamError`: a Transport error that keeps the status and the headers. `?` turns it into the client's error (`billing::Error`), or into a `ReadError` inside a reader of yours.
- A stream is never tried again once it was given.

**Records.** Concord does not cut a stream into records. Server-sent events, JSON Lines and CSV each have a crate that reads a stream of bytes, and `ByteStream` is one. A record that says "error" inside a stream is yours to read.

## When a status means something

A `404` that means "no such player". An `ETag` that belongs in the value. These are facts of an endpoint, so they are in the description, as arms:

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

**Arms add to the simple rule.** A status that has an arm gives what the arm says. A 2xx with no arm is still the success. In `by_riot_id` the `200` is not written: its body is read as an `Account`, and given as `Some`. So `None` is for a status, exactly: a `200` whose body is `null` is not a silent `None`. And an arm for one 2xx leaves the other 2xx to the simple rule, which reads them into the type after the arrow: with only `200 { body, headers } => Versioned { … }`, a `203` would be read as a `Versioned<Doc>`, which then needs `Deserialize`. An arm that builds the value from the head takes the class, as `get` does: `2xx { body, headers } => …`.

**"Nothing changed" needs no endpoint of its own.** Every call whose method is `GET` or `HEAD` has `send_if_none_match(etag)`. It takes the `ETag` as a `&str`, and writes `If-None-Match` when the request is built: in place of any that the description wrote, and before the edits of `edit_request`. A `304` that no arm takes is then not an error:

```rust
let answer = docs.get(&id).send_if_none_match(etag).await?;     // an `Answer<Option<Doc>>`
match answer.value() {
    Some(doc) => …,                      // changed: `answer.headers()` has the new `ETag`
    None      => …,                      // `304`: the headers are there too
}
```

**An arm is ordinary Rust, in one fixed place.** It binds what it uses, in a pattern, and its right side is an expression that gives the type after the arrow. The macro copies the expression as `Ok({ EXPR })` into a function that gives `Result<A, concord::Fail<E>>` ([contracts](../reference/contracts.md) C5.3); when it runs, and what a `?` in it does: [when an expression runs](../reference/grammar.md#when-an-expression-runs).

| In an arm | What happens |
|---|---|
| `200 { body } => …` | The body, read by the reader in reach before the arm runs. If it does not read: on a 2xx a Mismatch; on another status (`4xx { body } => Err(body)?`) a Refused error with that status, its headers and its bytes, and `problem()` is `None`, so retry still sees a refusal. Naming `body` is what reads it: `404 => None` reads nothing, and is the answer whatever the body holds. The type of `body` is found by the compiler: in `409 { body } => Err(body)` for a `Result<Charge, Declined>`, it is a `Declined`. When Rust cannot find it, the arm says it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`. |
| `201 { body, headers } => …`, `4xx { status } => …` | `headers` is a `&concord::Headers`, `status` is an `http::StatusCode`. They are there only when the pattern binds them. `headers.get("etag")` gives one header as text; `headers.as_map()` gives all of them. |
| `200 { body: task } => …` | The same field under a name of yours. Fields are left out without `..`. |
| A value of another type than the arrow | Does not compile. |
| `?` on a `Result` whose error is the API's problem type | The call ends with that problem, as a Refused error ([the API's own error](#the-apis-own-error)). |
| `headers.require("location")?` | The header, as text. If it is missing, the call ends with a Mismatch: the answer is not what was promised. |
| `.await` | Does not compile: an arm is not `async`. |

**A function of yours, used in an arm,** has a type to name for its failure: `concord::Fail<E>`, where `E` is the API's problem type. `concord::mismatch(…)` and `headers.require(…)` give one, and a problem becomes one with `?`.

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
| Keys | A status (`404`) or a class (`2xx`). An exact status wins over a class. |
| Where | At any node. A child has the arms above it; it replaces one with `override`, takes one away with `remove`. Most arms are on an endpoint. |
| Arms are compared by what they take | An arm is written, overridden and removed by its key. "What it takes" decides only when `override` is needed. A class arm from above takes every status of its class, so an exact status under it is written with `override`: `override 404 => None` under a client's `4xx { body } => …` takes the `404` away from the class arm. `override 2xx => …` over exact arms from above (`200`, `201`) is refused, until each one is overridden or removed. The other way needs no word: a class arm under an exact arm from above is written plainly, and the exact arm keeps its status. |
| In one node | A class and an exact status of that class may both be written; the exact one wins. `remove 4xx` takes away the arm written `4xx`, and nothing else: an exact arm set on the way stays. |
| An `Option` or a `Result` after the arrow | A 2xx is read as the inner type, and wrapped. The arms give the rest: `404 => None`, `409 { body } => Err(body)`. No arm is needed for the success. |
| `HEAD` | An answer to a `HEAD` has no body: its arms cannot bind one, and an arm from above that binds one does not reach it (the `override` and `remove` of the way down are applied first). With an arrow, there is nothing to read: the arm for a 2xx is written (`2xx => true`). |
| A redirect | Concord's own client follows none: a `3xx` is an answer like any other, and `302 => …` can be its arm. Without an arm it is a Refused error that keeps `Location`. |
| A `404` is not always "nothing" | It can also be a wrong URL, or a server that hides what you may not see. So an arm is a statement about one endpoint, by its author. |

## The API's own error

Most APIs refuse in one shape: a body with a code and a message. It is a fact of the whole API, so it is written once, at the root:

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
        None    => …,                                     // no answer, or a body that is not Riot's shape
    },
}
```

- The error of this client is `riot::Error` (it is `concord::Error<RiotProblem>`), and `riot::Result<T>` and `riot::Retry` (`concord::Retry<RiotProblem>`) go with it. With no `problem` line there is no typed problem: `problem()` gives `None`, and the bytes are in `e.body()`.
- On a status with no arm, Concord tries to decode the kept body as `RiotProblem`. If it does not decode, `problem()` gives `None`; the status and the bytes are still there. This never makes a second error.
- **It is read by the reader written on the client**, whatever an endpoint below says with its own `read`: how an API says no is a fact of the whole API. So `read Xml;` and `problem S3Problem;` on a client go together with nothing more. An API that says no in another format than it answers names that reader on the line: `problem Refusal read Json;` ([rules](../reference/rules.md), rule 14).
- Concord never looks inside the problem, and never prints it.

**No enum of every error.** `RiotProblem` is the one shape of the whole API. What only one endpoint can answer goes in that endpoint's own type, through its arms:

```rust
POST charge(new: NewCharge) -> Result<Charge, Declined> {
    path "charges";
    body form(new);
    status { 402 { body } => Err(body) }
}
```

The `200` is read as a `Charge` and given as `Ok`. A caller of `charge` handles `Declined`. A caller of another endpoint never sees it.

**One endpoint that says no in another shape.** `problem` is written once, on the client, because a client has one error type. Where a single endpoint refuses in a shape of its own, its arm reads that shape and gives the client's problem: `4xx { body } => Err(RiotProblem::from_upload(body))?`. The difference stays where it is written, and the caller still handles one error.

**An error inside a `200`.** Some APIs always answer `200`, with `{ "error": […], "result": … }`. The status says yes and the body says no. That is written once, on the client, with an arm that can fail:

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

Every endpoint below gives its own type. An arm on the client that binds a body reaches every endpoint below. **One with no arrow then does not compile:** the arm would read a body into nothing. It gets an arrow (`-> Ignored`, a type of yours that reads anything), or it removes the arm (`remove 2xx`). **One whose arrow is an `Option` or a `Result` does not compile either:** under the arm, a `null` in the body would be a `None`, with no word. It writes the arm itself (`override 2xx { body } => Some(unpack(body))`), or removes it. An endpoint below that gives a `200` its own meaning writes `override 200 { … } => …`. The arm does not reach a `HEAD`, which has no body: a `HEAD` under this client is a plain `HEAD`. A refusal inside the `200` is a Refused error with `problem()` set, like any other refusal. (An API that sends an error *together with* a result of another shape needs a `Deserialize` of yours that looks at `problem` first: with this simple struct, such a body does not decode at all, and the call is a Mismatch.)

## What an error keeps of a body

Everything that an error keeps from an answer is bounded. It keeps at most 64 KiB of the body, in a buffer of its own: a 200 MB error page does not stay in memory. It keeps at most 1 KiB of a reader's own message, behind `details()`. `e.retry_after()` gives the wait that was asked for: the `Retry-After` of the answer, or the wait of a rule of `when`, the longer of the two ([the standard policy](retries.md#the-standard-policy)). `kept()` says in what state: the whole body, a body cut at 64 KiB, or a body that broke while it was read; it is `None` when no byte of a body was kept (a size announced over the limit). `details()` says the state in words: `body (complete, 18 bytes)`, `body (cut at 64 KiB)`, `body (interrupted, 120 bytes)`. `problem()` is tried only from a whole body. An arm that ends with `?` gives an error with the typed problem and no bytes: the body was already read into a value.

**A failure that is known stays known.** A `503` whose body then breaks is still a Refused error with status 503. Reading the evidence can never replace the first failure. The exact lines are C5.6 to C5.8 of the [contracts](../reference/contracts.md).

## A body that is sent

One statement, in two forms. `body <expression>;`: the expression is ordinary Rust in which the inputs are variables, and it gives a `concord::Body`. Or a body made of entries, `body json { … }`, for a body that is its inputs.

| Written | Sent as |
|---|---|
| `body json(task);` | JSON, with `Content-Type: application/json` |
| `body form(task);` | A form, with `Content-Type: application/x-www-form-urlencoded` |
| `body file;` | An input that is a `concord::Body` already: the bytes as they are |
| `body json { query; cursor; "limit" = 50; }` | JSON, made of entries as in `query`: each input is a key (below) |
| `body multipart().text("chat_id", chat_id).file("photo", "a.jpg", "image/jpeg", photo).finish();` | A form made of several parts, with files (the feature `multipart`; below) |
| `body xml(task);` | Whatever a function of yours gives |

`json` and `form` are plain functions of `concord`, brought in by `use concord::{json, form};`. `multipart` is one more, behind the feature `multipart`. The macro knows none of them. A function of yours is the same thing:

```rust
// plain Rust, beside the macro
pub fn xml<T: Serialize>(value: &T) -> Result<concord::Body, quick_xml::SeError> {
    Ok(concord::Body::new(quick_xml::se::to_string(value)?, "application/xml"))
}
```

A writer of yours that puts bodies together has three more pieces: `Body::concat(parts)` gives one body made of several, in their order; `body.as_bytes()` gives the bytes of a body that is in memory, and `body.into_stream()` takes any body as a stream. A one-shot stream inside a body made of bodies is taken with the whole, when the request is built for a send: so a stream that was already sent is a Build error before anything goes out ([contracts](../reference/contracts.md) C2.15).

**A body made of entries.** In many APIs the body of a `POST` is its inputs. Then the body is written like a `query`:

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
| An entry | As in `query`: `name;` or `"key" = value;`. A template as a value (`"note" = "by {name}";`) is as there too: its text is written, a hole that reads nothing leaves the entry out, and a hole that reads several is a Build error. There is no `override` and no `remove` here. |
| What is written | The entries are one value, `concord::body::Entries`, which `serde` writes as a map, in the order written. The name before the block is called with it: `body json { a; b; }` is `body json(entries)`. So `form`, or a function of yours that takes something `serde` can write, fits too, and the macro still knows no format. |
| The name before the block | A path to a function, seen from where `api!` stands. An input of the same name does not hide it here. (In an expression it does: `body concord::form(form);`.) |
| A part | It may hold `body json { … }`. The node that names it writes `body json { … }` too, or no body: the entries are put together, the parts' first, in the order in which the node names them, then the node's own. Another function, the other form of `body`, or a key that two of them write, is refused as "written twice". |
| A `Secret` | It may be an entry, short or as a value, and it is written as its text: `body form { grant_type; client_id; client_secret; }` for a token endpoint. A struct that holds a `Secret` is still not written: `body json(task)` with a secret field does not compile. |

**A `None` writes nothing**, in a body of entries as in `query` and in `headers`. One level of `Option` is presence: the entry is there, or it is not. What is inside is written as the format writes it, with `serde` ([rules](../reference/rules.md), rules 8 and 9).

| Written | What goes out |
|---|---|
| `x?: T`, or `x: Option<T>`, with `x;` | Left out, or the value. |
| `milestone?: Option<u64>` with `milestone;` | Left out, `null`, or a number (JSON). |
| `"k" = value;` where the value is an `Option`: `"x" = client.x;`, `"ids" = ids.as_ref().map(join);` | Left out when it is `None`. |
| `"k" = Some(x);` with `x: Option<T>` | `null`, or the value. This is the way to write a `None` as `null`. |

A form has no `null`: there an inner `None` writes no pair either. A list or a struct as a value of a form is a Build error.

**A form with files: `multipart`.** It is brought in by `use concord::multipart;`. The body is built part by part, and `finish()` gives it ([contracts](../reference/contracts.md) C2.16):

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
| `multipart()` | An empty builder. The parts go out in the order in which they are added. |
| `.text(name, value)` | One field of text: its name, and a value that has `to_string()`. It gives the builder back. |
| `.file(name, file_name, content_type, body)` | One file: the name of the field, the name of the file, its content type, and its bytes: a `Body`, or what becomes one (a `Vec<u8>`, `Bytes`, a `String`). In the expression an input is a reference, and an input that is a `Body` is given as it is (`photo`): a reference to a `Body` becomes a `Body` that shares its stream, as a clone does. It gives the builder back. |
| `.finish()` | A `Result` of a `Body`, which carries the label `multipart/form-data` with its boundary. Its error is a `MultipartError`: a content type that is not a legal header value. Nothing panics. As what `body` gets, an `Err` is a Build error like any other: nothing is sent. |
| The boundary | It is not random when every part is in memory: it is the first of a fixed list that no part holds. So the same inputs give the same bytes, and such a body is tried again, and compared between two pages, like any bytes. With a part that is a stream it is random (128 bits): such a body goes out once anyway. |
| A part that is a stream | The whole body is then a stream: one try, one send. A stream that was already sent is a Build error before anything goes out, and `request()` refuses such a body without touching a part. |

The builder is a value, so a field that is optional is a function of yours, which adds the part or not:

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
| The inputs are borrowed | In the expression, an input is a reference: `task` is a `&NewTask`. `json` and `form` take anything that `serde` can write, and a `serde_json::Value` is one such thing. An input that is a `Body` is taken as it is. |
| The label goes with the bytes | A `Body` carries its `Content-Type`. A `content-type` header in reach of the endpoint wins over it: `headers { "content-type" = "application/merge-patch+json"; }` with `body json(patch);`. |
| A body that cannot be written | The expression may give a `Result`, with any error that `Box<dyn Error + Send + Sync>` takes. An `Err` is a Build error: nothing is sent, and the error of your writer is `e.source()`. |
| One body for an endpoint | It is not inherited from a node above: an endpoint says how its own body goes. (A part that it names may add entries.) |
| `form` is flat | It writes pairs of text. A nested form (`metadata[key]=value`) is a convention of one API: a function of yours that gives a `Body`. |
| `null` or absent | **With `body json(x)`, a body follows its type, not the rule of entries.** In an entry, a `None` writes nothing; in a hole of a path it is refused. In a body that an expression makes, a `None` is what the type writes: `json!({ "cursor": cursor })` writes `null`, and a struct leaves it out with `#[serde(skip_serializing_if = "Option::is_none")]`. A body of entries is the short way. |
| Sent again | Bytes are sent again as they are. **A body that is a stream is sent once:** a call that has one gets one try, whatever the retry setting says. A clone of the call shares the stream of an input, and the first send takes it: a second send of that same `Body`, through a clone or by the next page of a paged call, is a Build error, and nothing is sent. (An expression that makes a new stream each time it runs gives each send its own.) A large upload that must survive a failure is a loop of the caller. |
| A stream has no length | When the server needs one, the description writes it: `headers { "content-length" = size; }`. For a stream it is sent as written. For a body in memory it must be the length of the body, or the request is refused when it is built. |

## Types that survive a growing API

The types are yours, and Concord does not look inside them. One piece of advice comes from every SDK that lasted: **a value that the provider can extend must have a place for what you do not know yet.**

```rust
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Queue { RankedSolo, RankedFlex, Aram, #[serde(other)] Unknown }
```

Without the last case, the day Riot adds a queue, every answer that holds it stops decoding. Unknown fields are already ignored by `serde`. An id is text, also when it looks like a number.

## What the macro writes for an endpoint

How it works inside is on [the macro page](../internals/macro.md): the steps from a description to code, where the generated items live, how a name of yours reaches your Rust, and the list of what is written. The same thing as real code is in [`tests/tour/src/riot.rs`](../../tests/tour/src/riot.rs): the Riot client of the tour, written by hand as the macro writes it, with its description as a comment above it.

Two things to keep from it. **The whole request is in one function**, from the root to the leaf: what an endpoint inherits is worked out when the macro runs, and `request()` shows the result. **Every right side is copied as written:** `Json` after `read`, `json(task)` after `body`, the expression of an arm. The macro does not look inside them, your names and your imports are in scope, and the compiler checks each one against a trait of the core.
