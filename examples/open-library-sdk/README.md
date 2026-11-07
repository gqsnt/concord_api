# Open Library SDK21

`concord-sdk-open-library` is a bounded, public-data SDK over Concord at baseline
`56a16ee34f8aa714bd51dca67f744e5870e813b6`. This implementation is **example-assisted**:
Concord guide excerpts and the shared test server were consulted; the complete ledger is in
[PROVENANCE.md](PROVENANCE.md). No existing provider SDK source was read directly.

The slice supports these connected workflows:

1. Search works at `/search.json` with `q`, a one-based `page`, and `limit`, then
   retrieve a selected work at `/works/{id}.json`. An optional `sort` selects ordering.
2. Search authors at `/search/authors.json` with `q`, then retrieve a selected
   author's work batch at `/authors/{id}/works.json`, optionally using `limit` and `offset`.

The generated `OpenLibrary` client is the public client. Search, works and author scopes
lead to generated endpoints; every ordinary endpoint `send()` applies `ReadPolicy`.
There is no wrapper client, custom codec or native request rewrite.

```no_run
use std::{num::NonZeroU32, time::Duration};
use concord_sdk_open_library::{
    AuthorWorks, Error, OpenLibrary, Work, WorkSort, concord::reqwest,
};

// The application supplies its transport and any identification headers.
fn client(base_url: &str) -> Result<OpenLibrary, Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(OpenLibrary::new(http).with_base_url(base_url)?)
}

async fn selected_work(api: &OpenLibrary) -> Result<Option<Work>, Error> {
    let page = api.search().works("the lord of the rings")
        .set_page(NonZeroU32::new(2).unwrap())
        .set_limit(NonZeroU32::new(10).unwrap())
        .set_sort(WorkSort::New)
        .send().await?;
    match page.docs.first() {
        Some(hit) => Ok(Some(api.works().get(hit.id()?).send().await?)),
        None => Ok(None),
    }
}

async fn selected_author_works(api: &OpenLibrary) -> Result<Option<AuthorWorks>, Error> {
    let page = api.search().authors("j k rowling").send().await?;
    match page.docs.first() {
        Some(hit) => Ok(Some(api.author(hit.id()?).works()
            .set_limit(NonZeroU32::new(10).unwrap())
            .set_offset(0u64).send().await?)),
        None => Ok(None),
    }
}
# fn main() {}
```

Use a local fixture server URL for `base_url` when reproducing this package's evidence.
The declaration's default base is the official service, but **live provider calls: NOT_RUN**.
Only documentation pages were fetched during authoring. Tests always replace the base with a
loopback `TestServer` URL, disable proxies and redirects, and verify request targets.

Work search sends page 1 and limit 20 by SDK default. Optional `.set_sort(WorkSort::New)`
accepts `New`, `Old`, `Random`, or `Key`, encoded as `new`, `old`, `random`, or `key`.
These are the four values named on the [Search API page](https://openlibrary.org/dev/docs/api/search)
(read 2026-11-07); this bounded enum does not enumerate every facet in the linked server source.
Omission (or `.maybe_sort(None)`) leaves relevance ordering to the provider. Key order is
lexicographic, not numeric. `NonZeroU32` prevents zero-valued page/limit inputs without claiming
an undocumented provider maximum. Author search declares
only `q`. Work retrieval has no pagination parameters. Author works uses offset rather than
page; absent limit/offset are omitted (the cited author docs describe a default batch of 50).
Each call retrieves one page or batch; it never follows returned links or silently gathers all
results. The caller controls subsequent requests and selection. Counts may be approximate or
stale and do not establish completion or snapshot consistency.

`WorkId::parse` and `AuthorId::parse` accept a bare OLID or the canonical `/works/…` or
`/authors/…` resource key. They validate this SDK's supported ID shape, normalize it to a
bare ID, and reject full URLs, slugs, `.json` suffixes, wrong resource letters and path/query
injection. Calling `hit.id()` performs the same conversion; a missing key is an input error.

Successful empty searches have `docs: []`. Common descriptive fields are `Option`, so missing
or null titles, author names, publication years, descriptions and subjects are safe to inspect.
Unknown fields are ignored. `work.description_text()` returns one borrowed `Option<&str>`
from either a plain string or `{"type":"/type/text","value":"..."}`. Missing/null is `None`;
empty text in either shape is `Some("")`. Whitespace and markup remain unchanged. This also
works on every `Work` in `AuthorWorks.entries`. The original `description: Option<Value>`
remains available for compatibility; unrecognized shapes yield no text and remain in that raw
field. The [data dumps format documentation](https://openlibrary.org/developers/dumps#format-of-json-records)
links the [work schema](https://github.com/internetarchive/openlibrary-client/blob/master/olclient/schemata/work.schema.json)
and [text-block definition](https://github.com/internetarchive/openlibrary-client/blob/master/olclient/schemata/shared_definitions.json)
(read 2026-11-07). The schema documents the object form; accepting the plain string is the
maintenance ticket's compatibility requirement, not a claim that the schema enumerates both.
The SDK requires the collection envelope (`docs` or `entries`) to be an array and rejects
wrong types for modelled fields; it does not turn malformed envelopes into empty pages.
`numFound` and `num_found` are accepted because both appear on the official search page.
This limited projection is not a guarantee that the provider's schema will remain stable.

All SDK failures implement `Debug`, `Display` and `std::error::Error`:

| Failure | Meaning and observable behavior |
|---|---|
| `Error::InvalidId(RecordKind)` | Missing or invalid selected ID; conversion performs no I/O and never prints the input. |
| `Error::Http` | Any non-2xx status, including redirects, 4xx, 429 and 5xx. Original metadata/status remain authoritative; an opaque JSON document is read with a 16 KiB bound. |
| `Error::Concord` / `Build` | Request/base construction failed before sending. |
| `Error::Concord` / `Transport` | Sending or body transfer failed, including interruption after syntactically complete JSON. |
| `Error::Concord` / `Metadata` | JSON media check refused the success head, before reading its body. |
| `Error::Concord` / `Decode` | Success JSON syntax, truncation or model shape failed. |
| `Error::Concord` / `Limit` | Success body exceeded 1 MiB. The body is not decoded. |

`Error::status()` also retains the status of success-body failures. A failed HTTP error-document
read stays `Error::Http`, with its original status and nested metadata/decode/limit/transport
failure available through `HttpError::document_unredacted()`. No error-envelope schema is
promised in the cited endpoint docs, so the SDK retains arbitrary JSON rather than inventing
provider error codes. A non-JSON, empty or invalid error document cannot mask an HTTP failure.
Body limits count bytes after any decompression configured by the supplied transport.

The policy executes once and does not retry, sleep, authenticate, refresh credentials or impose
rate admission. For HTTP 429 the caller can inspect `http.meta().headers()["retry-after"]` if
present and choose its own scheduling policy. Transport settings and transport-level retry
configuration belong to the supplied reqwest client; tests observe one request per send.
No pagination/retry crate is needed for these explicit one-page workflows.

Search `q` is marked sensitive: generated request and `Prepared` Debug redact it. SDK error
formatting suppresses bodies, header values and parser sources, including echoed secrets.
There is no automatic error source chain. Explicit `document_unredacted`, Concord native
response/body/source accessors, success model Debug and native URL/request Debug expose data;
use them deliberately. The client has no authentication configuration.

`prepare()` remains Concord's I/O-free lower-level API. Calling `prepare()?.send()` directly
bypasses the SDK policy, uses Concord's default status errors and has no SDK byte caps.
Use the generated endpoint's `send()` for the behavior documented above.

Excluded: editing, loans, covers, editions, full author records, bulk ingestion, complete result
traversal, undocumented schema guarantees, and provider error-code or rate-limit guarantees.

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-open-library
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-open-library --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-open-library --no-deps
```

Fixtures, sources and retrieval dates are in [PROVENANCE.md](PROVENANCE.md).
Concord costs and proposed shared changes are in [FRICTION.md](FRICTION.md).
The subsequent maintenance change and its checks are recorded in [MAINTENANCE.md](MAINTENANCE.md).
