# concord-sdk-arxiv

A small public-metadata SDK using Concord's client → scope → endpoint hierarchy:
`Arxiv → papers → search / fetch`. Both operations use the arXiv query route;
search supplies `search_query`, while direct lookup supplies `id_list`.

**Live provider calls: NOT_RUN.** All workflows and protocol checks use
`concord-example-testserver` on loopback. This is documentation-assisted and
test-harness-example-assisted work; no existing `*-sdk` source file was consulted.
The exact sources and fixture derivations are in [PROVENANCE.md](PROVENANCE.md).

```rust,no_run
use concord_sdk_arxiv::{Arxiv, IdList, PageSize, SearchTerm, http_client};
use concord_sdk_arxiv::concord_pagination::Resume;
use std::num::NonZeroUsize;

// Supply the URL of your local fixture server, e.g. http://127.0.0.1:PORT/api.
async fn metadata(base_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = Arxiv::with_http(http_client()?).with_base_url(base_url)?;
    let papers = api.papers();

    let feed = papers.search(SearchTerm::new("electron")?)
        .set_size(PageSize::new(10)?)
        .send().await?;
    if let Some(entry) = feed.entries.first() {
        println!("{}: {}", entry.id, entry.title);
        println!("{}", entry.summary);
    }

    let known = papers.fetch(IdList::new(&["hep-ex/0307015v1", "cs/9901002v1"])?)
        .send().await?;
    for paper in known.entries {
        println!("{}: {} ({})", paper.id, paper.title, paper.published);
    }

    let mut pages = papers.search(SearchTerm::new("electron")?)
        .set_size(PageSize::new(10)?)
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
    while let Some(page) = pages.next_page().await? {
        for paper in page.items { println!("{}", paper.title); }
    }
    Ok(())
}
```

`SearchTerm` searches a literal phrase across indexed fields, encoded as `all:"term"`.
It accepts 1–512 UTF-8 bytes and refuses quotes, backslashes, controls and blank terms.
Advanced Boolean expressions and configurable sorting are outside this slice.
`IdList` accepts 1–50 bare IDs, including legacy slash IDs and version suffixes.
It validates delimiter safety; arXiv validates identifier grammar. Returned entries are
in provider order; missing IDs are not replaced by invented records.

Search defaults to `start=0&max_results=10`. `PageSize` permits 1–100; ID lookup
requests a single page of 50 so every permitted input ID fits. The SDK refuses search
windows ending beyond 30,000 before sending. Search traversal requires a page budget.
`PagesError::Bounded` includes the next offset; provider failures include the failed
offset. `items().collect_complete()` retains the collected prefix on either failure.
Resume only against the same query and page size. A changing index can cause duplicates
or gaps; a short page continues when the total indicates remaining records.

The policy admits one request per three seconds and holds a single concurrency lease
through decoding. Reuse a client or clone a `Policy` into multiple `ArxivConfig`s to
share that budget. Independent policies and processes require external coordination.
The provider also recommends caching repeated queries; cache results in the calling
application. The default base is `https://export.arxiv.org/api`, but no test uses it.
These usage and paging rules come from the
[arXiv API manual, §§3.1.1.2 and 3.3.1.1](https://info.arxiv.org/help/api/user-manual.html),
read 2026-11-07. The SDK's smaller page/ID caps and byte/node limits are local choices.

The custom Atom decoder checks namespace URIs independently of prefix spelling.
It ignores unknown extension elements, including foreign elements named `entry` or
`title`. Comment, journal reference, DOI, primary category, link attributes, category
scheme and author affiliations can be absent. Core article ID, title, summary and
timestamps and OpenSearch counts are required. Timestamps remain provider strings.
The SDK preserves metadata links as data and does not follow them.

Failures are explicit:

| Error | Meaning |
| --- | --- |
| `Input` | Invalid term/list/size or out-of-range window; no dispatch. |
| `Concord` | Transport, media, XML or byte-limit failure. Inspect the boxed `concord::Error`'s kind and head. Concord also represents request-building failures. |
| `Http` | Any non-2xx status, retained with a bounded `document: Result<Feed, concord::Error>`. A malformed document cannot hide the HTTP status. |
| `Api` | An Atom error entry inside a 2xx response; explicit `Problem` fields retain provider detail. |
| `Pagination` | Inconsistent echoed offset/page size or nonadvancing/out-of-total traversal. |
| `Admission` | A local limiter failure, such as clock arithmetic overflow; no dispatch. |

Success bodies are limited to 2 MiB; HTTP error documents to 64 KiB. XML is UTF-8,
DTD parsing is disabled, and parsing is capped at 50,000 nodes. Exactly one
`application/atom+xml` media type is required, with no charset or UTF-8/US-ASCII.
Malformed/truncated XML is a decode failure; an interrupted HTTP body is a transport
failure. No SDK retries occur, including on 429 or 503. `http_client()` also disables
reqwest retries/redirects, sets a 30-second timeout and avoids proxies. A supplied
transport may have its own behavior.

Generated request/client `Debug` and SDK error `Debug`/`Display` withhold terms, IDs,
provider messages, response header values and parser input. Input `Display` is its
explicit wire spelling. Article metadata is public data and its `Debug` prints it.
Native request/response access, explicit error-document fields, body accessors and
transport errors are disclosure boundaries inherited from Concord.

`prepare()` remains Concord's native exit: it builds without sending. Executing the
prepared request directly bypasses pacing, input-window checks and SDK Atom-error
interpretation; the declared media decoder and success byte limit still apply.
Prefer generated `send()`. The implementation itself needs no native mutation or
direct reqwest execution.

Excluded: PDF/full-text ingestion, bulk archive harvesting, author accounts, arbitrary
search syntax, sorting controls, persistent caching and automatic retry. See
[FRICTION.md](FRICTION.md) for the SDK work remaining outside shared Concord mechanics.

Verification (default worktree `target/`, one command at a time):

```sh
CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-arxiv
CARGO_BUILD_JOBS=4 cargo clippy --locked --offline -p concord-sdk-arxiv --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-arxiv --no-deps
```
