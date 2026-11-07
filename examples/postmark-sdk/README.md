# Postmark SDK slice

A small Concord SDK for single transactional email submission and outbound message search and details. The server token is sent in `X-Postmark-Server-Token`. The default base is `https://api.postmarkapp.com`; `with_base_url` is useful for local fixtures. Configure an HTTP timeout; use the provided builder to disable transport retries and redirects. Do not re-enable either, or configure an independent client to replay requests. Generated `prepare()` and native transport escapes bypass the SDK policy.

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

The slice excludes inbound parsing, webhooks, batch sending, attachments and other optional email fields, search filters, and delivery guarantees. Response bodies are limited to 16 MiB; error bodies to 64 KiB. All tests use loopback fixtures; live provider calls are **NOT_RUN**.
