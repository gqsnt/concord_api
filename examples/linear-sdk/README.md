# Linear issue SDK slice

A read-only, offline-tested Concord client for Linear's GraphQL issue connection and issue by ID or identifier. It selects `id`, `title`, and nullable `description`; the connection selects `nodes` and forward `pageInfo`. It uses fixed query documents and JSON variables. Mutations, schema introspection and generation, other GraphQL operations, filtering, backward paging, credential acquisition, retry and rate-limit policy are outside this slice. This is not a general GraphQL client.

Construct a `Linear` with an application-configured `concord::reqwest::Client`. `api_key` sends its argument verbatim in `Authorization`; `oauth` sends it as `Bearer`. Configure timeouts, proxies, and redirect behavior in the transport. Treat credentials and response headers as sensitive; the SDK does not obtain or refresh tokens. For tests or a trusted gateway, `with_base_url` changes the endpoint's base (the `/graphql` path is still appended).

```rust,no_run
use concord::reqwest;
use concord_sdk_linear::Linear;
use concord_pagination::Resume;
use std::num::NonZeroUsize;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let client = Linear::api_key(http, "synthetic-value")?;

// Workflow A: inspect each page, including any GraphQL errors.
let mut pages = client.issues(50, Resume::Start)
    .max_pages(NonZeroUsize::new(20).unwrap());
while let Some(page) = pages.next_page().await? {
    if !page.meta.errors.is_empty() {
        // Application policy: reject a partial page or explicitly use it.
        eprintln!("partial page: {} error(s)", page.meta.errors.len());
    }
    for issue in page.items { println!("{}: {}", issue.id, issue.title); }
}

// Workflow B: the answer may contain both data and diagnostics.
let answer = client.issue("ABC-1").await?;
if answer.is_success() {
    if let Some(issue) = answer.envelope.data.and_then(|d| d.issue) {
        println!("{}: {}", issue.id, issue.title);
    }
} else {
    eprintln!("partial or missing data: {:?}", answer.envelope.errors);
}
# Ok(())
# }
```

A successful HTTP response is decoded as an envelope, **not** automatically declared an application success. `Response::is_success` requires data and no GraphQL errors; it does not assert that a nullable `issue` exists. The complete envelope keeps errors, paths, locations, extensions and data together. A connection with data and errors is returned as a partial `Page` with errors in `PageMeta`; consumers must inspect those diagnostics before accepting it. A missing connection or invalid next cursor returns `Error::Page` with the entire response. Selected nullable fields must still be present in the JSON, even when null.

`Pages::next_page()` returns `Ok(None)` only on provider exhaustion. Caps and failed pages yield `PagesError`, retaining the resume point; `collect_complete()` retains pages already collected if incomplete. Do not use the item-flattening adapter when page diagnostics matter: it discards page metadata. An immediate non-advancing cursor and a missing next cursor are rejected. Longer cursor cycles are not detected; set a page cap. A resume cursor must belong to the same collection and page-size context. Linear's pagination can change as issues change, so traversal is not a snapshot.

`Error::Http` retains the status, headers, raw bounded error body and any parsed GraphQL errors; `Error::Response` retains metadata and a Concord error for body, format or decoding failures. Pre-response failures are `Error::Concord`. Successful JSON bodies are limited to 1 MiB and non-2xx bodies to 64 KiB; exceeding a limit is a response error, not truncated success. No retries are performed. `first` is restricted by this SDK to 1–250; this is an SDK policy, not a claim about the current provider maximum. Missing `data` and `data: null` are both represented as `None`.

All fixtures are hand-authored synthetic examples derived from documentation shapes; they are not provider conformance tests. No live provider or credentials are used. See [PROVENANCE.md](PROVENANCE.md) and [FRICTION.md](FRICTION.md).
