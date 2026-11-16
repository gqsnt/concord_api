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

**Records.** JSON Lines and server-sent events have readers of their own, which split the body into records ([below](#a-stream-of-records)). For another format, such as CSV, a crate that reads a stream of bytes can read a `ByteStream`. A record inside the stream that reports an error is yours to handle.

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
            PodEvent::Error(status) => break,             // an error record of the API is a record like any other
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
| **An error** | `try_next()` returns a `concord::read::RecordError`, and the first error ends the stream: `TooLarge` (over the bound), `Utf8` (bytes that are not UTF-8), `Decode` (a record that does not decode, with its line and column, and its record number when it is known), or `Stream`, the failure of the body itself, kept whole as a `StreamError` with the status and the headers. No error holds bytes of the response. A `RecordError` does not convert into the client's error, so a function that uses `?` on the call and on `try_next()` returns an error that holds both: `Box<dyn std::error::Error>`, or an enum of your own. |
| Timeouts and retries | As with `read Stream;`: both timeouts keep running, and the idle timeout watches the bytes that arrive, comments included, not the records. A stream of records is never retried, and a stream of events is never reconnected. Dropping the stream releases the body. |

## Binary frames

Use `read concord::read::Stream;` for the response, then a codec for the wire format. `tokio_util::io::StreamReader` adapts a `ByteStream` to `AsyncRead`, and `LengthDelimitedCodec` splits length-prefixed frames. The bridge keeps Concord's call and idle timeouts, and a body failure remains a `StreamError` inside an `io::Error`, with its status, headers and attempt number. It sends no new request.

These examples use `tokio-util` with features `codec` and `io`, `futures-util` for `TryStreamExt`, and `crc32fast` for AWS checksums. Their exact source files are [docker_frames.rs](../../tests/real-apis/fixtures/recipes/docker_frames.rs) and [aws_frames.rs](../../tests/real-apis/fixtures/recipes/aws_frames.rs); [stream_recipes.rs](../../tests/real-apis/tests/stream_recipes.rs) includes and runs them, and checks each block against this page word for word.

Docker's non-TTY logs have an eight-byte prefix: a channel, three reserved bytes, and a big-endian payload length. The codec's bound applies to the length field before adjustment, so this permits 64 KiB of payload plus the eight-byte prefix. TTY logs are raw bytes and need no frame codec.

```rust
use concord::{bytes::Bytes, ByteStream};
use futures_util::TryStreamExt;
use std::io;
use tokio_util::{
    codec::{FramedRead, LengthDelimitedCodec},
    io::StreamReader,
};

concord::api! {
    pub client Docker(origin: concord::Base) {
        base origin;
        GET logs(container: String) -> concord::ByteStream {
            path "containers/{container}/logs";
            query { "stdout" = true; "stderr" = true; "follow" = true; }
            read concord::read::Stream;
        }
    }
}

/// Splits non-TTY Docker logs, allowing at most 64 KiB of payload per frame.
pub fn docker_frames(
    body: ByteStream,
) -> FramedRead<StreamReader<ByteStream, Bytes>, LengthDelimitedCodec> {
    let codec = LengthDelimitedCodec::builder()
        .length_field_offset(4)
        .length_field_type::<u32>()
        .length_adjustment(8)
        .num_skip(0)
        .max_frame_length(64 * concord::KIB)
        .new_codec();
    FramedRead::new(StreamReader::new(body), codec)
}

/// Validates the complete prefix and returns its channel and uninterpreted payload.
pub fn docker_log(frame: Bytes) -> io::Result<(u8, Bytes)> {
    if frame.len() < 8 || !matches!(frame[0], 0..=2) || frame[1..4] != [0, 0, 0] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid Docker log prefix.",
        ));
    }
    Ok((frame[0], frame.slice(8..)))
}

/// Keeps completed log records until EOF, leaving them available on a later failure.
pub async fn read_logs(body: ByteStream, logs: &mut Vec<(u8, Bytes)>) -> io::Result<()> {
    let mut frames = docker_frames(body);
    while let Some(frame) = frames.try_next().await? {
        logs.push(docker_log(frame.freeze())?);
    }
    Ok(())
}
```

`read_logs` pulls complete frames and keeps their channel and payload in caller-owned storage; process or discard records as they arrive if retaining every log would use too much memory. The underlying head remains available through `frames.get_ref().get_ref().status()` and `.headers()`. Dropping a pending frame pull retains the codec's buffered bytes. The bridge polls `ByteStream` as a `Stream`: a pending idle timer includes time between polls, even when an outer pull future is dropped. Drop the whole codec to release the body. These adapters do not promise a fixed amount of work in one poll, and bytes or decoded values retained by the application take additional memory.

AWS event-stream messages carry their total length in the first four bytes. With no bytes skipped, a length adjustment of zero keeps the whole frame, including both checksums. The prelude is checked after the codec has read that bounded frame, not before buffering its payload. This S3 Select consumer keeps completed Records payloads in caller-owned storage and accepts completion only after an End event; EOF between complete frames is still an application error if End never arrived. The authorization input represents a signature for this request; use the API's signing code, as in [the S3 example](../examples/s3.md).

```rust
use concord::{bytes::Bytes, ByteStream};
use futures_util::TryStreamExt;
use std::io;
use tokio_util::{
    codec::{FramedRead, LengthDelimitedCodec},
    io::StreamReader,
};

concord::api! {
    pub client S3(origin: concord::Base, authorization: concord::Secret) {
        base origin;
        headers { "authorization" = authorization; }
        POST select(key: String, query: concord::Body) -> concord::ByteStream {
            path "{*key}";
            query { "select" = ""; "select-type" = 2; }
            body query;
            read concord::read::Stream;
        }
    }
}

/// Splits AWS event-stream messages, including the entire prelude and final CRC.
pub fn aws_frames(
    body: ByteStream,
) -> FramedRead<StreamReader<ByteStream, Bytes>, LengthDelimitedCodec> {
    let codec = LengthDelimitedCodec::builder()
        .length_field_type::<u32>()
        .length_adjustment(0)
        .num_skip(0)
        .max_frame_length(concord::MIB)
        .new_codec();
    FramedRead::new(StreamReader::new(body), codec)
}

/// Checks an S3 Select frame and returns its event type and payload.
pub fn select_event(frame: &[u8]) -> io::Result<(&[u8], &[u8])> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid AWS event frame.");
    if frame.len() < 16 {
        return Err(invalid());
    }
    let word = |bytes: &[u8]| -> io::Result<usize> {
        Ok(u32::from_be_bytes(bytes.try_into().map_err(|_| invalid())?) as usize)
    };
    let total = word(&frame[..4])?;
    let headers = word(&frame[4..8])?;
    if total != frame.len()
        || headers > total - 16
        || crc32fast::hash(&frame[..8]) as usize != word(&frame[8..12])?
        || crc32fast::hash(&frame[..total - 4]) as usize != word(&frame[total - 4..])?
    {
        return Err(invalid());
    }
    let mut fields = &frame[12..12 + headers];
    let mut message = None;
    let mut event = None;
    while !fields.is_empty() {
        let name_len = usize::from(fields[0]);
        let name = fields.get(1..1 + name_len).ok_or_else(invalid)?;
        let kind = *fields.get(1 + name_len).ok_or_else(invalid)?;
        fields = fields.get(2 + name_len..).ok_or_else(invalid)?;
        let size = match kind {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            5 | 8 => 8,
            9 => 16,
            6 | 7 => {
                let size = u16::from_be_bytes(
                    fields
                        .get(..2)
                        .ok_or_else(invalid)?
                        .try_into()
                        .map_err(|_| invalid())?,
                ) as usize;
                fields = &fields[2..];
                size
            }
            _ => return Err(invalid()),
        };
        let value = fields.get(..size).ok_or_else(invalid)?;
        fields = &fields[size..];
        if name == b":message-type" || name == b":event-type" {
            if kind != 7 {
                return Err(invalid());
            }
            let slot = if name == b":message-type" {
                &mut message
            } else {
                &mut event
            };
            if slot.replace(value).is_some() {
                return Err(invalid());
            }
        }
    }
    if message != Some(b"event".as_slice()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "S3 Select returned an error event.",
        ));
    }
    Ok((event.ok_or_else(invalid)?, &frame[12 + headers..total - 4]))
}

/// Keeps completed Records payloads and requires S3's terminal End event.
pub async fn read_select(body: ByteStream, records: &mut Vec<Bytes>) -> io::Result<()> {
    let mut frames = aws_frames(body);
    while let Some(frame) = frames.try_next().await? {
        let (event, payload) = select_event(&frame)?;
        match event {
            b"Records" => records.push(Bytes::copy_from_slice(payload)),
            b"End" => return Ok(()),
            b"Stats" | b"Progress" | b"Cont" => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unknown S3 Select event.",
                ))
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "S3 Select ended without an End event.",
    ))
}
```

The one-MiB cap is this application's choice, not a service default. Check the API's limits and adjust it. Process Records payloads as they arrive if retaining all of them would use too much memory. An error or unknown event is the application's error; the consumer drops the body rather than reconnecting or resending.

## A multipart response and its batch

For a finite multipart response, use `multer` on the same `ByteStream`. Take the boundary parameter from the response's `content-type`: `multer::parse_boundary` accepts only `multipart/form-data`, whereas a batch usually returns `multipart/mixed`. This example uses the `mime` crate to read that parameter, including a quoted boundary. Save the code as a module named `multipart_response`; the batch below uses it. The tested file is [multipart_response.rs](../../tests/real-apis/fixtures/recipes/multipart_response.rs).

```rust
use concord::ByteStream;
use std::io;

/// Application errors from either Concord or the MIME parser.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Opens a finite multipart response under a bound on all its wire bytes.
pub fn parts(body: ByteStream, max_bytes: u64) -> Result<multer::Multipart<'static>, BoxError> {
    let label: mime::Mime = body
        .headers()
        .get("content-type")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing multipart content type.",
            )
        })?
        .parse()?;
    if label.type_() != mime::MULTIPART {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "Expected a multipart response.").into(),
        );
    }
    // multer::parse_boundary accepts form-data only; mixed and related need this parameter.
    let boundary = label
        .get_param(mime::BOUNDARY)
        .filter(|value| !value.as_str().is_empty() && value.as_str().len() <= 70)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing or invalid MIME boundary.",
            )
        })?
        .as_str()
        .to_owned();
    let limits = multer::Constraints::new().size_limit(
        multer::SizeLimit::new()
            .whole_stream(max_bytes)
            .per_field(256 * concord::KIB as u64),
    );
    Ok(multer::Multipart::with_constraints(body, boundary, limits))
}
```

Choose a limit for the entire response, including MIME headers and delimiters. Multer may buffer all ready input before it checks a field's limit, so `per_field` alone does not protect memory; `whole_stream` is the essential bound here. This recipe does not supply a separate header-memory limit or a fixed amount of work per poll. Multer can reject a body containing a line that starts with the boundary text without being a complete delimiter; this recipe does not promise acceptance of every valid MIME spelling. Parser error messages are the parser's own and may contain response values: decide what your application logs. A parser may stop at the closing MIME delimiter without reading the HTTP body's remaining epilogue. When cancelling a part read, drop the whole consumer; do not assume that a cancelled `Field::bytes()` retains its temporary collection for another pull.

A batch has two layers of results. Its outer 200 says nothing about the statuses of its embedded HTTP responses. This example builds trusted GET targets with the shipped `multipart_mixed` writer (feature `multipart`), retains inner statuses and duplicate headers, and keeps completed replies in the caller's vector if a later MIME or HTTP part fails. It uses `httparse` for the embedded HTTP head, allows at most 16 KiB and 64 fields there, and refuses transfer-coded replies. The full tested file is [multipart_batch.rs](../../tests/real-apis/fixtures/recipes/multipart_batch.rs).

```rust
use crate::multipart_response::{parts, BoxError};
use concord::{
    bytes::Bytes,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    Answer, Body, ByteStream,
};
use std::io;

concord::api! {
    pub client Gmail(authorization: concord::Secret) {
        base "https://gmail.googleapis.com";
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "batch/gmail/v1"; body request; read concord::read::Stream;
        }
    }
}
concord::api! {
    pub client SharePoint(site: concord::Base, authorization: concord::Secret) {
        base site;
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "_api/$batch"; body request; read concord::read::Stream;
        }
    }
}

/// Builds a read-only batch from trusted content IDs and GET request targets.
pub fn get_batch(targets: &[(&str, &str)]) -> Result<Body, BoxError> {
    let mut requests = Vec::new();
    for (id, target) in targets {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/http"));
        headers.insert("content-id", HeaderValue::from_str(id)?);
        headers.insert(
            "content-transfer-encoding",
            HeaderValue::from_static("binary"),
        );
        let request = Body::new(
            format!("GET {target} HTTP/1.1\r\nAccept: application/json\r\n\r\n"),
            "application/http",
        );
        requests.push((headers, request));
    }
    Ok(concord::multipart_mixed(requests)?)
}

/// A MIME content ID, when present, and a complete embedded HTTP response.
pub type BatchReply = (Option<String>, Answer<Bytes>);

/// Decodes one buffered HTTP response; transfer-coded responses are refused.
pub fn http_response(bytes: Bytes, method: &Method) -> Result<Answer<Bytes>, BoxError> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid batch HTTP response.");
    let end = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .map(|offset| offset + 4)
        .ok_or_else(invalid)?;
    if end > 16 * concord::KIB {
        return Err(invalid().into());
    }
    let mut fields = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut fields);
    let httparse::Status::Complete(offset) = response.parse(&bytes)? else {
        return Err(invalid().into());
    };
    let status = StatusCode::from_u16(response.code.ok_or_else(invalid)?)?;
    if status.is_informational() {
        return Err(invalid().into());
    }
    let mut headers = HeaderMap::new();
    for header in response.headers.iter() {
        headers.append(
            HeaderName::from_bytes(header.name.as_bytes())?,
            HeaderValue::from_bytes(header.value)?,
        );
    }
    if headers.contains_key("transfer-encoding") {
        return Err(invalid().into());
    }
    let payload = bytes.slice(offset..);
    let no_body = method == Method::HEAD
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED;
    if no_body {
        if !payload.is_empty() {
            return Err(invalid().into());
        }
    } else if headers.contains_key("content-length") {
        let mut lengths = headers.get_all("content-length").iter();
        let length: usize = lengths.next().ok_or_else(invalid)?.to_str()?.parse()?;
        if lengths.next().is_some() || length != payload.len() {
            return Err(invalid().into());
        }
    }
    Ok(Answer::new(status, headers.into(), payload))
}

/// Appends completed GET replies; a later MIME or HTTP failure leaves them available.
pub async fn read_batch(
    body: ByteStream,
    completed: &mut Vec<BatchReply>,
    max_bytes: u64,
) -> Result<(), BoxError> {
    let mut response = parts(body, max_bytes)?;
    while let Some(part) = response.next_field().await? {
        if part.content_type().map(mime::Mime::essence_str) != Some("application/http") {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "Expected an HTTP batch part.").into(),
            );
        }
        let id = part
            .headers()
            .get("content-id")
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()?;
        let reply = http_response(part.bytes().await?, &Method::GET)?;
        completed.push((id, reply));
    }
    Ok(())
}

/// Reads two Gmail messages, keeping content IDs for correlation even on failure.
pub async fn gmail_batch(api: &Gmail, completed: &mut Vec<BatchReply>) -> Result<(), BoxError> {
    let start = completed.len();
    let request = get_batch(&[
        ("<one>", "/gmail/v1/users/me/messages/one"),
        ("<two>", "/gmail/v1/users/me/messages/two"),
    ])?;
    let body = api
        .batch(request)
        .with_retry(concord::Retry::none())
        .await?;
    read_batch(body, completed, 4 * concord::MIB as u64).await?;
    let replies = &completed[start..];
    if replies.len() != 2
        || ["response-one", "response-two"].iter().any(|expected| {
            replies
                .iter()
                .filter(|reply| {
                    reply
                        .0
                        .as_deref()
                        .is_some_and(|id| id.trim_matches(['<', '>']) == *expected)
                })
                .count()
                != 1
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Missing or duplicate Gmail reply ID.",
        )
        .into());
    }
    Ok(())
}

/// Reads this SharePoint site's lists and title, keeping the service's reply order.
pub async fn sharepoint_batch(
    api: &SharePoint,
    completed: &mut Vec<BatchReply>,
) -> Result<(), BoxError> {
    let start = completed.len();
    let request = get_batch(&[
        (
            "1",
            "https://tenant.sharepoint.com/sites/demo/_api/web/lists",
        ),
        (
            "2",
            "https://tenant.sharepoint.com/sites/demo/_api/web/title",
        ),
    ])?;
    let body = api
        .batch(request)
        .with_retry(concord::Retry::none())
        .await?;
    read_batch(body, completed, 4 * concord::MIB as u64).await?;
    if completed.len() - start != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unexpected SharePoint reply count.",
        )
        .into());
    }
    Ok(())
}
```

Gmail replies can arrive out of order: correlate each `response-` content ID with the request ID, and require each expected result once. The SharePoint example uses the request order of this read-only batch and an explicit site; adapt its targets and reply rules to your OData service. A nested changeset needs its own MIME decoding and the provider's transaction rules. The helper above does not decode one. A failure to read the rest of a response does not authorize sending completed operations again. The example disables engine retries for the outer POST; replaying a batch is a separate decision about every operation it contains.

A JSON array is read whole into `Vec<T>` with `read Json.limit(…)`; the limit bounds the complete body, and Concord has no incremental JSON reader.

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

**Parts with their own headers: `multipart_mixed` and `multipart_related`.** For a body other than a form, `multipart_mixed(parts)` and `multipart_related(parts)` (feature `multipart`) take pairs of a `HeaderMap` and a `Body` held in memory, in order. Each part has exactly one valid content type, and the first related part is the root, whose type the writer puts in the `type` parameter of the body's content type. The writer makes no `Content-ID` and rewrites no reference: they are yours, in the headers and in the root's bytes:

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

A part that is a stream or without one valid content type is refused with its one-based position, and so is a list with no part; the call then ends with a Build error, and nothing is sent. The rustdoc of `multipart_related` has the whole example, with its description and a call.

**For a body in either form:**

| | |
|---|---|
| The inputs are borrowed | In the expression, an input is a reference: `task` is a `&NewTask`. `json` and `form` take anything that `serde` can serialize, including a `serde_json::Value`. An input that is a `Body` is taken as it is. |
| The content type travels with the bytes | A `Body` carries its `Content-Type`. A `content-type` header that applies to the endpoint takes precedence: `headers { "content-type" = "application/merge-patch+json"; }` with `body json(patch);`. |
| A body that cannot be serialized | The expression may return a `Result`, with any error that converts into `Box<dyn Error + Send + Sync>`. An `Err` is a Build error: nothing is sent, and your writer's error is `e.source()`. |
| One body per endpoint | A body is not inherited from a node above: each endpoint declares how its own body is built. (A part that it names may add entries.) |
| `form` is flat | It writes pairs of strings. For a nested form (`metadata[key]=value`), `bracket_form` (feature `form`) writes `a[b]` for a map and `a[0]` for every list. An API with another convention (Rack and Rails read `a[0]` as a map key) needs your own function that returns a `Body`. |
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
