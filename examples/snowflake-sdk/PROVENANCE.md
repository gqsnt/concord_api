# Provenance and scope pin

Access date: **2026-11-06 (Europe/Paris)**. Protocol pin: **Snowflake SQL API v2**,
post-5.40 partition behavior. The official pages are rolling documentation, without
an immutable document revision or publication date shown. This is a date/version
pin, not a claim that the pages are immutable or that a live account was tested.

The three required pages were reopened during this author run. The reference and
authentication pages were also checked. Markdown-version URLs advertised by the
site returned tool access errors; the HTML documentation was successfully read.

| Source | Official page and sections used |
|---|---|
| S1 | [Submitting requests](https://docs.snowflake.com/en/developer-guide/sql-api/submitting-requests): Setting up the request; Example of a request. |
| S2 | [Handling responses](https://docs.snowflake.com/en/developer-guide/sql-api/handling-responses): Understanding the flow of execution; Checking status; Getting the results; Getting metadata about partitions; Retrieving additional partitions; Returning SQL NULL values. |
| S3 | [Canceling requests](https://docs.snowflake.com/en/developer-guide/sql-api/cancelling-requests): cancel POST endpoint and cancellation flow. |
| S4 | [SQL API reference](https://docs.snowflake.com/en/developer-guide/sql-api/reference): POST statements, GET statement handle, POST cancel, all-operation request headers, QueryStatus, QueryFailureStatus, CancelStatus, ResultSet and result metadata. |
| S5 | [Authentication](https://docs.snowflake.com/en/developer-guide/sql-api/authenticating): OAuth and token-type header values. |

All sources above use the same access date and v2 pin. S2 explicitly calls out the
partition behavior introduced in Snowflake 5.40. The cancellation status pair
`000604` / `57014` and cancel rejection `000709` / `02000` are from S4's examples;
S3 alone does not specify those JSON bodies.

## Fixture inventory

**No fixture is a captured vendor response or a verbatim complete vendor example.**
All JSON was hand-authored from the documented fields and then serialized as compact
UTF-8, without a trailing newline (`json.dumps(..., separators=(',', ':'),
ensure_ascii=False)`). Handles, request IDs, timestamps, SQL, data values, counts,
size descriptors and most message text are synthetic. All auth strings are plainly
synthetic. `.gitattributes` sets `fixtures/** -text` to preserve the committed bytes.

| Fixture | Source/section (v2; accessed 2026-11-06) | Transformation / synthetic case |
|---|---|---|
| `submit.json` | S1 request setup/example; S4 POST body | One artificial SELECT, escaped quotes and UTF-8 café; explicit timeout and context; no bindings or real table. Exact request-body assertion. |
| `pending-a.json` | S2 execution/status flow; S4 202 QueryStatus | Documented `333334`, synthetic handle A and message; advisory local status URL. |
| `pending-b.json` | Same as pending A | Separate artificial handle B for the long-running cancellation workflow. |
| `complete-1.json` | S2 result/partition metadata; S4 ResultSet | Artificial two-column/two-row first result with one descriptor, null cell, code `090001`. |
| `complete-2.json` | Same as complete-1 | Adds descriptor 1, changes total to three rows, keeps first rows unchanged. |
| `complete-3.json` | Same as complete-1 | Adds descriptor 2, five total rows; unequal partition row counts 2/1/2. |
| `complete-b.json` | Same as complete-1 | One-partition successful completion for handle B; used in synthetic cancel/completion race. |
| `complete-empty.json` | S2/S4 ResultSet fields | Synthetic zero-row result with a zero-row descriptor, preserving column metadata. |
| `partition-1.json` | S2 additional partitions and string/null data; S4 ResultSet.data | Synthetic metadata-free data envelope with one row and UTF-8 βeta. The page has no complete later-partition specimen. |
| `partition-2.json` | Same as partition-1 | Two synthetic rows, including literal string `null`. |
| `partition-bare.json` | Same data-array contract | Explicit compatibility assumption: bare row-array spelling, not a documented complete later-partition example. Tested separately. |
| `partition-1.json.gz` | S2 additional partitions (`Content-Encoding: gzip`) | Exact partition-1 bytes compressed using Python `gzip.compress(raw, mtime=0)`. |
| `partition-2.json.gz` | Same as gzip partition 1 | Exact partition-2 bytes compressed with the same deterministic procedure. |
| `partition-oversize.json` | S2 data shape; SDK local bound | Synthetic one-row envelope with a 2,000-character string. Adversarial body-size test, not a server example. |
| `partition-oversize.json.gz` | S2 gzip encoding; SDK local bound | Exact oversize bytes compressed with mtime=0; proves the local cap applies after inflation. |
| `cancelled-b.json` | S3 endpoint; S4 cancel 200 example | Documented code/SQLSTATE/message, synthetic handle/status URL. Reused as a synthetic subsequent 422 QueryFailureStatus observation. |
| `not-found-b.json` | S4 cancel 422 example | Documented code/SQLSTATE, synthetic handle and message. Completion after this rejection is a scripted race, not a claim that this is Snowflake's only race response. |
| `failed-a.json` | S4 POST/GET 422 QueryFailureStatus example | Documented `000904` / `42000`, artificial compilation-failure text and handle. |
| `timeout-a.json` | S4 POST 408 description and cancellation fields | Synthetic timeout with cancellation code/SQLSTATE. The exact combination/message is composed from documented shapes, not copied from a timeout specimen. |
| `rate-limit.json` | S4 error envelope; SDK transport policy | Synthetic rate-limit-shaped document, also used for transient 503. The string code `429` is not asserted to be a canonical Snowflake provider code. |
| `malformed.json` | SDK robustness, no vendor source | Deliberately invalid JSON `{broken`. |
| `proxy.html` | SDK robustness, no vendor source | Artificial non-JSON gateway error. |

Binary gzip fixtures are checked into source, not regenerated by tests. To reproduce
one: `gzip.compress(Path('partition-1.json').read_bytes(), mtime=0)` with Python's
standard gzip module. The JSON files supply the exact uncompressed inputs. Gzip
header implementation bytes can differ between Python/zlib releases; committed bytes
are authoritative for these tests.

Runtime fixture mutations in `tests/protocol.rs` deliberately corrupt handles, counts,
format, partition indexes, rows, status codes and JSON/media types. Other synthetic
cases are redirects, corrupt gzip bytes, truncated chunked transfer and oversized
error bodies. The decompression rework adds synthetic gzip checksum/trailer damage,
trailing junk, padded exact-limit JSON and multiple members generated by flate2 from
the existing partition fixture; it changes no existing fixture bytes.
`tests/workflows.rs` adds a fourth descriptor to prove a two-slot
concurrency window; it inserts pending statuses and scripts both cancellation and
completion winners. None is evidence of live provider timing or SQL semantics.

## Scope decisions and uncertainty

S2 says the first descriptor covers returned rows and later descriptors are retrieved
by their numeric partition index, with explicit examples for indexes 1 and 2. The
SDK verifies descriptor sum, format, first index and row dimensions before constructing
an independent plan. It preserves server byte sizes without treating them as local
limits. It uses no page-size arithmetic and never follows a returned URL.

S2 states that later partition responses are gzipped JSON without metadata; S4 shows
a generic ResultSet example for GET. Neither supplied a complete compressed later-part
specimen in this review. The SDK accepts both `{ "data": [...] }` and `[...]` under
that bounded assumption. Tests verify those synthetic formats, not their prevalence
on live accounts. An echoed handle is verified; absent identity is correlated through
the trusted originating request, never invented from partition body bytes.

The successful-first-response scope requires complete jsonv2 rows and partitionInfo.
Code 391908, missing descriptors and multi-statement responses are refused. No first
partition refetch or legacy fallback is implemented. The row strings are deliberately
not interpreted as Snowflake dates, numbers or other SQL types.

## Concord materials and dependencies

Read: root README; docs/CHEATSHEET.md, CUSTOM_FORMATS.md and relevant CALLER_GUIDE.md
sections; concord crate rustdoc (built with `cargo doc -p concord --no-deps --features
csv,jsonl` using the required build-cache prefix); concord-extras and
concord-pagination READMEs; public response/scheduler rustdoc and source; testserver
source. The existing **Salesforce SDK** supplied the `Prepared::receive()` → metadata
→ declared decode pattern (`src/transport.rs`) and a declaration example (`src/api.rs`).
It did not supply Snowflake workflow or protocol code. No other provider SDK supplied
the solution. Concord source was read, not edited; no private API was used.

Direct crate dependencies:

- `concord` by path: declaration, HTTP execution, operation identity, JSON codec and bounded reads.
- `concord-pagination` by path: independent scheduler, concurrency/budget/error policy and lossless reports.
- `flate2` 1.1.10, default features disabled, `rust_backend` enabled: mature gzip decoding local to partition bodies, including member/trailer/checksum validation. This avoids implementing the gzip protocol and avoids enabling reqwest features for other clients through Cargo feature unification. Reviewed the cached crate's README and `src/gz/bufread.rs` documentation for `MultiGzDecoder`. Concord's re-export supplies reqwest; there is no direct reqwest dependency.
- `serde` / `serde_json`: typed JSON serialization/deserialization and retained extension fields.
- `tokio`: finite deadlines, virtual clock and local-stop notification.
- Dev-only `concord-example-testserver` by path and Tokio `test-util`: owned loopback fixtures and paused-time tests.

No signing dependency: callers supply an already-issued bearer token. No vendor SDK
executes HTTP. No direct concord-core/extras dependency or private reach-through.

The initial author implementation enabled reqwest gzip and added nine lock entries: adler2, async-compression,
compression-codecs, compression-core, crc32fast, flate2, miniz_oxide, simd-adler32 and
zlib-rs. Initial resolution chose uncached releases and hit the read-only global
Cargo cache. Offline resolution selected cached async-compression 0.4.51,
compression-codecs 0.4.46 and crc32fast 1.5.2. Other package versions were preserved;
unrelated Windows dependency-edge changes from resolution were restored and the
locked build rechecked. This was an environment intervention, not a Concord change.

Coordinator follow-up found that enabling reqwest gzip in an SDK violates the
workspace's application-owned decompression rule (`scripts/verify.sh` and isolated
`checks/consumers/facade-gzip`) and breaks Concord's raw-encoding test when workspace
features unify. The rework removes that feature and the direct reqwest dependency.
Only flate2 and its Rust-backend dependencies remain for decoding; async-compression,
compression-codecs, compression-core and zlib-rs are removed from this SDK's lockfile
addition. Wire bytes and inflated bytes are bounded separately; original wire headers
remain observable. Invalid gzip and inflation overflow are typed SDK body errors,
not fabricated reqwest transport/Concord limit errors. The two existing assertions
were updated to check those exact causes and still verify the generated operation.
