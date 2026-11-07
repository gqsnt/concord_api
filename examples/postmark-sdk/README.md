# Postmark SDK slice

A small Concord SDK for transactional email submission (single and batch) and outbound message search and details. The server token is sent in `X-Postmark-Server-Token`. The default base is `https://api.postmarkapp.com`; `with_base_url` is useful for local fixtures. Configure an HTTP timeout; use the provided builder to disable transport retries and redirects. Do not re-enable either, or configure an independent client to replay requests. Generated `prepare()` and native transport escapes bypass the SDK policy.

```rust,no_run
use concord_sdk_postmark::{Email, Postmark, PostmarkConfig, http_client_builder};
use std::time::Duration;

# async fn example(token: String) -> Result<(), Box<dyn std::error::Error>> {
let http = http_client_builder().timeout(Duration::from_secs(10)).build()?;
let api = Postmark::new(http, PostmarkConfig { token });
let outbound = api.outbound();
let acceptance = outbound.send_email(Email {
    from: "sender@example.com".into(),
    to: "receiver@example.com".into(),
    subject: "Hello".into(),
    text_body: Some("Hello".into()),
    html_body: None,
    message_stream: None,
}).send().await?;
let details = outbound.details(&acceptance.message_id).send().await?;
let page = outbound.list(50, 0).send().await?;
if let Some(item) = page.messages.first() {
    let details = outbound.details(&item.message_id).send().await?;
    println!("{}: {}", details.message_id, details.status);
}
println!("{}: {}", details.message_id, details.status);
# Ok(()) }
```

An acceptance is not delivery. Inspect `MessageEvents` in the details' `fields` for provider activity. Postmark enforces sender and payload rules, as well as page bounds (count at most 500; count plus offset at most 10,000); this SDK sends the caller's inputs and reports the provider's validation errors. One `list` call fetches one page. Search results and message activity can change over time and expire under the server's retention policy.

`Error::UnknownOutcome` means a send was executed but no usable success response was obtained, including timeout, disconnect, malformed, oversized or wrong-media 2xx responses. The email may have been accepted: investigate before any manual resubmission. A non-2xx response is `Error::Api`, preserving status, a bounded JSON document when possible, and integer `Retry-After` seconds when provided. A server-side failure may also leave send outcome uncertain; status alone is not a delivery guarantee. Reads and request-build failures use `Error::Request`. Errors and client debug output hide credentials and error-document message text; accessing the document itself exposes the provider's text. No automatic retry is applied by the policy.

## Batch sending

`send_batch` submits several emails in one `POST /email/batch`. Postmark answers a readable batch with HTTP 200 even when some messages fail validation, with one result per submitted email in submission order: the `n`th `BatchResult` belongs to the `n`th email. `BatchResult::Accepted` holds the `Acceptance` (its `message_id` works with `details`; acceptance is not delivery); `BatchResult::Rejected` holds the provider's `ErrorCode` in an `ErrorDocument` whose message text, which can name recipient addresses, is omitted from `Debug`.

```rust,no_run
use concord_sdk_postmark::{BatchResult, Email, Postmark};

# async fn example(api: Postmark, emails: Vec<Email>) -> Result<(), concord_sdk_postmark::Error> {
let outbound = api.outbound();
match outbound.send_batch(emails).send().await {
    Ok(results) => {
        for (position, result) in results.iter().enumerate() {
            match result {
                BatchResult::Accepted(acceptance) => {
                    let details = outbound.details(&acceptance.message_id).send().await?;
                    println!("email {position}: {}", details.status);
                }
                BatchResult::Rejected(rejection) => {
                    println!("email {position} rejected with code {}", rejection.error_code);
                }
            }
        }
    }
    // Any email of the batch may have been accepted: investigate, never resend automatically.
    Err(error) if error.is_unknown_outcome() => eprintln!("{error}"),
    Err(error) => return Err(error),
}
# Ok(()) }
```

The whole batch has an unknown outcome (`Error::is_unknown_outcome`) when its response is lost or unreadable — `Error::UnknownOutcome` for a timeout, disconnect, malformed, oversized or wrong-media 2xx, or an entry with `ErrorCode` 0 but no `MessageID`, `To` or `SubmittedAt` — and `Error::UnmatchedBatch` when a 2xx holds a different number of results than emails submitted, so none can be attributed (the returned results are kept for investigation). The SDK never resends a batch. Postmark enforces at most 500 messages and 50 MB per batch (`ErrorCode` 410 or HTTP 413); the SDK sends the caller's list unchanged and reports such a refusal, like any non-2xx, as `Error::Api` for the whole batch.

The slice excludes inbound parsing, webhooks, attachments and other optional email fields, search filters, and delivery guarantees. Response bodies are limited to 16 MiB; error bodies to 64 KiB. All tests use loopback fixtures; live provider calls are **NOT_RUN**.
