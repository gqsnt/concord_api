# concord-sdk-elasticsearch

A bounded Elasticsearch SDK slice built on Concord. It covers two connected workflows:

1. **Bulk**: send a bounded set of `index` / `create` / `update` / `delete` operations as one
   NDJSON `POST /_bulk`, then check **every** per-item result against the operation it answers.
2. **Point in time + `search_after`**: open a PIT on an index, page through a sorted search with
   the last hit's sort values, and close the PIT — also when a page fails.

It is offline-tested only: no request in this crate's tests reaches a real Elasticsearch
(live provider calls: **NOT_RUN**). The contract comes from two official pages, cited in
[`PROVENANCE.md`](PROVENANCE.md).

## Constructing the client

```rust,ignore
use concord_sdk_elasticsearch::{ApiKey, Elasticsearch, concord::reqwest};

// Transport settings are yours. For bulk writes, turn off transport retries and redirects:
// a transport that replays a bulk body re-applies operations this SDK already reported.
let http = reqwest::Client::builder()
    .retry(reqwest::retry::never())
    .redirect(reqwest::redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(60))
    .build()?;
let es = Elasticsearch::new(http, "https://my-deployment.example:9200", ApiKey::new(&encoded_key)?)?;
```

The key is sent as `Authorization: ApiKey <key>` (the form of the bulk reference's curl
example), built with `concord_auth::authorization` and flagged sensitive: `Debug` of the client,
of prepared requests and of every error shows `<redacted>`.

`es.api()` is the generated `ElasticsearchApi` (`bulk`, `index(..).open_point_in_time`,
`search`, `close_point_in_time`): every generated `send()` returns `Result<T, Error>` through the
crate's `SendPolicy`, so the wire types (`BulkResponse`, `SearchResponse`, ...) are public too.

## Workflow 1: bulk

```rust,ignore
use serde_json::json;
use concord_sdk_elasticsearch::{BulkRequest, Error};

let mut request = BulkRequest::new(); // default limits: 1,000 operations, 8 MiB
request
    .index("articles", "a-1", &json!({ "title": "one" }))?
    .create("articles", "a-2", &json!({ "title": "two" }))?
    .update("articles", "a-0", &json!({ "views": 3 }))?
    .delete("articles", "a-9")?;

match es.bulk(request).await {
    Ok(report) => { /* every operation was applied */ }
    Err(Error::PartialBulk(report)) => {
        for item in report.failed() {
            eprintln!("{} {} failed: {} {:?}", item.action(), item.id(), item.status(),
                item.error().map(|e| e.kind()));
        }
        // Only failed operations can be resubmitted; here only the rejected (429) ones.
        if let Some(again) = report.resubmit_rejected() {
            es.bulk(again).await?;
        }
    }
    Err(e) if e.is_uncertain() => { /* see "Failure semantics" */ }
    Err(e) => return Err(e.into()),
}
```

- Every operation names its index and an explicit `_id`. Documents (and update partials) must
  serialize to a JSON object without raw line breaks; violations are refused when the operation
  is added, and the request stays unchanged. `BulkLimits` bounds operations (hard cap 10,000)
  and body bytes (hard cap 100 MiB, Elasticsearch's documented default request size limit).
- The body is Concord's `Ndjson` format: compact JSON, one line per action and per source, a
  line feed after every line including the last, `Content-Type: application/x-ndjson`.
- `BulkRequest::set_refresh(Some(Refresh::WaitFor))` sends `?refresh=wait_for`.

### What a report means

The bulk reference says the response `items` are the results "in the order they were
submitted". The SDK matches item *n* with operation *n* and checks the match before believing
any outcome: the item count, the action key and the document id must agree, or the call is
`Error::Protocol` (uncertain). Each item is then:

| Item | Outcome |
|---|---|
| `error` present (any status) | **failed**, with the error's `type` and `reason` |
| no `error`, 2xx status | **applied** (`created`, `updated`, `deleted`, `noop`) |
| no `error`, `delete` answered `404` with `result: not_found` | **applied** (the documented "successful" delete of a missing document) |
| no `error`, any other status | **failed**, without a cause |

The response's `errors` flag must agree with that classification; a contradiction in either
direction is `Error::Protocol`, never success. `Ok(report)` means every operation was applied;
any failure is `Err(Error::PartialBulk(report))`, so `?` in application code cannot mistake a
partial failure for success.

`report.resubmit(select)` builds a new request from the **failed** operations that `select`
accepts (it is never offered an applied item), in their original order and bytes.
`resubmit_rejected()` selects status `429` items: the SDK's only built-in rule. A conflict
(`409`), a missing document (`404`) or a mapping error (`400`) is not resubmitted by it.

## Workflow 2: point in time and `search_after`

```rust,ignore
use std::num::{NonZeroU32, NonZeroUsize};
use serde_json::json;
use concord_sdk_elasticsearch::{KeepAlive, PitSearch, Sort};

let search = PitSearch::new(vec![Sort::asc("date")], 1_000)?   // page size 1..=10,000
    .with_query(json!({ "match": { "title": "elasticsearch" } }))?;
let keep_alive = KeepAlive::minutes(NonZeroU32::new(1).unwrap());

// Everything at once: open, page, close (also after a failure). At most 50 × 1,000 hits.
let scan = es.scan("my-index-000001", keep_alive, &search, NonZeroUsize::new(50).unwrap()).await?;
for hit in &scan.hits { let doc: MyDoc = hit.source().unwrap()?; }
scan.close?; // closing failed: the hits are still complete

// Or step by step, consuming hits as they arrive (also `.items().into_stream()`).
let pit = es.open_point_in_time("my-index-000001", keep_alive).await?;
let mut hits = es.search_after(&pit, &search, NonZeroUsize::new(50).unwrap()).items();
while let Some(hit) = hits.next().await? { /* .. */ }
es.close_point_in_time(pit).await?;
```

- Open: `POST /<index>/_pit?keep_alive=1m`. Search: `GET /_search` with `pit.id` and
  `keep_alive` in the body and **no index in the path**, the same `query` and `sort` on every
  page, `track_total_hits: false`, and from the second page on the previous page's last
  `sort` values as `search_after`. Close: `DELETE /_pit` with `{"id": ..}`.
- Each page's `pit_id` replaces the PIT's id (the guide: "use the latest PIT ID"); the next page
  and the close use it. The sort values are sent back as the exact JSON text received (no
  re-encoding of large integers or decimals).
- The traversal is a `concord_pagination::Pages` chain with a mandatory page cap. It ends at
  the first page without hits (`Ok(None)`); reaching the cap with more pages is
  `PagesError::Bounded` (in `scan`: `Error::PageCap`), never completion.
- A dropped `PointInTime` is not closed (Rust has no asynchronous drop); Elasticsearch frees it
  when its keep-alive expires. `scan` always closes the PIT it opened, except when its future
  is cancelled.

## Failure semantics

| Failure | Error | `is_uncertain()` |
|---|---|---|
| input refused (index name, empty id, non-object document, limits, page size, sort, query, API key) | `Invalid` — nothing sent | no |
| non-2xx answer | `Provider`: status, head, error document (`type`, `reason`, `root_cause`), or why the document could not be read (absent, not JSON, over 64 KiB, malformed) | 5xx: yes; 4xx: no (the SDK reads a 4xx bulk answer as a rejection of the whole request) |
| no response head (connect failure, timeout, reset) | `Concord`, `delivery() == Unknown` | yes |
| 2xx head, then a body that is cut off, over 16 MiB, mislabelled (not `application/json` / `application/*+json`), malformed, or a filtered response | `Concord` (`Transport`, `Limit`, `Metadata`, `Decode`) | yes |
| bulk items that cannot be matched with the operations, or an `errors` flag that contradicts them | `Protocol` | yes |
| at least one failed bulk item | `PartialBulk(report)` | **no**: every outcome is known |
| PIT page without `pit_id`, a hit without `sort`, more hits than requested, a cursor that does not advance, an empty PIT id | `Protocol` | (reads) |
| PIT page with `timed_out: true` or `_shards.failed > 0` | `PartialSearch` — the traversal stops instead of skipping hits | (reads) |

**Uncertain bulk requests** may have been applied wholly or in part, and the SDK cannot tell
which operations. Never resubmit one blindly. Because every operation has an explicit id, a
deliberate replay can use `create` (which, per the bulk reference, fails when the id already
exists) or read the documents back first.

`Display` and `Debug` never show the API key, document sources, PIT ids, sort values or
Elasticsearch error `reason`s (which can quote document content: the bulk API's
`include_source_on_error` defaults to `true`). `ErrorCause::reason()` and `Hit::source()`
return them on request.

## Excluded

Cluster and index administration, every index option, mappings and settings; scroll; bulk
actions without an explicit `_id`, `filter_path` (a filtered bulk answer cannot be matched with
its operations; it is refused as malformed), `if_seq_no`/`if_primary_term`, `version`,
`routing`, pipelines, scripted updates and upserts; multi-index and wildcard PITs; `from`/`size`
paging; aggregations, `total` hits and typed query DSL (the query is caller-written JSON);
streamed (lazily produced) bulk bodies and any unbounded buffering; automatic retries of any kind;
Basic and bearer authentication.

## Tests

```sh
cargo test --locked -p concord-sdk-elasticsearch
```

`tests/protocol.rs` checks request shapes on the wire, every error class, unreadable bodies,
contract violations, input validation and redaction; `tests/workflows.rs` runs both workflows
end to end, including the required partial-failure case, against `examples/testserver` and the
fixtures listed in [`PROVENANCE.md`](PROVENANCE.md).
