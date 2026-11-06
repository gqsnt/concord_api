# Dropbox team folder listing

`concord-sdk-dropbox` is an unpublished, read-only Dropbox API v2 slice built with
`concord::api!`. It supports `files/list_folder` and `files/list_folder/continue`,
bearer authentication, and an explicit `Dropbox-API-Select-User` plus
`Dropbox-API-Path-Root` on every request. Home, root and namespace modes are supported.

Use a team-linked token with `team_data.member` and `files.metadata.read`, with
Full Dropbox access for team content. Supply already known member and namespace
IDs. Dropbox still enforces the selected member's permissions. This SDK neither
discovers IDs nor acquires credentials.

Excluded: upload/download, OAuth acquisition/refresh, namespace migration, team
management, Select-Admin, shared-link listing, custom property requests, longpoll,
and persistent change synchronization. Additional metadata fields outside the
inventory model are ignored. Timestamps remain provider strings.

Construct the client once. Configure transport timeouts and disable redirects so
context headers cannot be forwarded to an unrelated destination. The token below
is deliberately synthetic; the examples compile without making live calls.

```no_run
use concord::reqwest;
use concord_sdk_dropbox::{Client, ClientConfig, ListFolderOptions, PathRoot, RequestContext};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(20))
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let client = Client::new(http, ClientConfig {
    token: "SYNTHETIC_DROPBOX_TOKEN_NOT_A_CREDENTIAL".into(),
});

// Workflow A: exhaust one member's folder, retaining metadata page by page.
let member_a = RequestContext::new("dbmid:SYNTHETIC_MEMBER_A", PathRoot::root("100")?)?;
let mut pages = client.list_folder_pages(member_a, ListFolderOptions::new("/Projects"));
while let Some(page) = pages.next_page().await? {
    println!("request {:?}, {} entries", page.meta.http.request_id(), page.items.len());
}
# Ok(()) }
```

Workflow B runs two explicitly selected contexts against the same client. Changing
the local selection does not modify a client, pending request, traversal or cursor.

```no_run
# use concord_sdk_dropbox::{Client, RequestContext, PathRoot, ListFolderOptions};
# async fn concurrent(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
let mut selected = RequestContext::new("dbmid:SYNTHETIC_MEMBER_A", PathRoot::root("100")?)?;
let first = client.list_folder_pages(selected.clone(), ListFolderOptions::new("/Projects"));
selected = RequestContext::new("dbmid:SYNTHETIC_MEMBER_B", PathRoot::namespace("200")?)?;
let second = client.list_folder_pages(selected, ListFolderOptions::new("/Projects"));
let (a, b) = tokio::join!(
    first.items().collect_complete(),
    second.items().collect_complete(),
);
let a = a?;
let b = b?;
println!("A: {}, B: {}", a.len(), b.len());
# Ok(()) }
```

For one page use `client.list_folder(context, options).await?`, then
`client.list_folder_continue(&response.value.cursor).await?` while `has_more` is
true. Cursors retain their original context and cannot be constructed from raw
strings. Use the same token/app when continuing; cursors do not bind credentials.
Paths are JSON strings, never URL paths: pass `""` for the selected root, or raw
Unicode paths without percent encoding. `PageSize::new` validates 1–2000; the
provider's page size is approximate. Options are sent only on the initial request.

`FolderPages::next_page()` returns `Ok(None)` only after a decoded page with
`has_more: false`. Empty intermediate pages continue. The default cap is 1000
requests; `.max_pages(nonzero_limit)` changes it. A cap produces
`pagination::PagesError::Bounded`, and a failed request produces `Provider` with
the original SDK error and failed continuation. Calls after either failure return
`Ended`, never a false successful completion. `items().collect_complete()` returns
the collected prefix in `Incomplete` on failure; it is not a completed inventory.
`items().into_stream()` is a standard Rust stream and yields the typed error before
ending. Items discard page metadata; use pages when request IDs matter. Dropping
a traversal cancels its pending work and performs no further requests; dropped
work is not completion. Entry order and deletions are preserved, without deduplication
or claims of an atomic snapshot across pages.

`Response<T>` retains status, headers and generated operation identity. `Error::Http`
retains those same fields plus a bounded raw body and optional typed provider error.
Match `ProviderErrorKind::Tagged(TaggedError::Path { path: LookupError::NotFound })`,
`Reset`, `InvalidRoot`, `InvalidSelectUser` or `MissingScope` as appropriate.
Unknown errors retain their original JSON. Never match `error_summary` exactly:
Dropbox may vary its suffix. An invalid root is reported, never silently changed.
HTTP 429 stays an HTTP failure even if its body is unreadable; inspect
`error.meta().map(|m| m.retry_after())`. The header interpretation distinguishes
missing, invalid and valid zero seconds; a structured body delay is separate.

Each request executes once: no automatic retry, sleep, credential refresh or
background task. Success JSON is limited to 16 MiB; error bodies to 64 KiB. On
limit/interrupted error bodies the status/request ID survives and `read_error()`
explains the missing body. Invalid content type, malformed JSON, missing required
fields and unknown entry tags fail with metadata. Even a terminal page requires a
nonempty cursor. Diagnostic formatting omits credentials, contexts and raw error
bodies; explicit accessors and provider values are unredacted.

Tests use only the loopback `examples/testserver` and synthetic credentials.
Fixtures are hand-authored from the linked official documentation, **not vendor
conformance evidence**. See `PROVENANCE.md` for source dates and transformations,
and `FRICTION.md` for author observations. The optional real-service workflow needs
a suitable authorized team app and users; no live calls were made for this SDK.
