# Provenance

## Documentation read

For the initial bulk/PIT slice, both pages were read on **2026-11-07** (UTC), with an HTTP GET of the public page (HTTP 200,
`text/html`) whose text was extracted locally for reading; the bulk page was also read once
through a summarising web fetch. The local copies stayed in the worktree's `target/tmp/docs`
(not part of the crate). No other provider page was read at that stage. Neither page named an Elasticsearch
version; this records the documentation as observed on that date, not a versioned schema.

| Page | Retrieved (UTC) | Used for |
|---|---|---|
| [Bulk API reference](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk) | 2026-11-07 06:53 | `POST /_bulk`; NDJSON framing ("The final line of data must end with a newline character", "not pretty printed", `Content-Type` `application/json` or `application/x-ndjson`); `index`/`create` need a source line, `delete` none, `update` a line with `doc`; "A create action fails if a document with the same ID already exists"; the 100mb default request limit; `refresh` (`true`, `false`, `wait_for`); `include_source_on_error` (default `true`); response `took`, `errors` ("If true, one or more of the operations ... did not complete successfully"), `items` ("in the order they were submitted"), item `_index`, `_id`, `status`, `error` ("returned only for failed operations": `type`, `reason`, `root_cause`), `result` (`created`, `updated`, `deleted`, `noop`, `not_found`), `_version`, `_seq_no`, `_primary_term`, `_shards`; the curl example's `Authorization: ApiKey $ELASTIC_API_KEY` and `localhost:9200` |
| [Paginate search results](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results) | 2026-11-07 06:54 | `POST /<index>/_pit?keep_alive=1m` and its `id`; the PIT search (`pit.id`, `keep_alive`, `sort`, no index in the path, `size` 10000 in the examples, the 10,000-hit `index.max_result_window` default); "use the latest PIT ID", the implicit `_shard_doc` tiebreaker, the last hit's `sort` values as `search_after`, "query and sort arguments must remain unchanged", `track_total_hits: false`; `DELETE /_pit` with `{"id": ..}`; scroll is no longer recommended |

### Maintenance research: document reads and conditional indexing

Read on **2026-11-07**, using read-only web fetches of these exact pages and focused extracts.
The bulk and pagination pages above were also reread on this date. No live Elasticsearch
service was contacted; live provider calls: **NOT_RUN**. These are unversioned documentation
pages, observed on the stated date.

| Page | Facts used |
|---|---|
| [Get a document by its ID](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-get) | `GET /{index}/_doc/{id}`; `found`, `_index`, `_id`, `_source`, `_seq_no`, `_primary_term`; a 404 can contain either a get-result or an error response. Source data may be absent. |
| [Create or update a document in an index](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-index) | JSON object source; `PUT /{index}/_doc/{id}` with `if_seq_no` and `if_primary_term`; mismatches produce a version conflict and HTTP 409; response id/index, result, revision fields and shard counts. |
| [Optimistic concurrency control](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/optimistic-concurrency-control) | The GET-returned sequence number and primary term identify a change and must both be supplied to guard a later replacement. The example uses products/1567, sequence number 362 and primary term 2. Indices with sequence numbers disabled reject concurrency parameters. |

The 8 MiB source bound, unsigned sequence numbers, positive primary terms, mandatory tokens
even when the provider schema lists them as optional, response-id/status consistency checks,
and conservative treatment of write delivery failures are SDK decisions. The documents do not
promise that a client timeout cancels a write. This SDK never retries it; the caller must disable
transport retries/redirects and reconcile uncertain outcomes explicitly.

`tests/documents.rs` and `tests/maintenance_use.rs` contain **synthetic inline fixtures**.
Their product/id/source and initial concurrency values follow the OCC example; GET's
`found: false` shape and error envelope follow the GET schemas. Later revision numbers,
replacement tags, competing edits, shard counts, missing-index errors, the literal error type
`version_conflict_engine_exception`, malformed/oversized bodies and timing faults are synthetic.
Request expectations are written from the documented route/query/body contract, not captured
from SDK output. Existing fixture files and tests are unchanged.

## Fixtures

Every fixture is either taken from a cited example or labelled synthetic. No expectation was
copied from this SDK's output: request expectations are the documented examples (compared as
JSON per line, with the NDJSON framing checked byte-wise), response fixtures are the documented
answers or hand-written ones.

| Fixture | Source | Derivation |
|---|---|---|
| `bulk-multiple-operations.ndjson` | Bulk reference, "Multiple operations" request, curl tab (`-d $'…'`) | Verbatim: the shell string with each `\n` written as a line feed (seven lines, the last terminated). |
| `bulk-multiple-operations-response.json` | Bulk reference, response example "Multiple successful operations" | Verbatim (indentation of the first line normalised). Includes the `delete` answered `404`/`not_found` under `errors: false`. |
| `bulk-failed-actions.ndjson` | Bulk reference, request example "Filter for failed operations" (updates of ids 5 and 6, create of 7) | Whitespace inside each line removed (compact NDJSON); content verbatim. The SDK sends these operations **without** `filter_path`. |
| `bulk-failed-actions-response.json` | Bulk reference, response example "Failed actions" | Verbatim (indentation of the first line normalised). |
| `bulk-filtered-errors-response.json` | Bulk reference, response example "Filter for failed operations" (`filter_path=items.*.error`) | Verbatim (indentation of the first line normalised); used to show a filtered answer is refused. |
| `bulk-resubmitted-missing-response.json` | Derived from "Failed actions" | The `create` item removed and `took` set to 7: the answer to resubmitting the two updates. |
| `bulk-rejected-item-response.json` | **Synthetic**, item fields from the bulk response schema | Ids `a-1`…`a-4`; one item `429` `es_rejected_execution_exception`, one `409` `version_conflict_engine_exception`. These two error types and statuses are not on the cited pages. |
| `bulk-resubmitted-response.json` | **Synthetic** | One successful `index` item for `a-2`. |
| `bulk-secret-reason-response.json` | **Synthetic** | A failed item whose `reason` quotes a synthetic secret (`SYNTHETIC-SECRET-4111`), for the redaction test. |
| `error-illegal-argument.json`, `error-security.json`, `error-index-not-found.json`, `error-rejected.json`, `error-unavailable.json` | **Synthetic** | The top-level error document (`{"error": {...}, "status": n}`) is not shown on the cited pages; its `error` object uses the bulk item `error` fields (`type`, `reason`, `root_cause`). Error types and reasons are synthetic; one reason quotes the synthetic secret. |
| `pit-open-response.json` | Pagination guide, "The API returns a PIT ID" | `id` verbatim; the guide elides `_shards` (`...`): synthetic counts. |
| `pit-search-request-next.json` | Pagination guide, the next-page `GET /_search` body | Verbatim (indentation of the first line normalised). Tests derive the first page by removing `search_after`, and set `pit.id` to the id the rule "use the latest PIT ID" designates. |
| `pit-close-request.json` | Pagination guide, `DELETE /_pit` body | Verbatim (indentation of the first line normalised). |
| `pit-search-page-1.json` | Pagination guide, PIT search response | `took`, `timed_out`, `max_score`, the layout and the last hit (`_index`, `_id` `FaslK3QBySSL_rrj9zM5`, `_score`, `sort` `["2021-05-20T05:30:04.832Z", 4294967298]`) verbatim. `pit_id` replaced by the synthetic `SYNTHETIC-PIT-ID-2` (the guide's answer repeats the request's id, which would not show that the id is replaced); a synthetic first hit added; elided `_shards` and `_source` filled synthetically; `hits.total` left out (`track_total_hits: false`). |
| `pit-search-page-2.json`, `pit-search-page-empty.json` | **Synthetic**, page-1's shape | Further pages: one hit, then none; each with a new synthetic `pit_id`. |
| `pit-search-shard-failure.json` | **Synthetic** | `_shards.failed: 1`. The meaning of shard failures is not stated on the cited pages (see below). |
| `pit-search-raw-sort.json` | **Synthetic** | Sort values `[18446744073709551615, 1.10, "b"]`, to prove they are echoed as received. |
| `pit-close-response.json` | **Synthetic** | `{"succeeded": true, "num_freed": 1}`: the close answer is not shown on the cited pages; the SDK reads both fields as optional. |

Tests also build replies directly in Rust (malformed, truncated, oversized, stalled, HTML,
unlabelled bodies; edited copies of the fixtures above, each edit named in the test).

## SDK rules that are not documented on the cited pages

These are this SDK's decisions, stated in the README; no cited page backs them:

- A non-2xx answer to a bulk request is a rejection of the whole request for 4xx and uncertain
  for 5xx (the pages document only the 2xx answer).
- Only `429` items are resubmitted by `resubmit_rejected`.
- A search page with `timed_out: true` or `_shards.failed > 0` stops the traversal
  (`PartialSearch`); the guide's examples show `timed_out` and `_shards` but not their meaning.
- The traversal ends at the first page without hits (the guide gives no end condition).
- Index-name restrictions (`_` prefix, `,`, `*`, `/`, `\`, whitespace, controls), keep-alive in
  whole minutes only, explicit `_id` on every bulk operation, the operation/byte/response caps.
- `track_total_hits: false` on every page (the guide recommends it to speed up pagination).
- `GET` for `_search` follows the guide's examples; whether a deployment's proxies keep a `GET`
  body was not checked.

## New dependencies

None: `concord`, `concord-auth`, `concord-pagination`, `serde`, `serde_json`, `tokio` and
`concord-example-testserver` are workspace crates or already in `Cargo.lock`. `serde_json`'s
`raw_value` feature (already enabled by other workspace members) is used for exact JSON text.
The lock gained only this package's entry.

Maintenance adds **no dependencies or features**. Workspace membership and the package's lock
entry were already present. The first build was `CARGO_BUILD_JOBS=4 cargo test --offline -p
concord-sdk-elasticsearch --no-run` without `--locked`; both root manifests and the lock stayed
unchanged. Subsequent cargo commands use `--locked --offline`, four jobs and the default
worktree `target/`. No lockfile package version changed.
