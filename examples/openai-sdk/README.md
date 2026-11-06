# OpenAI SDK slice

A non-published, offline-tested Rust client for Responses create (ordinary and streaming), Files batch upload, file metadata and content, and Batch create/retrieve/poll. It uses Concord's public API; HTTP execution is not delegated to an OpenAI SDK. Realtime, other endpoints, model evaluation, automatic retries, and paid calls are outside scope. The fixtures are synthetic and are **not** vendor conformance tests.

Construct with a caller-configured Concord re-export of reqwest. Choose timeouts, proxy and redirect policy appropriate for your application; the authorization header is sensitive. `with_base_url` changes the trusted origin (the `/v1` path remains), so only use a destination trusted with the token. The examples use a clearly synthetic token and perform no live requests on their own.

## Responses

```rust,no_run
use concord::reqwest;
use concord_sdk_openai::{Event, OpenAi, ResponseRequest};
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let client = OpenAi::new(http, "synthetic-token")?;
let request = ResponseRequest::new("fixture-model", "Hello");
let ordinary = client.create_response(request.clone()).await?;
println!("response {}: {:?}", ordinary.value.id, ordinary.value.status);
let mut events = client.stream_response(request).await?;
while let Some(event) = events.next().await? {
    match event {
        Event::OutputTextDelta { delta } => print!("{delta}"),
        Event::Completed { response } => println!("completed {}", response.id),
        _ => {}
    }
}
# Ok(()) }
```

A `Completed` event is the stream's successful terminal condition; EOF before it, a failed response, an `error` event, malformed data, and a transport failure are errors. Events delivered before a failure remain with the caller. Drop the stream or cancel the future waiting for the next event to stop local consumption; this does not cancel provider-side work. The parser buffers at most 1 MiB per pending event/chunk, accepts LF and CRLF lines and multiline `data`, and ignores comments and SSE fields other than `data`. The selected common events are typed; other event kinds are represented by `Event::Other` without their payload. The batch itself is not an SSE stream.

## Batch

```rust,no_run
use std::time::Duration;
use concord::reqwest;
use concord_sdk_openai::{batch_jsonl, BatchInput, BatchRequest, OpenAi, ResponseRequest};
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = OpenAi::new(reqwest::Client::new(), "synthetic-token")?;
let rows = vec![
    BatchInput::response("one", ResponseRequest::new("fixture-model", "Hello")),
    BatchInput::response("two", ResponseRequest::new("fixture-model", "Goodbye")),
];
let jsonl = batch_jsonl(&rows)?;
let input = client.upload_batch_file("input.jsonl", &jsonl).await?.value;
let batch = client.create_batch(BatchRequest::responses(input.id)).await?.value;
let batch = client.poll_batch(&batch.id, 10, Duration::from_secs(30)).await?.value;
let results = client.batch_results(&batch, 10 * 1024 * 1024).await?;
for (custom_id, item) in &results.items {
    println!("{custom_id}: response={:?}, error={:?}", item.response, item.error);
}
for problem in &results.problems { eprintln!("row: {problem}"); }
for (file_id, error) in &results.file_errors { eprintln!("file {file_id}: {error}"); }
# Ok(()) }
```

Polling uses a caller-provided attempt cap and delay; dropping the future cancels local polling. `PollLimit` contains the last observed batch. Terminal `failed`, `expired`, and `cancelled` batches are returned as data, with available output/error file IDs and provider errors. They do not imply all items succeeded. `batch_results` reads both available files independently, retaining successful rows even if the other file fails; malformed rows and duplicate IDs are reported separately. Correlation is by `custom_id`, never row position. Download limits are per file. `content` returns raw bytes and `file` returns metadata. Upload and file download buffer in memory. Input serialization rejects duplicate or empty IDs and streaming requests; the caller is responsible for other provider constraints such as a single model per batch and total size. The fixed multipart boundary is checked against file bytes; unsafe filenames are rejected.

Every successful operation returns HTTP status, headers and Concord operation identity in `Response::meta`. `Error::Provider` retains status and headers and, when available, the typed error document; missing, malformed, unreadable or oversized (over 64 KiB) error bodies yield `None`. Decode errors after a response head retain metadata. A successful HTTP response can still report a failed response or batch. Headers, body fields and debug/error output can contain sensitive data; handle them accordingly. IDs and JSON payloads are deliberately only partially modeled; inspect the `Value` fields for additional provider content. No retries are performed, and a failed or cancelled write may already have reached the server.
