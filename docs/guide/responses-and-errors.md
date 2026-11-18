# Read responses and errors

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Call a client](calling.md) · next: [Send a body](bodies.md)*

This page shows how a response becomes the value that a call returns, or its error: the simple rule, the reader, a status with a meaning of its own, the API's own error. [Send a body](bodies.md) shows the other direction: how the description builds a request body.

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
| `JsonLines::new(max)` | `concord::read::RecordStream<T>`, for any `T` that `serde` can deserialize | One JSON value per line, as the body arrives ([a stream of records](#a-stream-of-records)) |
| `Sse::new(max)` | `concord::read::RecordStream<SseEvent>` | Server-sent events, as the body arrives ([a stream of records](#a-stream-of-records)) |
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
| **Why it did not decode** | The printed error names the location (`data.items[3].kind`), built from your field names and from indexes. The reader provides the location, and `Json` provides it wherever `serde` does (not through a flattened or a tagged enum). Concord builds `e.source()` from the location and the kind of failure, so it never holds a value from the response. The decoder's message, which can quote the body, is available through `e.details()`, limited to 1 KiB; `e.reason()` is `None` for a body that did not decode, because only a message from the author's code sets it ([what a call returns](calling.md#what-a-call-returns); see [the messages](../reference/messages.md) for the exact texts). An arm that decodes a body itself keeps all of this with `Json::decode` ([an error inside a `200`](#the-apis-own-error)). |

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

The shipped readers implement this same trait: [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs), and [`read/records.rs`](../../concord-extras/src/read/records.rs) for the readers of records.

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

**Records.** JSON Lines and server-sent events have readers of their own, which split the body into records ([below](#a-stream-of-records)). For another format, such as CSV, a crate that reads a stream of bytes can read a `ByteStream`. A record inside the stream that reports an error is yours to handle: an item of the stream, or, in a [typed stream](#a-typed-stream), the client's own error. A JSON array is read whole into `Vec<T>` with `read Json.limit(…)`; the limit bounds the complete body, and Concord has no incremental JSON reader.

## A stream of records

Some bodies are a sequence of records that arrive one after another: JSON Lines (one JSON value per line, as in a Kubernetes watch) and server-sent events. Two readers split such a body into records as it arrives, and return a `concord::read::RecordStream`. Both are in `concord::read`: `use concord::read::{JsonLines, RecordStream, Sse, SseEvent};`.

```rust
GET watch() -> RecordStream<PodEvent> {
    path "pods";
    query { "watch" = true; }
    read JsonLines::new(64 * concord::KIB);        // the longest line that it accepts
}

GET events() -> RecordStream<SseEvent> {
    path "events";
    read Sse::new(16 * concord::KIB);              // the largest event that it accepts
}
```

```rust
async fn follow(cluster: &Cluster) -> Result<(), Box<dyn std::error::Error>> {
    let mut events = cluster.watch().await?;              // returns when the head has arrived
    while let Some(event) = events.try_next().await? {    // its error is a `RecordError`
        match event {
            PodEvent::Added(pod) => …,
            PodEvent::Error(status) => break,             // a raw stream: an error record of the API is a record like any other
        }
    }
    Ok(())
}
```

| | |
|---|---|
| **The bound** | Each reader requires one, and has no default. `JsonLines::new(max_record_bytes)` counts the bytes of a line before its LF, a CR included. `Sse::new(max_event_bytes)` counts each line of an event that is not blank, comments and unknown fields included, plus one byte for its line end. A record over its bound is an error, so a reader holds at most its bound, plus one chunk of the transport, whatever the server sends. |
| `JsonLines` | One JSON value per line, decoded into `T` with `serde` (feature `json`). LF and CRLF both end a line, and a last line without LF is read. An empty line is an error. |
| `Sse` | The rules of server-sent events: LF, CRLF or CR ends a line, a byte order mark at the start is skipped, comments are ignored, and the `data` lines of an event are joined with LF. An `SseEvent` has `event` (`message` when none was given), `data`, and the last `id` and `retry` that the server sent. An event that the body ends before its blank line is dropped. `event.json::<T>()` decodes `data` (feature `json`). Event names mean nothing to the reader: an `error` or a `done` event is yours to handle. |
| The head | The call returns as soon as the head has arrived with a success status, as with `read Stream;`. `status()` and `headers()` on the stream stay readable, also after its end or an error. |
| **An error** | `try_next()` returns a `concord::read::RecordError`, and the first error ends the stream: `TooLarge` (over the bound), `Utf8` (bytes that are not UTF-8), `Decode` (a record that does not decode, with its line and column, and its record number when it is known), or `Stream`, the failure of the body itself, kept whole as a `StreamError` with the status and the headers. No error holds bytes of the response. A `RecordError` does not convert into the client's error, so a function that uses `?` on the call and on `try_next()` returns an error that holds both: `Box<dyn std::error::Error>`, or an enum of your own. A typed stream ([below](#a-typed-stream)) fails with the client's own error instead. |
| Timeouts and retries | As with `read Stream;`: both timeouts keep running, and the idle timeout watches the bytes that arrive, comments included, not the records. A stream of records is never retried, and a stream of events is never reconnected. Dropping the stream releases the body. |

### A typed stream

Most streams of records mean something: an LLM's reply sends typed events, skips `ping`, ends with `message_stop`, and reports the API's error as an `error` event inside its `200`; a watch sends typed lines, one of which is the API's error. A function of your own says what each record is, and the reader gives a typed stream whose error is the client's own: `?` on the call and `?` on `try_next()` meet in one type.

```rust
POST stream(..request) -> RecordStream<StreamEvent, chat::Error> {
    path "messages";
    read Sse::new(64 * concord::KIB).try_map(event).until_last_record();
}

fn event(event: SseEvent) -> Result<Step<StreamEvent>, RecordFail<ApiProblem>> {
    Ok(match event.event.as_str() {
        "ping" => Step::Skip,
        "error" => return Err(RecordFail::problem(event.json()?)),
        _ => match event.json()? {
            StreamEvent::MessageStop => Step::Last(StreamEvent::MessageStop),
            typed => Step::Next(typed),
        },
    })
}

async fn reply(chat: &Chat) -> chat::Result<Vec<StreamEvent>> {
    let mut events = chat.stream(request).await?;          // the call's error
    let mut all = Vec::new();
    while let Some(event) = events.try_next().await? {     // the same error type
        all.push(event);
    }
    Ok(all)
}
```

| | |
|---|---|
| **The mapping** | `Sse::new(n).try_map(f)` gives `f` each `SseEvent`; `JsonLines::new(n).try_map(f)` gives it each line, a `JsonLine`, which `line.json()` decodes (or `line.text()` reads as it is). The bound and the framing are the reader's. `f` is cloned for each response, so state that it keeps starts afresh with each stream. |
| **The two words** | A mapped reader is finished by one of two words, and without one the description does not compile (the compiler names both): `.until_last_record()`, when the stream ends with a record of its own (`message_stop`, `[DONE]`), so that a body that ends before it is an error, never a whole reply; `.until_end_of_body()`, when the end of the body is the normal end (a watch, a file of results). |
| **The four steps** | `Ok(Step::Next(value))` yields the value. `Ok(Step::Skip)` yields nothing for this record. `Ok(Step::Last(value))` yields the value and ends the stream; `Ok(Step::End)` ends it with nothing. After `Last` or `End`, nothing more of the body is read, and the body is released at once. |
| **The three failures** | `Err(RecordFail::problem(p))`: the API's own error, a Refused error whose `problem()` is `p`, with the status (the `200`) and the headers of the response. `Err(RecordFail::mismatch("…"))`: a record, or the sequence so far, does not fit; a Mismatch, `record 7 does not fit the stream`, with your fixed sentence in `reason()` (it is never printed; it must hold nothing of the response). `?` on `event.json()` or `line.json()`: the record does not decode, a Mismatch, `record 7 could not be decoded`, with the decoder's line and column in `details()`. `?` cannot also turn a problem into a failure: write `Err(RecordFail::problem(p))`. |
| **The client's problem type** | The stream's error names the client's problem type: `RecordStream<Event, client::Error>`, and `RecordFail<Problem>` in the mapping. Another problem type does not compile, also behind an alias or a reader of your own. |
| **An error ends it** | As for every stream of records, the first error ends the stream: a record over the bound, bytes that are not UTF-8, a record that does not decode, the body that breaks, a timeout. The record is named by its position, as the reader counts its records (a line; an event block, blocks that dispatch nothing included). After the error, `try_next()` returns `None`, and the body is already released, as it is when the stream is dropped, and when the mapping panics. |
| **Skipped records** | All the records skipped in one `try_next()` share one budget of work; when it is spent, the call yields to the runtime and goes on, and the call's timeout (`with_timeout`) is checked before it yields. A server that sends only `ping` holds the caller until that timeout ends the call, not a thread. |

**An input of the call in the mapping.** The description gives each expression its inputs by reference, so a closure written in `read` that keeps one fails to compile: the compiler says that `__value` escapes (`borrowed data escapes outside of associated function`). Take an owned copy first:

```rust
GET feed(prefix: String) -> RecordStream<String, feed::Error> {
    read {
        let prefix = prefix.clone();
        Sse::new(16 * concord::KIB)
            .try_map(move |event| Ok(Step::Next(format!("{prefix}{}", event.data))))
            .until_end_of_body()
    };
}
```

**Moving a stream that you wrote by hand.** A client that wrapped `RecordStream<SseEvent>` in a type of its own (a struct with `try_next`, a `stopped` flag, an error enum of the API's event, of the end before the last event and of `RecordError`) can describe the endpoint as a typed stream and delete the wrapper. What its users lose is said here, so that no caller learns it from a failing build: the stream's error is now the client's error, so code that matched the variants of the old error, or that downcast to `concord::read::RecordError` (a decoding failure, a record over the bound, a broken body), changes: these are now a Mismatch or a Transport error of the client, told apart by `kind()` and their text, with no line and column outside `details()`. An error record that the stream used to yield as an item (`WatchEvent::Error`) is now an `Err`, whose `problem()` holds it; its status is the `200` of the stream, so test the body's code, not `is_status`. Events after the last one are no longer read. The raw route stays: an endpoint that keeps `read Sse::new(n);` and `-> RecordStream<SseEvent>`, or `read JsonLines::new(n);` and `-> RecordStream<WatchEvent<Pod>>`, reads exactly as before.

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
| `Json::decode(&body).map_err(Fail::from_read)?` | An arm that decodes the body itself ([an error inside a `200`](#the-apis-own-error)). If the body does not fit, the call ends with the error that a reader gives: on a 2xx, a Mismatch that names the location of the fault; on another status, a Refused error with the bytes and no problem. |
| `.await` | Does not compile: an arm is not `async`. |

**Your own function, used in an arm,** needs a type for its failure: `concord::Fail<E>`, where `E` is the API's problem type. `concord::mismatch(…)` and `headers.require(…)` return one, `Fail::from_read(error)` makes one from the `ReadError` of a decoder, and `?` converts a problem into one. (`?` cannot convert a `ReadError` by itself, because it already converts the problem type, whatever that type is: write `.map_err(Fail::from_read)`.)

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

**An error inside a `200`.** Some APIs always respond with `200` and report an error in the body. The status reports success and the body reports an error. That is handled once, on the client, with an arm that can fail. How the arm is written depends on where the value sits in the body.

*The value under a key of its own*, as in `{ "error": […], "result": … }`:

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

The reader decodes the whole body as a `Reply<T>`, and Rust infers `T` from the return type of each endpoint, so the arm does not name it. A body that does not fit is the reader's Mismatch, which names the location of the fault (`result.items[3].kind`), as under the simple rule.

*The value beside the flag*, as in `{ "ok": true, "id": "C1", "name": "general" }` and `{ "ok": false, "error": "channel_not_found" }`: there is no key to put `T` under. A generic type with `#[serde(flatten)] value: T` beside `ok` compiles and is inferred in the same way, but it does not do the job. A fault inside the value loses its location, because `serde` decodes a flattened value from a copy of its own: the error says `the body could not be decoded`, and its column is the end of the body. And a body with `ok: false` does not fit `T`, so the problem never arrives: the call is a Mismatch. (With `Option<T>`, a success that does not fit silently becomes `None`.) Instead, the client reads the bytes, and the arm decodes them itself with `Json::decode`, which returns the error that `read Json;` gives for bytes that do not fit. `Fail::from_read` makes that error the failure of the arm. The whole client:

```rust
use concord::{bytes::Bytes, Binary, Fail, Json};
use serde::{de::DeserializeOwned, Deserialize};

concord::api! {
    pub client Chat {
        base "https://chat.example.test/api";
        read Binary;                        // the arm decodes the body itself
        problem ChatProblem read Json;      // the error of any other status
        status { 2xx { body } => envelope(body)? }

        GET info() -> Channel { path "conversations.info"; }
    }
}

/// `{"ok": false, "error": "channel_not_found"}`, with the status `200`.
#[derive(Debug, Deserialize)]
pub struct ChatProblem {
    pub error: String,
}

/// `{"ok": true, "id": "C1", "name": "general"}`.
#[derive(Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

/// The arm of every 2xx: `ok` says whether the body is the value or the problem.
pub fn envelope<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<ChatProblem>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    if head.ok {
        Json::decode(&body).map_err(Fail::from_read)
    } else {
        let problem: ChatProblem = Json::decode(&body).map_err(Fail::from_read)?;
        Err(problem)?
    }
}
```

For `{"ok": true, "id": "C1", "name": 5}`, the call fails with `mismatch: status 200, name could not be decoded, in chat.info`: the same text and `source()` as under `read Json;`, the decoder's message with its line and column in `details()`, and no `reason()`. `body()` returns what the decoder was given, and `details()` names it `decoder input (complete, 35 bytes)`, not `body (…)`: once the arm has the body, Concord does not verify that the decoder was given that body, rather than a part of it or bytes from another call. For the same reason, a fault at the root says `the decoder's input could not be decoded`, and an empty input `the decoder's input is empty` ([the messages](../reference/messages.md)). The rustdoc of `Json::decode` has this client with a call for each of the two errors.

| | |
|---|---|
| **When the location is the same** | The arm decodes the bytes twice: first `ok`, then the value. When the first pass succeeds with `ok: true`, the second pass is the plain path, and a fault is found exactly where `read Json;` finds it. The first pass reads the whole body too, and it can fail where the plain path would not: a body with `ok` twice decodes under `read Json;` (the type ignores `ok`) and fails the first pass with ``duplicate field `ok` ``. The first pass can also catch broken JSON, where its syntax breaks, even after a value that does not fit: for `{"ok": true, "name": 5, "x": [1, }`, `read Json;` names `name`, and the arm names `[key]`, the field that the first pass was skipping. It does not catch every fault of what it skips: bytes that are not UTF-8 inside a string pass it, and the second pass meets them. A missing `ok` fails the first pass: `the decoder's input could not be decoded`, with ``missing field `ok` `` in `details()`. |
| **What `Json::decode` does not do** | It has no byte limit of its own: the limit is the one of the reader that read the bytes, here `Binary` (10 MiB; `read Binary.limit(50 * concord::MIB);` sets another). `strict` does not apply. And it does not look at the content type, so its error never adds `(unexpected content type)`. |
| **An endpoint without the arm** | Under `read Binary;`, an endpoint that removes the arm reads its body as bytes, unless it declares `read Json;` itself. |

Every endpoint below still returns its own type. An arm on the client that binds a body applies to every endpoint below, which has two consequences. **An endpoint with no return type then does not compile,** because the arm would read a body into nothing. Give it a return type (`-> Ignored`, your own type that accepts any body), or remove the arm (`remove 2xx`). **An endpoint whose return type is an `Option` or a `Result` does not compile either,** because under the arm a `null` in the body would silently become a `None`. Such an endpoint writes the arm itself (`override 2xx { body } => Some(unpack(body))`), or removes it. An endpoint below that handles a `200` in its own way writes `override 200 { … } => …`. The arm does not apply to a `HEAD`, which has no body: a `HEAD` under this client behaves like any other `HEAD`. An error reported inside the `200` is a Refused error with `problem()` set, like any other error response. (An API that sends an error *together with* a result of another shape needs your own `Deserialize` implementation that looks at `problem` first: with the simple `Reply<T>` above, such a body does not decode at all, and the call is a Mismatch.)

## What an error keeps of a body

Everything an error keeps from a response is bounded. It keeps at most 64 KiB of the body, in a buffer of its own, so a 200 MB error page does not stay in memory. It keeps at most 1 KiB of a reader's own message, available through `details()`. `e.retry_after()` returns the wait that was requested: the `Retry-After` of the response or the wait from a `when` rule, whichever is longer ([the standard policy](retries.md#the-standard-policy)). `kept()` says in what state the body was kept: whole, truncated at 64 KiB, or interrupted while it was read; it is `None` when no byte of a body was kept (for example, when the announced size was over the limit). `details()` describes the state in words: `body (complete, 18 bytes)`, `body (cut at 64 KiB)`, `body (interrupted, 120 bytes)`. `problem()` is decoded only from a whole body. An arm that ends with `?` produces an error with the typed problem and no bytes, because the body was already read into a value. An arm that decodes a body itself and returns `Fail::from_read(…)` keeps the decoder's input, at most 64 KiB, which `details()` names `decoder input (…)`.

**A known failure stays known.** A `503` whose body then breaks while it is read is still a Refused error with status 503. Reading the details of a failure can never replace the original failure. See C5.6 to C5.8 of the [contracts](../reference/contracts.md) for the exact rules.

## Types that survive a growing API

The types are yours, and Concord does not look inside them. One piece of advice comes from every SDK that has lasted: **a value that the provider can extend must have room for what you do not know yet.** What kind of room depends on whether the value is only read, or also sent back to the API.

Both forms below are for an enum that is written as a string in JSON, as most enums of an API are. **A value that is only read** can put every unknown string value in one variant, with `#[serde(other)]`:

```rust
#[derive(Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Queue { RankedSolo, RankedFlex, Aram, #[serde(other)] Unknown }
```

Without the last variant, the day Riot adds a queue, every response that contains it stops decoding. `Unknown` keeps no text: every queue that you do not know becomes the same `Unknown`. So as the key of a map (`HashMap<Queue, u32>`), two unknown queues become one entry, and one of the two values is lost.

**A value that is sent back** must keep what it does not know, or the next write sends something else. A variant marked `#[serde(untagged)]` keeps the text, and writes it back as it arrived:

```rust
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxType { Input, Output, Exempt, #[serde(untagged)] Other(String) }
```

`"OUTPUT"` is read as `Output`, and `"TAX001"`, a string value that the enum does not list, as `Other("TAX001")`; written back, they are `"OUTPUT"` and `"TAX001"` again, also as the keys of a map. The untagged variant is tried last, after every named variant. Neither form accepts a number, or an unknown variant that carries data (`{"REFUND": {…}}`): those still fail to decode, and an enum that the API writes that way needs a variant of its own for them. An object whose only key has the value `null`, which `serde` reads as a variant without data, gives three cases: `Queue`, with `#[serde(other)]`, reads `{"NEW": null}` as `Unknown`; `TaxType` reads `{"OUTPUT": null}` as `Output`; and `TaxType` rejects `{"NEW": null}`, because `Other(String)` takes only a string.

`serde` already ignores unknown fields. Treat an id as a string, even when it looks like a number.

## What the macro generates for an endpoint

See [the macro page](../internals/macro.md) for how the macro works inside: the steps from a description to code, where the generated items live, how your own names reach your Rust code, and the list of what is generated. [`tests/tour/src/riot.rs`](../../tests/tour/src/riot.rs) shows the same thing as real code: the Riot client of the tour, written by hand with the same public types and methods, with its description in a comment above it.

Two points are worth remembering. **The whole request is built in one function**, from the root to the leaf: what an endpoint inherits is resolved when the macro runs, and `request()` shows the result. **Every right side is copied as written:** `Json` after `read`, `json(task)` after `body`, the expression of an arm. The macro does not look inside them, your names and your imports are in scope, and the compiler checks each one against a trait of the core.
