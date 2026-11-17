# Recipes

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [When Concord does not cover what you need](extending.md) · next: [Troubleshooting](troubleshooting.md)*

This page is for a need that Concord meets with no value of its own: the description, an extension point and another crate do the job, and a recipe is the code that joins them. A shorter recipe stays in the section of its subject, and the longer ones are on this page; the index below lists them all.

Every recipe is complete code, compiled and run by the tests of the repository. Each test includes the same file as the page, runs it against a scripted transport, and compares the block in the page with the file word for word, so a block cannot drift from the code that is tested. The line under each block names its file and its test.

| Need | Section | Crates besides Concord | Tested file |
|---|---|---|---|
| A new idempotency key for each write | [A key made for one write](retries.md#a-key-made-for-one-write) | `uuid` (feature `v4`) | [write_key.rs](../../tests/real-apis/fixtures/recipes/write_key.rs) |
| A provider's own HMAC proof | [A signature](credentials.md#a-signature) | `hmac`, `sha2`, `hex`; the feature `sign` of `concord-api` | [credential_signing.rs](../../tests/real-apis/src/credential_signing.rs) |
| A large upload from a `Body::stream`, sent again after a failure | [A body that is a stream](bodies.md#a-body-that-is-a-stream) | `tokio` (feature `fs`), `tokio-util` (feature `io`) | [upload_stream.rs](../../tests/real-apis/fixtures/recipes/upload_stream.rs) |
| A download that the API documents as a redirect | [A documented download redirect](extending.md#a-documented-download-redirect) | none | [redirect.rs](../../tests/real-apis/fixtures/recipes/redirect.rs) |
| Docker logs, or AWS event-stream messages | [Binary frames](#binary-frames) | `tokio-util` (features `codec` and `io`), `futures-util`, `crc32fast` | [docker_frames.rs](../../tests/real-apis/fixtures/recipes/docker_frames.rs), [aws_frames.rs](../../tests/real-apis/fixtures/recipes/aws_frames.rs) |
| A multipart response, and a batch of requests | [A multipart response and its batch](#a-multipart-response-and-its-batch) | `multer`, `mime`, `httparse`; the feature `multipart` of `concord-api` | [multipart_response.rs](../../tests/real-apis/fixtures/recipes/multipart_response.rs), [multipart_batch.rs](../../tests/real-apis/fixtures/recipes/multipart_batch.rs) |
| The same read, sent to several hosts | [Several hosts](#several-hosts) | `tokio` (feature `time`) | [several_hosts.rs](../../tests/real-apis/fixtures/recipes/several_hosts.rs) |

## Binary frames

A stream of binary frames is read with `read concord::read::Stream;` and a codec for the wire format. `tokio_util::io::StreamReader` turns a `ByteStream` into an `AsyncRead`, and `LengthDelimitedCodec` splits it into frames that each begin with their length. Through this bridge, Concord's call timeout and idle timeout still apply, and a failure of the body is still a `StreamError`, inside an `io::Error`, with its status, its headers and its attempt number. The bridge never sends a request again.

These examples use `tokio-util` with its features `codec` and `io`, `futures-util` for `TryStreamExt`, and `crc32fast` for the checksums of AWS.

Docker's logs without a TTY begin each frame with eight bytes: a channel, three reserved bytes, and the length of the payload, big-endian. The codec's bound applies to the length field before the adjustment, so this codec accepts at most 64 KiB of payload, plus the eight bytes of the prefix. Logs with a TTY are raw bytes and need no codec.

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

Tested: [`docker_frames.rs`](../../tests/real-apis/fixtures/recipes/docker_frames.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

`read_logs` reads whole frames and pushes their channel and payload into a vector that the caller owns; if keeping every log would use too much memory, process or discard each record as it arrives. The head of the response stays readable through `frames.get_ref().get_ref().status()` and `.headers()`. A frame pull that is dropped while it is pending leaves the bytes that the codec has buffered in the codec. The bridge polls the `ByteStream` as a `Stream`, so a pending idle timeout keeps counting between two polls, even when an outer pull future is dropped. To release the body, drop the whole codec. These adapters do not promise a fixed amount of work in one poll, and the bytes or values that the application keeps take memory of their own.

An AWS event-stream message carries its total length in its first four bytes. With no byte skipped and a length adjustment of zero, the codec keeps the whole frame, both checksums included. The prelude is checked after the codec has read that bounded frame, not before its payload is buffered. This S3 Select consumer pushes the payloads of complete `Records` events into a vector that the caller owns, and accepts the end of the stream only after an `End` event: a body that ends between two complete frames is still an error of the application when no `End` arrived. The `authorization` input stands for the signature of this request: sign it with the API's signing code, as in [the S3 example](../examples/s3.md).

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

Tested: [`aws_frames.rs`](../../tests/real-apis/fixtures/recipes/aws_frames.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

The cap of one MiB is this application's choice, not a limit of the service: check the API's limits and adjust it. If keeping every `Records` payload would use too much memory, process each one as it arrives. An error event or an unknown event is an error of the application: the consumer drops the body, and neither reconnects nor sends the request again.

## A multipart response and its batch

A finite multipart response is read with `multer`, on the same `ByteStream`. The boundary is a parameter of the response's `content-type`. `multer::parse_boundary` accepts only `multipart/form-data`, and a batch usually returns `multipart/mixed`, so this example reads the parameter with the `mime` crate, which also accepts a quoted boundary. Save the code as a module named `multipart_response`: the batch below uses it.

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

Tested: [`multipart_response.rs`](../../tests/real-apis/fixtures/recipes/multipart_response.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

Choose a limit for the whole response, MIME headers and delimiters included. `multer` may buffer all the input that is ready before it checks the limit of a field, so `per_field` alone does not protect memory: `whole_stream` is the bound that does. This recipe sets no separate limit on the memory of the headers, and does not promise a fixed amount of work in one poll. `multer` can reject a body with a line that begins with the boundary text without being a complete delimiter, so this recipe does not promise to accept every valid MIME spelling. The parser's error messages are its own and may contain values from the response: decide what your application logs. A parser may stop at the closing delimiter, without reading the rest of the HTTP body (its epilogue). To cancel the reading of a part, drop the whole consumer: do not count on a cancelled `Field::bytes()` to keep what it had collected for the next pull.

A batch has two levels of results: its outer `200` says nothing about the statuses of the HTTP responses inside it. This example builds `GET` requests to trusted targets with the shipped writer `multipart_mixed` (feature `multipart`). It keeps the status and every repeated header of each inner response, and when a later MIME or HTTP part fails, the replies already complete stay in the caller's vector. It reads the head of each inner response with `httparse`, accepts at most 16 KiB and 64 header fields there, and refuses a reply with a transfer coding.

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

Tested: [`multipart_batch.rs`](../../tests/real-apis/fixtures/recipes/multipart_batch.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

Gmail may send its replies in any order: match each `response-` content ID with the ID of its request, and require each expected reply exactly once. The SharePoint example relies on the order of the requests in this read-only batch, and on an explicit site: adapt its targets and its rules for replies to your OData service. A nested changeset needs MIME decoding of its own and the provider's rules for transactions; the helper above does not decode one. A failure to read the rest of a response is no reason to send the operations that completed again. The example turns off the engine's retries for the outer `POST`: sending a batch again is a separate decision about every operation in it.

## Several hosts

Use a separate client for each origin that you trust, with the credentials declared for that client. `with_base` moves every entry, secrets included, because the caller chose it explicitly, and `at_url` cannot move a call to another origin. A transport that rewrites the host can get around the credential checks, or break a signature. Concord has no pool of hosts, no way to find hosts and no automatic failover.

The application's loop decides which reads may be repeated, which failures justify trying the next host, and in which order the hosts are tried. Turn off the engine's retries on each call and put one timeout around the whole loop: otherwise each call starts its own count of attempts and its own call timeout. This example repeats an Algolia search or an Elasticsearch search, never a write of a document. It moves to the next client, each one constructed on its own, only after a failed connection or a `502`, `503` or `504`; an authentication failure stops the loop. When the response to a write is lost, nothing proves that the write was not applied.

```rust
use concord::{error::Class, Retry};
use std::{io, time::Duration};

concord::api! {
    pub client Algolia(origin: concord::Base, application: String, key: concord::Secret) {
        base origin;
        headers { "x-algolia-application-id" = application; "x-algolia-api-key" = key; }
        POST search() -> serde_json::Value {
            path "1/indexes/books/query";
            body concord::json(serde_json::json!({"query": "rust"}));
            idempotent; read concord::Json;
        }
    }
}
concord::api! {
    pub client Elasticsearch(origin: concord::Base, authorization: concord::Secret) {
        base origin;
        headers { "authorization" = authorization; }
        GET search() -> serde_json::Value { path "books/_search"; read concord::Json; }
    }
}

/// Application errors retain the last call failure or the overall timeout.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn next_host(error: &concord::Error) -> bool {
    error.class() == Some(Class::Connect)
        || matches!(
            error.status().map(|status| status.as_u16()),
            Some(502..=504)
        )
}

/// Repeats this read over separately authorized clients, within one overall timeout.
pub async fn algolia(clients: &[Algolia], budget: Duration) -> Result<serde_json::Value, BoxError> {
    tokio::time::timeout(budget, async {
        let mut last: BoxError = io::Error::new(
            io::ErrorKind::InvalidInput,
            "No Algolia host was configured.",
        )
        .into();
        for host in clients {
            match host.search().with_retry(Retry::none()).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    let again = next_host(&error);
                    last = error.into();
                    if !again {
                        break;
                    }
                }
            }
        }
        Err(last)
    })
    .await?
}

/// Repeats an Elasticsearch search; it does not accept a document write.
pub async fn elasticsearch(
    clients: &[Elasticsearch],
    budget: Duration,
) -> Result<serde_json::Value, BoxError> {
    tokio::time::timeout(budget, async {
        let mut last: BoxError = io::Error::new(
            io::ErrorKind::InvalidInput,
            "No Elasticsearch host was configured.",
        )
        .into();
        for host in clients {
            match host.search().with_retry(Retry::none()).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    let again = next_host(&error);
                    last = error.into();
                    if !again {
                        break;
                    }
                }
            }
        }
        Err(last)
    })
    .await?
}
```

Tested: [`several_hosts.rs`](../../tests/real-apis/fixtures/recipes/several_hosts.rs), run by [`stream_recipes.rs`](../../tests/real-apis/tests/stream_recipes.rs), which compares this block with the file.

Construct each client with a base that you trust and the credential authorized for it, before you call the loop. The read hosts of an Algolia application and the node addresses of an Elasticsearch deployment are chosen by the provider and by the deployment: never turn an address that was found at run time, and that you do not trust, into a destination for an existing key.
