# the Elasticsearch API in Concord
Written by GPT-6 (Codex), from the specified Concord guide and Elastic's public documentation, on 2026-11-12.

## The description

This is a deliberately small client for a normal Elasticsearch cluster, not Serverless. `url` and `bulk_url` are deployment-supplied bases for the same cluster; a separate bulk coordinator is a deployment choice, not an Elasticsearch requirement. Give both the same base otherwise. No request was sent and no build was run.

Single-index operations and multi-target reads have separate handles. Pass raw target expressions such as `logs-*,metrics-*`; Concord encodes the segment. Do not pre-encode it or use `{*targets}`: the star belongs to Elasticsearch's expression, not Concord's slash-spanning hole. Front-proxy treatment of encoded commas/stars is deployment-specific (unsure). [Target syntax](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/api-conventions).

```rust
use concord::{basic, json, Base, Json, Secret, Text};
use serde::Deserialize;
use serde_json::{json as j, Value};

concord::api! {
    pub client Elasticsearch(url: Base, bulk_url: Base, authorization: Secret) {
        base url;
        headers { "authorization" = authorization; }
        read Json;
        problem EsProblem;
        /// Logs endpoint names and try numbers, never payloads or credentials.
        middleware TraceRequests;

        part output(pretty?: bool, filter_path?: Vec<String>) {
            query { pretty; "filter_path" = filter_path.as_ref().map(|p| p.join(",")); }
        }
        part write_options(refresh?: Refresh, routing?: String) {
            query { refresh; routing; }
        }

        /// One index or single-index alias; not a wildcard or comma list.
        scope index(name: String) {
            path "{name}";
            PUT index_document(id: String, document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                body json(document);
                idempotent false;
            }
            POST index_auto(document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc";
                body json(document);
            }
            GET get(id: String, ..output) -> GetReply {
                path "_doc/{id}";
                status { 404 { body } => document_404::<GetReply>(body)? }
            }
            HEAD exists(id: String, ..output) -> bool {
                path "_doc/{id}";
                status { 2xx => true, 404 => false }
            }
            /// Patch is an update envelope: doc, script, upsert, etc.
            POST update(id: String, patch: Value, ..output, ..write_options) -> WriteReply {
                path "_update/{id}";
                body json(patch);
            }
            DELETE delete_document(id: String, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                status { 404 { body } => document_404::<WriteReply>(body)? }
                idempotent false;
            }
            PUT create(settings: Value, ..output) -> IndexAck {
                body json(settings);
                idempotent false;
            }
            DELETE delete(..output) -> IndexAck { idempotent false; }
        }

        /// A comma list and/or wildcard expression, kept in one path segment.
        scope indices(targets: String) {
            path "{targets}";
            GET search(request: Value, ..output) -> SearchPage {
                path "_search";
                body json(request);
                idempotent;
            }
            GET count(request: Value, ..output) -> CountReply {
                path "_count";
                body json(request);
            }
            POST open_pit(keep_alive: String, ..output) -> OpenedPit {
                path "_pit";
                query { keep_alive; }
            }
        }

        POST bulk(lines: Vec<Value>, ..output, ..write_options) -> BulkReply {
            base client.bulk_url;
            headers { override "authorization" = client.authorization; }
            path "_bulk";
            body ndjson(lines);
        }
        /// No index in the URL. Cursor replaces both PIT id and search_after.
        /// filter_path is deliberately unavailable: the pager needs its fields.
        POST search_pit(query: Value, pit_id: String, keep_alive: String,
                        size?: u32, cursor?: PitCursor, pretty?: bool) -> SearchPage {
            path "_search";
            query { pretty; }
            body json(pit_body(query, pit_id, keep_alive, size, cursor));
            paginate pit_after(cursor);
            idempotent;
        }
        DELETE close_pit(id: String, ..output) -> ClosedPit {
            path "_pit";
            body json(j!({ "id": id }));
            idempotent false;
        }
        GET cat_indices(format?: CatFormat, ..output) -> CatReply {
            path "_cat/indices";
            query { format; }
            read Text;
            status { 2xx { body } => cat_reply(body, format)? }
        }
    }
}
```

Indexing uses `PUT /{index}/_doc/{id}` or `POST /{index}/_doc`; update and delete have the paths above. `Refresh` serializes as `true`, `false`, or `wait_for` (text). Versioned writes are conservatively non-retriable here, including PUT; repeating an indexing request can increment its version. [Index](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-index), [update](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-update), [delete](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-delete).

`get` preserves a missing document's `found: false` and identity; a missing-index error remains a refusal. `exists` can only distinguish HTTP statuses: its bodyless 404 becomes false. The index create/delete operations keep their acknowledgment bodies. [Get and its two 404 shapes](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-get), [HEAD](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-exists), [create index](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-indices-create), [delete index](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-indices-delete).

GET search and count carry JSON bodies. Search also accepts POST; the `post()` method below changes only that method. CAT defaults to text, while `format=json` selects JSON; the Rust answer remains `CatReply`. [Search methods](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-search), [count](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-count), [CAT formats](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-cat-indices).

`pretty=true` is the useful spelling of `?pretty`. Arbitrary `filter_path` can remove fields required by a DTO: use `send_raw()` for such projections, accepting that it bypasses status arms and typed problem decoding. Ordinary typed calls need the fields listed below. [Common options](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/common-options).

## Beside the macro

All DTO fields are public; request/cursor types implement `Clone`, serializable inputs implement `Serialize`, and response DTOs implement `Deserialize`. Unknown response fields are ignored. Documents and Query DSL are intentionally `Value`, rather than an attempted model of every mapping and query variant. `EsProblem` is a Debug/Deserialize envelope with `error: EsCause, status: u16`; only its cause DTO needs expanding:

```rust
#[derive(Debug, Deserialize)]
pub struct EsCause {
    #[serde(rename = "type")]
    pub kind: String,
    pub reason: Option<String>,
}
```

`GetReply`: `index: String` (`_index`), `id: String` (`_id`), `found: bool`, `source: Option<Value>` (`_source`), `version: Option<u64>` (`_version`). `WriteReply`: the same identity, required `result: String`, optional version/sequence metadata. `IndexAck`: required `acknowledged: bool`, optional `index: String` and `shards_acknowledged: bool`.

`CountReply`: `count: u64`, `shards: ShardStats` (`_shards`). `SearchPage`: `timed_out: bool`, `shards: ShardStats` (`_shards`), `hits: Hits`, `pit_id: Option<String>`. `ShardStats` needs `failed: u32`; `Hits` holds `hits: Vec<Hit>`; `Hit` has `id: String` (`_id`), optional source (`_source`), and `sort: Vec<Value>` defaulting to empty when absent.

`OpenedPit` has `id: String`; `ClosedPit` has `succeeded: bool, num_freed: u64`. `PitCursor` has `id: String, after: Vec<Value>`. `CatFormat` has `Text` and `Json`, serialized lowercase. `CatReply` has `Text(String)` and `Json(Vec<Value>)`; JSON CAT columns remain provider-shaped values.

`BulkReply` has `took: u64, errors: bool, items: Vec<std::collections::BTreeMap<String, BulkItem>>`. Each map normally has one operation key; `BulkItem` has `status: u16`, optional `_index`, `_id`, `result`, and `error: EsCause`. A 200 can contain mixed successes and failures. There is one result per operation, not per NDJSON line; deletes have no source line. The writer emits compact JSON plus a final newline; callers supply correctly paired action/source lines. Bulk is not automatically retried. [Bulk framing and results](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk).

The error envelope models object errors with `type` and optional `reason`; other error shapes remain in Concord's kept bytes on ordinary refusals. The special document 404 arm requires one of its expected shapes.

```rust
use concord::page::{Call, Slot};
use concord::{Answer, Body, Fail, Middleware, Next, TransportError, TryInfo};

// Author-chosen telemetry, shared by every request, try, and page.
pub struct TraceRequests;
impl Middleware for TraceRequests {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        tracing::debug!(endpoint = info.endpoint(), attempt = info.number(), "Elasticsearch try");
        next.send(request, info).await
    }
}

fn ndjson(lines: &[Value]) -> Result<Body, serde_json::Error> {
    let mut bytes = Vec::new();
    for line in lines {
        serde_json::to_writer(&mut bytes, line)?;
        bytes.push(b'\n');
    }
    Ok(Body::new(bytes, "application/x-ndjson"))
}

fn document_404<T: serde::de::DeserializeOwned>(body: Value) -> Result<T, Fail<EsProblem>> {
    if body.get("error").is_some() {
        let problem: EsProblem = serde_json::from_value(body)
            .map_err(|_| concord::mismatch("invalid document error envelope"))?;
        return Err(problem.into());
    }
    serde_json::from_value(body)
        .map_err(|_| concord::mismatch("invalid missing-document reply"))
}

fn pit_body(query: &Value, initial: &str, keep_alive: &str,
            size: &Option<u32>, cursor: &Option<PitCursor>) -> Value {
    let id = cursor.as_ref().map_or(initial, |c| c.id.as_str());
    let mut body = j!({ "query": query, "pit": { "id": id, "keep_alive": keep_alive },
                       "sort": ["_shard_doc"], "track_total_hits": false });
    if let Some(n) = size { body["size"] = j!(n); }
    if let Some(c) = cursor { body["search_after"] = j!(c.after); }
    body
}

fn pit_after<C, S>(cursor: S)
    -> impl Fn(C, &Answer<SearchPage>) -> Result<Option<C>, Fail<EsProblem>> + Send + 'static
where
    C: Call<Value = SearchPage, Problem = EsProblem>,
    S: Slot<C, Value = PitCursor>,
{
    move |call, answer| {
        let page = answer.value();
        if page.timed_out || page.shards.failed != 0 {
            return Err(concord::mismatch("incomplete PIT search page"));
        }
        let Some(last) = page.hits.hits.last() else { return Ok(None); };
        let id = page.pit_id.clone().ok_or_else(|| concord::mismatch("missing PIT id"))?;
        if last.sort.is_empty() { return Err(concord::mismatch("missing search_after values")); }
        Ok(Some(cursor.set(call, PitCursor { id, after: last.sort.clone() })))
    }
}

fn cat_reply(text: String, format: &Option<CatFormat>) -> Result<CatReply, Fail<EsProblem>> {
    if matches!(format, Some(CatFormat::Json)) {
        return serde_json::from_str(&text).map(CatReply::Json)
            .map_err(|_| concord::mismatch("invalid CAT JSON"));
    }
    Ok(CatReply::Text(text))
}

impl elasticsearch::indices::Search {
    pub fn post(self) -> Self {
        self.edit_request(|request| *request.method_mut() = reqwest::Method::POST)
    }
}
```

PIT search uses the root URL, keeps the query and `_shard_doc` ordering unchanged, omits an absent `search_after`, and carries both the latest PIT id and the last hit's entire sort tuple. It stops on an empty complete page, not a guessed total. Partial/timed-out pages make the pager fail; Concord delivers the current page before reporting that failure on the next iteration. No `IntoItems` implementation is needed for the page loop used here. [PIT rules](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-open-point-in-time), [search-after and tiebreakers](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results), [partial results](https://www.elastic.co/docs/solutions/search/the-search-api).

The caller owns opening and closing. Even after an iteration error it attempts closure with the most recently received id; expiration is the fallback if closure fails or the task is cancelled. There is no automatic close-on-drop claim. [Close PIT](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-close-point-in-time).

## Calling it

These are illustrative async program fragments, not executed requests. `?` uses the application's error type. Both bases are trusted addresses of the same deployment; `basic` returns a `Secret`. An API key is the already-encoded Elasticsearch key, prefixed with `ApiKey`. [Authentication examples](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-search).

1. Connect, index, and distinguish a missing document without losing its identity.

```rust
let base: Base = std::env::var("ES_URL")?.parse()?;
let authorization = basic("elastic", &Secret::new(std::env::var("ES_PASSWORD")?));
// Alternative: Secret::new(format!("ApiKey {}", std::env::var("ES_API_KEY")?));
let es = Elasticsearch::new(base.clone(), base, authorization);
let books = es.index("books");
let made = books.index_auto(j!({ "title": "Dune" })).refresh(Refresh::WaitFor).await?;
let got = books.get(&made.id).await?;
println!("{}: found={}", got.id, got.found);
let present = books.exists(&made.id).await?;
```

2. Ingest a batch and inspect each operation, including failures under HTTP 200.

```rust
let reply = es.bulk(vec![
    j!({ "index": { "_index": "books", "_id": "42" } }),
    j!({ "title": "Dune" }),
    j!({ "delete": { "_index": "books", "_id": "obsolete" } }),
]).refresh(Refresh::WaitFor).await?;
for item in reply.items {
    for (operation, result) in item {
        if let Some(error) = result.error {
            eprintln!("{operation}: {} {:?}", error.kind, error.reason);
        }
    }
}
```

3. Walk a stable snapshot and attempt cleanup on both success and iteration error.

```rust
let opened = es.indices("logs-*,metrics-*").open_pit("1m").await?;
let mut latest = opened.id.clone();
let mut pages = es.search_pit(j!({ "match_all": {} }), &opened.id, "1m").size(250).pages();
let walked: elasticsearch::Result<()> = async {
    while let Some(page) = pages.try_next().await? {
        if let Some(id) = page.pit_id { latest = id; }
        if !page.timed_out && page.shards.failed == 0 {
            for hit in page.hits.hits { println!("{}", hit.id); }
        }
    }
    Ok(())
}.await;
let closed = es.close_pit(latest).await;
walked?;
closed?;
```

4. Search with a GET body, choose POST when needed, and request CAT JSON.

```rust
let targets = es.indices("books,magazines-*");
let query = j!({ "query": { "match_all": {} }, "size": 5 });
let first = targets.search(&query).pretty(true).await?; // GET with body
let same = targets.search(query).post().await?;       // equivalent POST form
let rows = es.cat_indices().format(CatFormat::Json).await?;
let raw = targets.search(j!({ "query": { "match_all": {} } }))
    .filter_path(["hits.hits._id"]).send_raw().await?;
let projected: Value = raw.error_for_status()?.json().await?;
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | `filter_path` | An arbitrary projection can invalidate the fixed DTO after the arrow. | Shared option, documented required fields; `send_raw()` for arbitrary projections, losing Concord's typed refusal handling. | cannot be said |
| 2 | Search methods | One endpoint declaration has one method; GET and POST cannot be aliases in it. | GET plus an author-written `post()` using `edit_request`; PIT's distinct root operation uses POST directly. | cannot be said |
| 3 | Document 404 | `404 => None` would discard identity and swallow missing-index errors; status alone cannot distinguish these bodies. | A JSON-value arm and `document_404`, returning a missing-document DTO or a typed refusal. | said with pain |
| 4 | PIT cursor | The cursor changes two body fields; absent is not JSON null. A stock cursor extractor was insufficient. | One optional compound cursor, a body assembler, and a pager that checks completeness. | said with pain |
| 5 | PIT lifetime | An endpoint tree cannot express open/walk/close, especially cleanup on cancellation. | An explicit caller loop and close attempt; server expiration remains necessary. | cannot be said |
| 6 | Bulk | NDJSON is neither one JSON value nor a JSON array; a 200 is not all-success. | A compact-line writer and a typed mixed-result envelope; inspect each operation, retry none automatically. | said with pain |
| 7 | CAT | An option changes wire format, but cannot change the arrow's Rust type. | A fixed enum, `Text`, and a `2xx` arm which parses JSON when selected. Covering all 2xx avoids needing `Text: Reader<CatReply>`. | said with pain |
| 8 | Bulk base | I stopped to check whether auth survived the second base; it intentionally does not. | Endpoint `base client.bulk_url` and an explicit authorization override, even for this same-cluster deployment. | said with pain |
| 9 | Target tree | Single-index writes and multi-target reads want the same URL slot but different constraints; choosing two handles still felt awkward after the scope advice. | Separate `index`/`indices` handles, with a documented single-index restriction on the former's String. | said with pain |
| 10 | `?pretty` | There is no key-only query entry. | `pretty?: bool` emits the accepted `pretty=true`. | small |
| 11 | Placement | Nearly every input needs its name repeated just to place it. | Two parts share the repeated query inputs; local inputs still need placement. | small |
| 12 | Versioned PUT | The HTTP-method retry default is too optimistic for the contract I want from versioned indexing. | `idempotent false;` on writes whose repetition I will not promise, including index creation/deletion. | small |

Wishes for the first two kinds, stated as intent rather than invented syntax:

- **1:** A projection-aware call could return its projected JSON while retaining the declared status and error rules.
- **2:** One operation could advertise GET and POST with the same inputs, body, and answer.
- **3:** A documented JSON envelope discriminator could distinguish missing-document answers from errors without a Value round trip.
- **4:** A cursor could update nested body fields and omit absent fields without a separate assembler and slot adapter.
- **5:** A PIT session could own its open/search/close lifecycle, including an explicit cancellation strategy.
- **6:** A reusable NDJSON writer could express action/source framing; item failures would still remain per-operation data.
- **7:** Selecting CAT JSON could give a JSON-row call type, and selecting text a String call type.
- **8:** A reviewed pair of same-cluster bases could share a credential explicitly once.
- **9:** One obvious target handle could constrain write targets while accepting read expressions, without cumbersome newtype arguments.

## Measures

- Endpoints: **15** declarations; `post()` is a method adapter, not another declaration. Close PIT is included to complete the lifecycle.
- `api!` block: **100** physical lines, from `concord::api! {` through its closing brace, including blank/comment lines; imports excluded.
- Placement-only second mentions of input names: **31**; inputs declared: **30** (parts counted once), **64** after part expansion. Expansion counts each node's own inputs, not additional copies of inherited client/scope inputs.
- The 31 count covers references in `base`, `path`, query/header entries, and request bodies; auth's second-host restatement counts again. Pager/arm references do behaviour, so are excluded. Parts are counted once, not per use.
- Beside-macro Rust needed for missing direct descriptions: **55** nonblank lines across `ndjson`, `document_404`, `pit_body`, `pit_after`, `cat_reply`, and the `post` impl, including signatures/braces. DTOs/imports and the optional telemetry middleware are excluded; the six helpers use the guide's public doors.
- Findings: **3 cannot be said**, **6 said with pain**, **3 small**. Paper review and text counts only; no compilation, build, or API traffic.

## Scores

- **Writing this description: 6/10.** The tree and parts were easy; body construction, projection typing, and PIT lifetime took most of the thought.
- **Reading it back a month later: 7/10.** Paths and exceptional statuses stay visible, but the PIT and CAT contracts require following Rust beside the macro.
- **The calling code: 7/10.** Normal document calls read well; projection escape hatches and explicit snapshot cleanup make advanced programs heavier.
