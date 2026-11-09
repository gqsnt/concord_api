# concord-sdk-notion

A read-only Notion SDK slice built from scratch with `concord::api!`. The default
`Notion-Version` is **2026-03-11**, checked against the three endpoint references on
**2026-11-06**. Version overrides are explicit and do not migrate the schema.

Supported operations:

- `query_data_source` / `query_pages`: POST a data source query, optionally using a
  select property's `equals` filter, then follow body cursors without changing the filter.
- `retrieve_page`: retrieve a page and its present property values.
- `list_block_children` / `children`: paginate one level of a page's or block's children.

Typed properties: title, rich_text, number, checkbox, select and date. Typed blocks:
paragraph, heading_1/2/3, bulleted_list_item, to_do and divider. Rich text supports
text with optional links and equation expressions. The renderer produces a plain-text
outline, drops styling, retains links in the model, and never follows them.

Unknown property, block and rich-text variants keep their entire JSON object in
`Unknown { kind, raw }` and render an explicit `[unsupported …: type]` marker. This
preserves parsed JSON values, not original whitespace or object-key order. Malformed
supported types fail decoding. Non-page wiki query results remain `QueryResult::Other`.
New discriminators need no SDK registration to be retained as unknowns. An unknown
block remains an item in provider order, including on later pages, and its
`has_children` flag still lets callers expand it. Common identity fields remain required.

Excluded: mutations, general filters/sorts, data source schema retrieval, full rich-text
and property schemas, property-item pagination, comments, Views, and automatic API
version migration. Known payloads intentionally omit unmodeled metadata and styling.

## Construction

Depend on `concord-sdk-notion` by path in this workspace. All auth below is synthetic.
Supply your own transport; the SDK re-exports Concord's reqwest types.

```rust
use concord_sdk_notion::{NotionClient, reqwest};

let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(20))
    .build()?;
let notion = NotionClient::new(http, "synthetic-notion-token");
// For offline fixtures only, include the API prefix:
let local = notion.clone().with_base_url("http://127.0.0.1:12345/v1")?;
assert!(!format!("{local:?}").contains("synthetic-notion-token"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Every declared request receives bearer auth and a version header from its owned
context. `client.clone().with_version(...)` changes that clone only; concurrent
clients can share the connection pool. Synthetic tests check version/auth isolation.
Lazy traversals capture the client's context when created: even if the originating
client is later consumed by `with_version`, every page of the existing traversal
keeps its captured version. Response version headers do not reconfigure the client.
An actual integration would need read-content capabilities and access to the selected
pages and data source's parent database. No credentials or live provider calls were used.

## Workflow A: query pages, then retrieve a selection

```no_run
use std::num::NonZeroUsize;
use concord_sdk_notion::{NotionClient, PageSize, Query, SelectEquals, render};

async fn selected_page(notion: &NotionClient) -> Result<(), Box<dyn std::error::Error>> {
    let query = Query {
        filter: Some(SelectEquals::new("Priority", "Done")),
        page_size: PageSize::new(50)?,
    };
    let results = notion.query_pages("synthetic-data-source-id", query)
        .max_pages(NonZeroUsize::new(20).unwrap())
        .items().collect_complete().await?;

    // Wiki queries may also return data sources; inspect Other when relevant.
    if let Some(page) = results.iter().find_map(|result| result.as_page()) {
        let selected = notion.retrieve_page(page.id.clone()).await?;
        for (name, value) in &selected.value.properties {
            println!("{name}: {}", render::property(value));
        }
        // selected.meta retains status, HTTP version, headers and operation identity.
    }
    Ok(())
}
```

Page retrieval does **not** retrieve block content. It also does **not** promise full
property values: relations, people, rich text and titles can omit references, and
formula/rollup results can be incomplete. The property-item endpoint needed to finish
those values is outside this slice. Unknown relation payloads retain `has_more`, but
there is no universal property-completeness flag, so absence of a flag proves nothing.

## Workflow B: read paginated children and render a tree

This example expands `has_children` on supported and unknown parents. Its page, depth,
and total-block budgets are application policy; errors preserve already printed output
as partial output. A staged export should publish only after successful completion.

```no_run
use std::{collections::HashSet, num::NonZeroUsize};
use concord_sdk_notion::{NotionClient, PageSize, render};
use concord_sdk_notion::models::Block;

async fn render_tree(notion: &NotionClient, page_id: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    enum Work { Children(String, usize), Render(Block, usize) }
    let mut work = vec![Work::Children(page_id.to_owned(), 0)];
    let mut visited = HashSet::new();
    let mut rendered = 0;
    while let Some(next) = work.pop() {
        match next {
            Work::Children(id, depth) => {
                if depth > 10 || !visited.insert(id.clone()) {
                    return Err("tree depth limit or repeated parent".into());
                }
                let children = notion.children(id, PageSize::new(50)?)
                    .max_pages(NonZeroUsize::new(20).unwrap())
                    .items().collect_complete().await?;
                for child in children.into_iter().rev() {
                    work.push(Work::Render(child, depth));
                }
            }
            Work::Render(block, depth) => {
                if rendered == 10_000 { return Err("block budget reached".into()); }
                println!("{}{}", "  ".repeat(depth), render::block(&block));
                rendered += 1;
                if block.has_children {
                    work.push(Work::Children(block.id, depth + 1));
                }
            }
        }
    }
    Ok(())
}
```

## Errors, completion and partial outcomes

Successful single operations return `Response<T>` with decoded content and Concord's owned
`ResponseMeta`. `Error::Provider` retains a typed `ErrorCode`, message, body request ID,
additional_data, and actual response metadata. Unknown error codes keep their spelling.
Headers such as `Retry-After`, `x-request-id` and `Notion-Version` are available unchanged
when sent. Errors never invent a status for build/connect failures. No automatic retry
or rate-limit wait occurs.

`Error::Api` retains the original Concord error and, once received, metadata through
JSON, body-read, media-type and limit failures. A malformed non-2xx error document is
`InvalidErrorBody` with bounded raw bytes. Error and response Debug omit body content
and header values; model fields, raw bytes, and explicitly accessed headers are unredacted.

```no_run
use concord_sdk_notion::{NotionClient, Error, ErrorCode};

async fn inspect(notion: &NotionClient) -> Result<(), Box<dyn std::error::Error>> {
    match notion.retrieve_page("synthetic-page-id").await {
        Ok(response) => println!("page {}", response.value.id),
        Err(Error::Provider { error, meta }) if error.code == ErrorCode::RateLimited => {
            let retry_after = meta.headers().get("retry-after");
            // Application decides whether/when to retry. No request is repeated here.
            println!("rate limited; retry guidance present: {}", retry_after.is_some());
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
```

- `next_page()` / item `next()` return `Ok(None)` only after a validated last page.
  Empty pages with cursors continue. Inconsistent or repeated cursors fail explicitly.
- `.max_pages(...)` is opt-in and counts logical page attempts. Reaching it with work
  remaining is `PagesError::Bounded`, followed by `PagesError::Ended` on later calls.
- `.collect_complete()` returns a complete collection or `Incomplete { collected,
  error }`; the collected prefix is retained. The failed page is not emitted.
- A query response marked `request_status.type != "complete"` is
  `Error::IncompleteQuery`, even if `has_more` is false. Its current page and status
  remain inside the error, in addition to the traversal's previously collected prefix.
  Absent request_status is accepted; an unfamiliar supplied status is refused.
- `.items().into_stream()` streams items **between fully decoded JSON pages**; it does
  not incrementally decode a JSON response. An error is yielded before stream termination.
  A stream's final `None` after an error does not imply success.
- Page metadata is available from `next_page()`; flattening to items discards page metadata.
  A page checkpoint or `PagesError::resume()` can be passed to `query_pages_from` or
  `children_from`. Reuse the same operation, parent, auth/version, filter and page size.
  Item checkpoints include an offset and cannot be passed directly to these methods.
- Dropping a traversal cancels its owned in-flight future. Dropping only a pending
  `next_page` future leaves the request attached to the traversal. Nothing is spawned.
  Early abandonment is partial work, never a successful collection.

Successful JSON bodies are limited to 8 MiB and provider errors to 64 KiB, counted
after transport decompression. Each JSON page is buffered; complete collections can
grow with the number of items. Cursor cycle detection keeps one key per attempted
cursor. Use page/tree budgets for resource limits. Concurrent edits can change results
between cursor requests: the SDK supplies no snapshot or deduplication guarantee.

`client.api()` exposes the public generated operations for request preparation and
Concord's native boundary. The 8 MiB success-body bound is declared on the client, so
direct generated `.send()` and `prepare()?.send()` refuse a larger body too
(`ErrorKind::Limit`); they return Concord errors. The 64 KiB provider-error bound,
provider-error mapping and envelope checks apply to the `NotionClient` domain methods
above.

## Evidence and dependencies

All tests run against `examples/testserver` on loopback. Fixtures are hand-authored
from the official docs and include synthetic perturbations; they are **not vendor
conformance**. See [PROVENANCE.md](PROVENANCE.md) and [FRICTION.md](FRICTION.md).
The [maintenance tests](tests/maintenance.rs) add fresh unknown property/block types
and a second children page with all supported variants preserved. Server gates force
versioned clones to overlap on both pages. The tests also cover exact opaque cursor
round trips, retained links, expansion of an unknown parent, cap/resume, typed provider
failure, and a malformed supported block after an unknown block. See
[MAINTENANCE.md](MAINTENANCE.md) for the change record; existing callers need no changes.
No vendor SDK, signer or additional HTTP stack is used. `serde`/`serde_json` handle
JSON and retained unknown variants; `concord-pagination` supplies lazy traversal,
streams, caps, checkpoints and partial outcomes. The GitHub SDK's source helped locate
Concord response/pagination patterns; this SDK's models, fixtures and adapters were
written for this provider. Other SDK campaign records were not read.

The root `Cargo.lock` adds this package only; no new registry packages were needed.
The required verification commands are:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-notion
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo clippy --locked -p concord-sdk-notion --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo fmt -p concord-sdk-notion
```
